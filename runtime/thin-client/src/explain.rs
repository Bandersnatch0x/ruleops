//! Rich explain boundary.
//!
//! Layers verified rule metadata and an authority-status label on top of a
//! digest-only `FindingView`, or fails closed with a typed refusal. This module
//! never carries source content, never emits a Gate decision, and never changes
//! trust state: it only explains findings whose rule metadata is present, active
//! and identity-matched, and refuses otherwise.

use serde::Serialize;
use serde_json::{json, Map, Value};
use erf_trust::{SignedAsset, TrustMetadata, TrustStore, DSSE_PAYLOAD_TYPE};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    digest_json, is_sha256_digest, reject_forbidden_fields, require_exact_fields, required_sha256_digest,
    required_text, ClientError, FindingLocation, FindingView, CLIENT_API_VERSION,
};

pub const RULE_METADATA_KIND: &str = "RuleExplanationMetadata";
pub const RULE_METADATA_VERSION: &str = "rule-explanation-metadata-v1";
pub const RICH_EXPLANATION_KIND: &str = "RichExplanation";
pub const RICH_EXPLANATION_REFUSAL_KIND: &str = "RichExplanationRefusal";
pub const RICH_EXPLANATION_VERSION: &str = "rich-explanation-v1";

const AUTHORITY_CONTEXT_KIND: &str = "AuthorityContext";
const AUTHORITY_TRUST_KIND: &str = "AuthorityTrust";
const AUTHORITY_SIGNING_ROLE: &str = "gate-signing";
const AUTHORITY_PAYLOAD_PREFIX: &[u8] = b"erf-rich-explain-authority-v1\0";
const FINDING_ASSESSMENT_KIND: &str = "FindingAssessmentView";
const ACTIONS_LINE: &str = "actions=show,navigate,feedback-draft,exception-request";

const METADATA_SIGNED_FIELDS: &[&str] = &[
    "apiVersion", "kind", "schemaVersion", "ruleId", "ruleVersion", "severity", "status",
    "rationale", "remediation", "standardId", "standardVersion", "bindingDigest",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataStatus {
    Active,
    Expired,
    Revoked,
}

impl MetadataStatus {
    fn parse(value: &str) -> Result<Self, ClientError> {
        match value {
            "active" => Ok(Self::Active),
            "expired" => Ok(Self::Expired),
            "revoked" => Ok(Self::Revoked),
            _ => Err(ClientError::InvalidField("status")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Advisory,
    Warning,
    Blocking,
}

impl Severity {
    fn parse(value: &str) -> Result<Self, ClientError> {
        match value {
            "advisory" => Ok(Self::Advisory),
            "warning" => Ok(Self::Warning),
            "blocking" => Ok(Self::Blocking),
            _ => Err(ClientError::InvalidField("severity")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Advisory => "advisory",
            Self::Warning => "warning",
            Self::Blocking => "blocking",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityLabel {
    TrustedCi,
    LocalUnattested,
    Inconclusive,
}

impl AuthorityLabel {
    fn as_str(self) -> &'static str {
        match self {
            Self::TrustedCi => "TRUSTED_CI",
            Self::LocalUnattested => "LOCAL_UNATTESTED",
            Self::Inconclusive => "INCONCLUSIVE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalReason {
    MetadataMissing,
    MetadataExpired,
    MetadataRevoked,
    DigestMismatch,
    RuleIdentityMismatch,
}

impl RefusalReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::MetadataMissing => "METADATA_MISSING",
            Self::MetadataExpired => "METADATA_EXPIRED",
            Self::MetadataRevoked => "METADATA_REVOKED",
            Self::DigestMismatch => "DIGEST_MISMATCH",
            Self::RuleIdentityMismatch => "RULE_IDENTITY_MISMATCH",
        }
    }

    fn next_step(self) -> &'static str {
        match self {
            Self::MetadataMissing => {
                "提供经验证且与 Finding identity 匹配的 RuleBundle/Policy 元数据 (--rule-meta) 后重试"
            }
            Self::MetadataExpired => "刷新已过期的 RuleBundle/Policy 元数据，取得当前版本后重新解释",
            Self::MetadataRevoked => {
                "该规则元数据已被撤回，联系规则治理方获取有效版本或撤下该规则后再解释"
            }
            Self::DigestMismatch => "重新拉取与该 Finding 权威结果绑定一致的规则元数据后重试",
            Self::RuleIdentityMismatch => "使用与 Finding.ruleId 一致的规则元数据重新解释",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateDecisionLabel {
    Pass,
    Fail,
    Inconclusive,
}

impl GateDecisionLabel {
    fn parse(value: &str) -> Result<Self, ClientError> {
        match value {
            "PASS" => Ok(Self::Pass),
            "FAIL" => Ok(Self::Fail),
            "INCONCLUSIVE" => Ok(Self::Inconclusive),
            _ => Err(ClientError::InvalidField("gateDecision")),
        }
    }
}

/// Verified rule metadata a rich explanation is built from. Parsed from a
/// closed, digest-only JSON projection; the embedded `digest` must match the
/// canonical self-digest, so tampered metadata fails closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleExplanationMetadata {
    pub rule_id: String,
    pub rule_version: String,
    pub severity: Severity,
    pub status: MetadataStatus,
    pub rationale: String,
    pub remediation: String,
    pub standard_id: String,
    pub standard_version: String,
    pub binding_digest: String,
}

impl RuleExplanationMetadata {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let text = fs::read_to_string(path).map_err(|error| ClientError::Io(error.to_string()))?;
        let json: Value = serde_json::from_str(&text).map_err(|error| ClientError::InvalidJson(error.to_string()))?;
        Self::from_json(&json)
    }

    pub fn from_json(json: &Value) -> Result<Self, ClientError> {
        reject_forbidden_fields(json, "rule-metadata")?;
        let object = json.as_object().ok_or(ClientError::InvalidShape("rule metadata must be an object"))?;
        let mut allowed = METADATA_SIGNED_FIELDS.to_vec();
        allowed.push("digest");
        require_exact_fields(object, &allowed)?;
        let metadata = Self::parse_fields(object)?;
        let supplied = required_sha256_digest(object, "digest")?;
        if supplied != metadata.digest() {
            return Err(ClientError::InvalidField("digest"));
        }
        Ok(metadata)
    }

    /// Complete an unsigned metadata projection (all fields except `digest`) by
    /// validating it and returning the canonical, self-digested JSON. Used to
    /// author verified metadata fixtures deterministically.
    pub fn sign(json: &Value) -> Result<Value, ClientError> {
        reject_forbidden_fields(json, "rule-metadata")?;
        let object = json.as_object().ok_or(ClientError::InvalidShape("rule metadata must be an object"))?;
        require_exact_fields(object, METADATA_SIGNED_FIELDS)?;
        Ok(Self::parse_fields(object)?.to_signed_json())
    }

    fn parse_fields(object: &Map<String, Value>) -> Result<Self, ClientError> {
        if required_text(object, "apiVersion")? != CLIENT_API_VERSION {
            return Err(ClientError::InvalidField("apiVersion"));
        }
        if required_text(object, "kind")? != RULE_METADATA_KIND {
            return Err(ClientError::InvalidField("kind"));
        }
        if required_text(object, "schemaVersion")? != RULE_METADATA_VERSION {
            return Err(ClientError::InvalidField("schemaVersion"));
        }
        Ok(Self {
            rule_id: required_text(object, "ruleId")?,
            rule_version: required_text(object, "ruleVersion")?,
            severity: Severity::parse(&required_text(object, "severity")?)?,
            status: MetadataStatus::parse(&required_text(object, "status")?)?,
            rationale: required_explanation_text(object, "rationale")?,
            remediation: required_explanation_text(object, "remediation")?,
            standard_id: required_text(object, "standardId")?,
            standard_version: required_text(object, "standardVersion")?,
            binding_digest: required_sha256_digest(object, "bindingDigest")?,
        })
    }

    fn fields_value(&self) -> Value {
        json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": RULE_METADATA_KIND,
            "schemaVersion": RULE_METADATA_VERSION,
            "ruleId": self.rule_id,
            "ruleVersion": self.rule_version,
            "severity": self.severity.as_str(),
            "status": self.status.as_str(),
            "rationale": self.rationale,
            "remediation": self.remediation,
            "standardId": self.standard_id,
            "standardVersion": self.standard_version,
            "bindingDigest": self.binding_digest,
        })
    }

    pub fn digest(&self) -> String {
        digest_json(&self.fields_value())
    }

    fn to_signed_json(&self) -> Value {
        let mut object = self.fields_value();
        object["digest"] = Value::String(self.digest());
        object
    }
}

fn required_explanation_text(object: &Map<String, Value>, field: &'static str) -> Result<String, ClientError> {
    let value = object.get(field).and_then(Value::as_str).ok_or(ClientError::InvalidField(field))?;
    if value.trim().is_empty() || value.chars().any(char::is_control) || value.chars().count() > 4000 {
        return Err(ClientError::InvalidField(field));
    }
    Ok(value.to_owned())
}

/// Local verifier configuration for a signed authority context. The file is
/// an operational trust input, not a claim carried by the result itself; the
/// caller is responsible for provisioning it from the trusted runtime pack.
#[derive(Debug, Clone)]
pub struct AuthorityTrust {
    store: TrustStore,
    metadata: TrustMetadata,
    role: String,
    now_epoch_secs: u64,
}

impl AuthorityTrust {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let text = fs::read_to_string(path).map_err(|error| ClientError::Io(error.to_string()))?;
        let json: Value = serde_json::from_str(&text).map_err(|error| ClientError::InvalidJson(error.to_string()))?;
        Self::from_json(&json)
    }

    pub fn from_json(json: &Value) -> Result<Self, ClientError> {
        let now_epoch_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ClientError::Io("trusted clock is before Unix epoch".into()))?
            .as_secs();
        Self::parse_json(json, now_epoch_secs)
    }

    #[cfg(test)]
    fn from_json_at(json: &Value, now_epoch_secs: u64) -> Result<Self, ClientError> {
        Self::parse_json(json, now_epoch_secs)
    }

    fn parse_json(json: &Value, now_epoch_secs: u64) -> Result<Self, ClientError> {
        reject_forbidden_fields(json, "authority-trust")?;
        let object = json.as_object().ok_or(ClientError::InvalidShape("authority trust must be an object"))?;
        require_exact_fields(object, &["apiVersion", "kind", "role", "keys", "metadata"])?;
        if required_text(object, "apiVersion")? != CLIENT_API_VERSION {
            return Err(ClientError::InvalidField("apiVersion"));
        }
        if required_text(object, "kind")? != AUTHORITY_TRUST_KIND {
            return Err(ClientError::InvalidField("kind"));
        }
        let role = required_text(object, "role")?;
        if role != AUTHORITY_SIGNING_ROLE {
            return Err(ClientError::InvalidField("role"));
        }
        let keys = object.get("keys").and_then(Value::as_array).ok_or(ClientError::InvalidShape("authority trust keys must be an array"))?;
        if keys.is_empty() {
            return Err(ClientError::InvalidField("keys"));
        }
        let mut store = TrustStore::default();
        for key in keys {
            let key = key.as_object().ok_or(ClientError::InvalidShape("authority trust key must be an object"))?;
            require_exact_fields(key, &["keyId", "publicKeyHex"])?;
            let key_id = required_text(key, "keyId")?;
            let public_key = decode_hex(required_text(key, "publicKeyHex")?.as_str(), "publicKeyHex", 32)?;
            store.insert_public_key_bytes(key_id.clone(), &public_key).map_err(|_| ClientError::InvalidField("publicKeyHex"))?;
            store.authorize_role(role.clone(), key_id).map_err(|_| ClientError::InvalidField("role"))?;
        }
        let metadata = object.get("metadata").and_then(Value::as_object).ok_or(ClientError::InvalidShape("authority trust metadata must be an object"))?;
        require_exact_fields(metadata, &["version", "expiresAtEpochSecs", "revokedKeyIds"])?;
        let version = metadata.get("version").and_then(Value::as_u64).ok_or(ClientError::InvalidField("metadata.version"))?;
        let expires_at_epoch_secs = metadata.get("expiresAtEpochSecs").and_then(Value::as_u64).ok_or(ClientError::InvalidField("metadata.expiresAtEpochSecs"))?;
        let revoked_key_ids = metadata
            .get("revokedKeyIds")
            .and_then(Value::as_array)
            .ok_or(ClientError::InvalidShape("metadata.revokedKeyIds must be an array"))?
            .iter()
            .map(|value| value.as_str().map(str::to_owned).ok_or(ClientError::InvalidField("metadata.revokedKeyIds")))
            .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
        Ok(Self {
            store,
            metadata: TrustMetadata { version, expires_at_epoch_secs, revoked_key_ids },
            role,
            now_epoch_secs,
        })
    }
}

/// Authority context bound to a specific authoritative scan result. Trusted
/// status is accepted only after a DSSE signature is verified against an
/// explicit local trust configuration and fresh TrustMetadata; result JSON
/// cannot self-declare `TRUSTED_CI`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityContext {
    gate_decision: GateDecisionLabel,
    attested: bool,
    rule_id: String,
    binding_digest: String,
    finding_fingerprint: String,
}

impl AuthorityContext {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let text = fs::read_to_string(path).map_err(|error| ClientError::Io(error.to_string()))?;
        let json: Value = serde_json::from_str(&text).map_err(|error| ClientError::InvalidJson(error.to_string()))?;
        Self::from_json(&json)
    }

    pub fn from_file_with_trust(path: impl AsRef<Path>, trust: &AuthorityTrust) -> Result<Self, ClientError> {
        let text = fs::read_to_string(path).map_err(|error| ClientError::Io(error.to_string()))?;
        let json: Value = serde_json::from_str(&text).map_err(|error| ClientError::InvalidJson(error.to_string()))?;
        Self::from_json_with_trust(&json, trust)
    }

    pub fn from_json(json: &Value) -> Result<Self, ClientError> {
        Self::parse(json, None)
    }

    pub fn from_json_with_trust(json: &Value, trust: &AuthorityTrust) -> Result<Self, ClientError> {
        Self::parse(json, Some(trust))
    }

    /// Returns the canonical DSSE payload that a Gate authority signer must
    /// sign for the supplied closed identity projection. This is public only
    /// so a trusted producer can construct the proof envelope; verification
    /// always remains on the client side.
    pub fn authority_payload(
        gate_decision: &str,
        rule_id: &str,
        binding_digest: &str,
        finding_fingerprint: &str,
        issued_at_epoch_secs: u64,
        expires_at_epoch_secs: u64,
    ) -> Result<Vec<u8>, ClientError> {
        let gate_decision = GateDecisionLabel::parse(gate_decision)?;
        if rule_id.trim().is_empty() || rule_id.chars().any(char::is_control) {
            return Err(ClientError::InvalidField("ruleId"));
        }
        if !is_sha256_digest(binding_digest) {
            return Err(ClientError::InvalidField("bindingDigest"));
        }
        if finding_fingerprint.trim().is_empty() || finding_fingerprint.chars().any(char::is_control) {
            return Err(ClientError::InvalidField("findingFingerprint"));
        }
        if expires_at_epoch_secs <= issued_at_epoch_secs {
            return Err(ClientError::InvalidField("authorityProof"));
        }
        Ok(canonical_authority_payload(
            gate_decision,
            rule_id,
            binding_digest,
            finding_fingerprint,
            issued_at_epoch_secs,
            expires_at_epoch_secs,
        ))
    }

    fn parse(json: &Value, trust: Option<&AuthorityTrust>) -> Result<Self, ClientError> {
        reject_forbidden_fields(json, "authority")?;
        let object = json.as_object().ok_or(ClientError::InvalidShape("authority context must be an object"))?;
        let required = ["apiVersion", "kind", "gateDecision", "authority", "ruleId", "bindingDigest", "findingFingerprint"];
        if object.keys().any(|key| !required.contains(&key.as_str()) && key != "proof")
            || required.iter().any(|key| !object.contains_key(*key))
        {
            return Err(ClientError::InvalidShape("input fields are not the closed contract"));
        }
        if required_text(object, "apiVersion")? != CLIENT_API_VERSION {
            return Err(ClientError::InvalidField("apiVersion"));
        }
        if required_text(object, "kind")? != AUTHORITY_CONTEXT_KIND {
            return Err(ClientError::InvalidField("kind"));
        }
        let authority = required_text(object, "authority")?;
        let attested = match authority.as_str() {
            "TRUSTED_CI" => {
                let trust = trust.ok_or(ClientError::InvalidField("authorityProof"))?;
                verify_authority_proof(object, trust)?;
                true
            }
            "LOCAL_UNATTESTED" => {
                if object.contains_key("proof") {
                    return Err(ClientError::InvalidField("proof"));
                }
                false
            }
            _ => return Err(ClientError::InvalidField("authority")),
        };
        Ok(Self {
            gate_decision: GateDecisionLabel::parse(&required_text(object, "gateDecision")?)?,
            attested,
            rule_id: required_text(object, "ruleId")?,
            binding_digest: required_sha256_digest(object, "bindingDigest")?,
            finding_fingerprint: required_text(object, "findingFingerprint")?,
        })
    }

    fn label(&self) -> AuthorityLabel {
        if self.gate_decision == GateDecisionLabel::Inconclusive {
            AuthorityLabel::Inconclusive
        } else if self.attested {
            AuthorityLabel::TrustedCi
        } else {
            AuthorityLabel::LocalUnattested
        }
    }
}

fn verify_authority_proof(object: &Map<String, Value>, trust: &AuthorityTrust) -> Result<(), ClientError> {
    let proof = object.get("proof").and_then(Value::as_object).ok_or(ClientError::InvalidField("authorityProof"))?;
    require_exact_fields(proof, &["payloadType", "payloadHex", "keyId", "signatureHex", "role", "issuedAtEpochSecs", "expiresAtEpochSecs"])?;
    let payload_type = required_text(proof, "payloadType")?;
    let payload = decode_hex(required_text(proof, "payloadHex")?.as_str(), "payloadHex", 64 * 1024)?;
    let key_id = required_text(proof, "keyId")?;
    let signature = decode_hex(required_text(proof, "signatureHex")?.as_str(), "signatureHex", 64)?;
    let role = required_text(proof, "role")?;
    let issued_at_epoch_secs = proof.get("issuedAtEpochSecs").and_then(Value::as_u64).ok_or(ClientError::InvalidField("issuedAtEpochSecs"))?;
    let expires_at_epoch_secs = proof.get("expiresAtEpochSecs").and_then(Value::as_u64).ok_or(ClientError::InvalidField("expiresAtEpochSecs"))?;
    if payload_type != DSSE_PAYLOAD_TYPE || role != trust.role {
        return Err(ClientError::InvalidField("authorityProof"));
    }
    if trust.now_epoch_secs < issued_at_epoch_secs || trust.now_epoch_secs >= expires_at_epoch_secs {
        return Err(ClientError::InvalidField("authorityProof"));
    }
    let expected = AuthorityContext::authority_payload(
        &required_text(object, "gateDecision")?,
        &required_text(object, "ruleId")?,
        &required_text(object, "bindingDigest")?,
        &required_text(object, "findingFingerprint")?,
        issued_at_epoch_secs,
        expires_at_epoch_secs,
    )?;
    if payload != expected {
        return Err(ClientError::InvalidField("authorityProof"));
    }
    let asset = SignedAsset { payload_type, payload, key_id, signature };
    trust
        .store
        .verify_with_metadata(&asset, &trust.role, &trust.metadata, trust.now_epoch_secs)
        .map_err(|_| ClientError::InvalidField("authorityProof"))?;
    Ok(())
}

fn canonical_authority_payload(
    gate_decision: GateDecisionLabel,
    rule_id: &str,
    binding_digest: &str,
    finding_fingerprint: &str,
    issued_at_epoch_secs: u64,
    expires_at_epoch_secs: u64,
) -> Vec<u8> {
    let mut payload = AUTHORITY_PAYLOAD_PREFIX.to_vec();
    payload.push(match gate_decision {
        GateDecisionLabel::Pass => 0,
        GateDecisionLabel::Fail => 1,
        GateDecisionLabel::Inconclusive => 2,
    });
    for value in [rule_id, binding_digest, finding_fingerprint] {
        payload.extend_from_slice(&(value.len() as u64).to_be_bytes());
        payload.extend_from_slice(value.as_bytes());
    }
    payload.extend_from_slice(&issued_at_epoch_secs.to_be_bytes());
    payload.extend_from_slice(&expires_at_epoch_secs.to_be_bytes());
    payload
}

fn decode_hex(value: &str, field: &'static str, max_bytes: usize) -> Result<Vec<u8>, ClientError> {
    if value.is_empty() || !value.len().is_multiple_of(2) || value.len() / 2 > max_bytes || !value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(ClientError::InvalidField(field));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| ClientError::InvalidField(field)))
        .collect()
}

/// Client-facing baseline and exception assessment of a finding. Optional; when
/// present it is surfaced verbatim and never rewrites the authority decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingAssessmentView {
    baseline: String,
    exception: String,
}

impl FindingAssessmentView {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let text = fs::read_to_string(path).map_err(|error| ClientError::Io(error.to_string()))?;
        let json: Value = serde_json::from_str(&text).map_err(|error| ClientError::InvalidJson(error.to_string()))?;
        Self::from_json(&json)
    }

    pub fn from_json(json: &Value) -> Result<Self, ClientError> {
        reject_forbidden_fields(json, "assessment")?;
        let object = json.as_object().ok_or(ClientError::InvalidShape("assessment must be an object"))?;
        require_exact_fields(object, &["apiVersion", "kind", "baseline", "exception"])?;
        if required_text(object, "apiVersion")? != CLIENT_API_VERSION {
            return Err(ClientError::InvalidField("apiVersion"));
        }
        if required_text(object, "kind")? != FINDING_ASSESSMENT_KIND {
            return Err(ClientError::InvalidField("kind"));
        }
        let baseline = match required_text(object, "baseline")?.as_str() {
            value @ ("NEW" | "HISTORICAL") => value.to_owned(),
            _ => return Err(ClientError::InvalidField("baseline")),
        };
        let exception = match required_text(object, "exception")?.as_str() {
            value @ ("NOT_APPLICABLE" | "EXEMPTED") => value.to_owned(),
            _ => return Err(ClientError::InvalidField("exception")),
        };
        Ok(Self { baseline, exception })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RuleSection {
    #[serde(rename = "ruleVersion")]
    rule_version: String,
    severity: String,
    status: String,
    rationale: String,
    remediation: String,
    #[serde(rename = "standardId")]
    standard_id: String,
    #[serde(rename = "standardVersion")]
    standard_version: String,
    #[serde(rename = "bindingDigest")]
    binding_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AssessmentSection {
    baseline: String,
    exception: String,
}

/// A rich explanation. `authoritative` is true only when active, identity
/// matched rule metadata backed it; otherwise it is a provisional view carrying
/// a `next_step` toward an authoritative explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RichExplanation {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    authoritative: bool,
    #[serde(rename = "findingId")]
    finding_id: String,
    #[serde(rename = "ruleId")]
    rule_id: String,
    fingerprint: String,
    message: String,
    location: FindingLocation,
    #[serde(rename = "findingDigest")]
    finding_digest: String,
    #[serde(rename = "authorityStatus")]
    authority_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    rule: Option<RuleSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assessment: Option<AssessmentSection>,
    #[serde(rename = "nextStep", skip_serializing_if = "Option::is_none")]
    next_step: Option<String>,
}

impl RichExplanation {
    fn new(
        finding: &FindingView,
        authoritative: bool,
        label: AuthorityLabel,
        rule: Option<RuleSection>,
        assessment: Option<AssessmentSection>,
        next_step: Option<String>,
    ) -> Self {
        Self {
            api_version: CLIENT_API_VERSION.to_owned(),
            kind: RICH_EXPLANATION_KIND.to_owned(),
            schema_version: RICH_EXPLANATION_VERSION.to_owned(),
            authoritative,
            finding_id: finding.finding_id.clone(),
            rule_id: finding.rule_id.clone(),
            fingerprint: finding.fingerprint.clone(),
            message: finding.message.clone(),
            location: finding.location.clone(),
            finding_digest: finding.digest(),
            authority_status: label.as_str().to_owned(),
            rule,
            assessment,
            next_step,
        }
    }

    fn digest(&self) -> String {
        digest_json(self)
    }

    fn to_json(&self) -> Value {
        let mut object = serde_json::to_value(self).expect("RichExplanation is serializable");
        object["explanationDigest"] = Value::String(self.digest());
        object
    }

    fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "finding_id={}", self.finding_id);
        let _ = writeln!(out, "rule_id={}", self.rule_id);
        let _ = writeln!(out, "fingerprint={}", self.fingerprint);
        let _ = writeln!(out, "message={}", self.message);
        let _ = writeln!(out, "location={}:{}:{}", self.location.path, self.location.line, self.location.column);
        let _ = writeln!(out, "finding_digest={}", self.finding_digest);
        let _ = writeln!(out, "authoritative={}", self.authoritative);
        let _ = writeln!(out, "authority_status={}", self.authority_status);
        if let Some(rule) = &self.rule {
            let _ = writeln!(out, "rule_version={}", rule.rule_version);
            let _ = writeln!(out, "severity={}", rule.severity);
            let _ = writeln!(out, "rule_status={}", rule.status);
            let _ = writeln!(out, "standard={}@{}", rule.standard_id, rule.standard_version);
            let _ = writeln!(out, "binding_digest={}", rule.binding_digest);
            let _ = writeln!(out, "rationale={}", rule.rationale);
            let _ = writeln!(out, "remediation={}", rule.remediation);
        }
        if let Some(assessment) = &self.assessment {
            let _ = writeln!(out, "baseline={}", assessment.baseline);
            let _ = writeln!(out, "exception={}", assessment.exception);
        }
        let _ = writeln!(out, "{ACTIONS_LINE}");
        let _ = writeln!(out, "explanation_digest={}", self.digest());
        if let Some(next) = &self.next_step {
            let _ = writeln!(out, "next_step={next}");
        }
        out
    }
}

/// A fail-closed refusal to produce an authoritative explanation, carrying a
/// typed reason and a concrete next step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RichExplanationRefusal {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    authoritative: bool,
    refused: bool,
    reason: String,
    #[serde(rename = "findingId")]
    finding_id: String,
    #[serde(rename = "ruleId")]
    rule_id: String,
    #[serde(rename = "nextStep")]
    next_step: String,
}

impl RichExplanationRefusal {
    fn digest(&self) -> String {
        digest_json(self)
    }

    fn to_json(&self) -> Value {
        let mut object = serde_json::to_value(self).expect("RichExplanationRefusal is serializable");
        object["refusalDigest"] = Value::String(self.digest());
        object
    }

    fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "finding_id={}", self.finding_id);
        let _ = writeln!(out, "rule_id={}", self.rule_id);
        let _ = writeln!(out, "authoritative=false");
        let _ = writeln!(out, "refused=true");
        let _ = writeln!(out, "reason={}", self.reason);
        let _ = writeln!(out, "next_step={}", self.next_step);
        let _ = writeln!(out, "refusal_digest={}", self.digest());
        out
    }
}

/// The outcome of an explain request: either an explanation (authoritative or
/// provisional) or a fail-closed refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum ExplainOutcome {
    Explained(RichExplanation),
    Refused(RichExplanationRefusal),
}

impl ExplainOutcome {
    pub fn is_authoritative(&self) -> bool {
        matches!(self, ExplainOutcome::Explained(explanation) if explanation.authoritative)
    }

    pub fn is_refusal(&self) -> bool {
        matches!(self, ExplainOutcome::Refused(_))
    }

    pub fn exit_code(&self) -> i32 {
        if self.is_refusal() {
            3
        } else {
            0
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            ExplainOutcome::Explained(explanation) => explanation.to_json(),
            ExplainOutcome::Refused(refusal) => refusal.to_json(),
        }
    }

    pub fn render_text(&self) -> String {
        match self {
            ExplainOutcome::Explained(explanation) => explanation.render(),
            ExplainOutcome::Refused(refusal) => refusal.render(),
        }
    }
}

/// Explain a finding, layering verified metadata and authority status on top of
/// the digest-only view, or fail closed with a typed refusal.
///
/// An authoritative explanation requires active rule metadata whose `ruleId`
/// matches the finding; when an authority context is supplied it must also bind
/// to the same rule, fingerprint and binding digest. Without metadata the result
/// is a provisional (non-authoritative) view, unless an authority context is
/// present, which signals intent to obtain an authoritative explanation and thus
/// refuses as `METADATA_MISSING`.
pub fn explain_finding(
    finding: &FindingView,
    metadata: Option<&RuleExplanationMetadata>,
    authority: Option<&AuthorityContext>,
    assessment: Option<&FindingAssessmentView>,
) -> ExplainOutcome {
    let Some(metadata) = metadata else {
        if authority.is_some() {
            return refuse(finding, RefusalReason::MetadataMissing);
        }
        return ExplainOutcome::Explained(RichExplanation::new(
            finding,
            false,
            AuthorityLabel::LocalUnattested,
            None,
            assessment.map(assessment_section),
            Some("提供经验证且与 Finding identity 匹配的规则元数据 (--rule-meta) 以生成权威解释".to_owned()),
        ));
    };
    match metadata.status {
        MetadataStatus::Expired => return refuse(finding, RefusalReason::MetadataExpired),
        MetadataStatus::Revoked => return refuse(finding, RefusalReason::MetadataRevoked),
        MetadataStatus::Active => {}
    }
    if metadata.rule_id != finding.rule_id {
        return refuse(finding, RefusalReason::RuleIdentityMismatch);
    }
    if let Some(authority) = authority {
        if authority.rule_id != finding.rule_id {
            return refuse(finding, RefusalReason::RuleIdentityMismatch);
        }
        if authority.finding_fingerprint != finding.fingerprint {
            return refuse(finding, RefusalReason::DigestMismatch);
        }
        if authority.binding_digest != metadata.binding_digest {
            return refuse(finding, RefusalReason::DigestMismatch);
        }
    }
    let label = authority.map(AuthorityContext::label).unwrap_or(AuthorityLabel::LocalUnattested);
    let rule = RuleSection {
        rule_version: metadata.rule_version.clone(),
        severity: metadata.severity.as_str().to_owned(),
        status: metadata.status.as_str().to_owned(),
        rationale: metadata.rationale.clone(),
        remediation: metadata.remediation.clone(),
        standard_id: metadata.standard_id.clone(),
        standard_version: metadata.standard_version.clone(),
        binding_digest: metadata.binding_digest.clone(),
    };
    ExplainOutcome::Explained(RichExplanation::new(
        finding,
        true,
        label,
        Some(rule),
        assessment.map(assessment_section),
        None,
    ))
}

fn assessment_section(view: &FindingAssessmentView) -> AssessmentSection {
    AssessmentSection {
        baseline: view.baseline.clone(),
        exception: view.exception.clone(),
    }
}

fn refuse(finding: &FindingView, reason: RefusalReason) -> ExplainOutcome {
    ExplainOutcome::Refused(RichExplanationRefusal {
        api_version: CLIENT_API_VERSION.to_owned(),
        kind: RICH_EXPLANATION_REFUSAL_KIND.to_owned(),
        schema_version: RICH_EXPLANATION_VERSION.to_owned(),
        authoritative: false,
        refused: true,
        reason: reason.as_str().to_owned(),
        finding_id: finding.finding_id.clone(),
        rule_id: finding.rule_id.clone(),
        next_step: reason.next_step().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FINDING_VIEW_KIND;
    use ed25519_dalek::{Signer, SigningKey};

    const BINDING: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OTHER_BINDING: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const RULE_ID: &str = "corp.java.no-system-exit";
    const FINGERPRINT: &str = "fp-1";

    fn finding() -> FindingView {
        FindingView::from_json(&json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": FINDING_VIEW_KIND,
            "findingId": "finding-1",
            "ruleId": RULE_ID,
            "fingerprint": FINGERPRINT,
            "message": "System.exit is forbidden",
            "location": {"path": "src/main/java/Main.java", "line": 12, "column": 5}
        }))
        .unwrap()
    }

    fn signed_metadata(status: &str, rule_id: &str, binding: &str) -> Value {
        RuleExplanationMetadata::sign(&json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": RULE_METADATA_KIND,
            "schemaVersion": RULE_METADATA_VERSION,
            "ruleId": rule_id,
            "ruleVersion": "1.4.0",
            "severity": "blocking",
            "status": status,
            "rationale": "System.exit ends the JVM abruptly and bypasses orderly shutdown.",
            "remediation": "Throw a checked exception and let the caller decide how to exit.",
            "standardId": "SEC-JAVA-017",
            "standardVersion": "2.1.0",
            "bindingDigest": binding
        }))
        .unwrap()
    }

    fn metadata(status: &str, rule_id: &str, binding: &str) -> RuleExplanationMetadata {
        RuleExplanationMetadata::from_json(&signed_metadata(status, rule_id, binding)).unwrap()
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn authority_trust() -> AuthorityTrust {
        let key = SigningKey::from_bytes(&[77; 32]);
        AuthorityTrust::from_json_at(&json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": AUTHORITY_TRUST_KIND,
            "role": AUTHORITY_SIGNING_ROLE,
            "keys": [{"keyId": "gate-key", "publicKeyHex": hex(&key.verifying_key().to_bytes())}],
            "metadata": {"version": 1, "expiresAtEpochSecs": 1000, "revokedKeyIds": []}
        }), 100)
        .unwrap()
    }

    fn authority(gate: &str, attested: &str, rule_id: &str, binding: &str, fingerprint: &str) -> AuthorityContext {
        let mut value = json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": AUTHORITY_CONTEXT_KIND,
            "gateDecision": gate,
            "authority": if attested == "TRUSTED" { "TRUSTED_CI" } else { attested },
            "ruleId": rule_id,
            "bindingDigest": binding,
            "findingFingerprint": fingerprint
        });
        if attested == "TRUSTED" {
            let key = SigningKey::from_bytes(&[77; 32]);
            let payload = AuthorityContext::authority_payload(gate, rule_id, binding, fingerprint, 100, 1000).unwrap();
            let signature = key.sign(&erf_trust::pae(DSSE_PAYLOAD_TYPE, &payload));
            value["proof"] = json!({
                "payloadType": DSSE_PAYLOAD_TYPE,
                "payloadHex": hex(&payload),
                "keyId": "gate-key",
                "signatureHex": hex(&signature.to_bytes()),
                "role": AUTHORITY_SIGNING_ROLE,
                "issuedAtEpochSecs": 100,
                "expiresAtEpochSecs": 1000
            });
            AuthorityContext::from_json_with_trust(&value, &authority_trust()).unwrap()
        } else {
            AuthorityContext::from_json(&value).unwrap()
        }
    }

    fn assessment(baseline: &str, exception: &str) -> FindingAssessmentView {
        FindingAssessmentView::from_json(&json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": FINDING_ASSESSMENT_KIND,
            "baseline": baseline,
            "exception": exception
        }))
        .unwrap()
    }

    #[test]
    fn authoritative_when_metadata_and_authority_identity_match() {
        let meta = metadata("active", RULE_ID, BINDING);
        let ctx = authority("FAIL", "TRUSTED", RULE_ID, BINDING, FINGERPRINT);
        let view = assessment("HISTORICAL", "NOT_APPLICABLE");
        let outcome = explain_finding(&finding(), Some(&meta), Some(&ctx), Some(&view));
        assert!(outcome.is_authoritative());
        assert_eq!(outcome.exit_code(), 0);
        let json = outcome.to_json();
        assert_eq!(json["authoritative"], true);
        assert_eq!(json["authorityStatus"], "TRUSTED_CI");
        assert_eq!(json["rule"]["ruleVersion"], "1.4.0");
        assert_eq!(json["rule"]["severity"], "blocking");
        assert_eq!(json["rule"]["bindingDigest"], BINDING);
        assert_eq!(json["assessment"]["baseline"], "HISTORICAL");
        assert!(json["explanationDigest"].as_str().unwrap().starts_with("sha256:"));
        assert!(json.get("nextStep").is_none());
    }

    #[test]
    fn provisional_without_metadata_is_local_unattested_and_never_refuses() {
        let outcome = explain_finding(&finding(), None, None, None);
        assert!(!outcome.is_authoritative());
        assert!(!outcome.is_refusal());
        assert_eq!(outcome.exit_code(), 0);
        let json = outcome.to_json();
        assert_eq!(json["authoritative"], false);
        assert_eq!(json["authorityStatus"], "LOCAL_UNATTESTED");
        assert!(json.get("rule").is_none());
        assert!(json["nextStep"].as_str().is_some());
        assert!(outcome.render_text().contains("location=src/main/java/Main.java:12:5"));
    }

    #[test]
    fn authority_without_metadata_refuses_as_missing() {
        let ctx = authority("PASS", "TRUSTED", RULE_ID, BINDING, FINGERPRINT);
        let outcome = explain_finding(&finding(), None, Some(&ctx), None);
        assert!(outcome.is_refusal());
        assert_eq!(outcome.exit_code(), 3);
        assert_eq!(outcome.to_json()["reason"], "METADATA_MISSING");
    }

    #[test]
    fn expired_and_revoked_metadata_fail_closed() {
        let expired = metadata("expired", RULE_ID, BINDING);
        let outcome = explain_finding(&finding(), Some(&expired), None, None);
        assert_eq!(outcome.to_json()["reason"], "METADATA_EXPIRED");
        assert_eq!(outcome.exit_code(), 3);
        let revoked = metadata("revoked", RULE_ID, BINDING);
        let outcome = explain_finding(&finding(), Some(&revoked), None, None);
        assert_eq!(outcome.to_json()["reason"], "METADATA_REVOKED");
    }

    #[test]
    fn rule_identity_mismatch_fails_closed() {
        let meta = metadata("active", "corp.java.other-rule", BINDING);
        let outcome = explain_finding(&finding(), Some(&meta), None, None);
        assert_eq!(outcome.to_json()["reason"], "RULE_IDENTITY_MISMATCH");
        assert_eq!(outcome.exit_code(), 3);
    }

    #[test]
    fn binding_and_fingerprint_mismatch_fail_closed() {
        let meta = metadata("active", RULE_ID, BINDING);
        let wrong_binding = authority("PASS", "TRUSTED", RULE_ID, OTHER_BINDING, FINGERPRINT);
        let outcome = explain_finding(&finding(), Some(&meta), Some(&wrong_binding), None);
        assert_eq!(outcome.to_json()["reason"], "DIGEST_MISMATCH");
        let wrong_fp = authority("PASS", "TRUSTED", RULE_ID, BINDING, "fp-different");
        let outcome = explain_finding(&finding(), Some(&meta), Some(&wrong_fp), None);
        assert_eq!(outcome.to_json()["reason"], "DIGEST_MISMATCH");
    }

    #[test]
    fn inconclusive_gate_collapses_to_inconclusive_label() {
        let meta = metadata("active", RULE_ID, BINDING);
        let ctx = authority("INCONCLUSIVE", "TRUSTED", RULE_ID, BINDING, FINGERPRINT);
        let outcome = explain_finding(&finding(), Some(&meta), Some(&ctx), None);
        assert!(outcome.is_authoritative());
        assert_eq!(outcome.to_json()["authorityStatus"], "INCONCLUSIVE");
    }

    #[test]
    fn local_unattested_authority_keeps_local_label() {
        let meta = metadata("active", RULE_ID, BINDING);
        let ctx = authority("PASS", "LOCAL_UNATTESTED", RULE_ID, BINDING, FINGERPRINT);
        let outcome = explain_finding(&finding(), Some(&meta), Some(&ctx), None);
        assert_eq!(outcome.to_json()["authorityStatus"], "LOCAL_UNATTESTED");
    }

    #[test]
    fn trusted_status_cannot_be_self_declared() {
        let json = json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": AUTHORITY_CONTEXT_KIND,
            "gateDecision": "PASS",
            "authority": "TRUSTED_CI",
            "ruleId": RULE_ID,
            "bindingDigest": BINDING,
            "findingFingerprint": FINGERPRINT
        });
        assert_eq!(AuthorityContext::from_json(&json), Err(ClientError::InvalidField("authorityProof")));
    }

    #[test]
    fn tampered_metadata_and_forbidden_fields_fail_closed() {
        let mut signed = signed_metadata("active", RULE_ID, BINDING);
        signed["digest"] = Value::String(OTHER_BINDING.to_owned());
        assert_eq!(RuleExplanationMetadata::from_json(&signed), Err(ClientError::InvalidField("digest")));
        let mut with_source = signed_metadata("active", RULE_ID, BINDING);
        with_source["sourceSnippet"] = Value::String("secret".to_owned());
        assert!(matches!(RuleExplanationMetadata::from_json(&with_source), Err(ClientError::ForbiddenPrivacyField(_))));
    }

    #[test]
    fn explanation_digest_is_stable_across_identical_requests() {
        let meta = metadata("active", RULE_ID, BINDING);
        let ctx = authority("PASS", "TRUSTED", RULE_ID, BINDING, FINGERPRINT);
        let first = explain_finding(&finding(), Some(&meta), Some(&ctx), None).to_json();
        let second = explain_finding(&finding(), Some(&meta), Some(&ctx), None).to_json();
        assert_eq!(first, second);
        assert_eq!(first["explanationDigest"], second["explanationDigest"]);
    }

    #[test]
    fn refusal_text_and_json_carry_reason_and_next_step() {
        let expired = metadata("expired", RULE_ID, BINDING);
        let outcome = explain_finding(&finding(), Some(&expired), None, None);
        let text = outcome.render_text();
        assert!(text.contains("refused=true"));
        assert!(text.contains("reason=METADATA_EXPIRED"));
        assert!(text.contains("next_step="));
        let json = outcome.to_json();
        assert_eq!(json["refused"], true);
        assert!(json["nextStep"].as_str().is_some());
        assert!(json["refusalDigest"].as_str().unwrap().starts_with("sha256:"));
    }
}
