//! Thin-client request boundary for digest-only finding UX.
//!
//! The client can explain a finding, prepare digest-only feedback/exception
//! requests, and explicitly submit them to a selected Server. It cannot
//! approve an exception, alter a Gate decision, or carry source content.

use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use thiserror::Error;

mod explain;
mod transport;

pub use explain::{
    explain_finding, AuthorityContext, AuthorityLabel, AuthorityTrust, ExplainOutcome, FindingAssessmentView,
    MetadataStatus, RefusalReason, RichExplanation, RichExplanationRefusal,
    RuleExplanationMetadata, Severity, RICH_EXPLANATION_KIND, RICH_EXPLANATION_REFUSAL_KIND,
    RICH_EXPLANATION_VERSION, RULE_METADATA_KIND, RULE_METADATA_VERSION,
};
pub use transport::{
    query_submission_status, submit_exception, submit_feedback, SubmissionAuditContext,
    SubmissionReceipt, SubmissionResource, SubmissionStatus,
};

pub const CLIENT_API_VERSION: &str = "rulefactory.io/v1";
pub const FINDING_VIEW_KIND: &str = "FindingView";
pub const FINDING_VIEW_VERSION: &str = "finding-view-v1";
pub const FEEDBACK_DRAFT_KIND: &str = "FeedbackDraft";
pub const FEEDBACK_DRAFT_VERSION: &str = "feedback-draft-v1";
pub const EXCEPTION_REQUEST_KIND: &str = "ExceptionRequest";
pub const EXCEPTION_REQUEST_VERSION: &str = "exception-request-v1";

const FORBIDDEN_FIELDS: &[&str] = &[
    "source", "sourceCode", "sourceContent", "sourceSnippet", "fullFinding", "findings",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindingLocation {
    pub path: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindingView {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(rename = "findingId")]
    pub finding_id: String,
    #[serde(rename = "ruleId")]
    pub rule_id: String,
    pub fingerprint: String,
    pub message: String,
    pub location: FindingLocation,
}

impl FindingView {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let text = fs::read_to_string(path).map_err(|error| ClientError::Io(error.to_string()))?;
        let json: Value = serde_json::from_str(&text).map_err(|error| ClientError::InvalidJson(error.to_string()))?;
        Self::from_json(&json)
    }

    pub fn from_json(json: &Value) -> Result<Self, ClientError> {
        reject_forbidden_fields(json, "finding")?;
        let object = json.as_object().ok_or(ClientError::InvalidShape("finding must be an object"))?;
        require_exact_fields(
            object,
            &["apiVersion", "kind", "findingId", "ruleId", "fingerprint", "message", "location"],
        )?;
        let api_version = required_text(object, "apiVersion")?;
        if api_version != CLIENT_API_VERSION {
            return Err(ClientError::InvalidField("apiVersion"));
        }
        if required_text(object, "kind")? != FINDING_VIEW_KIND {
            return Err(ClientError::InvalidField("kind"));
        }
        let location = object
            .get("location")
            .and_then(Value::as_object)
            .ok_or(ClientError::InvalidShape("location must be an object"))?;
        require_exact_fields(location, &["path", "line", "column"])?;
        let path = required_text(location, "path")?;
        validate_relative_path(&path, "location.path")?;
        let line = required_positive_u32(location, "line")?;
        let column = required_positive_u32(location, "column")?;
        Ok(Self {
            api_version,
            kind: FINDING_VIEW_KIND.into(),
            finding_id: required_text(object, "findingId")?,
            rule_id: required_text(object, "ruleId")?,
            fingerprint: required_text(object, "fingerprint")?,
            message: required_text(object, "message")?,
            location: FindingLocation { path, line, column },
        })
    }

    pub fn digest(&self) -> String {
        digest_json(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeedbackDraft {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub status: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "findingDigest")]
    pub finding_digest: String,
    #[serde(rename = "findingFingerprint")]
    pub finding_fingerprint: String,
    #[serde(rename = "ruleId")]
    pub rule_id: String,
    pub classification: String,
    pub reason: String,
    pub actor: String,
}

impl FeedbackDraft {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let text = fs::read_to_string(path).map_err(|error| ClientError::Io(error.to_string()))?;
        let json: Value = serde_json::from_str(&text).map_err(|error| ClientError::InvalidJson(error.to_string()))?;
        Self::from_json(&json)
    }

    pub fn from_json(json: &Value) -> Result<Self, ClientError> {
        reject_forbidden_fields(json, "feedback")?;
        let object = json.as_object().ok_or(ClientError::InvalidShape("feedback must be an object"))?;
        require_exact_fields(
            object,
            &[
                "apiVersion", "kind", "schemaVersion", "status", "requestId", "findingDigest",
                "findingFingerprint", "ruleId", "classification", "reason", "actor", "digest",
            ],
        )?;
        let draft = Self {
            api_version: required_text(object, "apiVersion")?,
            kind: required_text(object, "kind")?,
            schema_version: required_text(object, "schemaVersion")?,
            status: required_text(object, "status")?,
            request_id: required_sha256_digest(object, "requestId")?,
            finding_digest: required_sha256_digest(object, "findingDigest")?,
            finding_fingerprint: required_text(object, "findingFingerprint")?,
            rule_id: required_text(object, "ruleId")?,
            classification: required_text(object, "classification")?,
            reason: required_text(object, "reason")?,
            actor: required_text(object, "actor")?,
        };
        if draft.api_version != CLIENT_API_VERSION
            || draft.kind != FEEDBACK_DRAFT_KIND
            || draft.schema_version != FEEDBACK_DRAFT_VERSION
            || draft.status != "PROPOSED"
            || !matches!(draft.classification.as_str(), "true-fp" | "not-fp" | "scope-issue" | "documentation")
        {
            return Err(ClientError::InvalidField("feedback contract"));
        }
        validate_context(&draft.reason, "reason")?;
        validate_context(&draft.actor, "actor")?;
        let supplied_digest = required_sha256_digest(object, "digest")?;
        if supplied_digest != draft.digest() {
            return Err(ClientError::InvalidField("digest"));
        }
        if draft.request_id != request_envelope_digest(&draft) {
            return Err(ClientError::RequestIdMismatch("feedback"));
        }
        Ok(draft)
    }

    pub fn digest(&self) -> String {
        digest_json(self)
    }

    pub fn as_json(&self) -> Value {
        let mut object = serde_json::to_value(self).expect("FeedbackDraft is serializable");
        object["digest"] = Value::String(self.digest());
        object
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExceptionRequest {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub status: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "findingDigest")]
    pub finding_digest: String,
    #[serde(rename = "findingFingerprint")]
    pub finding_fingerprint: String,
    #[serde(rename = "ruleId")]
    pub rule_id: String,
    pub scope: String,
    pub reason: String,
    pub actor: String,
}

impl ExceptionRequest {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ClientError> {
        let text = fs::read_to_string(path).map_err(|error| ClientError::Io(error.to_string()))?;
        let json: Value = serde_json::from_str(&text).map_err(|error| ClientError::InvalidJson(error.to_string()))?;
        Self::from_json(&json)
    }

    pub fn from_json(json: &Value) -> Result<Self, ClientError> {
        reject_forbidden_fields(json, "exception")?;
        let object = json.as_object().ok_or(ClientError::InvalidShape("exception must be an object"))?;
        require_exact_fields(
            object,
            &[
                "apiVersion", "kind", "schemaVersion", "status", "requestId", "findingDigest",
                "findingFingerprint", "ruleId", "scope", "reason", "actor", "digest",
            ],
        )?;
        let request = Self {
            api_version: required_text(object, "apiVersion")?,
            kind: required_text(object, "kind")?,
            schema_version: required_text(object, "schemaVersion")?,
            status: required_text(object, "status")?,
            request_id: required_sha256_digest(object, "requestId")?,
            finding_digest: required_sha256_digest(object, "findingDigest")?,
            finding_fingerprint: required_text(object, "findingFingerprint")?,
            rule_id: required_text(object, "ruleId")?,
            scope: required_text(object, "scope")?,
            reason: required_text(object, "reason")?,
            actor: required_text(object, "actor")?,
        };
        if request.api_version != CLIENT_API_VERSION
            || request.kind != EXCEPTION_REQUEST_KIND
            || request.schema_version != EXCEPTION_REQUEST_VERSION
            || request.status != "REQUESTED"
        {
            return Err(ClientError::InvalidField("exception contract"));
        }
        validate_relative_path(&request.scope, "scope")?;
        if request.scope == "**" || request.scope == "*" {
            return Err(ClientError::InvalidField("scope"));
        }
        validate_context(&request.reason, "reason")?;
        validate_context(&request.actor, "actor")?;
        let supplied_digest = required_sha256_digest(object, "digest")?;
        if supplied_digest != request.digest() {
            return Err(ClientError::InvalidField("digest"));
        }
        if request.request_id != request_envelope_digest(&request) {
            return Err(ClientError::RequestIdMismatch("exception"));
        }
        Ok(request)
    }

    pub fn digest(&self) -> String {
        digest_json(self)
    }

    pub fn as_json(&self) -> Value {
        let mut object = serde_json::to_value(self).expect("ExceptionRequest is serializable");
        object["digest"] = Value::String(self.digest());
        object
    }
}

pub fn make_feedback_draft(
    finding: &FindingView,
    classification: &str,
    reason: &str,
    actor: &str,
) -> Result<FeedbackDraft, ClientError> {
    if !matches!(classification, "true-fp" | "not-fp" | "scope-issue" | "documentation") {
        return Err(ClientError::InvalidField("classification"));
    }
    validate_context(reason, "reason")?;
    validate_context(actor, "actor")?;
    let mut draft = FeedbackDraft {
        api_version: CLIENT_API_VERSION.into(),
        kind: FEEDBACK_DRAFT_KIND.into(),
        schema_version: FEEDBACK_DRAFT_VERSION.into(),
        status: "PROPOSED".into(),
        request_id: String::new(),
        finding_digest: finding.digest(),
        finding_fingerprint: finding.fingerprint.clone(),
        rule_id: finding.rule_id.clone(),
        classification: classification.into(),
        reason: reason.into(),
        actor: actor.into(),
    };
    draft.request_id = request_envelope_digest(&draft);
    Ok(draft)
}

pub fn make_exception_request(
    finding: &FindingView,
    scope: &str,
    reason: &str,
    actor: &str,
) -> Result<ExceptionRequest, ClientError> {
    validate_relative_path(scope, "scope")?;
    if scope == "**" || scope == "*" {
        return Err(ClientError::InvalidField("scope"));
    }
    validate_context(reason, "reason")?;
    validate_context(actor, "actor")?;
    let mut request = ExceptionRequest {
        api_version: CLIENT_API_VERSION.into(),
        kind: EXCEPTION_REQUEST_KIND.into(),
        schema_version: EXCEPTION_REQUEST_VERSION.into(),
        status: "REQUESTED".into(),
        request_id: String::new(),
        finding_digest: finding.digest(),
        finding_fingerprint: finding.fingerprint.clone(),
        rule_id: finding.rule_id.clone(),
        scope: scope.into(),
        reason: reason.into(),
        actor: actor.into(),
    };
    request.request_id = request_envelope_digest(&request);
    Ok(request)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ClientError {
    #[error("client I/O failed: {0}")]
    Io(String),
    #[error("invalid JSON: {0}")]
    InvalidJson(String),
    #[error("invalid client input shape: {0}")]
    InvalidShape(&'static str),
    #[error("invalid client field: {0}")]
    InvalidField(&'static str),
    #[error("request identity does not match the canonical request envelope: {0}")]
    RequestIdMismatch(&'static str),
    #[error("client input includes forbidden privacy field: {0}")]
    ForbiddenPrivacyField(String),
    #[error("client output serialization failed: {0}")]
    Serialization(String),
    #[error("invalid server endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("server rejected submission (HTTP {status}, {code}): {message}")]
    SubmissionRejected {
        status: u16,
        code: String,
        message: String,
    },
    #[error("submission outcome is unknown: {0}")]
    SubmissionUnknown(String),
    #[error("server response is invalid: {0}")]
    ResponseInvalid(String),
}

pub(crate) fn require_exact_fields(object: &Map<String, Value>, allowed: &[&str]) -> Result<(), ClientError> {
    if object.keys().any(|key| !allowed.contains(&key.as_str()))
        || allowed.iter().any(|key| !object.contains_key(*key))
    {
        return Err(ClientError::InvalidShape("input fields are not the closed contract"));
    }
    Ok(())
}

pub(crate) fn required_text(object: &Map<String, Value>, field: &'static str) -> Result<String, ClientError> {
    let value = object.get(field).and_then(Value::as_str).ok_or(ClientError::InvalidField(field))?;
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(ClientError::InvalidField(field));
    }
    Ok(value.to_owned())
}

pub(crate) fn required_sha256_digest(object: &Map<String, Value>, field: &'static str) -> Result<String, ClientError> {
    let value = required_text(object, field)?;
    if !is_sha256_digest(&value) {
        return Err(ClientError::InvalidField(field));
    }
    Ok(value)
}

pub(crate) fn is_sha256_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn required_positive_u32(object: &Map<String, Value>, field: &'static str) -> Result<u32, ClientError> {
    let value = object.get(field).and_then(Value::as_u64).ok_or(ClientError::InvalidField(field))?;
    u32::try_from(value).ok().filter(|value| *value > 0).ok_or(ClientError::InvalidField(field))
}

fn validate_context(value: &str, field: &'static str) -> Result<(), ClientError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) || value.len() > 2000 {
        return Err(ClientError::InvalidField(field));
    }
    Ok(())
}

fn validate_relative_path(value: &str, field: &'static str) -> Result<(), ClientError> {
    if value.trim().is_empty()
        || value != value.trim()
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains(':')
        || value.chars().any(char::is_control)
        || value.split('/').any(|segment| segment == "." || segment == ".." || segment.is_empty())
    {
        return Err(ClientError::InvalidField(field));
    }
    Ok(())
}

pub(crate) fn reject_forbidden_fields(value: &Value, path: &str) -> Result<(), ClientError> {
    match value {
        Value::Object(object) => {
            for (field, child) in object {
                if FORBIDDEN_FIELDS.contains(&field.as_str()) {
                    return Err(ClientError::ForbiddenPrivacyField(format!("{path}.{field}")));
                }
                reject_forbidden_fields(child, &format!("{path}.{field}"))?;
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                reject_forbidden_fields(child, &format!("{path}[{index}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn digest_json<T: Serialize>(value: &T) -> String {
    let json = serde_json::to_value(value).expect("client contract serialization should be infallible");
    let canonical = canonical_json(&json);
    let bytes = serde_json::to_vec(&canonical).expect("client contract serialization should be infallible");
    format!("sha256:{}", hex(&Sha256::digest(bytes)))
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort_unstable();
            let mut canonical = Map::new();
            for key in keys {
                canonical.insert(key.clone(), canonical_json(&object[key]));
            }
            Value::Object(canonical)
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        other => other.clone(),
    }
}

/// requestId 冻结算法（#121 / ADR-0165）：对版本化闭合的 canonical request
/// envelope 计算 SHA-256。envelope 为请求 payload 去除派生自字段 `requestId`
/// 与 `digest` 后的全部字段；canonicalization 为递归 key 字典序排序，
/// `schemaVersion` 作为 envelope 字段参与摘要。Client 与 Server 独立复算并交叉校验，
/// 任意格式正确但与 envelope 不匹配的 requestId 一律 fail-closed 拒绝。
pub(crate) fn request_envelope_digest<T: Serialize>(payload: &T) -> String {
    let mut envelope =
        serde_json::to_value(payload).expect("client contract serialization should be infallible");
    if let Some(object) = envelope.as_object_mut() {
        object.remove("requestId");
        object.remove("digest");
    }
    digest_json(&envelope)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding() -> FindingView {
        FindingView::from_json(&serde_json::json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": FINDING_VIEW_KIND,
            "findingId": "finding-1",
            "ruleId": "corp.java.no-system-exit",
            "fingerprint": "fp-1",
            "message": "System.exit is forbidden",
            "location": {"path": "src/Main.java", "line": 12, "column": 5}
        }))
        .unwrap()
    }

    #[test]
    fn finding_view_is_closed_digest_only_and_replayable() {
        let first = finding();
        let second = finding();
        assert_eq!(first, second);
        assert_eq!(first.digest(), second.digest());
        assert!(first.digest().starts_with("sha256:"));
    }

    #[test]
    fn source_and_unknown_fields_fail_closed() {
        let mut json = serde_json::json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": FINDING_VIEW_KIND,
            "findingId": "finding-1",
            "ruleId": "rule",
            "fingerprint": "fp",
            "message": "message",
            "location": {"path": "src/Main.java", "line": 1, "column": 1}
        });
        json["sourceSnippet"] = Value::String("secret".into());
        assert!(matches!(FindingView::from_json(&json), Err(ClientError::ForbiddenPrivacyField(_))));
        json.as_object_mut().unwrap().remove("sourceSnippet");
        json["extra"] = Value::String("not-contract".into());
        assert_eq!(FindingView::from_json(&json), Err(ClientError::InvalidShape("input fields are not the closed contract")));
    }

    #[test]
    fn absolute_and_parent_paths_are_rejected() {
        let mut json = serde_json::to_value(finding()).unwrap();
        json["location"]["path"] = Value::String("/etc/passwd".into());
        assert_eq!(FindingView::from_json(&json), Err(ClientError::InvalidField("location.path")));
        json["location"]["path"] = Value::String("src/../Main.java".into());
        assert_eq!(FindingView::from_json(&json), Err(ClientError::InvalidField("location.path")));
        json["location"]["path"] = Value::String(r"..\Main.java".into());
        assert_eq!(FindingView::from_json(&json), Err(ClientError::InvalidField("location.path")));
    }

    #[test]
    fn feedback_and_exception_are_requests_not_authority() {
        let feedback = make_feedback_draft(&finding(), "true-fp", "reproduced as a false positive", "alice").unwrap();
        assert_eq!(feedback.status, "PROPOSED");
        assert_eq!(feedback.as_json()["kind"], FEEDBACK_DRAFT_KIND);
        assert_eq!(feedback.as_json()["schemaVersion"], FEEDBACK_DRAFT_VERSION);
        assert_eq!(feedback.as_json()["reason"], "reproduced as a false positive");
        assert!(feedback.as_json().get("source").is_none());
        let exception = make_exception_request(&finding(), "src/module/**", "migration window", "alice").unwrap();
        assert_eq!(exception.status, "REQUESTED");
        assert_eq!(exception.as_json()["kind"], EXCEPTION_REQUEST_KIND);
        assert!(make_exception_request(&finding(), "**", "all", "alice").is_err());
    }

    #[test]
    fn identical_requests_have_identical_ids_and_digests() {
        let first = make_feedback_draft(&finding(), "not-fp", "confirmed", "alice").unwrap();
        let second = make_feedback_draft(&finding(), "not-fp", "confirmed", "alice").unwrap();
        assert_eq!(first.request_id, second.request_id);
        assert_eq!(first.digest(), second.digest());
    }

    #[test]
    fn request_id_is_the_canonical_envelope_digest() {
        let feedback = make_feedback_draft(&finding(), "true-fp", "reproduced", "alice").unwrap();
        assert_eq!(feedback.request_id, request_envelope_digest(&feedback));
        let exception = make_exception_request(&finding(), "src/**", "migration window", "alice").unwrap();
        assert_eq!(exception.request_id, request_envelope_digest(&exception));
        // requestId 排除派生自字段：它不参与自身，也不参与 digest 自字段。
        assert_ne!(feedback.request_id, feedback.digest());
    }

    #[test]
    fn tampered_request_id_is_rejected_fail_closed() {
        let feedback = make_feedback_draft(&finding(), "true-fp", "reproduced", "alice").unwrap();
        let mut json = feedback.as_json();
        // 保持 digest 与承载内容一致，仅伪造 requestId：必须 fail-closed 拒绝。
        json["requestId"] = Value::String(format!("sha256:{}", "0".repeat(64)));
        json["digest"] = Value::String(digest_json(&{
            let mut content = json.clone();
            content.as_object_mut().unwrap().remove("digest");
            content
        }));
        assert!(matches!(
            FeedbackDraft::from_json(&json),
            Err(ClientError::RequestIdMismatch(_))
        ));

        let exception = make_exception_request(&finding(), "src/**", "window", "alice").unwrap();
        let mut json = exception.as_json();
        json["requestId"] = Value::String(format!("sha256:{}", "0".repeat(64)));
        json["digest"] = Value::String(digest_json(&{
            let mut content = json.clone();
            content.as_object_mut().unwrap().remove("digest");
            content
        }));
        assert!(matches!(
            ExceptionRequest::from_json(&json),
            Err(ClientError::RequestIdMismatch(_))
        ));
    }
}
