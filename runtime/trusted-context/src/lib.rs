//! Cross-asset trusted execution closure verification.
//!
//! Individual verifiers establish that an asset is authentic. This crate
//! establishes the stronger fact that the accepted lock, measured executable
//! assets, conformance evidence and GateAttestation all describe one exact
//! execution context.

use erf_bootstrap::RunnerMeasurement;
use erf_conformance::{ConformanceEvidence, VERIFICATION_SIGNING_ROLE, TRUSTED_PLATFORM};
use erf_packs::{PackMeasurement, BACKEND_RUNTIME_PACK_ASSET_TYPE, WORKSPACE_RESOLVER_PACK_ASSET_TYPE};
use erf_trust::{SignedAsset, TrustCheckpoint, TrustMetadata, TrustStore};
use erf_trust_state::PersistedTrustState;
use rf_gate::{GateAttestation, GateDecision};
use rf_runner::SignedExecutionLock;
use thiserror::Error;

pub const EXECUTION_LOCK_SIGNING_ROLE: &str = "execution-lock";
pub const GATE_SIGNING_ROLE: &str = "gate-signing";
pub const TRUST_CHECKPOINT_SIGNING_ROLE: &str = "trust-root";

#[derive(Debug, Clone)]
pub struct GateInputIdentity<'a> {
    pub baseline_digest: &'a str,
    pub workspace_attestation_digest: &'a str,
    pub execution_evidence_digest: &'a str,
    pub workload_issuer: &'a str,
    pub workload_audience: &'a str,
    pub workflow: &'a str,
    pub run_identity: &'a str,
    pub environment_identity: &'a str,
}

#[derive(Debug, Clone)]
pub struct TrustedContextInput<'a> {
    pub signed_execution_lock: &'a SignedExecutionLock,
    pub runner_measurement: &'a RunnerMeasurement,
    pub backend_pack_measurement: &'a PackMeasurement,
    pub resolver_pack_measurement: &'a PackMeasurement,
    pub conformance_evidence: &'a ConformanceEvidence,
    pub signed_conformance_evidence: &'a SignedAsset,
    pub conformance_metadata: Option<&'a TrustMetadata>,
    pub gate_attestation: &'a GateAttestation,
    pub signed_gate_attestation: &'a SignedAsset,
    pub gate_input: GateInputIdentity<'a>,
    pub gate_metadata: Option<&'a TrustMetadata>,
    pub trust_store: &'a TrustStore,
    pub now_epoch_secs: u64,
}

/// Evidence that the trust floor was already accepted and persisted before a
/// trusted handoff. The verifier deliberately receives the loaded state
/// rather than a mutable store so scanning cannot advance trust state as a
/// side effect of evaluating a Gate.
#[derive(Debug, Clone, Copy)]
pub struct TrustedCheckpointInput<'a> {
    pub checkpoint: &'a TrustCheckpoint,
    pub signed_checkpoint: &'a SignedAsset,
    pub persisted_trust_state: &'a PersistedTrustState,
    pub signing_role: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedContext {
    pub repository: String,
    pub commit: String,
    pub execution_lock_digest: String,
    pub runner_release_digest: String,
    pub backend_pack_digest: String,
    pub resolver_pack_digest: String,
    pub conformance_evidence_subject: [u8; 32],
    pub gate_attestation_subject: [u8; 32],
    pub decision: GateDecision,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TrustedContextError {
    #[error("signed ExecutionLock has unexpected signing role")]
    ExecutionLockRoleMismatch,
    #[error("signed ExecutionLock verification failed: {0}")]
    ExecutionLock(String),
    #[error("ConformanceEvidence verification failed: {0}")]
    ConformanceEvidence(String),
    #[error("GateAttestation verification failed: {0}")]
    GateAttestation(String),
    #[error("trusted context identity mismatch: {0}")]
    IdentityMismatch(&'static str),
    #[error("trusted context pack type mismatch: expected {expected}, got {received}")]
    PackTypeMismatch { expected: &'static str, received: String },
    #[error("trusted context platform mismatch: {0}")]
    PlatformMismatch(&'static str),
    #[error("trusted context protocol or canonical set mismatch")]
    ProtocolMismatch,
    #[error("trusted context repository or commit mismatch")]
    RepositoryCommitMismatch,
    #[error("trusted context TrustCheckpoint verification failed: {0}")]
    TrustCheckpoint(String),
    #[error("trusted context TrustCheckpoint was not present in the persisted trust state")]
    TrustCheckpointNotPersisted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedContextExpectations {
    pub repository: String,
    pub commit: String,
    pub policy_digest: String,
    pub backend_id: String,
    pub backend_version: String,
    pub resolver_id: String,
    pub resolver_version: String,
    pub platform: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedContextVerifier {
    pub expected: TrustedContextExpectations,
}

impl TrustedContextVerifier {
    pub fn new(expected: TrustedContextExpectations) -> Self {
        Self { expected }
    }

    pub fn verify(&self, input: TrustedContextInput<'_>) -> Result<TrustedContext, TrustedContextError> {
        if self.expected.repository.is_empty()
            || self.expected.commit.is_empty()
            || self.expected.policy_digest.is_empty()
            || self.expected.backend_id.is_empty()
            || self.expected.backend_version.is_empty()
            || self.expected.resolver_id.is_empty()
            || self.expected.resolver_version.is_empty()
            || self.expected.platform.is_empty()
        {
            return Err(TrustedContextError::RepositoryCommitMismatch);
        }
        if self.expected.platform != TRUSTED_PLATFORM {
            return Err(TrustedContextError::PlatformMismatch("verifier expectations"));
        }
        let signed_lock = input.signed_execution_lock;
        if signed_lock.role != EXECUTION_LOCK_SIGNING_ROLE {
            return Err(TrustedContextError::ExecutionLockRoleMismatch);
        }
        signed_lock.verify_at(input.now_epoch_secs).map_err(TrustedContextError::ExecutionLock)?;

        if input.runner_measurement.platform != self.expected.platform {
            return Err(TrustedContextError::PlatformMismatch("runner"));
        }
        if input.backend_pack_measurement.platform != self.expected.platform {
            return Err(TrustedContextError::PlatformMismatch("backend pack"));
        }
        if input.resolver_pack_measurement.platform != self.expected.platform {
            return Err(TrustedContextError::PlatformMismatch("resolver pack"));
        }
        if input.backend_pack_measurement.asset_type != BACKEND_RUNTIME_PACK_ASSET_TYPE {
            return Err(TrustedContextError::PackTypeMismatch { expected: BACKEND_RUNTIME_PACK_ASSET_TYPE, received: input.backend_pack_measurement.asset_type.clone() });
        }
        if input.resolver_pack_measurement.asset_type != WORKSPACE_RESOLVER_PACK_ASSET_TYPE {
            return Err(TrustedContextError::PackTypeMismatch { expected: WORKSPACE_RESOLVER_PACK_ASSET_TYPE, received: input.resolver_pack_measurement.asset_type.clone() });
        }

        if let Some(metadata) = input.conformance_metadata {
            input
                .conformance_evidence
                .verify_signed_with_metadata(input.signed_conformance_evidence, input.trust_store, VERIFICATION_SIGNING_ROLE, metadata, input.now_epoch_secs)
                .map_err(|error| TrustedContextError::ConformanceEvidence(error.to_string()))?;
        } else {
            input
                .conformance_evidence
                .verify_signed(input.signed_conformance_evidence, input.trust_store, VERIFICATION_SIGNING_ROLE)
                .map_err(|error| TrustedContextError::ConformanceEvidence(error.to_string()))?;
        }
        if let Some(metadata) = input.gate_metadata {
            input
                .gate_attestation
                .verify_signed_with_metadata(input.signed_gate_attestation, input.trust_store, GATE_SIGNING_ROLE, metadata, input.now_epoch_secs)
                .map_err(TrustedContextError::GateAttestation)?;
        } else {
            input
                .gate_attestation
                .verify_signed(input.signed_gate_attestation, input.trust_store, GATE_SIGNING_ROLE, input.now_epoch_secs)
                .map_err(TrustedContextError::GateAttestation)?;
        }

        let lock = &signed_lock.lock;
        if input.gate_attestation.repository != self.expected.repository
            || input.gate_attestation.commit != self.expected.commit
        {
            return Err(TrustedContextError::RepositoryCommitMismatch);
        }
        if input.gate_attestation.policy_digest != self.expected.policy_digest {
            return Err(TrustedContextError::IdentityMismatch("GateAttestation policy"));
        }
        if lock.backend_id != self.expected.backend_id
            || lock.backend_version != self.expected.backend_version
            || lock.resolver_id != self.expected.resolver_id
            || lock.resolver_version != self.expected.resolver_version
        {
            return Err(TrustedContextError::IdentityMismatch("execution lock tool identity"));
        }
        if !same_digest(lock.runner_release_digest.as_str(), &input.runner_measurement.artifact_digest)
            || !same_digest(lock.backend_pack_digest.as_str(), &input.backend_pack_measurement.manifest_digest)
            || !same_digest(lock.resolver_pack_digest.as_str(), &input.resolver_pack_measurement.manifest_digest)
        {
            return Err(TrustedContextError::IdentityMismatch("execution lock measurement"));
        }
        if input.conformance_evidence.runner_release_digest != lock.runner_release_digest
            || input.conformance_evidence.backend_pack_digest != lock.backend_pack_digest
            || input.conformance_evidence.resolver_pack_digest != lock.resolver_pack_digest
            || input.conformance_evidence.sandbox_provider_id != lock.sandbox_provider_id
            || input.conformance_evidence.sandbox_policy_digest != lock.sandbox_policy_digest
            || input.conformance_evidence.resolver_sandbox_provider_id != lock.resolver_sandbox_provider_id
            || input.conformance_evidence.resolver_sandbox_policy_digest != lock.resolver_sandbox_policy_digest
            || !same_digest(lock.conformance_suite_digest.as_str(), &input.conformance_evidence.suite_digest)
        {
            return Err(TrustedContextError::IdentityMismatch("conformance evidence"));
        }
        if input.conformance_evidence.platform != self.expected.platform {
            return Err(TrustedContextError::PlatformMismatch("conformance evidence"));
        }
        if input.conformance_evidence.protocol_major != lock.protocol_major {
            return Err(TrustedContextError::ProtocolMismatch);
        }
        if input.gate_attestation.rule_bundle_digest != lock.rule_bundle_digest
            || input.gate_attestation.execution_lock_digest != lock.lock_digest
            || input.gate_attestation.runner_release_digest != lock.runner_release_digest
            || input.gate_attestation.sandbox_provider_id != lock.sandbox_provider_id
            || input.gate_attestation.sandbox_policy_digest != lock.sandbox_policy_digest
        {
            return Err(TrustedContextError::IdentityMismatch("GateAttestation"));
        }
        let gate_input = &input.gate_input;
        if input.gate_attestation.baseline_digest != gate_input.baseline_digest
            || input.gate_attestation.workspace_attestation_digest != gate_input.workspace_attestation_digest
            || input.gate_attestation.execution_evidence_digest != gate_input.execution_evidence_digest
        {
            return Err(TrustedContextError::IdentityMismatch("GateAttestation input digests"));
        }
        if input.gate_attestation.workload_issuer != gate_input.workload_issuer
            || input.gate_attestation.workload_audience != gate_input.workload_audience
            || input.gate_attestation.workflow != gate_input.workflow
            || input.gate_attestation.run_identity != gate_input.run_identity
            || input.gate_attestation.environment_identity != gate_input.environment_identity
        {
            return Err(TrustedContextError::IdentityMismatch("GateAttestation workload identity"));
        }

        Ok(TrustedContext {
            repository: input.gate_attestation.repository.clone(),
            commit: input.gate_attestation.commit.clone(),
            execution_lock_digest: lock.lock_digest.clone(),
            runner_release_digest: lock.runner_release_digest.clone(),
            backend_pack_digest: lock.backend_pack_digest.clone(),
            resolver_pack_digest: lock.resolver_pack_digest.clone(),
            conformance_evidence_subject: input.signed_conformance_evidence.subject_digest(),
            gate_attestation_subject: input.gate_attestation.subject_digest(),
            decision: input.gate_attestation.decision,
        })
    }

    /// Verifies the full context only after proving that the supplied signed
    /// checkpoint is already reflected in the persisted trust floor. A
    /// caller that merely presents a fresh checkpoint without first using the
    /// trust-state persistence adapter is rejected, preventing a scan from
    /// silently bootstrapping or advancing trust.
    pub fn verify_with_checkpoint(
        &self,
        input: TrustedContextInput<'_>,
        checkpoint: TrustedCheckpointInput<'_>,
    ) -> Result<TrustedContext, TrustedContextError> {
        if checkpoint.signing_role != TRUST_CHECKPOINT_SIGNING_ROLE || checkpoint.signing_role.is_empty() {
            return Err(TrustedContextError::TrustCheckpoint("unexpected checkpoint signing role".into()));
        }
        if checkpoint.checkpoint.repository != self.expected.repository {
            return Err(TrustedContextError::TrustCheckpoint("checkpoint repository does not match trusted context".into()));
        }
        let mut replayed_state = checkpoint.persisted_trust_state.as_trust_state().clone();
        replayed_state
            .accept_signed_checkpoint(
                checkpoint.checkpoint,
                checkpoint.signed_checkpoint,
                input.trust_store,
                checkpoint.signing_role,
                input.now_epoch_secs,
            )
            .map_err(|error| TrustedContextError::TrustCheckpoint(error.to_string()))?;
        if replayed_state.snapshot() != checkpoint.persisted_trust_state.snapshot() {
            return Err(TrustedContextError::TrustCheckpointNotPersisted);
        }
        self.verify(input)
    }
}

fn same_digest(expected_hex: &str, actual: &[u8; 32]) -> bool {
    expected_hex.len() == 64 && expected_hex.eq_ignore_ascii_case(&hex_digest(actual))
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use erf_bootstrap::{RunnerMeasurement, LINUX_AMD64};
    use erf_conformance::{CheckResult, ConformanceEvidence, SuiteReport, VERIFICATION_SIGNING_ROLE, EXPECTED_ACCEPTANCE_CHECKS, SUITE_MAJOR, SUITE_VERSION};
    use erf_verification_worker::sign_verified_evidence;
    use erf_packs::PackMeasurement;
    use erf_trust_state::TrustStateFile;
    use rf_runner::ExecutionLock;
    use std::collections::BTreeMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn lock(conformance_suite_digest: String) -> ExecutionLock {
        let mut lock = ExecutionLock {
            rule_bundle_digest: "a".repeat(64),
            runner_release_digest: "b".repeat(64),
            backend_pack_digest: "c".repeat(64),
            resolver_pack_digest: "d".repeat(64),
            sandbox_provider_id: "rootless-oci-docker-v0".into(),
            sandbox_policy_digest: "e".repeat(64),
            resolver_sandbox_provider_id: "rootless-oci-docker-v0".into(),
            resolver_sandbox_policy_digest: "1".repeat(64),
            conformance_suite_digest,
            resolver_id: "maven-static".into(),
            resolver_version: "0.1".into(),
            backend_id: "pmd-java".into(),
            backend_version: "7.9.0".into(),
            protocol_major: erf_host_client::PROTOCOL_MAJOR,
            canonical_set_version: rf_gate::CANONICAL_SET_VERSION.into(),
            lock_digest: String::new(),
        };
        lock.lock_digest = lock.content_digest();
        lock
    }

    fn measurements() -> (RunnerMeasurement, PackMeasurement, PackMeasurement) {
        (
            RunnerMeasurement { release_version: "runner-0.1".into(), platform: LINUX_AMD64.into(), artifact_digest: [0xbb; 32], statement_digest: [1; 32] },
            PackMeasurement { asset_type: BACKEND_RUNTIME_PACK_ASSET_TYPE.into(), platform: LINUX_AMD64.into(), manifest_digest: [0xcc; 32], statement_digest: [2; 32] },
            PackMeasurement { asset_type: WORKSPACE_RESOLVER_PACK_ASSET_TYPE.into(), platform: LINUX_AMD64.into(), manifest_digest: [0xdd; 32], statement_digest: [3; 32] },
        )
    }

    fn make_input() -> TrustedContextInput<'static> {
        let lock_key = SigningKey::from_bytes(&[71; 32]);
        let gate_key = SigningKey::from_bytes(&[72; 32]);
        let evidence_key = SigningKey::from_bytes(&[73; 32]);
        let mut trust_store = TrustStore::default();
        trust_store.insert_key("gate-key", gate_key.verifying_key()).unwrap();
        trust_store.insert_key("evidence-key", evidence_key.verifying_key()).unwrap();
        trust_store.authorize_role(GATE_SIGNING_ROLE, "gate-key").unwrap();
        trust_store.authorize_role(VERIFICATION_SIGNING_ROLE, "evidence-key").unwrap();
        let checks = EXPECTED_ACCEPTANCE_CHECKS
            .iter()
            .map(|&name| CheckResult { name, passed: true, detail: "acceptance check passed".into() })
            .collect();
        let report = SuiteReport { suite_version: SUITE_VERSION.into(), protocol_major: SUITE_MAJOR, checks };
        let suite_digest = hex_digest(&report.suite_digest());
        let lock = lock(suite_digest);
        let lock_payload = lock.canonical_payload();
        let lock_asset = SignedAsset {
            payload_type: erf_trust::DSSE_PAYLOAD_TYPE.into(),
            payload: lock_payload.clone(),
            key_id: "lock-key".into(),
            signature: lock_key.sign(&erf_trust::pae(erf_trust::DSSE_PAYLOAD_TYPE, &lock_payload)).to_bytes().to_vec(),
        };
        let mut signed_lock = Box::new(SignedExecutionLock { lock, asset: lock_asset, role: EXECUTION_LOCK_SIGNING_ROLE.into(), trust_store: TrustStore::default(), metadata: None });
        signed_lock.trust_store.insert_key("lock-key", lock_key.verifying_key()).unwrap();
        signed_lock.trust_store.authorize_role(EXECUTION_LOCK_SIGNING_ROLE, "lock-key").unwrap();
        let (runner, backend, resolver) = measurements();
        let evidence = ConformanceEvidence::from_report(&report, "a".repeat(64), "b".repeat(64), hex_digest(&[0xbb; 32]), hex_digest(&[0xcc; 32]), hex_digest(&[0xdd; 32]), "rootless-oci-docker-v0", "e".repeat(64), "rootless-oci-docker-v0", "1".repeat(64), LINUX_AMD64).unwrap();
        let evidence_asset = sign_verified_evidence(&evidence, "evidence-key", |bytes| evidence_key.sign(bytes).to_bytes().to_vec()).unwrap();
        let gate = GateAttestation {
            repository: "org/repo".into(), commit: "abc".into(), policy_digest: "f".repeat(64), rule_bundle_digest: "a".repeat(64), execution_lock_digest: signed_lock.lock.lock_digest.clone(), runner_release_digest: "b".repeat(64), workspace_descriptor_digest: "1".repeat(64), baseline_digest: "0".repeat(64), workspace_attestation_digest: "1".repeat(64), execution_evidence_digest: "2".repeat(64), sandbox_provider_id: "rootless-oci-docker-v0".into(), sandbox_policy_digest: "e".repeat(64), workload_issuer: "https://issuer.example".into(), workload_audience: "erf-gate".into(), workflow: "ci".into(), run_identity: "run-1".into(), environment_identity: "linux/amd64".into(), semantic_result_digest: [9; 32], completeness: rf_gate::Completeness { planned: 1, accounted: 1, failed: 0, missing: 0, observation_errors: 0, complete: true }, decision: GateDecision::Pass, exception_grant_digests: Vec::new(), issued_at_epoch_secs: 1, expires_at_epoch_secs: 100,
        };
        let gate_asset = gate.sign_with("gate-key", |bytes| gate_key.sign(bytes).to_bytes().to_vec());
        let gate_input = GateInputIdentity {
            baseline_digest: Box::leak("0".repeat(64).into_boxed_str()),
            workspace_attestation_digest: Box::leak("1".repeat(64).into_boxed_str()),
            execution_evidence_digest: Box::leak("2".repeat(64).into_boxed_str()),
            workload_issuer: "https://issuer.example",
            workload_audience: "erf-gate",
            workflow: "ci",
            run_identity: "run-1",
            environment_identity: "linux/amd64",
        };
        TrustedContextInput { signed_execution_lock: Box::leak(signed_lock), runner_measurement: Box::leak(Box::new(runner)), backend_pack_measurement: Box::leak(Box::new(backend)), resolver_pack_measurement: Box::leak(Box::new(resolver)), conformance_evidence: Box::leak(Box::new(evidence)), signed_conformance_evidence: Box::leak(Box::new(evidence_asset)), conformance_metadata: None, gate_attestation: Box::leak(Box::new(gate)), signed_gate_attestation: Box::leak(Box::new(gate_asset)), gate_input, gate_metadata: None, trust_store: Box::leak(Box::new(trust_store)), now_epoch_secs: 10 }
    }

    fn verifier() -> TrustedContextVerifier {
        TrustedContextVerifier::new(TrustedContextExpectations {
            repository: "org/repo".into(),
            commit: "abc".into(),
            policy_digest: "f".repeat(64),
            backend_id: "pmd-java".into(),
            backend_version: "7.9.0".into(),
            resolver_id: "maven-static".into(),
            resolver_version: "0.1".into(),
            platform: LINUX_AMD64.into(),
        })
    }

    #[test]
    fn closure_verifies_only_when_all_assets_share_lock_identity() {
        let input = make_input();
        let context = verifier().verify(input).unwrap();
        assert_eq!(context.repository, "org/repo");
        assert_eq!(context.commit, "abc");
        assert_eq!(context.decision, GateDecision::Pass);
    }

    #[test]
    fn closure_rejects_measurement_mismatch_and_wrong_lock_role() {
        let mut input = make_input();
        let mut changed = (*input.runner_measurement).clone();
        changed.artifact_digest = [0xaa; 32];
        input.runner_measurement = Box::leak(Box::new(changed));
        assert_eq!(verifier().verify(input), Err(TrustedContextError::IdentityMismatch("execution lock measurement")));

        let mut input = make_input();
        let mut changed_lock = (*input.signed_execution_lock).clone();
        changed_lock.role = "wrong-role".into();
        input.signed_execution_lock = Box::leak(Box::new(changed_lock));
        assert_eq!(verifier().verify(input), Err(TrustedContextError::ExecutionLockRoleMismatch));

        let input = make_input();
        let wrong_backend = TrustedContextVerifier::new(TrustedContextExpectations { backend_id: "other-backend".into(), ..verifier().expected });
        assert_eq!(wrong_backend.verify(input), Err(TrustedContextError::IdentityMismatch("execution lock tool identity")));
    }

    #[test]
    fn closure_rejects_expired_gate_or_evidence_metadata_and_cross_commit() {
        let mut input = make_input();
        input.now_epoch_secs = 100;
        assert!(matches!(verifier().verify(input), Err(TrustedContextError::GateAttestation(_))));

        let mut input = make_input();
        input.conformance_metadata = Some(Box::leak(Box::new(TrustMetadata { version: 1, expires_at_epoch_secs: 10, revoked_key_ids: Default::default() })));
        assert!(matches!(verifier().verify(input), Err(TrustedContextError::ConformanceEvidence(_))));

        let input = make_input();
        let wrong_commit = TrustedContextVerifier::new(TrustedContextExpectations { commit: "other".into(), ..verifier().expected });
        assert_eq!(wrong_commit.verify(input), Err(TrustedContextError::RepositoryCommitMismatch));
    }

    #[test]
    fn closure_rejects_gate_attestation_for_a_different_policy() {
        let mut input = make_input();
        let mut changed = (*input.gate_attestation).clone();
        changed.policy_digest = "0".repeat(64);
        input.gate_attestation = Box::leak(Box::new(changed));
        assert_eq!(verifier().verify(input), Err(TrustedContextError::GateAttestation("gate attestation payload does not match statement".into())));
    }

    #[test]
    fn trusted_closure_requires_a_signed_checkpoint_already_reflected_in_state() {
        let mut input = make_input();
        let checkpoint_key = SigningKey::from_bytes(&[74; 32]);
        let checkpoint = TrustCheckpoint {
            repository: "org/repo".into(),
            version: 3,
            role_versions: BTreeMap::from([(String::from(TRUST_CHECKPOINT_SIGNING_ROLE), 3)]),
            snapshot_digest: [75; 32],
            issued_at_epoch_secs: 1,
            expires_at_epoch_secs: 100,
        };
        let signed_checkpoint = checkpoint.sign_with("checkpoint-key", |bytes| checkpoint_key.sign(bytes).to_bytes().to_vec());
        let mut trust_store = (*input.trust_store).clone();
        trust_store.insert_key("checkpoint-key", checkpoint_key.verifying_key()).unwrap();
        trust_store.authorize_role(TRUST_CHECKPOINT_SIGNING_ROLE, "checkpoint-key").unwrap();
        input.trust_store = Box::leak(Box::new(trust_store));

        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let persisted_path = std::env::temp_dir().join(format!("erf-trusted-context-checkpoint-{}-{suffix}", std::process::id()));
        let persisted_file = TrustStateFile::new(&persisted_path);
        persisted_file.initialize_signed_checkpoint(&checkpoint, &signed_checkpoint, input.trust_store, TRUST_CHECKPOINT_SIGNING_ROLE, 10).unwrap();
        let persisted_state = Box::leak(Box::new(persisted_file.load_for_trusted_execution().unwrap()));
        let accepted = verifier().verify_with_checkpoint(input.clone(), TrustedCheckpointInput {
            checkpoint: Box::leak(Box::new(checkpoint.clone())),
            signed_checkpoint: Box::leak(Box::new(signed_checkpoint.clone())),
            persisted_trust_state: persisted_state,
            signing_role: TRUST_CHECKPOINT_SIGNING_ROLE,
        });
        assert!(accepted.is_ok());

        let stale_checkpoint = TrustCheckpoint { version: 2, snapshot_digest: [76; 32], ..checkpoint.clone() };
        let stale_signed_checkpoint = stale_checkpoint.sign_with("checkpoint-key", |bytes| checkpoint_key.sign(bytes).to_bytes().to_vec());
        let stale_path = std::env::temp_dir().join(format!("erf-trusted-context-stale-checkpoint-{}-{suffix}", std::process::id()));
        let stale_file = TrustStateFile::new(&stale_path);
        stale_file.initialize_signed_checkpoint(&stale_checkpoint, &stale_signed_checkpoint, input.trust_store, TRUST_CHECKPOINT_SIGNING_ROLE, 10).unwrap();
        let stale_state = Box::leak(Box::new(stale_file.load_for_trusted_execution().unwrap()));
        let not_persisted = verifier().verify_with_checkpoint(input, TrustedCheckpointInput {
            checkpoint: Box::leak(Box::new(checkpoint)),
            signed_checkpoint: Box::leak(Box::new(signed_checkpoint)),
            persisted_trust_state: stale_state,
            signing_role: TRUST_CHECKPOINT_SIGNING_ROLE,
        });
        assert_eq!(not_persisted, Err(TrustedContextError::TrustCheckpointNotPersisted));
        fs::remove_file(persisted_path).unwrap();
        fs::remove_file(stale_path).unwrap();
    }
}
