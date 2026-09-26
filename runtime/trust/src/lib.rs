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
}
