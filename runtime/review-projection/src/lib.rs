//! Digest-only review persistence and read-only presentation seams.
//!
//! This crate intentionally stops at local preparation evidence.  It accepts
//! metadata and digests, keeps an append-only event chain in memory, projects
//! counts/identities into telemetry, and renders a resumable SSE envelope.  It
//! never reads source, invokes SCM, changes a Gate, or exposes a write
//! transport.  A server or durable store can adapt these value types without
//! giving this crate authority to do those things.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;

/// Optional P2 metadata projections.  The module is intentionally separate
/// from the append-only ledger so callers can adopt the metadata seam without
/// expanding ledger identity or authority.
pub mod metadata;
pub use metadata::{
    BaseRulesConfigBinding, FindingDisposition, FindingSeverity, ImpactContext,
    ImpactFinding, ImpactScoreProjection, Provider, ProviderDraftMetadata,
    ProviderFindingMetadata, ProviderRescoreMetadata, ProviderScorerMetadata,
    RescoreResult, RescoreStatus, ReviewMetadataAdapter, ReviewProjectionMetadata,
    ReviewProjectionMetadataAdapter,
};

pub const REVIEW_LEDGER_VERSION: &str = "review-event-ledger-v1";
pub const REVIEW_RUN_VERSION: &str = "review-run-ledger-v1";
pub const REVIEW_TELEMETRY_VERSION: &str = "review-telemetry-v1";
pub const REVIEW_DASHBOARD_VERSION: &str = "review-dashboard-sse-v1";
pub const REVIEW_SNAPSHOT_VERSION: &str = "review-telemetry-snapshot-v1";
pub const REVIEW_EVENT_VERSION: &str = "review-dashboard-sse-event-v1";
pub const PREPARATION_MODE: &str = "PREPARATION_ONLY";
pub const PREPARATION_AUTHORITY: &str = "LOCAL_UNATTESTED";
pub const GENESIS_EVENT_DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

const FORBIDDEN_KEYS: &[&str] = &[
    "source", "sourceCode", "sourceContent", "sourceSnippet", "diff", "patch",
    "rawPrompt", "rawPrompts", "prompt", "body", "comment", "comments", "snippet",
    "fullFinding", "findings",
];

/// Stable SHA-256 identity used by all projection contracts.
pub fn sha256_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut result = String::with_capacity(71);
    result.push_str("sha256:");
    for byte in digest {
        result.push_str(&format!("{byte:02x}"));
    }
    result
}

fn domain_digest(domain: &str, fields: &[&str]) -> String {
    let mut bytes = domain.as_bytes().to_vec();
    bytes.push(0);
    for field in fields {
        bytes.extend_from_slice(&(field.len() as u64).to_be_bytes());
        bytes.extend_from_slice(field.as_bytes());
    }
    sha256_digest(&bytes)
}

fn digest_json(value: &Value) -> Result<String, ReviewProjectionError> {
    reject_forbidden_fields(value)?;
    Ok(sha256_digest(canonical_json(value).as_bytes()))
}

fn valid_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else { return false; };
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn validate_digest(value: &str, field: &'static str) -> Result<(), ReviewProjectionError> {
    if valid_digest(value) { Ok(()) } else { Err(ReviewProjectionError::InvalidDigest { field, value: value.to_owned() }) }
}

fn validate_text(value: &str, field: &'static str) -> Result<(), ReviewProjectionError> {
    if value.is_empty() || value.chars().any(char::is_control) {
        Err(ReviewProjectionError::InvalidField(field))
    } else {
        Ok(())
    }
}

fn reject_forbidden_fields(value: &Value) -> Result<(), ReviewProjectionError> {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if FORBIDDEN_KEYS.iter().any(|forbidden| *forbidden == key) {
                    return Err(ReviewProjectionError::ForbiddenField(key.clone()));
                }
                reject_forbidden_fields(child)?;
            }
        }
        Value::Array(values) => {
            for child in values { reject_forbidden_fields(child)?; }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReviewProjectionError {
    #[error("invalid field: {0}")]
    InvalidField(&'static str),
    #[error("invalid digest for {field}: {value}")]
    InvalidDigest { field: &'static str, value: String },
    #[error("forbidden source-bearing field: {0}")]
    ForbiddenField(String),
    #[error("review run already exists with a different identity: {0}")]
    ConflictingRun(String),
    #[error("review run was not found: {0}")]
    UnknownRun(String),
    #[error("review run head is stale")]
    StaleRevision,
    #[error("event sequence must be {expected}, received {received}")]
    InvalidSequence { expected: u64, received: u64 },
    #[error("event chain previous digest does not match the current head")]
    BrokenEventChain,
    #[error("event id already exists with different content: {0}")]
    ConflictingEvent(String),
    #[error("event payload must be a JSON object")]
    InvalidPayload,
    #[error("cursor is stale or unknown: {0}")]
    StaleCursor(String),
    #[error("dashboard event sequence must be strictly increasing")]
    NonIncreasingDashboardSequence,
    #[error("dashboard event cursor does not match snapshot cursor")]
    CursorMismatch,
    #[error("dashboard event snapshot digest does not match snapshot")]
    SnapshotMismatch,
    #[error("dashboard event type cannot be used for this payload")]
    InvalidEventType,
    #[error("serialization failed: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewRunState {
    Planned,
    Running,
    Succeeded,
    Inconclusive,
    Rejected,
}

impl ReviewRunState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "PLANNED",
            Self::Running => "RUNNING",
            Self::Succeeded => "SUCCEEDED",
            Self::Inconclusive => "INCONCLUSIVE",
            Self::Rejected => "REJECTED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewEventType {
    RunCreated,
    ScopeStarted,
    ScopeCompleted,
    FindingUpdated,
    VerifierDiscarded,
    RunCompleted,
    RunRejected,
}

impl ReviewEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RunCreated => "RUN_CREATED",
            Self::ScopeStarted => "SCOPE_STARTED",
            Self::ScopeCompleted => "SCOPE_COMPLETED",
            Self::FindingUpdated => "FINDING_UPDATED",
            Self::VerifierDiscarded => "VERIFIER_DISCARDED",
            Self::RunCompleted => "RUN_COMPLETED",
            Self::RunRejected => "RUN_REJECTED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRunRecord {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "reviewRunId")]
    pub review_run_id: String,
    #[serde(rename = "revisionBindingDigest")]
    pub revision_binding_digest: String,
    #[serde(rename = "planDigest")]
    pub plan_digest: String,
    pub state: ReviewRunState,
    #[serde(rename = "headDigest")]
    pub head_digest: String,
    #[serde(rename = "eventHeadDigest")]
    pub event_head_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewEvent {
    pub sequence: u64,
    #[serde(rename = "eventId")]
    pub event_id: String,
    #[serde(rename = "reviewRunId")]
    pub review_run_id: String,
    #[serde(rename = "eventType")]
    pub event_type: ReviewEventType,
    #[serde(rename = "payloadDigest")]
    pub payload_digest: String,
    #[serde(rename = "previousEventDigest")]
    pub previous_event_digest: String,
    #[serde(rename = "eventDigest")]
    pub event_digest: String,
}

#[derive(Debug, Clone)]
pub struct ReviewRunInput {
    pub review_run_id: String,
    pub revision_binding_digest: String,
    pub plan_digest: String,
    pub head_digest: String,
}

#[derive(Debug, Clone, Default)]
pub struct ReviewLedger {
    runs: BTreeMap<String, ReviewRunRecord>,
    events: Vec<ReviewEvent>,
}

impl ReviewLedger {
    pub fn new() -> Self { Self::default() }

    pub fn create_run(&mut self, input: ReviewRunInput) -> Result<ReviewRunRecord, ReviewProjectionError> {
        validate_text(&input.review_run_id, "review_run_id")?;
        validate_digest(&input.revision_binding_digest, "revision_binding_digest")?;
        validate_digest(&input.plan_digest, "plan_digest")?;
        validate_digest(&input.head_digest, "head_digest")?;
        if let Some(existing) = self.runs.get(&input.review_run_id) {
            if existing.revision_binding_digest == input.revision_binding_digest
                && existing.plan_digest == input.plan_digest
                && existing.head_digest == input.head_digest
            {
                return Ok(existing.clone());
            }
            return Err(ReviewProjectionError::ConflictingRun(input.review_run_id));
        }
        let record = ReviewRunRecord {
            schema_version: REVIEW_RUN_VERSION.to_owned(),
            review_run_id: input.review_run_id.clone(),
            revision_binding_digest: input.revision_binding_digest,
            plan_digest: input.plan_digest,
            state: ReviewRunState::Planned,
            head_digest: input.head_digest,
            event_head_digest: GENESIS_EVENT_DIGEST.to_owned(),
        };
        self.runs.insert(input.review_run_id, record.clone());
        Ok(record)
    }

    pub fn resume(&self, review_run_id: &str, expected_head_digest: &str) -> Result<&ReviewRunRecord, ReviewProjectionError> {
        validate_digest(expected_head_digest, "expected_head_digest")?;
        let run = self.runs.get(review_run_id).ok_or_else(|| ReviewProjectionError::UnknownRun(review_run_id.to_owned()))?;
        if run.head_digest != expected_head_digest { return Err(ReviewProjectionError::StaleRevision); }
        Ok(run)
    }

    pub fn append_event(
        &mut self,
        event_id: impl Into<String>,
        review_run_id: impl Into<String>,
        event_type: ReviewEventType,
        payload: &Value,
    ) -> Result<ReviewEvent, ReviewProjectionError> {
        let event_id = event_id.into();
        let review_run_id = review_run_id.into();
        validate_text(&event_id, "event_id")?;
        let _run = self.runs.get(&review_run_id).ok_or_else(|| ReviewProjectionError::UnknownRun(review_run_id.clone()))?;
        let payload_object = payload.as_object().ok_or(ReviewProjectionError::InvalidPayload)?;
        if payload_object.is_empty() { return Err(ReviewProjectionError::InvalidPayload); }
        let payload_digest = digest_json(payload)?;
        let sequence = self.events.len() as u64 + 1;
        let previous_event_digest = self.events.last().map(|event| event.event_digest.as_str()).unwrap_or(GENESIS_EVENT_DIGEST);
        let event_digest = event_chain_digest(sequence, &event_id, &review_run_id, event_type, &payload_digest, previous_event_digest);
        if let Some(existing) = self.events.iter().find(|event| event.event_id == event_id) {
            if existing.event_digest == event_digest { return Ok(existing.clone()); }
            return Err(ReviewProjectionError::ConflictingEvent(event_id));
        }
        let event = ReviewEvent {
            sequence,
            event_id,
            review_run_id: review_run_id.clone(),
            event_type,
            payload_digest,
            previous_event_digest: previous_event_digest.to_owned(),
            event_digest: event_digest.clone(),
        };
        self.events.push(event.clone());
        let run = self.runs.get_mut(&review_run_id).expect("run checked above");
        run.event_head_digest = event_digest;
        run.state = state_after_event(run.state, event_type);
        Ok(event)
    }

    pub fn run(&self, review_run_id: &str) -> Option<&ReviewRunRecord> { self.runs.get(review_run_id) }
    pub fn runs(&self) -> impl Iterator<Item = &ReviewRunRecord> { self.runs.values() }
    pub fn events(&self) -> &[ReviewEvent] { &self.events }
    pub fn event_count(&self) -> u64 { self.events.len() as u64 }
    pub fn head_event_digest(&self) -> &str { self.events.last().map(|event| event.event_digest.as_str()).unwrap_or(GENESIS_EVENT_DIGEST) }

    pub fn to_json(&self) -> Value {
        let run = self.runs.values().next();
        let (bundle_digest, review_run_digest, revision_binding_digest, plan_digest) = if let Some(run) = run {
            let review_run_digest = domain_digest(
                REVIEW_RUN_VERSION,
                &[
                    &run.review_run_id,
                    &run.revision_binding_digest,
                    &run.plan_digest,
                    run.state.as_str(),
                    &run.head_digest,
                    &run.event_head_digest,
                ],
            );
            let bundle_digest = domain_digest(REVIEW_LEDGER_VERSION, &[&review_run_digest, self.head_event_digest()]);
            (
                bundle_digest,
                review_run_digest,
                run.revision_binding_digest.clone(),
                run.plan_digest.clone(),
            )
        } else {
            (
                domain_digest(REVIEW_LEDGER_VERSION, &[GENESIS_EVENT_DIGEST]),
                domain_digest(REVIEW_RUN_VERSION, &[GENESIS_EVENT_DIGEST]),
                GENESIS_EVENT_DIGEST.to_owned(),
                GENESIS_EVENT_DIGEST.to_owned(),
            )
        };
        serde_json::json!({
            "schemaVersion": REVIEW_LEDGER_VERSION,
            "bundleDigest": bundle_digest,
            "reviewRunDigest": review_run_digest,
            "revisionBindingDigest": revision_binding_digest,
            "planDigest": plan_digest,
            "reviewRuns": self.runs.values().collect::<Vec<_>>(),
            "events": self.events,
            "authority": PREPARATION_AUTHORITY,
            "mode": PREPARATION_MODE,
            "scmWriteback": "forbidden",
            "gateMutation": "forbidden",
        })
    }

    pub fn events_after(&self, sequence: u64) -> impl Iterator<Item = &ReviewEvent> {
        self.events.iter().filter(move |event| event.sequence > sequence)
    }
}

fn state_after_event(current: ReviewRunState, event_type: ReviewEventType) -> ReviewRunState {
    match event_type {
        ReviewEventType::RunCreated => ReviewRunState::Running,
        ReviewEventType::RunCompleted => ReviewRunState::Succeeded,
        ReviewEventType::RunRejected => ReviewRunState::Rejected,
        _ => current,
    }
}

fn event_chain_digest(
    sequence: u64,
    event_id: &str,
    review_run_id: &str,
    event_type: ReviewEventType,
    payload_digest: &str,
    previous_event_digest: &str,
) -> String {
    let sequence = sequence.to_string();
    domain_digest(REVIEW_LEDGER_VERSION, &[&sequence, event_id, review_run_id, event_type.as_str(), payload_digest, previous_event_digest])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct QueueCounts {
    pub queued: u64,
    pub running: u64,
    pub succeeded: u64,
    pub inconclusive: u64,
    pub rejected: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionChain {
    #[serde(rename = "reviewRunId")]
    pub review_run_id: String,
    #[serde(rename = "baseDigest")]
    pub base_digest: String,
    #[serde(rename = "headDigest")]
    pub head_digest: String,
    pub status: ReviewRunState,
    pub stale: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FindingSummary {
    pub open: u64,
    pub kept: u64,
    pub resolved: u64,
    pub dismissed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CoverageSummary {
    pub files: u64,
    #[serde(rename = "changedLines")]
    pub changed_lines: u64,
    #[serde(rename = "reviewedScopes")]
    pub reviewed_scopes: u64,
    #[serde(rename = "completedScopes")]
    pub completed_scopes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageSummary {
    #[serde(rename = "durationMs")]
    pub duration_ms: u64,
    #[serde(rename = "modelTurns")]
    pub model_turns: u64,
    #[serde(rename = "toolCalls")]
    pub tool_calls: u64,
    #[serde(rename = "deniedToolCalls")]
    pub denied_tool_calls: u64,
    #[serde(rename = "estimatedCost")]
    pub estimated_cost: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditSummary {
    #[serde(rename = "eventCount")]
    pub event_count: u64,
    #[serde(rename = "headEventDigest")]
    pub head_event_digest: String,
    #[serde(rename = "lastEventDigest")]
    pub last_event_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryProjection {
    pub queue: QueueCounts,
    pub version_chain: VersionChain,
    pub finding_summary: FindingSummary,
    pub coverage: CoverageSummary,
    pub usage: UsageSummary,
    pub audit: AuditSummary,
}

impl TelemetryProjection {
    pub fn new(
        queue: QueueCounts,
        version_chain: VersionChain,
        finding_summary: FindingSummary,
        coverage: CoverageSummary,
        usage: UsageSummary,
        audit: AuditSummary,
    ) -> Result<Self, ReviewProjectionError> {
        validate_digest(&version_chain.base_digest, "base_digest")?;
        validate_digest(&version_chain.head_digest, "head_digest")?;
        validate_digest(&usage.estimated_cost, "estimated_cost")?;
        validate_digest(&audit.head_event_digest, "head_event_digest")?;
        validate_digest(&audit.last_event_digest, "last_event_digest")?;
        validate_text(&version_chain.review_run_id, "review_run_id")?;
        if audit.event_count == 0 && audit.head_event_digest != GENESIS_EVENT_DIGEST {
            return Err(ReviewProjectionError::InvalidField("audit.head_event_digest"));
        }
        Ok(Self { queue, version_chain, finding_summary, coverage, usage, audit })
    }

    pub fn from_ledger(
        ledger: &ReviewLedger,
        review_run_id: &str,
        stale: bool,
        queue: QueueCounts,
        finding_summary: FindingSummary,
        coverage: CoverageSummary,
        usage: UsageSummary,
    ) -> Result<Self, ReviewProjectionError> {
        let run = ledger.run(review_run_id).ok_or_else(|| ReviewProjectionError::UnknownRun(review_run_id.to_owned()))?;
        let audit = AuditSummary {
            event_count: ledger.event_count(),
            head_event_digest: ledger.head_event_digest().to_owned(),
            last_event_digest: ledger.head_event_digest().to_owned(),
        };
        Self::new(queue, VersionChain {
            review_run_id: run.review_run_id.clone(),
            base_digest: run.revision_binding_digest.clone(),
            head_digest: run.head_digest.clone(),
            status: run.state,
            stale,
        }, finding_summary, coverage, usage, audit)
    }

    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "schemaVersion": REVIEW_TELEMETRY_VERSION,
            "snapshotDigest": self.snapshot_digest(),
            "queue": self.queue,
            "versionChain": self.version_chain,
            "findingSummary": self.finding_summary,
            "coverage": self.coverage,
            "usage": self.usage,
            "audit": self.audit,
            "authority": PREPARATION_AUTHORITY,
            "mode": PREPARATION_MODE,
        })
    }

    pub fn snapshot_digest(&self) -> String {
        let value = serde_json::json!({
            "schemaVersion": REVIEW_TELEMETRY_VERSION,
            "queue": self.queue,
            "versionChain": self.version_chain,
            "findingSummary": self.finding_summary,
            "coverage": self.coverage,
            "usage": self.usage,
            "audit": self.audit,
            "authority": PREPARATION_AUTHORITY,
            "mode": PREPARATION_MODE,
        });
        digest_json(&value).expect("telemetry projection is digest-only")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub value: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardSnapshot {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub cursor: Cursor,
    pub sequence: u64,
    #[serde(rename = "snapshotDigest")]
    pub snapshot_digest: String,
    pub queue: Value,
    pub version: Value,
    pub finding: Value,
    pub coverage: Value,
    pub audit: Value,
    pub authority: String,
    pub mode: String,
}

impl DashboardSnapshot {
    pub fn from_telemetry(telemetry: &TelemetryProjection, sequence: u64) -> Result<Self, ReviewProjectionError> {
        if sequence == 0 { return Err(ReviewProjectionError::InvalidSequence { expected: 1, received: 0 }); }
        let queue = section_with_digest(serde_json::to_value(telemetry.queue).map_err(serialization_error)?)?;
        let version = section_with_digest(serde_json::to_value(&telemetry.version_chain).map_err(serialization_error)?)?;
        let finding = section_with_digest(serde_json::to_value(telemetry.finding_summary).map_err(serialization_error)?)?;
        let coverage = section_with_digest(serde_json::to_value(telemetry.coverage).map_err(serialization_error)?)?;
        let audit = section_with_digest(serde_json::to_value(&telemetry.audit).map_err(serialization_error)?)?;
        let cursor = Cursor { value: domain_digest("review-dashboard-cursor/v1", &[&sequence.to_string(), &telemetry.version_chain.review_run_id]), sequence };
        let mut snapshot = Self {
            schema_version: REVIEW_SNAPSHOT_VERSION.to_owned(),
            cursor,
            sequence,
            snapshot_digest: String::new(),
            queue,
            version,
            finding,
            coverage,
            audit,
            authority: PREPARATION_AUTHORITY.to_owned(),
            mode: PREPARATION_MODE.to_owned(),
        };
        let without_digest = snapshot_without_digest(&snapshot)?;
        snapshot.snapshot_digest = digest_json(&without_digest)?;
        Ok(snapshot)
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("dashboard snapshot is serializable")
    }
}

fn serialization_error(error: serde_json::Error) -> ReviewProjectionError {
    ReviewProjectionError::Serialization(error.to_string())
}

fn section_with_digest(value: Value) -> Result<Value, ReviewProjectionError> {
    let digest = digest_json(&value)?;
    let mut object = value.as_object().cloned().ok_or(ReviewProjectionError::InvalidPayload)?;
    object.insert("projectionDigest".to_owned(), Value::String(digest));
    Ok(Value::Object(object))
}

fn snapshot_without_digest(snapshot: &DashboardSnapshot) -> Result<Value, ReviewProjectionError> {
    let mut value = snapshot.to_json();
    value.as_object_mut().expect("snapshot object").remove("snapshotDigest");
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DashboardEventType {
    Snapshot,
    Queue,
    Version,
    Finding,
    Coverage,
    Audit,
    Heartbeat,
    Reset,
}

impl DashboardEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Snapshot => "snapshot",
            Self::Queue => "queue",
            Self::Version => "version",
            Self::Finding => "finding",
            Self::Coverage => "coverage",
            Self::Audit => "audit",
            Self::Heartbeat => "heartbeat",
            Self::Reset => "reset",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardEvent {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "eventId")]
    pub event_id: String,
    #[serde(rename = "eventType")]
    pub event_type: DashboardEventType,
    pub cursor: String,
    pub sequence: u64,
    #[serde(rename = "snapshotDigest")]
    pub snapshot_digest: String,
    #[serde(rename = "payloadDigest")]
    pub payload_digest: String,
    #[serde(rename = "previousEventDigest")]
    pub previous_event_digest: String,
    #[serde(rename = "eventDigest")]
    pub event_digest: String,
    pub payload: Value,
}

impl DashboardEvent {
    pub fn to_json(&self) -> Value { serde_json::to_value(self).expect("dashboard event is serializable") }

    /// Canonical SSE frame. No method other than a GET consumer is implied by
    /// this value; callers still own network/authentication policy.
    pub fn sse_frame(&self, retry_millis: Option<u64>) -> String {
        let mut frame = format!("id: {}\nevent: {}\ndata: {}\n", self.cursor, self.event_type.as_str(), canonical_json(&self.to_json()));
        if let Some(retry) = retry_millis { frame.push_str(&format!("retry: {retry}\n")); }
        frame.push('\n');
        frame
    }
}

#[derive(Debug, Clone, Default)]
pub struct DashboardTransport {
    events: Vec<DashboardEvent>,
}

impl DashboardTransport {
    pub fn new() -> Self { Self::default() }

    pub fn publish(&mut self, snapshot: &DashboardSnapshot, event_type: DashboardEventType, payload: &Value) -> Result<DashboardEvent, ReviewProjectionError> {
        reject_forbidden_fields(payload)?;
        if payload.as_object().is_none() { return Err(ReviewProjectionError::InvalidPayload); }
        if snapshot.authority != PREPARATION_AUTHORITY || snapshot.mode != PREPARATION_MODE {
            return Err(ReviewProjectionError::InvalidField("dashboard authority boundary"));
        }
        validate_digest(&snapshot.snapshot_digest, "snapshot_digest")?;
        let last_sequence = self.events.last().map(|event| event.sequence).unwrap_or(0);
        if snapshot.sequence <= last_sequence { return Err(ReviewProjectionError::NonIncreasingDashboardSequence); }
        if snapshot.cursor.sequence != snapshot.sequence { return Err(ReviewProjectionError::CursorMismatch); }
        let payload_digest = digest_json(payload)?;
        let previous_event_digest = self.events.last().map(|event| event.event_digest.as_str()).unwrap_or(GENESIS_EVENT_DIGEST);
        let event_id = domain_digest("review-dashboard-event-id/v1", &[&snapshot.cursor.value, event_type.as_str(), &payload_digest]);
        let event_digest = dashboard_event_digest(&event_id, event_type, &snapshot.cursor.value, snapshot.sequence, &snapshot.snapshot_digest, &payload_digest, previous_event_digest);
        let event = DashboardEvent {
            schema_version: REVIEW_EVENT_VERSION.to_owned(),
            event_id,
            event_type,
            cursor: snapshot.cursor.value.clone(),
            sequence: snapshot.sequence,
            snapshot_digest: snapshot.snapshot_digest.clone(),
            payload_digest,
            previous_event_digest: previous_event_digest.to_owned(),
            event_digest,
            payload: payload.clone(),
        };
        self.events.push(event.clone());
        Ok(event)
    }

    pub fn events(&self) -> &[DashboardEvent] { &self.events }

    /// Replay is exclusive, matching `replayFrom: exclusive` in the contract.
    /// An unknown cursor is stale rather than silently replaying from genesis.
    pub fn replay_after(&self, cursor: Option<&str>) -> Result<Vec<&DashboardEvent>, ReviewProjectionError> {
        let Some(cursor) = cursor else { return Ok(self.events.iter().collect()); };
        let index = self.events.iter().position(|event| event.cursor == cursor)
            .ok_or_else(|| ReviewProjectionError::StaleCursor(cursor.to_owned()))?;
        Ok(self.events.iter().skip(index + 1).collect())
    }
}

/// Small runtime facade that keeps the ledger and read-only dashboard stream
/// on one local boundary.  It is deliberately not a server: callers choose
/// how (or whether) to persist the JSON values and expose GET/SSE routes.
#[derive(Debug, Clone, Default)]
pub struct ReviewProjectionAdapter {
    ledger: ReviewLedger,
    dashboard: DashboardTransport,
}

impl ReviewProjectionAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ledger(&self) -> &ReviewLedger {
        &self.ledger
    }

    pub fn ledger_mut(&mut self) -> &mut ReviewLedger {
        &mut self.ledger
    }

    pub fn dashboard(&self) -> &DashboardTransport {
        &self.dashboard
    }

    pub fn create_run(&mut self, input: ReviewRunInput) -> Result<ReviewRunRecord, ReviewProjectionError> {
        self.ledger.create_run(input)
    }

    pub fn append_event(
        &mut self,
        event_id: impl Into<String>,
        review_run_id: impl Into<String>,
        event_type: ReviewEventType,
        payload: &Value,
    ) -> Result<ReviewEvent, ReviewProjectionError> {
        self.ledger.append_event(event_id, review_run_id, event_type, payload)
    }

    pub fn snapshot(&self, telemetry: &TelemetryProjection, sequence: u64) -> Result<DashboardSnapshot, ReviewProjectionError> {
        DashboardSnapshot::from_telemetry(telemetry, sequence)
    }

    pub fn publish(
        &mut self,
        snapshot: &DashboardSnapshot,
        event_type: DashboardEventType,
        payload: &Value,
    ) -> Result<DashboardEvent, ReviewProjectionError> {
        self.dashboard.publish(snapshot, event_type, payload)
    }

    pub fn telemetry(
        &self,
        review_run_id: &str,
        stale: bool,
        queue: QueueCounts,
        finding_summary: FindingSummary,
        coverage: CoverageSummary,
        usage: UsageSummary,
    ) -> Result<TelemetryProjection, ReviewProjectionError> {
        TelemetryProjection::from_ledger(&self.ledger, review_run_id, stale, queue, finding_summary, coverage, usage)
    }
}

fn dashboard_event_digest(
    event_id: &str,
    event_type: DashboardEventType,
    cursor: &str,
    sequence: u64,
    snapshot_digest: &str,
    payload_digest: &str,
    previous_event_digest: &str,
) -> String {
    let sequence = sequence.to_string();
    domain_digest(REVIEW_EVENT_VERSION, &[event_id, event_type.as_str(), cursor, &sequence, snapshot_digest, payload_digest, previous_event_digest])
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            let fields = keys.into_iter().map(|key| format!("{}:{}", serde_json::to_string(key).unwrap(), canonical_json(&object[key]))).collect::<Vec<_>>();
            format!("{{{}}}", fields.join(","))
        }
        Value::Array(values) => format!("[{}]", values.iter().map(canonical_json).collect::<Vec<_>>().join(",")),
        _ => serde_json::to_string(value).unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(ch: char) -> String { format!("sha256:{}", ch.to_string().repeat(64)) }

    fn ledger() -> ReviewLedger {
        let mut ledger = ReviewLedger::new();
        ledger.create_run(ReviewRunInput {
            review_run_id: "run-1".into(),
            revision_binding_digest: digest('a'),
            plan_digest: digest('b'),
            head_digest: digest('c'),
        }).unwrap();
        ledger
    }

    fn usage() -> UsageSummary {
        UsageSummary { duration_ms: 12, model_turns: 2, tool_calls: 3, denied_tool_calls: 1, estimated_cost: digest('d') }
    }

    #[test]
    fn ledger_is_append_only_digest_bound_and_rejects_source() {
        let mut ledger = ledger();
        let first = ledger.append_event("e-1", "run-1", ReviewEventType::RunCreated, &serde_json::json!({"scopeDigest": digest('e')})).unwrap();
        let second = ledger.append_event("e-2", "run-1", ReviewEventType::ScopeStarted, &serde_json::json!({"scopeIdDigest": digest('f')})).unwrap();
        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
        assert_eq!(second.previous_event_digest, first.event_digest);
        assert_eq!(ledger.run("run-1").unwrap().event_head_digest, second.event_digest);
        assert!(matches!(ledger.resume("run-1", &digest('0')), Err(ReviewProjectionError::StaleRevision)));
        assert!(matches!(ledger.append_event("e-3", "run-1", ReviewEventType::FindingUpdated, &serde_json::json!({"sourceContent": "class A {}"})), Err(ReviewProjectionError::ForbiddenField(_))));
        let json = ledger.to_json();
        assert_eq!(json["mode"], PREPARATION_MODE);
        assert_eq!(json["authority"], PREPARATION_AUTHORITY);
        assert_eq!(json["scmWriteback"], "forbidden");
        assert_eq!(json["gateMutation"], "forbidden");
    }

    #[test]
    fn telemetry_and_snapshot_have_section_digests_and_fixed_boundary() {
        let ledger = ledger();
        let telemetry = TelemetryProjection::from_ledger(
            &ledger,
            "run-1",
            false,
            QueueCounts { queued: 1, running: 1, succeeded: 0, inconclusive: 0, rejected: 0 },
            FindingSummary { open: 2, kept: 1, resolved: 0, dismissed: 0 },
            CoverageSummary { files: 1, changed_lines: 4, reviewed_scopes: 1, completed_scopes: 0 },
            usage(),
        ).unwrap();
        let snapshot = DashboardSnapshot::from_telemetry(&telemetry, 1).unwrap();
        assert_eq!(snapshot.cursor.sequence, snapshot.sequence);
        assert!(valid_digest(&snapshot.snapshot_digest));
        for section in [&snapshot.queue, &snapshot.version, &snapshot.finding, &snapshot.coverage, &snapshot.audit] {
            assert!(valid_digest(section["projectionDigest"].as_str().unwrap()));
        }
        assert_eq!(snapshot.authority, PREPARATION_AUTHORITY);
        assert_eq!(snapshot.mode, PREPARATION_MODE);
        assert!(snapshot.to_json().get("sourceContent").is_none());
    }

    #[test]
    fn dashboard_transport_renders_sse_and_replays_exclusively() {
        let ledger = ledger();
        let telemetry = TelemetryProjection::from_ledger(&ledger, "run-1", false, QueueCounts::default(), FindingSummary::default(), CoverageSummary::default(), usage()).unwrap();
        let snapshot = DashboardSnapshot::from_telemetry(&telemetry, 1).unwrap();
        let mut transport = DashboardTransport::new();
        let event = transport.publish(&snapshot, DashboardEventType::Snapshot, &serde_json::json!({"projectionDigest": digest('e')})).unwrap();
        assert_eq!(event.previous_event_digest, GENESIS_EVENT_DIGEST);
        let frame = event.sse_frame(Some(1_000));
        assert!(frame.starts_with("id: "));
        assert!(frame.contains("event: snapshot\n"));
        assert!(frame.contains("retry: 1000\n"));
        assert_eq!(transport.replay_after(Some(&event.cursor)).unwrap().len(), 0);
        assert!(matches!(transport.replay_after(Some("stale-cursor")), Err(ReviewProjectionError::StaleCursor(_))));
        assert!(!frame.contains("sourceContent"));
    }

    #[test]
    fn digest_is_lowercase_and_dashboard_chain_binds_snapshot() {
        let ledger = ledger();
        let telemetry = TelemetryProjection::from_ledger(&ledger, "run-1", false, QueueCounts::default(), FindingSummary::default(), CoverageSummary::default(), usage()).unwrap();
        let snapshot = DashboardSnapshot::from_telemetry(&telemetry, 1).unwrap();
        let mut transport = DashboardTransport::new();
        let first = transport.publish(&snapshot, DashboardEventType::Snapshot, &serde_json::json!({"section": "snapshot"})).unwrap();
        assert_eq!(first.event_digest, dashboard_event_digest(&first.event_id, first.event_type, &first.cursor, first.sequence, &first.snapshot_digest, &first.payload_digest, GENESIS_EVENT_DIGEST));
        assert_eq!(first.event_id, domain_digest("review-dashboard-event-id/v1", &[&first.cursor, first.event_type.as_str(), &first.payload_digest]));
    }

    #[test]
    fn facade_keeps_ledger_and_dashboard_on_the_same_local_boundary() {
        let mut adapter = ReviewProjectionAdapter::new();
        adapter.create_run(ReviewRunInput {
            review_run_id: "run-1".into(),
            revision_binding_digest: digest('a'),
            plan_digest: digest('b'),
            head_digest: digest('c'),
        }).unwrap();
        adapter.append_event("e-1", "run-1", ReviewEventType::RunCreated, &serde_json::json!({"scopeDigest": digest('d')})).unwrap();
        let telemetry = adapter.telemetry("run-1", false, QueueCounts::default(), FindingSummary::default(), CoverageSummary::default(), usage()).unwrap();
        let snapshot = adapter.snapshot(&telemetry, 1).unwrap();
        let event = adapter.publish(&snapshot, DashboardEventType::Snapshot, &serde_json::json!({"projectionDigest": digest('e')})).unwrap();
        assert_eq!(adapter.ledger().event_count(), 1);
        assert_eq!(adapter.dashboard().events()[0].event_digest, event.event_digest);
    }
}
