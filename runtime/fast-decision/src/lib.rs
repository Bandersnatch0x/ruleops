//! Provider-neutral, shadow-only typed routing evidence.
//!
//! Fast decisions are advisory observations. They never produce a Gate
//! decision, mutate TrustState, approve an exception, retry a request, or
//! write SCM state. Unknown or low-confidence answers carry an explicit
//! fallback route.

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use thiserror::Error;

pub const FAST_DECISION_CONTRACT_VERSION: &str = "fast-decision-v1";
pub const FAST_DECISION_EVIDENCE_FORMAT: &str = "fast-decision-evidence-v1";
pub const BASIS_POINTS_MAX: u16 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionKind {
    Choice,
    Score,
    Boolean,
}

impl DecisionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Choice => "CHOICE",
            Self::Score => "SCORE",
            Self::Boolean => "BOOLEAN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionStatus {
    Decided,
    Unknown,
}

impl DecisionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Decided => "DECIDED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownReason {
    LowConfidence,
    MarginTooSmall,
    InputExpired,
    AnswerConflict,
    ProviderUnavailable,
    InvalidInput,
}

impl UnknownReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LowConfidence => "LOW_CONFIDENCE",
            Self::MarginTooSmall => "MARGIN_TOO_SMALL",
            Self::InputExpired => "INPUT_EXPIRED",
            Self::AnswerConflict => "ANSWER_CONFLICT",
            Self::ProviderUnavailable => "PROVIDER_UNAVAILABLE",
            Self::InvalidInput => "INVALID_INPUT",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackAction {
    ExistingDeterministicPath,
    HumanReview,
}

impl FallbackAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExistingDeterministicPath => "EXISTING_DETERMINISTIC_PATH",
            Self::HumanReview => "HUMAN_REVIEW",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionAnswer<'a> {
    Choice(&'a str),
    Score(i64),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnedDecisionAnswer {
    Choice(String),
    Score(i64),
    Boolean(bool),
}

impl OwnedDecisionAnswer {
    pub fn kind(&self) -> DecisionKind {
        match self {
            Self::Choice(_) => DecisionKind::Choice,
            Self::Score(_) => DecisionKind::Score,
            Self::Boolean(_) => DecisionKind::Boolean,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionSpec {
    pub id: String,
    pub version: String,
    pub kind: DecisionKind,
    pub choices: BTreeSet<String>,
    pub score_min: Option<i64>,
    pub score_max: Option<i64>,
    pub min_confidence_bps: u16,
    pub min_margin_bps: u16,
    pub expires_after_secs: u64,
    pub fallback: FallbackAction,
}

impl DecisionSpec {
    pub fn route_v1() -> Self {
        Self {
            id: "structured-scan-diagnostic-route".into(),
            version: "1.0.0".into(),
            kind: DecisionKind::Choice,
            choices: ["LOCAL_EXPLAIN", "DEEP_ANALYSIS", "HUMAN_REVIEW", "NO_ACTION"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            score_min: None,
            score_max: None,
            min_confidence_bps: 7_000,
            min_margin_bps: 1_000,
            expires_after_secs: 3_600,
            fallback: FallbackAction::HumanReview,
        }
    }

    pub fn validate(&self) -> Result<(), FastDecisionError> {
        validate_text("spec_id", &self.id)?;
        validate_text("spec_version", &self.version)?;
        if self.min_confidence_bps > BASIS_POINTS_MAX || self.min_margin_bps > BASIS_POINTS_MAX {
            return Err(FastDecisionError::InvalidSpec("threshold basis points"));
        }
        if self.expires_after_secs == 0 || self.expires_after_secs == u64::MAX {
            return Err(FastDecisionError::InvalidSpec("expiry"));
        }
        match self.kind {
            DecisionKind::Choice => {
                if self.choices.is_empty() || self.score_min.is_some() || self.score_max.is_some() {
                    return Err(FastDecisionError::InvalidSpec("choice schema"));
                }
                for choice in &self.choices {
                    validate_text("choice", choice)?;
                }
            }
            DecisionKind::Score => {
                let (Some(min), Some(max)) = (self.score_min, self.score_max) else {
                    return Err(FastDecisionError::InvalidSpec("score bounds"));
                };
                if min >= max || !self.choices.is_empty() {
                    return Err(FastDecisionError::InvalidSpec("score schema"));
                }
                let span = i128::from(max) - i128::from(min) + 1;
                if span <= 0 || span > i128::from(u64::MAX) + 1 {
                    return Err(FastDecisionError::InvalidSpec("score range"));
                }
            }
            DecisionKind::Boolean => {
                if self.score_min.is_some() || self.score_max.is_some() || !self.choices.is_empty() {
                    return Err(FastDecisionError::InvalidSpec("boolean schema"));
                }
            }
        }
        Ok(())
    }

    pub fn digest(&self) -> [u8; 32] {
        let mut payload = FAST_DECISION_CONTRACT_VERSION.as_bytes().to_vec();
        payload.push(0);
        append_string(&mut payload, &self.id);
        append_string(&mut payload, &self.version);
        payload.push(kind_byte(self.kind));
        append_u64(&mut payload, self.choices.len() as u64);
        for choice in &self.choices {
            append_string(&mut payload, choice);
        }
        append_optional_i64(&mut payload, self.score_min);
        append_optional_i64(&mut payload, self.score_max);
        payload.extend_from_slice(&self.min_confidence_bps.to_be_bytes());
        payload.extend_from_slice(&self.min_margin_bps.to_be_bytes());
        payload.extend_from_slice(&self.expires_after_secs.to_be_bytes());
        payload.push(fallback_byte(self.fallback));
        Sha256::digest(payload).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineInput {
    pub input_digest: [u8; 32],
    pub signal: u64,
    pub created_at_epoch_secs: u64,
    pub provider_available: bool,
    pub near_miss: bool,
    pub conflict: bool,
}

impl BaselineInput {
    pub fn validate(&self) -> Result<(), FastDecisionError> {
        if self.input_digest == [0; 32] || self.created_at_epoch_secs == 0 {
            return Err(FastDecisionError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterIdentity {
    pub provider_id: String,
    pub provider_version: String,
    pub model_id: String,
}

impl AdapterIdentity {
    fn validate(&self) -> Result<(), FastDecisionError> {
        validate_text("provider_id", &self.provider_id)?;
        validate_text("provider_version", &self.provider_version)?;
        validate_text("model_id", &self.model_id)
    }
}

pub trait FastDecisionAdapter {
    fn identity(&self) -> AdapterIdentity;

    fn evaluate(
        &self,
        spec: &DecisionSpec,
        input: &BaselineInput,
        _now_epoch_secs: u64,
    ) -> Result<FastDecisionEvidence, FastDecisionError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DeterministicBaselineAdapter;

impl FastDecisionAdapter for DeterministicBaselineAdapter {
    fn identity(&self) -> AdapterIdentity {
        AdapterIdentity {
            provider_id: "deterministic-baseline".into(),
            provider_version: "1.0.0".into(),
            model_id: "typed-rules-v1".into(),
        }
    }

    fn evaluate(
        &self,
        spec: &DecisionSpec,
        input: &BaselineInput,
        now_epoch_secs: u64,
    ) -> Result<FastDecisionEvidence, FastDecisionError> {
        spec.validate()?;
        let identity = self.identity();
        identity.validate()?;

        if input.input_digest == [0; 32]
            || input.created_at_epoch_secs == 0
            || now_epoch_secs == 0
            || now_epoch_secs < input.created_at_epoch_secs
            || input.created_at_epoch_secs.checked_add(spec.expires_after_secs).is_none()
        {
            return FastDecisionEvidence::invalid_input(spec, input, identity, now_epoch_secs);
        }
        let expires_at = input
            .created_at_epoch_secs
            .checked_add(spec.expires_after_secs)
            .expect("checked above");
        if now_epoch_secs > expires_at {
            return FastDecisionEvidence::unknown(
                spec,
                input,
                identity,
                now_epoch_secs,
                expires_at,
                UnknownReason::InputExpired,
                0,
                0,
            );
        }
        if !input.provider_available {
            return FastDecisionEvidence::unknown(
                spec,
                input,
                identity,
                now_epoch_secs,
                expires_at,
                UnknownReason::ProviderUnavailable,
                0,
                0,
            );
        }
        if input.conflict {
            return FastDecisionEvidence::unknown(
                spec,
                input,
                identity,
                now_epoch_secs,
                expires_at,
                UnknownReason::AnswerConflict,
                5_000,
                0,
            );
        }
        let confidence = if input.near_miss {
            spec.min_confidence_bps.saturating_sub(1)
        } else {
            BASIS_POINTS_MAX
        };
        let margin = if input.near_miss {
            spec.min_margin_bps.saturating_sub(1)
        } else {
            BASIS_POINTS_MAX
        };
        let probability = if input.near_miss { 5_000 } else { 9_500 };
        let reason = if confidence < spec.min_confidence_bps {
            Some(UnknownReason::LowConfidence)
        } else if margin < spec.min_margin_bps {
            Some(UnknownReason::MarginTooSmall)
        } else {
            None
        };
        if let Some(reason) = reason {
            return FastDecisionEvidence::unknown(
                spec,
                input,
                identity,
                now_epoch_secs,
                expires_at,
                reason,
                confidence,
                margin,
            );
        }
        let answer = match spec.kind {
            DecisionKind::Choice => {
                let index = (input.signal % spec.choices.len() as u64) as usize;
                let choice = spec.choices.iter().nth(index).expect("validated choices");
                OwnedDecisionAnswer::Choice(choice.clone())
            }
            DecisionKind::Score => {
                let min = spec.score_min.expect("validated score minimum");
                let max = spec.score_max.expect("validated score maximum");
                let span = (i128::from(max) - i128::from(min) + 1) as u128;
                let score = i128::from(min) + (u128::from(input.signal) % span) as i128;
                OwnedDecisionAnswer::Score(score as i64)
            }
            DecisionKind::Boolean => OwnedDecisionAnswer::Boolean(input.signal.is_multiple_of(2)),
        };
        FastDecisionEvidence::decided(
            spec,
            input,
            identity,
            now_epoch_secs,
            expires_at,
            answer,
            probability,
            confidence,
            margin,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastDecisionEvidence {
    contract_version: String,
    decision_spec_id: String,
    decision_spec_version: String,
    decision_spec_digest: [u8; 32],
    input_digest: [u8; 32],
    provider: AdapterIdentity,
    kind: DecisionKind,
    status: DecisionStatus,
    answer: Option<OwnedDecisionAnswer>,
    probability_bps: u16,
    confidence_bps: u16,
    margin_bps: u16,
    min_confidence_bps: u16,
    min_margin_bps: u16,
    fallback: FallbackAction,
    unknown_reason: Option<UnknownReason>,
    created_at_epoch_secs: u64,
    expires_at_epoch_secs: u64,
    evidence_digest: [u8; 32],
}

impl FastDecisionEvidence {
    /// Build a decided evidence item for an external adapter.
    ///
    /// The returned value is validated against the supplied spec before it
    /// leaves this module, so callers cannot publish a partially-bound item.
    #[allow(clippy::too_many_arguments)]
    pub fn decided_for_adapter(
        spec: &DecisionSpec,
        input: &BaselineInput,
        provider: AdapterIdentity,
        now_epoch_secs: u64,
        answer: OwnedDecisionAnswer,
        probability_bps: u16,
        confidence_bps: u16,
        margin_bps: u16,
    ) -> Result<Self, FastDecisionError> {
        spec.validate()?;
        input.validate()?;
        provider.validate()?;
        if now_epoch_secs == 0 || now_epoch_secs < input.created_at_epoch_secs {
            return Err(FastDecisionError::InvalidInput);
        }
        let expires_at = input
            .created_at_epoch_secs
            .checked_add(spec.expires_after_secs)
            .ok_or(FastDecisionError::InvalidInput)?;
        if now_epoch_secs > expires_at {
            return Err(FastDecisionError::InvalidInput);
        }
        Self::decided(
            spec,
            input,
            provider,
            now_epoch_secs,
            expires_at,
            answer,
            probability_bps,
            confidence_bps,
            margin_bps,
        )
    }

    /// Build an unknown evidence item for an external adapter.
    pub fn unknown_for_adapter(
        spec: &DecisionSpec,
        input: &BaselineInput,
        provider: AdapterIdentity,
        now_epoch_secs: u64,
        reason: UnknownReason,
        confidence_bps: u16,
        margin_bps: u16,
    ) -> Result<Self, FastDecisionError> {
        spec.validate()?;
        input.validate()?;
        provider.validate()?;
        if now_epoch_secs == 0 || now_epoch_secs < input.created_at_epoch_secs {
            return Err(FastDecisionError::InvalidInput);
        }
        let expires_at = input
            .created_at_epoch_secs
            .checked_add(spec.expires_after_secs)
            .ok_or(FastDecisionError::InvalidInput)?;
        Self::unknown(
            spec,
            input,
            provider,
            now_epoch_secs,
            expires_at,
            reason,
            confidence_bps,
            margin_bps,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn base(
        spec: &DecisionSpec,
        input: &BaselineInput,
        provider: AdapterIdentity,
        _now_epoch_secs: u64,
        expires_at: u64,
        status: DecisionStatus,
        answer: Option<OwnedDecisionAnswer>,
        probability_bps: u16,
        confidence_bps: u16,
        margin_bps: u16,
        unknown_reason: Option<UnknownReason>,
    ) -> Self {
        let mut evidence = Self {
            contract_version: FAST_DECISION_CONTRACT_VERSION.into(),
            decision_spec_id: spec.id.clone(),
            decision_spec_version: spec.version.clone(),
            decision_spec_digest: spec.digest(),
            input_digest: input.input_digest,
            provider,
            kind: spec.kind,
            status,
            answer,
            probability_bps,
            confidence_bps,
            margin_bps,
            min_confidence_bps: spec.min_confidence_bps,
            min_margin_bps: spec.min_margin_bps,
            fallback: spec.fallback,
            unknown_reason,
            created_at_epoch_secs: input.created_at_epoch_secs,
            expires_at_epoch_secs: expires_at,
            evidence_digest: [0; 32],
        };
        evidence.evidence_digest = evidence.content_digest();
        evidence
    }

    #[allow(clippy::too_many_arguments)]
    fn decided(
        spec: &DecisionSpec,
        input: &BaselineInput,
        provider: AdapterIdentity,
        now_epoch_secs: u64,
        expires_at: u64,
        answer: OwnedDecisionAnswer,
        probability_bps: u16,
        confidence_bps: u16,
        margin_bps: u16,
    ) -> Result<Self, FastDecisionError> {
        let evidence = Self::base(
            spec,
            input,
            provider,
            now_epoch_secs,
            expires_at,
            DecisionStatus::Decided,
            Some(answer),
            probability_bps,
            confidence_bps,
            margin_bps,
            None,
        );
        evidence.validate_against(spec)?;
        Ok(evidence)
    }

    #[allow(clippy::too_many_arguments)]
    fn unknown(
        spec: &DecisionSpec,
        input: &BaselineInput,
        provider: AdapterIdentity,
        now_epoch_secs: u64,
        expires_at: u64,
        reason: UnknownReason,
        confidence_bps: u16,
        margin_bps: u16,
    ) -> Result<Self, FastDecisionError> {
        let evidence = Self::base(
            spec,
            input,
            provider,
            now_epoch_secs,
            expires_at,
            DecisionStatus::Unknown,
            None,
            0,
            confidence_bps,
            margin_bps,
            Some(reason),
        );
        evidence.validate_against(spec)?;
        Ok(evidence)
    }

    fn invalid_input(
        spec: &DecisionSpec,
        input: &BaselineInput,
        provider: AdapterIdentity,
        now_epoch_secs: u64,
    ) -> Result<Self, FastDecisionError> {
        let mut normalized = input.clone();
        if normalized.input_digest == [0; 32] {
            let mut payload = b"fast-decision-invalid-input-v1\0".to_vec();
            payload.extend_from_slice(&normalized.signal.to_be_bytes());
            payload.extend_from_slice(&normalized.created_at_epoch_secs.to_be_bytes());
            payload.push(normalized.provider_available as u8);
            payload.push(normalized.near_miss as u8);
            payload.push(normalized.conflict as u8);
            normalized.input_digest = Sha256::digest(payload).into();
        }
        if normalized.created_at_epoch_secs == 0
            || now_epoch_secs == 0
            || now_epoch_secs < normalized.created_at_epoch_secs
            || normalized
                .created_at_epoch_secs
                .checked_add(spec.expires_after_secs)
                .is_none()
        {
            normalized.created_at_epoch_secs = 1;
        }
        let expires_at = normalized
            .created_at_epoch_secs
            .checked_add(spec.expires_after_secs)
            .expect("normalized invalid input timestamp is bounded");
        Self::unknown(
            spec,
            &normalized,
            provider,
            now_epoch_secs.max(1),
            expires_at,
            UnknownReason::InvalidInput,
            0,
            0,
        )
    }

    pub fn contract_version(&self) -> &str {
        &self.contract_version
    }

    pub fn decision_spec_id(&self) -> &str {
        &self.decision_spec_id
    }

    pub fn decision_spec_version(&self) -> &str {
        &self.decision_spec_version
    }

    pub fn decision_spec_digest(&self) -> [u8; 32] {
        self.decision_spec_digest
    }

    pub fn input_digest(&self) -> [u8; 32] {
        self.input_digest
    }

    pub fn provider(&self) -> &AdapterIdentity {
        &self.provider
    }

    pub fn kind(&self) -> DecisionKind {
        self.kind
    }

    pub fn status(&self) -> DecisionStatus {
        self.status
    }

    pub fn answer(&self) -> Option<&OwnedDecisionAnswer> {
        self.answer.as_ref()
    }

    pub fn probability_bps(&self) -> u16 {
        self.probability_bps
    }

    pub fn confidence_bps(&self) -> u16 {
        self.confidence_bps
    }

    pub fn margin_bps(&self) -> u16 {
        self.margin_bps
    }

    pub fn min_confidence_bps(&self) -> u16 {
        self.min_confidence_bps
    }

    pub fn min_margin_bps(&self) -> u16 {
        self.min_margin_bps
    }

    pub fn fallback(&self) -> FallbackAction {
        self.fallback
    }

    pub fn unknown_reason(&self) -> Option<UnknownReason> {
        self.unknown_reason
    }

    pub fn created_at_epoch_secs(&self) -> u64 {
        self.created_at_epoch_secs
    }

    pub fn expires_at_epoch_secs(&self) -> u64 {
        self.expires_at_epoch_secs
    }

    pub fn evidence_digest(&self) -> [u8; 32] {
        self.evidence_digest
    }

    pub fn content_digest(&self) -> [u8; 32] {
        let mut payload = FAST_DECISION_CONTRACT_VERSION.as_bytes().to_vec();
        payload.push(0);
        append_string(&mut payload, &self.decision_spec_id);
        append_string(&mut payload, &self.decision_spec_version);
        payload.extend_from_slice(&self.decision_spec_digest);
        payload.extend_from_slice(&self.input_digest);
        append_string(&mut payload, &self.provider.provider_id);
        append_string(&mut payload, &self.provider.provider_version);
        append_string(&mut payload, &self.provider.model_id);
        payload.push(kind_byte(self.kind));
        payload.push(status_byte(self.status));
        append_answer(&mut payload, self.answer.as_ref());
        payload.extend_from_slice(&self.probability_bps.to_be_bytes());
        payload.extend_from_slice(&self.confidence_bps.to_be_bytes());
        payload.extend_from_slice(&self.margin_bps.to_be_bytes());
        payload.extend_from_slice(&self.min_confidence_bps.to_be_bytes());
        payload.extend_from_slice(&self.min_margin_bps.to_be_bytes());
        payload.push(fallback_byte(self.fallback));
        payload.push(self.unknown_reason.map(unknown_reason_byte).unwrap_or(255));
        payload.extend_from_slice(&self.created_at_epoch_secs.to_be_bytes());
        payload.extend_from_slice(&self.expires_at_epoch_secs.to_be_bytes());
        Sha256::digest(payload).into()
    }

    pub fn validate_against(&self, spec: &DecisionSpec) -> Result<(), FastDecisionError> {
        spec.validate()?;
        self.validate_integrity()?;
        if self.contract_version != FAST_DECISION_CONTRACT_VERSION
            || self.decision_spec_id != spec.id
            || self.decision_spec_version != spec.version
            || self.decision_spec_digest != spec.digest()
            || self.kind != spec.kind
        {
            return Err(FastDecisionError::EvidenceBindingMismatch);
        }
        let expected_expiry = self
            .created_at_epoch_secs
            .checked_add(spec.expires_after_secs)
            .ok_or(FastDecisionError::InvalidEvidence)?;
        if self.expires_at_epoch_secs != expected_expiry {
            return Err(FastDecisionError::EvidenceBindingMismatch);
        }
        if self.min_confidence_bps != spec.min_confidence_bps || self.min_margin_bps != spec.min_margin_bps || self.fallback != spec.fallback {
            return Err(FastDecisionError::EvidenceBindingMismatch);
        }
        if let Some(answer) = self.answer.as_ref() {
            validate_answer(spec, answer)?;
        }
        Ok(())
    }

    pub fn validate_integrity(&self) -> Result<(), FastDecisionError> {
        self.provider.validate()?;
        if self.contract_version != FAST_DECISION_CONTRACT_VERSION
            || self.input_digest == [0; 32]
            || self.evidence_digest == [0; 32]
            || self.evidence_digest != self.content_digest()
            || self.created_at_epoch_secs == 0
            || self.expires_at_epoch_secs <= self.created_at_epoch_secs
        {
            return Err(FastDecisionError::InvalidEvidence);
        }
        for value in [
            self.probability_bps,
            self.confidence_bps,
            self.margin_bps,
            self.min_confidence_bps,
            self.min_margin_bps,
        ] {
            if value > BASIS_POINTS_MAX {
                return Err(FastDecisionError::InvalidEvidence);
            }
        }
        match (self.status, self.answer.as_ref(), self.unknown_reason) {
            (DecisionStatus::Decided, Some(answer), None)
                if answer.kind() == self.kind
                    && self.confidence_bps >= self.min_confidence_bps
                    && self.margin_bps >= self.min_margin_bps => {}
            (DecisionStatus::Unknown, None, Some(_)) => {}
            _ => return Err(FastDecisionError::InvalidEvidence),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationOutcome {
    Pending,
    Correct,
    Incorrect,
    NotApplicable,
}

impl CalibrationOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Correct => "CORRECT",
            Self::Incorrect => "INCORRECT",
            Self::NotApplicable => "NOT_APPLICABLE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastDecisionShadowRecord {
    evidence_digest: [u8; 32],
    evidence_status: DecisionStatus,
    existing_route: String,
    observed_route: Option<String>,
    disagreement: bool,
    abstain: bool,
    latency_millis: u64,
    calibration: CalibrationOutcome,
    record_digest: [u8; 32],
}

impl FastDecisionShadowRecord {
    pub fn new(
        evidence: &FastDecisionEvidence,
        existing_route: impl Into<String>,
        observed_route: Option<String>,
        latency_millis: u64,
        calibration: CalibrationOutcome,
    ) -> Result<Self, FastDecisionError> {
        evidence.validate_integrity()?;
        let existing_route = existing_route.into();
        validate_text("existing_route", &existing_route)?;
        if latency_millis == 0 {
            return Err(FastDecisionError::InvalidInput);
        }
        let observed_route = match observed_route {
            Some(route) => {
                validate_text("observed_route", &route)?;
                Some(route)
            }
            None => None,
        };
        match (evidence.status, observed_route.is_some()) {
            (DecisionStatus::Decided, false) | (DecisionStatus::Unknown, true) => {
                return Err(FastDecisionError::InvalidEvidence)
            }
            _ => {}
        }
        let abstain = evidence.status == DecisionStatus::Unknown;
        let disagreement = observed_route.as_deref().is_some_and(|route| route != existing_route);
        let mut record = Self {
            evidence_digest: evidence.evidence_digest,
            evidence_status: evidence.status,
            existing_route,
            observed_route,
            disagreement,
            abstain,
            latency_millis,
            calibration,
            record_digest: [0; 32],
        };
        record.record_digest = record.content_digest();
        Ok(record)
    }

    pub fn content_digest(&self) -> [u8; 32] {
        let mut payload = b"fast-decision-shadow-v2\0".to_vec();
        payload.extend_from_slice(&self.evidence_digest);
        payload.push(status_byte(self.evidence_status));
        append_string(&mut payload, &self.existing_route);
        append_optional_string(&mut payload, self.observed_route.as_deref());
        payload.push(self.disagreement as u8);
        payload.push(self.abstain as u8);
        payload.extend_from_slice(&self.latency_millis.to_be_bytes());
        payload.push(calibration_byte(self.calibration));
        Sha256::digest(payload).into()
    }

    pub fn validate(&self) -> Result<(), FastDecisionError> {
        validate_text("existing_route", &self.existing_route)?;
        if let Some(route) = &self.observed_route {
            validate_text("observed_route", route)?;
        }
        if self.evidence_digest == [0; 32] || self.record_digest != self.content_digest() || self.latency_millis == 0 {
            return Err(FastDecisionError::InvalidEvidence);
        }
        if self.abstain != (self.evidence_status == DecisionStatus::Unknown)
            || self.abstain != self.observed_route.is_none()
        {
            return Err(FastDecisionError::InvalidEvidence);
        }
        let expected_disagreement = self.observed_route.as_deref().is_some_and(|route| route != self.existing_route);
        if self.disagreement != expected_disagreement {
            return Err(FastDecisionError::InvalidEvidence);
        }
        Ok(())
    }

    pub fn evidence_digest(&self) -> [u8; 32] {
        self.evidence_digest
    }

    pub fn evidence_status(&self) -> DecisionStatus {
        self.evidence_status
    }

    pub fn existing_route(&self) -> &str {
        &self.existing_route
    }

    pub fn observed_route(&self) -> Option<&str> {
        self.observed_route.as_deref()
    }

    pub fn disagreement(&self) -> bool {
        self.disagreement
    }

    pub fn abstain(&self) -> bool {
        self.abstain
    }

    pub fn latency_millis(&self) -> u64 {
        self.latency_millis
    }

    pub fn calibration(&self) -> CalibrationOutcome {
        self.calibration
    }

    pub fn record_digest(&self) -> [u8; 32] {
        self.record_digest
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FastDecisionError {
    #[error("fast decision text field is invalid: {0}")]
    InvalidField(&'static str),
    #[error("fast decision spec is invalid: {0}")]
    InvalidSpec(&'static str),
    #[error("fast decision input is invalid")]
    InvalidInput,
    #[error("fast decision answer is invalid")]
    InvalidAnswer,
    #[error("fast decision evidence is invalid")]
    InvalidEvidence,
    #[error("fast decision evidence binding does not match its spec")]
    EvidenceBindingMismatch,
}

fn validate_answer(spec: &DecisionSpec, answer: &OwnedDecisionAnswer) -> Result<(), FastDecisionError> {
    match (spec.kind, answer) {
        (DecisionKind::Choice, OwnedDecisionAnswer::Choice(choice)) if spec.choices.contains(choice) => Ok(()),
        (DecisionKind::Score, OwnedDecisionAnswer::Score(score)) if Some(score) >= spec.score_min.as_ref() && Some(score) <= spec.score_max.as_ref() => Ok(()),
        (DecisionKind::Boolean, OwnedDecisionAnswer::Boolean(_)) => Ok(()),
        _ => Err(FastDecisionError::InvalidAnswer),
    }
}

fn validate_text(field: &'static str, value: &str) -> Result<(), FastDecisionError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(FastDecisionError::InvalidField(field));
    }
    Ok(())
}

fn append_string(payload: &mut Vec<u8>, value: &str) {
    append_u64(payload, value.len() as u64);
    payload.extend_from_slice(value.as_bytes());
}

fn append_optional_string(payload: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            payload.push(1);
            append_string(payload, value);
        }
        None => payload.push(0),
    }
}

fn append_optional_i64(payload: &mut Vec<u8>, value: Option<i64>) {
    match value {
        Some(value) => {
            payload.push(1);
            payload.extend_from_slice(&value.to_be_bytes());
        }
        None => payload.push(0),
    }
}

fn append_u64(payload: &mut Vec<u8>, value: u64) {
    payload.extend_from_slice(&value.to_be_bytes());
}

fn append_answer(payload: &mut Vec<u8>, answer: Option<&OwnedDecisionAnswer>) {
    match answer {
        None => payload.push(255),
        Some(OwnedDecisionAnswer::Choice(choice)) => {
            payload.push(0);
            append_string(payload, choice);
        }
        Some(OwnedDecisionAnswer::Score(score)) => {
            payload.push(1);
            payload.extend_from_slice(&score.to_be_bytes());
        }
        Some(OwnedDecisionAnswer::Boolean(value)) => {
            payload.push(2);
            payload.push(*value as u8);
        }
    }
}

fn kind_byte(value: DecisionKind) -> u8 {
    match value {
        DecisionKind::Choice => 0,
        DecisionKind::Score => 1,
        DecisionKind::Boolean => 2,
    }
}

fn status_byte(value: DecisionStatus) -> u8 {
    match value {
        DecisionStatus::Decided => 0,
        DecisionStatus::Unknown => 1,
    }
}

fn fallback_byte(value: FallbackAction) -> u8 {
    match value {
        FallbackAction::ExistingDeterministicPath => 0,
        FallbackAction::HumanReview => 1,
    }
}

fn unknown_reason_byte(value: UnknownReason) -> u8 {
    match value {
        UnknownReason::LowConfidence => 0,
        UnknownReason::MarginTooSmall => 1,
        UnknownReason::InputExpired => 2,
        UnknownReason::AnswerConflict => 3,
        UnknownReason::ProviderUnavailable => 4,
        UnknownReason::InvalidInput => 5,
    }
}

fn calibration_byte(value: CalibrationOutcome) -> u8 {
    match value {
        CalibrationOutcome::Pending => 0,
        CalibrationOutcome::Correct => 1,
        CalibrationOutcome::Incorrect => 2,
        CalibrationOutcome::NotApplicable => 3,
    }
}

pub fn hex_digest(value: &[u8; 32]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn parse_hex_digest(value: &str) -> Result<[u8; 32], FastDecisionError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(FastDecisionError::InvalidInput);
    }
    let mut digest = [0; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        digest[index] = (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]);
    }
    Ok(digest)
}

fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => unreachable!("parse_hex_digest validates hexadecimal input"),
    }
}

pub fn serialize_evidence(
    evidence: &FastDecisionEvidence,
    spec: &DecisionSpec,
) -> Result<String, FastDecisionError> {
    evidence.validate_against(spec)?;
    let mut output = String::new();
    output.push_str(&format!("format={FAST_DECISION_EVIDENCE_FORMAT}\n"));
    output.push_str(&format!("contract-version={}\n", evidence.contract_version));
    output.push_str(&format!("spec-id={}\n", evidence.decision_spec_id));
    output.push_str(&format!("spec-version={}\n", evidence.decision_spec_version));
    output.push_str(&format!("spec-digest={}\n", hex_digest(&evidence.decision_spec_digest)));
    output.push_str(&format!("input-digest={}\n", hex_digest(&evidence.input_digest)));
    output.push_str(&format!("provider={}\n", evidence.provider.provider_id));
    output.push_str(&format!("provider-version={}\n", evidence.provider.provider_version));
    output.push_str(&format!("model={}\n", evidence.provider.model_id));
    output.push_str(&format!("kind={}\n", evidence.kind.as_str()));
    output.push_str(&format!("status={}\n", evidence.status.as_str()));
    output.push_str(&format!("answer={}\n", format_answer(evidence.answer.as_ref())));
    output.push_str(&format!("probability-bps={}\n", evidence.probability_bps));
    output.push_str(&format!("confidence-bps={}\n", evidence.confidence_bps));
    output.push_str(&format!("margin-bps={}\n", evidence.margin_bps));
    output.push_str(&format!("min-confidence-bps={}\n", evidence.min_confidence_bps));
    output.push_str(&format!("min-margin-bps={}\n", evidence.min_margin_bps));
    output.push_str(&format!("fallback={}\n", evidence.fallback.as_str()));
    output.push_str(&format!("unknown-reason={}\n", evidence.unknown_reason.map(UnknownReason::as_str).unwrap_or("NONE")));
    output.push_str(&format!("created-at={}\n", evidence.created_at_epoch_secs));
    output.push_str(&format!("expires-at={}\n", evidence.expires_at_epoch_secs));
    output.push_str(&format!("evidence-digest={}\n", hex_digest(&evidence.evidence_digest)));
    Ok(output)
}

fn format_answer(answer: Option<&OwnedDecisionAnswer>) -> String {
    match answer {
        None => "UNKNOWN".into(),
        Some(OwnedDecisionAnswer::Choice(choice)) => format!("CHOICE:{choice}"),
        Some(OwnedDecisionAnswer::Score(score)) => format!("SCORE:{score}"),
        Some(OwnedDecisionAnswer::Boolean(value)) => format!("BOOLEAN:{value}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(signal: u64) -> BaselineInput {
        BaselineInput {
            input_digest: [7; 32],
            signal,
            created_at_epoch_secs: 1_000,
            provider_available: true,
            near_miss: false,
            conflict: false,
        }
    }

    #[test]
    fn choice_baseline_is_typed_and_bound_to_spec() {
        let spec = DecisionSpec::route_v1();
        let evidence = DeterministicBaselineAdapter.evaluate(&spec, &input(1), 1_100).unwrap();
        assert_eq!(evidence.kind, DecisionKind::Choice);
        assert_eq!(evidence.status, DecisionStatus::Decided);
        assert!(matches!(evidence.answer, Some(OwnedDecisionAnswer::Choice(_))));
        evidence.validate_against(&spec).unwrap();
    }

    #[test]
    fn score_and_boolean_specs_are_supported_without_text_parsing() {
        let score = DecisionSpec {
            id: "score".into(), version: "1".into(), kind: DecisionKind::Score,
            choices: BTreeSet::new(), score_min: Some(0), score_max: Some(100),
            min_confidence_bps: 1, min_margin_bps: 1, expires_after_secs: 100, fallback: FallbackAction::ExistingDeterministicPath,
        };
        let boolean = DecisionSpec {
            id: "boolean".into(), version: "1".into(), kind: DecisionKind::Boolean,
            choices: BTreeSet::new(), score_min: None, score_max: None,
            min_confidence_bps: 1, min_margin_bps: 1, expires_after_secs: 100, fallback: FallbackAction::ExistingDeterministicPath,
        };
        assert!(matches!(DeterministicBaselineAdapter.evaluate(&score, &input(4), 1_010).unwrap().answer, Some(OwnedDecisionAnswer::Score(_))));
        assert!(matches!(DeterministicBaselineAdapter.evaluate(&boolean, &input(4), 1_010).unwrap().answer, Some(OwnedDecisionAnswer::Boolean(true))));
    }

    #[test]
    fn extreme_score_range_is_evaluated_without_integer_overflow() {
        let spec = DecisionSpec {
            id: "extreme-score".into(),
            version: "1".into(),
            kind: DecisionKind::Score,
            choices: BTreeSet::new(),
            score_min: Some(i64::MIN),
            score_max: Some(i64::MAX),
            min_confidence_bps: 1,
            min_margin_bps: 1,
            expires_after_secs: 100,
            fallback: FallbackAction::ExistingDeterministicPath,
        };
        let evidence = DeterministicBaselineAdapter
            .evaluate(&spec, &input(u64::MAX), 1_010)
            .unwrap();
        assert!(matches!(evidence.answer, Some(OwnedDecisionAnswer::Score(_))));
        evidence.validate_against(&spec).unwrap();
    }

    #[test]
    fn near_miss_is_unknown_and_falls_back() {
        let spec = DecisionSpec::route_v1();
        let mut near_miss = input(2);
        near_miss.near_miss = true;
        let evidence = DeterministicBaselineAdapter.evaluate(&spec, &near_miss, 1_100).unwrap();
        assert_eq!(evidence.status, DecisionStatus::Unknown);
        assert_eq!(evidence.unknown_reason, Some(UnknownReason::LowConfidence));
        assert_eq!(evidence.fallback, FallbackAction::HumanReview);
        evidence.validate_against(&spec).unwrap();
    }

    #[test]
    fn holdout_signal_remains_deterministic_but_is_not_training_claim() {
        let spec = DecisionSpec::route_v1();
        let first = DeterministicBaselineAdapter.evaluate(&spec, &input(1), 1_100).unwrap();
        let holdout = DeterministicBaselineAdapter.evaluate(&spec, &input(2), 1_100).unwrap();
        assert_ne!(first.answer, holdout.answer);
        first.validate_against(&spec).unwrap();
        holdout.validate_against(&spec).unwrap();
    }

    #[test]
    fn margin_threshold_can_abstain_independently_of_confidence() {
        let spec = DecisionSpec { min_confidence_bps: 0, min_margin_bps: 1_000, ..DecisionSpec::route_v1() };
        let mut near_miss = input(1);
        near_miss.near_miss = true;
        let evidence = DeterministicBaselineAdapter.evaluate(&spec, &near_miss, 1_100).unwrap();
        assert_eq!(evidence.unknown_reason, Some(UnknownReason::MarginTooSmall));
    }

    #[test]
    fn expiry_provider_failure_and_conflict_are_unknown() {
        let spec = DecisionSpec::route_v1();
        let mut unavailable = input(1);
        unavailable.provider_available = false;
        assert_eq!(DeterministicBaselineAdapter.evaluate(&spec, &unavailable, 1_100).unwrap().unknown_reason, Some(UnknownReason::ProviderUnavailable));
        let mut conflict = input(1);
        conflict.conflict = true;
        assert_eq!(DeterministicBaselineAdapter.evaluate(&spec, &conflict, 1_100).unwrap().unknown_reason, Some(UnknownReason::AnswerConflict));
        assert_eq!(DeterministicBaselineAdapter.evaluate(&spec, &input(1), 5_000).unwrap().unknown_reason, Some(UnknownReason::InputExpired));
    }

    #[test]
    fn invalid_input_is_emitted_as_unknown_evidence() {
        let spec = DecisionSpec::route_v1();
        let invalid = BaselineInput {
            input_digest: [0; 32],
            signal: 9,
            created_at_epoch_secs: 0,
            provider_available: true,
            near_miss: false,
            conflict: false,
        };
        let evidence = DeterministicBaselineAdapter.evaluate(&spec, &invalid, 0).unwrap();
        assert_eq!(evidence.status, DecisionStatus::Unknown);
        assert_eq!(evidence.unknown_reason, Some(UnknownReason::InvalidInput));
        evidence.validate_against(&spec).unwrap();
        serialize_evidence(&evidence, &spec).unwrap();
    }

    #[test]
    fn impossible_expiry_and_expired_decided_factory_fail_closed() {
        let invalid_spec = DecisionSpec {
            expires_after_secs: u64::MAX,
            ..DecisionSpec::route_v1()
        };
        assert_eq!(invalid_spec.validate(), Err(FastDecisionError::InvalidSpec("expiry")));

        let spec = DecisionSpec::route_v1();
        let provider = DeterministicBaselineAdapter.identity();
        assert_eq!(
            FastDecisionEvidence::decided_for_adapter(
                &spec,
                &input(1),
                provider,
                5_000,
                OwnedDecisionAnswer::Choice("LOCAL_EXPLAIN".into()),
                9_500,
                10_000,
                10_000,
            ),
            Err(FastDecisionError::InvalidInput)
        );
    }

    #[test]
    fn tampered_evidence_and_spec_mismatch_fail_closed() {
        let spec = DecisionSpec::route_v1();
        let mut evidence = DeterministicBaselineAdapter.evaluate(&spec, &input(1), 1_100).unwrap();
        evidence.confidence_bps = 1;
        assert_eq!(evidence.validate_against(&spec), Err(FastDecisionError::InvalidEvidence));
        let other = DecisionSpec { version: "2".into(), ..spec };
        let evidence = DeterministicBaselineAdapter.evaluate(&DecisionSpec::route_v1(), &input(1), 1_100).unwrap();
        assert_eq!(evidence.validate_against(&other), Err(FastDecisionError::EvidenceBindingMismatch));
    }

    #[test]
    fn expiry_is_exactly_bound_to_the_spec() {
        let spec = DecisionSpec::route_v1();
        let mut evidence = DeterministicBaselineAdapter.evaluate(&spec, &input(1), 1_100).unwrap();
        evidence.expires_at_epoch_secs += 1;
        evidence.evidence_digest = evidence.content_digest();
        assert_eq!(evidence.validate_against(&spec), Err(FastDecisionError::EvidenceBindingMismatch));
    }

    #[test]
    fn shadow_record_captures_disagreement_abstain_latency_and_calibration() {
        let spec = DecisionSpec::route_v1();
        let evidence = DeterministicBaselineAdapter.evaluate(&spec, &input(1), 1_100).unwrap();
        let record = FastDecisionShadowRecord::new(&evidence, "HUMAN_REVIEW", Some("LOCAL_EXPLAIN".into()), 12, CalibrationOutcome::Pending).unwrap();
        assert!(record.disagreement);
        assert!(!record.abstain);
        record.validate().unwrap();
    }

    #[test]
    fn shadow_record_requires_route_only_for_decided_evidence() {
        let spec = DecisionSpec::route_v1();
        let mut near_miss = input(1);
        near_miss.near_miss = true;
        let unknown = DeterministicBaselineAdapter.evaluate(&spec, &near_miss, 1_100).unwrap();
        assert_eq!(
            FastDecisionShadowRecord::new(
                &unknown,
                "HUMAN_REVIEW",
                Some("HUMAN_REVIEW".into()),
                12,
                CalibrationOutcome::NotApplicable,
            ),
            Err(FastDecisionError::InvalidEvidence)
        );
        let decided = DeterministicBaselineAdapter.evaluate(&spec, &input(1), 1_100).unwrap();
        assert_eq!(
            FastDecisionShadowRecord::new(
                &decided,
                "HUMAN_REVIEW",
                None,
                12,
                CalibrationOutcome::Pending,
            ),
            Err(FastDecisionError::InvalidEvidence)
        );
    }

    #[test]
    fn shadow_record_cannot_rebind_evidence_status() {
        let spec = DecisionSpec::route_v1();
        let decided = DeterministicBaselineAdapter.evaluate(&spec, &input(1), 1_100).unwrap();
        let mut record = FastDecisionShadowRecord::new(
            &decided,
            "HUMAN_REVIEW",
            Some("LOCAL_EXPLAIN".into()),
            12,
            CalibrationOutcome::Pending,
        )
        .unwrap();
        record.evidence_status = DecisionStatus::Unknown;
        record.record_digest = record.content_digest();
        assert_eq!(record.validate(), Err(FastDecisionError::InvalidEvidence));
    }

    #[test]
    fn serialized_evidence_is_digest_only_and_deterministic() {
        let spec = DecisionSpec::route_v1();
        let evidence = DeterministicBaselineAdapter.evaluate(&spec, &input(1), 1_100).unwrap();
        let first = serialize_evidence(&evidence, &spec).unwrap();
        let second = serialize_evidence(&evidence, &spec).unwrap();
        assert_eq!(first, second);
        assert!(first.contains("input-digest="));
        assert!(!first.contains("source"));
        assert!(!first.contains("Finding"));
    }
}
