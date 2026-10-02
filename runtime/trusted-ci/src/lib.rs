//! Trusted CI handoff from the real Runner scan to independent verification.
//!
//! The runner remains responsible for resolution, execution, atomic attempt
//! commit and the shared Gate preview inputs. This crate is the narrow seam
//! after that work: it turns the actual `ScanResult` into a signed
//! GateAttestation, verifies the full cross-asset TrustedContext, and hands
//! the unchanged decision to the credential-isolated Status Publisher.

use erf_bootstrap::RunnerMeasurement;
use erf_incremental_baseline::{digest_summary, Stage, StageTimer};
use erf_conformance::ConformanceEvidence;
use erf_packs::PackMeasurement;
use erf_status::{PublishOutcome, StatusPublisher, StatusPublishError};
use erf_trust::{SignedAsset, TrustCheckpoint, TrustMetadata, TrustStore};
use erf_trust_state::PersistedTrustState;
use erf_trusted_context::{GateInputIdentity, TrustedCheckpointInput, TrustedContext, TrustedContextError, TrustedContextInput, TrustedContextVerifier};
use erf_unified_result::{AuditEventKind, AuditTrail, ResultInputs, UnifiedResult};
use rf_gate::{Baseline, GateAttestation, GatePolicy, GatePreview};
use rf_runner::{ScanConfig, ScanError, ScanResult, SignedExecutionLock};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TrustedCiError {
    #[error("trusted CI requires a signed ExecutionLock in the scan config")]
    MissingSignedExecutionLock,
    #[error("trusted CI requires independent Backend Host and Workspace Resolver sandbox policies; an unsandboxed run is INCONCLUSIVE")]
    MissingSandboxPolicy,
    #[error("trusted CI requires fresh Conformance and Gate TrustMetadata; missing metadata is INCONCLUSIVE")]
    MissingTrustMetadata,
    #[error("trusted CI could not read the verification clock: {0}")]
    TrustedClock(String),
    #[error("trusted CI request validity window is not active")]
    InvalidValidityWindow,
    #[error("scan lock and trusted-context lock are different")]
    LockMismatch,
    #[error("runner scan did not return a 32-byte WorkspaceDescriptor digest")]
    InvalidWorkspaceDescriptorDigest,
    #[error("runner scan failed: {0}")]
    Scan(#[from] ScanError),
    #[error("trusted context rejected the scan: {0}")]
    TrustedContext(#[from] TrustedContextError),
    #[error("status publication rejected the scan: {0}")]
    Status(#[from] StatusPublishError),
    #[error("unified result rejected the trusted handoff: {0}")]
    UnifiedResult(#[from] erf_unified_result::ResultError),
    #[error("the frozen ScanPlan is not complete and Gate-ineligible")]
    ScanPlanNotAuthoritative,
    #[error("runner returned a non-32-byte digest for the audit event: {0}")]
    InvalidAuditDigest(&'static str),
    #[error("runner stage telemetry rejected the Gate measurement: {0}")]
    StageTelemetry(String),
}

/// Immutable material that must describe the same execution as the runner.
pub struct TrustedCiInputs<'a> {
    pub signed_execution_lock: &'a SignedExecutionLock,
    pub runner_measurement: &'a RunnerMeasurement,
    pub backend_pack_measurement: &'a PackMeasurement,
    pub resolver_pack_measurement: &'a PackMeasurement,
    pub conformance_evidence: &'a ConformanceEvidence,
    pub signed_conformance_evidence: &'a SignedAsset,
    pub conformance_metadata: Option<&'a TrustMetadata>,
    pub gate_metadata: Option<&'a TrustMetadata>,
    pub trust_checkpoint: &'a TrustCheckpoint,
    pub signed_trust_checkpoint: &'a SignedAsset,
    pub persisted_trust_state: &'a PersistedTrustState,
    pub trust_checkpoint_role: &'a str,
    pub trust_store: &'a TrustStore,
    pub verifier: &'a TrustedContextVerifier,
}

/// Inputs that are fixed before a trusted CI run starts.
pub struct TrustedCiRequest<'a> {
    pub scan: &'a ScanConfig,
    pub context: TrustedCiInputs<'a>,
    pub repository: String,
    pub commit: String,
    pub policy_digest: String,
    pub baseline: Option<&'a Baseline>,
    pub exception_fingerprints: &'a BTreeSet<String>,
    pub gate_policy: Option<&'a GatePolicy>,
    pub exception_grant_digests: Vec<String>,
    pub gate_key_id: String,
    pub issued_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
    pub run_sequence: u64,
    pub workload_issuer: String,
    pub workload_audience: String,
    pub workflow: String,
    pub run_identity: String,
    pub environment_identity: String,
}

#[derive(Debug)]
pub struct TrustedCiOutcome {
    pub scan: ScanResult,
    pub preview: GatePreview,
    pub gate_attestation: GateAttestation,
    pub signed_gate_attestation: SignedAsset,
    pub trusted_context: TrustedContext,
    pub published: PublishOutcome,
    pub unified_result: UnifiedResult,
}

/// Runs the real scan and crosses the trusted CI boundary exactly once.
///
/// The signer is supplied by the caller (for example a short-lived workload
/// identity or restricted signer socket); this crate never stores a key or
/// talks to SCM. The Status Publisher receives the same GateAttestation that
/// TrustedContext verified and cannot alter its decision.
pub fn run(
    request: TrustedCiRequest<'_>,
    publisher: &mut StatusPublisher,
    signer: impl FnOnce(&[u8]) -> Vec<u8>,
) -> Result<TrustedCiOutcome, TrustedCiError> {
    validate_request_clock(&request)?;
    if request.scan.sandbox_policy.is_none() || request.scan.resolver_sandbox_policy.is_none() {
        return Err(TrustedCiError::MissingSandboxPolicy);
    }
    if request.context.conformance_metadata.is_none() || request.context.gate_metadata.is_none() {
        return Err(TrustedCiError::MissingTrustMetadata);
    }
    let configured_lock = request
        .scan
        .signed_execution_lock
        .as_ref()
        .ok_or(TrustedCiError::MissingSignedExecutionLock)?;
    if configured_lock.lock != request.context.signed_execution_lock.lock
        || configured_lock.asset != request.context.signed_execution_lock.asset
        || configured_lock.role != request.context.signed_execution_lock.role
    {
        return Err(TrustedCiError::LockMismatch);
    }

    let scan = rf_runner::scan(request.scan)?;
    handoff(scan, request, publisher, signer)
}

/// Converts an already completed `ScanResult` into a signed GateAttestation,
/// verifies it with the supplied cross-asset context, and publishes it. This
/// split keeps the post-run handoff independently testable while `run` above
/// remains the production path that invokes the actual Runner scan.
pub fn handoff(
    mut scan: ScanResult,
    request: TrustedCiRequest<'_>,
    publisher: &mut StatusPublisher,
    signer: impl FnOnce(&[u8]) -> Vec<u8>,
) -> Result<TrustedCiOutcome, TrustedCiError> {
    let trusted_now = validate_request_clock(&request)?;
    if request.scan.sandbox_policy.is_none() || request.scan.resolver_sandbox_policy.is_none() {
        return Err(TrustedCiError::MissingSandboxPolicy);
    }
    if request.context.conformance_metadata.is_none() || request.context.gate_metadata.is_none() {
        return Err(TrustedCiError::MissingTrustMetadata);
    }
    let gate_timer = StageTimer::start(Stage::Gate, digest_summary(&format!("trusted-ci-gate:{:?}", scan.plan_digest)))
        .map_err(|error| TrustedCiError::StageTelemetry(error.to_string()))?;
    let delivery_descriptor_digest = scan
        .bundle_delivery_identity
        .as_ref()
        .map(|identity| identity.descriptor_digest.as_str());
    let preview = rf_gate::preview_with_policy_and_baseline_and_exceptions_and_delivery_and_observation_errors(
        &scan.planned_unit_ids,
        &scan.unit_statuses,
        &scan.committed_findings,
        request.baseline,
        request.exception_fingerprints,
        request.gate_policy,
        delivery_descriptor_digest,
        scan.resolver_required_unresolved_facts.len(),
    );
    if let Some(mut telemetry) = scan.stage_telemetry.take() {
        telemetry
            .replace_measurement(gate_timer.finish())
            .map_err(|error| TrustedCiError::StageTelemetry(error.to_string()))?;
        if !telemetry.has_all_stage_summaries() {
            return Err(TrustedCiError::StageTelemetry("missing stage summary before Gate handoff".into()));
        }
        scan.stage_telemetry = Some(telemetry);
    } else {
        return Err(TrustedCiError::StageTelemetry("missing stage telemetry before Gate handoff".into()));
    }
    if scan.plan_authority.is_some() && scan.plan_completeness.as_ref().is_none_or(|report| !report.allows_authoritative_gate()) {
        return Err(TrustedCiError::ScanPlanNotAuthoritative);
    }
    if scan.fallback_decision.as_ref().is_some_and(|decision| !decision.allows_authoritative_gate()) {
        return Err(TrustedCiError::ScanPlanNotAuthoritative);
    }
    let lock = &request.context.signed_execution_lock.lock;
    if scan.workspace_descriptor_digest.len() != 32 {
        return Err(TrustedCiError::InvalidWorkspaceDescriptorDigest);
    }
    let workspace_descriptor_digest = hex_digest(&scan.workspace_descriptor_digest);
    let baseline_digest = request
        .baseline
        .map(|baseline| hex_digest(&Sha256::digest(baseline.canonical_payload())))
        .unwrap_or_else(|| hex_digest(&Sha256::digest(b"erf-baseline:none")));
    let workspace_attestation_digest = request
        .scan
        .workspace_attestation
        .as_ref()
        .map(|attestation| hex_digest(&attestation.signed_asset.subject_digest()))
        .unwrap_or_else(|| hex_digest(&Sha256::digest(b"erf-workspace-attestation:none")));
    let execution_evidence_digest = execution_evidence_digest(&scan, &preview);
    let gate_attestation = GateAttestation {
        repository: request.repository,
        commit: request.commit,
        policy_digest: request.policy_digest,
        rule_bundle_digest: lock.rule_bundle_digest.clone(),
        execution_lock_digest: lock.lock_digest.clone(),
        runner_release_digest: lock.runner_release_digest.clone(),
        workspace_descriptor_digest,
        baseline_digest: baseline_digest.clone(),
        workspace_attestation_digest: workspace_attestation_digest.clone(),
        execution_evidence_digest: execution_evidence_digest.clone(),
        sandbox_provider_id: lock.sandbox_provider_id.clone(),
        sandbox_policy_digest: lock.sandbox_policy_digest.clone(),
        workload_issuer: request.workload_issuer,
        workload_audience: request.workload_audience,
        workflow: request.workflow,
        run_identity: request.run_identity,
        environment_identity: request.environment_identity,
        semantic_result_digest: preview.semantic_result_digest,
        completeness: preview.completeness.clone(),
        decision: preview.decision,
        exception_grant_digests: request.exception_grant_digests,
        issued_at_epoch_secs: request.issued_at_epoch_secs,
        expires_at_epoch_secs: request.expires_at_epoch_secs,
    };
    let signed_gate_attestation = gate_attestation.sign_with(request.gate_key_id, signer);
    let trusted_context = request.context.verifier.verify_with_checkpoint(TrustedContextInput {
        signed_execution_lock: request.context.signed_execution_lock,
        runner_measurement: request.context.runner_measurement,
        backend_pack_measurement: request.context.backend_pack_measurement,
        resolver_pack_measurement: request.context.resolver_pack_measurement,
        conformance_evidence: request.context.conformance_evidence,
        signed_conformance_evidence: request.context.signed_conformance_evidence,
        conformance_metadata: request.context.conformance_metadata,
        gate_attestation: &gate_attestation,
        signed_gate_attestation: &signed_gate_attestation,
        gate_input: GateInputIdentity {
            baseline_digest: &baseline_digest,
            workspace_attestation_digest: &workspace_attestation_digest,
            execution_evidence_digest: &execution_evidence_digest,
            workload_issuer: &gate_attestation.workload_issuer,
            workload_audience: &gate_attestation.workload_audience,
            workflow: &gate_attestation.workflow,
            run_identity: &gate_attestation.run_identity,
            environment_identity: &gate_attestation.environment_identity,
        },
        gate_metadata: request.context.gate_metadata,
        trust_store: request.context.trust_store,
        now_epoch_secs: trusted_now,
    }, TrustedCheckpointInput {
        checkpoint: request.context.trust_checkpoint,
        signed_checkpoint: request.context.signed_trust_checkpoint,
        persisted_trust_state: request.context.persisted_trust_state,
        signing_role: request.context.trust_checkpoint_role,
    })?;
    let published = publisher.publish(
        &gate_attestation,
        &signed_gate_attestation,
        request.context.trust_store,
        trusted_now,
        request.run_sequence,
    )?;
    let mut audit = AuditTrail::new();
    audit.append(AuditEventKind::ScanPlanned, fixed_digest("plan", &scan.plan_digest)?);
    audit.append(AuditEventKind::ExecutionCompleted, fixed_digest("attempt transcript", &scan.attempt_transcript_digest)?);
    audit.append(AuditEventKind::FindingSetComputed, preview.semantic_result_digest);
    audit.append(AuditEventKind::GateEvaluated, preview.semantic_result_digest);
    audit.append(AuditEventKind::Attested, trusted_context.gate_attestation_subject);
    audit.append(AuditEventKind::Published, trusted_context.gate_attestation_subject);
    let unified_result = UnifiedResult::from_trusted_preview(
        ResultInputs {
            repository: gate_attestation.repository.clone(),
            commit: gate_attestation.commit.clone(),
            policy_digest: gate_attestation.policy_digest.clone(),
            rule_bundle_digest: gate_attestation.rule_bundle_digest.clone(),
            execution_lock_digest: gate_attestation.execution_lock_digest.clone(),
            runner_release_digest: gate_attestation.runner_release_digest.clone(),
            workspace_descriptor_digest: gate_attestation.workspace_descriptor_digest.clone(),
        },
        &preview,
        &gate_attestation,
        trusted_context.gate_attestation_subject,
        audit,
    )?;
    Ok(TrustedCiOutcome { scan, preview, gate_attestation, signed_gate_attestation, trusted_context, published, unified_result })
}

fn validate_request_clock(request: &TrustedCiRequest<'_>) -> Result<u64, TrustedCiError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| TrustedCiError::TrustedClock(error.to_string()))?
        .as_secs();
    if request.issued_at_epoch_secs > now || request.expires_at_epoch_secs <= now {
        return Err(TrustedCiError::InvalidValidityWindow);
    }
    Ok(now)
}

fn fixed_digest(name: &'static str, bytes: &[u8]) -> Result<[u8; 32], TrustedCiError> {
    bytes.try_into().map_err(|_| TrustedCiError::InvalidAuditDigest(name))
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn execution_evidence_digest(scan: &ScanResult, preview: &GatePreview) -> String {
    let mut digest = Sha256::new();
    digest.update(b"erf-execution-evidence-v0\0");
    for value in [
        scan.plan_digest.as_slice(),
        scan.workspace_descriptor_digest.as_slice(),
        scan.attempt_transcript_digest.as_slice(),
        &preview.semantic_result_digest[..],
    ] {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value);
    }
    for value in [
        preview.completeness.planned as u64,
        preview.completeness.accounted as u64,
        preview.completeness.failed as u64,
        preview.completeness.missing as u64,
        preview.completeness.observation_errors as u64,
    ] {
        digest.update(value.to_be_bytes());
    }
    digest.update([preview.completeness.complete as u8]);
    hex_digest(&digest.finalize())
}
