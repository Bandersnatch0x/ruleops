//! Verification boundary for immutable ERF assets.
//!
//! This crate deliberately has no filesystem, network, registry or publishing
//! authority. It verifies a DSSE-style envelope against an explicitly supplied
//! role-scoped TrustStore; callers decide separately whether the verified
//! asset is allowed in a particular execution context.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const DSSE_PAYLOAD_TYPE: &str = "application/vnd.erf.asset.v0";
pub const IN_TOTO_STATEMENT_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";
pub const IN_TOTO_STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";
pub const WORKSPACE_PREDICATE_TYPE: &str = "https://rulefactory.io/attestations/workspace/v0";
pub const WORKSPACE_ATTESTATION_SIGNING_ROLE: &str = "workspace-attestation-signing";
/// Payload type reserved for the ADR-0180 Root metadata preparation contract.
///
/// This type is intentionally not consumed by `TrustStore`: the Root metadata
/// verifier below is a pure, offline test seam and does not grant execution or
/// publishing authority until custody and recovery evidence are available.
pub const ROOT_TRUST_METADATA_PAYLOAD_TYPE: &str =
    "application/vnd.erf.root-trust-metadata.v1";
pub const ROOT_THRESHOLD: usize = 2;
pub const ROOT_KEY_COUNT: usize = 3;
pub const MAX_ROOT_SIGNATURES: usize = 16;
/// A rotation may carry one previous delegated key while the new key is
/// distributed.  Keeping this bound conservative avoids turning overlap into
/// an unbounded alternate trust path; changing it requires a new contract.
pub const MAX_DELEGATED_ROTATION_OVERLAP_KEYS: usize = 1;
const MAX_DSSE_ENCODED_FIELD_CHARS: usize = 64 * 1024 * 1024;
const MAX_DSSE_SIGNATURES: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedAsset {
    pub payload_type: String,
    pub payload: Vec<u8>,
    pub key_id: String,
    pub signature: Vec<u8>,
}

/// Standard DSSE envelope representation for one-signature ERF assets.
///
/// `SignedAsset` remains the typed in-process form used by the runtime. This
/// value is the portable JSON boundary: payload bytes and signatures use the
/// DSSE-required base64 encoding and the envelope carries the standard
/// `payloadType`, `payload` and `signatures[]` fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DsseEnvelope {
    pub payload_type: String,
    pub payload: Vec<u8>,
    pub key_id: String,
    pub signature: Vec<u8>,
}

impl SignedAsset {
    pub fn to_dsse_envelope(&self) -> DsseEnvelope {
        DsseEnvelope {
            payload_type: self.payload_type.clone(),
            payload: self.payload.clone(),
            key_id: self.key_id.clone(),
            signature: self.signature.clone(),
        }
    }

    pub fn to_dsse_json(&self) -> Value {
        json!({
            "payloadType": self.payload_type,
            "payload": base64_encode(&self.payload),
            "signatures": [{
                "keyid": self.key_id,
                "sig": base64_encode(&self.signature),
            }],
        })
    }

    pub fn from_dsse_json(value: &Value) -> Result<Self, String> {
        let object = value
            .as_object()
            .ok_or_else(|| "DSSE envelope must be an object".to_string())?;
        if object.keys().any(|key| !matches!(key.as_str(), "payloadType" | "payload" | "signatures")) {
            return Err("DSSE envelope contains an unknown field".into());
        }
        let payload_type = object
            .get("payloadType")
            .and_then(Value::as_str)
            .ok_or_else(|| "DSSE envelope payloadType is missing".to_string())?
            .to_string();
        let encoded_payload = object
            .get("payload")
            .and_then(Value::as_str)
            .ok_or_else(|| "DSSE envelope payload is missing".to_string())?;
        if encoded_payload.len() > MAX_DSSE_ENCODED_FIELD_CHARS {
            return Err("DSSE envelope payload exceeds the configured size limit".into());
        }
        let payload = base64_decode(
            encoded_payload,
        )?;
        let signatures = object
            .get("signatures")
            .and_then(Value::as_array)
            .ok_or_else(|| "DSSE envelope signatures is missing".to_string())?;
        if signatures.is_empty() || signatures.len() > MAX_DSSE_SIGNATURES {
            return Err("ERF portable DSSE assets require exactly one signature".into());
        }
        if signatures.len() != 1 {
            return Err("ERF portable DSSE assets require exactly one signature".into());
        }
        let signature = signatures[0]
            .as_object()
            .ok_or_else(|| "DSSE signature must be an object".to_string())?;
        if signature.keys().any(|key| !matches!(key.as_str(), "keyid" | "sig")) {
            return Err("DSSE signature contains an unknown field".into());
        }
        let key_id = signature
            .get("keyid")
            .and_then(Value::as_str)
            .ok_or_else(|| "DSSE signature keyid is missing".to_string())?
            .to_string();
        let encoded_signature = signature
            .get("sig")
            .and_then(Value::as_str)
            .ok_or_else(|| "DSSE signature sig is missing".to_string())?;
        if encoded_signature.len() > MAX_DSSE_ENCODED_FIELD_CHARS {
            return Err("DSSE signature exceeds the configured size limit".into());
        }
        let signature = base64_decode(
            encoded_signature,
        )?;
        if payload_type.is_empty() || key_id.is_empty() || signature.is_empty() {
            return Err("DSSE envelope contains an empty identity or signature".into());
        }
        Ok(Self {
            payload_type,
            payload,
            key_id,
            signature,
        })
    }
}

impl DsseEnvelope {
    pub fn into_signed_asset(self) -> SignedAsset {
        SignedAsset {
            payload_type: self.payload_type,
            payload: self.payload,
            key_id: self.key_id,
            signature: self.signature,
        }
    }
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0] as u32;
        let second = chunk.get(1).copied().unwrap_or(0) as u32;
        let third = chunk.get(2).copied().unwrap_or(0) as u32;
        let value = (first << 16) | (second << 8) | third;
        output.push(BASE64_ALPHABET[((value >> 18) & 0x3f) as usize] as char);
        output.push(BASE64_ALPHABET[((value >> 12) & 0x3f) as usize] as char);
        output.push(if chunk.len() > 1 {
            BASE64_ALPHABET[((value >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            BASE64_ALPHABET[(value & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn base64_decode(value: &str) -> Result<Vec<u8>, String> {
    if value.is_empty() || value.len() % 4 != 0 {
        return Err("invalid base64 length".into());
    }
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(value.len() / 4 * 3);
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let last = index == bytes.len() / 4 - 1;
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c = if chunk[2] == b'=' { 0 } else { base64_value(chunk[2])? };
        let d = if chunk[3] == b'=' { 0 } else { base64_value(chunk[3])? };
        if chunk[0] == b'=' || chunk[1] == b'=' || (!last && (chunk[2] == b'=' || chunk[3] == b'=')) {
            return Err("invalid base64 padding".into());
        }
        if chunk[2] == b'=' && chunk[3] != b'=' {
            return Err("invalid base64 padding".into());
        }
        let value = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | d as u32;
        output.push((value >> 16) as u8);
        if chunk[2] != b'=' {
            output.push((value >> 8) as u8);
        }
        if chunk[3] != b'=' {
            output.push(value as u8);
        }
    }
    Ok(output)
}

fn base64_value(byte: u8) -> Result<u8, String> {
    BASE64_ALPHABET
        .iter()
        .position(|candidate| *candidate == byte)
        .map(|value| value as u8)
        .ok_or_else(|| "invalid base64 character".into())
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
        let statement = json!({
            "_type": IN_TOTO_STATEMENT_TYPE,
            "subject": [{
                "name": self.asset_type,
                "digest": { "sha256": hex_digest(&self.subject_digest) },
            }],
            "predicateType": "https://rulefactory.io/attestations/artifact/v0",
            "predicate": {
                "schemaVersion": self.schema_version,
                "signingRole": self.signing_role,
                "provenanceDigest": hex_digest(&self.provenance_digest),
                "sbomDigest": hex_digest(&self.sbom_digest),
                "compatibilityIdentity": self.compatibility_identity,
            },
        });
        serde_json::to_vec(&statement).expect("ArtifactStatement is JSON-serializable")
    }

    pub fn sign_with(&self, key_id: impl Into<String>, signer: impl FnOnce(&[u8]) -> Vec<u8>) -> SignedAsset {
        let payload = self.canonical_payload();
        SignedAsset {
            payload_type: IN_TOTO_STATEMENT_PAYLOAD_TYPE.into(),
            payload: payload.clone(),
            key_id: key_id.into(),
            signature: signer(&pae(IN_TOTO_STATEMENT_PAYLOAD_TYPE, &payload)),
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
        store.verify_with_payload_type(asset, role, IN_TOTO_STATEMENT_PAYLOAD_TYPE)?;
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
        store.verify_with_metadata_and_payload_type(
            asset,
            role,
            metadata,
            now_epoch_secs,
            IN_TOTO_STATEMENT_PAYLOAD_TYPE,
        )?;
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

/// A material bound by the ERF Workspace predicate. `digest` is intentionally
/// kept as an opaque ERF value here: repository and execution-lock identities
/// are already content-addressed outside this crate, while the in-toto
/// Statement still carries them in the standard material shape.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkspaceMaterial {
    pub name: String,
    pub digest: String,
}

/// DSSE-wrapped in-toto Statement for an ATTESTED_BUILD_EXPORT. The
/// WorkspaceDescriptor digest is the Statement subject; the ERF predicate
/// binds repository/workload context, execution lock, Resolver Pack, toolchain,
/// optional config/lockfile materials and build outputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceAttestation {
    pub subject_digest: [u8; 32],
    pub repository: String,
    pub reference: String,
    pub commit_tree_digest: String,
    pub execution_lock_digest: String,
    pub resolver_pack_digest: String,
    pub toolchain_identity: String,
    pub materials: Vec<WorkspaceMaterial>,
    pub build_output_digests: Vec<[u8; 32]>,
    pub workload_issuer: String,
    pub workload_audience: String,
    pub workflow: String,
    pub run_identity: String,
    pub environment_identity: String,
    pub issued_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
}

impl WorkspaceAttestation {
    /// Returns the actual in-toto Statement carried as the DSSE payload.
    /// `serde_json`'s default object ordering is deterministic, while arrays
    /// are sorted below before serialization, so the signed bytes are stable
    /// across repeated builds.
    pub fn statement_payload(&self) -> serde_json::Value {
        let mut materials = self.materials.clone();
        materials.sort();
        let mut build_output_digests = self.build_output_digests.clone();
        build_output_digests.sort();
        let build_outputs = build_output_digests
            .iter()
            .map(|digest| {
                json!({
                    "name": "erf://workspace/build-output",
                    "digest": { "sha256": hex_digest(digest) },
                })
            })
            .collect::<Vec<_>>();
        let mut predicate_materials = vec![
            json!({
                "name": format!("git+{}@{}", self.repository, self.reference),
                "digest": { "erf": self.commit_tree_digest },
            }),
            json!({
                "name": "erf://execution-lock",
                "digest": { "erf": self.execution_lock_digest },
            }),
            json!({
                "name": "erf://resolver-pack",
                "digest": { "erf": self.resolver_pack_digest },
            }),
            json!({
                "name": "erf://toolchain",
                "digest": { "erf": self.toolchain_identity },
            }),
        ];
        predicate_materials.extend(materials.into_iter().map(|material| {
            json!({
                "name": material.name,
                "digest": { "erf": material.digest },
            })
        }));
        json!({
            "_type": IN_TOTO_STATEMENT_TYPE,
            "subject": [{
                "name": "erf://workspace-descriptor",
                "digest": { "sha256": hex_digest(&self.subject_digest) },
            }],
            "predicateType": WORKSPACE_PREDICATE_TYPE,
            "predicate": {
                "schemaVersion": "workspace/v0",
                "repository": self.repository,
                "reference": self.reference,
                "commitTreeDigest": self.commit_tree_digest,
                "executionLockDigest": self.execution_lock_digest,
                "resolverPackDigest": self.resolver_pack_digest,
                "toolchainIdentity": self.toolchain_identity,
                "materials": predicate_materials,
                "results": build_outputs,
                "invocation": {
                    "workloadIssuer": self.workload_issuer,
                    "workloadAudience": self.workload_audience,
                    "workflow": self.workflow,
                    "runIdentity": self.run_identity,
                    "environment": self.environment_identity,
                },
                "metadata": {
                    "issuedAtEpochSecs": self.issued_at_epoch_secs,
                    "expiresAtEpochSecs": self.expires_at_epoch_secs,
                },
            },
        })
    }

    /// Complete verifier-owned context for an ATTESTED_BUILD_EXPORT. Every
    /// identity is mandatory so a valid signature from another repository,
    /// ref, workflow or run cannot be replayed in the current scan.
    pub fn verify_signed_with_context(
        &self,
        asset: &SignedAsset,
        store: &TrustStore,
        metadata: Option<&TrustMetadata>,
        now_epoch_secs: u64,
        expected: &WorkspaceAttestationExpectations<'_>,
    ) -> Result<(), TrustError> {
        self.validate(now_epoch_secs, expected.subject_digest)?;
        for (actual, expected_value, label) in [
            (&self.repository, expected.repository, "repository"),
            (&self.reference, expected.reference, "reference"),
            (&self.commit_tree_digest, expected.commit_tree_digest, "commit/tree digest"),
            (&self.execution_lock_digest, expected.execution_lock_digest, "execution lock digest"),
            (&self.resolver_pack_digest, expected.resolver_pack_digest, "resolver Pack digest"),
            (&self.toolchain_identity, expected.toolchain_identity, "toolchain identity"),
            (&self.workload_issuer, expected.workload_issuer, "workload issuer"),
            (&self.workload_audience, expected.workload_audience, "workload audience"),
            (&self.workflow, expected.workflow, "workflow"),
            (&self.run_identity, expected.run_identity, "run identity"),
            (&self.environment_identity, expected.environment_identity, "environment identity"),
        ] {
            if actual != expected_value {
                return Err(TrustError::WorkspaceAttestationContextMismatch(label));
            }
        }
        if asset.payload != self.canonical_payload() {
            return Err(TrustError::InvalidWorkspaceAttestationPayload);
        }
        if let Some(metadata) = metadata {
            store.verify_with_metadata_and_payload_type(
                asset,
                WORKSPACE_ATTESTATION_SIGNING_ROLE,
                metadata,
                now_epoch_secs,
                IN_TOTO_STATEMENT_PAYLOAD_TYPE,
            )?;
        } else {
            store.verify_with_payload_type(
                asset,
                WORKSPACE_ATTESTATION_SIGNING_ROLE,
                IN_TOTO_STATEMENT_PAYLOAD_TYPE,
            )?;
        }
        Ok(())
    }

    pub fn canonical_payload(&self) -> Vec<u8> {
        serde_json::to_vec(&self.statement_payload()).expect("WorkspaceAttestation is JSON-serializable")
    }

    pub fn sign_with(
        &self,
        key_id: impl Into<String>,
        signer: impl FnOnce(&[u8]) -> Vec<u8>,
    ) -> SignedAsset {
        let payload = self.canonical_payload();
        SignedAsset {
            payload_type: IN_TOTO_STATEMENT_PAYLOAD_TYPE.into(),
            payload: payload.clone(),
            key_id: key_id.into(),
            signature: signer(&pae(IN_TOTO_STATEMENT_PAYLOAD_TYPE, &payload)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn verify_signed(
        &self,
        asset: &SignedAsset,
        store: &TrustStore,
        metadata: Option<&TrustMetadata>,
        now_epoch_secs: u64,
        expected_subject_digest: [u8; 32],
        expected_execution_lock_digest: Option<&str>,
        expected_resolver_pack_digest: Option<&str>,
    ) -> Result<(), TrustError> {
        self.validate(now_epoch_secs, expected_subject_digest)?;
        if let Some(expected) = expected_execution_lock_digest {
            if self.execution_lock_digest != expected {
                return Err(TrustError::WorkspaceAttestationContextMismatch(
                    "execution lock digest",
                ));
            }
        }
        if let Some(expected) = expected_resolver_pack_digest {
            if self.resolver_pack_digest != expected {
                return Err(TrustError::WorkspaceAttestationContextMismatch(
                    "resolver Pack digest",
                ));
            }
        }
        if asset.payload != self.canonical_payload() {
            return Err(TrustError::InvalidWorkspaceAttestationPayload);
        }
        if let Some(metadata) = metadata {
            store.verify_with_metadata_and_payload_type(
                asset,
                WORKSPACE_ATTESTATION_SIGNING_ROLE,
                metadata,
                now_epoch_secs,
                IN_TOTO_STATEMENT_PAYLOAD_TYPE,
            )?;
        } else {
            store.verify_with_payload_type(
                asset,
                WORKSPACE_ATTESTATION_SIGNING_ROLE,
                IN_TOTO_STATEMENT_PAYLOAD_TYPE,
            )?;
        }
        Ok(())
    }

    fn validate(
        &self,
        now_epoch_secs: u64,
        expected_subject_digest: [u8; 32],
    ) -> Result<(), TrustError> {
        if self.subject_digest == [0; 32] || self.subject_digest != expected_subject_digest {
            return Err(TrustError::WorkspaceAttestationSubjectMismatch);
        }
        if self.expires_at_epoch_secs <= self.issued_at_epoch_secs
            || now_epoch_secs < self.issued_at_epoch_secs
            || now_epoch_secs >= self.expires_at_epoch_secs
        {
            return Err(TrustError::WorkspaceAttestationExpired);
        }
        for value in [
            &self.repository,
            &self.reference,
            &self.commit_tree_digest,
            &self.execution_lock_digest,
            &self.resolver_pack_digest,
            &self.toolchain_identity,
            &self.workload_issuer,
            &self.workload_audience,
            &self.workflow,
            &self.run_identity,
            &self.environment_identity,
        ] {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(TrustError::InvalidWorkspaceAttestation);
            }
        }
        if self.build_output_digests.is_empty()
            || self.build_output_digests.contains(&[0; 32])
        {
            return Err(TrustError::InvalidWorkspaceAttestation);
        }
        let mut material_names = BTreeSet::new();
        for material in &self.materials {
            if material.name.trim().is_empty()
                || material.digest.trim().is_empty()
                || material.name.chars().any(char::is_control)
                || material.digest.chars().any(char::is_control)
                || !material_names.insert(&material.name)
            {
                return Err(TrustError::InvalidWorkspaceAttestation);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceAttestationExpectations<'a> {
    pub subject_digest: [u8; 32],
    pub repository: &'a str,
    pub reference: &'a str,
    pub commit_tree_digest: &'a str,
    pub execution_lock_digest: &'a str,
    pub resolver_pack_digest: &'a str,
    pub toolchain_identity: &'a str,
    pub workload_issuer: &'a str,
    pub workload_audience: &'a str,
    pub workflow: &'a str,
    pub run_identity: &'a str,
    pub environment_identity: &'a str,
}

fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
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

/// A Root identity in the ADR-0180 preparation contract.
///
/// `key_id` is an operator-facing label; the public key bytes are the
/// cryptographic identity.  Verification therefore rejects two different
/// labels carrying the same public key instead of counting them twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootKeyIdentity {
    pub key_id: String,
    pub public_key: [u8; 32],
    pub revoked: bool,
}

/// A delegated role key carried by Root metadata.  The key is data in this
/// preparation seam only; it is not inserted into `TrustStore` automatically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegatedKeyIdentity {
    pub key_id: String,
    pub public_key: [u8; 32],
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegatedRoleMetadata {
    pub role: String,
    pub namespace: String,
    pub issued_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
    pub keys: Vec<DelegatedKeyIdentity>,
    /// Keys from the immediately preceding role generation that may overlap
    /// during a bounded rotation window.  The key records remain in `keys` so
    /// the canonical payload is self-contained and content-addressable.
    pub previous_key_ids: BTreeSet<String>,
    pub overlap_until_epoch_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootTrustMetadata {
    pub namespace: String,
    pub version: u64,
    pub issued_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
    pub threshold: u8,
    pub root_keys: Vec<RootKeyIdentity>,
    pub delegated_roles: Vec<DelegatedRoleMetadata>,
    /// Digest of the immediately preceding Root metadata generation.  The
    /// digest is an explicit chain anchor; monotonic acceptance remains the
    /// responsibility of `TrustState` and is deliberately not bypassed here.
    pub previous_metadata_digest: Option<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootThresholdSignature {
    pub key_id: String,
    pub signature: Vec<u8>,
}

impl RootTrustMetadata {
    /// Deterministic bytes signed by each independent Root identity.
    ///
    /// This is a preparation contract, not a new DSSE asset verifier.  The
    /// domain separator and every field are length-delimited or fixed-width so
    /// cross-language implementations cannot accidentally sign an ambiguous
    /// concatenation.
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-root-trust-metadata-v1\0".to_vec();
        append_string(&mut payload, &self.namespace);
        payload.extend_from_slice(&self.version.to_be_bytes());
        payload.extend_from_slice(&self.issued_at_epoch_secs.to_be_bytes());
        payload.extend_from_slice(&self.expires_at_epoch_secs.to_be_bytes());
        payload.push(self.threshold);

        let mut root_keys = self.root_keys.clone();
        root_keys.sort_by(|left, right| left.key_id.cmp(&right.key_id));
        payload.extend_from_slice(&(root_keys.len() as u64).to_be_bytes());
        for key in root_keys {
            append_string(&mut payload, &key.key_id);
            payload.extend_from_slice(&key.public_key);
            payload.push(u8::from(key.revoked));
        }

        let mut roles = self.delegated_roles.clone();
        roles.sort_by(|left, right| left.role.cmp(&right.role));
        payload.extend_from_slice(&(roles.len() as u64).to_be_bytes());
        for role in roles {
            append_string(&mut payload, &role.role);
            append_string(&mut payload, &role.namespace);
            payload.extend_from_slice(&role.issued_at_epoch_secs.to_be_bytes());
            payload.extend_from_slice(&role.expires_at_epoch_secs.to_be_bytes());
            match role.overlap_until_epoch_secs {
                Some(until) => {
                    payload.push(1);
                    payload.extend_from_slice(&until.to_be_bytes());
                }
                None => payload.push(0),
            }

            let mut keys = role.keys.clone();
            keys.sort_by(|left, right| left.key_id.cmp(&right.key_id));
            payload.extend_from_slice(&(keys.len() as u64).to_be_bytes());
            for key in keys {
                append_string(&mut payload, &key.key_id);
                payload.extend_from_slice(&key.public_key);
                payload.push(u8::from(key.revoked));
            }
            payload.extend_from_slice(&(role.previous_key_ids.len() as u64).to_be_bytes());
            for key_id in role.previous_key_ids {
                append_string(&mut payload, &key_id);
            }
        }

        match self.previous_metadata_digest {
            Some(digest) => {
                payload.push(1);
                payload.extend_from_slice(&digest);
            }
            None => payload.push(0),
        }
        payload
    }

    pub fn subject_digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_payload()).into()
    }

    /// Validate metadata structure and its time window without granting any
    /// role or execution authority.
    pub fn validate_at(&self, now_epoch_secs: u64) -> Result<(), TrustError> {
        if self.namespace.trim().is_empty() || self.namespace.chars().any(char::is_control) {
            return Err(TrustError::InvalidRootNamespace);
        }
        if self.version == 0 {
            return Err(TrustError::InvalidRootMetadataVersion);
        }
        if self.threshold as usize != ROOT_THRESHOLD || self.root_keys.len() != ROOT_KEY_COUNT {
            return Err(TrustError::InvalidRootThresholdPolicy);
        }
        if self.expires_at_epoch_secs <= self.issued_at_epoch_secs {
            return Err(TrustError::InvalidRootMetadataWindow);
        }
        if self.previous_metadata_digest == Some([0; 32]) {
            return Err(TrustError::InvalidRootPreviousDigest);
        }
        if now_epoch_secs < self.issued_at_epoch_secs {
            return Err(TrustError::RootMetadataNotYetValid);
        }
        if now_epoch_secs >= self.expires_at_epoch_secs {
            return Err(TrustError::RootMetadataExpired);
        }

        let mut key_ids = BTreeSet::new();
        let mut public_keys = BTreeSet::new();
        for key in &self.root_keys {
            if key.key_id.trim().is_empty() || key.key_id.chars().any(char::is_control) {
                return Err(TrustError::InvalidRootKeyIdentity);
            }
            if key.public_key == [0; 32] {
                return Err(TrustError::InvalidRootKeyIdentity);
            }
            if !key_ids.insert(&key.key_id) {
                return Err(TrustError::DuplicateRootKeyId(key.key_id.clone()));
            }
            if !public_keys.insert(key.public_key) {
                return Err(TrustError::DuplicateRootPublicKey);
            }
        }

        let mut roles = BTreeSet::new();
        for role in &self.delegated_roles {
            if role.role.trim().is_empty() || role.role.chars().any(char::is_control) {
                return Err(TrustError::InvalidDelegatedRole);
            }
            if role.namespace != self.namespace {
                return Err(TrustError::DelegatedNamespaceMismatch {
                    role: role.role.clone(),
                });
            }
            if !roles.insert(&role.role) {
                return Err(TrustError::DuplicateDelegatedRole(role.role.clone()));
            }
            if role.expires_at_epoch_secs <= role.issued_at_epoch_secs {
                return Err(TrustError::InvalidDelegatedRoleWindow {
                    role: role.role.clone(),
                });
            }
            if now_epoch_secs < role.issued_at_epoch_secs {
                return Err(TrustError::DelegatedRoleNotYetValid {
                    role: role.role.clone(),
                });
            }
            if now_epoch_secs >= role.expires_at_epoch_secs {
                return Err(TrustError::DelegatedRoleExpired {
                    role: role.role.clone(),
                });
            }
            if role.keys.is_empty() {
                return Err(TrustError::DelegatedRoleHasNoKeys {
                    role: role.role.clone(),
                });
            }
            if role.previous_key_ids.len() > MAX_DELEGATED_ROTATION_OVERLAP_KEYS {
                return Err(TrustError::DelegatedRotationOverlapTooLarge {
                    role: role.role.clone(),
                });
            }
            match (role.overlap_until_epoch_secs, role.previous_key_ids.is_empty()) {
                (Some(until), false) if until > role.issued_at_epoch_secs && until <= role.expires_at_epoch_secs => {}
                (None, true) => {}
                _ => return Err(TrustError::InvalidDelegatedRotation {
                    role: role.role.clone(),
                }),
            }

            let mut role_key_ids = BTreeSet::new();
            let mut role_public_keys = BTreeSet::new();
            for key in &role.keys {
                if key.key_id.trim().is_empty() || key.key_id.chars().any(char::is_control) || key.public_key == [0; 32] {
                    return Err(TrustError::InvalidDelegatedKeyIdentity {
                        role: role.role.clone(),
                    });
                }
                if !role_key_ids.insert(&key.key_id) {
                    return Err(TrustError::DuplicateDelegatedKeyId {
                        role: role.role.clone(),
                        key_id: key.key_id.clone(),
                    });
                }
                if !role_public_keys.insert(key.public_key) {
                    return Err(TrustError::DuplicateDelegatedPublicKey {
                        role: role.role.clone(),
                    });
                }
            }
            if role.previous_key_ids.iter().any(|key_id| !role_key_ids.contains(key_id)) {
                return Err(TrustError::DelegatedRotationKeyMissing {
                    role: role.role.clone(),
                });
            }
        }
        Ok(())
    }

    /// Validate this metadata generation against the immediately preceding
    /// generation without mutating any trust state or granting authority.
    ///
    /// The previous generation is supplied explicitly so this remains a pure
    /// offline seam: callers are still responsible for independently
    /// validating and threshold-verifying every generation before using its
    /// delegated keys.  An expired previous generation is therefore valid as a
    /// historical chain link; only the candidate generation is checked against
    /// `now_epoch_secs` here.
    pub fn validate_chain(
        &self,
        previous: Option<&RootTrustMetadata>,
        now_epoch_secs: u64,
    ) -> Result<(), TrustError> {
        self.validate_at(now_epoch_secs)?;

        match previous {
            None if self.previous_metadata_digest.is_none() => Ok(()),
            None => Err(TrustError::RootMetadataChainMissingPrevious),
            Some(previous) => {
                if self.namespace != previous.namespace {
                    return Err(TrustError::RootMetadataChainNamespaceMismatch);
                }
                if self.version <= previous.version {
                    return Err(TrustError::RootMetadataChainVersionRollback {
                        previous: previous.version,
                        current: self.version,
                    });
                }
                if self.issued_at_epoch_secs <= previous.issued_at_epoch_secs {
                    return Err(TrustError::RootMetadataChainIssuedAtRollback {
                        previous: previous.issued_at_epoch_secs,
                        current: self.issued_at_epoch_secs,
                    });
                }

                match self.previous_metadata_digest {
                    None => Err(TrustError::RootMetadataChainMissingDigest),
                    Some(digest) if digest == previous.subject_digest() => Ok(()),
                    Some(_) => Err(TrustError::RootMetadataChainDigestMismatch),
                }
            }
        }
    }

    /// Verify a 2-of-3 Root signature set over the canonical metadata bytes.
    /// Duplicate labels and duplicate public keys never count as independent
    /// identities.  This function intentionally does not mutate `TrustStore`
    /// or accept a delegated asset.
    pub fn verify_threshold(
        &self,
        signatures: &[RootThresholdSignature],
        now_epoch_secs: u64,
    ) -> Result<(), TrustError> {
        self.validate_at(now_epoch_secs)?;
        if signatures.is_empty() {
            return Err(TrustError::InsufficientRootSignatures {
                required: ROOT_THRESHOLD,
                valid: 0,
            });
        }
        if signatures.len() > MAX_ROOT_SIGNATURES {
            return Err(TrustError::TooManyRootSignatures);
        }
        let mut root_by_id = BTreeMap::new();
        for key in &self.root_keys {
            root_by_id.insert(key.key_id.as_str(), key);
        }
        let mut seen_key_ids = BTreeSet::new();
        let mut seen_public_keys = BTreeSet::new();
        let mut valid = 0usize;
        let payload = self.canonical_payload();
        for signature in signatures {
            if !seen_key_ids.insert(&signature.key_id) {
                return Err(TrustError::DuplicateRootSignature(signature.key_id.clone()));
            }
            let key = root_by_id
                .get(signature.key_id.as_str())
                .ok_or_else(|| TrustError::UnknownRootKey(signature.key_id.clone()))?;
            if key.revoked {
                return Err(TrustError::RevokedRootKey(signature.key_id.clone()));
            }
            // A duplicate public key is an alias, not an independent signer.
            if !seen_public_keys.insert(key.public_key) {
                continue;
            }
            let verifying_key = VerifyingKey::from_bytes(&key.public_key)
                .map_err(|_| TrustError::InvalidRootPublicKey(signature.key_id.clone()))?;
            let signature_bytes: Signature = signature
                .signature
                .as_slice()
                .try_into()
                .map_err(|_| TrustError::RootSignatureLength)?;
            verifying_key
                .verify(&pae(ROOT_TRUST_METADATA_PAYLOAD_TYPE, &payload), &signature_bytes)
                .map_err(|_| TrustError::InvalidRootSignature(signature.key_id.clone()))?;
            valid += 1;
        }
        if valid < ROOT_THRESHOLD {
            return Err(TrustError::InsufficientRootSignatures {
                required: ROOT_THRESHOLD,
                valid,
            });
        }
        Ok(())
    }

    /// Resolve one delegated key for an explicitly named namespace and role.
    /// The returned bytes are metadata only; callers still need an independent
    /// asset signature verifier and must not treat this as a production grant.
    pub fn delegated_key(
        &self,
        role_name: &str,
        namespace: &str,
        key_id: &str,
        now_epoch_secs: u64,
    ) -> Result<[u8; 32], TrustError> {
        self.validate_at(now_epoch_secs)?;
        let role = self
            .delegated_roles
            .iter()
            .find(|role| role.role == role_name)
            .ok_or_else(|| TrustError::UnknownDelegatedRole(role_name.to_string()))?;
        if role.namespace != namespace {
            return Err(TrustError::DelegatedNamespaceMismatch {
                role: role_name.to_string(),
            });
        }
        let key = role
            .keys
            .iter()
            .find(|key| key.key_id == key_id)
            .ok_or_else(|| TrustError::UnknownDelegatedKey {
                role: role_name.to_string(),
                key_id: key_id.to_string(),
            })?;
        if key.revoked {
            return Err(TrustError::RevokedDelegatedKey {
                role: role_name.to_string(),
                key_id: key_id.to_string(),
            });
        }
        if role.previous_key_ids.contains(key_id)
            && role.overlap_until_epoch_secs.is_some_and(|until| now_epoch_secs >= until)
        {
            return Err(TrustError::DelegatedRotationExpired {
                role: role_name.to_string(),
                key_id: key_id.to_string(),
            });
        }
        Ok(key.public_key)
    }
}

/// Signed, content-addressed revocation state carried with an offline
/// delivery.  TrustMetadata revokes signing identities; this snapshot also
/// revokes concrete asset digests and therefore closes the gap between a
/// valid signature and a still-authorized artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevocationSnapshot {
    pub version: u64,
    pub issued_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
    pub revoked_asset_digests: BTreeSet<[u8; 32]>,
}

impl RevocationSnapshot {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-revocation-snapshot-v0\0".to_vec();
        payload.extend_from_slice(&self.version.to_be_bytes());
        payload.extend_from_slice(&self.issued_at_epoch_secs.to_be_bytes());
        payload.extend_from_slice(&self.expires_at_epoch_secs.to_be_bytes());
        payload.extend_from_slice(&(self.revoked_asset_digests.len() as u64).to_be_bytes());
        for digest in &self.revoked_asset_digests {
            payload.extend_from_slice(digest);
        }
        payload
    }

    pub fn subject_digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_payload()).into()
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

    pub fn verify_signed_with_metadata(
        &self,
        asset: &SignedAsset,
        store: &TrustStore,
        role: &str,
        metadata: &TrustMetadata,
        now_epoch_secs: u64,
    ) -> Result<(), TrustError> {
        self.validate(now_epoch_secs)?;
        if asset.payload != self.canonical_payload() {
            return Err(TrustError::InvalidRevocationSnapshotPayload);
        }
        store.verify_with_metadata(asset, role, metadata, now_epoch_secs)?;
        Ok(())
    }

    pub fn is_asset_revoked(&self, digest: &[u8; 32]) -> bool {
        self.revoked_asset_digests.contains(digest)
    }

    fn validate(&self, now_epoch_secs: u64) -> Result<(), TrustError> {
        if self.version == 0 || self.expires_at_epoch_secs <= self.issued_at_epoch_secs {
            return Err(TrustError::InvalidRevocationSnapshot);
        }
        if now_epoch_secs < self.issued_at_epoch_secs || now_epoch_secs >= self.expires_at_epoch_secs {
            return Err(TrustError::RevocationSnapshotExpired);
        }
        if self.revoked_asset_digests.contains(&[0; 32]) {
            return Err(TrustError::InvalidRevocationSnapshot);
        }
        Ok(())
    }
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
    #[error("workspace attestation is invalid")]
    InvalidWorkspaceAttestation,
    #[error("workspace attestation payload does not match its canonical form")]
    InvalidWorkspaceAttestationPayload,
    #[error("workspace attestation subject does not match the resolved descriptor")]
    WorkspaceAttestationSubjectMismatch,
    #[error("workspace attestation is expired or not yet valid")]
    WorkspaceAttestationExpired,
    #[error("workspace attestation context does not match the expected {0}")]
    WorkspaceAttestationContextMismatch(&'static str),
    #[error("trust metadata version must be non-zero")]
    InvalidMetadataVersion,
    #[error("trust metadata is expired")]
    MetadataExpired,
    #[error("signing key is revoked: {0}")]
    RevokedKey(String),
    #[error("revocation snapshot is invalid")]
    InvalidRevocationSnapshot,
    #[error("revocation snapshot payload does not match its canonical form")]
    InvalidRevocationSnapshotPayload,
    #[error("revocation snapshot is expired or not yet valid")]
    RevocationSnapshotExpired,
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
    #[error("Root metadata namespace is empty or contains a control character")]
    InvalidRootNamespace,
    #[error("Root metadata version must be non-zero")]
    InvalidRootMetadataVersion,
    #[error("Root metadata threshold policy must be exactly 2-of-3")]
    InvalidRootThresholdPolicy,
    #[error("Root metadata validity window is invalid")]
    InvalidRootMetadataWindow,
    #[error("Root metadata previous-generation digest is invalid")]
    InvalidRootPreviousDigest,
    #[error("Root metadata chain is missing the previous generation")]
    RootMetadataChainMissingPrevious,
    #[error("Root metadata chain is missing the previous-generation digest")]
    RootMetadataChainMissingDigest,
    #[error("Root metadata chain namespace does not match the previous generation")]
    RootMetadataChainNamespaceMismatch,
    #[error("Root metadata chain version moved backwards from {previous} to {current}")]
    RootMetadataChainVersionRollback { previous: u64, current: u64 },
    #[error("Root metadata chain issued-at moved backwards from {previous} to {current}")]
    RootMetadataChainIssuedAtRollback { previous: u64, current: u64 },
    #[error("Root metadata chain previous-generation digest does not match")]
    RootMetadataChainDigestMismatch,
    #[error("Root metadata is not yet valid")]
    RootMetadataNotYetValid,
    #[error("Root metadata is expired")]
    RootMetadataExpired,
    #[error("Root key identity is invalid")]
    InvalidRootKeyIdentity,
    #[error("duplicate Root key id: {0}")]
    DuplicateRootKeyId(String),
    #[error("duplicate Root public-key identity")]
    DuplicateRootPublicKey,
    #[error("unknown Root key: {0}")]
    UnknownRootKey(String),
    #[error("Root key is revoked: {0}")]
    RevokedRootKey(String),
    #[error("Root public key is invalid: {0}")]
    InvalidRootPublicKey(String),
    #[error("Root signature has invalid length")]
    RootSignatureLength,
    #[error("Root signature verification failed: {0}")]
    InvalidRootSignature(String),
    #[error("duplicate Root signature: {0}")]
    DuplicateRootSignature(String),
    #[error("Root threshold is not met: required {required}, valid {valid}")]
    InsufficientRootSignatures { required: usize, valid: usize },
    #[error("Root signature set exceeds the configured limit")]
    TooManyRootSignatures,
    #[error("delegated role metadata is invalid")]
    InvalidDelegatedRole,
    #[error("delegated role namespace does not match Root namespace: {role}")]
    DelegatedNamespaceMismatch { role: String },
    #[error("duplicate delegated role: {0}")]
    DuplicateDelegatedRole(String),
    #[error("delegated role validity window is invalid: {role}")]
    InvalidDelegatedRoleWindow { role: String },
    #[error("delegated role is not yet valid: {role}")]
    DelegatedRoleNotYetValid { role: String },
    #[error("delegated role is expired: {role}")]
    DelegatedRoleExpired { role: String },
    #[error("delegated role has no keys: {role}")]
    DelegatedRoleHasNoKeys { role: String },
    #[error("delegated rotation overlap is too large: {role}")]
    DelegatedRotationOverlapTooLarge { role: String },
    #[error("delegated rotation metadata is invalid: {role}")]
    InvalidDelegatedRotation { role: String },
    #[error("delegated rotation key is not present in role metadata: {role}")]
    DelegatedRotationKeyMissing { role: String },
    #[error("delegated key identity is invalid: {role}")]
    InvalidDelegatedKeyIdentity { role: String },
    #[error("duplicate delegated key id in role {role}: {key_id}")]
    DuplicateDelegatedKeyId { role: String, key_id: String },
    #[error("duplicate delegated public-key identity in role {role}")]
    DuplicateDelegatedPublicKey { role: String },
    #[error("unknown delegated role: {0}")]
    UnknownDelegatedRole(String),
    #[error("unknown delegated key {key_id} in role {role}")]
    UnknownDelegatedKey { role: String, key_id: String },
    #[error("delegated key is revoked: {role}/{key_id}")]
    RevokedDelegatedKey { role: String, key_id: String },
    #[error("delegated rotation overlap expired: {role}/{key_id}")]
    DelegatedRotationExpired { role: String, key_id: String },
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

    pub fn public_key_bytes(&self, key_id: &str) -> Option<[u8; 32]> {
        self.keys.get(key_id).map(VerifyingKey::to_bytes)
    }

    pub fn role_authorized(&self, role: &str, key_id: &str) -> bool {
        self.role_keys
            .get(role)
            .map(|keys| keys.contains(key_id))
            .unwrap_or(false)
    }

    pub fn verify(&self, asset: &SignedAsset, role: &str) -> Result<[u8; 32], TrustError> {
        self.verify_with_payload_type(asset, role, DSSE_PAYLOAD_TYPE)
    }

    pub fn verify_with_payload_type(
        &self,
        asset: &SignedAsset,
        role: &str,
        expected_payload_type: &str,
    ) -> Result<[u8; 32], TrustError> {
        if asset.payload_type != expected_payload_type { return Err(TrustError::PayloadType); }
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
        self.verify_with_payload_type(asset, role, DSSE_PAYLOAD_TYPE)
    }

    pub fn verify_with_metadata_and_payload_type(
        &self,
        asset: &SignedAsset,
        role: &str,
        metadata: &TrustMetadata,
        now_epoch_secs: u64,
        expected_payload_type: &str,
    ) -> Result<[u8; 32], TrustError> {
        if metadata.version == 0 { return Err(TrustError::InvalidMetadataVersion); }
        if now_epoch_secs >= metadata.expires_at_epoch_secs { return Err(TrustError::MetadataExpired); }
        if metadata.revoked_key_ids.contains(&asset.key_id) { return Err(TrustError::RevokedKey(asset.key_id.clone())); }
        self.verify_with_payload_type(asset, role, expected_payload_type)
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

    fn root_metadata() -> (RootTrustMetadata, [SigningKey; ROOT_KEY_COUNT]) {
        let signing_keys = [
            SigningKey::from_bytes(&[31; 32]),
            SigningKey::from_bytes(&[32; 32]),
            SigningKey::from_bytes(&[33; 32]),
        ];
        let metadata = RootTrustMetadata {
            namespace: "org/repo".into(),
            version: 7,
            issued_at_epoch_secs: 10,
            expires_at_epoch_secs: 100,
            threshold: ROOT_THRESHOLD as u8,
            root_keys: signing_keys
                .iter()
                .enumerate()
                .map(|(index, key)| RootKeyIdentity {
                    key_id: format!("root-{}", (b'a' + index as u8) as char),
                    public_key: key.verifying_key().to_bytes(),
                    revoked: false,
                })
                .collect(),
            delegated_roles: vec![DelegatedRoleMetadata {
                role: "runtime-pack-signing".into(),
                namespace: "org/repo".into(),
                issued_at_epoch_secs: 10,
                expires_at_epoch_secs: 100,
                keys: vec![DelegatedKeyIdentity {
                    key_id: "pack-a".into(),
                    public_key: [41; 32],
                    revoked: false,
                }],
                previous_key_ids: BTreeSet::new(),
                overlap_until_epoch_secs: None,
            }],
            previous_metadata_digest: None,
        };
        (metadata, signing_keys)
    }

    fn root_signatures(keys: &[SigningKey; ROOT_KEY_COUNT]) -> Vec<RootThresholdSignature> {
        let metadata = root_metadata().0;
        let payload = metadata.canonical_payload();
        keys[..ROOT_THRESHOLD]
            .iter()
            .enumerate()
            .map(|(index, key)| RootThresholdSignature {
                key_id: format!("root-{}", (b'a' + index as u8) as char),
                signature: key
                    .sign(&pae(ROOT_TRUST_METADATA_PAYLOAD_TYPE, &payload))
                    .to_bytes()
                    .to_vec(),
            })
            .collect()
    }

    fn hex_bytes(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
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
    fn standard_dsse_envelope_round_trips_base64_payload_and_signature() {
        let key = SigningKey::from_bytes(&[6; 32]);
        let asset = signed(DSSE_PAYLOAD_TYPE, b"portable", "key-id", &key);
        let json = asset.to_dsse_json();
        assert_eq!(json["payloadType"], DSSE_PAYLOAD_TYPE);
        assert!(json.get("payloadHex").is_none());
        assert_eq!(json["signatures"][0]["keyid"], "key-id");
        let parsed = SignedAsset::from_dsse_json(&json).unwrap();
        assert_eq!(parsed, asset);
        assert_eq!(parsed.to_dsse_envelope().payload, b"portable");
        let mut unknown = json.clone();
        unknown["future"] = json!("ignored");
        assert!(SignedAsset::from_dsse_json(&unknown).is_err());
        let mut mixed = json.clone();
        mixed["payloadHex"] = json!("00");
        assert!(SignedAsset::from_dsse_json(&mixed).is_err());
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
        let statement_json: serde_json::Value = serde_json::from_slice(&asset.payload).unwrap();
        assert_eq!(statement_json["_type"], IN_TOTO_STATEMENT_TYPE);
        assert_eq!(asset.payload_type, IN_TOTO_STATEMENT_PAYLOAD_TYPE);
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
    fn workspace_attestation_binds_descriptor_lock_pack_and_freshness() {
        let key = SigningKey::from_bytes(&[16; 32]);
        let subject: [u8; 32] = Sha256::digest(b"workspace-descriptor").into();
        let attestation = WorkspaceAttestation {
            subject_digest: subject,
            repository: "org/repo".into(),
            reference: "refs/heads/main".into(),
            commit_tree_digest: "tree-1".into(),
            execution_lock_digest: "lock-1".into(),
            resolver_pack_digest: "resolver-pack-1".into(),
            toolchain_identity: "java-21".into(),
            materials: vec![WorkspaceMaterial {
                name: "erf://config/pom.xml".into(),
                digest: "config-1".into(),
            }],
            build_output_digests: vec![[6; 32]],
            workload_issuer: "https://issuer.example".into(),
            workload_audience: "erf-workspace".into(),
            workflow: "build".into(),
            run_identity: "run-1".into(),
            environment_identity: "linux/amd64".into(),
            issued_at_epoch_secs: 10,
            expires_at_epoch_secs: 100,
        };
        let asset = attestation.sign_with("workspace-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let store = store("workspace-key", &key, WORKSPACE_ATTESTATION_SIGNING_ROLE);
        assert!(attestation
            .verify_signed(&asset, &store, None, 20, subject, Some("lock-1"), Some("resolver-pack-1"))
            .is_ok());
        assert_eq!(
            attestation.verify_signed(&asset, &store, None, 20, [7; 32], Some("lock-1"), Some("resolver-pack-1")),
            Err(TrustError::WorkspaceAttestationSubjectMismatch)
        );
        assert_eq!(
            attestation.verify_signed(&asset, &store, None, 20, subject, Some("other-lock"), Some("resolver-pack-1")),
            Err(TrustError::WorkspaceAttestationContextMismatch("execution lock digest"))
        );
        assert_eq!(
            attestation.verify_signed(&asset, &store, None, 100, subject, Some("lock-1"), Some("resolver-pack-1")),
            Err(TrustError::WorkspaceAttestationExpired)
        );
    }

    #[test]
    fn workspace_attestation_is_an_in_toto_statement_and_uses_its_payload_type() {
        let subject = [9; 32];
        let attestation = WorkspaceAttestation {
            subject_digest: subject,
            repository: "org/repo".into(),
            reference: "refs/heads/main".into(),
            commit_tree_digest: "tree-1".into(),
            execution_lock_digest: "lock-1".into(),
            resolver_pack_digest: "resolver-pack-1".into(),
            toolchain_identity: "java-21".into(),
            materials: vec![
                WorkspaceMaterial { name: "z-lockfile".into(), digest: "z".into() },
                WorkspaceMaterial { name: "a-config".into(), digest: "a".into() },
            ],
            build_output_digests: vec![[7; 32]],
            workload_issuer: "https://issuer.example".into(),
            workload_audience: "erf-workspace".into(),
            workflow: "build".into(),
            run_identity: "run-1".into(),
            environment_identity: "linux/amd64".into(),
            issued_at_epoch_secs: 10,
            expires_at_epoch_secs: 100,
        };
        let payload = attestation.canonical_payload();
        let statement: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(statement["_type"], IN_TOTO_STATEMENT_TYPE);
        assert_eq!(statement["predicateType"], WORKSPACE_PREDICATE_TYPE);
        assert_eq!(statement["subject"][0]["name"], "erf://workspace-descriptor");
        assert_eq!(statement["predicate"]["materials"][4]["name"], "a-config");
        assert_eq!(statement["predicate"]["materials"][5]["name"], "z-lockfile");
        let key = SigningKey::from_bytes(&[19; 32]);
        let asset = attestation.sign_with("workspace-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        assert_eq!(asset.payload_type, IN_TOTO_STATEMENT_PAYLOAD_TYPE);
        assert_eq!(asset.signing_bytes(), pae(IN_TOTO_STATEMENT_PAYLOAD_TYPE, &payload));
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
    fn revocation_snapshot_binds_assets_and_external_trust_metadata() {
        let key = SigningKey::from_bytes(&[15; 32]);
        let revoked_digest = [7; 32];
        let snapshot = RevocationSnapshot {
            version: 1,
            issued_at_epoch_secs: 10,
            expires_at_epoch_secs: 100,
            revoked_asset_digests: [revoked_digest].into_iter().collect(),
        };
        let asset = snapshot.sign_with("snapshot-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let store = store("snapshot-key", &key, "revocation-snapshot-signing");
        let metadata = TrustMetadata { version: 1, expires_at_epoch_secs: 100, revoked_key_ids: BTreeSet::new() };
        assert!(snapshot
            .verify_signed_with_metadata(
                &asset,
                &store,
                "revocation-snapshot-signing",
                &metadata,
                99,
            )
            .is_ok());
        assert!(snapshot.is_asset_revoked(&revoked_digest));
        assert!(!snapshot.is_asset_revoked(&[8; 32]));
        assert_eq!(
            snapshot.verify_signed_with_metadata(
                &asset,
                &store,
                "revocation-snapshot-signing",
                &metadata,
                100,
            ),
            Err(TrustError::RevocationSnapshotExpired)
        );
        let mut changed = snapshot.clone();
        changed.version = 2;
        assert_eq!(
            changed.verify_signed_with_metadata(
                &asset,
                &store,
                "revocation-snapshot-signing",
                &metadata,
                99,
            ),
            Err(TrustError::InvalidRevocationSnapshotPayload)
        );
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

    #[test]
    fn root_threshold_requires_two_independent_signers() {
        let (metadata, keys) = root_metadata();
        let signatures = root_signatures(&keys);
        assert_eq!(metadata.verify_threshold(&signatures, 20), Ok(()));
        assert_eq!(metadata.verify_threshold(&signatures[..1], 20), Err(TrustError::InsufficientRootSignatures { required: 2, valid: 1 }));

        let mut duplicate = signatures.clone();
        duplicate.push(signatures[0].clone());
        assert_eq!(metadata.verify_threshold(&duplicate, 20), Err(TrustError::DuplicateRootSignature("root-a".into())));

        let mut too_many = signatures.clone();
        too_many.resize(MAX_ROOT_SIGNATURES + 1, signatures[0].clone());
        assert_eq!(metadata.verify_threshold(&too_many, 20), Err(TrustError::TooManyRootSignatures));
    }

    #[test]
    fn root_threshold_rejects_aliases_and_revoked_identities() {
        let (mut metadata, _keys) = root_metadata();
        metadata.root_keys[1].public_key = metadata.root_keys[0].public_key;
        assert_eq!(metadata.verify_threshold(&[], 20), Err(TrustError::DuplicateRootPublicKey));

        let (mut metadata, keys) = root_metadata();
        metadata.root_keys[0].revoked = true;
        let signatures = root_signatures(&keys);
        assert_eq!(metadata.verify_threshold(&signatures, 20), Err(TrustError::RevokedRootKey("root-a".into())));
    }

    #[test]
    fn root_metadata_window_and_namespace_are_fail_closed() {
        let (mut metadata, _) = root_metadata();
        assert_eq!(metadata.validate_at(9), Err(TrustError::RootMetadataNotYetValid));
        assert_eq!(metadata.validate_at(100), Err(TrustError::RootMetadataExpired));
        metadata.namespace = "".into();
        assert_eq!(metadata.validate_at(20), Err(TrustError::InvalidRootNamespace));
        metadata.namespace = "org/repo".into();
        metadata.previous_metadata_digest = Some([0; 32]);
        assert_eq!(metadata.validate_at(20), Err(TrustError::InvalidRootPreviousDigest));
        metadata.previous_metadata_digest = None;
        metadata.delegated_roles[0].namespace = "other/repo".into();
        assert_eq!(metadata.validate_at(20), Err(TrustError::DelegatedNamespaceMismatch { role: "runtime-pack-signing".into() }));
    }

    #[test]
    fn root_metadata_chain_is_pure_and_requires_digest_identity_and_monotonicity() {
        let (previous, _) = root_metadata();
        let mut current = previous.clone();
        current.version += 1;
        current.issued_at_epoch_secs = 20;
        current.expires_at_epoch_secs = 200;
        current.delegated_roles[0].issued_at_epoch_secs = 20;
        current.delegated_roles[0].expires_at_epoch_secs = 200;
        current.previous_metadata_digest = Some(previous.subject_digest());

        // The previous generation is expired at this point, but remains a
        // valid historical chain link.  No trust state is mutated by this
        // pure check.
        assert_eq!(current.validate_chain(Some(&previous), 120), Ok(()));
        assert_eq!(current.validate_chain(None, 120), Err(TrustError::RootMetadataChainMissingPrevious));

        let mut wrong_digest = current.clone();
        wrong_digest.previous_metadata_digest = Some([9; 32]);
        assert_eq!(
            wrong_digest.validate_chain(Some(&previous), 120),
            Err(TrustError::RootMetadataChainDigestMismatch)
        );

        let mut wrong_namespace = current.clone();
        wrong_namespace.namespace = "other/repo".into();
        assert_eq!(
            wrong_namespace.validate_chain(Some(&previous), 120),
            Err(TrustError::DelegatedNamespaceMismatch { role: "runtime-pack-signing".into() })
        );
        wrong_namespace.delegated_roles[0].namespace = "other/repo".into();
        assert_eq!(
            wrong_namespace.validate_chain(Some(&previous), 120),
            Err(TrustError::RootMetadataChainNamespaceMismatch)
        );

        let mut version_rollback = current.clone();
        version_rollback.version = previous.version;
        assert_eq!(
            version_rollback.validate_chain(Some(&previous), 120),
            Err(TrustError::RootMetadataChainVersionRollback {
                previous: previous.version,
                current: previous.version,
            })
        );

        let mut issued_at_rollback = current.clone();
        issued_at_rollback.issued_at_epoch_secs = previous.issued_at_epoch_secs;
        assert_eq!(
            issued_at_rollback.validate_chain(Some(&previous), 120),
            Err(TrustError::RootMetadataChainIssuedAtRollback {
                previous: previous.issued_at_epoch_secs,
                current: previous.issued_at_epoch_secs,
            })
        );

        let mut missing_digest = current;
        missing_digest.previous_metadata_digest = None;
        assert_eq!(
            missing_digest.validate_chain(Some(&previous), 120),
            Err(TrustError::RootMetadataChainMissingDigest)
        );
    }

    #[test]
    fn delegated_rotation_is_bounded_and_expiry_is_enforced() {
        let (mut metadata, _) = root_metadata();
        {
            let role = &mut metadata.delegated_roles[0];
            role.keys.push(DelegatedKeyIdentity {
                key_id: "pack-old".into(),
                public_key: [42; 32],
                revoked: false,
            });
            role.previous_key_ids.insert("pack-old".into());
            role.overlap_until_epoch_secs = Some(30);
        }
        assert_eq!(metadata.delegated_key("runtime-pack-signing", "org/repo", "pack-old", 20), Ok([42; 32]));
        assert_eq!(metadata.delegated_key("runtime-pack-signing", "org/repo", "pack-old", 30), Err(TrustError::DelegatedRotationExpired { role: "runtime-pack-signing".into(), key_id: "pack-old".into() }));
        assert_eq!(metadata.delegated_key("runtime-pack-signing", "other/repo", "pack-a", 20), Err(TrustError::DelegatedNamespaceMismatch { role: "runtime-pack-signing".into() }));

        metadata.delegated_roles[0].previous_key_ids.insert("pack-extra".into());
        assert_eq!(metadata.validate_at(20), Err(TrustError::DelegatedRotationOverlapTooLarge { role: "runtime-pack-signing".into() }));
    }

    #[test]
    fn root_metadata_canonical_digest_is_stable_for_test_vector() {
        let (metadata, _) = root_metadata();
        assert_eq!(hex_digest(&metadata.subject_digest()), "6b031cc3fdaaa0edcc528cf7f2af0333a84d085e26e18ba790d10f4c91aa00f1");
    }

    #[test]
    fn root_threshold_fixture_freezes_contract_shape_and_digest() {
        let fixture: Value = serde_json::from_str(include_str!("../test-vectors/root-threshold-v1.json")).unwrap();
        let (metadata, keys) = root_metadata();
        assert_eq!(fixture["payloadType"], ROOT_TRUST_METADATA_PAYLOAD_TYPE);
        assert_eq!(fixture["authority"], "LOCAL_UNATTESTED_SHADOW");
        assert_eq!(fixture["metadata"]["threshold"], ROOT_THRESHOLD);
        assert_eq!(fixture["metadata"]["rootKeys"].as_array().unwrap().len(), ROOT_KEY_COUNT);
        assert_eq!(fixture["canonicalSubjectDigest"], hex_digest(&metadata.subject_digest()));
        let signatures = root_signatures(&keys);
        assert_eq!(fixture["signatures"].as_array().unwrap().len(), signatures.len());
        for (index, signature) in signatures.iter().enumerate() {
            assert_eq!(fixture["signatures"][index]["keyId"], signature.key_id);
            assert_eq!(fixture["signatures"][index]["signatureHex"], hex_bytes(&signature.signature));
        }
        assert_eq!(fixture["acceptance"]["expected"], "PASS_TWO_OF_THREE");
        assert_eq!(fixture["negativeCases"].as_array().unwrap().len(), 5);
    }
}
