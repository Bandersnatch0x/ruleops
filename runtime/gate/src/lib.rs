//! Governance-side normalization, Completeness and local Gate preview.
//! This crate never executes a backend and has no publish/signing authority.

use erf_contracts::backend::v0 as pb;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const FINGERPRINT_VERSION: &str = "finding-v0";
pub const CANONICAL_SET_VERSION: &str = "canonical-finding-set-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateDecision { Pass, Fail, Inconclusive }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub fingerprint_version: &'static str,
    pub fingerprint: String,
    pub rule_id: String,
    pub path: String,
    pub semantic_anchor: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completeness {
    pub planned: usize,
    pub accounted: usize,
    pub failed: usize,
    pub missing: usize,
    pub observation_errors: usize,
    pub complete: bool,
}

#[derive(Debug, Clone)]
pub struct GatePreview {
    pub decision: GateDecision,
    pub findings: Vec<Finding>,
    pub completeness: Completeness,
    pub canonical_set_version: &'static str,
    pub trust: &'static str,
    pub input_summary: String,
}

pub fn preview(planned_ids: &[String], statuses: &[pb::UnitStatus], observations: &[pb::BackendObservation]) -> GatePreview {
    let planned: BTreeSet<&str> = planned_ids.iter().map(String::as_str).collect();
    let mut accounted = BTreeSet::new();
    let mut failed = 0;
    for status in statuses {
        if planned.contains(status.unit_id.as_str()) {
            accounted.insert(status.unit_id.as_str());
            if status.outcome != pb::UnitOutcome::Executed as i32 { failed += 1; }
        }
    }
    let missing = planned.len().saturating_sub(accounted.len());
    let mut findings = Vec::new();
    let mut observation_errors = 0;
    for observation in observations {
        match normalize(observation) { Ok(f) => findings.push(f), Err(_) => observation_errors += 1 }
    }
    findings.sort_by(|a, b| a.fingerprint.cmp(&b.fingerprint));
    findings.dedup_by(|a, b| a.fingerprint == b.fingerprint);
    let complete = missing == 0 && failed == 0 && observation_errors == 0 && accounted.len() == planned.len();
    let completeness = Completeness { planned: planned.len(), accounted: accounted.len(), failed, missing, observation_errors, complete };
    let decision = if !complete || findings.is_empty() { GateDecision::Inconclusive } else { GateDecision::Fail };
    let input_summary = format!("findings={}, planned={}, accounted={}, failed={}, missing={}, observation_errors={}", findings.len(), completeness.planned, completeness.accounted, completeness.failed, completeness.missing, completeness.observation_errors);
    GatePreview { decision, findings, completeness, canonical_set_version: CANONICAL_SET_VERSION, trust: "LOCAL_UNATTESTED", input_summary }
}

fn normalize(observation: &pb::BackendObservation) -> Result<Finding, &'static str> {
    if observation.rule_id.is_empty() { return Err("missing rule identity"); }
    let location = observation.location.as_ref().ok_or("missing location")?;
    let span = match location.kind.as_ref() { Some(pb::observation_location::Kind::SourceSpan(s)) => s, _ => return Err("unsupported or missing source span") };
    if span.path.is_empty() || span.path.starts_with('/') || span.path.contains("..") || span.blob_sha256.len() != 32 || span.end_byte < span.start_byte { return Err("invalid source span identity"); }
    let anchor = format!("source-span|{}|{}|{}", span.path, span.start_byte, span.end_byte);
    let mut identity = Vec::new(); identity.extend_from_slice(FINGERPRINT_VERSION.as_bytes()); identity.push(0); identity.extend_from_slice(observation.rule_id.as_bytes()); identity.push(0); identity.extend_from_slice(anchor.as_bytes());
    let fingerprint = format!("{}:{:x}", FINGERPRINT_VERSION, Sha256::digest(identity));
    Ok(Finding { fingerprint_version: FINGERPRINT_VERSION, fingerprint, rule_id: observation.rule_id.clone(), path: span.path.clone(), semantic_anchor: anchor, message: observation.message.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn obs(path: &str) -> pb::BackendObservation { pb::BackendObservation { rule_id: "rule.a".into(), location: Some(pb::ObservationLocation { kind: Some(pb::observation_location::Kind::SourceSpan(pb::SourceSpan { path: path.into(), blob_sha256: vec![1; 32], start_byte: 2, end_byte: 4, line: 1, column: 1 })) }), message: "diagnostic".into(), engine_severity: "warning".into() } }
    fn status(id: &str, outcome: pb::UnitOutcome) -> pb::UnitStatus { pb::UnitStatus { unit_id: id.into(), outcome: outcome as i32, reason_code: 0, detail: String::new() } }
    #[test] fn stable_finding_and_complete_nonempty_is_fail() { let p = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[obs("src/A.java")]); assert_eq!(p.decision, GateDecision::Fail); assert!(p.completeness.complete); assert_eq!(p.findings[0].fingerprint_version, FINGERPRINT_VERSION); }
    #[test] fn zero_findings_is_not_pass_and_missing_is_inconclusive() { let p = preview(&["u1".into()], &[], &[]); assert_eq!(p.decision, GateDecision::Inconclusive); assert!(!p.completeness.complete); }
    #[test] fn invalid_observation_is_not_a_finding() { let p = preview(&["u1".into()], &[status("u1", pb::UnitOutcome::Executed)], &[obs("../A.java")]); assert!(p.findings.is_empty()); assert_eq!(p.completeness.observation_errors, 1); assert_eq!(p.decision, GateDecision::Inconclusive); }
}
