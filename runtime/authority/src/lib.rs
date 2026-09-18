//! Pure authority evaluation for a protected-branch adapter.
//!
//! This crate only evaluates an already validated `UnifiedResult`. It has no
//! SCM, network, credential, persistence, or write capability. The caller
//! must still decide how to transport the returned decision to a platform.

use erf_unified_result::{ResultError, ResultStatus, ResultTrust, UnifiedResult};
use rf_gate::GateDecision;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const TRUSTED_PLATFORM: &str = "linux/amd64";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityDecision {
    AllowMerge,
    BlockMerge,
    ActionRequired,
}

impl AuthorityDecision {
    pub fn from_gate_decision(decision: GateDecision) -> Self {
        match decision {
            GateDecision::Pass => Self::AllowMerge,
            GateDecision::Fail => Self::BlockMerge,
            GateDecision::Inconclusive => Self::ActionRequired,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityEvaluation {
    pub platform: String,
    pub result_digest: [u8; 32],
    pub gate_decision: GateDecision,
    pub status: ResultStatus,
    pub decision: AuthorityDecision,
}

impl AuthorityEvaluation {
    pub fn is_authoritative(&self) -> bool {
        true
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthorityError {
    #[error("unified result is invalid: {0}")]
    InvalidResult(#[from] ResultError),
    #[error("result trust {0:?} is not authoritative")]
    NonAuthoritativeTrust(ResultTrust),
    #[error("platform {0:?} is outside the trusted authority boundary")]
    UnsupportedPlatform(String),
}

/// Evaluate whether a UnifiedResult may authorize a protected-branch action.
///
/// Only a complete, attested Trusted result produced on `linux/amd64` is
/// authoritative. PASS, FAIL, and INCONCLUSIVE retain their distinct
/// outcomes as AllowMerge, BlockMerge, and ActionRequired respectively.
pub fn evaluate(result: &UnifiedResult, platform: &str) -> Result<AuthorityEvaluation, AuthorityError> {
    result.validate()?;
    if result.trust != ResultTrust::Trusted {
        return Err(AuthorityError::NonAuthoritativeTrust(result.trust));
    }
    if platform != TRUSTED_PLATFORM {
        return Err(AuthorityError::UnsupportedPlatform(platform.to_owned()));
    }

    let mut hasher = Sha256::new();
    hasher.update(result.canonical_payload());
    Ok(AuthorityEvaluation {
        platform: platform.to_owned(),
        result_digest: hasher.finalize().into(),
        gate_decision: result.decision,
        status: result.status,
        decision: AuthorityDecision::from_gate_decision(result.decision),
    })
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
            completeness: Completeness {
                planned: 1,
                accounted: if complete { 1 } else { 0 },
                failed: 0,
                missing: if complete { 0 } else { 1 },
                observation_errors: 0,
                complete,
            },
            canonical_set_version: rf_gate::CANONICAL_SET_VERSION,
            trust: "LOCAL_UNATTESTED",
            input_summary: "test".into(),
            assessments: vec![FindingAssessment {
                fingerprint: finding.fingerprint,
                baseline: BaselineState::New,
                exception: ExceptionDecision::NotApplicable,
            }],
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

    fn trusted(decision: GateDecision) -> UnifiedResult {
        let local = erf_unified_result::UnifiedResult::from_preview(inputs(), &preview(decision, true), audit()).unwrap();
        UnifiedResult {
            trust: ResultTrust::Trusted,
            gate_attestation_subject: Some([8; 32]),
            ..local
        }
    }

    fn incomplete_pass() -> UnifiedResult {
        let local = erf_unified_result::UnifiedResult {
            inputs: inputs(),
            semantic_result_digest: [7; 32],
            completeness: Completeness {
                planned: 1,
                accounted: 0,
                failed: 0,
                missing: 1,
                observation_errors: 0,
                complete: false,
            },
            decision: GateDecision::Pass,
            status: ResultStatus::Success,
            trust: ResultTrust::Trusted,
            finding_count: 1,
            new_finding_count: 1,
            historical_finding_count: 0,
            exempted_finding_count: 0,
            gate_attestation_subject: Some([8; 32]),
            audit: audit(),
        };
        local
    }

    #[test]
    fn local_result_is_rejected_as_non_authoritative() {
        let result = erf_unified_result::UnifiedResult::from_preview(inputs(), &preview(GateDecision::Pass, true), audit()).unwrap();
        assert_eq!(
            evaluate(&result, TRUSTED_PLATFORM),
            Err(AuthorityError::NonAuthoritativeTrust(ResultTrust::LocalUnattested))
        );
    }

    #[test]
    fn non_linux_platform_is_rejected() {
        assert_eq!(
            evaluate(&trusted(GateDecision::Pass), "linux/arm64"),
            Err(AuthorityError::UnsupportedPlatform("linux/arm64".into()))
        );
    }

    #[test]
    fn trusted_pass_allows_merge() {
        let evaluation = evaluate(&trusted(GateDecision::Pass), TRUSTED_PLATFORM).unwrap();
        assert_eq!(evaluation.decision, AuthorityDecision::AllowMerge);
        assert_eq!(evaluation.status, ResultStatus::Success);
        assert!(evaluation.is_authoritative());
    }

    #[test]
    fn trusted_fail_blocks_merge() {
        let evaluation = evaluate(&trusted(GateDecision::Fail), TRUSTED_PLATFORM).unwrap();
        assert_eq!(evaluation.decision, AuthorityDecision::BlockMerge);
        assert_eq!(evaluation.status, ResultStatus::Failure);
    }

    #[test]
    fn trusted_inconclusive_requires_action() {
        let evaluation = evaluate(&trusted(GateDecision::Inconclusive), TRUSTED_PLATFORM).unwrap();
        assert_eq!(evaluation.decision, AuthorityDecision::ActionRequired);
        assert_eq!(evaluation.status, ResultStatus::ActionRequired);
    }

    #[test]
    fn incomplete_pass_fails_closed_and_digest_is_stable() {
        assert_eq!(
            evaluate(&incomplete_pass(), TRUSTED_PLATFORM),
            Err(AuthorityError::InvalidResult(erf_unified_result::ResultError::IncompletePass))
        );
        let first = evaluate(&trusted(GateDecision::Pass), TRUSTED_PLATFORM).unwrap();
        let second = evaluate(&trusted(GateDecision::Pass), TRUSTED_PLATFORM).unwrap();
        assert_eq!(first.result_digest, second.result_digest);
        assert_eq!(first.platform, TRUSTED_PLATFORM);
    }
}
