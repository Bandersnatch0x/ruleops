//! Signed, immutable composition for the control-plane delivery seam.
//!
//! A RuleBundle does not replace the signatures of its inputs. Each
//! RuleRelease, PolicyProfile, ExecutionLock and OntologyContractPack is
//! verified at its own seam; this module signs the exact set of input
//! digests and compatibility identity that the execution plane is allowed to
//! consume together.

use erf_trust::{SignedAsset, TrustError, TrustMetadata, TrustStore, DSSE_PAYLOAD_TYPE};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const RULE_BUNDLE_SCHEMA_VERSION: &str = "rule-bundle-v0";
pub const RULE_BUNDLE_ASSET_TYPE: &str = "rule-bundle";
pub const RULE_BUNDLE_SIGNING_ROLE: &str = "rule-bundle-signing";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleBundleManifest {
    pub bundle_version: String,
    pub rule_release_digest: [u8; 32],
    pub policy_profile_digest: [u8; 32],
    pub execution_lock_digest: [u8; 32],
    pub ontology_contract_pack_digest: [u8; 32],
    pub compatibility_identity: String,
    pub platform: String,
}

impl RuleBundleManifest {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-rule-bundle-manifest-v0\0".to_vec();
        append_string(&mut payload, &self.bundle_version);
        payload.extend_from_slice(&self.rule_release_digest);
        payload.extend_from_slice(&self.policy_profile_digest);
        payload.extend_from_slice(&self.execution_lock_digest);
        payload.extend_from_slice(&self.ontology_contract_pack_digest);
        append_string(&mut payload, &self.compatibility_identity);
        append_string(&mut payload, &self.platform);
        payload
    }

    pub fn manifest_digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_payload()).into()
    }

    pub fn sign_with(
        &self,
        key_id: impl Into<String>,
        signer: impl FnOnce(&[u8]) -> Vec<u8>,
    ) -> SignedRuleBundle {
        let payload = self.canonical_payload();
        let asset = SignedAsset {
            payload_type: DSSE_PAYLOAD_TYPE.into(),
            signature: signer(&erf_trust::pae(DSSE_PAYLOAD_TYPE, &payload)),
            payload,
            key_id: key_id.into(),
        };
        SignedRuleBundle { manifest: self.clone(), signed_manifest: asset }
    }

    fn validate(&self) -> Result<(), BundleError> {
        for (name, digest) in [
            ("rule_release_digest", self.rule_release_digest),
            ("policy_profile_digest", self.policy_profile_digest),
            ("execution_lock_digest", self.execution_lock_digest),
            ("ontology_contract_pack_digest", self.ontology_contract_pack_digest),
        ] {
            if digest == [0; 32] {
                return Err(BundleError::InvalidManifest(format!("{name} is zero")));
            }
        }
        for (name, value) in [
            ("bundle_version", &self.bundle_version),
            ("compatibility_identity", &self.compatibility_identity),
            ("platform", &self.platform),
        ] {
            if value.is_empty() || value.chars().any(char::is_control) {
                return Err(BundleError::InvalidManifest(format!("{name} is empty or contains control characters")));
            }
        }
        if self.bundle_version != RULE_BUNDLE_SCHEMA_VERSION {
            return Err(BundleError::InvalidManifest(format!(
                "bundle_version must be {RULE_BUNDLE_SCHEMA_VERSION}"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRuleBundle {
    pub manifest: RuleBundleManifest,
    pub signed_manifest: SignedAsset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleBundleInputs {
    pub rule_release_digest: [u8; 32],
    pub policy_profile_digest: [u8; 32],
    pub execution_lock_digest: [u8; 32],
    pub ontology_contract_pack_digest: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleBundleMeasurement {
    pub manifest_digest: [u8; 32],
    pub signed_manifest_digest: [u8; 32],
    pub compatibility_identity: String,
    pub platform: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleBundleVerifier {
    pub expected_compatibility_identity: String,
    pub expected_platform: String,
}

impl RuleBundleVerifier {
    pub fn new(compatibility_identity: impl Into<String>, platform: impl Into<String>) -> Self {
        Self {
            expected_compatibility_identity: compatibility_identity.into(),
            expected_platform: platform.into(),
        }
    }

    pub fn verify_and_measure(
        &self,
        bundle: &SignedRuleBundle,
        manifest_bytes: &[u8],
        inputs: RuleBundleInputs,
        store: &TrustStore,
        metadata: Option<&TrustMetadata>,
        now_epoch_secs: u64,
    ) -> Result<RuleBundleMeasurement, BundleError> {
        bundle.manifest.validate()?;
        if bundle.manifest.compatibility_identity != self.expected_compatibility_identity
            || bundle.manifest.platform != self.expected_platform
        {
            return Err(BundleError::CompatibilityMismatch);
        }
        if bundle.manifest.rule_release_digest != inputs.rule_release_digest {
            return Err(BundleError::InputDigestMismatch("rule_release_digest"));
        }
        if bundle.manifest.policy_profile_digest != inputs.policy_profile_digest {
            return Err(BundleError::InputDigestMismatch("policy_profile_digest"));
        }
        if bundle.manifest.execution_lock_digest != inputs.execution_lock_digest {
            return Err(BundleError::InputDigestMismatch("execution_lock_digest"));
        }
        if bundle.manifest.ontology_contract_pack_digest != inputs.ontology_contract_pack_digest {
            return Err(BundleError::InputDigestMismatch("ontology_contract_pack_digest"));
        }

        let expected_manifest = bundle.manifest.canonical_payload();
        if manifest_bytes != expected_manifest {
            return Err(BundleError::ManifestBytesMismatch);
        }
        if bundle.signed_manifest.payload != expected_manifest {
            return Err(BundleError::SignedManifestMismatch);
        }
        let manifest_digest: [u8; 32] = Sha256::digest(manifest_bytes).into();
        let signed_manifest_digest = match metadata {
            Some(metadata) => store.verify_with_metadata(
                &bundle.signed_manifest,
                RULE_BUNDLE_SIGNING_ROLE,
                metadata,
                now_epoch_secs,
            )?,
            None => store.verify(&bundle.signed_manifest, RULE_BUNDLE_SIGNING_ROLE)?,
        };
        if signed_manifest_digest != manifest_digest {
            return Err(BundleError::SignedManifestSubjectMismatch);
        }
        Ok(RuleBundleMeasurement {
            manifest_digest,
            signed_manifest_digest,
            compatibility_identity: bundle.manifest.compatibility_identity.clone(),
            platform: bundle.manifest.platform.clone(),
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BundleError {
    #[error("invalid RuleBundle manifest: {0}")]
    InvalidManifest(String),
    #[error("RuleBundle compatibility or platform identity does not match the verifier")]
    CompatibilityMismatch,
    #[error("RuleBundle input {0} does not match the supplied verified asset")]
    InputDigestMismatch(&'static str),
    #[error("RuleBundle manifest bytes do not match the canonical manifest")]
    ManifestBytesMismatch,
    #[error("signed RuleBundle manifest payload does not match the canonical manifest")]
    SignedManifestMismatch,
    #[error("signed RuleBundle subject digest does not match the manifest")]
    SignedManifestSubjectMismatch,
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

    const INPUTS: RuleBundleInputs = RuleBundleInputs {
        rule_release_digest: [1; 32],
        policy_profile_digest: [2; 32],
        execution_lock_digest: [3; 32],
        ontology_contract_pack_digest: [4; 32],
    };

    fn fixture() -> (SignedRuleBundle, TrustStore) {
        let key = SigningKey::from_bytes(&[91; 32]);
        let manifest = RuleBundleManifest {
            bundle_version: RULE_BUNDLE_SCHEMA_VERSION.into(),
            rule_release_digest: INPUTS.rule_release_digest,
            policy_profile_digest: INPUTS.policy_profile_digest,
            execution_lock_digest: INPUTS.execution_lock_digest,
            ontology_contract_pack_digest: INPUTS.ontology_contract_pack_digest,
            compatibility_identity: "java21-pmd7".into(),
            platform: "linux/amd64".into(),
        };
        let bundle = manifest.sign_with("bundle-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let mut store = TrustStore::default();
        store.insert_key("bundle-key", key.verifying_key()).unwrap();
        store.authorize_role(RULE_BUNDLE_SIGNING_ROLE, "bundle-key").unwrap();
        (bundle, store)
    }

    #[test]
    fn verified_bundle_binds_all_control_plane_inputs() {
        let (bundle, store) = fixture();
        let manifest = bundle.manifest.canonical_payload();
        let measurement = RuleBundleVerifier::new("java21-pmd7", "linux/amd64")
            .verify_and_measure(&bundle, &manifest, INPUTS, &store, None, 10)
            .unwrap();
        assert_eq!(measurement.manifest_digest, bundle.manifest.manifest_digest());
        assert_eq!(measurement.signed_manifest_digest, measurement.manifest_digest);
    }

    #[test]
    fn verifier_rejects_input_tampering_and_wrong_execution_identity() {
        let (bundle, store) = fixture();
        let manifest = bundle.manifest.canonical_payload();
        let mut tampered = INPUTS;
        tampered.policy_profile_digest = [9; 32];
        assert_eq!(
            RuleBundleVerifier::new("java21-pmd7", "linux/amd64")
                .verify_and_measure(&bundle, &manifest, tampered, &store, None, 10),
            Err(BundleError::InputDigestMismatch("policy_profile_digest"))
        );
        assert_eq!(
            RuleBundleVerifier::new("java17-pmd7", "linux/amd64")
                .verify_and_measure(&bundle, &manifest, INPUTS, &store, None, 10),
            Err(BundleError::CompatibilityMismatch)
        );
    }

    #[test]
    fn verifier_rejects_manifest_mutation_and_revoked_signer() {
        let (bundle, store) = fixture();
        let mut manifest = bundle.manifest.canonical_payload();
        let last = manifest.len() - 1;
        manifest[last] ^= 1;
        assert_eq!(
            RuleBundleVerifier::new("java21-pmd7", "linux/amd64")
                .verify_and_measure(&bundle, &manifest, INPUTS, &store, None, 10),
            Err(BundleError::ManifestBytesMismatch)
        );
        let metadata = TrustMetadata {
            version: 1,
            expires_at_epoch_secs: 20,
            revoked_key_ids: ["bundle-key".into()].into_iter().collect(),
        };
        assert_eq!(
            RuleBundleVerifier::new("java21-pmd7", "linux/amd64")
                .verify_and_measure(&bundle, &bundle.manifest.canonical_payload(), INPUTS, &store, Some(&metadata), 10),
            Err(BundleError::Trust(TrustError::RevokedKey("bundle-key".into())))
        );
    }

    #[test]
    fn verifier_rejects_unknown_bundle_schema_version() {
        let (mut bundle, store) = fixture();
        bundle.manifest.bundle_version = "rule-bundle-v9".into();
        let error = RuleBundleVerifier::new("java21-pmd7", "linux/amd64")
            .verify_and_measure(
                &bundle,
                &bundle.manifest.canonical_payload(),
                INPUTS,
                &store,
                None,
                10,
            )
            .unwrap_err();
        assert!(matches!(error, BundleError::InvalidManifest(message) if message.contains("bundle_version must be")));
    }
}
