//! Read-only reconciliation for one Backend Host execute attempt.
//!
//! The wire contract already makes `ExecuteComplete` the host's accounting
//! statement.  This module deliberately does not add any protocol fields or
//! make an authority/trust decision; it gives evidence and presentation
//! layers a small, deterministic view over the facts they already collected.
//! Missing, duplicate, or contradictory facts never become a verified
//! attempt.  A missing terminal frame is reported as `INCONCLUSIVE` when no
//! stronger contradiction is present, while contradictory facts are
//! `FAIL_CLOSED`.

use crate::pb;
use prost::Message;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;

/// Read-only input collected while consuming one Execute stream.
#[derive(Debug, Clone, Copy)]
pub struct AttemptIntegrityInput<'a> {
    /// Unit IDs accepted by the runtime's Plan/Execute boundary.
    pub planned_unit_ids: &'a [String],
    /// Every UnitStatus observed before the terminal frame (including failed
    /// statuses, if any).
    pub statuses: &'a [pb::UnitStatus],
    /// Number of BackendObservation events observed in the stream.
    pub observation_events: u64,
    /// The terminal ExecuteComplete, when one was received.
    pub completion: Option<&'a pb::ExecuteComplete>,
    /// Digest computed by the consumer from the received semantic event
    /// sequence.  The Host's value is read from `completion.attempt_digest`.
    pub computed_attempt_digest: Option<&'a [u8]>,
}

/// Outcome of a read-only attempt reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptIntegrityStatus {
    /// All planned units and terminal accounting facts agree, and both sides
    /// have the same canonical semantic attempt digest.
    Verified,
    /// A terminal frame is absent (or a digest needed by the dashboard was
    /// not available), so the attempt cannot be treated as complete.
    Inconclusive,
    /// The observed facts contradict one another.  Do not consume findings or
    /// promote this attempt based on this summary.
    FailClosed,
}

impl AttemptIntegrityStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::Inconclusive => "INCONCLUSIVE",
            Self::FailClosed => "FAIL_CLOSED",
        }
    }
}

impl fmt::Display for AttemptIntegrityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Deterministic, digest-only dashboard data for one execute attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptIntegrityDashboard {
    pub status: AttemptIntegrityStatus,
    pub planned: usize,
    pub status_events: usize,
    pub unique_statuses: usize,
    pub executed_statuses: usize,
    pub failed_statuses: usize,
    pub observation_events: u64,
    pub completion_units_reported: Option<u64>,
    pub completion_observation_count: Option<u64>,
    pub attempt_digest: Option<Vec<u8>>,
    pub computed_attempt_digest: Option<Vec<u8>>,
    /// Stable machine-readable reason codes, sorted and deduplicated.
    pub reasons: Vec<String>,
}

impl AttemptIntegrityDashboard {
    /// Reconciles planned IDs, status events, ExecuteComplete accounting, and
    /// the independently computed semantic attempt digest.  This function is
    /// intentionally side-effect free and never changes a Gate decision.
    pub fn reconcile(input: AttemptIntegrityInput<'_>) -> Self {
        let planned_set: BTreeSet<&str> = input
            .planned_unit_ids
            .iter()
            .map(String::as_str)
            .collect();
        let status_set: BTreeSet<&str> = input
            .statuses
            .iter()
            .map(|status| status.unit_id.as_str())
            .collect();
        let duplicate_planned = input.planned_unit_ids.len() != planned_set.len();
        let duplicate_statuses = input.statuses.len() != status_set.len();

        let mut reasons = Vec::new();
        let mut fail_closed = false;
        let mut inconclusive = false;
        let reason = |reasons: &mut Vec<String>, value: &str| {
            if !reasons.iter().any(|existing| existing == value) {
                reasons.push(value.to_owned());
            }
        };

        if duplicate_planned {
            fail_closed = true;
            reason(&mut reasons, "DUPLICATE_PLANNED_UNIT");
        }
        if duplicate_statuses {
            fail_closed = true;
            reason(&mut reasons, "DUPLICATE_UNIT_STATUS");
        }
        if input
            .planned_unit_ids
            .iter()
            .any(String::is_empty)
        {
            fail_closed = true;
            reason(&mut reasons, "EMPTY_PLANNED_UNIT_ID");
        }
        if input.statuses.iter().any(|status| status.unit_id.is_empty()) {
            fail_closed = true;
            reason(&mut reasons, "EMPTY_STATUS_UNIT_ID");
        }

        if planned_set != status_set {
            fail_closed = true;
            reason(&mut reasons, "PLANNED_STATUS_SET_MISMATCH");
        }

        let mut executed_statuses = 0;
        let mut failed_statuses = 0;
        for status in input.statuses {
            match pb::UnitOutcome::try_from(status.outcome) {
                Ok(pb::UnitOutcome::Executed) => executed_statuses += 1,
                Ok(_) => {
                    failed_statuses += 1;
                    fail_closed = true;
                    reason(&mut reasons, "FAILED_UNIT_STATUS");
                }
                Err(_) => {
                    fail_closed = true;
                    reason(&mut reasons, "INVALID_UNIT_OUTCOME");
                }
            }
        }

        let completion_units_reported = input.completion.map(|completion| completion.units_reported);
        let completion_observation_count =
            input.completion.map(|completion| completion.observation_count);
        let attempt_digest = input
            .completion
            .map(|completion| completion.attempt_digest.clone());
        let computed_attempt_digest = input.computed_attempt_digest.map(<[u8]>::to_vec);

        match input.completion {
            None => {
                inconclusive = true;
                reason(&mut reasons, "MISSING_EXECUTE_COMPLETE");
            }
            Some(completion) => {
                if completion.units_reported != input.statuses.len() as u64
                    || completion.units_reported != input.planned_unit_ids.len() as u64
                {
                    fail_closed = true;
                    reason(&mut reasons, "UNITS_REPORTED_MISMATCH");
                }
                if completion.observation_count != input.observation_events {
                    fail_closed = true;
                    reason(&mut reasons, "OBSERVATION_COUNT_MISMATCH");
                }
                if completion.attempt_digest.len() != 32 {
                    fail_closed = true;
                    reason(&mut reasons, "INVALID_ATTEMPT_DIGEST_LENGTH");
                }
                match input.computed_attempt_digest {
                    None => {
                        inconclusive = true;
                        reason(&mut reasons, "MISSING_COMPUTED_ATTEMPT_DIGEST");
                    }
                    Some(computed) if computed.len() != 32 => {
                        fail_closed = true;
                        reason(&mut reasons, "INVALID_COMPUTED_ATTEMPT_DIGEST_LENGTH");
                    }
                    Some(computed) if computed != completion.attempt_digest.as_slice() => {
                        fail_closed = true;
                        reason(&mut reasons, "ATTEMPT_DIGEST_MISMATCH");
                    }
                    Some(_) => {}
                }
            }
        }

        reasons.sort();
        let status = if fail_closed {
            AttemptIntegrityStatus::FailClosed
        } else if inconclusive {
            AttemptIntegrityStatus::Inconclusive
        } else {
            AttemptIntegrityStatus::Verified
        };

        Self {
            status,
            planned: input.planned_unit_ids.len(),
            status_events: input.statuses.len(),
            unique_statuses: status_set.len(),
            executed_statuses,
            failed_statuses,
            observation_events: input.observation_events,
            completion_units_reported,
            completion_observation_count,
            attempt_digest,
            computed_attempt_digest,
            reasons,
        }
    }

    pub fn is_verified(&self) -> bool {
        self.status == AttemptIntegrityStatus::Verified
    }

    /// A compact, deterministic line suitable for a local evidence log or a
    /// text dashboard.  Raw observations are intentionally not included.
    pub fn render_line(&self) -> String {
        format!(
            "attempt_integrity status={} planned={} status_events={} unique_statuses={} executed={} failed={} observations={} completion_units={} completion_observations={} attempt_digest={} computed_attempt_digest={} reasons={}",
            self.status,
            self.planned,
            self.status_events,
            self.unique_statuses,
            self.executed_statuses,
            self.failed_statuses,
            self.observation_events,
            self.completion_units_reported
                .map_or_else(|| "missing".into(), |value| value.to_string()),
            self.completion_observation_count
                .map_or_else(|| "missing".into(), |value| value.to_string()),
            self.attempt_digest
                .as_deref()
                .map_or_else(|| "missing".into(), hex_digest),
            self.computed_attempt_digest
                .as_deref()
                .map_or_else(|| "missing".into(), hex_digest),
            if self.reasons.is_empty() {
                "none".into()
            } else {
                self.reasons.join(",")
            },
        )
    }
}

/// Computes the canonical semantic attempt digest defined by ADR-0179.
/// Keeping this helper public lets a local evidence/dashboard consumer
/// independently recompute the Host's `ExecuteComplete.attempt_digest`.
pub fn canonical_attempt_digest(
    events: &[pb::ExecuteEvent],
    units_reported: u64,
    observation_count: u64,
) -> Vec<u8> {
    let mut digest = semantic_attempt_hasher();
    for event in events {
        append_length_prefixed(&mut digest, &event.encode_to_vec());
    }
    append_length_prefixed(&mut digest, &units_reported.to_be_bytes());
    append_length_prefixed(&mut digest, &observation_count.to_be_bytes());
    digest.finalize().to_vec()
}

fn semantic_attempt_hasher() -> Sha256 {
    let mut hasher = Sha256::new();
    hasher.update(b"erf-execute-attempt-v1\0");
    hasher
}

fn append_length_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(unit_id: &str, outcome: pb::UnitOutcome) -> pb::UnitStatus {
        pb::UnitStatus {
            unit_id: unit_id.into(),
            outcome: outcome as i32,
            reason_code: 0,
            detail: String::new(),
        }
    }

    fn event(unit_id: &str) -> pb::ExecuteEvent {
        pb::ExecuteEvent {
            event: Some(pb::execute_event::Event::UnitStatus(status(
                unit_id,
                pb::UnitOutcome::Executed,
            ))),
        }
    }

    fn events_digest(events: &[pb::ExecuteEvent], units: u64, observations: u64) -> Vec<u8> {
        canonical_attempt_digest(events, units, observations)
    }

    #[test]
    fn complete_matching_attempt_is_verified_and_renderable() {
        let planned = vec!["a".into()];
        let statuses = vec![status("a", pb::UnitOutcome::Executed)];
        let events = vec![event("a")];
        let digest = events_digest(&events, 1, 0);
        let completion = pb::ExecuteComplete {
            units_reported: 1,
            observation_count: 0,
            attempt_digest: digest.clone(),
        };
        let report = AttemptIntegrityDashboard::reconcile(AttemptIntegrityInput {
            planned_unit_ids: &planned,
            statuses: &statuses,
            observation_events: 0,
            completion: Some(&completion),
            computed_attempt_digest: Some(&digest),
        });
        assert!(report.is_verified());
        assert_eq!(report.reasons, Vec::<String>::new());
        assert!(report.render_line().contains("status=VERIFIED"));
    }

    #[test]
    fn missing_completion_is_inconclusive() {
        let planned = vec!["a".into()];
        let statuses = vec![status("a", pb::UnitOutcome::Executed)];
        let report = AttemptIntegrityDashboard::reconcile(AttemptIntegrityInput {
            planned_unit_ids: &planned,
            statuses: &statuses,
            observation_events: 0,
            completion: None,
            computed_attempt_digest: None,
        });
        assert_eq!(report.status, AttemptIntegrityStatus::Inconclusive);
        assert!(report.reasons.contains(&"MISSING_EXECUTE_COMPLETE".into()));
    }

    #[test]
    fn missing_computed_digest_is_inconclusive() {
        let planned = vec!["a".into()];
        let statuses = vec![status("a", pb::UnitOutcome::Executed)];
        let completion = pb::ExecuteComplete {
            units_reported: 1,
            observation_count: 0,
            attempt_digest: vec![7; 32],
        };
        let report = AttemptIntegrityDashboard::reconcile(AttemptIntegrityInput {
            planned_unit_ids: &planned,
            statuses: &statuses,
            observation_events: 0,
            completion: Some(&completion),
            computed_attempt_digest: None,
        });
        assert_eq!(report.status, AttemptIntegrityStatus::Inconclusive);
        assert!(report
            .reasons
            .contains(&"MISSING_COMPUTED_ATTEMPT_DIGEST".into()));
    }

    #[test]
    fn duplicate_status_and_digest_mismatch_fail_closed() {
        let planned = vec!["a".into()];
        let statuses = vec![
            status("a", pb::UnitOutcome::Executed),
            status("a", pb::UnitOutcome::Executed),
        ];
        let completion = pb::ExecuteComplete {
            units_reported: 2,
            observation_count: 0,
            attempt_digest: vec![7; 32],
        };
        let report = AttemptIntegrityDashboard::reconcile(AttemptIntegrityInput {
            planned_unit_ids: &planned,
            statuses: &statuses,
            observation_events: 0,
            completion: Some(&completion),
            computed_attempt_digest: Some(&[8; 32]),
        });
        assert_eq!(report.status, AttemptIntegrityStatus::FailClosed);
        assert!(report.reasons.contains(&"DUPLICATE_UNIT_STATUS".into()));
        assert!(report.reasons.contains(&"ATTEMPT_DIGEST_MISMATCH".into()));
    }

    #[test]
    fn observation_count_mismatch_fail_closes() {
        let planned = vec!["a".into()];
        let statuses = vec![status("a", pb::UnitOutcome::Executed)];
        let completion = pb::ExecuteComplete {
            units_reported: 1,
            observation_count: 2,
            attempt_digest: vec![9; 32],
        };
        let report = AttemptIntegrityDashboard::reconcile(AttemptIntegrityInput {
            planned_unit_ids: &planned,
            statuses: &statuses,
            observation_events: 1,
            completion: Some(&completion),
            computed_attempt_digest: Some(&[9; 32]),
        });
        assert_eq!(report.status, AttemptIntegrityStatus::FailClosed);
        assert!(report.reasons.contains(&"OBSERVATION_COUNT_MISMATCH".into()));
    }
}
