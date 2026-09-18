//! Digest-only feedback and regression-case handoff.
//!
//! Feedback references an exact Finding identity and UnifiedResult digest but
//! never carries source, paths beyond the supplied finding identity, or a full
//! engine report. The ledger is an in-memory core; adapters own persistence
//! and transport.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;

pub const FEEDBACK_VERSION: &str = "feedback-v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackClass {
    ConfirmedValidFinding,
    DocumentationGap,
    ScopeIssue,
    ConfirmedFalsePositive,
}

impl FeedbackClass {
    fn wire_name(self) -> &'static str {
        match self {
            Self::ConfirmedValidFinding => "not-fp",
            Self::DocumentationGap => "documentation",
            Self::ScopeIssue => "scope-issue",
            Self::ConfirmedFalsePositive => "true-fp",
        }
    }

    pub fn creates_regression_case(self) -> bool {
        self == Self::ConfirmedFalsePositive
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackRecord {
    pub feedback_id: String,
    pub repository: String,
    pub rule_id: String,
    pub finding_fingerprint: String,
    pub unified_result_digest: [u8; 32],
    pub classification: FeedbackClass,
    pub reporter: String,
    pub rationale: String,
    pub ticket: Option<String>,
    pub created_at_epoch_secs: u64,
}

impl FeedbackRecord {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = FEEDBACK_VERSION.as_bytes().to_vec();
        payload.push(0);
        for value in [&self.feedback_id, &self.repository, &self.rule_id, &self.finding_fingerprint, &self.reporter, &self.rationale] {
            append_string(&mut payload, value);
        }
        payload.extend_from_slice(&self.unified_result_digest);
        append_string(&mut payload, self.classification.wire_name());
        match &self.ticket {
            Some(ticket) => { payload.push(1); append_string(&mut payload, ticket); }
            None => payload.push(0),
        }
        payload.extend_from_slice(&self.created_at_epoch_secs.to_be_bytes());
        payload
    }

    pub fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_payload()).into()
    }

    pub fn validate(&self) -> Result<(), FeedbackError> {
        for (name, value) in [
            ("feedback_id", &self.feedback_id),
            ("repository", &self.repository),
            ("rule_id", &self.rule_id),
            ("finding_fingerprint", &self.finding_fingerprint),
            ("reporter", &self.reporter),
            ("rationale", &self.rationale),
        ] {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(FeedbackError::InvalidField(name));
            }
        }
        if self.unified_result_digest == [0; 32] {
            return Err(FeedbackError::ZeroResultDigest);
        }
        if let Some(ticket) = &self.ticket {
            if ticket.trim().is_empty() || ticket.chars().any(char::is_control) {
                return Err(FeedbackError::InvalidField("ticket"));
            }
        }
        Ok(())
    }

    pub fn regression_case(&self) -> Option<RegressionCase> {
        self.classification.creates_regression_case().then(|| RegressionCase {
            feedback_digest: self.digest(),
            repository: self.repository.clone(),
            rule_id: self.rule_id.clone(),
            finding_fingerprint: self.finding_fingerprint.clone(),
            expectation: RegressionExpectation::MustBeClean,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegressionExpectation {
    MustBeClean,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegressionCase {
    pub feedback_digest: [u8; 32],
    pub repository: String,
    pub rule_id: String,
    pub finding_fingerprint: String,
    pub expectation: RegressionExpectation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackState {
    Open,
    RegressionCreated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackEntry {
    pub digest: [u8; 32],
    pub record: FeedbackRecord,
    pub state: FeedbackState,
    pub regression_case: Option<RegressionCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub audit_sequence: u64,
    pub operation_sequence: u64,
    pub feedback_digest: [u8; 32],
    pub actor: String,
    pub reason: String,
    pub resulting_state: FeedbackState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitOutcome {
    Admitted(FeedbackEntry),
    Idempotent(FeedbackEntry),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FeedbackError {
    #[error("feedback field is invalid: {0}")]
    InvalidField(&'static str),
    #[error("feedback references a zero UnifiedResult digest")]
    ZeroResultDigest,
    #[error("feedback actor is empty")]
    EmptyActor,
    #[error("feedback audit reason is empty")]
    EmptyReason,
    #[error("feedback id {0} already refers to different content")]
    ConflictingIdentity(String),
    #[error("feedback operation sequence {received} is not newer than {current}")]
    StaleOperation { received: u64, current: u64 },
    #[error("unknown feedback digest {0}")]
    UnknownFeedback(String),
}

pub struct FeedbackLedger {
    entries: BTreeMap<[u8; 32], FeedbackEntry>,
    by_feedback_id: BTreeMap<String, [u8; 32]>,
    audit: Vec<AuditRecord>,
    next_audit_sequence: u64,
    last_operation_sequence: u64,
}

impl FeedbackLedger {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            by_feedback_id: BTreeMap::new(),
            audit: Vec::new(),
            next_audit_sequence: 1,
            last_operation_sequence: 0,
        }
    }

    pub fn submit(&mut self, record: FeedbackRecord, actor: &str, reason: &str, operation_sequence: u64) -> Result<SubmitOutcome, FeedbackError> {
        record.validate()?;
        validate_context(actor, reason)?;
        let digest = record.digest();
        if let Some(existing) = self.entries.get(&digest) {
            if existing.record == record {
                return Ok(SubmitOutcome::Idempotent(existing.clone()));
            }
        }
        if self.by_feedback_id.contains_key(&record.feedback_id) {
            return Err(FeedbackError::ConflictingIdentity(record.feedback_id));
        }
        if operation_sequence <= self.last_operation_sequence {
            return Err(FeedbackError::StaleOperation { received: operation_sequence, current: self.last_operation_sequence });
        }
        let regression_case = record.regression_case();
        let state = if regression_case.is_some() { FeedbackState::RegressionCreated } else { FeedbackState::Open };
        let entry = FeedbackEntry { digest, record: record.clone(), state, regression_case };
        self.entries.insert(digest, entry.clone());
        self.by_feedback_id.insert(record.feedback_id, digest);
        self.last_operation_sequence = operation_sequence;
        self.audit.push(AuditRecord {
            audit_sequence: self.next_audit_sequence,
            operation_sequence,
            feedback_digest: digest,
            actor: actor.into(),
            reason: reason.into(),
            resulting_state: state,
        });
        self.next_audit_sequence += 1;
        Ok(SubmitOutcome::Admitted(entry))
    }

    pub fn entry(&self, digest: [u8; 32]) -> Result<&FeedbackEntry, FeedbackError> {
        self.entries.get(&digest).ok_or_else(|| FeedbackError::UnknownFeedback(hex_digest(&digest)))
    }

    pub fn audit_log(&self) -> &[AuditRecord] {
        &self.audit
    }

    pub fn last_operation_sequence(&self) -> u64 {
        self.last_operation_sequence
    }
}

impl Default for FeedbackLedger {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_context(actor: &str, reason: &str) -> Result<(), FeedbackError> {
    if actor.trim().is_empty() { return Err(FeedbackError::EmptyActor); }
    if reason.trim().is_empty() { return Err(FeedbackError::EmptyReason); }
    Ok(())
}

fn append_string(payload: &mut Vec<u8>, value: &str) {
    payload.extend_from_slice(&(value.len() as u64).to_be_bytes());
    payload.extend_from_slice(value.as_bytes());
}

fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, classification: FeedbackClass) -> FeedbackRecord {
        FeedbackRecord {
            feedback_id: id.into(),
            repository: "org/repo".into(),
            rule_id: "corp.java.rule".into(),
            finding_fingerprint: "finding-v0:abc".into(),
            unified_result_digest: [7; 32],
            classification,
            reporter: "reviewer".into(),
            rationale: "reviewed against the rule intent".into(),
            ticket: Some("ERF-123".into()),
            created_at_epoch_secs: 100,
        }
    }

    #[test]
    fn classification_and_regression_handoff_are_explicit() {
        assert!(!FeedbackClass::ConfirmedValidFinding.creates_regression_case());
        assert!(!FeedbackClass::DocumentationGap.creates_regression_case());
        assert!(!FeedbackClass::ScopeIssue.creates_regression_case());
        let record = record("feedback/fp/1", FeedbackClass::ConfirmedFalsePositive);
        let regression = record.regression_case().unwrap();
        assert_eq!(regression.expectation, RegressionExpectation::MustBeClean);
        assert_eq!(regression.feedback_digest, record.digest());
    }

    #[test]
    fn ledger_is_idempotent_and_audited() {
        let record = record("feedback/not-fp/1", FeedbackClass::ConfirmedValidFinding);
        let digest = record.digest();
        let mut ledger = FeedbackLedger::new();
        let first = ledger.submit(record.clone(), "reviewer", "triage", 1).unwrap();
        assert!(matches!(first, SubmitOutcome::Admitted(entry) if entry.digest == digest && entry.state == FeedbackState::Open && entry.regression_case.is_none()));
        let second = ledger.submit(record, "reviewer", "replay", 1).unwrap();
        assert!(matches!(second, SubmitOutcome::Idempotent(entry) if entry.digest == digest));
        assert_eq!(ledger.audit_log().len(), 1);
        assert_eq!(ledger.last_operation_sequence(), 1);
    }

    #[test]
    fn false_positive_creates_digest_only_regression_entry() {
        let record = record("feedback/fp/2", FeedbackClass::ConfirmedFalsePositive);
        let digest = record.digest();
        let mut ledger = FeedbackLedger::new();
        let outcome = ledger.submit(record, "reviewer", "confirmed false positive", 1).unwrap();
        let SubmitOutcome::Admitted(entry) = outcome else { panic!("first submission must admit") };
        assert_eq!(entry.digest, digest);
        assert_eq!(entry.state, FeedbackState::RegressionCreated);
        assert_eq!(entry.regression_case.unwrap().finding_fingerprint, "finding-v0:abc");
        assert!(!String::from_utf8_lossy(&entry.record.canonical_payload()).contains("src/") );
        assert!(!String::from_utf8_lossy(&entry.record.canonical_payload()).contains("class Secret") );
    }

    #[test]
    fn same_feedback_id_cannot_change_meaning_and_operations_are_monotonic() {
        let first = record("feedback/same-id", FeedbackClass::ScopeIssue);
        let mut changed = first.clone();
        changed.classification = FeedbackClass::ConfirmedFalsePositive;
        let mut ledger = FeedbackLedger::new();
        ledger.submit(first, "reviewer", "triage", 5).unwrap();
        assert!(matches!(ledger.submit(changed, "reviewer", "rewrite", 6), Err(FeedbackError::ConflictingIdentity(_))));
        assert!(matches!(ledger.submit(record("feedback/new", FeedbackClass::DocumentationGap), "reviewer", "stale", 5), Err(FeedbackError::StaleOperation { .. })));
    }

    #[test]
    fn malformed_feedback_and_audit_context_fail_closed() {
        let mut invalid = record("feedback/invalid", FeedbackClass::ScopeIssue);
        invalid.unified_result_digest = [0; 32];
        assert_eq!(invalid.validate(), Err(FeedbackError::ZeroResultDigest));
        let mut ledger = FeedbackLedger::new();
        assert_eq!(ledger.submit(record("feedback/actor", FeedbackClass::ScopeIssue), "", "reason", 1), Err(FeedbackError::EmptyActor));
        assert_eq!(ledger.submit(record("feedback/reason", FeedbackClass::ScopeIssue), "reviewer", "", 1), Err(FeedbackError::EmptyReason));
        assert!(matches!(ledger.entry([9; 32]), Err(FeedbackError::UnknownFeedback(_))));
    }

    #[test]
    fn canonical_digest_binds_all_feedback_fields() {
        let first = record("feedback/digest/1", FeedbackClass::DocumentationGap);
        let mut changed = first.clone();
        changed.rationale = "different rationale".into();
        assert_ne!(first.digest(), changed.digest());
        changed = first.clone();
        changed.unified_result_digest = [8; 32];
        assert_ne!(first.digest(), changed.digest());
    }
}
