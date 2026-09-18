//! In-memory RuleRelease Registry lifecycle core.
//!
//! Admission verifies the complete signed RuleRelease and stores it by its
//! immutable manifest digest. Lifecycle state and audit records are separate
//! from the release bytes: promotion, revocation and rollback selection never
//! rewrite the admitted release. Persistence and SCM/API adapters belong
//! outside this crate.

use erf_release::{RuleReleaseMeasurement, RuleReleaseVerifier, SignedRuleRelease};
use erf_trust::{TrustMetadata, TrustStore};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseState {
    Verified,
    Active,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEntry {
    pub manifest_digest: [u8; 32],
    pub release: SignedRuleRelease,
    pub measurement: RuleReleaseMeasurement,
    pub state: ReleaseState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub audit_sequence: u64,
    pub operation_sequence: u64,
    pub actor: String,
    pub reason: String,
    pub release_digest: [u8; 32],
    pub resulting_state: ReleaseState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryOutcome {
    Admitted(RegistryEntry),
    Idempotent(RegistryEntry),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("registry actor is empty")]
    EmptyActor,
    #[error("registry reason is empty")]
    EmptyReason,
    #[error("RuleRelease verification failed: {0}")]
    Verification(String),
    #[error("a different release is already stored at manifest digest {0}")]
    ConflictingAdmission(String),
    #[error("unknown RuleRelease manifest digest {0}")]
    UnknownRelease(String),
    #[error("invalid lifecycle transition for {digest}: {from:?} -> {to:?}")]
    InvalidTransition { digest: String, from: ReleaseState, to: ReleaseState },
    #[error("release {0} is already active for its RuleIntent")]
    ActiveConflict(String),
    #[error("release {0} is revoked and cannot be selected")]
    RevokedTarget(String),
    #[error("operation sequence {received} is not newer than {current}")]
    StaleOperation { received: u64, current: u64 },
}

pub struct RuleReleaseRegistry {
    verifier: RuleReleaseVerifier,
    trust_store: TrustStore,
    entries: BTreeMap<[u8; 32], RegistryEntry>,
    active_by_intent: BTreeMap<[u8; 32], [u8; 32]>,
    audit: Vec<AuditRecord>,
    next_audit_sequence: u64,
    last_operation_sequence: u64,
}

impl RuleReleaseRegistry {
    pub fn new(verifier: RuleReleaseVerifier, trust_store: TrustStore) -> Self {
        Self {
            verifier,
            trust_store,
            entries: BTreeMap::new(),
            active_by_intent: BTreeMap::new(),
            audit: Vec::new(),
            next_audit_sequence: 1,
            last_operation_sequence: 0,
        }
    }

    pub fn admit(
        &mut self,
        release: SignedRuleRelease,
        manifest_bytes: &[u8],
        artifact_bytes: &[u8],
        metadata: Option<&TrustMetadata>,
        now_epoch_secs: u64,
        actor: &str,
        reason: &str,
        operation_sequence: u64,
    ) -> Result<RegistryOutcome, RegistryError> {
        validate_text(actor, reason)?;
        let measurement = self
            .verifier
            .verify_and_measure(&release, manifest_bytes, artifact_bytes, &self.trust_store, metadata, now_epoch_secs)
            .map_err(|error| RegistryError::Verification(error.to_string()))?;
        let digest = measurement.manifest_digest;
        if let Some(existing) = self.entries.get(&digest) {
            if existing.release == release && existing.measurement == measurement {
                return Ok(RegistryOutcome::Idempotent(existing.clone()));
            }
            return Err(RegistryError::ConflictingAdmission(hex_digest(&digest)));
        }
        self.check_operation_sequence(operation_sequence)?;
        let entry = RegistryEntry { manifest_digest: digest, release, measurement, state: ReleaseState::Verified };
        self.entries.insert(digest, entry.clone());
        self.last_operation_sequence = operation_sequence;
        self.record(operation_sequence, actor, reason, digest, ReleaseState::Verified);
        Ok(RegistryOutcome::Admitted(entry))
    }

    pub fn promote(&mut self, digest: [u8; 32], actor: &str, reason: &str, operation_sequence: u64) -> Result<RegistryEntry, RegistryError> {
        validate_text(actor, reason)?;
        self.check_operation_sequence(operation_sequence)?;
        let (intent_digest, state) = {
            let entry = self.entry(digest)?;
            (entry.release.manifest.intent_digest, entry.state)
        };
        if state != ReleaseState::Verified {
            return Err(RegistryError::InvalidTransition { digest: hex_digest(&digest), from: state, to: ReleaseState::Active });
        }
        if let Some(active) = self.active_by_intent.get(&intent_digest) {
            return Err(RegistryError::ActiveConflict(hex_digest(active)));
        }
        let entry = self.entries.get_mut(&digest).expect("entry checked above");
        entry.state = ReleaseState::Active;
        let result = entry.clone();
        self.active_by_intent.insert(intent_digest, digest);
        self.last_operation_sequence = operation_sequence;
        self.record(operation_sequence, actor, reason, digest, ReleaseState::Active);
        Ok(result)
    }

    pub fn revoke(&mut self, digest: [u8; 32], actor: &str, reason: &str, operation_sequence: u64) -> Result<RegistryEntry, RegistryError> {
        validate_text(actor, reason)?;
        self.check_operation_sequence(operation_sequence)?;
        let (intent_digest, state) = {
            let entry = self.entry(digest)?;
            (entry.release.manifest.intent_digest, entry.state)
        };
        if state == ReleaseState::Revoked {
            return Err(RegistryError::InvalidTransition { digest: hex_digest(&digest), from: state, to: ReleaseState::Revoked });
        }
        let entry = self.entries.get_mut(&digest).expect("entry checked above");
        entry.state = ReleaseState::Revoked;
        if self.active_by_intent.get(&intent_digest) == Some(&digest) {
            self.active_by_intent.remove(&intent_digest);
        }
        let result = entry.clone();
        self.last_operation_sequence = operation_sequence;
        self.record(operation_sequence, actor, reason, digest, ReleaseState::Revoked);
        Ok(result)
    }

    /// Selects an admitted Verified release for an intent and retires the
    /// currently Active release. Both state changes are audited separately;
    /// neither release's signed bytes are modified.
    pub fn rollback(&mut self, intent_digest: [u8; 32], target_digest: [u8; 32], actor: &str, reason: &str, operation_sequence: u64) -> Result<RegistryEntry, RegistryError> {
        validate_text(actor, reason)?;
        self.check_operation_sequence(operation_sequence)?;
        let target_state = self.entry(target_digest)?.state;
        if target_state == ReleaseState::Revoked {
            return Err(RegistryError::RevokedTarget(hex_digest(&target_digest)));
        }
        if target_state != ReleaseState::Verified {
            return Err(RegistryError::InvalidTransition { digest: hex_digest(&target_digest), from: target_state, to: ReleaseState::Active });
        }
        if self.entry(target_digest)?.release.manifest.intent_digest != intent_digest {
            return Err(RegistryError::InvalidTransition { digest: hex_digest(&target_digest), from: target_state, to: ReleaseState::Active });
        }
        let previous = self.active_by_intent.get(&intent_digest).copied();
        if let Some(previous_digest) = previous {
            let previous_entry = self.entries.get_mut(&previous_digest).expect("active index points to entry");
            previous_entry.state = ReleaseState::Revoked;
            self.record(operation_sequence, actor, reason, previous_digest, ReleaseState::Revoked);
        }
        let target = self.entries.get_mut(&target_digest).expect("target checked above");
        target.state = ReleaseState::Active;
        let result = target.clone();
        self.active_by_intent.insert(intent_digest, target_digest);
        self.last_operation_sequence = operation_sequence;
        self.record(operation_sequence, actor, reason, target_digest, ReleaseState::Active);
        Ok(result)
    }

    pub fn entry(&self, digest: [u8; 32]) -> Result<&RegistryEntry, RegistryError> {
        self.entries.get(&digest).ok_or_else(|| RegistryError::UnknownRelease(hex_digest(&digest)))
    }

    pub fn active_for_intent(&self, intent_digest: [u8; 32]) -> Option<&RegistryEntry> {
        self.active_by_intent.get(&intent_digest).and_then(|digest| self.entries.get(digest))
    }

    pub fn audit_log(&self) -> &[AuditRecord] {
        &self.audit
    }

    pub fn last_operation_sequence(&self) -> u64 {
        self.last_operation_sequence
    }

    fn check_operation_sequence(&self, received: u64) -> Result<(), RegistryError> {
        if received <= self.last_operation_sequence {
            return Err(RegistryError::StaleOperation { received, current: self.last_operation_sequence });
        }
        Ok(())
    }

    fn record(&mut self, operation_sequence: u64, actor: &str, reason: &str, digest: [u8; 32], resulting_state: ReleaseState) {
        self.audit.push(AuditRecord { audit_sequence: self.next_audit_sequence, operation_sequence, actor: actor.into(), reason: reason.into(), release_digest: digest, resulting_state });
        self.next_audit_sequence += 1;
    }
}

fn validate_text(actor: &str, reason: &str) -> Result<(), RegistryError> {
    if actor.trim().is_empty() { return Err(RegistryError::EmptyActor); }
    if reason.trim().is_empty() { return Err(RegistryError::EmptyReason); }
    Ok(())
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use erf_release::{RuleReleaseExpectations, RULE_RELEASE_PLATFORM, RULE_RELEASE_SIGNING_ROLE};
    use sha2::{Digest, Sha256};

    const INTENT: [u8; 32] = [1; 32];

    fn fixture(version: &str, artifact: &[u8]) -> (SignedRuleRelease, Vec<u8>, Vec<u8>, TrustStore) {
        let release_key = SigningKey::from_bytes(&[71; 32]);
        let statement_key = SigningKey::from_bytes(&[72; 32]);
        let artifact_digest: [u8; 32] = Sha256::digest(artifact).into();
        let manifest = erf_release::RuleReleaseManifest { release_version: version.into(), intent_digest: INTENT, rule_artifact_digest: artifact_digest, backend_id: "pmd-java".into(), backend_version: "7.9.0".into(), engine_lock_digest: [3; 32], ontology_digest: [4; 32], evidence_digest: [5; 32], sbom_digest: [6; 32], provenance_digest: [7; 32], compatibility_identity: "java21-pmd7".into(), platform: RULE_RELEASE_PLATFORM.into() };
        let release = manifest.sign_with("release-key", |bytes| release_key.sign(bytes).to_bytes().to_vec(), "statement-key", |bytes| statement_key.sign(bytes).to_bytes().to_vec());
        let manifest_bytes = manifest.canonical_payload();
        let mut store = TrustStore::default();
        store.insert_key("release-key", release_key.verifying_key()).unwrap();
        store.insert_key("statement-key", statement_key.verifying_key()).unwrap();
        store.authorize_role(RULE_RELEASE_SIGNING_ROLE, "release-key").unwrap();
        store.authorize_role(RULE_RELEASE_SIGNING_ROLE, "statement-key").unwrap();
        (release, manifest_bytes, artifact.to_vec(), store)
    }

    fn registry(store: TrustStore) -> RuleReleaseRegistry {
        RuleReleaseRegistry::new(RuleReleaseVerifier::new(RuleReleaseExpectations { expected_intent_digest: INTENT, expected_engine_lock_digest: [3; 32], expected_ontology_digest: [4; 32], expected_evidence_digest: [5; 32], expected_sbom_digest: [6; 32], expected_provenance_digest: [7; 32], expected_backend_id: "pmd-java".into(), expected_backend_version: "7.9.0".into(), expected_compatibility_identity: "java21-pmd7".into(), expected_platform: RULE_RELEASE_PLATFORM.into() }), store)
    }

    #[test]
    fn admits_verified_release_by_digest_and_duplicate_is_idempotent() {
        let (release, manifest, artifact, store) = fixture("2026.09.17.1", b"rule-v1");
        let digest: [u8; 32] = Sha256::digest(&manifest).into();
        let mut registry = registry(store);
        let first = registry.admit(release.clone(), &manifest, &artifact, None, 10, "release-bot", "admit", 1).unwrap();
        assert!(matches!(first, RegistryOutcome::Admitted(_)));
        let second = registry.admit(release, &manifest, &artifact, None, 10, "release-bot", "retry", 1).unwrap();
        assert!(matches!(second, RegistryOutcome::Idempotent(_)));
        assert_eq!(registry.entry(digest).unwrap().state, ReleaseState::Verified);
        assert_eq!(registry.audit_log().len(), 1);
    }

    #[test]
    fn promotion_and_revocation_keep_release_bytes_unchanged() {
        let (release, manifest, artifact, store) = fixture("2026.09.17.1", b"rule-v1");
        let original = release.clone();
        let digest: [u8; 32] = Sha256::digest(&manifest).into();
        let mut registry = registry(store);
        registry.admit(release, &manifest, &artifact, None, 10, "release-bot", "admit", 1).unwrap();
        registry.promote(digest, "governance", "activate", 2).unwrap();
        registry.revoke(digest, "security", "revoke", 3).unwrap();
        assert_eq!(registry.entry(digest).unwrap().release, original);
        assert_eq!(registry.entry(digest).unwrap().state, ReleaseState::Revoked);
        assert_eq!(registry.audit_log().len(), 3);
    }

    #[test]
    fn rollback_retires_current_release_and_selects_previous_verified_release() {
        let (first, first_manifest, first_artifact, store) = fixture("2026.09.17.1", b"rule-v1");
        let (second, second_manifest, second_artifact, _) = fixture("2026.09.17.2", b"rule-v2");
        let first_digest: [u8; 32] = Sha256::digest(&first_manifest).into();
        let second_digest: [u8; 32] = Sha256::digest(&second_manifest).into();
        let mut registry = registry(store);
        registry.admit(first, &first_manifest, &first_artifact, None, 10, "release-bot", "admit-v1", 1).unwrap();
        registry.admit(second, &second_manifest, &second_artifact, None, 10, "release-bot", "admit-v2", 2).unwrap();
        registry.promote(first_digest, "governance", "activate-v1", 3).unwrap();
        let selected = registry.rollback(INTENT, second_digest, "governance", "rollback", 4).unwrap();
        assert_eq!(selected.state, ReleaseState::Active);
        assert_eq!(registry.active_for_intent(INTENT).unwrap().manifest_digest, second_digest);
        assert_eq!(registry.entry(first_digest).unwrap().state, ReleaseState::Revoked);
        assert_eq!(registry.audit_log().len(), 5);
    }

    #[test]
    fn rejects_unknown_conflicting_and_stale_lifecycle_operations() {
        let (release, manifest, artifact, store) = fixture("2026.09.17.1", b"rule-v1");
        let digest: [u8; 32] = Sha256::digest(&manifest).into();
        let mut registry = registry(store);
        assert!(matches!(registry.promote(digest, "governance", "unknown", 1), Err(RegistryError::UnknownRelease(_))));
        registry.admit(release, &manifest, &artifact, None, 10, "release-bot", "admit", 1).unwrap();
        registry.promote(digest, "governance", "activate", 2).unwrap();
        assert!(matches!(registry.promote(digest, "governance", "repeat", 3), Err(RegistryError::InvalidTransition { .. })));
        assert!(matches!(registry.revoke(digest, "security", "stale", 2), Err(RegistryError::StaleOperation { .. })));
    }

    #[test]
    fn rejects_tampered_manifest_artifact_and_wrong_backend_before_admission() {
        let (release, manifest, artifact, store) = fixture("2026.09.17.1", b"rule-v1");
        let mut registry = registry(store);
        let mut changed_manifest = manifest.clone();
        changed_manifest[0] ^= 1;
        assert!(matches!(registry.admit(release.clone(), &changed_manifest, &artifact, None, 10, "release-bot", "tamper", 1), Err(RegistryError::Verification(_))));
        assert!(matches!(registry.admit(release.clone(), &manifest, b"tampered", None, 10, "release-bot", "tamper", 1), Err(RegistryError::Verification(_))));
        let mut changed = release.clone();
        changed.manifest.backend_id = "other-backend".into();
        assert!(matches!(registry.admit(changed, &manifest, &artifact, None, 10, "release-bot", "wrong-backend", 1), Err(RegistryError::Verification(_))));
    }

    #[test]
    fn rejects_revoked_rollback_target_and_empty_audit_context() {
        let (first, first_manifest, first_artifact, store) = fixture("2026.09.17.1", b"rule-v1");
        let (second, second_manifest, second_artifact, _) = fixture("2026.09.17.2", b"rule-v2");
        let first_digest: [u8; 32] = Sha256::digest(&first_manifest).into();
        let second_digest: [u8; 32] = Sha256::digest(&second_manifest).into();
        let mut registry = registry(store);
        assert_eq!(registry.admit(first.clone(), &first_manifest, &first_artifact, None, 10, "", "reason", 1), Err(RegistryError::EmptyActor));
        registry.admit(first, &first_manifest, &first_artifact, None, 10, "release-bot", "admit-v1", 1).unwrap();
        registry.admit(second, &second_manifest, &second_artifact, None, 10, "release-bot", "admit-v2", 2).unwrap();
        registry.revoke(second_digest, "security", "revoke", 3).unwrap();
        assert!(matches!(registry.rollback(INTENT, second_digest, "governance", "rollback", 4), Err(RegistryError::RevokedTarget(_))));
        assert_eq!(registry.entry(first_digest).unwrap().release.manifest.release_version, "2026.09.17.1");
    }
}
