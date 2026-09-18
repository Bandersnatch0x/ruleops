//! Signed, immutable RuleRelease verification.
//!
//! A RuleRelease is a governed release manifest, not a Candidate or local
//! EvidenceBundle. Verification binds the manifest and the actual rule
//! artifact bytes to independent DSSE signatures and an ArtifactStatement;
//! there is no registry, tag or network fallback in this crate.

use erf_trust::{ArtifactStatement, SignedAsset, TrustError, TrustMetadata, TrustStore, DSSE_PAYLOAD_TYPE};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const RULE_RELEASE_ASSET_TYPE: &str = "rule-release";
pub const RULE_RELEASE_SCHEMA_VERSION: &str = "rule-release-v0";
pub const RULE_RELEASE_SIGNING_ROLE: &str = "rule-release-signing";
pub const RULE_RELEASE_PLATFORM: &str = "linux/amd64";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleReleaseManifest {
    pub release_version: String,
    pub intent_digest: [u8; 32],
    pub rule_artifact_digest: [u8; 32],
    pub backend_id: String,
    pub backend_version: String,
    pub engine_lock_digest: [u8; 32],
    pub ontology_digest: [u8; 32],
    pub evidence_digest: [u8; 32],
    pub sbom_digest: [u8; 32],
    pub provenance_digest: [u8; 32],
    pub compatibility_identity: String,
    pub platform: String,
}

impl RuleReleaseManifest {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-rule-release-manifest-v0\0".to_vec();
        append_string(&mut payload, &self.release_version);
        for digest in [
            self.intent_digest,
            self.rule_artifact_digest,
            self.engine_lock_digest,
            self.ontology_digest,
            self.evidence_digest,
            self.sbom_digest,
            self.provenance_digest,
        ] {
            payload.extend_from_slice(&digest);
        }
        append_string(&mut payload, &self.backend_id);
        append_string(&mut payload, &self.backend_version);
        append_string(&mut payload, &self.compatibility_identity);
        append_string(&mut payload, &self.platform);
        payload
    }

    pub fn sign_with(
        &self,
        release_key_id: impl Into<String>,
        release_signer: impl FnOnce(&[u8]) -> Vec<u8>,
        statement_key_id: impl Into<String>,
        statement_signer: impl FnOnce(&[u8]) -> Vec<u8>,
    ) -> SignedRuleRelease {
        let manifest_bytes = self.canonical_payload();
        let manifest_digest: [u8; 32] = Sha256::digest(&manifest_bytes).into();
        let signed_manifest = SignedAsset {
            payload_type: DSSE_PAYLOAD_TYPE.into(),
            signature: release_signer(&erf_trust::pae(DSSE_PAYLOAD_TYPE, &manifest_bytes)),
            payload: manifest_bytes,
            key_id: release_key_id.into(),
        };
        let statement = ArtifactStatement {
            subject_digest: manifest_digest,
            asset_type: RULE_RELEASE_ASSET_TYPE.into(),
            schema_version: RULE_RELEASE_SCHEMA_VERSION.into(),
            signing_role: RULE_RELEASE_SIGNING_ROLE.into(),
            provenance_digest: self.provenance_digest,
            sbom_digest: self.sbom_digest,
            compatibility_identity: self.compatibility_identity.clone(),
        };
        let signed_statement = statement.sign_with(statement_key_id, statement_signer);
        SignedRuleRelease { manifest: self.clone(), signed_manifest, statement, signed_statement }
    }

    fn validate(&self) -> Result<(), ReleaseError> {
        for (name, digest) in [
            ("intent_digest", self.intent_digest),
            ("rule_artifact_digest", self.rule_artifact_digest),
            ("engine_lock_digest", self.engine_lock_digest),
            ("ontology_digest", self.ontology_digest),
            ("evidence_digest", self.evidence_digest),
            ("sbom_digest", self.sbom_digest),
            ("provenance_digest", self.provenance_digest),
        ] {
            if digest == [0; 32] {
                return Err(ReleaseError::InvalidManifest(format!("{name} is zero")));
            }
        }
        for (name, value) in [
            ("release_version", &self.release_version),
            ("backend_id", &self.backend_id),
            ("backend_version", &self.backend_version),
            ("compatibility_identity", &self.compatibility_identity),
            ("platform", &self.platform),
        ] {
            if value.is_empty() {
                return Err(ReleaseError::InvalidManifest(format!("{name} is empty")));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRuleRelease {
    pub manifest: RuleReleaseManifest,
    pub signed_manifest: SignedAsset,
    pub statement: ArtifactStatement,
    pub signed_statement: SignedAsset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleReleaseExpectations {
    pub expected_intent_digest: [u8; 32],
    pub expected_engine_lock_digest: [u8; 32],
    pub expected_ontology_digest: [u8; 32],
    pub expected_evidence_digest: [u8; 32],
    pub expected_sbom_digest: [u8; 32],
    pub expected_provenance_digest: [u8; 32],
    pub expected_backend_id: String,
    pub expected_backend_version: String,
    pub expected_compatibility_identity: String,
    pub expected_platform: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleReleaseVerifier {
    pub expected: RuleReleaseExpectations,
}

impl RuleReleaseVerifier {
    pub fn new(expected: RuleReleaseExpectations) -> Self {
        Self { expected }
    }

    pub fn verify_and_measure(
        &self,
        release: &SignedRuleRelease,
        manifest_bytes: &[u8],
        rule_artifact_bytes: &[u8],
        trust_store: &TrustStore,
        metadata: Option<&TrustMetadata>,
        now_epoch_secs: u64,
    ) -> Result<RuleReleaseMeasurement, ReleaseError> {
        release.manifest.validate()?;
        if release.manifest.backend_id != self.expected.expected_backend_id
            || release.manifest.backend_version != self.expected.expected_backend_version
        {
            return Err(ReleaseError::BackendMismatch);
        }
        if release.manifest.intent_digest != self.expected.expected_intent_digest {
            return Err(ReleaseError::InputDigestMismatch("intent_digest"));
        }
        if release.manifest.engine_lock_digest != self.expected.expected_engine_lock_digest {
            return Err(ReleaseError::InputDigestMismatch("engine_lock_digest"));
        }
        if release.manifest.ontology_digest != self.expected.expected_ontology_digest {
            return Err(ReleaseError::OntologyMismatch);
        }
        if release.manifest.evidence_digest != self.expected.expected_evidence_digest {
            return Err(ReleaseError::InputDigestMismatch("evidence_digest"));
        }
        if release.manifest.sbom_digest != self.expected.expected_sbom_digest {
            return Err(ReleaseError::InputDigestMismatch("sbom_digest"));
        }
        if release.manifest.provenance_digest != self.expected.expected_provenance_digest {
            return Err(ReleaseError::InputDigestMismatch("provenance_digest"));
        }
        if release.manifest.compatibility_identity != self.expected.expected_compatibility_identity
            || release.manifest.platform != self.expected.expected_platform
        {
            return Err(ReleaseError::CompatibilityMismatch);
        }
        let expected_manifest = release.manifest.canonical_payload();
        if manifest_bytes != expected_manifest {
            return Err(ReleaseError::ManifestBytesMismatch);
        }
        let manifest_digest: [u8; 32] = Sha256::digest(manifest_bytes).into();
        let artifact_digest: [u8; 32] = Sha256::digest(rule_artifact_bytes).into();
        if artifact_digest != release.manifest.rule_artifact_digest {
            return Err(ReleaseError::ArtifactDigestMismatch);
        }
        if release.signed_manifest.payload != expected_manifest {
            return Err(ReleaseError::SignedManifestMismatch);
        }
        if let Some(metadata) = metadata {
            trust_store.verify_with_metadata(&release.signed_manifest, RULE_RELEASE_SIGNING_ROLE, metadata, now_epoch_secs)?;
        } else {
            trust_store.verify(&release.signed_manifest, RULE_RELEASE_SIGNING_ROLE)?;
        }
        if release.statement.subject_digest != manifest_digest
            || release.statement.asset_type != RULE_RELEASE_ASSET_TYPE
            || release.statement.schema_version != RULE_RELEASE_SCHEMA_VERSION
            || release.statement.signing_role != RULE_RELEASE_SIGNING_ROLE
            || release.statement.provenance_digest != release.manifest.provenance_digest
            || release.statement.sbom_digest != release.manifest.sbom_digest
            || release.statement.compatibility_identity != release.manifest.compatibility_identity
        {
            return Err(ReleaseError::StatementMismatch);
        }
        release.statement.verify_signed(
            &release.signed_statement,
            trust_store,
            RULE_RELEASE_SIGNING_ROLE,
            manifest_digest,
        )?;
        Ok(RuleReleaseMeasurement {
            release_version: release.manifest.release_version.clone(),
            manifest_digest,
            rule_artifact_digest: artifact_digest,
            statement_digest: release.signed_statement.subject_digest(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleReleaseMeasurement {
    pub release_version: String,
    pub manifest_digest: [u8; 32],
    pub rule_artifact_digest: [u8; 32],
    pub statement_digest: [u8; 32],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReleaseError {
    #[error("invalid RuleRelease manifest: {0}")]
    InvalidManifest(String),
    #[error("RuleRelease backend identity does not match the verifier")]
    BackendMismatch,
    #[error("RuleRelease ontology digest does not match the verifier")]
    OntologyMismatch,
    #[error("RuleRelease {0} does not match the verifier")]
    InputDigestMismatch(&'static str),
    #[error("RuleRelease platform or compatibility identity does not match the verifier")]
    CompatibilityMismatch,
    #[error("RuleRelease manifest bytes do not match the canonical manifest")]
    ManifestBytesMismatch,
    #[error("RuleRelease artifact bytes do not match the manifest digest")]
    ArtifactDigestMismatch,
    #[error("signed RuleRelease manifest payload does not match the canonical manifest")]
    SignedManifestMismatch,
    #[error("RuleRelease ArtifactStatement does not match the manifest")]
    StatementMismatch,
    #[error(transparent)]
    Trust(#[from] TrustError),
}

fn append_string(payload: &mut Vec<u8>, value: &str) {
    payload.extend_from_slice(&(value.len() as u64).to_be_bytes());
    payload.extend_from_slice(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::collections::BTreeSet;

    fn manifest(artifact: &[u8]) -> RuleReleaseManifest {
        RuleReleaseManifest {
            release_version: "2026.09.17.1".into(),
            intent_digest: [1; 32],
            rule_artifact_digest: Sha256::digest(artifact).into(),
            backend_id: "pmd-java".into(),
            backend_version: "7.9.0".into(),
            engine_lock_digest: [3; 32],
            ontology_digest: [4; 32],
            evidence_digest: [5; 32],
            sbom_digest: [6; 32],
            provenance_digest: [7; 32],
            compatibility_identity: "java21-pmd7".into(),
            platform: RULE_RELEASE_PLATFORM.into(),
        }
    }

    fn trust(release_key: &SigningKey, statement_key: &SigningKey) -> TrustStore {
        let mut store = TrustStore::default();
        store.insert_key("release-key", release_key.verifying_key()).unwrap();
        store.insert_key("statement-key", statement_key.verifying_key()).unwrap();
        store.authorize_role(RULE_RELEASE_SIGNING_ROLE, "release-key").unwrap();
        store.authorize_role(RULE_RELEASE_SIGNING_ROLE, "statement-key").unwrap();
        store
    }

    fn verifier() -> RuleReleaseVerifier {
        RuleReleaseVerifier::new(RuleReleaseExpectations {
            expected_intent_digest: [1; 32],
            expected_engine_lock_digest: [3; 32],
            expected_ontology_digest: [4; 32],
            expected_evidence_digest: [5; 32],
            expected_sbom_digest: [6; 32],
            expected_provenance_digest: [7; 32],
            expected_backend_id: "pmd-java".into(),
            expected_backend_version: "7.9.0".into(),
            expected_compatibility_identity: "java21-pmd7".into(),
            expected_platform: RULE_RELEASE_PLATFORM.into(),
        })
    }

    #[test]
    fn verifies_actual_manifest_and_artifact_and_repeats_measurement() {
        let artifact = b"compiled-pmd-rule";
        let release_key = SigningKey::from_bytes(&[61; 32]);
        let statement_key = SigningKey::from_bytes(&[62; 32]);
        let manifest = manifest(artifact);
        let release = manifest.sign_with(
            "release-key",
            |bytes| release_key.sign(bytes).to_bytes().to_vec(),
            "statement-key",
            |bytes| statement_key.sign(bytes).to_bytes().to_vec(),
        );
        let store = trust(&release_key, &statement_key);
        let manifest_bytes = manifest.canonical_payload();
        let first = verifier().verify_and_measure(&release, &manifest_bytes, artifact, &store, None, 10).unwrap();
        let second = verifier().verify_and_measure(&release, &manifest_bytes, artifact, &store, None, 10).unwrap();
        assert_eq!(first, second);
        let expected_artifact_digest: [u8; 32] = Sha256::digest(artifact).into();
        assert_eq!(first.rule_artifact_digest, expected_artifact_digest);
    }

    #[test]
    fn rejects_tampered_manifest_artifact_statement_and_roles() {
        let artifact = b"compiled-pmd-rule";
        let release_key = SigningKey::from_bytes(&[63; 32]);
        let statement_key = SigningKey::from_bytes(&[64; 32]);
        let manifest = manifest(artifact);
        let release = manifest.sign_with(
            "release-key",
            |bytes| release_key.sign(bytes).to_bytes().to_vec(),
            "statement-key",
            |bytes| statement_key.sign(bytes).to_bytes().to_vec(),
        );
        let store = trust(&release_key, &statement_key);
        let manifest_bytes = manifest.canonical_payload();
        assert_eq!(verifier().verify_and_measure(&release, b"tampered", artifact, &store, None, 10), Err(ReleaseError::ManifestBytesMismatch));
        assert_eq!(verifier().verify_and_measure(&release, &manifest_bytes, b"other", &store, None, 10), Err(ReleaseError::ArtifactDigestMismatch));

        let mut changed = release.clone();
        changed.manifest.compatibility_identity = "other".into();
        assert_eq!(verifier().verify_and_measure(&changed, &manifest_bytes, artifact, &store, None, 10), Err(ReleaseError::CompatibilityMismatch));

        let mut wrong_input = release.clone();
        wrong_input.manifest.evidence_digest = [8; 32];
        assert_eq!(verifier().verify_and_measure(&wrong_input, &manifest_bytes, artifact, &store, None, 10), Err(ReleaseError::InputDigestMismatch("evidence_digest")));

        let mut wrong_role = release.clone();
        wrong_role.signed_manifest.key_id = "statement-key".into();
        assert!(matches!(verifier().verify_and_measure(&wrong_role, &manifest_bytes, artifact, &store, None, 10), Err(ReleaseError::Trust(TrustError::InvalidSignature | TrustError::UnauthorizedRole { .. }))));
    }

    #[test]
    fn rejects_stale_metadata_and_statement_subject_mismatch() {
        let artifact = b"compiled-pmd-rule";
        let release_key = SigningKey::from_bytes(&[65; 32]);
        let statement_key = SigningKey::from_bytes(&[66; 32]);
        let manifest = manifest(artifact);
        let release = manifest.sign_with(
            "release-key",
            |bytes| release_key.sign(bytes).to_bytes().to_vec(),
            "statement-key",
            |bytes| statement_key.sign(bytes).to_bytes().to_vec(),
        );
        let store = trust(&release_key, &statement_key);
        let metadata = TrustMetadata { version: 1, expires_at_epoch_secs: 20, revoked_key_ids: ["release-key".into()].into_iter().collect() };
        let manifest_bytes = manifest.canonical_payload();
        assert_eq!(verifier().verify_and_measure(&release, &manifest_bytes, artifact, &store, Some(&metadata), 10), Err(ReleaseError::Trust(TrustError::RevokedKey("release-key".into()))));
        let expired = TrustMetadata { revoked_key_ids: BTreeSet::new(), ..metadata };
        assert_eq!(verifier().verify_and_measure(&release, &manifest_bytes, artifact, &store, Some(&expired), 20), Err(ReleaseError::Trust(TrustError::MetadataExpired)));

        let mut changed = release.clone();
        changed.statement.subject_digest = [8; 32];
        assert_eq!(verifier().verify_and_measure(&changed, &manifest_bytes, artifact, &store, None, 10), Err(ReleaseError::StatementMismatch));
    }
}
