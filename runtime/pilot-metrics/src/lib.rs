//! Digest-only, transport-neutral measurements for a controlled pilot.
//!
//! The ledger records summaries derived from validated UnifiedResults and
//! aggregate governance/feedback observations. It deliberately does not
//! retain source, paths, full Findings, SCM data, credentials, or transport
//! concerns. Adapters may persist or publish the records outside this crate.

use erf_feedback::FeedbackClass;
use erf_unified_result::{ResultError, ResultTrust, UnifiedResult};
use rf_gate::GateDecision;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const PILOT_METRICS_VERSION: &str = "pilot-metrics-v1";
pub const PILOT_METRICS_JOURNAL_VERSION: &str = "pilot-metrics-journal-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegressionOutcome {
    NotApplicable,
    Pending,
    Clean,
    FindingDetected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricRecordKind {
    Run,
    Feedback,
    Governance,
}

/// Distinguishes synthetic local commit sequences from the registered
/// read-only external repository observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationMode {
    LocalSimulation,
    LiveShadow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunMeasurement {
    pub measurement_id: String,
    pub mode: ObservationMode,
    pub repository_digest: [u8; 32],
    pub commit_digest: [u8; 32],
    pub unified_result_digest: [u8; 32],
    pub trust: ResultTrust,
    pub decision: GateDecision,
    pub complete: bool,
    pub new_finding_count: usize,
    pub historical_finding_count: usize,
    pub latency_millis: u64,
    pub peak_memory_bytes: u64,
    pub observed_at_epoch_secs: u64,
}

impl RunMeasurement {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = PILOT_METRICS_VERSION.as_bytes().to_vec();
        payload.push(0);
        payload.push(metric_kind_byte(MetricRecordKind::Run));
        append_string(&mut payload, &self.measurement_id);
        payload.push(observation_mode_byte(self.mode));
        payload.extend_from_slice(&self.repository_digest);
        payload.extend_from_slice(&self.commit_digest);
        payload.extend_from_slice(&self.unified_result_digest);
        payload.push(match self.trust { ResultTrust::LocalUnattested => 0, ResultTrust::Trusted => 1 });
        payload.push(gate_decision_byte(self.decision));
        payload.push(self.complete as u8);
        payload.extend_from_slice(&(self.new_finding_count as u64).to_be_bytes());
        payload.extend_from_slice(&(self.historical_finding_count as u64).to_be_bytes());
        payload.extend_from_slice(&self.latency_millis.to_be_bytes());
        payload.extend_from_slice(&self.peak_memory_bytes.to_be_bytes());
        payload.extend_from_slice(&self.observed_at_epoch_secs.to_be_bytes());
        payload
    }

    pub fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_payload()).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackMeasurement {
    pub measurement_id: String,
    pub classification: FeedbackClass,
    pub regression_outcome: RegressionOutcome,
    pub observed_at_epoch_secs: u64,
}

impl FeedbackMeasurement {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = PILOT_METRICS_VERSION.as_bytes().to_vec();
        payload.push(0);
        payload.push(metric_kind_byte(MetricRecordKind::Feedback));
        append_string(&mut payload, &self.measurement_id);
        payload.push(feedback_class_byte(self.classification));
        payload.push(regression_outcome_byte(self.regression_outcome));
        payload.extend_from_slice(&self.observed_at_epoch_secs.to_be_bytes());
        payload
    }

    pub fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_payload()).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceMeasurement {
    pub measurement_id: String,
    pub rollback_count: u64,
    pub active_exception_count: u64,
    pub oldest_active_exception_age_secs: u64,
    pub observed_at_epoch_secs: u64,
}

impl GovernanceMeasurement {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = PILOT_METRICS_VERSION.as_bytes().to_vec();
        payload.push(0);
        payload.push(metric_kind_byte(MetricRecordKind::Governance));
        append_string(&mut payload, &self.measurement_id);
        payload.extend_from_slice(&self.rollback_count.to_be_bytes());
        payload.extend_from_slice(&self.active_exception_count.to_be_bytes());
        payload.extend_from_slice(&self.oldest_active_exception_age_secs.to_be_bytes());
        payload.extend_from_slice(&self.observed_at_epoch_secs.to_be_bytes());
        payload
    }

    pub fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_payload()).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub audit_sequence: u64,
    pub operation_sequence: u64,
    pub kind: MetricRecordKind,
    pub record_digest: [u8; 32],
    pub actor: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetricOutcome<T> {
    Admitted(T),
    Idempotent(T),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PilotMetricsSnapshot {
    pub run_count: u64,
    pub trusted_run_count: u64,
    pub local_run_count: u64,
    pub complete_run_count: u64,
    pub inconclusive_run_count: u64,
    pub pass_count: u64,
    pub fail_count: u64,
    pub new_finding_count: u64,
    pub historical_finding_count: u64,
    pub repository_count: usize,
    pub false_positive_count: u64,
    pub regression_pending_count: u64,
    pub regression_clean_count: u64,
    pub regression_finding_count: u64,
    pub latest_rollback_count: Option<u64>,
    pub latest_active_exception_count: Option<u64>,
    pub latest_oldest_exception_age_secs: Option<u64>,
    pub total_latency_millis: u64,
    pub max_latency_millis: u64,
    pub max_peak_memory_bytes: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MetricsError {
    #[error("metric field is invalid: {0}")]
    InvalidField(&'static str),
    #[error("metric value is invalid: {0}")]
    InvalidMeasurement(&'static str),
    #[error("unified result is invalid: {0}")]
    InvalidResult(#[from] ResultError),
    #[error("metric id {0} already refers to different content")]
    ConflictingIdentity(String),
    #[error("metric operation sequence {received} is not newer than {current}")]
    StaleOperation { received: u64, current: u64 },
    #[error("metric journal is invalid: {0}")]
    InvalidJournal(&'static str),
}

pub struct PilotMetricsLedger {
    runs: Vec<RunMeasurement>,
    run_index: BTreeMap<String, usize>,
    feedback: Vec<FeedbackMeasurement>,
    feedback_index: BTreeMap<String, usize>,
    governance: Vec<GovernanceMeasurement>,
    governance_index: BTreeMap<String, usize>,
    audit: Vec<AuditRecord>,
    next_audit_sequence: u64,
    last_operation_sequence: u64,
}

impl PilotMetricsLedger {
    pub fn new() -> Self {
        Self {
            runs: Vec::new(),
            run_index: BTreeMap::new(),
            feedback: Vec::new(),
            feedback_index: BTreeMap::new(),
            governance: Vec::new(),
            governance_index: BTreeMap::new(),
            audit: Vec::new(),
            next_audit_sequence: 1,
            last_operation_sequence: 0,
        }
    }

    pub fn record_result(
        &mut self,
        measurement_id: &str,
        result: &UnifiedResult,
        latency_millis: u64,
        peak_memory_bytes: u64,
        observed_at_epoch_secs: u64,
        actor: &str,
        reason: &str,
        operation_sequence: u64,
    ) -> Result<MetricOutcome<RunMeasurement>, MetricsError> {
        self.record_result_with_mode(
            measurement_id,
            ObservationMode::LiveShadow,
            result,
            latency_millis,
            peak_memory_bytes,
            observed_at_epoch_secs,
            actor,
            reason,
            operation_sequence,
        )
    }

    pub fn record_result_with_mode(
        &mut self,
        measurement_id: &str,
        mode: ObservationMode,
        result: &UnifiedResult,
        latency_millis: u64,
        peak_memory_bytes: u64,
        observed_at_epoch_secs: u64,
        actor: &str,
        reason: &str,
        operation_sequence: u64,
    ) -> Result<MetricOutcome<RunMeasurement>, MetricsError> {
        result.validate()?;
        validate_text("measurement_id", measurement_id)?;
        validate_text("actor", actor)?;
        validate_text("reason", reason)?;
        if latency_millis == 0 { return Err(MetricsError::InvalidMeasurement("latency_millis")); }
        if peak_memory_bytes == 0 { return Err(MetricsError::InvalidMeasurement("peak_memory_bytes")); }
        let measurement = RunMeasurement {
            measurement_id: measurement_id.into(),
            mode,
            repository_digest: digest_text(&result.inputs.repository),
            commit_digest: digest_text(&result.inputs.commit),
            unified_result_digest: Sha256::digest(result.canonical_payload()).into(),
            trust: result.trust,
            decision: result.decision,
            complete: result.completeness.complete,
            new_finding_count: result.new_finding_count,
            historical_finding_count: result.historical_finding_count,
            latency_millis,
            peak_memory_bytes,
            observed_at_epoch_secs,
        };
        if let Some(index) = self.run_index.get(measurement_id) {
            let existing = &self.runs[*index];
            if existing == &measurement { return Ok(MetricOutcome::Idempotent(existing.clone())); }
            return Err(MetricsError::ConflictingIdentity(measurement_id.into()));
        }
        self.check_operation_sequence(operation_sequence)?;
        let digest = measurement.digest();
        self.run_index.insert(measurement_id.into(), self.runs.len());
        self.runs.push(measurement.clone());
        self.commit_operation(operation_sequence, MetricRecordKind::Run, digest, actor, reason);
        Ok(MetricOutcome::Admitted(measurement))
    }

    pub fn record_feedback(
        &mut self,
        measurement_id: &str,
        classification: FeedbackClass,
        regression_outcome: RegressionOutcome,
        observed_at_epoch_secs: u64,
        actor: &str,
        reason: &str,
        operation_sequence: u64,
    ) -> Result<MetricOutcome<FeedbackMeasurement>, MetricsError> {
        validate_text("measurement_id", measurement_id)?;
        validate_text("actor", actor)?;
        validate_text("reason", reason)?;
        if classification == FeedbackClass::ConfirmedFalsePositive {
            if regression_outcome == RegressionOutcome::NotApplicable {
                return Err(MetricsError::InvalidMeasurement("false_positive_regression_outcome"));
            }
        } else if regression_outcome != RegressionOutcome::NotApplicable {
            return Err(MetricsError::InvalidMeasurement("non_false_positive_regression_outcome"));
        }
        let measurement = FeedbackMeasurement { measurement_id: measurement_id.into(), classification, regression_outcome, observed_at_epoch_secs };
        if let Some(index) = self.feedback_index.get(measurement_id) {
            let existing = &self.feedback[*index];
            if existing == &measurement { return Ok(MetricOutcome::Idempotent(existing.clone())); }
            return Err(MetricsError::ConflictingIdentity(measurement_id.into()));
        }
        self.check_operation_sequence(operation_sequence)?;
        let digest = measurement.digest();
        self.feedback_index.insert(measurement_id.into(), self.feedback.len());
        self.feedback.push(measurement.clone());
        self.commit_operation(operation_sequence, MetricRecordKind::Feedback, digest, actor, reason);
        Ok(MetricOutcome::Admitted(measurement))
    }

    pub fn record_governance(
        &mut self,
        measurement_id: &str,
        rollback_count: u64,
        active_exception_count: u64,
        oldest_active_exception_age_secs: u64,
        observed_at_epoch_secs: u64,
        actor: &str,
        reason: &str,
        operation_sequence: u64,
    ) -> Result<MetricOutcome<GovernanceMeasurement>, MetricsError> {
        validate_text("measurement_id", measurement_id)?;
        validate_text("actor", actor)?;
        validate_text("reason", reason)?;
        if active_exception_count == 0 && oldest_active_exception_age_secs != 0 {
            return Err(MetricsError::InvalidMeasurement("oldest_active_exception_age_secs"));
        }
        let measurement = GovernanceMeasurement { measurement_id: measurement_id.into(), rollback_count, active_exception_count, oldest_active_exception_age_secs, observed_at_epoch_secs };
        if let Some(index) = self.governance_index.get(measurement_id) {
            let existing = &self.governance[*index];
            if existing == &measurement { return Ok(MetricOutcome::Idempotent(existing.clone())); }
            return Err(MetricsError::ConflictingIdentity(measurement_id.into()));
        }
        self.check_operation_sequence(operation_sequence)?;
        let digest = measurement.digest();
        self.governance_index.insert(measurement_id.into(), self.governance.len());
        self.governance.push(measurement.clone());
        self.commit_operation(operation_sequence, MetricRecordKind::Governance, digest, actor, reason);
        Ok(MetricOutcome::Admitted(measurement))
    }

    pub fn snapshot(&self) -> PilotMetricsSnapshot {
        let mut snapshot = PilotMetricsSnapshot::default();
        let mut repositories = BTreeSet::new();
        for run in &self.runs {
            snapshot.run_count += 1;
            if run.trust == ResultTrust::Trusted { snapshot.trusted_run_count += 1; } else { snapshot.local_run_count += 1; }
            if run.complete { snapshot.complete_run_count += 1; }
            if run.decision == GateDecision::Inconclusive { snapshot.inconclusive_run_count += 1; }
            match run.decision {
                GateDecision::Pass => snapshot.pass_count += 1,
                GateDecision::Fail => snapshot.fail_count += 1,
                GateDecision::Inconclusive => {}
            }
            snapshot.new_finding_count = snapshot.new_finding_count.saturating_add(run.new_finding_count as u64);
            snapshot.historical_finding_count = snapshot.historical_finding_count.saturating_add(run.historical_finding_count as u64);
            snapshot.total_latency_millis = snapshot.total_latency_millis.saturating_add(run.latency_millis);
            snapshot.max_latency_millis = snapshot.max_latency_millis.max(run.latency_millis);
            snapshot.max_peak_memory_bytes = snapshot.max_peak_memory_bytes.max(run.peak_memory_bytes);
            repositories.insert(run.repository_digest);
        }
        snapshot.repository_count = repositories.len();
        for feedback in &self.feedback {
            if feedback.classification != FeedbackClass::ConfirmedFalsePositive { continue; }
            snapshot.false_positive_count += 1;
            match feedback.regression_outcome {
                RegressionOutcome::Pending => snapshot.regression_pending_count += 1,
                RegressionOutcome::Clean => snapshot.regression_clean_count += 1,
                RegressionOutcome::FindingDetected => snapshot.regression_finding_count += 1,
                RegressionOutcome::NotApplicable => {}
            }
        }
        if let Some(governance) = self.governance.last() {
            snapshot.latest_rollback_count = Some(governance.rollback_count);
            snapshot.latest_active_exception_count = Some(governance.active_exception_count);
            snapshot.latest_oldest_exception_age_secs = Some(governance.oldest_active_exception_age_secs);
        }
        snapshot
    }

    pub fn run_trend(&self) -> &[RunMeasurement] { &self.runs }
    pub fn feedback_measurements(&self) -> &[FeedbackMeasurement] { &self.feedback }
    pub fn governance_measurements(&self) -> &[GovernanceMeasurement] { &self.governance }
    pub fn audit_log(&self) -> &[AuditRecord] { &self.audit }
    pub fn last_operation_sequence(&self) -> u64 { self.last_operation_sequence }

    /// Exports the ledger as a deterministic, digest-only observation journal.
    ///
    /// The journal is intentionally a binary value rather than a source or
    /// Finding serialization. It contains only the measurement summaries and
    /// append-only audit metadata needed to resume a local observation run.
    pub fn export_journal(&self) -> Result<Vec<u8>, MetricsError> {
        let mut bytes = PILOT_METRICS_JOURNAL_VERSION.as_bytes().to_vec();
        bytes.push(0);
        append_u64(&mut bytes, self.audit.len() as u64);
        let mut previous_operation = 0;
        for (index, audit) in self.audit.iter().enumerate() {
            if audit.audit_sequence != index as u64 + 1 {
                return Err(MetricsError::InvalidJournal("audit sequence"));
            }
            if audit.operation_sequence <= previous_operation {
                return Err(MetricsError::InvalidJournal("operation sequence"));
            }
            validate_text("actor", &audit.actor)?;
            validate_text("reason", &audit.reason)?;
            let record = self.record_for_audit(audit)
                .ok_or(MetricsError::InvalidJournal("audit record missing"))?;
            if record.digest() != audit.record_digest {
                return Err(MetricsError::InvalidJournal("audit digest"));
            }
            append_u64(&mut bytes, audit.audit_sequence);
            append_u64(&mut bytes, audit.operation_sequence);
            bytes.push(metric_kind_byte(audit.kind));
            bytes.extend_from_slice(&audit.record_digest);
            append_string(&mut bytes, &audit.actor);
            append_string(&mut bytes, &audit.reason);
            append_journal_record(&mut bytes, audit.kind, &record);
            previous_operation = audit.operation_sequence;
        }
        Ok(bytes)
    }

    /// Restores a ledger from a journal produced by [`Self::export_journal`].
    /// Every record, digest and sequence is checked before it becomes visible.
    pub fn from_journal(bytes: &[u8]) -> Result<Self, MetricsError> {
        let mut cursor = JournalCursor::new(bytes);
        cursor.expect_version(PILOT_METRICS_JOURNAL_VERSION)?;
        let count = cursor.u64()?;
        let count = usize::try_from(count).map_err(|_| MetricsError::InvalidJournal("record count"))?;
        let mut ledger = Self::new();
        let mut previous_operation = 0;
        for expected_audit_sequence in 1..=count as u64 {
            let audit_sequence = cursor.u64()?;
            if audit_sequence != expected_audit_sequence {
                return Err(MetricsError::InvalidJournal("audit sequence"));
            }
            let operation_sequence = cursor.u64()?;
            if operation_sequence == 0 || operation_sequence <= previous_operation {
                return Err(MetricsError::InvalidJournal("operation sequence"));
            }
            let kind = metric_kind_from_byte(cursor.byte()?)?;
            let record_digest = cursor.digest()?;
            let actor = cursor.string()?;
            let reason = cursor.string()?;
            validate_text("actor", &actor)?;
            validate_text("reason", &reason)?;
            let record = read_journal_record(&mut cursor, kind)?;
            if record.digest() != record_digest {
                return Err(MetricsError::InvalidJournal("record digest"));
            }
            ledger.admit_journal_record(record)?;
            ledger.audit.push(AuditRecord { audit_sequence, operation_sequence, kind, record_digest, actor, reason });
            previous_operation = operation_sequence;
        }
        cursor.finish()?;
        ledger.next_audit_sequence = count as u64 + 1;
        ledger.last_operation_sequence = previous_operation;
        Ok(ledger)
    }

    fn record_for_audit(&self, audit: &AuditRecord) -> Option<JournalRecord> {
        match audit.kind {
            MetricRecordKind::Run => self.runs.iter().find(|record| record.digest() == audit.record_digest).cloned().map(JournalRecord::Run),
            MetricRecordKind::Feedback => self.feedback.iter().find(|record| record.digest() == audit.record_digest).cloned().map(JournalRecord::Feedback),
            MetricRecordKind::Governance => self.governance.iter().find(|record| record.digest() == audit.record_digest).cloned().map(JournalRecord::Governance),
        }
    }

    fn admit_journal_record(&mut self, record: JournalRecord) -> Result<(), MetricsError> {
        match record {
            JournalRecord::Run(measurement) => {
                validate_run_measurement(&measurement)?;
                if self.run_index.contains_key(&measurement.measurement_id) {
                    return Err(MetricsError::ConflictingIdentity(measurement.measurement_id));
                }
                self.run_index.insert(measurement.measurement_id.clone(), self.runs.len());
                self.runs.push(measurement);
            }
            JournalRecord::Feedback(measurement) => {
                validate_feedback_measurement(&measurement)?;
                if self.feedback_index.contains_key(&measurement.measurement_id) {
                    return Err(MetricsError::ConflictingIdentity(measurement.measurement_id));
                }
                self.feedback_index.insert(measurement.measurement_id.clone(), self.feedback.len());
                self.feedback.push(measurement);
            }
            JournalRecord::Governance(measurement) => {
                validate_governance_measurement(&measurement)?;
                if self.governance_index.contains_key(&measurement.measurement_id) {
                    return Err(MetricsError::ConflictingIdentity(measurement.measurement_id));
                }
                self.governance_index.insert(measurement.measurement_id.clone(), self.governance.len());
                self.governance.push(measurement);
            }
        }
        Ok(())
    }

    fn check_operation_sequence(&self, received: u64) -> Result<(), MetricsError> {
        if received <= self.last_operation_sequence {
            return Err(MetricsError::StaleOperation { received, current: self.last_operation_sequence });
        }
        Ok(())
    }

    fn commit_operation(&mut self, operation_sequence: u64, kind: MetricRecordKind, digest: [u8; 32], actor: &str, reason: &str) {
        self.last_operation_sequence = operation_sequence;
        self.audit.push(AuditRecord { audit_sequence: self.next_audit_sequence, operation_sequence, kind, record_digest: digest, actor: actor.into(), reason: reason.into() });
        self.next_audit_sequence += 1;
    }
}

impl Default for PilotMetricsLedger {
    fn default() -> Self { Self::new() }
}

fn validate_text(name: &'static str, value: &str) -> Result<(), MetricsError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) { return Err(MetricsError::InvalidField(name)); }
    Ok(())
}

fn validate_run_measurement(measurement: &RunMeasurement) -> Result<(), MetricsError> {
    validate_text("measurement_id", &measurement.measurement_id)?;
    if measurement.latency_millis == 0 { return Err(MetricsError::InvalidMeasurement("latency_millis")); }
    if measurement.peak_memory_bytes == 0 { return Err(MetricsError::InvalidMeasurement("peak_memory_bytes")); }
    if measurement.decision == GateDecision::Pass && !measurement.complete {
        return Err(MetricsError::InvalidMeasurement("incomplete_pass"));
    }
    Ok(())
}

fn validate_feedback_measurement(measurement: &FeedbackMeasurement) -> Result<(), MetricsError> {
    validate_text("measurement_id", &measurement.measurement_id)?;
    if measurement.classification == FeedbackClass::ConfirmedFalsePositive {
        if measurement.regression_outcome == RegressionOutcome::NotApplicable {
            return Err(MetricsError::InvalidMeasurement("false_positive_regression_outcome"));
        }
    } else if measurement.regression_outcome != RegressionOutcome::NotApplicable {
        return Err(MetricsError::InvalidMeasurement("non_false_positive_regression_outcome"));
    }
    Ok(())
}

fn validate_governance_measurement(measurement: &GovernanceMeasurement) -> Result<(), MetricsError> {
    validate_text("measurement_id", &measurement.measurement_id)?;
    if measurement.active_exception_count == 0 && measurement.oldest_active_exception_age_secs != 0 {
        return Err(MetricsError::InvalidMeasurement("oldest_active_exception_age_secs"));
    }
    Ok(())
}

fn digest_text(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

fn append_string(payload: &mut Vec<u8>, value: &str) {
    payload.extend_from_slice(&(value.len() as u64).to_be_bytes());
    payload.extend_from_slice(value.as_bytes());
}

fn metric_kind_byte(kind: MetricRecordKind) -> u8 {
    match kind { MetricRecordKind::Run => 0, MetricRecordKind::Feedback => 1, MetricRecordKind::Governance => 2 }
}

fn gate_decision_byte(decision: GateDecision) -> u8 {
    match decision { GateDecision::Pass => 0, GateDecision::Fail => 1, GateDecision::Inconclusive => 2 }
}

fn feedback_class_byte(classification: FeedbackClass) -> u8 {
    match classification {
        FeedbackClass::ConfirmedValidFinding => 0,
        FeedbackClass::DocumentationGap => 1,
        FeedbackClass::ScopeIssue => 2,
        FeedbackClass::ConfirmedFalsePositive => 3,
    }
}

fn regression_outcome_byte(outcome: RegressionOutcome) -> u8 {
    match outcome { RegressionOutcome::NotApplicable => 0, RegressionOutcome::Pending => 1, RegressionOutcome::Clean => 2, RegressionOutcome::FindingDetected => 3 }
}

#[derive(Debug, Clone)]
enum JournalRecord {
    Run(RunMeasurement),
    Feedback(FeedbackMeasurement),
    Governance(GovernanceMeasurement),
}

impl JournalRecord {
    fn digest(&self) -> [u8; 32] {
        match self {
            Self::Run(record) => record.digest(),
            Self::Feedback(record) => record.digest(),
            Self::Governance(record) => record.digest(),
        }
    }
}

fn append_journal_record(bytes: &mut Vec<u8>, kind: MetricRecordKind, record: &JournalRecord) {
    match (kind, record) {
        (MetricRecordKind::Run, JournalRecord::Run(record)) => {
            append_string(bytes, &record.measurement_id);
            bytes.push(observation_mode_byte(record.mode));
            bytes.extend_from_slice(&record.repository_digest);
            bytes.extend_from_slice(&record.commit_digest);
            bytes.extend_from_slice(&record.unified_result_digest);
            bytes.push(match record.trust { ResultTrust::LocalUnattested => 0, ResultTrust::Trusted => 1 });
            bytes.push(gate_decision_byte(record.decision));
            bytes.push(record.complete as u8);
            append_u64(bytes, record.new_finding_count as u64);
            append_u64(bytes, record.historical_finding_count as u64);
            append_u64(bytes, record.latency_millis);
            append_u64(bytes, record.peak_memory_bytes);
            append_u64(bytes, record.observed_at_epoch_secs);
        }
        (MetricRecordKind::Feedback, JournalRecord::Feedback(record)) => {
            append_string(bytes, &record.measurement_id);
            bytes.push(feedback_class_byte(record.classification));
            bytes.push(regression_outcome_byte(record.regression_outcome));
            append_u64(bytes, record.observed_at_epoch_secs);
        }
        (MetricRecordKind::Governance, JournalRecord::Governance(record)) => {
            append_string(bytes, &record.measurement_id);
            append_u64(bytes, record.rollback_count);
            append_u64(bytes, record.active_exception_count);
            append_u64(bytes, record.oldest_active_exception_age_secs);
            append_u64(bytes, record.observed_at_epoch_secs);
        }
        _ => unreachable!("audit kind and journal record must match"),
    }
}

fn read_journal_record(cursor: &mut JournalCursor<'_>, kind: MetricRecordKind) -> Result<JournalRecord, MetricsError> {
    match kind {
        MetricRecordKind::Run => Ok(JournalRecord::Run(RunMeasurement {
            measurement_id: cursor.string()?,
            mode: observation_mode_from_byte(cursor.byte()?)?,
            repository_digest: cursor.digest()?,
            commit_digest: cursor.digest()?,
            unified_result_digest: cursor.digest()?,
            trust: result_trust_from_byte(cursor.byte()?)?,
            decision: gate_decision_from_byte(cursor.byte()?)?,
            complete: cursor.bool()?,
            new_finding_count: cursor.usize()?,
            historical_finding_count: cursor.usize()?,
            latency_millis: cursor.u64()?,
            peak_memory_bytes: cursor.u64()?,
            observed_at_epoch_secs: cursor.u64()?,
        })),
        MetricRecordKind::Feedback => Ok(JournalRecord::Feedback(FeedbackMeasurement {
            measurement_id: cursor.string()?,
            classification: feedback_class_from_byte(cursor.byte()?)?,
            regression_outcome: regression_outcome_from_byte(cursor.byte()?)?,
            observed_at_epoch_secs: cursor.u64()?,
        })),
        MetricRecordKind::Governance => Ok(JournalRecord::Governance(GovernanceMeasurement {
            measurement_id: cursor.string()?,
            rollback_count: cursor.u64()?,
            active_exception_count: cursor.u64()?,
            oldest_active_exception_age_secs: cursor.u64()?,
            observed_at_epoch_secs: cursor.u64()?,
        })),
    }
}

fn append_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn metric_kind_from_byte(value: u8) -> Result<MetricRecordKind, MetricsError> {
    match value {
        0 => Ok(MetricRecordKind::Run),
        1 => Ok(MetricRecordKind::Feedback),
        2 => Ok(MetricRecordKind::Governance),
        _ => Err(MetricsError::InvalidJournal("record kind")),
    }
}

fn observation_mode_byte(mode: ObservationMode) -> u8 {
    match mode {
        ObservationMode::LocalSimulation => 0,
        ObservationMode::LiveShadow => 1,
    }
}

fn observation_mode_from_byte(value: u8) -> Result<ObservationMode, MetricsError> {
    match value {
        0 => Ok(ObservationMode::LocalSimulation),
        1 => Ok(ObservationMode::LiveShadow),
        _ => Err(MetricsError::InvalidJournal("observation mode")),
    }
}

fn result_trust_from_byte(value: u8) -> Result<ResultTrust, MetricsError> {
    match value {
        0 => Ok(ResultTrust::LocalUnattested),
        1 => Ok(ResultTrust::Trusted),
        _ => Err(MetricsError::InvalidJournal("result trust")),
    }
}

fn gate_decision_from_byte(value: u8) -> Result<GateDecision, MetricsError> {
    match value {
        0 => Ok(GateDecision::Pass),
        1 => Ok(GateDecision::Fail),
        2 => Ok(GateDecision::Inconclusive),
        _ => Err(MetricsError::InvalidJournal("gate decision")),
    }
}

fn feedback_class_from_byte(value: u8) -> Result<FeedbackClass, MetricsError> {
    match value {
        0 => Ok(FeedbackClass::ConfirmedValidFinding),
        1 => Ok(FeedbackClass::DocumentationGap),
        2 => Ok(FeedbackClass::ScopeIssue),
        3 => Ok(FeedbackClass::ConfirmedFalsePositive),
        _ => Err(MetricsError::InvalidJournal("feedback class")),
    }
}

fn regression_outcome_from_byte(value: u8) -> Result<RegressionOutcome, MetricsError> {
    match value {
        0 => Ok(RegressionOutcome::NotApplicable),
        1 => Ok(RegressionOutcome::Pending),
        2 => Ok(RegressionOutcome::Clean),
        3 => Ok(RegressionOutcome::FindingDetected),
        _ => Err(MetricsError::InvalidJournal("regression outcome")),
    }
}

struct JournalCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> JournalCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self { Self { bytes, offset: 0 } }

    fn take(&mut self, length: usize) -> Result<&'a [u8], MetricsError> {
        let end = self.offset.checked_add(length).ok_or(MetricsError::InvalidJournal("length"))?;
        if end > self.bytes.len() { return Err(MetricsError::InvalidJournal("truncated")); }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn expect_version(&mut self, version: &str) -> Result<(), MetricsError> {
        let expected = version.as_bytes();
        if self.take(expected.len())? != expected || self.byte()? != 0 {
            return Err(MetricsError::InvalidJournal("version"));
        }
        Ok(())
    }

    fn byte(&mut self) -> Result<u8, MetricsError> { Ok(self.take(1)?[0]) }

    fn bool(&mut self) -> Result<bool, MetricsError> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(MetricsError::InvalidJournal("boolean")),
        }
    }

    fn u64(&mut self) -> Result<u64, MetricsError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().expect("fixed length")))
    }

    fn usize(&mut self) -> Result<usize, MetricsError> {
        usize::try_from(self.u64()?).map_err(|_| MetricsError::InvalidJournal("usize"))
    }

    fn digest(&mut self) -> Result<[u8; 32], MetricsError> {
        Ok(self.take(32)?.try_into().expect("fixed length"))
    }

    fn string(&mut self) -> Result<String, MetricsError> {
        let length = self.usize()?;
        let value = self.take(length)?;
        String::from_utf8(value.to_vec()).map_err(|_| MetricsError::InvalidJournal("utf8"))
    }

    fn finish(&self) -> Result<(), MetricsError> {
        if self.offset == self.bytes.len() { Ok(()) } else { Err(MetricsError::InvalidJournal("trailing bytes")) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erf_unified_result::{AuditEventKind, AuditTrail, ResultInputs};
    use rf_gate::{BaselineState, Completeness, ExceptionDecision, Finding, FindingAssessment, FindingLocationKind, GatePreview};

    fn inputs() -> ResultInputs {
        ResultInputs {
            repository: "org/repo".into(),
            commit: "abc123".into(),
            policy_digest: "f".repeat(64),
            rule_bundle_digest: "a".repeat(64),
            execution_lock_digest: "b".repeat(64),
            runner_release_digest: "c".repeat(64),
            workspace_descriptor_digest: "d".repeat(64),
        }
    }

    fn preview(decision: GateDecision, complete: bool) -> GatePreview {
        let finding = Finding {
            fingerprint_version: rf_gate::FINGERPRINT_VERSION,
            fingerprint: "finding-v0:abc".into(),
            rule_id: "corp.rule".into(),
            location_kind: FindingLocationKind::SourceSpan,
            path: "src/A.java".into(),
            semantic_anchor: "source-span|src/A.java|1|2".into(),
            message: "finding".into(),
        };
        GatePreview {
            decision,
            findings: vec![finding.clone()],
            completeness: Completeness { planned: 1, accounted: if complete { 1 } else { 0 }, failed: 0, missing: if complete { 0 } else { 1 }, observation_errors: 0, complete },
            canonical_set_version: rf_gate::CANONICAL_SET_VERSION,
            trust: "LOCAL_UNATTESTED",
            input_summary: "test".into(),
            assessments: vec![FindingAssessment { fingerprint: finding.fingerprint, baseline: BaselineState::New, exception: ExceptionDecision::NotApplicable }],
            semantic_result_digest: [7; 32],
        }
    }

    fn audit() -> AuditTrail {
        let mut audit = AuditTrail::new();
        audit.append(AuditEventKind::ScanPlanned, [1; 32]);
        audit.append(AuditEventKind::ExecutionCompleted, [2; 32]);
        audit.append(AuditEventKind::FindingSetComputed, [7; 32]);
        audit.append(AuditEventKind::GateEvaluated, [7; 32]);
        audit
    }

    fn local(decision: GateDecision, complete: bool) -> UnifiedResult {
        UnifiedResult::from_preview(inputs(), &preview(decision, complete), audit()).unwrap()
    }

    fn trusted_pass() -> UnifiedResult {
        let result = local(GateDecision::Pass, true);
        UnifiedResult { trust: ResultTrust::Trusted, gate_attestation_subject: Some([8; 32]), ..result }
    }

    #[test]
    fn result_measurements_aggregate_trust_tri_state_counts_and_trend() {
        let mut ledger = PilotMetricsLedger::new();
        ledger.record_result("run-1", &trusted_pass(), 7, 9, 100, "runner", "scan", 1).unwrap();
        ledger.record_result("run-2", &local(GateDecision::Fail, true), 11, 12, 200, "runner", "scan", 2).unwrap();
        ledger.record_result("run-3", &local(GateDecision::Inconclusive, false), 13, 14, 300, "runner", "crash", 3).unwrap();
        let snapshot = ledger.snapshot();
        assert_eq!(snapshot.run_count, 3);
        assert_eq!(snapshot.trusted_run_count, 1);
        assert_eq!(snapshot.local_run_count, 2);
        assert_eq!(snapshot.complete_run_count, 2);
        assert_eq!(snapshot.inconclusive_run_count, 1);
        assert_eq!(snapshot.pass_count, 1);
        assert_eq!(snapshot.fail_count, 1);
        assert_eq!(snapshot.new_finding_count, 3);
        assert_eq!(snapshot.historical_finding_count, 0);
        assert_eq!(snapshot.repository_count, 1);
        assert_eq!(snapshot.total_latency_millis, 31);
        assert_eq!(snapshot.max_peak_memory_bytes, 14);
        assert_eq!(ledger.run_trend().iter().map(|run| run.measurement_id.as_str()).collect::<Vec<_>>(), vec!["run-1", "run-2", "run-3"]);
    }

    #[test]
    fn replay_is_idempotent_conflict_is_rejected_and_audit_is_monotonic() {
        let result = trusted_pass();
        let mut ledger = PilotMetricsLedger::new();
        let first = ledger.record_result("same", &result, 7, 9, 100, "runner", "scan", 4).unwrap();
        assert!(matches!(first, MetricOutcome::Admitted(_)));
        let replay = ledger.record_result("same", &result, 7, 9, 100, "other", "replay", 4).unwrap();
        assert!(matches!(replay, MetricOutcome::Idempotent(_)));
        assert!(matches!(ledger.record_result("same", &result, 8, 9, 100, "runner", "rewrite", 5), Err(MetricsError::ConflictingIdentity(_))));
        assert_eq!(ledger.audit_log().len(), 1);
        assert_eq!(ledger.last_operation_sequence(), 4);
        assert!(matches!(ledger.record_result("new", &result, 7, 9, 100, "runner", "stale", 4), Err(MetricsError::StaleOperation { .. })));
    }

    #[test]
    fn feedback_regression_and_governance_snapshots_are_aggregated() {
        let mut ledger = PilotMetricsLedger::new();
        ledger.record_feedback("fp-1", FeedbackClass::ConfirmedFalsePositive, RegressionOutcome::Clean, 100, "reviewer", "confirmed", 1).unwrap();
        ledger.record_feedback("fp-2", FeedbackClass::ConfirmedFalsePositive, RegressionOutcome::Pending, 200, "reviewer", "queued", 2).unwrap();
        ledger.record_feedback("scope-1", FeedbackClass::ScopeIssue, RegressionOutcome::NotApplicable, 300, "reviewer", "scope", 3).unwrap();
        ledger.record_governance("gov-1", 2, 3, 86400, 400, "owner", "snapshot", 4).unwrap();
        let snapshot = ledger.snapshot();
        assert_eq!(snapshot.false_positive_count, 2);
        assert_eq!(snapshot.regression_clean_count, 1);
        assert_eq!(snapshot.regression_pending_count, 1);
        assert_eq!(snapshot.latest_rollback_count, Some(2));
        assert_eq!(snapshot.latest_active_exception_count, Some(3));
        assert_eq!(snapshot.latest_oldest_exception_age_secs, Some(86400));
    }

    #[test]
    fn invalid_result_measurement_fails_closed() {
        let mut ledger = PilotMetricsLedger::new();
        assert!(matches!(ledger.record_result("zero-latency", &trusted_pass(), 0, 9, 100, "runner", "scan", 1), Err(MetricsError::InvalidMeasurement("latency_millis"))));
        assert!(matches!(ledger.record_result("zero-memory", &trusted_pass(), 7, 0, 100, "runner", "scan", 1), Err(MetricsError::InvalidMeasurement("peak_memory_bytes"))));
        assert!(matches!(ledger.record_feedback("bad-fp", FeedbackClass::ConfirmedFalsePositive, RegressionOutcome::NotApplicable, 100, "reviewer", "bad", 1), Err(MetricsError::InvalidMeasurement("false_positive_regression_outcome"))));
        assert!(matches!(ledger.record_governance("bad-gov", 1, 0, 1, 100, "owner", "bad", 1), Err(MetricsError::InvalidMeasurement("oldest_active_exception_age_secs"))));
    }

    #[test]
    fn metric_payloads_are_digest_only_and_deterministic() {
        let result = trusted_pass();
        let mut first = PilotMetricsLedger::new();
        let mut second = PilotMetricsLedger::new();
        first.record_result("run", &result, 7, 9, 100, "runner", "scan", 1).unwrap();
        second.record_result("run", &result, 7, 9, 100, "runner", "scan", 1).unwrap();
        let first_run = &first.run_trend()[0];
        let second_run = &second.run_trend()[0];
        assert_eq!(first_run, second_run);
        assert_eq!(first_run.digest(), second_run.digest());
        let payload = first_run.canonical_payload();
        let bytes = String::from_utf8_lossy(&payload);
        assert!(!bytes.contains("org/repo"));
        assert!(!bytes.contains("src/A.java"));
        assert!(!bytes.contains("finding"));
    }

    #[test]
    fn journal_round_trip_preserves_order_snapshot_and_digest() {
        let mut ledger = PilotMetricsLedger::new();
        ledger.record_result("run", &trusted_pass(), 7, 9, 100, "runner", "scan", 1).unwrap();
        ledger.record_feedback("fp", FeedbackClass::ConfirmedFalsePositive, RegressionOutcome::Clean, 200, "reviewer", "confirmed", 2).unwrap();
        ledger.record_governance("gov", 1, 2, 60, 300, "owner", "snapshot", 3).unwrap();

        let journal = ledger.export_journal().unwrap();
        assert!(!journal.windows(7).any(|window| window == b"org/repo"));
        assert!(!journal.windows(10).any(|window| window == b"src/A.java"));
        let restored = PilotMetricsLedger::from_journal(&journal).unwrap();

        assert_eq!(restored.snapshot(), ledger.snapshot());
        assert_eq!(restored.audit_log(), ledger.audit_log());
        assert_eq!(restored.export_journal().unwrap(), journal);
    }

    #[test]
    fn observation_mode_is_digest_bound_and_journal_roundtrips() {
        let result = trusted_pass();
        let mut ledger = PilotMetricsLedger::new();
        ledger.record_result_with_mode("local", ObservationMode::LocalSimulation, &result, 7, 9, 100, "runner", "local", 1).unwrap();
        ledger.record_result_with_mode("live", ObservationMode::LiveShadow, &result, 7, 9, 100, "runner", "live", 2).unwrap();

        assert_ne!(ledger.run_trend()[0].digest(), ledger.run_trend()[1].digest());
        let restored = PilotMetricsLedger::from_journal(&ledger.export_journal().unwrap()).unwrap();
        assert_eq!(restored.run_trend()[0].mode, ObservationMode::LocalSimulation);
        assert_eq!(restored.run_trend()[1].mode, ObservationMode::LiveShadow);
    }

    #[test]
    fn journal_rejects_tampering_and_truncation() {
        let mut ledger = PilotMetricsLedger::new();
        ledger.record_result("run", &trusted_pass(), 7, 9, 100, "runner", "scan", 1).unwrap();
        let journal = ledger.export_journal().unwrap();

        let mut tampered = journal.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(matches!(PilotMetricsLedger::from_journal(&tampered), Err(MetricsError::InvalidJournal("record digest"))));
        assert!(matches!(PilotMetricsLedger::from_journal(&journal[..journal.len() - 1]), Err(MetricsError::InvalidJournal("truncated"))));
    }

    #[test]
    fn restored_journal_keeps_replay_and_sequence_guards() {
        let result = trusted_pass();
        let mut ledger = PilotMetricsLedger::new();
        ledger.record_result("run", &result, 7, 9, 100, "runner", "scan", 1).unwrap();
        let mut restored = PilotMetricsLedger::from_journal(&ledger.export_journal().unwrap()).unwrap();

        assert!(matches!(restored.record_result("run", &result, 7, 9, 100, "different", "replay", 1), Ok(MetricOutcome::Idempotent(_))));
        assert!(matches!(restored.record_result("new", &result, 7, 9, 100, "runner", "stale", 1), Err(MetricsError::StaleOperation { .. })));
        assert!(matches!(restored.record_result("new", &result, 8, 9, 101, "runner", "scan", 2), Ok(MetricOutcome::Admitted(_))));
    }
}
