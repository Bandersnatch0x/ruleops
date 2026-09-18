//! Signed PolicyProfile and ExceptionGrant governance lifecycle.
//!
//! This crate is deliberately transport-neutral. It verifies supplied signed
//! assets, keeps immutable content-addressed entries, and records lifecycle
//! changes in an append-only audit log. Filesystem, SCM, network, database,
//! and signing-key concerns stay outside this boundary.

use erf_trust::{SignedAsset, TrustMetadata, TrustStore};
use rf_gate::ExceptionGrant;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const POLICY_PROFILE_SIGNING_ROLE: &str = "policy-profile-signing";
pub const EXCEPTION_GRANT_SIGNING_ROLE: &str = rf_gate::EXCEPTION_SIGNING_ROLE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceState {
    Verified,
    Active,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyProfile {
    pub profile_id: String,
    pub version: String,
    pub rule_ids: BTreeSet<String>,
    pub blocking_rule_ids: BTreeSet<String>,
    pub exception_grant_digests: BTreeSet<[u8; 32]>,
    pub require_complete_scan: bool,
}

impl PolicyProfile {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-policy-profile-v0\0".to_vec();
        append_string(&mut payload, &self.profile_id);
        append_string(&mut payload, &self.version);
        append_strings(&mut payload, &self.rule_ids);
        append_strings(&mut payload, &self.blocking_rule_ids);
        payload.extend_from_slice(&(self.exception_grant_digests.len() as u64).to_be_bytes());
        for digest in &self.exception_grant_digests {
            payload.extend_from_slice(digest);
        }
        payload.push(self.require_complete_scan as u8);
        payload
    }

    pub fn sign_with(&self, key_id: impl Into<String>, signer: impl FnOnce(&[u8]) -> Vec<u8>) -> SignedAsset {
        let payload = self.canonical_payload();
        SignedAsset {
            payload_type: erf_trust::DSSE_PAYLOAD_TYPE.into(),
            signature: signer(&erf_trust::pae(erf_trust::DSSE_PAYLOAD_TYPE, &payload)),
            payload,
            key_id: key_id.into(),
        }
    }

    pub fn verify_signed(&self, asset: &SignedAsset, store: &TrustStore) -> Result<[u8; 32], String> {
        self.validate()?;
        if asset.payload != self.canonical_payload() {
            return Err("signed PolicyProfile payload does not match canonical value".into());
        }
        store.verify(asset, POLICY_PROFILE_SIGNING_ROLE).map_err(|error| error.to_string())
    }

    pub fn verify_signed_with_metadata(
        &self,
        asset: &SignedAsset,
        store: &TrustStore,
        metadata: &TrustMetadata,
        now_epoch_secs: u64,
    ) -> Result<[u8; 32], String> {
        self.validate()?;
        if asset.payload != self.canonical_payload() {
            return Err("signed PolicyProfile payload does not match canonical value".into());
        }
        store
            .verify_with_metadata(asset, POLICY_PROFILE_SIGNING_ROLE, metadata, now_epoch_secs)
            .map_err(|error| error.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.profile_id.is_empty() || self.profile_id.chars().any(char::is_control) {
            return Err("PolicyProfile has an invalid profile_id".into());
        }
        if self.version.is_empty() || self.version.chars().any(char::is_control) {
            return Err("PolicyProfile has an invalid version".into());
        }
        if self.rule_ids.is_empty() {
            return Err("PolicyProfile requires at least one rule identity".into());
        }
        if self.blocking_rule_ids.iter().any(|rule| !self.rule_ids.contains(rule)) {
            return Err("PolicyProfile blocking rules must be declared rule identities".into());
        }
        if self.exception_grant_digests.iter().any(|digest| *digest == [0; 32]) {
            return Err("PolicyProfile contains a zero ExceptionGrant digest".into());
        }
        if self.rule_ids.iter().any(|rule| rule.is_empty() || rule.chars().any(char::is_whitespace))
            || self.blocking_rule_ids.iter().any(|rule| rule.is_empty() || rule.chars().any(char::is_whitespace))
        {
            return Err("PolicyProfile contains an invalid rule identity".into());
        }
        Ok(())
    }

    pub fn gate_policy(&self) -> rf_gate::GatePolicy {
        rf_gate::GatePolicy { blocking_rule_ids: self.blocking_rule_ids.clone() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyProfileEntry {
    pub digest: [u8; 32],
    pub profile: PolicyProfile,
    pub signed_asset: SignedAsset,
    pub state: GovernanceState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionGrantEntry {
    pub digest: [u8; 32],
    pub grant: ExceptionGrant,
    pub signed_asset: SignedAsset,
    pub state: GovernanceState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    PolicyProfile,
    ExceptionGrant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub audit_sequence: u64,
    pub operation_sequence: u64,
    pub kind: AssetKind,
    pub actor: String,
    pub reason: String,
    pub asset_digest: [u8; 32],
    pub resulting_state: GovernanceState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionOutcome<T> {
    Admitted(T),
    Idempotent(T),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GovernanceError {
    #[error("governance actor is empty")]
    EmptyActor,
    #[error("governance reason is empty")]
    EmptyReason,
    #[error("governance asset verification failed: {0}")]
    Verification(String),
    #[error("a different asset already occupies digest {0}")]
    ConflictingAdmission(String),
    #[error("a different asset already occupies identity {0}")]
    ConflictingIdentity(String),
    #[error("unknown PolicyProfile digest {0}")]
    UnknownProfile(String),
    #[error("unknown ExceptionGrant digest {0}")]
    UnknownGrant(String),
    #[error("invalid lifecycle transition for {kind:?} {digest}: {from:?} -> {to:?}")]
    InvalidTransition { kind: AssetKind, digest: String, from: GovernanceState, to: GovernanceState },
    #[error("an active PolicyProfile already exists for {0}")]
    ActiveProfileConflict(String),
    #[error("an active ExceptionGrant already exists for grant id {0}")]
    ActiveGrantConflict(String),
    #[error("ExceptionGrant {0} is revoked and cannot be selected")]
    RevokedTarget(String),
    #[error("operation sequence {received} is not newer than {current}")]
    StaleOperation { received: u64, current: u64 },
}

pub struct GovernanceRegistry {
    trust_store: TrustStore,
    profiles: BTreeMap<[u8; 32], PolicyProfileEntry>,
    profile_identity: BTreeMap<(String, String), [u8; 32]>,
    active_profiles: BTreeMap<String, [u8; 32]>,
    grants: BTreeMap<[u8; 32], ExceptionGrantEntry>,
    active_grants: BTreeMap<String, [u8; 32]>,
    audit: Vec<AuditRecord>,
    next_audit_sequence: u64,
    last_operation_sequence: u64,
}

impl GovernanceRegistry {
    pub fn new(trust_store: TrustStore) -> Self {
        Self {
            trust_store,
            profiles: BTreeMap::new(),
            profile_identity: BTreeMap::new(),
            active_profiles: BTreeMap::new(),
            grants: BTreeMap::new(),
            active_grants: BTreeMap::new(),
            audit: Vec::new(),
            next_audit_sequence: 1,
            last_operation_sequence: 0,
        }
    }

    pub fn admit_profile(
        &mut self,
        profile: PolicyProfile,
        signed_asset: SignedAsset,
        metadata: Option<&TrustMetadata>,
        now_epoch_secs: u64,
        actor: &str,
        reason: &str,
        operation_sequence: u64,
    ) -> Result<AdmissionOutcome<PolicyProfileEntry>, GovernanceError> {
        validate_context(actor, reason)?;
        let digest = verify_profile(&profile, &signed_asset, &self.trust_store, metadata, now_epoch_secs)?;
        if let Some(existing) = self.profiles.get(&digest) {
            if existing.profile == profile && existing.signed_asset == signed_asset {
                return Ok(AdmissionOutcome::Idempotent(existing.clone()));
            }
            return Err(GovernanceError::ConflictingAdmission(hex_digest(&digest)));
        }
        let identity = (profile.profile_id.clone(), profile.version.clone());
        if self.profile_identity.contains_key(&identity) {
            return Err(GovernanceError::ConflictingIdentity(format!("{}/{}", identity.0, identity.1)));
        }
        self.check_operation_sequence(operation_sequence)?;
        let entry = PolicyProfileEntry { digest, profile, signed_asset, state: GovernanceState::Verified };
        self.profiles.insert(digest, entry.clone());
        self.profile_identity.insert(identity, digest);
        self.commit_operation(operation_sequence, AssetKind::PolicyProfile, actor, reason, digest, GovernanceState::Verified);
        Ok(AdmissionOutcome::Admitted(entry))
    }

    pub fn admit_grant(
        &mut self,
        grant: ExceptionGrant,
        signed_asset: SignedAsset,
        metadata: Option<&TrustMetadata>,
        now_epoch_secs: u64,
        actor: &str,
        reason: &str,
        operation_sequence: u64,
    ) -> Result<AdmissionOutcome<ExceptionGrantEntry>, GovernanceError> {
        validate_context(actor, reason)?;
        let digest = verify_grant(&grant, &signed_asset, &self.trust_store, metadata, now_epoch_secs)?;
        if let Some(existing) = self.grants.get(&digest) {
            if existing.grant == grant && existing.signed_asset == signed_asset {
                return Ok(AdmissionOutcome::Idempotent(existing.clone()));
            }
            return Err(GovernanceError::ConflictingAdmission(hex_digest(&digest)));
        }
        self.check_operation_sequence(operation_sequence)?;
        let entry = ExceptionGrantEntry { digest, grant: grant.clone(), signed_asset, state: GovernanceState::Verified };
        self.grants.insert(digest, entry.clone());
        self.commit_operation(operation_sequence, AssetKind::ExceptionGrant, actor, reason, digest, GovernanceState::Verified);
        Ok(AdmissionOutcome::Admitted(entry))
    }

    pub fn promote_profile(&mut self, digest: [u8; 32], actor: &str, reason: &str, operation_sequence: u64) -> Result<PolicyProfileEntry, GovernanceError> {
        validate_context(actor, reason)?;
        self.check_operation_sequence(operation_sequence)?;
        let (profile_id, state) = {
            let entry = self.profile(digest)?;
            (entry.profile.profile_id.clone(), entry.state)
        };
        if state != GovernanceState::Verified {
            return Err(invalid_transition(AssetKind::PolicyProfile, digest, state, GovernanceState::Active));
        }
        if self.active_profiles.contains_key(&profile_id) {
            return Err(GovernanceError::ActiveProfileConflict(profile_id));
        }
        let entry = self.profiles.get_mut(&digest).expect("profile checked above");
        entry.state = GovernanceState::Active;
        let result = entry.clone();
        self.active_profiles.insert(result.profile.profile_id.clone(), digest);
        self.commit_operation(operation_sequence, AssetKind::PolicyProfile, actor, reason, digest, GovernanceState::Active);
        Ok(result)
    }

    pub fn revoke_profile(&mut self, digest: [u8; 32], actor: &str, reason: &str, operation_sequence: u64) -> Result<PolicyProfileEntry, GovernanceError> {
        validate_context(actor, reason)?;
        self.check_operation_sequence(operation_sequence)?;
        let (profile_id, state) = {
            let entry = self.profile(digest)?;
            (entry.profile.profile_id.clone(), entry.state)
        };
        if state == GovernanceState::Revoked {
            return Err(invalid_transition(AssetKind::PolicyProfile, digest, state, GovernanceState::Revoked));
        }
        let entry = self.profiles.get_mut(&digest).expect("profile checked above");
        entry.state = GovernanceState::Revoked;
        if self.active_profiles.get(&profile_id) == Some(&digest) {
            self.active_profiles.remove(&profile_id);
        }
        let result = entry.clone();
        self.commit_operation(operation_sequence, AssetKind::PolicyProfile, actor, reason, digest, GovernanceState::Revoked);
        Ok(result)
    }

    pub fn rollback_profile(
        &mut self,
        profile_id: &str,
        target_digest: [u8; 32],
        actor: &str,
        reason: &str,
        operation_sequence: u64,
    ) -> Result<PolicyProfileEntry, GovernanceError> {
        validate_context(actor, reason)?;
        self.check_operation_sequence(operation_sequence)?;
        let target = self.profile(target_digest)?.clone();
        if target.profile.profile_id != profile_id {
            return Err(GovernanceError::ConflictingIdentity(profile_id.into()));
        }
        if target.state == GovernanceState::Revoked {
            return Err(GovernanceError::RevokedTarget(hex_digest(&target_digest)));
        }
        if target.state != GovernanceState::Verified {
            return Err(invalid_transition(AssetKind::PolicyProfile, target_digest, target.state, GovernanceState::Active));
        }
        if let Some(previous_digest) = self.active_profiles.get(profile_id).copied() {
            let previous = self.profiles.get_mut(&previous_digest).expect("active profile index points to entry");
            previous.state = GovernanceState::Revoked;
            self.commit_operation(operation_sequence, AssetKind::PolicyProfile, actor, reason, previous_digest, GovernanceState::Revoked);
        }
        let target = self.profiles.get_mut(&target_digest).expect("target checked above");
        target.state = GovernanceState::Active;
        let result = target.clone();
        self.active_profiles.insert(profile_id.into(), target_digest);
        self.commit_operation(operation_sequence, AssetKind::PolicyProfile, actor, reason, target_digest, GovernanceState::Active);
        Ok(result)
    }

    pub fn promote_grant(&mut self, digest: [u8; 32], now_epoch_secs: u64, actor: &str, reason: &str, operation_sequence: u64) -> Result<ExceptionGrantEntry, GovernanceError> {
        validate_context(actor, reason)?;
        self.check_operation_sequence(operation_sequence)?;
        let (grant_id, state) = {
            let entry = self.grant(digest)?;
            entry.grant.validate_at(now_epoch_secs).map_err(GovernanceError::Verification)?;
            (entry.grant.grant_id.clone(), entry.state)
        };
        if state != GovernanceState::Verified {
            return Err(invalid_transition(AssetKind::ExceptionGrant, digest, state, GovernanceState::Active));
        }
        if self.active_grants.contains_key(&grant_id) {
            return Err(GovernanceError::ActiveGrantConflict(grant_id));
        }
        let entry = self.grants.get_mut(&digest).expect("grant checked above");
        entry.state = GovernanceState::Active;
        let result = entry.clone();
        self.active_grants.insert(result.grant.grant_id.clone(), digest);
        self.commit_operation(operation_sequence, AssetKind::ExceptionGrant, actor, reason, digest, GovernanceState::Active);
        Ok(result)
    }

    pub fn revoke_grant(&mut self, digest: [u8; 32], actor: &str, reason: &str, operation_sequence: u64) -> Result<ExceptionGrantEntry, GovernanceError> {
        validate_context(actor, reason)?;
        self.check_operation_sequence(operation_sequence)?;
        let grant_id = self.grant(digest)?.grant.grant_id.clone();
        let state = self.grants.get(&digest).expect("grant checked above").state;
        if state == GovernanceState::Revoked {
            return Err(invalid_transition(AssetKind::ExceptionGrant, digest, state, GovernanceState::Revoked));
        }
        let entry = self.grants.get_mut(&digest).expect("grant checked above");
        entry.state = GovernanceState::Revoked;
        if self.active_grants.get(&grant_id) == Some(&digest) {
            self.active_grants.remove(&grant_id);
        }
        let result = entry.clone();
        self.commit_operation(operation_sequence, AssetKind::ExceptionGrant, actor, reason, digest, GovernanceState::Revoked);
        Ok(result)
    }

    pub fn rollback_grant(
        &mut self,
        grant_id: &str,
        target_digest: [u8; 32],
        now_epoch_secs: u64,
        actor: &str,
        reason: &str,
        operation_sequence: u64,
    ) -> Result<ExceptionGrantEntry, GovernanceError> {
        validate_context(actor, reason)?;
        self.check_operation_sequence(operation_sequence)?;
        let target = self.grant(target_digest)?.clone();
        if target.grant.grant_id != grant_id {
            return Err(GovernanceError::ConflictingIdentity(grant_id.into()));
        }
        if target.state == GovernanceState::Revoked {
            return Err(GovernanceError::RevokedTarget(hex_digest(&target_digest)));
        }
        target.grant.validate_at(now_epoch_secs).map_err(GovernanceError::Verification)?;
        if target.state != GovernanceState::Verified {
            return Err(invalid_transition(AssetKind::ExceptionGrant, target_digest, target.state, GovernanceState::Active));
        }
        if let Some(previous_digest) = self.active_grants.get(grant_id).copied() {
            let previous = self.grants.get_mut(&previous_digest).expect("active grant index points to entry");
            previous.state = GovernanceState::Revoked;
            self.commit_operation(operation_sequence, AssetKind::ExceptionGrant, actor, reason, previous_digest, GovernanceState::Revoked);
        }
        let target = self.grants.get_mut(&target_digest).expect("target checked above");
        target.state = GovernanceState::Active;
        let result = target.clone();
        self.active_grants.insert(grant_id.into(), target_digest);
        self.commit_operation(operation_sequence, AssetKind::ExceptionGrant, actor, reason, target_digest, GovernanceState::Active);
        Ok(result)
    }

    pub fn profile(&self, digest: [u8; 32]) -> Result<&PolicyProfileEntry, GovernanceError> {
        self.profiles.get(&digest).ok_or_else(|| GovernanceError::UnknownProfile(hex_digest(&digest)))
    }

    pub fn grant(&self, digest: [u8; 32]) -> Result<&ExceptionGrantEntry, GovernanceError> {
        self.grants.get(&digest).ok_or_else(|| GovernanceError::UnknownGrant(hex_digest(&digest)))
    }

    pub fn active_profile(&self, profile_id: &str) -> Option<&PolicyProfileEntry> {
        self.active_profiles.get(profile_id).and_then(|digest| self.profiles.get(digest))
    }

    pub fn active_grant(&self, grant_id: &str) -> Option<&ExceptionGrantEntry> {
        self.active_grants.get(grant_id).and_then(|digest| self.grants.get(digest))
    }

    pub fn audit_log(&self) -> &[AuditRecord] {
        &self.audit
    }

    pub fn last_operation_sequence(&self) -> u64 {
        self.last_operation_sequence
    }

    fn check_operation_sequence(&self, received: u64) -> Result<(), GovernanceError> {
        if received <= self.last_operation_sequence {
            return Err(GovernanceError::StaleOperation { received, current: self.last_operation_sequence });
        }
        Ok(())
    }

    fn commit_operation(&mut self, operation_sequence: u64, kind: AssetKind, actor: &str, reason: &str, digest: [u8; 32], state: GovernanceState) {
        self.last_operation_sequence = operation_sequence;
        self.audit.push(AuditRecord {
            audit_sequence: self.next_audit_sequence,
            operation_sequence,
            kind,
            actor: actor.into(),
            reason: reason.into(),
            asset_digest: digest,
            resulting_state: state,
        });
        self.next_audit_sequence += 1;
    }
}

fn verify_profile(
    profile: &PolicyProfile,
    asset: &SignedAsset,
    store: &TrustStore,
    metadata: Option<&TrustMetadata>,
    now_epoch_secs: u64,
) -> Result<[u8; 32], GovernanceError> {
    metadata
        .map(|metadata| profile.verify_signed_with_metadata(asset, store, metadata, now_epoch_secs))
        .unwrap_or_else(|| profile.verify_signed(asset, store))
        .map_err(GovernanceError::Verification)
}

fn verify_grant(
    grant: &ExceptionGrant,
    asset: &SignedAsset,
    store: &TrustStore,
    metadata: Option<&TrustMetadata>,
    now_epoch_secs: u64,
) -> Result<[u8; 32], GovernanceError> {
    metadata
        .map(|metadata| grant.verify_signed_with_metadata(asset, store, EXCEPTION_GRANT_SIGNING_ROLE, metadata, now_epoch_secs))
        .unwrap_or_else(|| grant.verify_signed(asset, store, EXCEPTION_GRANT_SIGNING_ROLE, now_epoch_secs))
        .map(|_| asset.subject_digest())
        .map_err(GovernanceError::Verification)
}

fn validate_context(actor: &str, reason: &str) -> Result<(), GovernanceError> {
    if actor.trim().is_empty() { return Err(GovernanceError::EmptyActor); }
    if reason.trim().is_empty() { return Err(GovernanceError::EmptyReason); }
    Ok(())
}

fn invalid_transition(kind: AssetKind, digest: [u8; 32], from: GovernanceState, to: GovernanceState) -> GovernanceError {
    GovernanceError::InvalidTransition { kind, digest: hex_digest(&digest), from, to }
}

fn append_string(payload: &mut Vec<u8>, value: &str) {
    payload.extend_from_slice(&(value.len() as u64).to_be_bytes());
    payload.extend_from_slice(value.as_bytes());
}

fn append_strings(payload: &mut Vec<u8>, values: &BTreeSet<String>) {
    payload.extend_from_slice(&(values.len() as u64).to_be_bytes());
    for value in values { append_string(payload, value); }
}

fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn trust() -> (TrustStore, SigningKey, SigningKey) {
        let profile_key = SigningKey::from_bytes(&[61; 32]);
        let grant_key = SigningKey::from_bytes(&[62; 32]);
        let mut store = TrustStore::default();
        store.insert_key("profile-key", profile_key.verifying_key()).unwrap();
        store.insert_key("grant-key", grant_key.verifying_key()).unwrap();
        store.authorize_role(POLICY_PROFILE_SIGNING_ROLE, "profile-key").unwrap();
        store.authorize_role(EXCEPTION_GRANT_SIGNING_ROLE, "grant-key").unwrap();
        (store, profile_key, grant_key)
    }

    fn profile(version: &str) -> PolicyProfile {
        PolicyProfile {
            profile_id: "company-java-backend".into(),
            version: version.into(),
            rule_ids: ["corp.rule".into(), "security.rule".into()].into_iter().collect(),
            blocking_rule_ids: ["corp.rule".into()].into_iter().collect(),
            exception_grant_digests: BTreeSet::new(),
            require_complete_scan: true,
        }
    }

    fn grant(id: &str, effective: u64, expires: u64) -> ExceptionGrant {
        ExceptionGrant {
            grant_id: id.into(),
            repository: "org/repo".into(),
            rule_id: "corp.rule".into(),
            scenario: "direct-system-exit".into(),
            path_glob: "src/**/*.java".into(),
            symbol: Some("org.repo.Bootstrap#shutdown()".into()),
            fingerprint: Some("finding-v0:abc".into()),
            owner: "platform-team".into(),
            reason: "JVM bootstrap requires explicit termination".into(),
            ticket: "RISK-1234".into(),
            approvals: vec!["platform-owner".into(), "security-owner".into()],
            effective_at_epoch_secs: effective,
            expires_at_epoch_secs: expires,
        }
    }

    fn sign_profile(profile: &PolicyProfile, key: &SigningKey) -> SignedAsset {
        profile.sign_with("profile-key", |bytes| key.sign(bytes).to_bytes().to_vec())
    }

    fn sign_grant(grant: &ExceptionGrant, key: &SigningKey) -> SignedAsset {
        grant.sign_with("grant-key", |bytes| key.sign(bytes).to_bytes().to_vec())
    }

    #[test]
    fn profile_admission_is_signed_content_addressed_and_idempotent() {
        let (store, profile_key, _) = trust();
        let profile = profile("2026.09");
        let asset = sign_profile(&profile, &profile_key);
        let digest = asset.subject_digest();
        let mut registry = GovernanceRegistry::new(store);
        let first = registry.admit_profile(profile.clone(), asset.clone(), None, 100, "policy-owner", "initial approval", 1).unwrap();
        assert!(matches!(first, AdmissionOutcome::Admitted(entry) if entry.digest == digest && entry.state == GovernanceState::Verified));
        let second = registry.admit_profile(profile, asset, None, 100, "policy-owner", "replay", 1).unwrap();
        assert!(matches!(second, AdmissionOutcome::Idempotent(entry) if entry.digest == digest));
        assert_eq!(registry.audit_log().len(), 1);
    }

    #[test]
    fn profile_lifecycle_promotes_rolls_back_and_preserves_signed_bytes() {
        let (store, profile_key, _) = trust();
        let old = profile("2026.09");
        let old_asset = sign_profile(&old, &profile_key);
        let old_bytes = old_asset.clone();
        let old_digest = old_asset.subject_digest();
        let new = profile("2026.10");
        let new_asset = sign_profile(&new, &profile_key);
        let new_digest = new_asset.subject_digest();
        let mut registry = GovernanceRegistry::new(store);
        registry.admit_profile(old, old_asset, None, 100, "owner", "admit old", 1).unwrap();
        registry.promote_profile(old_digest, "owner", "activate old", 2).unwrap();
        registry.admit_profile(new, new_asset, None, 100, "owner", "admit new", 3).unwrap();
        registry.rollback_profile("company-java-backend", new_digest, "owner", "rollback", 4).unwrap();
        assert_eq!(registry.active_profile("company-java-backend").unwrap().digest, new_digest);
        assert_eq!(registry.profile(old_digest).unwrap().state, GovernanceState::Revoked);
        assert_eq!(registry.profile(new_digest).unwrap().signed_asset, sign_profile(&profile("2026.10"), &profile_key));
        assert_eq!(old_bytes, registry.profile(old_digest).unwrap().signed_asset);
        assert_eq!(registry.audit_log().len(), 5);
    }

    #[test]
    fn grant_lifecycle_enforces_effective_expiry_and_rollback() {
        let (store, _, grant_key) = trust();
        let first = grant("exception/direct-exit/2026-09", 100, 200);
        let first_asset = sign_grant(&first, &grant_key);
        let first_digest = first_asset.subject_digest();
        let mut registry = GovernanceRegistry::new(store);
        assert!(matches!(registry.admit_grant(first.clone(), first_asset.clone(), None, 99, "owner", "too early", 1), Err(GovernanceError::Verification(message)) if message.contains("not yet effective")));
        registry.admit_grant(first, first_asset.clone(), None, 100, "owner", "approved", 1).unwrap();
        registry.promote_grant(first_digest, 150, "owner", "activate", 2).unwrap();
        let replacement = grant("exception/direct-exit/2026-09", 150, 300);
        let replacement_asset = sign_grant(&replacement, &grant_key);
        let replacement_digest = replacement_asset.subject_digest();
        registry.admit_grant(replacement, replacement_asset.clone(), None, 150, "owner", "prepare rollback", 3).unwrap();
        registry.rollback_grant("exception/direct-exit/2026-09", replacement_digest, 150, "owner", "rollback", 4).unwrap();
        assert_eq!(registry.active_grant("exception/direct-exit/2026-09").unwrap().digest, replacement_digest);
        assert_eq!(registry.grant(first_digest).unwrap().state, GovernanceState::Revoked);
        registry.revoke_grant(replacement_digest, "owner", "withdraw", 5).unwrap();
        assert!(registry.active_grant("exception/direct-exit/2026-09").is_none());
        assert!(matches!(registry.promote_grant(replacement_digest, 300, "owner", "expired", 6), Err(GovernanceError::Verification(message)) if message.contains("expired")));
        assert_eq!(registry.grant(first_digest).unwrap().signed_asset, first_asset);
        assert_eq!(registry.grant(replacement_digest).unwrap().signed_asset, replacement_asset);
    }

    #[test]
    fn rejects_tampering_wrong_role_conflicts_and_stale_operations() {
        let (mut store, profile_key, grant_key) = trust();
        let original = profile("2026.09");
        let asset = sign_profile(&original, &profile_key);
        let mut tampered = asset.clone();
        tampered.payload.push(0);
        let alternate_key = SigningKey::from_bytes(&[63; 32]);
        store.insert_key("profile-alt-key", alternate_key.verifying_key()).unwrap();
        store.authorize_role(POLICY_PROFILE_SIGNING_ROLE, "profile-alt-key").unwrap();
        let mut registry = GovernanceRegistry::new(store.clone());
        assert!(matches!(registry.admit_profile(original.clone(), tampered, None, 100, "owner", "tamper", 1), Err(GovernanceError::Verification(_))));
        let wrong_role = original.sign_with("grant-key", |bytes| grant_key.sign(bytes).to_bytes().to_vec());
        assert!(matches!(registry.admit_profile(original.clone(), wrong_role, None, 100, "owner", "wrong role", 1), Err(GovernanceError::Verification(_))));
        assert!(registry.admit_profile(original.clone(), asset.clone(), None, 100, "owner", "admit", 1).is_ok());
        let alternate_asset = original.sign_with("profile-alt-key", |bytes| alternate_key.sign(bytes).to_bytes().to_vec());
        assert!(matches!(registry.admit_profile(original.clone(), alternate_asset, None, 100, "owner", "same content different signer", 2), Err(GovernanceError::ConflictingAdmission(_))));
        let mut conflict = profile("2026.09");
        conflict.rule_ids.insert("new.rule".into());
        let conflict_asset = sign_profile(&conflict, &profile_key);
        assert!(matches!(registry.admit_profile(conflict, conflict_asset, None, 100, "owner", "conflict", 2), Err(GovernanceError::ConflictingIdentity(_))));
        assert!(matches!(registry.promote_profile(original_profile_digest(&original), "owner", "stale", 1), Err(GovernanceError::StaleOperation { .. })));
    }

    fn original_profile_digest(profile: &PolicyProfile) -> [u8; 32] {
        profile.sign_with("profile-key", |bytes| bytes.to_vec()).subject_digest()
    }

    #[test]
    fn metadata_and_selector_validation_fail_closed() {
        let (store, _, grant_key) = trust();
        let invalid_path = ExceptionGrant { path_glob: "**".into(), ..grant("exception/invalid", 1, 100) };
        let asset = sign_grant(&invalid_path, &grant_key);
        let metadata = TrustMetadata { version: 1, expires_at_epoch_secs: 50, revoked_key_ids: BTreeSet::new() };
        let mut registry = GovernanceRegistry::new(store);
        assert!(matches!(registry.admit_grant(invalid_path, asset, Some(&metadata), 10, "owner", "invalid selector", 1), Err(GovernanceError::Verification(message)) if message.contains("whole repository")));

        let grant = grant("exception/metadata", 1, 100);
        let asset = sign_grant(&grant, &grant_key);
        assert!(matches!(registry.admit_grant(grant, asset, Some(&metadata), 50, "owner", "expired trust", 1), Err(GovernanceError::Verification(message)) if message.contains("expired")));
    }

    #[test]
    fn empty_audit_context_and_revoked_rollback_target_are_rejected() {
        let (store, _, grant_key) = trust();
        let first = grant("exception/rollback", 1, 100);
        let first_asset = sign_grant(&first, &grant_key);
        let digest = first_asset.subject_digest();
        let mut registry = GovernanceRegistry::new(store);
        assert_eq!(registry.admit_grant(first.clone(), first_asset, None, 10, "", "reason", 1), Err(GovernanceError::EmptyActor));
        registry.admit_grant(first, sign_grant(&grant("exception/rollback", 1, 100), &grant_key), None, 10, "owner", "admit", 1).unwrap();
        registry.revoke_grant(digest, "owner", "revoke", 2).unwrap();
        assert!(matches!(registry.rollback_grant("exception/rollback", digest, 10, "owner", "rollback", 3), Err(GovernanceError::RevokedTarget(_))));
        assert!(registry.audit_log().iter().all(|record| !record.actor.is_empty() && !record.reason.is_empty()));
    }
}
