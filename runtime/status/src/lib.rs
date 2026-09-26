//! Credential-isolated Status Publisher core.
//!
//! This crate verifies an already-produced GateAttestation and turns its
//! three-state decision into a transport-neutral SCM status model. It has no
//! SCM client, network access or write credential; an adapter may publish the
//! returned model after this boundary accepts it.

use rf_gate::{GateAttestation, GateDecision};
use erf_trust::{SignedAsset, TrustStore};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::Digest;
use thiserror::Error;

mod deployment;

pub const DEFAULT_GATE_SIGNING_ROLE: &str = "gate-signing";
pub const STATUS_PUBLICATION_API_VERSION: &str = "rulefactory.io/v1";
pub const STATUS_PUBLICATION_KIND: &str = "StatusPublication";
pub const STATUS_PUBLICATION_SCHEMA_VERSION: &str = "status-publication-v1";

pub use deployment::{
    StatusAdapterDeliveryStatus, StatusAdapterDeployment, StatusAdapterDeploymentError,
    StatusAdapterReceipt, STATUS_ADAPTER_DEPLOYMENT_KIND, STATUS_ADAPTER_DEPLOYMENT_SCHEMA_VERSION,
    STATUS_ADAPTER_RECEIPT_KIND,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishedState {
    Success,
    Failure,
    ActionRequired,
}

impl PublishedState {
    pub fn from_decision(decision: GateDecision) -> Self {
        match decision {
            GateDecision::Pass => Self::Success,
            GateDecision::Fail => Self::Failure,
            GateDecision::Inconclusive => Self::ActionRequired,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusPublisherContext {
    pub repository: String,
    pub commit: String,
    pub policy_digest: String,
    pub gate_signing_role: String,
}

impl StatusPublisherContext {
    pub fn new(repository: impl Into<String>, commit: impl Into<String>, policy_digest: impl Into<String>) -> Self {
        Self {
            repository: repository.into(),
            commit: commit.into(),
            policy_digest: policy_digest.into(),
            gate_signing_role: DEFAULT_GATE_SIGNING_ROLE.into(),
        }
    }

    pub fn with_gate_signing_role(mut self, role: impl Into<String>) -> Self {
        self.gate_signing_role = role.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StatusIdentity {
    pub repository: String,
    pub commit: String,
    pub policy_digest: String,
    pub attestation_subject: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedStatus {
    pub identity: StatusIdentity,
    pub state: PublishedState,
    pub decision: GateDecision,
    pub issued_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
    pub run_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusPublication {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub status: String,
    pub repository: String,
    pub commit: String,
    #[serde(rename = "policyDigest")]
    pub policy_digest: String,
    #[serde(rename = "attestationSubject")]
    pub attestation_subject: String,
    pub decision: String,
    pub state: String,
    #[serde(rename = "runSequence")]
    pub run_sequence: u64,
    #[serde(rename = "issuedAtEpochSecs")]
    pub issued_at_epoch_secs: u64,
    #[serde(rename = "expiresAtEpochSecs")]
    pub expires_at_epoch_secs: u64,
    #[serde(rename = "publicationDigest")]
    pub publication_digest: String,
    #[serde(rename = "externalAdapterRequired")]
    pub external_adapter_required: bool,
    #[serde(rename = "scmWritten")]
    pub scm_written: bool,
}

impl PublishedStatus {
    pub fn publication(&self) -> StatusPublication {
        let mut publication = StatusPublication {
            api_version: STATUS_PUBLICATION_API_VERSION.into(),
            kind: STATUS_PUBLICATION_KIND.into(),
            schema_version: STATUS_PUBLICATION_SCHEMA_VERSION.into(),
            status: "READY_FOR_ADAPTER".into(),
            repository: self.identity.repository.clone(),
            commit: self.identity.commit.clone(),
            policy_digest: self.identity.policy_digest.clone(),
            attestation_subject: format!("sha256:{}", hex(&self.identity.attestation_subject)),
            decision: decision_name(self.decision).into(),
            state: state_name(self.state).into(),
            run_sequence: self.run_sequence,
            issued_at_epoch_secs: self.issued_at_epoch_secs,
            expires_at_epoch_secs: self.expires_at_epoch_secs,
            publication_digest: String::new(),
            external_adapter_required: true,
            scm_written: false,
        };
        publication.publication_digest = publication.content_digest();
        publication
    }
}

impl StatusPublication {
    pub fn as_json(&self) -> Value {
        serde_json::to_value(self).expect("StatusPublication is serializable")
    }

    pub fn validate(&self) -> Result<(), StatusAdapterError> {
        if self.api_version != STATUS_PUBLICATION_API_VERSION
            || self.kind != STATUS_PUBLICATION_KIND
            || self.schema_version != STATUS_PUBLICATION_SCHEMA_VERSION
            || self.status != "READY_FOR_ADAPTER"
        {
            return Err(StatusAdapterError::InvalidPublication("publication version or status".into()));
        }
        if self.repository.trim().is_empty()
            || self.commit.trim().is_empty()
            || self.repository.chars().any(char::is_control)
            || self.commit.chars().any(char::is_control)
        {
            return Err(StatusAdapterError::InvalidPublication("repository or commit".into()));
        }
        if !is_hex_digest(&self.policy_digest) || !is_sha256_digest(&self.attestation_subject) {
            return Err(StatusAdapterError::InvalidPublication("publication identity digest".into()));
        }
        if self.run_sequence == 0 || self.issued_at_epoch_secs >= self.expires_at_epoch_secs {
            return Err(StatusAdapterError::InvalidPublication("run sequence or freshness".into()));
        }
        let expected_state = match self.decision.as_str() {
            "PASS" => "success",
            "FAIL" => "failure",
            "INCONCLUSIVE" => "action-required",
            _ => return Err(StatusAdapterError::InvalidPublication("decision".into())),
        };
        if self.state != expected_state {
            return Err(StatusAdapterError::InvalidPublication("decision/state mapping".into()));
        }
        if !self.external_adapter_required || self.scm_written {
            return Err(StatusAdapterError::InvalidPublication("SCM authority boundary".into()));
        }
        if !is_sha256_digest(&self.publication_digest) || self.publication_digest != self.content_digest() {
            return Err(StatusAdapterError::DigestMismatch);
        }
        Ok(())
    }

    pub fn validate_at(&self, now_epoch_secs: u64) -> Result<(), StatusAdapterError> {
        self.validate()?;
        if now_epoch_secs < self.issued_at_epoch_secs || now_epoch_secs >= self.expires_at_epoch_secs {
            return Err(StatusAdapterError::NotFresh {
                now: now_epoch_secs,
                issued: self.issued_at_epoch_secs,
                expires: self.expires_at_epoch_secs,
            });
        }
        Ok(())
    }

    fn content_digest(&self) -> String {
        let mut value = self.as_json();
        value
            .as_object_mut()
            .expect("StatusPublication serializes as an object")
            .remove("publicationDigest");
        let canonical = canonical_json(&value);
        let bytes = serde_json::to_vec(&canonical).expect("StatusPublication serialization is infallible");
        format!("sha256:{}", hex(&sha2::Sha256::digest(bytes)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishOutcome {
    Published(PublishedStatus),
    Idempotent(PublishedStatus),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterOutcome {
    Accepted(StatusPublication),
    Idempotent(StatusPublication),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StatusAdapterError {
    #[error("status publication is invalid: {0}")]
    InvalidPublication(String),
    #[error("status publication digest does not match its content")]
    DigestMismatch,
    #[error("publication identity does not match the adapter target")]
    IdentityMismatch,
    #[error("publication run sequence {received} is older than {current}")]
    StaleRun { received: u64, current: u64 },
    #[error("publication run sequence {sequence} conflicts with an existing publication")]
    SequenceConflict { sequence: u64 },
    #[error("publication issued at {received} is not newer than {current}")]
    StalePublication { received: u64, current: u64 },
    #[error("publication is not fresh at {now}; valid interval is [{issued}, {expires})")]
    NotFresh { now: u64, issued: u64, expires: u64 },
}

pub trait StatusPublicationAdapter {
    fn accept(&mut self, publication: &StatusPublication, now_epoch_secs: u64) -> Result<AdapterOutcome, StatusAdapterError>;
}

#[derive(Debug, Clone, Default)]
pub struct FakeStatusAdapter {
    latest: Option<StatusPublication>,
}

impl FakeStatusAdapter {
    pub fn latest(&self) -> Option<&StatusPublication> {
        self.latest.as_ref()
    }
}

impl StatusPublicationAdapter for FakeStatusAdapter {
    fn accept(&mut self, publication: &StatusPublication, now_epoch_secs: u64) -> Result<AdapterOutcome, StatusAdapterError> {
        publication.validate_at(now_epoch_secs)?;
        if let Some(current) = &self.latest {
            if current.publication_digest == publication.publication_digest {
                if current.run_sequence == publication.run_sequence {
                    return Ok(AdapterOutcome::Idempotent(publication.clone()));
                }
                return Err(StatusAdapterError::SequenceConflict { sequence: publication.run_sequence });
            }
            if current.repository != publication.repository
                || current.commit != publication.commit
                || current.policy_digest != publication.policy_digest
            {
                return Err(StatusAdapterError::IdentityMismatch);
            }
            if publication.run_sequence < current.run_sequence {
                return Err(StatusAdapterError::StaleRun {
                    received: publication.run_sequence,
                    current: current.run_sequence,
                });
            }
            if publication.run_sequence == current.run_sequence {
                return Err(StatusAdapterError::SequenceConflict { sequence: publication.run_sequence });
            }
            if publication.issued_at_epoch_secs <= current.issued_at_epoch_secs {
                return Err(StatusAdapterError::StalePublication {
                    received: publication.issued_at_epoch_secs,
                    current: current.issued_at_epoch_secs,
                });
            }
        }
        self.latest = Some(publication.clone());
        Ok(AdapterOutcome::Accepted(publication.clone()))
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StatusPublishError {
    #[error("publisher context has an invalid {0}")]
    InvalidContext(&'static str),
    #[error("gate attestation repository or commit does not match publisher context")]
    RepositoryCommitMismatch,
    #[error("gate attestation policy does not match publisher context")]
    PolicyMismatch,
    #[error("gate attestation is not publishable: {0}")]
    AttestationInvalid(String),
    #[error("a PASS attestation is incomplete")]
    IncompletePass,
    #[error("run sequence {received} is older than the published sequence {current}")]
    StaleRun { received: u64, current: u64 },
    #[error("run sequence {sequence} already has a different attestation subject")]
    SequenceConflict { sequence: u64 },
    #[error("the same attestation subject was presented with a different run sequence")]
    SubjectSequenceConflict,
    #[error("attestation issued at {received} is not newer than the published proof at {current}")]
    StaleAttestation { received: u64, current: u64 },
}

/// In-memory publisher state for one repository/commit/policy target.
///
/// The state model is intentionally transport-neutral. A real SCM adapter
/// should persist the accepted identity and apply the same monotonic checks
/// before writing a commit status or check.
#[derive(Debug, Clone)]
pub struct StatusPublisher {
    context: StatusPublisherContext,
    latest: Option<PublishedStatus>,
}

impl StatusPublisher {
    pub fn new(context: StatusPublisherContext) -> Self {
        Self { context, latest: None }
    }

    pub fn context(&self) -> &StatusPublisherContext {
        &self.context
    }

    pub fn latest(&self) -> Option<&PublishedStatus> {
        self.latest.as_ref()
    }

    pub fn publish(
        &mut self,
        attestation: &GateAttestation,
        signed_attestation: &SignedAsset,
        trust_store: &TrustStore,
        now_epoch_secs: u64,
        run_sequence: u64,
    ) -> Result<PublishOutcome, StatusPublishError> {
        self.validate_context()?;
        if attestation.repository != self.context.repository || attestation.commit != self.context.commit {
            return Err(StatusPublishError::RepositoryCommitMismatch);
        }
        if attestation.policy_digest != self.context.policy_digest {
            return Err(StatusPublishError::PolicyMismatch);
        }
        if !attestation.completeness.complete {
            if attestation.decision == GateDecision::Pass {
                return Err(StatusPublishError::IncompletePass);
            }
        }
        let subject = attestation
            .verify_signed(signed_attestation, trust_store, &self.context.gate_signing_role, now_epoch_secs)
            .map_err(StatusPublishError::AttestationInvalid)?;

        if let Some(current) = &self.latest {
            if current.identity.attestation_subject == subject {
                if current.run_sequence == run_sequence {
                    return Ok(PublishOutcome::Idempotent(current.clone()));
                }
                return Err(StatusPublishError::SubjectSequenceConflict);
            }
            if run_sequence < current.run_sequence {
                return Err(StatusPublishError::StaleRun { received: run_sequence, current: current.run_sequence });
            }
            if run_sequence == current.run_sequence {
                return Err(StatusPublishError::SequenceConflict { sequence: run_sequence });
            }
            if attestation.issued_at_epoch_secs <= current.issued_at_epoch_secs {
                return Err(StatusPublishError::StaleAttestation {
                    received: attestation.issued_at_epoch_secs,
                    current: current.issued_at_epoch_secs,
                });
            }
        }

        let published = PublishedStatus {
            identity: StatusIdentity {
                repository: self.context.repository.clone(),
                commit: self.context.commit.clone(),
                policy_digest: self.context.policy_digest.clone(),
                attestation_subject: subject,
            },
            state: PublishedState::from_decision(attestation.decision),
            decision: attestation.decision,
            issued_at_epoch_secs: attestation.issued_at_epoch_secs,
            expires_at_epoch_secs: attestation.expires_at_epoch_secs,
            run_sequence,
        };
        self.latest = Some(published.clone());
        Ok(PublishOutcome::Published(published))
    }

    pub fn publish_artifact(
        &mut self,
        attestation: &GateAttestation,
        signed_attestation: &SignedAsset,
        trust_store: &TrustStore,
        now_epoch_secs: u64,
        run_sequence: u64,
    ) -> Result<StatusPublication, StatusPublishError> {
        let outcome = self.publish(attestation, signed_attestation, trust_store, now_epoch_secs, run_sequence)?;
        Ok(match outcome {
            PublishOutcome::Published(status) | PublishOutcome::Idempotent(status) => status.publication(),
        })
    }

    fn validate_context(&self) -> Result<(), StatusPublishError> {
        if self.context.repository.is_empty() {
            return Err(StatusPublishError::InvalidContext("repository"));
        }
        if self.context.commit.is_empty() {
            return Err(StatusPublishError::InvalidContext("commit"));
        }
        if !is_hex_digest(&self.context.policy_digest) {
            return Err(StatusPublishError::InvalidContext("policy digest"));
        }
        if self.context.gate_signing_role.is_empty() {
            return Err(StatusPublishError::InvalidContext("gate signing role"));
        }
        Ok(())
    }
}

fn is_hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn is_sha256_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn decision_name(decision: GateDecision) -> &'static str {
    match decision {
        GateDecision::Pass => "PASS",
        GateDecision::Fail => "FAIL",
        GateDecision::Inconclusive => "INCONCLUSIVE",
    }
}

fn state_name(state: PublishedState) -> &'static str {
    match state {
        PublishedState::Success => "success",
        PublishedState::Failure => "failure",
        PublishedState::ActionRequired => "action-required",
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort_unstable();
            let mut canonical = Map::new();
            for key in keys {
                canonical.insert(key.clone(), canonical_json(&object[key]));
            }
            Value::Object(canonical)
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    fn attestation(repository: &str, commit: &str, decision: GateDecision, issued: u64, expires: u64, complete: bool) -> GateAttestation {
        GateAttestation {
            repository: repository.into(),
            commit: commit.into(),
            policy_digest: "f".repeat(64),
            rule_bundle_digest: "a".repeat(64),
            execution_lock_digest: "b".repeat(64),
            runner_release_digest: "c".repeat(64),
            workspace_descriptor_digest: "d".repeat(64),
            baseline_digest: "0".repeat(64),
            workspace_attestation_digest: "1".repeat(64),
            execution_evidence_digest: "2".repeat(64),
            sandbox_provider_id: "rootless-oci-docker-v0".into(),
            sandbox_policy_digest: "e".repeat(64),
            workload_issuer: "https://issuer.example".into(),
            workload_audience: "erf-gate".into(),
            workflow: "ci".into(),
            run_identity: "run-1".into(),
            environment_identity: "linux/amd64".into(),
            semantic_result_digest: [9; 32],
            completeness: rf_gate::Completeness { planned: 1, accounted: if complete { 1 } else { 0 }, failed: 0, missing: if complete { 0 } else { 1 }, observation_errors: 0, complete },
            decision,
            exception_grant_digests: Vec::new(),
            issued_at_epoch_secs: issued,
            expires_at_epoch_secs: expires,
        }
    }

    fn trust(key: &SigningKey) -> TrustStore {
        let mut store = TrustStore::default();
        store.insert_key("gate-key", key.verifying_key()).unwrap();
        store.authorize_role(DEFAULT_GATE_SIGNING_ROLE, "gate-key").unwrap();
        store
    }

    #[test]
    fn maps_three_gate_states_without_neutral_fallback() {
        assert_eq!(PublishedState::from_decision(GateDecision::Pass), PublishedState::Success);
        assert_eq!(PublishedState::from_decision(GateDecision::Fail), PublishedState::Failure);
        assert_eq!(PublishedState::from_decision(GateDecision::Inconclusive), PublishedState::ActionRequired);
    }

    #[test]
    fn verifies_and_repeats_by_attestation_subject_idempotently() {
        let key = SigningKey::from_bytes(&[51; 32]);
        let store = trust(&key);
        let context = StatusPublisherContext::new("org/repo", "abc", "f".repeat(64));
        let mut publisher = StatusPublisher::new(context);
        let gate = attestation("org/repo", "abc", GateDecision::Pass, 10, 20, true);
        let asset = gate.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());

        let first = publisher.publish(&gate, &asset, &store, 19, 1).unwrap();
        assert!(matches!(first, PublishOutcome::Published(ref status) if status.state == PublishedState::Success));
        let retry = publisher.publish(&gate, &asset, &store, 19, 1).unwrap();
        assert!(matches!(retry, PublishOutcome::Idempotent(ref status) if status.state == PublishedState::Success));
    }

    #[test]
    fn refuses_stale_conflicting_and_cross_commit_results() {
        let key = SigningKey::from_bytes(&[52; 32]);
        let store = trust(&key);
        let mut publisher = StatusPublisher::new(StatusPublisherContext::new("org/repo", "abc", "f".repeat(64)));
        let first = attestation("org/repo", "abc", GateDecision::Fail, 10, 30, true);
        let first_asset = first.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        publisher.publish(&first, &first_asset, &store, 20, 2).unwrap();

        let old = attestation("org/repo", "abc", GateDecision::Pass, 11, 30, true);
        let old_asset = old.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        assert_eq!(publisher.publish(&old, &old_asset, &store, 20, 1), Err(StatusPublishError::StaleRun { received: 1, current: 2 }));

        let replay = attestation("org/repo", "abc", GateDecision::Pass, 10, 30, true);
        let replay_asset = replay.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        assert_eq!(publisher.publish(&replay, &replay_asset, &store, 20, 3), Err(StatusPublishError::StaleAttestation { received: 10, current: 10 }));

        let conflict = attestation("org/repo", "abc", GateDecision::Pass, 12, 30, true);
        let conflict_asset = conflict.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        assert_eq!(publisher.publish(&conflict, &conflict_asset, &store, 20, 2), Err(StatusPublishError::SequenceConflict { sequence: 2 }));

        let other_commit = attestation("org/repo", "def", GateDecision::Pass, 13, 30, true);
        let other_asset = other_commit.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        assert_eq!(publisher.publish(&other_commit, &other_asset, &store, 20, 3), Err(StatusPublishError::RepositoryCommitMismatch));
    }

    #[test]
    fn rejects_tampering_expiry_wrong_role_and_incomplete_pass() {
        let key = SigningKey::from_bytes(&[53; 32]);
        let store = trust(&key);
        let context = StatusPublisherContext::new("org/repo", "abc", "f".repeat(64));

        let expired = attestation("org/repo", "abc", GateDecision::Fail, 10, 20, true);
        let expired_asset = expired.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let mut expired_publisher = StatusPublisher::new(context.clone());
        assert!(matches!(expired_publisher.publish(&expired, &expired_asset, &store, 20, 1), Err(StatusPublishError::AttestationInvalid(_))));

        let mut tampered_asset = expired_asset.clone();
        tampered_asset.signature[0] ^= 1;
        let mut tampered_publisher = StatusPublisher::new(context.clone());
        assert!(matches!(tampered_publisher.publish(&expired, &tampered_asset, &store, 19, 1), Err(StatusPublishError::AttestationInvalid(_))));

        let incomplete = attestation("org/repo", "abc", GateDecision::Pass, 10, 20, false);
        let incomplete_asset = incomplete.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let mut incomplete_publisher = StatusPublisher::new(context.with_gate_signing_role("other-role"));
        assert_eq!(incomplete_publisher.publish(&incomplete, &incomplete_asset, &store, 19, 1), Err(StatusPublishError::IncompletePass));

        let mut role_publisher = StatusPublisher::new(StatusPublisherContext::new("org/repo", "abc", "f".repeat(64)).with_gate_signing_role("other-role"));
        let valid_pass = attestation("org/repo", "abc", GateDecision::Pass, 10, 20, true);
        let valid_asset = valid_pass.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        assert!(matches!(role_publisher.publish(&valid_pass, &valid_asset, &store, 19, 1), Err(StatusPublishError::AttestationInvalid(_))));
    }

    #[test]
    fn rejects_invalid_policy_context_before_accepting_evidence() {
        let key = SigningKey::from_bytes(&[54; 32]);
        let store = trust(&key);
        let gate = attestation("org/repo", "abc", GateDecision::Inconclusive, 10, 20, false);
        let asset = gate.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let mut publisher = StatusPublisher::new(StatusPublisherContext::new("org/repo", "abc", "not-a-digest"));
        assert_eq!(publisher.publish(&gate, &asset, &store, 19, 1), Err(StatusPublishError::InvalidContext("policy digest")));
    }

    #[test]
    fn rejects_attestation_for_a_different_policy() {
        let key = SigningKey::from_bytes(&[55; 32]);
        let store = trust(&key);
        let mut gate = attestation("org/repo", "abc", GateDecision::Pass, 10, 30, true);
        gate.policy_digest = "0".repeat(64);
        let asset = gate.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let mut publisher = StatusPublisher::new(StatusPublisherContext::new("org/repo", "abc", "f".repeat(64)));
        assert_eq!(publisher.publish(&gate, &asset, &store, 20, 1), Err(StatusPublishError::PolicyMismatch));
    }

    #[test]
    fn publication_artifact_binds_mapping_freshness_and_external_authority() {
        let key = SigningKey::from_bytes(&[56; 32]);
        let store = trust(&key);
        let mut publisher = StatusPublisher::new(StatusPublisherContext::new("org/repo", "abc", "f".repeat(64)));
        let pass = attestation("org/repo", "abc", GateDecision::Pass, 10, 30, true);
        let pass_asset = pass.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let publication = publisher.publish_artifact(&pass, &pass_asset, &store, 20, 1).unwrap();
        publication.validate().unwrap();
        assert_eq!(publication.decision, "PASS");
        assert_eq!(publication.state, "success");
        assert_eq!(publication.status, "READY_FOR_ADAPTER");
        assert!(publication.external_adapter_required);
        assert!(!publication.scm_written);
        assert!(publication.publication_digest.starts_with("sha256:"));

        let mut adapter = FakeStatusAdapter::default();
        assert!(matches!(adapter.accept(&publication, 20), Ok(AdapterOutcome::Accepted(_))));
        assert!(matches!(adapter.accept(&publication, 20), Ok(AdapterOutcome::Idempotent(_))));
        assert_eq!(
            adapter.accept(&publication, 30),
            Err(StatusAdapterError::NotFresh { now: 30, issued: 10, expires: 30 })
        );

        let mut tampered = publication.clone();
        tampered.commit = "other".into();
        assert_eq!(adapter.accept(&tampered, 20), Err(StatusAdapterError::DigestMismatch));
    }

    #[test]
    fn publication_adapter_maps_fail_and_inconclusive_without_neutral_success() {
        let key = SigningKey::from_bytes(&[57; 32]);
        let store = trust(&key);
        let context = StatusPublisherContext::new("org/repo", "abc", "f".repeat(64));
        let mut publisher = StatusPublisher::new(context);
        let fail = attestation("org/repo", "abc", GateDecision::Fail, 10, 30, true);
        let fail_asset = fail.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let fail_publication = publisher.publish_artifact(&fail, &fail_asset, &store, 20, 1).unwrap();
        assert_eq!(fail_publication.state, "failure");

        let inconclusive = attestation("org/repo", "abc", GateDecision::Inconclusive, 11, 30, false);
        let inconclusive_asset = inconclusive.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let inconclusive_publication = publisher.publish_artifact(&inconclusive, &inconclusive_asset, &store, 20, 2).unwrap();
        assert_eq!(inconclusive_publication.state, "action-required");
        assert_ne!(inconclusive_publication.state, "success");

        let mut adapter = FakeStatusAdapter::default();
        adapter.accept(&fail_publication, 20).unwrap();
        adapter.accept(&inconclusive_publication, 20).unwrap();
        assert_eq!(adapter.latest().unwrap().state, "action-required");

        assert_eq!(
            adapter.accept(&inconclusive_publication, 9),
            Err(StatusAdapterError::NotFresh { now: 9, issued: 11, expires: 30 })
        );
    }

    #[test]
    fn deployment_descriptor_is_minimal_https_scoped_and_replayable() {
        let key = SigningKey::from_bytes(&[58; 32]);
        let store = trust(&key);
        let mut publisher = StatusPublisher::new(StatusPublisherContext::new("org/repo", "abc", "f".repeat(64)));
        let gate = attestation("org/repo", "abc", GateDecision::Pass, 10, 30, true);
        let asset = gate.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let publication = publisher.publish_artifact(&gate, &asset, &store, 20, 1).unwrap();
        let deployment = StatusAdapterDeployment::new(
            "github",
            "https://scm.example.test/ruleops/status",
            "secretref://ci/status-publisher",
            &publication,
            "req-1",
            "idem-1",
        )
        .unwrap();
        deployment.validate_for(&publication, 20).unwrap();
        assert_eq!(deployment.permission_scope, vec!["status:write"]);
        assert!(!deployment.as_json().to_string().contains("BEGIN"));
        let mut closed_publication = publication.as_json();
        closed_publication
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), serde_json::Value::String("rejected".into()));
        assert!(serde_json::from_value::<StatusPublication>(closed_publication).is_err());

        let mut adapter = FakeStatusAdapter::default();
        let first = adapter.deploy(&deployment, &publication, 20).unwrap();
        assert_eq!(first.outcome, "ACCEPTED");
        first.validate().unwrap();
        let replay = adapter.deploy(&deployment, &publication, 20).unwrap();
        assert_eq!(replay.outcome, "IDEMPOTENT");
        assert_eq!(replay.request_id, "req-1");
        assert_eq!(StatusAdapterDeploymentError::UnknownTransport.delivery_status(), StatusAdapterDeliveryStatus::Unknown);
        assert_eq!(StatusAdapterDeploymentError::PublicationMismatch.delivery_status(), StatusAdapterDeliveryStatus::Rejected);

        assert!(matches!(
            StatusAdapterDeployment::new("github", "http://scm.example.test/status", "secretref://ci/status", &publication, "req-2", "idem-2"),
            Err(StatusAdapterDeploymentError::InvalidDeployment(message)) if message.contains("HTTPS")
        ));
        assert!(matches!(
            StatusAdapterDeployment::new("github", "https://scm.example.test/status", "secretref://token=raw", &publication, "req-3", "idem-3"),
            Err(StatusAdapterDeploymentError::InvalidDeployment(message)) if message.contains("secret material")
        ));
        assert!(matches!(
            StatusAdapterDeployment::new("github", "https://scm.example.test/status", "secretref://ci/../status", &publication, "req-4", "idem-4"),
            Err(StatusAdapterDeploymentError::InvalidDeployment(message)) if message.contains("secret material")
        ));

        let mut broad_scope = deployment.clone();
        broad_scope.permission_scope = vec!["repo:admin".into()];
        assert!(matches!(broad_scope.validate(), Err(StatusAdapterDeploymentError::InvalidDeployment(message)) if message.contains("minimal")));
        assert!(matches!(
            adapter.deploy(&deployment, &publication, 30),
            Err(StatusAdapterDeploymentError::PublicationRejected(message)) if message.contains("not fresh")
        ));

        let mut other_publisher = StatusPublisher::new(StatusPublisherContext::new("org/other", "def", "f".repeat(64)));
        let other_gate = attestation("org/other", "def", GateDecision::Pass, 10, 30, true);
        let other_asset = other_gate.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let other_publication = other_publisher.publish_artifact(&other_gate, &other_asset, &store, 20, 1).unwrap();
        assert_eq!(deployment.validate_for(&other_publication, 20), Err(StatusAdapterDeploymentError::PublicationMismatch));
    }
}
