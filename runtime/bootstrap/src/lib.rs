//! Minimal RunnerBootstrap boundary for measured trusted execution.
//!
//! Bootstrap receives immutable artifact bytes from its caller, measures the
//! bytes itself, verifies the signed RunnerRelease statement, and returns the
//! measurement that Runtime may carry into a scan. It has no registry,
//! network, rule parsing, scan execution, or self-reported digest authority.

use erf_trust::{ArtifactStatement, SignedAsset, TrustError, TrustMetadata, TrustStore};
use sha2::{Digest, Sha256};

pub const LINUX_AMD64: &str = "linux/amd64";
pub const RUNNER_RELEASE_ASSET_TYPE: &str = "application/vnd.erf.runner-release.v0";
pub const RUNNER_RELEASE_SIGNING_ROLE: &str = "runner-release-signing";

#[derive(Debug, Clone)]
pub struct RunnerRelease {
    pub version: String,
    pub platform: String,
    pub artifact_statement: ArtifactStatement,
    pub signed_statement: SignedAsset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerMeasurement {
    pub release_version: String,
    pub platform: String,
    pub artifact_digest: [u8; 32],
    pub statement_digest: [u8; 32],
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BootstrapError {
    #[error("runner release version is empty")]
    EmptyReleaseVersion,
    #[error("runner platform mismatch: expected {expected}, got {received}")]
    PlatformMismatch { expected: String, received: String },
    #[error("runner release asset type mismatch: expected {expected}, got {received}")]
    AssetTypeMismatch { expected: String, received: String },
    #[error("runner release signing role mismatch: expected {expected}, got {received}")]
    SigningRoleMismatch { expected: String, received: String },
    #[error("runner compatibility identity mismatch: expected {expected}, got {received}")]
    CompatibilityMismatch { expected: String, received: String },
    #[error("runner artifact digest does not match its ArtifactStatement subject")]
    ArtifactDigestMismatch,
    #[error("runner release statement verification failed: {0}")]
    Statement(#[from] TrustError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerBootstrap {
    pub expected_platform: String,
    pub expected_asset_type: String,
    pub expected_signing_role: String,
    pub expected_compatibility_identity: String,
}

impl Default for RunnerBootstrap {
    fn default() -> Self {
        Self {
            expected_platform: LINUX_AMD64.into(),
            expected_asset_type: RUNNER_RELEASE_ASSET_TYPE.into(),
            expected_signing_role: RUNNER_RELEASE_SIGNING_ROLE.into(),
            expected_compatibility_identity: LINUX_AMD64.into(),
        }
    }
}

impl RunnerBootstrap {
    pub fn verify_and_measure(
        &self,
        release: &RunnerRelease,
        artifact_bytes: &[u8],
        store: &TrustStore,
        metadata: Option<&TrustMetadata>,
        now_epoch_secs: u64,
    ) -> Result<RunnerMeasurement, BootstrapError> {
        if release.version.is_empty() {
            return Err(BootstrapError::EmptyReleaseVersion);
        }
        if release.platform != self.expected_platform {
            return Err(BootstrapError::PlatformMismatch {
                expected: self.expected_platform.clone(),
                received: release.platform.clone(),
            });
        }
        if release.artifact_statement.asset_type != self.expected_asset_type {
            return Err(BootstrapError::AssetTypeMismatch {
                expected: self.expected_asset_type.clone(),
                received: release.artifact_statement.asset_type.clone(),
            });
        }
        if release.artifact_statement.signing_role != self.expected_signing_role {
            return Err(BootstrapError::SigningRoleMismatch {
                expected: self.expected_signing_role.clone(),
                received: release.artifact_statement.signing_role.clone(),
            });
        }
        if release.artifact_statement.compatibility_identity != self.expected_compatibility_identity {
            return Err(BootstrapError::CompatibilityMismatch {
                expected: self.expected_compatibility_identity.clone(),
                received: release.artifact_statement.compatibility_identity.clone(),
            });
        }

        let artifact_digest: [u8; 32] = Sha256::digest(artifact_bytes).into();
        if release.artifact_statement.subject_digest != artifact_digest {
            return Err(BootstrapError::ArtifactDigestMismatch);
        }
        if let Some(metadata) = metadata {
            release.artifact_statement.verify_signed_with_metadata(
                &release.signed_statement,
                store,
                &self.expected_signing_role,
                metadata,
                now_epoch_secs,
                artifact_digest,
            )?;
        } else {
            release.artifact_statement.verify_signed(
                &release.signed_statement,
                store,
                &self.expected_signing_role,
                artifact_digest,
            )?;
        }
        Ok(RunnerMeasurement {
            release_version: release.version.clone(),
            platform: release.platform.clone(),
            artifact_digest,
            statement_digest: release.signed_statement.subject_digest(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::collections::BTreeSet;

    fn release(artifact: &[u8], key: &SigningKey) -> (RunnerRelease, TrustStore) {
        let subject_digest: [u8; 32] = Sha256::digest(artifact).into();
        let statement = ArtifactStatement {
            subject_digest,
            asset_type: RUNNER_RELEASE_ASSET_TYPE.into(),
            schema_version: "runner-release/v0".into(),
            signing_role: RUNNER_RELEASE_SIGNING_ROLE.into(),
            provenance_digest: [2; 32],
            sbom_digest: [3; 32],
            compatibility_identity: LINUX_AMD64.into(),
        };
        let signed_statement = statement.sign_with("runner-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let mut store = TrustStore::default();
        store.insert_key("runner-key", key.verifying_key()).unwrap();
        store.authorize_role(RUNNER_RELEASE_SIGNING_ROLE, "runner-key").unwrap();
        (RunnerRelease { version: "runner-0.1.0".into(), platform: LINUX_AMD64.into(), artifact_statement: statement, signed_statement }, store)
    }

    #[test]
    fn bootstrap_measures_actual_artifact_and_verifies_release() {
        let artifact = b"runner-binary-v1";
        let key = SigningKey::from_bytes(&[21; 32]);
        let (release, store) = release(artifact, &key);
        let measurement = RunnerBootstrap::default().verify_and_measure(&release, artifact, &store, None, 10).unwrap();
        let expected_digest: [u8; 32] = Sha256::digest(artifact).into();
        assert_eq!(measurement.artifact_digest, expected_digest);
        assert_eq!(measurement.release_version, "runner-0.1.0");
        assert_eq!(measurement.platform, LINUX_AMD64);
        assert_eq!(measurement.statement_digest, release.signed_statement.subject_digest());
    }

    #[test]
    fn bootstrap_rejects_tampered_bytes_and_unsupported_platform() {
        let artifact = b"runner-binary-v1";
        let key = SigningKey::from_bytes(&[22; 32]);
        let (mut release, store) = release(artifact, &key);
        assert_eq!(RunnerBootstrap::default().verify_and_measure(&release, b"runner-binary-tampered", &store, None, 10), Err(BootstrapError::ArtifactDigestMismatch));
        release.platform = "linux/arm64".into();
        assert!(matches!(RunnerBootstrap::default().verify_and_measure(&release, artifact, &store, None, 10), Err(BootstrapError::PlatformMismatch { .. })));
    }

    #[test]
    fn bootstrap_rejects_expired_or_revoked_trust_metadata() {
        let artifact = b"runner-binary-v1";
        let key = SigningKey::from_bytes(&[23; 32]);
        let (release, store) = release(artifact, &key);
        let expired = TrustMetadata { version: 1, expires_at_epoch_secs: 10, revoked_key_ids: BTreeSet::new() };
        assert_eq!(RunnerBootstrap::default().verify_and_measure(&release, artifact, &store, Some(&expired), 10), Err(BootstrapError::Statement(TrustError::MetadataExpired)));
        let revoked = TrustMetadata { version: 1, expires_at_epoch_secs: 20, revoked_key_ids: ["runner-key".into()].into_iter().collect() };
        assert_eq!(RunnerBootstrap::default().verify_and_measure(&release, artifact, &store, Some(&revoked), 10), Err(BootstrapError::Statement(TrustError::RevokedKey("runner-key".into()))));
    }

    #[test]
    fn bootstrap_rejects_wrong_release_role_and_asset_type() {
        let artifact = b"runner-binary-v1";
        let key = SigningKey::from_bytes(&[24; 32]);
        let (mut release, store) = release(artifact, &key);
        release.artifact_statement.signing_role = "policy-signing".into();
        assert!(matches!(RunnerBootstrap::default().verify_and_measure(&release, artifact, &store, None, 10), Err(BootstrapError::SigningRoleMismatch { .. })));
        release.artifact_statement.signing_role = RUNNER_RELEASE_SIGNING_ROLE.into();
        release.artifact_statement.asset_type = "application/vnd.erf.policy.v0".into();
        assert!(matches!(RunnerBootstrap::default().verify_and_measure(&release, artifact, &store, None, 10), Err(BootstrapError::AssetTypeMismatch { .. })));
    }
}
