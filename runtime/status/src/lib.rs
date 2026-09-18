//! Credential-isolated Status Publisher core.
//!
//! This crate verifies an already-produced GateAttestation and turns its
//! three-state decision into a transport-neutral SCM status model. It has no
//! SCM client, network access or write credential; an adapter may publish the
//! returned model after this boundary accepts it.

use rf_gate::{GateAttestation, GateDecision};
use erf_trust::{SignedAsset, TrustStore};
use thiserror::Error;

pub const DEFAULT_GATE_SIGNING_ROLE: &str = "gate-signing";

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
    pub run_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishOutcome {
    Published(PublishedStatus),
    Idempotent(PublishedStatus),
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
            run_sequence,
        };
        self.latest = Some(published.clone());
        Ok(PublishOutcome::Published(published))
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
            sandbox_provider_id: "rootless-oci-docker-v0".into(),
            sandbox_policy_digest: "e".repeat(64),
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
}
