//! Content-addressed identity for executable OCI packs.
//!
//! This boundary verifies manifest bytes supplied by the caller. It does not
//! resolve tags, contact a registry, parse executable layers, or grant the
//! pack execution authority.

use erf_trust::{ArtifactStatement, SignedAsset, TrustError, TrustMetadata, TrustStore};
use sha2::{Digest, Sha256};

pub const LINUX_AMD64: &str = "linux/amd64";
pub const OCI_MANIFEST_MEDIA_TYPE: &str = "application/vnd.oci.image.manifest.v1+json";
pub const BACKEND_RUNTIME_PACK_ASSET_TYPE: &str = "application/vnd.erf.backend-runtime-pack.v0";
pub const WORKSPACE_RESOLVER_PACK_ASSET_TYPE: &str = "application/vnd.erf.workspace-resolver-pack.v0";
pub const RUNTIME_PACK_SIGNING_ROLE: &str = "runtime-pack-signing";

#[derive(Debug, Clone)]
pub struct OciPackRelease {
    pub manifest_media_type: String,
    pub platform: String,
    pub artifact_statement: ArtifactStatement,
    pub signed_statement: SignedAsset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackMeasurement {
    pub asset_type: String,
    pub platform: String,
    pub manifest_digest: [u8; 32],
    pub statement_digest: [u8; 32],
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PackError {
    #[error("OCI manifest media type mismatch: expected {expected}, got {received}")]
    ManifestMediaTypeMismatch { expected: String, received: String },
    #[error("OCI pack platform mismatch: expected {expected}, got {received}")]
    PlatformMismatch { expected: String, received: String },
    #[error("OCI pack asset type mismatch: expected {expected}, got {received}")]
    AssetTypeMismatch { expected: String, received: String },
    #[error("OCI pack signing role mismatch: expected {expected}, got {received}")]
    SigningRoleMismatch { expected: String, received: String },
    #[error("OCI pack compatibility identity mismatch: expected {expected}, got {received}")]
    CompatibilityMismatch { expected: String, received: String },
    #[error("OCI manifest digest does not match its ArtifactStatement subject")]
    ManifestDigestMismatch,
    #[error("OCI pack statement verification failed: {0}")]
    Statement(#[from] TrustError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OciPackVerifier {
    pub expected_manifest_media_type: String,
    pub expected_platform: String,
    pub expected_asset_type: String,
    pub expected_signing_role: String,
    pub expected_compatibility_identity: String,
}

impl OciPackVerifier {
    pub fn backend_runtime() -> Self {
        Self::new(BACKEND_RUNTIME_PACK_ASSET_TYPE)
    }

    pub fn workspace_resolver() -> Self {
        Self::new(WORKSPACE_RESOLVER_PACK_ASSET_TYPE)
    }

    pub fn new(expected_asset_type: impl Into<String>) -> Self {
        Self {
            expected_manifest_media_type: OCI_MANIFEST_MEDIA_TYPE.into(),
            expected_platform: LINUX_AMD64.into(),
            expected_asset_type: expected_asset_type.into(),
            expected_signing_role: RUNTIME_PACK_SIGNING_ROLE.into(),
            expected_compatibility_identity: LINUX_AMD64.into(),
        }
    }

    pub fn with_compatibility_identity(mut self, identity: impl Into<String>) -> Self {
        self.expected_compatibility_identity = identity.into();
        self
    }

    pub fn verify_and_measure(
        &self,
        release: &OciPackRelease,
        manifest_bytes: &[u8],
        store: &TrustStore,
        metadata: Option<&TrustMetadata>,
        now_epoch_secs: u64,
    ) -> Result<PackMeasurement, PackError> {
        if release.manifest_media_type != self.expected_manifest_media_type {
            return Err(PackError::ManifestMediaTypeMismatch {
                expected: self.expected_manifest_media_type.clone(),
                received: release.manifest_media_type.clone(),
            });
        }
        if release.platform != self.expected_platform {
            return Err(PackError::PlatformMismatch {
                expected: self.expected_platform.clone(),
                received: release.platform.clone(),
            });
        }
        if release.artifact_statement.asset_type != self.expected_asset_type {
            return Err(PackError::AssetTypeMismatch {
                expected: self.expected_asset_type.clone(),
                received: release.artifact_statement.asset_type.clone(),
            });
        }
        if release.artifact_statement.signing_role != self.expected_signing_role {
            return Err(PackError::SigningRoleMismatch {
                expected: self.expected_signing_role.clone(),
                received: release.artifact_statement.signing_role.clone(),
            });
        }
        if release.artifact_statement.compatibility_identity != self.expected_compatibility_identity {
            return Err(PackError::CompatibilityMismatch {
                expected: self.expected_compatibility_identity.clone(),
                received: release.artifact_statement.compatibility_identity.clone(),
            });
        }
        let manifest_digest: [u8; 32] = Sha256::digest(manifest_bytes).into();
        if release.artifact_statement.subject_digest != manifest_digest {
            return Err(PackError::ManifestDigestMismatch);
        }
        if let Some(metadata) = metadata {
            release.artifact_statement.verify_signed_with_metadata(
                &release.signed_statement,
                store,
                &self.expected_signing_role,
                metadata,
                now_epoch_secs,
                manifest_digest,
            )?;
        } else {
            release.artifact_statement.verify_signed(
                &release.signed_statement,
                store,
                &self.expected_signing_role,
                manifest_digest,
            )?;
        }
        Ok(PackMeasurement {
            asset_type: release.artifact_statement.asset_type.clone(),
            platform: release.platform.clone(),
            manifest_digest,
            statement_digest: release.signed_statement.subject_digest(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::collections::BTreeSet;

    fn release(manifest: &[u8], key: &SigningKey) -> (OciPackRelease, TrustStore) {
        let subject_digest: [u8; 32] = Sha256::digest(manifest).into();
        let statement = ArtifactStatement {
            subject_digest,
            asset_type: BACKEND_RUNTIME_PACK_ASSET_TYPE.into(),
            schema_version: "backend-runtime-pack/v0".into(),
            signing_role: RUNTIME_PACK_SIGNING_ROLE.into(),
            provenance_digest: [5; 32],
            sbom_digest: [6; 32],
            compatibility_identity: LINUX_AMD64.into(),
        };
        let signed_statement = statement.sign_with("pack-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let mut store = TrustStore::default();
        store.insert_key("pack-key", key.verifying_key()).unwrap();
        store.authorize_role(RUNTIME_PACK_SIGNING_ROLE, "pack-key").unwrap();
        (
            OciPackRelease {
                manifest_media_type: OCI_MANIFEST_MEDIA_TYPE.into(),
                platform: LINUX_AMD64.into(),
                artifact_statement: statement,
                signed_statement,
            },
            store,
        )
    }

    #[test]
    fn verifier_measures_manifest_as_identity() {
        let manifest = br#"{"schemaVersion":2,"config":{"digest":"sha256:config"}}"#;
        let key = SigningKey::from_bytes(&[31; 32]);
        let (release, store) = release(manifest, &key);
        let measurement = OciPackVerifier::backend_runtime().verify_and_measure(&release, manifest, &store, None, 10).unwrap();
        let expected_digest: [u8; 32] = Sha256::digest(manifest).into();
        assert_eq!(measurement.manifest_digest, expected_digest);
        assert_eq!(measurement.asset_type, BACKEND_RUNTIME_PACK_ASSET_TYPE);
        assert_eq!(measurement.platform, LINUX_AMD64);
        assert!(measurement.manifest_digest != [0; 32]);
    }

    #[test]
    fn verifier_rejects_manifest_tamper_and_metadata_revocation() {
        let manifest = br#"{"schemaVersion":2}"#;
        let key = SigningKey::from_bytes(&[32; 32]);
        let (release, store) = release(manifest, &key);
        assert_eq!(OciPackVerifier::backend_runtime().verify_and_measure(&release, br#"{"schemaVersion":1}"#, &store, None, 10), Err(PackError::ManifestDigestMismatch));
        let metadata = TrustMetadata { version: 1, expires_at_epoch_secs: 20, revoked_key_ids: ["pack-key".into()].into_iter().collect() };
        assert_eq!(OciPackVerifier::backend_runtime().verify_and_measure(&release, manifest, &store, Some(&metadata), 10), Err(PackError::Statement(TrustError::RevokedKey("pack-key".into()))));
        let expired = TrustMetadata { revoked_key_ids: BTreeSet::new(), ..metadata };
        assert_eq!(OciPackVerifier::backend_runtime().verify_and_measure(&release, manifest, &store, Some(&expired), 20), Err(PackError::Statement(TrustError::MetadataExpired)));
    }

    #[test]
    fn verifier_rejects_wrong_media_type_platform_type_and_role() {
        let manifest = b"oci-manifest-v1";
        let key = SigningKey::from_bytes(&[33; 32]);
        let (mut release, store) = release(manifest, &key);
        release.manifest_media_type = "application/json".into();
        assert!(matches!(OciPackVerifier::backend_runtime().verify_and_measure(&release, manifest, &store, None, 10), Err(PackError::ManifestMediaTypeMismatch { .. })));
        release.manifest_media_type = OCI_MANIFEST_MEDIA_TYPE.into();
        release.platform = "linux/arm64".into();
        assert!(matches!(OciPackVerifier::backend_runtime().verify_and_measure(&release, manifest, &store, None, 10), Err(PackError::PlatformMismatch { .. })));
        release.platform = LINUX_AMD64.into();
        release.artifact_statement.asset_type = WORKSPACE_RESOLVER_PACK_ASSET_TYPE.into();
        assert!(matches!(OciPackVerifier::backend_runtime().verify_and_measure(&release, manifest, &store, None, 10), Err(PackError::AssetTypeMismatch { .. })));
        release.artifact_statement.asset_type = BACKEND_RUNTIME_PACK_ASSET_TYPE.into();
        release.artifact_statement.signing_role = "policy-signing".into();
        assert!(matches!(OciPackVerifier::backend_runtime().verify_and_measure(&release, manifest, &store, None, 10), Err(PackError::SigningRoleMismatch { .. })));
    }
}
