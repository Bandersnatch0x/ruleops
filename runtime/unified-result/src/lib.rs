//! Immutable result summary and audit chain for the ERF execution boundary.
//!
//! The unified result is a summary of already-derived governance facts. It
//! does not execute a backend, upload findings, or publish an SCM status. The
//! same object can represent local feedback or a trusted CI handoff while
//! preserving the strict PASS/FAIL/INCONCLUSIVE decision space.

use rf_gate::{BaselineState, Completeness, ExceptionDecision, GateAttestation, GateDecision, GatePreview};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const UNIFIED_RESULT_VERSION: &str = "unified-result-v0";
pub const AUDIT_CHAIN_VERSION: &str = "audit-chain-v0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultTrust {
    LocalUnattested,
    Trusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultStatus {
    Success,
    Failure,
    ActionRequired,
}

impl ResultStatus {
    pub fn from_decision(decision: GateDecision) -> Self {
        match decision {
            GateDecision::Pass => Self::Success,
            GateDecision::Fail => Self::Failure,
            GateDecision::Inconclusive => Self::ActionRequired,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultInputs {
    pub repository: String,
    pub commit: String,
    pub policy_digest: String,
    pub rule_bundle_digest: String,
    pub execution_lock_digest: String,
    pub runner_release_digest: String,
    pub workspace_descriptor_digest: String,
}

impl ResultInputs {
    pub fn validate(&self) -> Result<(), ResultError> {
        for (name, value) in [
            ("repository", &self.repository),
            ("commit", &self.commit),
            ("policy_digest", &self.policy_digest),
            ("rule_bundle_digest", &self.rule_bundle_digest),
            ("execution_lock_digest", &self.execution_lock_digest),
            ("runner_release_digest", &self.runner_release_digest),
            ("workspace_descriptor_digest", &self.workspace_descriptor_digest),
        ] {
            if value.trim().is_empty() { return Err(ResultError::InvalidInput(name)); }
        }
        for (name, value) in [
            ("policy_digest", &self.policy_digest),
            ("rule_bundle_digest", &self.rule_bundle_digest),
            ("execution_lock_digest", &self.execution_lock_digest),
            ("runner_release_digest", &self.runner_release_digest),
            ("workspace_descriptor_digest", &self.workspace_descriptor_digest),
        ] {
            if !is_hex_digest(value) { return Err(ResultError::InvalidInput(name)); }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEventKind {
    ScanPlanned,
    ExecutionCompleted,
    FindingSetComputed,
    GateEvaluated,
    Attested,
    Published,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub sequence: u64,
    pub kind: AuditEventKind,
    pub subject_digest: [u8; 32],
    pub previous_digest: [u8; 32],
    pub event_digest: [u8; 32],
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditTrail {
    events: Vec<AuditEvent>,
}

impl AuditTrail {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append(&mut self, kind: AuditEventKind, subject_digest: [u8; 32]) -> u64 {
        let sequence = self.events.len() as u64 + 1;
        let previous_digest = self.tip_digest();
        let event_digest = digest_event(sequence, kind, subject_digest, previous_digest);
        self.events.push(AuditEvent { sequence, kind, subject_digest, previous_digest, event_digest });
        sequence
    }

    pub fn events(&self) -> &[AuditEvent] {
        &self.events
    }

    pub fn tip_digest(&self) -> [u8; 32] {
        self.events.last().map(|event| event.event_digest).unwrap_or([0; 32])
    }

    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(AUDIT_CHAIN_VERSION.as_bytes());
        hasher.update([0]);
        for event in &self.events { hasher.update(event.event_digest); }
        hasher.finalize().into()
    }

    pub fn verify(&self) -> Result<(), AuditError> {
        let mut previous_digest = [0; 32];
        for (index, event) in self.events.iter().enumerate() {
            let expected_sequence = index as u64 + 1;
            if event.sequence != expected_sequence {
                return Err(AuditError::Sequence { expected: expected_sequence, received: event.sequence });
            }
            if event.previous_digest != previous_digest {
                return Err(AuditError::PreviousDigest { sequence: event.sequence });
            }
            let expected_digest = digest_event(event.sequence, event.kind, event.subject_digest, event.previous_digest);
            if event.event_digest != expected_digest {
                return Err(AuditError::EventDigest { sequence: event.sequence });
            }
            previous_digest = event.event_digest;
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuditError {
    #[error("audit sequence expected {expected}, received {received}")]
    Sequence { expected: u64, received: u64 },
    #[error("audit event {sequence} has a wrong previous digest")]
    PreviousDigest { sequence: u64 },
    #[error("audit event {sequence} has a wrong event digest")]
    EventDigest { sequence: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifiedResult {
    pub inputs: ResultInputs,
    pub semantic_result_digest: [u8; 32],
    pub completeness: Completeness,
    pub decision: GateDecision,
    pub status: ResultStatus,
    pub trust: ResultTrust,
    pub finding_count: usize,
    pub new_finding_count: usize,
    pub historical_finding_count: usize,
    pub exempted_finding_count: usize,
    pub gate_attestation_subject: Option<[u8; 32]>,
    pub audit: AuditTrail,
}

impl UnifiedResult {
    pub fn from_preview(inputs: ResultInputs, preview: &GatePreview, audit: AuditTrail) -> Result<Self, ResultError> {
        let result = Self::build(inputs, preview, ResultTrust::LocalUnattested, None, audit)?;
        result.validate()?;
        Ok(result)
    }

    pub fn from_trusted_preview(
        inputs: ResultInputs,
        preview: &GatePreview,
        attestation: &GateAttestation,
        attestation_subject: [u8; 32],
        audit: AuditTrail,
    ) -> Result<Self, ResultError> {
        if attestation.repository != inputs.repository || attestation.commit != inputs.commit {
            return Err(ResultError::AttestationMismatch("repository or commit"));
        }
        if attestation.policy_digest != inputs.policy_digest
            || attestation.rule_bundle_digest != inputs.rule_bundle_digest
            || attestation.execution_lock_digest != inputs.execution_lock_digest
            || attestation.runner_release_digest != inputs.runner_release_digest
            || attestation.workspace_descriptor_digest != inputs.workspace_descriptor_digest
        {
            return Err(ResultError::AttestationMismatch("bound identity"));
        }
        if attestation.semantic_result_digest != preview.semantic_result_digest
            || attestation.completeness != preview.completeness
            || attestation.decision != preview.decision
        {
            return Err(ResultError::AttestationMismatch("gate result"));
        }
        let result = Self::build(inputs, preview, ResultTrust::Trusted, Some(attestation_subject), audit)?;
        result.validate()?;
        Ok(result)
    }

    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = UNIFIED_RESULT_VERSION.as_bytes().to_vec();
        payload.push(0);
        for value in [
            &self.inputs.repository,
            &self.inputs.commit,
            &self.inputs.policy_digest,
            &self.inputs.rule_bundle_digest,
            &self.inputs.execution_lock_digest,
            &self.inputs.runner_release_digest,
            &self.inputs.workspace_descriptor_digest,
        ] {
            append_string(&mut payload, value);
        }
        payload.extend_from_slice(&self.semantic_result_digest);
        for value in [self.completeness.planned, self.completeness.accounted, self.completeness.failed, self.completeness.missing, self.completeness.observation_errors, self.finding_count, self.new_finding_count, self.historical_finding_count, self.exempted_finding_count] {
            payload.extend_from_slice(&(value as u64).to_be_bytes());
        }
        payload.push(self.completeness.complete as u8);
        payload.push(match self.decision { GateDecision::Pass => 0, GateDecision::Fail => 1, GateDecision::Inconclusive => 2 });
        payload.push(match self.trust { ResultTrust::LocalUnattested => 0, ResultTrust::Trusted => 1 });
        match self.gate_attestation_subject {
            Some(subject) => { payload.push(1); payload.extend_from_slice(&subject); }
            None => payload.push(0),
        }
        payload.extend_from_slice(&(self.audit.events.len() as u64).to_be_bytes());
        payload.extend_from_slice(&self.audit.digest());
        payload
    }

    pub fn validate(&self) -> Result<(), ResultError> {
        self.inputs.validate()?;
        self.audit.verify().map_err(ResultError::Audit)?;
        if self.status != ResultStatus::from_decision(self.decision) {
            return Err(ResultError::StatusMismatch);
        }
        if self.decision == GateDecision::Pass && !self.completeness.complete {
            return Err(ResultError::IncompletePass);
        }
        if self.trust == ResultTrust::Trusted && self.gate_attestation_subject.is_none() {
            return Err(ResultError::MissingAttestation);
        }
        if self.trust == ResultTrust::LocalUnattested && self.gate_attestation_subject.is_some() {
            return Err(ResultError::UnexpectedAttestation);
        }
        if self.finding_count != self.new_finding_count + self.historical_finding_count {
            return Err(ResultError::FindingCountMismatch);
        }
        if self.exempted_finding_count > self.finding_count {
            return Err(ResultError::FindingCountMismatch);
        }
        Ok(())
    }

    fn build(
        inputs: ResultInputs,
        preview: &GatePreview,
        trust: ResultTrust,
        gate_attestation_subject: Option<[u8; 32]>,
        audit: AuditTrail,
    ) -> Result<Self, ResultError> {
        let new_finding_count = preview.assessments.iter().filter(|assessment| assessment.baseline == BaselineState::New).count();
        let historical_finding_count = preview.assessments.iter().filter(|assessment| assessment.baseline == BaselineState::Historical).count();
        let exempted_finding_count = preview.assessments.iter().filter(|assessment| assessment.exception == ExceptionDecision::Exempted).count();
        Ok(Self {
            inputs,
            semantic_result_digest: preview.semantic_result_digest,
            completeness: preview.completeness.clone(),
            decision: preview.decision,
            status: ResultStatus::from_decision(preview.decision),
            trust,
            finding_count: preview.findings.len(),
            new_finding_count,
            historical_finding_count,
            exempted_finding_count,
            gate_attestation_subject,
            audit,
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResultError {
    #[error("unified result input is invalid: {0}")]
    InvalidInput(&'static str),
    #[error("trusted GateAttestation does not match the unified result: {0}")]
    AttestationMismatch(&'static str),
    #[error("unified result contains an incomplete PASS")]
    IncompletePass,
    #[error("trusted unified result has no GateAttestation subject")]
    MissingAttestation,
    #[error("local unified result unexpectedly contains an attestation subject")]
    UnexpectedAttestation,
    #[error("unified result status does not map from its GateDecision")]
    StatusMismatch,
    #[error("unified result finding counts do not add up")]
    FindingCountMismatch,
    #[error("unified result audit chain is invalid: {0}")]
    Audit(#[from] AuditError),
}

fn digest_event(sequence: u64, kind: AuditEventKind, subject_digest: [u8; 32], previous_digest: [u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(AUDIT_CHAIN_VERSION.as_bytes());
    hasher.update([0]);
    hasher.update(sequence.to_be_bytes());
    hasher.update([audit_kind_byte(kind)]);
    hasher.update(subject_digest);
    hasher.update(previous_digest);
    hasher.finalize().into()
}

fn audit_kind_byte(kind: AuditEventKind) -> u8 {
    match kind {
        AuditEventKind::ScanPlanned => 0,
        AuditEventKind::ExecutionCompleted => 1,
        AuditEventKind::FindingSetComputed => 2,
        AuditEventKind::GateEvaluated => 3,
        AuditEventKind::Attested => 4,
        AuditEventKind::Published => 5,
    }
}

fn append_string(payload: &mut Vec<u8>, value: &str) {
    payload.extend_from_slice(&(value.len() as u64).to_be_bytes());
    payload.extend_from_slice(value.as_bytes());
}

fn is_hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf_gate::{Finding, FindingAssessment, FindingLocationKind};

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

    #[test]
    fn local_result_preserves_tri_state_and_counts() {
        let result = UnifiedResult::from_preview(inputs(), &preview(GateDecision::Fail, true), audit()).unwrap();
        assert_eq!(result.status, ResultStatus::Failure);
        assert_eq!(result.trust, ResultTrust::LocalUnattested);
        assert_eq!(result.finding_count, 1);
        assert_eq!(result.new_finding_count, 1);
        assert_eq!(result.historical_finding_count, 0);
        assert_eq!(result.exempted_finding_count, 0);
        assert!(result.gate_attestation_subject.is_none());
        assert!(result.validate().is_ok());
    }

    #[test]
    fn all_decisions_map_without_a_neutral_or_fourth_state() {
        assert_eq!(ResultStatus::from_decision(GateDecision::Pass), ResultStatus::Success);
        assert_eq!(ResultStatus::from_decision(GateDecision::Fail), ResultStatus::Failure);
        assert_eq!(ResultStatus::from_decision(GateDecision::Inconclusive), ResultStatus::ActionRequired);
        let result = UnifiedResult::from_preview(inputs(), &preview(GateDecision::Inconclusive, false), audit()).unwrap();
        assert_eq!(result.status, ResultStatus::ActionRequired);
    }

    #[test]
    fn incomplete_pass_and_missing_trusted_attestation_fail_closed() {
        let mut invalid = preview(GateDecision::Pass, false);
        assert_eq!(UnifiedResult::from_preview(inputs(), &invalid, audit()), Err(ResultError::IncompletePass));
        invalid.completeness.complete = true;
        invalid.completeness.missing = 0;
        let result = UnifiedResult::from_preview(inputs(), &invalid, audit());
        assert!(result.is_ok());
        let mut trusted = result.unwrap();
        assert_eq!(trusted.trust, ResultTrust::LocalUnattested);
        trusted.trust = ResultTrust::Trusted;
        assert_eq!(trusted.validate(), Err(ResultError::MissingAttestation));
    }

    #[test]
    fn trusted_result_requires_attestation_identity_to_match() {
        let preview = preview(GateDecision::Pass, true);
        let attestation = GateAttestation {
            repository: "org/repo".into(),
            commit: "abc123".into(),
            policy_digest: "f".repeat(64),
            rule_bundle_digest: "a".repeat(64),
            execution_lock_digest: "b".repeat(64),
            runner_release_digest: "c".repeat(64),
            workspace_descriptor_digest: "d".repeat(64),
            sandbox_provider_id: "rootless-oci-docker-v0".into(),
            sandbox_policy_digest: "e".repeat(64),
            semantic_result_digest: [7; 32],
            completeness: preview.completeness.clone(),
            decision: GateDecision::Pass,
            exception_grant_digests: Vec::new(),
            issued_at_epoch_secs: 10,
            expires_at_epoch_secs: 20,
        };
        let result = UnifiedResult::from_trusted_preview(inputs(), &preview, &attestation, [8; 32], audit()).unwrap();
        assert_eq!(result.trust, ResultTrust::Trusted);
        assert_eq!(result.status, ResultStatus::Success);
        let mut changed = attestation.clone();
        changed.policy_digest = "0".repeat(64);
        assert!(matches!(UnifiedResult::from_trusted_preview(inputs(), &preview, &changed, [8; 32], audit()), Err(ResultError::AttestationMismatch("bound identity"))));
    }

    #[test]
    fn audit_chain_is_deterministic_and_tamper_evident() {
        let first = audit();
        let second = audit();
        assert_eq!(first.digest(), second.digest());
        assert_eq!(first.events(), second.events());
        let mut tampered = first.clone();
        tampered.events[1].subject_digest = [9; 32];
        assert!(matches!(tampered.verify(), Err(AuditError::EventDigest { sequence: 2 })));
        tampered = first;
        tampered.events[1].sequence = 9;
        assert!(matches!(tampered.verify(), Err(AuditError::Sequence { expected: 2, received: 9 })));
    }

    #[test]
    fn canonical_payload_changes_with_audit_or_identity() {
        let result = UnifiedResult::from_preview(inputs(), &preview(GateDecision::Fail, true), audit()).unwrap();
        let original = result.canonical_payload();
        let mut changed_inputs = inputs();
        changed_inputs.commit = "def456".into();
        let changed = UnifiedResult::from_preview(changed_inputs, &preview(GateDecision::Fail, true), audit()).unwrap();
        assert_ne!(original, changed.canonical_payload());
        let mut changed_audit = audit();
        changed_audit.append(AuditEventKind::Published, [8; 32]);
        let changed = UnifiedResult::from_preview(inputs(), &preview(GateDecision::Fail, true), changed_audit).unwrap();
        assert_ne!(original, changed.canonical_payload());
    }
}
