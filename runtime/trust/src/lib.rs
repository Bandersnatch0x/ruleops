//! Verification boundary for immutable ERF assets.
//!
//! This crate deliberately has no filesystem, network, registry or publishing
//! authority. It verifies a DSSE-style envelope against an explicitly supplied
//! role-scoped TrustStore; callers decide separately whether the verified
//! asset is allowed in a particular execution context.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const DSSE_PAYLOAD_TYPE: &str = "application/vnd.erf.asset.v0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedAsset {
    pub payload_type: String,
    pub payload: Vec<u8>,
    pub key_id: String,
    pub signature: Vec<u8>,
}

impl SignedAsset {
    pub fn subject_digest(&self) -> [u8; 32] {
        Sha256::digest(&self.payload).into()
    }

    pub fn signing_bytes(&self) -> Vec<u8> {
        pae(&self.payload_type, &self.payload)
    }
}

/// Portable, content-bound supply-chain statement for an ERF release asset.
/// The statement is signed separately from the asset and never grants local
/// execution authority by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactStatement {
    pub subject_digest: [u8; 32],
    pub asset_type: String,
    pub schema_version: String,
    pub signing_role: String,
    pub provenance_digest: [u8; 32],
    pub sbom_digest: [u8; 32],
    pub compatibility_identity: String,
}

impl ArtifactStatement {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-artifact-statement-v0\0".to_vec();
        payload.extend_from_slice(&self.subject_digest);
        append_string(&mut payload, &self.asset_type);
        append_string(&mut payload, &self.schema_version);
        append_string(&mut payload, &self.signing_role);
        payload.extend_from_slice(&self.provenance_digest);
        payload.extend_from_slice(&self.sbom_digest);
        append_string(&mut payload, &self.compatibility_identity);
        payload
    }

    pub fn sign_with(&self, key_id: impl Into<String>, signer: impl FnOnce(&[u8]) -> Vec<u8>) -> SignedAsset {
        let payload = self.canonical_payload();
        SignedAsset {
            payload_type: DSSE_PAYLOAD_TYPE.into(),
            payload: payload.clone(),
            key_id: key_id.into(),
            signature: signer(&pae(DSSE_PAYLOAD_TYPE, &payload)),
        }
    }

    pub fn verify_signed(
        &self,
        asset: &SignedAsset,
        store: &TrustStore,
        role: &str,
        artifact_subject_digest: [u8; 32],
    ) -> Result<(), TrustError> {
        self.validate(artifact_subject_digest)?;
        if self.signing_role != role {
            return Err(TrustError::StatementRoleMismatch);
        }
        if asset.payload != self.canonical_payload() {
            return Err(TrustError::InvalidStatementPayload);
        }
        store.verify(asset, role)?;
        Ok(())
    }

    pub fn verify_signed_with_metadata(
        &self,
        asset: &SignedAsset,
        store: &TrustStore,
        role: &str,
        metadata: &TrustMetadata,
        now_epoch_secs: u64,
        artifact_subject_digest: [u8; 32],
    ) -> Result<(), TrustError> {
        self.validate(artifact_subject_digest)?;
        if self.signing_role != role {
            return Err(TrustError::StatementRoleMismatch);
        }
        if asset.payload != self.canonical_payload() {
            return Err(TrustError::InvalidStatementPayload);
        }
        store.verify_with_metadata(asset, role, metadata, now_epoch_secs)?;
        Ok(())
    }

    fn validate(&self, artifact_subject_digest: [u8; 32]) -> Result<(), TrustError> {
        if self.subject_digest == [0; 32] || self.subject_digest != artifact_subject_digest {
            return Err(TrustError::ArtifactSubjectMismatch);
        }
        if self.asset_type.is_empty()
            || self.schema_version.is_empty()
            || self.signing_role.is_empty()
            || self.compatibility_identity.is_empty()
            || self.provenance_digest == [0; 32]
            || self.sbom_digest == [0; 32]
        {
            return Err(TrustError::InvalidArtifactStatement);
        }
        Ok(())
    }
}

fn append_string(payload: &mut Vec<u8>, value: &str) {
    payload.extend_from_slice(&(value.len() as u64).to_be_bytes());
    payload.extend_from_slice(value.as_bytes());
}

#[derive(Debug, Clone, Default)]
pub struct TrustStore {
    keys: BTreeMap<String, VerifyingKey>,
    role_keys: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustMetadata {
    pub version: u64,
    pub expires_at_epoch_secs: u64,
    pub revoked_key_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustCheckpoint {
    pub repository: String,
    pub version: u64,
    pub role_versions: BTreeMap<String, u64>,
    pub snapshot_digest: [u8; 32],
    pub issued_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
}

impl TrustCheckpoint {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-trust-checkpoint-v0\0".to_vec();
        append_string(&mut payload, &self.repository);
        payload.extend_from_slice(&self.version.to_be_bytes());
        payload.extend_from_slice(&(self.role_versions.len() as u64).to_be_bytes());
        for (role, version) in &self.role_versions {
            append_string(&mut payload, role);
            payload.extend_from_slice(&version.to_be_bytes());
        }
        payload.extend_from_slice(&self.snapshot_digest);
        payload.extend_from_slice(&self.issued_at_epoch_secs.to_be_bytes());
        payload.extend_from_slice(&self.expires_at_epoch_secs.to_be_bytes());
        payload
    }

    pub fn sign_with(&self, key_id: impl Into<String>, signer: impl FnOnce(&[u8]) -> Vec<u8>) -> SignedAsset {
        let payload = self.canonical_payload();
        let signing_bytes = pae(DSSE_PAYLOAD_TYPE, &payload);
        SignedAsset { payload_type: DSSE_PAYLOAD_TYPE.into(), payload, key_id: key_id.into(), signature: signer(&signing_bytes) }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustState {
    highest_checkpoint_version: u64,
    repository: Option<String>,
    highest_role_versions: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustStateSnapshot {
    pub highest_checkpoint_version: u64,
    pub repository: Option<String>,
    pub highest_role_versions: BTreeMap<String, u64>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TrustError {
    #[error("payload type is not the ERF asset type")]
    PayloadType,
    #[error("asset key id is empty")]
    EmptyKeyId,
    #[error("unknown key id: {0}")]
    UnknownKey(String),
    #[error("key {key_id} is not authorized for role {role}")]
    UnauthorizedRole { key_id: String, role: String },
    #[error("signature has invalid length")]
    SignatureLength,
    #[error("public key has invalid length")]
    PublicKeyLength,
    #[error("public key is invalid")]
    InvalidPublicKey,
    #[error("signature verification failed")]
    InvalidSignature,
    #[error("artifact statement is invalid")]
    InvalidArtifactStatement,
    #[error("artifact statement payload does not match its canonical form")]
    InvalidStatementPayload,
    #[error("artifact statement signing role does not match the verifier role")]
    StatementRoleMismatch,
    #[error("artifact statement subject digest does not match the supplied artifact")]
    ArtifactSubjectMismatch,
    #[error("trust metadata version must be non-zero")]
    InvalidMetadataVersion,
    #[error("trust metadata is expired")]
    MetadataExpired,
    #[error("signing key is revoked: {0}")]
    RevokedKey(String),
    #[error("trust checkpoint has an invalid version")]
    InvalidCheckpointVersion,
    #[error("trust checkpoint has an invalid repository identity")]
    InvalidCheckpointRepository,
    #[error("trust checkpoint has an invalid role version map")]
    InvalidCheckpointRole,
    #[error("trust checkpoint repository changed from {expected} to {received}")]
    CheckpointRepositoryMismatch { expected: String, received: String },
    #[error("trust checkpoint has an invalid snapshot digest")]
    InvalidCheckpointDigest,
    #[error("trust checkpoint is rolled back from version {highest_seen} to {received}")]
    CheckpointRollback { highest_seen: u64, received: u64 },
    #[error("trust checkpoint role {role} is rolled back from version {highest_seen} to {received}")]
    CheckpointRoleRollback { role: String, highest_seen: u64, received: u64 },
    #[error("trust checkpoint omitted previously tracked role: {0}")]
    CheckpointRoleMissing(String),
    #[error("persisted trust state is invalid")]
    InvalidTrustState,
}

impl TrustStore {
    pub fn insert_key(&mut self, key_id: impl Into<String>, key: VerifyingKey) -> Result<(), TrustError> {
        let key_id = key_id.into();
        if key_id.is_empty() { return Err(TrustError::EmptyKeyId); }
        self.keys.insert(key_id, key);
        Ok(())
    }

    pub fn authorize_role(&mut self, role: impl Into<String>, key_id: impl Into<String>) -> Result<(), TrustError> {
        let role = role.into();
        let key_id = key_id.into();
        if role.is_empty() || key_id.is_empty() { return Err(TrustError::EmptyKeyId); }
        if !self.keys.contains_key(&key_id) { return Err(TrustError::UnknownKey(key_id)); }
        self.role_keys.entry(role).or_default().insert(key_id);
        Ok(())
    }

    pub fn insert_public_key_bytes(&mut self, key_id: impl Into<String>, bytes: &[u8]) -> Result<(), TrustError> {
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| TrustError::PublicKeyLength)?;
        let key = VerifyingKey::from_bytes(&bytes).map_err(|_| TrustError::InvalidPublicKey)?;
        self.insert_key(key_id, key)
    }

    pub fn verify(&self, asset: &SignedAsset, role: &str) -> Result<[u8; 32], TrustError> {
        if asset.payload_type != DSSE_PAYLOAD_TYPE { return Err(TrustError::PayloadType); }
        let key = self.keys.get(&asset.key_id).ok_or_else(|| TrustError::UnknownKey(asset.key_id.clone()))?;
        if !self.role_keys.get(role).map(|keys| keys.contains(&asset.key_id)).unwrap_or(false) {
            return Err(TrustError::UnauthorizedRole { key_id: asset.key_id.clone(), role: role.into() });
        }
        let signature: Signature = asset.signature.as_slice().try_into().map_err(|_| TrustError::SignatureLength)?;
        key.verify(&asset.signing_bytes(), &signature).map_err(|_| TrustError::InvalidSignature)?;
        Ok(asset.subject_digest())
    }

    pub fn verify_with_metadata(
        &self,
        asset: &SignedAsset,
        role: &str,
        metadata: &TrustMetadata,
        now_epoch_secs: u64,
    ) -> Result<[u8; 32], TrustError> {
        if metadata.version == 0 { return Err(TrustError::InvalidMetadataVersion); }
        if now_epoch_secs >= metadata.expires_at_epoch_secs { return Err(TrustError::MetadataExpired); }
        if metadata.revoked_key_ids.contains(&asset.key_id) { return Err(TrustError::RevokedKey(asset.key_id.clone())); }
        self.verify(asset, role)
    }
}

impl TrustState {
    pub fn highest_checkpoint_version(&self) -> u64 { self.highest_checkpoint_version }

    pub fn repository(&self) -> Option<&str> { self.repository.as_deref() }

    pub fn snapshot(&self) -> TrustStateSnapshot {
        TrustStateSnapshot {
            highest_checkpoint_version: self.highest_checkpoint_version,
            repository: self.repository.clone(),
            highest_role_versions: self.highest_role_versions.clone(),
        }
    }

    pub fn from_snapshot(snapshot: TrustStateSnapshot) -> Result<Self, TrustError> {
        if snapshot.highest_checkpoint_version == 0 {
            if snapshot.repository.is_some() || !snapshot.highest_role_versions.is_empty() {
                return Err(TrustError::InvalidTrustState);
            }
        } else if snapshot.repository.as_deref().unwrap_or_default().is_empty() || snapshot.highest_role_versions.is_empty() {
            return Err(TrustError::InvalidTrustState);
        }
        if snapshot.highest_role_versions.iter().any(|(role, version)| role.is_empty() || *version == 0) {
            return Err(TrustError::InvalidTrustState);
        }
        Ok(Self {
            highest_checkpoint_version: snapshot.highest_checkpoint_version,
            repository: snapshot.repository,
            highest_role_versions: snapshot.highest_role_versions,
        })
    }

    pub fn highest_role_version(&self, role: &str) -> Option<u64> {
        self.highest_role_versions.get(role).copied()
    }

    pub fn accept_checkpoint(&mut self, checkpoint: &TrustCheckpoint, now_epoch_secs: u64) -> Result<(), TrustError> {
        if checkpoint.repository.is_empty() { return Err(TrustError::InvalidCheckpointRepository); }
        if checkpoint.version == 0 { return Err(TrustError::InvalidCheckpointVersion); }
        if checkpoint.role_versions.is_empty() || checkpoint.role_versions.iter().any(|(role, version)| role.is_empty() || *version == 0) {
            return Err(TrustError::InvalidCheckpointRole);
        }
        if checkpoint.snapshot_digest == [0; 32] { return Err(TrustError::InvalidCheckpointDigest); }
        if checkpoint.expires_at_epoch_secs <= checkpoint.issued_at_epoch_secs || now_epoch_secs < checkpoint.issued_at_epoch_secs || now_epoch_secs >= checkpoint.expires_at_epoch_secs {
            return Err(TrustError::MetadataExpired);
        }
        if checkpoint.version < self.highest_checkpoint_version {
            return Err(TrustError::CheckpointRollback { highest_seen: self.highest_checkpoint_version, received: checkpoint.version });
        }
        if let Some(repository) = &self.repository {
            if repository != &checkpoint.repository {
                return Err(TrustError::CheckpointRepositoryMismatch { expected: repository.clone(), received: checkpoint.repository.clone() });
            }
        }
        for (role, highest_seen) in &self.highest_role_versions {
            let Some(received) = checkpoint.role_versions.get(role) else {
                return Err(TrustError::CheckpointRoleMissing(role.clone()));
            };
            if received < highest_seen {
                return Err(TrustError::CheckpointRoleRollback { role: role.clone(), highest_seen: *highest_seen, received: *received });
            }
        }
        self.repository = Some(checkpoint.repository.clone());
        self.highest_checkpoint_version = checkpoint.version;
        self.highest_role_versions = checkpoint.role_versions.clone();
        Ok(())
    }

    pub fn accept_signed_checkpoint(
        &mut self,
        checkpoint: &TrustCheckpoint,
        asset: &SignedAsset,
        store: &TrustStore,
        role: &str,
        now_epoch_secs: u64,
    ) -> Result<(), TrustError> {
        if asset.payload != checkpoint.canonical_payload() {
            return Err(TrustError::InvalidSignature);
        }
        store.verify(asset, role)?;
        self.accept_checkpoint(checkpoint, now_epoch_secs)
    }
}

/// DSSE Pre-Auth Encoding: `DSSEv1` plus ASCII decimal lengths separated by
/// spaces, as required for interoperability with standard DSSE verifiers.
pub fn pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = b"DSSEv1 ".to_vec();
    append_field(&mut out, payload_type.as_bytes());
    out.push(b' ');
    append_field(&mut out, payload);
    out
}

fn append_field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(bytes.len().to_string().as_bytes());
    out.push(b' ');
    out.extend_from_slice(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed(payload_type: &str, payload: &[u8], key_id: &str, key: &SigningKey) -> SignedAsset {
        SignedAsset { payload_type: payload_type.into(), payload: payload.into(), key_id: key_id.into(), signature: key.sign(&pae(payload_type, payload)).to_bytes().to_vec() }
    }

    fn store(key_id: &str, key: &SigningKey, role: &str) -> TrustStore {
        let mut store = TrustStore::default();
        store.insert_key(key_id, key.verifying_key()).unwrap();
        store.authorize_role(role, key_id).unwrap();
        store
    }

    fn statement(subject_digest: [u8; 32]) -> ArtifactStatement {
        ArtifactStatement {
            subject_digest,
            asset_type: "application/vnd.erf.backend-runtime-pack.v0".into(),
            schema_version: "backend-runtime-pack/v0".into(),
            signing_role: "runtime-pack-signing".into(),
            provenance_digest: [2; 32],
            sbom_digest: [3; 32],
            compatibility_identity: "linux/amd64;protocol=0".into(),
        }
    }

    #[test]
    fn verifies_role_scoped_dsse_asset_and_subject() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let asset = signed(DSSE_PAYLOAD_TYPE, b"lock-v1", "ci-key", &key);
        let store = store("ci-key", &key, "execution-lock");
        assert_eq!(store.verify(&asset, "execution-lock").unwrap(), asset.subject_digest());
    }

    #[test]
    fn pae_matches_dsse_wire_encoding() {
        assert_eq!(pae("abc", b"xyz"), b"DSSEv1 3 abc 3 xyz");
    }

    #[test]
    fn rejects_payload_tamper_and_wrong_role() {
        let key = SigningKey::from_bytes(&[8; 32]);
        let asset = signed(DSSE_PAYLOAD_TYPE, b"policy-v1", "policy-key", &key);
        let store = store("policy-key", &key, "policy");
        let mut tampered = asset.clone();
        tampered.payload = b"policy-tampered".into();
        assert_eq!(store.verify(&tampered, "policy"), Err(TrustError::InvalidSignature));
        assert!(matches!(store.verify(&asset, "execution-lock"), Err(TrustError::UnauthorizedRole { .. })));
    }

    #[test]
    fn rejects_wrong_type_unknown_key_and_bad_signature() {
        let key = SigningKey::from_bytes(&[9; 32]);
        let mut store = TrustStore::default();
        store.insert_key("key", key.verifying_key()).unwrap();
        store.authorize_role("role", "key").unwrap();
        let wrong_type = signed("application/json", b"asset", "key", &key);
        assert_eq!(store.verify(&wrong_type, "role"), Err(TrustError::PayloadType));
        let unknown = SignedAsset { key_id: "other".into(), ..signed(DSSE_PAYLOAD_TYPE, b"asset", "key", &key) };
        assert_eq!(store.verify(&unknown, "role"), Err(TrustError::UnknownKey("other".into())));
        let bad_sig = SignedAsset { signature: vec![1, 2], ..signed(DSSE_PAYLOAD_TYPE, b"asset", "key", &key) };
        assert_eq!(store.verify(&bad_sig, "role"), Err(TrustError::SignatureLength));
    }

    #[test]
    fn artifact_statement_binds_subject_and_role() {
        let key = SigningKey::from_bytes(&[12; 32]);
        let subject = Sha256::digest(b"runtime-pack").into();
        let statement = statement(subject);
        let asset = statement.sign_with("pack-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let store = store("pack-key", &key, "runtime-pack-signing");
        assert!(statement.verify_signed(&asset, &store, "runtime-pack-signing", subject).is_ok());
        assert_eq!(statement.verify_signed(&asset, &store, "runtime-pack-signing", [4; 32]), Err(TrustError::ArtifactSubjectMismatch));
        assert_eq!(statement.verify_signed(&asset, &store, "execution-lock", subject), Err(TrustError::StatementRoleMismatch));
    }

    #[test]
    fn artifact_statement_rejects_payload_tamper_and_expired_metadata() {
        let key = SigningKey::from_bytes(&[13; 32]);
        let subject = Sha256::digest(b"rule-bundle").into();
        let statement = statement(subject);
        let asset = statement.sign_with("bundle-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let store = store("bundle-key", &key, "runtime-pack-signing");
        let metadata = TrustMetadata { version: 1, expires_at_epoch_secs: 20, revoked_key_ids: BTreeSet::new() };
        assert_eq!(statement.verify_signed_with_metadata(&asset, &store, "runtime-pack-signing", &metadata, 20, subject), Err(TrustError::MetadataExpired));
        let revoked = TrustMetadata { revoked_key_ids: ["bundle-key".into()].into_iter().collect(), ..metadata };
        assert_eq!(statement.verify_signed_with_metadata(&asset, &store, "runtime-pack-signing", &revoked, 19, subject), Err(TrustError::RevokedKey("bundle-key".into())));
        let mut tampered = asset.clone();
        tampered.payload.push(0);
        assert_eq!(statement.verify_signed(&tampered, &store, "runtime-pack-signing", subject), Err(TrustError::InvalidStatementPayload));
    }

    #[test]
    fn artifact_statement_canonical_form_is_deterministic_and_validated() {
        let subject = [1; 32];
        let first = statement(subject).canonical_payload();
        let second = statement(subject).canonical_payload();
        assert_eq!(first, second);
        let mut invalid = statement(subject);
        invalid.sbom_digest = [0; 32];
        let key = SigningKey::from_bytes(&[14; 32]);
        let asset = invalid.sign_with("key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let store = store("key", &key, "runtime-pack-signing");
        assert_eq!(invalid.verify_signed(&asset, &store, "runtime-pack-signing", subject), Err(TrustError::InvalidArtifactStatement));
    }

    #[test]
    fn metadata_expiry_and_revocation_override_valid_signature() {
        let key = SigningKey::from_bytes(&[10; 32]);
        let asset = signed(DSSE_PAYLOAD_TYPE, b"asset", "key", &key);
        let store = store("key", &key, "role");
        let metadata = TrustMetadata { version: 1, expires_at_epoch_secs: 100, revoked_key_ids: BTreeSet::new() };
        assert!(store.verify_with_metadata(&asset, "role", &metadata, 99).is_ok());
        assert_eq!(store.verify_with_metadata(&asset, "role", &metadata, 100), Err(TrustError::MetadataExpired));
        let revoked = TrustMetadata { revoked_key_ids: ["key".into()].into_iter().collect(), ..metadata };
        assert_eq!(store.verify_with_metadata(&asset, "role", &revoked, 1), Err(TrustError::RevokedKey("key".into())));
    }

    #[test]
    fn checkpoint_state_rejects_expiry_and_rollback() {
        let digest = [4; 32];
        let first = TrustCheckpoint { repository: "org/repo".into(), version: 2, role_versions: BTreeMap::from([(String::from("trust-root"), 2), (String::from("runtime-pack-signing"), 3)]), snapshot_digest: digest, issued_at_epoch_secs: 10, expires_at_epoch_secs: 20 };
        let mut state = TrustState::default();
        assert!(state.accept_checkpoint(&first, 11).is_ok());
        assert_eq!(state.highest_checkpoint_version(), 2);
        assert_eq!(state.repository(), Some("org/repo"));
        assert_eq!(state.highest_role_version("runtime-pack-signing"), Some(3));
        let rollback = TrustCheckpoint { version: 1, ..first.clone() };
        assert!(matches!(state.accept_checkpoint(&rollback, 11), Err(TrustError::CheckpointRollback { .. })));
        assert_eq!(state.accept_checkpoint(&first, 20), Err(TrustError::MetadataExpired));
        let role_rollback = TrustCheckpoint { version: 3, role_versions: BTreeMap::from([(String::from("trust-root"), 1), (String::from("runtime-pack-signing"), 4)]), ..first.clone() };
        assert!(matches!(state.accept_checkpoint(&role_rollback, 11), Err(TrustError::CheckpointRoleRollback { .. })));
        let repository_change = TrustCheckpoint { repository: "other/repo".into(), version: 3, role_versions: BTreeMap::from([(String::from("trust-root"), 3), (String::from("runtime-pack-signing"), 4)]), ..first.clone() };
        assert!(matches!(state.accept_checkpoint(&repository_change, 11), Err(TrustError::CheckpointRepositoryMismatch { .. })));
    }

    #[test]
    fn signed_checkpoint_requires_matching_role_and_payload() {
        use ed25519_dalek::{Signer, SigningKey};
        let key = SigningKey::from_bytes(&[11; 32]);
        let checkpoint = TrustCheckpoint { repository: "org/repo".into(), version: 1, role_versions: BTreeMap::from([(String::from("trust-root"), 1)]), snapshot_digest: [5; 32], issued_at_epoch_secs: 10, expires_at_epoch_secs: 20 };
        let asset = checkpoint.sign_with("root-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let mut store = TrustStore::default();
        store.insert_key("root-key", key.verifying_key()).unwrap();
        store.authorize_role("trust-root", "root-key").unwrap();
        let mut state = TrustState::default();
        assert!(state.accept_signed_checkpoint(&checkpoint, &asset, &store, "trust-root", 11).is_ok());
        let mut changed = checkpoint.clone();
        changed.snapshot_digest = [6; 32];
        let mut next = TrustState::default();
        assert_eq!(next.accept_signed_checkpoint(&changed, &asset, &store, "trust-root", 11), Err(TrustError::InvalidSignature));
        assert_eq!(next.highest_checkpoint_version(), 0);
        let mut wrong_role = TrustState::default();
        assert!(matches!(wrong_role.accept_signed_checkpoint(&checkpoint, &asset, &store, "gate-signing", 11), Err(TrustError::UnauthorizedRole { .. })));
    }
}
