//! Governance-side normalization, Completeness and local Gate preview.
//! This crate never executes a backend and has no publish/signing authority.

use erf_contracts::backend::v0 as pb;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const FINGERPRINT_VERSION: &str = "finding-v0";
pub const CANONICAL_SET_VERSION: &str = "canonical-finding-set-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateDecision { Pass, Fail, Inconclusive }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineState { New, Historical }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionDecision { NotApplicable, Exempted }

/// Local-only baseline input.  Its kind is explicit so debt, comparison and
/// quality baselines cannot be silently mixed by the preview engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    pub kind: BaselineKind,
    pub finding_fingerprints: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineKind { Debt, Comparison, QualityBenchmark }

pub const POLICY_SIGNING_ROLE: &str = "policy-signing";
pub const BASELINE_SIGNING_ROLE: &str = "baseline-signing";
pub const EXCEPTION_SIGNING_ROLE: &str = "exception-signing";

impl GatePolicy {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-gate-policy-v0\0".to_vec();
        let mut rules: Vec<&String> = self.blocking_rule_ids.iter().collect();
        rules.sort();
        payload.extend_from_slice(&(rules.len() as u64).to_be_bytes());
        for rule in rules { append_attestation_string(&mut payload, rule); }
        payload
    }

    pub fn sign_with(&self, key_id: impl Into<String>, signer: impl FnOnce(&[u8]) -> Vec<u8>) -> erf_trust::SignedAsset {
        sign_gate_asset(&self.canonical_payload(), key_id, signer)
    }

    pub fn verify_signed(&self, asset: &erf_trust::SignedAsset, store: &erf_trust::TrustStore, role: &str) -> Result<(), String> {
        verify_gate_asset(&self.canonical_payload(), asset, store, role, None, 0)
    }

    pub fn verify_signed_with_metadata(&self, asset: &erf_trust::SignedAsset, store: &erf_trust::TrustStore, role: &str, metadata: &erf_trust::TrustMetadata, now_epoch_secs: u64) -> Result<(), String> {
        verify_gate_asset(&self.canonical_payload(), asset, store, role, Some(metadata), now_epoch_secs)
    }
}

impl Baseline {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-gate-baseline-v0\0".to_vec();
        payload.push(match self.kind { BaselineKind::Debt => 0, BaselineKind::Comparison => 1, BaselineKind::QualityBenchmark => 2 });
        let fingerprints: Vec<&String> = self.finding_fingerprints.iter().collect();
        payload.extend_from_slice(&(fingerprints.len() as u64).to_be_bytes());
        for fingerprint in fingerprints { append_attestation_string(&mut payload, fingerprint); }
        payload
    }

    pub fn sign_with(&self, key_id: impl Into<String>, signer: impl FnOnce(&[u8]) -> Vec<u8>) -> erf_trust::SignedAsset {
        sign_gate_asset(&self.canonical_payload(), key_id, signer)
    }

    pub fn verify_signed(&self, asset: &erf_trust::SignedAsset, store: &erf_trust::TrustStore, role: &str) -> Result<(), String> {
        verify_gate_asset(&self.canonical_payload(), asset, store, role, None, 0)
    }

    pub fn verify_signed_with_metadata(&self, asset: &erf_trust::SignedAsset, store: &erf_trust::TrustStore, role: &str, metadata: &erf_trust::TrustMetadata, now_epoch_secs: u64) -> Result<(), String> {
        verify_gate_asset(&self.canonical_payload(), asset, store, role, Some(metadata), now_epoch_secs)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionGrant {
    pub grant_id: String,
    pub repository: String,
    pub rule_id: String,
    pub scenario: String,
    pub path_glob: String,
    pub symbol: Option<String>,
    pub fingerprint: Option<String>,
    pub owner: String,
    pub reason: String,
    pub ticket: String,
    pub approvals: Vec<String>,
    pub effective_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
}

impl ExceptionGrant {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-exception-grant-v0\0".to_vec();
        for value in [&self.grant_id, &self.repository, &self.rule_id, &self.scenario, &self.path_glob, &self.owner, &self.reason, &self.ticket] {
            append_attestation_string(&mut payload, value);
        }
        append_optional_string(&mut payload, self.symbol.as_deref());
        append_optional_string(&mut payload, self.fingerprint.as_deref());
        let mut approvals = self.approvals.clone();
        approvals.sort();
        payload.extend_from_slice(&(approvals.len() as u64).to_be_bytes());
        for approval in approvals { append_attestation_string(&mut payload, &approval); }
        payload.extend_from_slice(&self.effective_at_epoch_secs.to_be_bytes());
        payload.extend_from_slice(&self.expires_at_epoch_secs.to_be_bytes());
        payload
    }

    pub fn sign_with(&self, key_id: impl Into<String>, signer: impl FnOnce(&[u8]) -> Vec<u8>) -> erf_trust::SignedAsset {
        sign_gate_asset(&self.canonical_payload(), key_id, signer)
    }

    pub fn verify_signed(&self, asset: &erf_trust::SignedAsset, store: &erf_trust::TrustStore, role: &str, now_epoch_secs: u64) -> Result<(), String> {
        self.validate_at(now_epoch_secs)?;
        verify_gate_asset(&self.canonical_payload(), asset, store, role, None, 0)
    }

    pub fn verify_signed_with_metadata(&self, asset: &erf_trust::SignedAsset, store: &erf_trust::TrustStore, role: &str, metadata: &erf_trust::TrustMetadata, now_epoch_secs: u64) -> Result<(), String> {
        self.validate_at(now_epoch_secs)?;
        verify_gate_asset(&self.canonical_payload(), asset, store, role, Some(metadata), now_epoch_secs)
    }

    pub fn validate_at(&self, now_epoch_secs: u64) -> Result<(), String> {
        for (name, value) in [
            ("grant_id", &self.grant_id),
            ("repository", &self.repository),
            ("rule_id", &self.rule_id),
            ("scenario", &self.scenario),
            ("path_glob", &self.path_glob),
            ("owner", &self.owner),
            ("reason", &self.reason),
            ("ticket", &self.ticket),
        ] {
            if value.is_empty() || value.chars().any(char::is_control) { return Err(format!("exception grant has an invalid {name}")); }
        }
        validate_path_glob(&self.path_glob)?;
        if let Some(symbol) = &self.symbol { validate_symbol(symbol)?; }
        if let Some(fingerprint) = &self.fingerprint {
            if fingerprint.is_empty() || fingerprint.chars().any(char::is_whitespace) { return Err("exception grant has an invalid fingerprint".into()); }
        }
        if self.approvals.is_empty() || self.approvals.iter().any(|approval| approval.is_empty() || approval.chars().any(char::is_control)) {
            return Err("exception grant requires non-empty approvals".into());
        }
        if self.effective_at_epoch_secs >= self.expires_at_epoch_secs { return Err("exception grant validity interval is empty".into()); }
        if now_epoch_secs < self.effective_at_epoch_secs { return Err("exception grant is not yet effective".into()); }
        if now_epoch_secs >= self.expires_at_epoch_secs { return Err("exception grant is expired".into()); }
        Ok(())
    }
}

fn append_optional_string(payload: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => { payload.push(1); append_attestation_string(payload, value); }
        None => payload.push(0),
    }
}

fn validate_path_glob(path: &str) -> Result<(), String> {
    if path.starts_with('/') || path.contains('\\') || path.contains('[') || path.contains(']') || path.contains('{') || path.contains('}') || path.contains('(') || path.contains(')') {
        return Err("exception grant path_glob is not a closed repository-relative glob".into());
    }
    if path.split('/').all(|segment| segment == "*" || segment == "**") {
        return Err("exception grant path_glob cannot select the whole repository".into());
    }
    if path.split('/').any(|segment| segment.is_empty() || segment == "." || segment == "..") {
        return Err("exception grant path_glob contains an invalid path segment".into());
    }
    Ok(())
}

fn validate_symbol(symbol: &str) -> Result<(), String> {
    if symbol.is_empty() || symbol.chars().any(char::is_whitespace) || symbol.contains("..") {
        return Err("exception grant symbol is not a normalized identity".into());
    }
    Ok(())
}

fn sign_gate_asset(payload: &[u8], key_id: impl Into<String>, signer: impl FnOnce(&[u8]) -> Vec<u8>) -> erf_trust::SignedAsset {
    let payload = payload.to_vec();
    erf_trust::SignedAsset { payload_type: erf_trust::DSSE_PAYLOAD_TYPE.into(), signature: signer(&erf_trust::pae(erf_trust::DSSE_PAYLOAD_TYPE, &payload)), payload, key_id: key_id.into() }
}

fn verify_gate_asset(payload: &[u8], asset: &erf_trust::SignedAsset, store: &erf_trust::TrustStore, role: &str, metadata: Option<&erf_trust::TrustMetadata>, now_epoch_secs: u64) -> Result<(), String> {
    if asset.payload != payload { return Err("signed gate asset payload does not match canonical value".into()); }
    if let Some(metadata) = metadata {
        store.verify_with_metadata(asset, role, metadata, now_epoch_secs).map_err(|error| error.to_string())?;
    } else {
        store.verify(asset, role).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatePolicy {
    /// Only these rule identities can cause FAIL. An empty set means the
    /// compatibility/default local policy treats every rule as blocking.
    pub blocking_rule_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingLocationKind { SourceSpan, Symbol, BytecodeMember, File }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub fingerprint_version: &'static str,
    pub fingerprint: String,
    pub rule_id: String,
    pub location_kind: FindingLocationKind,
    pub path: String,
    /// Diagnostic coordinates are presentation facts only. They are not part
    /// of the semantic finding identity or SemanticResultDigest.
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub semantic_anchor: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completeness {
    pub planned: usize,
    pub accounted: usize,
    pub failed: usize,
    pub missing: usize,
    pub observation_errors: usize,
    pub complete: bool,
}

#[derive(Debug, Clone)]
pub struct GatePreview {
    pub decision: GateDecision,
    pub findings: Vec<Finding>,
    pub completeness: Completeness,
    pub canonical_set_version: &'static str,
    pub trust: &'static str,
    pub input_summary: String,
    pub assessments: Vec<FindingAssessment>,
    pub semantic_result_digest: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateAttestation {
    pub repository: String,
    pub commit: String,
    pub policy_digest: String,
    pub rule_bundle_digest: String,
    pub execution_lock_digest: String,
    pub runner_release_digest: String,
    pub workspace_descriptor_digest: String,
    pub baseline_digest: String,
    pub workspace_attestation_digest: String,
    pub execution_evidence_digest: String,
    pub sandbox_provider_id: String,
    pub sandbox_policy_digest: String,
    pub workload_issuer: String,
    pub workload_audience: String,
    pub workflow: String,
    pub run_identity: String,
    pub environment_identity: String,
    pub semantic_result_digest: [u8; 32],
    pub completeness: Completeness,
    pub decision: GateDecision,
    pub exception_grant_digests: Vec<String>,
    pub issued_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
}

impl GateAttestation {
    /// Digest of the complete normalized GateDecision/ExecutionEvidence
    /// identity. A semantic-result digest alone is insufficient because
    /// completeness, decision, exceptions and execution context can change
    /// while the finding set remains unchanged.
    pub fn subject_digest(&self) -> [u8; 32] {
        let mut payload = b"erf-gate-decision-v1\0".to_vec();
        for value in [
            &self.repository,
            &self.commit,
            &self.policy_digest,
            &self.rule_bundle_digest,
            &self.execution_lock_digest,
            &self.runner_release_digest,
            &self.workspace_descriptor_digest,
            &self.baseline_digest,
            &self.workspace_attestation_digest,
            &self.execution_evidence_digest,
            &self.sandbox_provider_id,
            &self.sandbox_policy_digest,
            &self.workload_issuer,
            &self.workload_audience,
            &self.workflow,
            &self.run_identity,
            &self.environment_identity,
        ] {
            append_attestation_string(&mut payload, value);
        }
        payload.extend_from_slice(&self.semantic_result_digest);
        payload.extend_from_slice(&(self.completeness.planned as u64).to_be_bytes());
        payload.extend_from_slice(&(self.completeness.accounted as u64).to_be_bytes());
        payload.extend_from_slice(&(self.completeness.failed as u64).to_be_bytes());
        payload.extend_from_slice(&(self.completeness.missing as u64).to_be_bytes());
        payload.extend_from_slice(&(self.completeness.observation_errors as u64).to_be_bytes());
        payload.push(self.completeness.complete as u8);
        payload.push(match self.decision {
            GateDecision::Pass => 0,
            GateDecision::Fail => 1,
            GateDecision::Inconclusive => 2,
        });
        let mut exceptions = self.exception_grant_digests.clone();
        exceptions.sort();
        payload.extend_from_slice(&(exceptions.len() as u64).to_be_bytes());
        for exception in exceptions {
            append_attestation_string(&mut payload, &exception);
        }
        payload.extend_from_slice(&self.issued_at_epoch_secs.to_be_bytes());
        payload.extend_from_slice(&self.expires_at_epoch_secs.to_be_bytes());
        Sha256::digest(payload).into()
    }

    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut exceptions = self.exception_grant_digests.clone();
        exceptions.sort();
        let materials = [
            ("erf://policy", &self.policy_digest),
            ("erf://rule-bundle", &self.rule_bundle_digest),
            ("erf://execution-lock", &self.execution_lock_digest),
            ("erf://runner-release", &self.runner_release_digest),
            ("erf://workspace-descriptor", &self.workspace_descriptor_digest),
            ("erf://baseline", &self.baseline_digest),
            ("erf://workspace-attestation", &self.workspace_attestation_digest),
            ("erf://execution-evidence", &self.execution_evidence_digest),
            ("erf://sandbox-policy", &self.sandbox_policy_digest),
        ]
        .into_iter()
        .map(|(name, digest)| json!({ "name": name, "digest": { "sha256": digest } }))
        .chain(exceptions.iter().map(|digest| {
            json!({ "name": "erf://exception-grant", "digest": { "sha256": digest } })
        }))
        .collect::<Vec<_>>();
        let decision = match self.decision {
            GateDecision::Pass => "PASS",
            GateDecision::Fail => "FAIL",
            GateDecision::Inconclusive => "INCONCLUSIVE",
        };
        let statement = json!({
            "_type": erf_trust::IN_TOTO_STATEMENT_TYPE,
            "subject": [{
                "name": "erf://gate-decision",
                "digest": { "sha256": hex_digest(&self.subject_digest()) },
            }],
            "predicateType": "https://rulefactory.io/attestations/gate/v0",
            "predicate": {
                "schemaVersion": "gate/v0",
                "repository": self.repository,
                "commit": self.commit,
                "baselineDigest": self.baseline_digest,
                "workspaceAttestationDigest": self.workspace_attestation_digest,
                "executionEvidenceDigest": self.execution_evidence_digest,
                "materials": materials,
                "runner": {
                    "releaseDigest": self.runner_release_digest,
                    "sandboxProviderId": self.sandbox_provider_id,
                    "sandboxPolicyDigest": self.sandbox_policy_digest,
                },
                "canonicalFindingSetVersion": CANONICAL_SET_VERSION,
                "semanticResultDigest": hex_digest(&self.semantic_result_digest),
                "completeness": {
                    "planned": self.completeness.planned,
                    "accounted": self.completeness.accounted,
                    "failed": self.completeness.failed,
                    "missing": self.completeness.missing,
                    "observationErrors": self.completeness.observation_errors,
                    "complete": self.completeness.complete,
                },
                "decision": decision,
                "exceptionGrantDigests": exceptions,
                "invocation": {
                    "workloadIssuer": self.workload_issuer,
                    "workloadAudience": self.workload_audience,
                    "workflow": self.workflow,
                    "runIdentity": self.run_identity,
                    "environment": self.environment_identity,
                },
                "issuedAtEpochSecs": self.issued_at_epoch_secs,
                "expiresAtEpochSecs": self.expires_at_epoch_secs,
            },
        });
        serde_json::to_vec(&statement).expect("GateAttestation is JSON-serializable")
    }

    pub fn sign_with(&self, key_id: impl Into<String>, signer: impl FnOnce(&[u8]) -> Vec<u8>) -> erf_trust::SignedAsset {
        let payload = self.canonical_payload();
        let signing_bytes = erf_trust::pae(erf_trust::IN_TOTO_STATEMENT_PAYLOAD_TYPE, &payload);
        erf_trust::SignedAsset { payload_type: erf_trust::IN_TOTO_STATEMENT_PAYLOAD_TYPE.into(), payload, key_id: key_id.into(), signature: signer(&signing_bytes) }
    }

    pub fn verify_signed(&self, asset: &erf_trust::SignedAsset, store: &erf_trust::TrustStore, role: &str, now_epoch_secs: u64) -> Result<[u8; 32], String> {
        self.validate()?;
        if self.expires_at_epoch_secs <= self.issued_at_epoch_secs || now_epoch_secs < self.issued_at_epoch_secs || now_epoch_secs >= self.expires_at_epoch_secs {
            return Err("gate attestation is outside its validity interval".into());
        }
        if asset.payload != self.canonical_payload() { return Err("gate attestation payload does not match statement".into()); }
        store.verify_with_payload_type(asset, role, erf_trust::IN_TOTO_STATEMENT_PAYLOAD_TYPE)
            .map_err(|error| error.to_string())?;
        Ok(self.subject_digest())
    }

    pub fn verify_signed_with_metadata(&self, asset: &erf_trust::SignedAsset, store: &erf_trust::TrustStore, role: &str, metadata: &erf_trust::TrustMetadata, now_epoch_secs: u64) -> Result<[u8; 32], String> {
        self.validate()?;
        if self.expires_at_epoch_secs <= self.issued_at_epoch_secs || now_epoch_secs < self.issued_at_epoch_secs || now_epoch_secs >= self.expires_at_epoch_secs {
            return Err("gate attestation is outside its validity interval".into());
        }
        if asset.payload != self.canonical_payload() { return Err("gate attestation payload does not match statement".into()); }
        store.verify_with_metadata_and_payload_type(asset, role, metadata, now_epoch_secs, erf_trust::IN_TOTO_STATEMENT_PAYLOAD_TYPE)
            .map_err(|error| error.to_string())?;
        Ok(self.subject_digest())
    }

    fn validate(&self) -> Result<(), String> {
        if self.repository.is_empty() || self.commit.is_empty() || self.sandbox_provider_id.is_empty() {
            return Err("gate attestation has an empty trusted-context identity".into());
        }
        for (name, value) in [
            ("policy_digest", &self.policy_digest),
            ("rule_bundle_digest", &self.rule_bundle_digest),
            ("execution_lock_digest", &self.execution_lock_digest),
            ("runner_release_digest", &self.runner_release_digest),
            ("workspace_descriptor_digest", &self.workspace_descriptor_digest),
            ("baseline_digest", &self.baseline_digest),
            ("workspace_attestation_digest", &self.workspace_attestation_digest),
            ("execution_evidence_digest", &self.execution_evidence_digest),
            ("sandbox_policy_digest", &self.sandbox_policy_digest),
        ] {
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(format!("gate attestation has invalid {name}"));
            }
        }
        for (name, value) in [
            ("repository", &self.repository),
            ("commit", &self.commit),
            ("sandbox_provider_id", &self.sandbox_provider_id),
            ("workload_issuer", &self.workload_issuer),
            ("workload_audience", &self.workload_audience),
            ("workflow", &self.workflow),
            ("run_identity", &self.run_identity),
            ("environment_identity", &self.environment_identity),
        ] {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(format!("gate attestation has empty or invalid {name}"));
            }
        }
        Ok(())
    }
}

fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn append_attestation_string(payload: &mut Vec<u8>, value: &str) {
    payload.extend_from_slice(&(value.len() as u64).to_be_bytes());
    payload.extend_from_slice(value.as_bytes());
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingAssessment {
    pub fingerprint: String,
    pub baseline: BaselineState,
    pub exception: ExceptionDecision,
}

pub fn preview(planned_ids: &[String], statuses: &[pb::UnitStatus], observations: &[pb::BackendObservation]) -> GatePreview {
    preview_with_policy_and_baseline_and_exceptions(planned_ids, statuses, observations, None, &BTreeSet::new(), None)
}

/// Computes a local preview with explicit baseline and exact-fingerprint
/// exceptions. Exceptions never alter findings or baseline state. Any
/// incomplete execution remains INCONCLUSIVE regardless of exemptions.
pub fn preview_with_baseline_and_exceptions(
    planned_ids: &[String],
    statuses: &[pb::UnitStatus],
    observations: &[pb::BackendObservation],
    baseline: Option<&Baseline>,
    exceptions: &BTreeSet<String>,
) -> GatePreview {
    preview_with_policy_and_baseline_and_exceptions(planned_ids, statuses, observations, baseline, exceptions, None)
}

/// Computes a preview with an explicit policy. A non-empty blocking rule set
/// makes advisory-only findings visible but non-blocking; baseline and exact
/// exceptions remain independent inputs.
pub fn preview_with_policy_and_baseline_and_exceptions(
    planned_ids: &[String],
    statuses: &[pb::UnitStatus],
    observations: &[pb::BackendObservation],
    baseline: Option<&Baseline>,
    exceptions: &BTreeSet<String>,
    policy: Option<&GatePolicy>,
) -> GatePreview {
    preview_with_policy_and_baseline_and_exceptions_and_delivery(
        planned_ids,
        statuses,
        observations,
        baseline,
        exceptions,
        policy,
        None,
    )
}

/// Computes a preview whose semantic identity is optionally bound to the
/// immutable delivery descriptor. The ordinary finding-set API remains
/// unchanged for local and legacy callers; the standard delivery path passes
/// the descriptor digest so Gate, unified-result and scan-output share one
/// identity.
pub fn preview_with_policy_and_baseline_and_exceptions_and_delivery(
    planned_ids: &[String],
    statuses: &[pb::UnitStatus],
    observations: &[pb::BackendObservation],
    baseline: Option<&Baseline>,
    exceptions: &BTreeSet<String>,
    policy: Option<&GatePolicy>,
    delivery_descriptor_digest: Option<&str>,
) -> GatePreview {
    preview_with_policy_and_baseline_and_exceptions_and_delivery_and_observation_errors(
        planned_ids,
        statuses,
        observations,
        baseline,
        exceptions,
        policy,
        delivery_descriptor_digest,
        0,
    )
}

/// Computes a preview while carrying structured observation errors discovered
/// before Backend execution (for example Resolver facts that are required by
/// the selected backend but unresolved in SAFE_STATIC mode). Those errors are
/// additive to malformed BackendObservation errors and always keep the result
/// INCONCLUSIVE.
#[allow(clippy::too_many_arguments)]
pub fn preview_with_policy_and_baseline_and_exceptions_and_delivery_and_observation_errors(
    planned_ids: &[String],
    statuses: &[pb::UnitStatus],
    observations: &[pb::BackendObservation],
    baseline: Option<&Baseline>,
    exceptions: &BTreeSet<String>,
    policy: Option<&GatePolicy>,
    delivery_descriptor_digest: Option<&str>,
    pre_execution_observation_errors: usize,
) -> GatePreview {
    let planned: BTreeSet<&str> = planned_ids.iter().map(String::as_str).collect();
    let mut accounted = BTreeSet::new();
    let mut failed = 0;
    for status in statuses {
        if planned.contains(status.unit_id.as_str()) {
            accounted.insert(status.unit_id.as_str());
            if status.outcome != pb::UnitOutcome::Executed as i32 { failed += 1; }
        }
    }
    let missing = planned.len().saturating_sub(accounted.len());
    let mut findings = Vec::new();
    let mut observation_errors = pre_execution_observation_errors;
    for observation in observations {
        match normalize(observation) { Ok(f) => findings.push(f), Err(_) => observation_errors += 1 }
    }
    findings.sort_by(|a, b| a.fingerprint.cmp(&b.fingerprint));
    findings.dedup_by(|a, b| a.fingerprint == b.fingerprint);
    let complete = missing == 0 && failed == 0 && observation_errors == 0 && accounted.len() == planned.len();
    let completeness = Completeness { planned: planned.len(), accounted: accounted.len(), failed, missing, observation_errors, complete };
    let assessments = findings.iter().map(|finding| FindingAssessment {
        fingerprint: finding.fingerprint.clone(),
        baseline: if baseline.map(|b| b.finding_fingerprints.contains(&finding.fingerprint)).unwrap_or(false) { BaselineState::Historical } else { BaselineState::New },
        exception: if exceptions.contains(&finding.fingerprint) { ExceptionDecision::Exempted } else { ExceptionDecision::NotApplicable },
    }).collect::<Vec<_>>();
    let semantic_result_digest = semantic_result_digest(&findings, delivery_descriptor_digest);
    let has_unexempted_new = assessments.iter().zip(findings.iter()).any(|(assessment, finding)| {
        assessment.baseline == BaselineState::New
            && assessment.exception == ExceptionDecision::NotApplicable
            && policy.map(|p| p.blocking_rule_ids.is_empty() || p.blocking_rule_ids.contains(&finding.rule_id)).unwrap_or(true)
    });
    let decision = if !complete { GateDecision::Inconclusive }
        else if has_unexempted_new { GateDecision::Fail }
        else { GateDecision::Pass };
    let input_summary = format!("findings={}, planned={}, accounted={}, failed={}, missing={}, observation_errors={}", findings.len(), completeness.planned, completeness.accounted, completeness.failed, completeness.missing, completeness.observation_errors);
    GatePreview { decision, findings, completeness, canonical_set_version: CANONICAL_SET_VERSION, trust: "LOCAL_UNATTESTED", input_summary, assessments, semantic_result_digest }
}

/// Digest of the canonical semantic finding set. Presentation fields such as
/// message are deliberately excluded, so renderer wording cannot change the
/// semantic identity of a scan result.
fn semantic_result_digest(findings: &[Finding], delivery_descriptor_digest: Option<&str>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(CANONICAL_SET_VERSION.as_bytes());
    hasher.update([0]);
    for finding in findings {
        hasher.update(finding.fingerprint.as_bytes());
        hasher.update([0]);
        hasher.update(finding.rule_id.as_bytes());
        hasher.update([0]);
        hasher.update(finding.path.as_bytes());
        hasher.update([0]);
        hasher.update(finding.semantic_anchor.as_bytes());
        hasher.update([0]);
    }
    if let Some(delivery_descriptor_digest) = delivery_descriptor_digest {
        hasher.update(b"delivery-descriptor");
        hasher.update([0]);
        hasher.update((delivery_descriptor_digest.len() as u64).to_be_bytes());
        hasher.update(delivery_descriptor_digest.as_bytes());
    }
    hasher.finalize().into()
}

fn normalize(observation: &pb::BackendObservation) -> Result<Finding, &'static str> {
    if observation.rule_id.is_empty() { return Err("missing rule identity"); }
    let location = observation.location.as_ref().ok_or("missing location")?;
    let (location_kind, path, line, column, anchor) = match location.kind.as_ref() {
        Some(pb::observation_location::Kind::SourceSpan(span)) => {
            validate_repo_path(&span.path)?;
            if span.blob_sha256.len() != 32 || span.end_byte < span.start_byte || span.line == 0 || span.column == 0 {
                return Err("invalid source span identity");
            }
            (FindingLocationKind::SourceSpan, span.path.clone(), Some(span.line), Some(span.column), format!("source-span|{}|{}|{}", span.path, span.start_byte, span.end_byte))
        }
        Some(pb::observation_location::Kind::Symbol(symbol)) => {
            validate_atom(&symbol.owner)?;
            validate_atom(&symbol.name)?;
            validate_atom(&symbol.signature)?;
            (FindingLocationKind::Symbol, symbol.owner.clone(), None, None, format!("symbol|{}|{}|{}", symbol.owner, symbol.name, symbol.signature))
        }
        Some(pb::observation_location::Kind::BytecodeMember(member)) => {
            validate_atom(&member.class_fqn)?;
            validate_atom(&member.member)?;
            validate_atom(&member.member_descriptor)?;
            (FindingLocationKind::BytecodeMember, member.class_fqn.clone(), None, None, format!("bytecode-member|{}|{}|{}", member.class_fqn, member.member, member.member_descriptor))
        }
        Some(pb::observation_location::Kind::File(file)) => {
            validate_repo_path(&file.path)?;
            if file.blob_sha256.len() != 32 { return Err("invalid file identity"); }
            (FindingLocationKind::File, file.path.clone(), None, None, format!("file|{}", file.path))
        }
        None => return Err("unsupported or missing location"),
    };
    let mut identity = Vec::new(); identity.extend_from_slice(FINGERPRINT_VERSION.as_bytes()); identity.push(0); identity.extend_from_slice(observation.rule_id.as_bytes()); identity.push(0); identity.extend_from_slice(anchor.as_bytes());
    let fingerprint = format!("{}:{:x}", FINGERPRINT_VERSION, Sha256::digest(identity));
    Ok(Finding { fingerprint_version: FINGERPRINT_VERSION, fingerprint, rule_id: observation.rule_id.clone(), location_kind, path, line, column, semantic_anchor: anchor, message: observation.message.clone() })
}

fn validate_repo_path(path: &str) -> Result<(), &'static str> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains('\0') {
        return Err("invalid repository-relative path");
    }
    if path.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
        return Err("invalid repository-relative path");
    }
    Ok(())
}

fn validate_atom(value: &str) -> Result<(), &'static str> {
    if value.is_empty() || value.bytes().any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace()) {
        return Err("invalid typed location identity");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn obs(path: &str) -> pb::BackendObservation { pb::BackendObservation { rule_id: "rule.a".into(), location: Some(pb::ObservationLocation { kind: Some(pb::observation_location::Kind::SourceSpan(pb::SourceSpan { path: path.into(), blob_sha256: vec![1; 32], start_byte: 2, end_byte: 4, line: 1, column: 1 })) }), message: "diagnostic".into(), engine_severity: "warning".into() } }
    fn status(id: &str, outcome: pb::UnitOutcome) -> pb::UnitStatus { pb::UnitStatus { unit_id: id.into(), outcome: outcome as i32, reason_code: 0, detail: String::new() } }
    #[test] fn stable_finding_and_complete_nonempty_is_fail() { let p = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[obs("src/A.java")]); assert_eq!(p.decision, GateDecision::Fail); assert!(p.completeness.complete); assert_eq!(p.findings[0].fingerprint_version, FINGERPRINT_VERSION); }
    #[test] fn zero_findings_is_not_pass_and_missing_is_inconclusive() { let p = preview(&["u1".into()], &[], &[]); assert_eq!(p.decision, GateDecision::Inconclusive); assert!(!p.completeness.complete); }
    #[test] fn complete_zero_findings_is_pass() {
        let p = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[]);
        assert_eq!(p.decision, GateDecision::Pass);
        assert!(p.completeness.complete);
        assert!(p.findings.is_empty());
    }
    #[test] fn invalid_observation_is_not_a_finding() { let p = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[obs("../A.java")]); assert!(p.findings.is_empty()); assert_eq!(p.completeness.observation_errors, 1); assert_eq!(p.decision, GateDecision::Inconclusive); }
    #[test] fn historical_finding_passes_only_when_exactly_baselined() {
        let initial = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[obs("src/A.java")]);
        let baseline = Baseline { kind: BaselineKind::Debt, finding_fingerprints: [initial.findings[0].fingerprint.clone()].into_iter().collect() };
        let p = preview_with_baseline_and_exceptions(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[obs("src/A.java")], Some(&baseline), &BTreeSet::new());
        assert_eq!(p.decision, GateDecision::Pass);
        assert_eq!(p.assessments[0].baseline, BaselineState::Historical);
    }
    #[test] fn exact_exception_does_not_hide_finding_or_cover_incomplete_scan() {
        let initial = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[obs("src/A.java")]);
        let exceptions = [initial.findings[0].fingerprint.clone()].into_iter().collect();
        let p = preview_with_baseline_and_exceptions(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[obs("src/A.java")], None, &exceptions);
        assert_eq!(p.decision, GateDecision::Pass);
        assert_eq!(p.findings.len(), 1);
        let incomplete = preview_with_baseline_and_exceptions(&["u1".into()], &[], &[obs("src/A.java")], None, &exceptions);
        assert_eq!(incomplete.decision, GateDecision::Inconclusive);
    }
    #[test] fn semantic_digest_is_repeatable_and_excludes_message() {
        let first = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[obs("src/A.java")]);
        let mut changed = obs("src/A.java");
        changed.message = "renderer wording changed".into();
        let second = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[changed]);
        assert_eq!(first.semantic_result_digest, second.semantic_result_digest);
        let repeated = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[obs("src/A.java")]);
        assert_eq!(first.semantic_result_digest, repeated.semantic_result_digest);
    }
    #[test] fn delivery_descriptor_identity_changes_semantic_digest() {
        let planned = ["u1".into()];
        let statuses = [status("u1", pb::UnitOutcome::Executed)];
        let observations = [obs("src/A.java")];
        let plain = preview(&planned, &statuses, &observations);
        let delivery = preview_with_policy_and_baseline_and_exceptions_and_delivery(
            &planned,
            &statuses,
            &observations,
            None,
            &BTreeSet::new(),
            None,
            Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        );
        assert_ne!(plain.semantic_result_digest, delivery.semantic_result_digest);
    }
    #[test] fn all_typed_locations_have_deterministic_fingerprints() {
        let observations = vec![
            obs("src/A.java"),
            pb::BackendObservation { rule_id: "rule.a".into(), location: Some(pb::ObservationLocation { kind: Some(pb::observation_location::Kind::Symbol(pb::SymbolRef { owner: "pkg.A".into(), name: "run".into(), signature: "()V".into() })) }), message: "symbol".into(), engine_severity: "warning".into() },
            pb::BackendObservation { rule_id: "rule.a".into(), location: Some(pb::ObservationLocation { kind: Some(pb::observation_location::Kind::BytecodeMember(pb::BytecodeMember { class_fqn: "pkg.A".into(), member: "run".into(), member_descriptor: "()V".into() })) }), message: "bytecode".into(), engine_severity: "warning".into() },
            pb::BackendObservation { rule_id: "rule.a".into(), location: Some(pb::ObservationLocation { kind: Some(pb::observation_location::Kind::File(pb::FileRef { path: "target/classes/A.class".into(), blob_sha256: vec![2; 32] })) }), message: "file".into(), engine_severity: "warning".into() },
        ];
        let first = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &observations);
        let second = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &observations);
        assert_eq!(first.findings.len(), 4);
        assert_eq!(first.findings, second.findings);
        assert!(first.findings.iter().any(|finding| finding.location_kind == FindingLocationKind::BytecodeMember));
    }
    #[test] fn malformed_typed_locations_are_inconclusive() {
        let malformed = [
            pb::BackendObservation { rule_id: "rule.a".into(), location: Some(pb::ObservationLocation { kind: Some(pb::observation_location::Kind::BytecodeMember(pb::BytecodeMember { class_fqn: "pkg.A".into(), member: "run".into(), member_descriptor: "() V".into() })) }), message: String::new(), engine_severity: String::new() },
            pb::BackendObservation { rule_id: "rule.a".into(), location: Some(pb::ObservationLocation { kind: Some(pb::observation_location::Kind::File(pb::FileRef { path: "../A.class".into(), blob_sha256: vec![2; 32] })) }), message: String::new(), engine_severity: String::new() },
        ];
        let p = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &malformed);
        assert_eq!(p.findings.len(), 0);
        assert_eq!(p.completeness.observation_errors, 2);
        assert_eq!(p.decision, GateDecision::Inconclusive);
    }
    #[test] fn advisory_only_policy_does_not_fail_for_nonblocking_rule() {
        let policy = GatePolicy { blocking_rule_ids: ["other.rule".into()].into_iter().collect() };
        let p = preview_with_policy_and_baseline_and_exceptions(
            &["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[obs("src/A.java")],
            None, &BTreeSet::new(), Some(&policy));
        assert_eq!(p.decision, GateDecision::Pass);
        assert_eq!(p.findings.len(), 1);
    }
    #[test] fn signed_policy_baseline_and_exception_grants_bind_roles_and_expiry() {
        use ed25519_dalek::{Signer, SigningKey};

        let policy_key = SigningKey::from_bytes(&[42; 32]);
        let baseline_key = SigningKey::from_bytes(&[43; 32]);
        let exception_key = SigningKey::from_bytes(&[44; 32]);
        let mut store = erf_trust::TrustStore::default();
        store.insert_key("policy-key", policy_key.verifying_key()).unwrap();
        store.insert_key("baseline-key", baseline_key.verifying_key()).unwrap();
        store.insert_key("exception-key", exception_key.verifying_key()).unwrap();
        store.authorize_role(POLICY_SIGNING_ROLE, "policy-key").unwrap();
        store.authorize_role(BASELINE_SIGNING_ROLE, "baseline-key").unwrap();
        store.authorize_role(EXCEPTION_SIGNING_ROLE, "exception-key").unwrap();

        let policy = GatePolicy { blocking_rule_ids: ["corp.rule".into()].into_iter().collect() };
        let policy_asset = policy.sign_with("policy-key", |bytes| policy_key.sign(bytes).to_bytes().to_vec());
        assert!(policy.verify_signed(&policy_asset, &store, POLICY_SIGNING_ROLE).is_ok());
        let mut changed_policy = policy.clone();
        changed_policy.blocking_rule_ids.insert("other.rule".into());
        assert!(changed_policy.verify_signed(&policy_asset, &store, POLICY_SIGNING_ROLE).is_err());
        assert!(policy.verify_signed(&policy_asset, &store, BASELINE_SIGNING_ROLE).is_err());

        let baseline = Baseline {
            kind: BaselineKind::Debt,
            finding_fingerprints: ["finding-v0:abc".into()].into_iter().collect(),
        };
        let baseline_asset = baseline.sign_with("baseline-key", |bytes| baseline_key.sign(bytes).to_bytes().to_vec());
        let metadata = erf_trust::TrustMetadata {
            version: 1,
            expires_at_epoch_secs: 20,
            revoked_key_ids: BTreeSet::new(),
        };
        assert!(baseline.verify_signed_with_metadata(&baseline_asset, &store, BASELINE_SIGNING_ROLE, &metadata, 19).is_ok());
        assert!(baseline.verify_signed_with_metadata(&baseline_asset, &store, BASELINE_SIGNING_ROLE, &metadata, 20).is_err());

        let grant = ExceptionGrant {
            grant_id: "exception/org-repo/corp-rule/2026-09".into(),
            repository: "org/repo".into(),
            rule_id: "corp.rule".into(),
            scenario: "java-source".into(),
            path_glob: "src/**/*.java".into(),
            symbol: Some("org.repo.Bootstrap#shutdown()".into()),
            fingerprint: Some("finding-v0:abc".into()),
            owner: "platform-team".into(),
            reason: "approved migration exception".into(),
            ticket: "RISK-1234".into(),
            approvals: vec!["platform-owner".into()],
            effective_at_epoch_secs: 10,
            expires_at_epoch_secs: 20,
        };
        let grant_asset = grant.sign_with("exception-key", |bytes| exception_key.sign(bytes).to_bytes().to_vec());
        assert!(grant.verify_signed(&grant_asset, &store, EXCEPTION_SIGNING_ROLE, 19).is_ok());
        assert!(grant.verify_signed(&grant_asset, &store, EXCEPTION_SIGNING_ROLE, 20).is_err());
        assert!(grant.verify_signed(&grant_asset, &store, POLICY_SIGNING_ROLE, 19).is_err());
    }
    #[test] fn gate_attestation_signs_aggregate_without_finding_content() {
        use ed25519_dalek::{Signer, SigningKey};
        let attestation = GateAttestation {
            repository: "org/repo".into(), commit: "abc123".into(), policy_digest: "f".repeat(64), rule_bundle_digest: "a".repeat(64), execution_lock_digest: "b".repeat(64),
            runner_release_digest: "c".repeat(64), workspace_descriptor_digest: "d".repeat(64), baseline_digest: "0".repeat(64), workspace_attestation_digest: "1".repeat(64), execution_evidence_digest: "2".repeat(64), sandbox_provider_id: "rootless-oci-docker-v0".into(), sandbox_policy_digest: "e".repeat(64), workload_issuer: "https://issuer.example".into(), workload_audience: "erf-gate".into(), workflow: "ci".into(), run_identity: "run-1".into(), environment_identity: "linux/amd64".into(),
            semantic_result_digest: [3; 32], completeness: Completeness { planned: 2, accounted: 2, failed: 0, missing: 0, observation_errors: 0, complete: true },
            decision: GateDecision::Pass, exception_grant_digests: vec!["g".repeat(64), "f".repeat(64)], issued_at_epoch_secs: 10, expires_at_epoch_secs: 20,
        };
        let key = SigningKey::from_bytes(&[21; 32]);
        let asset = attestation.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let mut store = erf_trust::TrustStore::default();
        store.insert_key("gate-key", key.verifying_key()).unwrap();
        store.authorize_role("gate-signing", "gate-key").unwrap();
        let statement: serde_json::Value = serde_json::from_slice(&asset.payload).unwrap();
        assert_eq!(statement["_type"], erf_trust::IN_TOTO_STATEMENT_TYPE);
        assert_eq!(statement["predicateType"], "https://rulefactory.io/attestations/gate/v0");
        assert_eq!(statement["predicate"]["decision"], "PASS");
        assert!(attestation.verify_signed(&asset, &store, "gate-signing", 19).is_ok());
        assert!(!String::from_utf8_lossy(&asset.payload).contains("source/Secret.java"));
        assert!(attestation.verify_signed(&asset, &store, "gate-signing", 20).is_err());
        let mut changed = attestation.clone();
        changed.workspace_descriptor_digest = "9".repeat(64);
        assert!(changed.verify_signed(&asset, &store, "gate-signing", 19).is_err());
    }
}
