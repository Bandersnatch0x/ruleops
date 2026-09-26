//! Fail-closed incremental-scan fallback and performance evidence.
//!
//! This crate is deliberately smaller than a build/SCM adapter. It consumes
//! the immutable identities and coverage facts produced by `erf-scan-plan`,
//! records only monotonic durations plus digest summaries, and turns an
//! incremental request into either a diagnostic fast path or an explicit full
//! authority path. The synthetic fixture matrix is a local evidence generator;
//! it does not claim measurements from external repositories.

use erf_scan_plan::{CompletenessReport, PlanAuthority, PlanMode, ScanPlan};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Instant;
use thiserror::Error;

pub const INCREMENTAL_BASELINE_VERSION: &str = "incremental-baseline/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum ExecutionRoute {
    Full,
    Incremental,
}

impl ExecutionRoute {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Full => "FULL",
            Self::Incremental => "INCREMENTAL",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum CacheObservation {
    NotUsed,
    ColdMiss,
    WarmHit,
    Corrupt,
    Expired,
    IdentityMismatch,
    Incomplete,
}

impl CacheObservation {
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotUsed => "NOT_USED",
            Self::ColdMiss => "COLD_MISS",
            Self::WarmHit => "WARM_HIT",
            Self::Corrupt => "CORRUPT",
            Self::Expired => "EXPIRED",
            Self::IdentityMismatch => "IDENTITY_MISMATCH",
            Self::Incomplete => "INCOMPLETE",
        }
    }

}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum RuleMatch {
    Hit,
    ZeroHit,
}

impl RuleMatch {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Hit => "HIT",
            Self::ZeroHit => "ZERO_HIT",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum BuildOutcome {
    Success,
    Failed,
}

impl BuildOutcome {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Failed => "FAILED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum DependencyGraphState {
    Proven,
    Uncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum GeneratedSourceState {
    Stable,
    Changed,
    Uncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum MergeBaseState {
    Verifiable,
    Unverifiable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum FallbackReason {
    BuildConfigurationChanged,
    RuleBundleChanged,
    PolicyChanged,
    ResolverChanged,
    DependencyGraphUncertain,
    GeneratedSourceChanged,
    GeneratedSourceUncertain,
    CacheCorrupt,
    CacheExpired,
    CacheIdentityMismatch,
    CacheIncomplete,
    MergeBaseUnverifiable,
    ScopeBudgetExceeded,
    BuildFailed,
    PlanNotAuthoritative,
    PlanIncomplete,
    PlanWidenedToFull,
}

impl FallbackReason {
    pub const fn code(self) -> &'static str {
        match self {
            Self::BuildConfigurationChanged => "build-configuration-changed",
            Self::RuleBundleChanged => "rule-bundle-changed",
            Self::PolicyChanged => "policy-changed",
            Self::ResolverChanged => "resolver-changed",
            Self::DependencyGraphUncertain => "dependency-graph-uncertain",
            Self::GeneratedSourceChanged => "generated-source-changed",
            Self::GeneratedSourceUncertain => "generated-source-uncertain",
            Self::CacheCorrupt => "cache-corrupt",
            Self::CacheExpired => "cache-expired",
            Self::CacheIdentityMismatch => "cache-identity-mismatch",
            Self::CacheIncomplete => "cache-incomplete",
            Self::MergeBaseUnverifiable => "merge-base-unverifiable",
            Self::ScopeBudgetExceeded => "scope-budget-exceeded",
            Self::BuildFailed => "build-failed",
            Self::PlanNotAuthoritative => "plan-not-authoritative",
            Self::PlanIncomplete => "plan-incomplete",
            Self::PlanWidenedToFull => "plan-widened-to-full",
        }
    }
}

impl fmt::Display for FallbackReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// Only immutable ScanPlan facts cross into the fallback decision. No source
/// path, repository URL, credential, or full Finding is accepted here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanObservation {
    pub plan_digest: String,
    pub mode: PlanMode,
    pub authority: PlanAuthority,
    pub planned_units: usize,
    pub complete: bool,
}

impl PlanObservation {
    pub fn from_plan(plan: &ScanPlan) -> Self {
        Self {
            plan_digest: plan.digest().to_string(),
            mode: plan.mode(),
            authority: plan.authority(),
            planned_units: plan.planned_units().len(),
            complete: false,
        }
    }

    pub fn from_completion(plan: &ScanPlan, report: &CompletenessReport) -> Result<Self, BaselineError> {
        if report.plan_digest() != plan.digest() {
            return Err(BaselineError::PlanMismatch);
        }
        Ok(Self {
            plan_digest: plan.digest().to_string(),
            mode: report.mode(),
            authority: report.authority(),
            planned_units: report.gate().planned,
            complete: report.allows_authoritative_gate(),
        })
    }

    pub fn new(
        plan_digest: impl Into<String>,
        mode: PlanMode,
        authority: PlanAuthority,
        planned_units: usize,
        complete: bool,
    ) -> Result<Self, BaselineError> {
        let plan_digest = plan_digest.into();
        validate_digest("plan_digest", &plan_digest)?;
        if planned_units == 0 {
            return Err(BaselineError::InvalidValue("planned_units"));
        }
        Ok(Self { plan_digest, mode, authority, planned_units, complete })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FallbackInputs {
    pub requested_route: ExecutionRoute,
    pub plan: PlanObservation,
    pub cache: CacheObservation,
    pub build: BuildOutcome,
    pub build_configuration_changed: bool,
    pub rule_bundle_changed: bool,
    pub policy_changed: bool,
    pub resolver_changed: bool,
    pub dependency_graph: DependencyGraphState,
    pub generated_source: GeneratedSourceState,
    pub merge_base: MergeBaseState,
    pub affected_unit_count: usize,
    pub scope_budget: usize,
}

impl FallbackInputs {
    fn validate(&self) -> Result<(), BaselineError> {
        validate_digest("plan_digest", &self.plan.plan_digest)?;
        if self.plan.planned_units == 0 {
            return Err(BaselineError::InvalidValue("planned_units"));
        }
        if self.scope_budget == 0 {
            return Err(BaselineError::InvalidValue("scope_budget"));
        }
        Ok(())
    }
}

/// The outcome is intentionally explicit: an incremental result is a fast
/// diagnostic until a complete full authority run has been paired with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FallbackDecision {
    pub requested_route: ExecutionRoute,
    pub effective_route: ExecutionRoute,
    pub plan_digest: String,
    pub cache: CacheObservation,
    pub reasons: Vec<FallbackReason>,
    pub fast_feedback: bool,
    pub requires_authoritative_full: bool,
}

impl FallbackDecision {
    pub fn is_fallback(&self) -> bool {
        self.requested_route == ExecutionRoute::Incremental
            && self.effective_route == ExecutionRoute::Full
            && !self.reasons.is_empty()
    }

    pub fn has_reason(&self, reason: FallbackReason) -> bool {
        self.reasons.contains(&reason)
    }

    pub fn allows_authoritative_gate(&self) -> bool {
        !self.requires_authoritative_full || self.effective_route == ExecutionRoute::Full
    }
}

pub fn assess_fallback(inputs: FallbackInputs) -> Result<FallbackDecision, BaselineError> {
    inputs.validate()?;
    let mut reasons = BTreeSet::new();

    if inputs.build_configuration_changed {
        reasons.insert(FallbackReason::BuildConfigurationChanged);
    }
    if inputs.rule_bundle_changed {
        reasons.insert(FallbackReason::RuleBundleChanged);
    }
    if inputs.policy_changed {
        reasons.insert(FallbackReason::PolicyChanged);
    }
    if inputs.resolver_changed {
        reasons.insert(FallbackReason::ResolverChanged);
    }
    if inputs.dependency_graph == DependencyGraphState::Uncertain {
        reasons.insert(FallbackReason::DependencyGraphUncertain);
    }
    match inputs.generated_source {
        GeneratedSourceState::Stable => {}
        GeneratedSourceState::Changed => {
            reasons.insert(FallbackReason::GeneratedSourceChanged);
        }
        GeneratedSourceState::Uncertain => {
            reasons.insert(FallbackReason::GeneratedSourceUncertain);
        }
    }
    match inputs.cache {
        CacheObservation::Corrupt => {
            reasons.insert(FallbackReason::CacheCorrupt);
        }
        CacheObservation::Expired => {
            reasons.insert(FallbackReason::CacheExpired);
        }
        CacheObservation::IdentityMismatch => {
            reasons.insert(FallbackReason::CacheIdentityMismatch);
        }
        CacheObservation::Incomplete => {
            reasons.insert(FallbackReason::CacheIncomplete);
        }
        CacheObservation::NotUsed | CacheObservation::ColdMiss | CacheObservation::WarmHit => {}
    }
    if inputs.merge_base == MergeBaseState::Unverifiable {
        reasons.insert(FallbackReason::MergeBaseUnverifiable);
    }
    if inputs.affected_unit_count > inputs.scope_budget {
        reasons.insert(FallbackReason::ScopeBudgetExceeded);
    }
    if inputs.build == BuildOutcome::Failed {
        reasons.insert(FallbackReason::BuildFailed);
    }
    if inputs.plan.authority != PlanAuthority::GateEligible {
        reasons.insert(FallbackReason::PlanNotAuthoritative);
    }
    if !inputs.plan.complete {
        reasons.insert(FallbackReason::PlanIncomplete);
    }
    if inputs.requested_route == ExecutionRoute::Incremental && inputs.plan.mode == PlanMode::Full {
        reasons.insert(FallbackReason::PlanWidenedToFull);
    }
    let reasons = reasons.into_iter().collect::<Vec<_>>();
    let effective_route = if reasons.is_empty() {
        inputs.requested_route
    } else {
        ExecutionRoute::Full
    };
    let fast_feedback = inputs.requested_route == ExecutionRoute::Incremental
        && reasons.is_empty()
        && inputs.build == BuildOutcome::Success;
    Ok(FallbackDecision {
        requested_route: inputs.requested_route,
        effective_route,
        plan_digest: inputs.plan.plan_digest,
        cache: inputs.cache,
        reasons,
        fast_feedback,
        requires_authoritative_full: inputs.requested_route == ExecutionRoute::Incremental,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Stage {
    Resolve,
    AffectedScope,
    Compile,
    Backend,
    Cache,
    Gate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum StageOutcome {
    Completed,
    Failed,
    Skipped,
}

impl Stage {
    pub const ALL: [Self; 6] = [
        Self::Resolve,
        Self::AffectedScope,
        Self::Compile,
        Self::Backend,
        Self::Cache,
        Self::Gate,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Resolve => "resolve",
            Self::AffectedScope => "affected-scope",
            Self::Compile => "compile",
            Self::Backend => "backend",
            Self::Cache => "cache",
            Self::Gate => "gate",
        }
    }
}

/// A stage summary has no wall-clock timestamp and cannot carry source text.
/// `StageTimer` below is the only constructor intended for live execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageMeasurement {
    pub stage: Stage,
    pub elapsed_nanos: u64,
    pub input_digest: String,
    pub outcome: StageOutcome,
}

impl StageMeasurement {
    pub fn new(stage: Stage, elapsed_nanos: u64, input_digest: impl Into<String>) -> Result<Self, BaselineError> {
        let input_digest = input_digest.into();
        validate_digest("stage_input_digest", &input_digest)?;
        Ok(Self { stage, elapsed_nanos, input_digest, outcome: StageOutcome::Completed })
    }

    pub fn skipped(stage: Stage, input_digest: impl Into<String>) -> Result<Self, BaselineError> {
        let input_digest = input_digest.into();
        validate_digest("stage_input_digest", &input_digest)?;
        Ok(Self { stage, elapsed_nanos: 0, input_digest, outcome: StageOutcome::Skipped })
    }

    pub fn failed(stage: Stage, elapsed_nanos: u64, input_digest: impl Into<String>) -> Result<Self, BaselineError> {
        let input_digest = input_digest.into();
        validate_digest("stage_input_digest", &input_digest)?;
        Ok(Self { stage, elapsed_nanos, input_digest, outcome: StageOutcome::Failed })
    }
}

pub struct StageTimer {
    stage: Stage,
    input_digest: String,
    started: Instant,
}

impl StageTimer {
    pub fn start(stage: Stage, input_digest: impl Into<String>) -> Result<Self, BaselineError> {
        let input_digest = input_digest.into();
        validate_digest("stage_input_digest", &input_digest)?;
        Ok(Self { stage, input_digest, started: Instant::now() })
    }

    pub fn finish(self) -> StageMeasurement {
        self.finish_with_outcome(StageOutcome::Completed)
    }

    pub fn finish_with_outcome(self, outcome: StageOutcome) -> StageMeasurement {
        StageMeasurement {
            stage: self.stage,
            elapsed_nanos: self.started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
            input_digest: self.input_digest,
            outcome,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StageTelemetry {
    measurements: Vec<StageMeasurement>,
}

impl StageTelemetry {
    pub fn from_measurements(measurements: Vec<StageMeasurement>) -> Result<Self, BaselineError> {
        if measurements.is_empty() {
            return Err(BaselineError::InvalidValue("stage_measurements"));
        }
        let mut seen = BTreeSet::new();
        for measurement in &measurements {
            if !seen.insert(measurement.stage) {
                return Err(BaselineError::DuplicateStage(measurement.stage));
            }
            validate_digest("stage_input_digest", &measurement.input_digest)?;
        }
        let mut measurements = measurements;
        measurements.sort_by_key(|measurement| measurement.stage);
        if !Stage::ALL.iter().all(|stage| measurements.iter().any(|entry| entry.stage == *stage)) {
            return Err(BaselineError::MissingStage);
        }
        Ok(Self { measurements })
    }

    pub fn new() -> Self {
        Self { measurements: Vec::new() }
    }

    pub fn record(&mut self, measurement: StageMeasurement) -> Result<(), BaselineError> {
        if self.measurements.iter().any(|existing| existing.stage == measurement.stage) {
            return Err(BaselineError::DuplicateStage(measurement.stage));
        }
        validate_digest("stage_input_digest", &measurement.input_digest)?;
        self.measurements.push(measurement);
        self.measurements.sort_by_key(|entry| entry.stage);
        Ok(())
    }

    pub fn finish(&mut self, timer: StageTimer) -> Result<(), BaselineError> {
        self.record(timer.finish())
    }

    pub fn finish_with_outcome(&mut self, timer: StageTimer, outcome: StageOutcome) -> Result<(), BaselineError> {
        self.record(timer.finish_with_outcome(outcome))
    }

    pub fn record_skipped(&mut self, stage: Stage, input_digest: impl Into<String>) -> Result<(), BaselineError> {
        self.record(StageMeasurement::skipped(stage, input_digest)?)
    }

    pub fn measurements(&self) -> &[StageMeasurement] {
        &self.measurements
    }

    pub fn replace_elapsed(&mut self, stage: Stage, elapsed_nanos: u64) -> Result<(), BaselineError> {
        let measurement = self
            .measurements
            .iter_mut()
            .find(|measurement| measurement.stage == stage)
            .ok_or(BaselineError::MissingStage)?;
        measurement.elapsed_nanos = elapsed_nanos;
        Ok(())
    }

    pub fn replace_measurement(&mut self, measurement: StageMeasurement) -> Result<(), BaselineError> {
        validate_digest("stage_input_digest", &measurement.input_digest)?;
        if let Some(existing) = self.measurements.iter_mut().find(|entry| entry.stage == measurement.stage) {
            *existing = measurement;
            Ok(())
        } else {
            self.record(measurement)
        }
    }

    pub fn total_elapsed_nanos(&self) -> u64 {
        self.measurements.iter().fold(0, |total, measurement| total.saturating_add(measurement.elapsed_nanos))
    }

    pub fn has_all_required_stages(&self) -> bool {
        Stage::ALL.iter().all(|stage| {
            self.measurements
                .iter()
                .any(|entry| entry.stage == *stage && entry.outcome == StageOutcome::Completed)
        })
    }

    pub fn has_all_stage_summaries(&self) -> bool {
        Stage::ALL.iter().all(|stage| self.measurements.iter().any(|entry| entry.stage == *stage))
    }

    pub fn digest(&self) -> String {
        let mut bytes = INCREMENTAL_BASELINE_VERSION.as_bytes().to_vec();
        bytes.push(0);
        for measurement in &self.measurements {
            append_string(&mut bytes, measurement.stage.code());
            bytes.extend_from_slice(&measurement.elapsed_nanos.to_be_bytes());
            append_string(&mut bytes, &measurement.input_digest);
            append_string(&mut bytes, match measurement.outcome {
                StageOutcome::Completed => "completed",
                StageOutcome::Failed => "failed",
                StageOutcome::Skipped => "skipped",
            });
        }
        hex_digest(&Sha256::digest(bytes))
    }
}

impl Default for StageTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl<'de> Deserialize<'de> for StageTelemetry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireStageTelemetry {
            measurements: Vec<StageMeasurement>,
        }

        let raw = WireStageTelemetry::deserialize(deserializer)?;
        Self::from_measurements(raw.measurements).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoverageObservation {
    planned_units: usize,
    accounted_units: usize,
    complete: bool,
}

impl CoverageObservation {
    pub fn new(planned_units: usize, accounted_units: usize, complete: bool) -> Result<Self, BaselineError> {
        if planned_units == 0 || accounted_units > planned_units {
            return Err(BaselineError::InvalidValue("coverage"));
        }
        if complete && accounted_units != planned_units {
            return Err(BaselineError::InvalidValue("complete_coverage"));
        }
        Ok(Self { planned_units, accounted_units, complete })
    }

    pub fn percent(&self) -> u8 {
        ((self.accounted_units.saturating_mul(100)) / self.planned_units).min(100) as u8
    }

    pub fn planned_units(&self) -> usize {
        self.planned_units
    }

    pub fn accounted_units(&self) -> usize {
        self.accounted_units
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }
}

impl<'de> Deserialize<'de> for CoverageObservation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireCoverageObservation {
            planned_units: usize,
            accounted_units: usize,
            complete: bool,
        }

        let raw = WireCoverageObservation::deserialize(deserializer)?;
        Self::new(raw.planned_units, raw.accounted_units, raw.complete)
            .map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityComparison {
    pub full_semantic_result_digest: String,
    pub incremental_semantic_result_digest: String,
    pub full_completeness_digest: String,
    pub incremental_completeness_digest: String,
}

impl SecurityComparison {
    pub fn new(
        full_semantic_result_digest: impl Into<String>,
        incremental_semantic_result_digest: impl Into<String>,
        full_completeness_digest: impl Into<String>,
        incremental_completeness_digest: impl Into<String>,
    ) -> Result<Self, BaselineError> {
        let comparison = Self {
            full_semantic_result_digest: full_semantic_result_digest.into(),
            incremental_semantic_result_digest: incremental_semantic_result_digest.into(),
            full_completeness_digest: full_completeness_digest.into(),
            incremental_completeness_digest: incremental_completeness_digest.into(),
        };
        validate_digest("full_semantic_result_digest", &comparison.full_semantic_result_digest)?;
        validate_digest("incremental_semantic_result_digest", &comparison.incremental_semantic_result_digest)?;
        validate_digest("full_completeness_digest", &comparison.full_completeness_digest)?;
        validate_digest("incremental_completeness_digest", &comparison.incremental_completeness_digest)?;
        Ok(comparison)
    }

    pub fn equivalent(&self) -> bool {
        self.full_semantic_result_digest == self.incremental_semantic_result_digest
            && self.full_completeness_digest == self.incremental_completeness_digest
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum FixtureScale {
    Small,
    Medium,
    Large,
}

impl FixtureScale {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Small => "SMALL",
            Self::Medium => "MEDIUM",
            Self::Large => "LARGE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum BenchmarkBand {
    Full,
    IncrementalCold,
    IncrementalWarm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FixtureTimingSample {
    pub scale: FixtureScale,
    pub band: BenchmarkBand,
    pub repetition: u8,
    pub elapsed_nanos: u64,
}

impl FixtureTimingSample {
    pub fn new(scale: FixtureScale, band: BenchmarkBand, repetition: u8, elapsed_nanos: u64) -> Result<Self, BaselineError> {
        if repetition >= 3 || elapsed_nanos == 0 {
            return Err(BaselineError::InvalidValue("fixture_timing_sample"));
        }
        Ok(Self { scale, band, repetition, elapsed_nanos })
    }
}

impl<'de> Deserialize<'de> for FixtureTimingSample {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireFixtureTimingSample {
            scale: FixtureScale,
            band: BenchmarkBand,
            repetition: u8,
            elapsed_nanos: u64,
        }

        let raw = WireFixtureTimingSample::deserialize(deserializer)?;
        Self::new(raw.scale, raw.band, raw.repetition, raw.elapsed_nanos)
            .map_err(de::Error::custom)
    }
}

impl BenchmarkBand {
    pub const ALL: [Self; 3] = [Self::Full, Self::IncrementalCold, Self::IncrementalWarm];

    pub const fn code(self) -> &'static str {
        match self {
            Self::Full => "FULL",
            Self::IncrementalCold => "INCREMENTAL_COLD",
            Self::IncrementalWarm => "INCREMENTAL_WARM",
        }
    }
}

fn band_for(route: ExecutionRoute, cache: CacheObservation) -> BenchmarkBand {
    match route {
        ExecutionRoute::Full => BenchmarkBand::Full,
        ExecutionRoute::Incremental => match cache {
            CacheObservation::WarmHit => BenchmarkBand::IncrementalWarm,
            CacheObservation::NotUsed
            | CacheObservation::ColdMiss
            | CacheObservation::Corrupt
            | CacheObservation::Expired
            | CacheObservation::IdentityMismatch
            | CacheObservation::Incomplete => BenchmarkBand::IncrementalCold,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BenchmarkCase {
    pub case_id: String,
    pub repository_digest: String,
    pub scale: FixtureScale,
    pub route: ExecutionRoute,
    pub cache_temperature: CacheObservation,
    pub cache: CacheObservation,
    pub rule_match: RuleMatch,
    pub build: BuildOutcome,
    pub fallback: FallbackDecision,
    pub telemetry: StageTelemetry,
    pub coverage: CoverageObservation,
    pub security_comparison: Option<SecurityComparison>,
}

impl BenchmarkCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        case_id: impl Into<String>,
        repository_digest: impl Into<String>,
        scale: FixtureScale,
        route: ExecutionRoute,
        cache_temperature: CacheObservation,
        cache: CacheObservation,
        rule_match: RuleMatch,
        build: BuildOutcome,
        fallback: FallbackDecision,
        telemetry: StageTelemetry,
        coverage: CoverageObservation,
        security_comparison: Option<SecurityComparison>,
    ) -> Result<Self, BaselineError> {
        let case_id = case_id.into();
        let repository_digest = repository_digest.into();
        if case_id.trim().is_empty() || case_id.chars().any(char::is_control) {
            return Err(BaselineError::InvalidValue("case_id"));
        }
        validate_digest("repository_digest", &repository_digest)?;
        if fallback.requested_route != route {
            return Err(BaselineError::RouteMismatch);
        }
        if !telemetry.has_all_required_stages() {
            return Err(BaselineError::MissingStage);
        }
        Ok(Self {
            case_id,
            repository_digest,
            scale,
            route,
            cache_temperature,
            cache,
            rule_match,
            build,
            fallback,
            telemetry,
            coverage,
            security_comparison,
        })
    }

    pub fn band(&self) -> BenchmarkBand {
        band_for(self.route, self.cache_temperature)
    }

    pub fn total_elapsed_nanos(&self) -> u64 {
        self.telemetry.total_elapsed_nanos()
    }

    pub fn security_equivalent(&self) -> Option<bool> {
        self.security_comparison.as_ref().map(SecurityComparison::equivalent)
    }

    pub fn replace_stage_elapsed(&mut self, stage: Stage, elapsed_nanos: u64) -> Result<(), BaselineError> {
        self.telemetry.replace_elapsed(stage, elapsed_nanos)
    }
}

impl<'de> Deserialize<'de> for BenchmarkCase {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireBenchmarkCase {
            case_id: String,
            repository_digest: String,
            scale: FixtureScale,
            route: ExecutionRoute,
            cache_temperature: CacheObservation,
            cache: CacheObservation,
            rule_match: RuleMatch,
            build: BuildOutcome,
            fallback: FallbackDecision,
            telemetry: StageTelemetry,
            coverage: CoverageObservation,
            security_comparison: Option<SecurityComparison>,
        }

        let raw = WireBenchmarkCase::deserialize(deserializer)?;
        Self::new(
            raw.case_id,
            raw.repository_digest,
            raw.scale,
            raw.route,
            raw.cache_temperature,
            raw.cache,
            raw.rule_match,
            raw.build,
            raw.fallback,
            raw.telemetry,
            raw.coverage,
            raw.security_comparison,
        )
        .map_err(de::Error::custom)
    }
}

fn is_fault_injection(case: &BenchmarkCase) -> bool {
    case.fallback.is_fallback()
        || case.build == BuildOutcome::Failed
        || matches!(
            case.cache,
            CacheObservation::Corrupt
                | CacheObservation::Expired
                | CacheObservation::IdentityMismatch
                | CacheObservation::Incomplete
        )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetPolicy {
    pub full_p95_max_nanos: u64,
    pub incremental_cold_p95_max_nanos: u64,
    pub incremental_warm_p95_max_nanos: u64,
    pub minimum_coverage_percent: u8,
}

impl BudgetPolicy {
    pub fn validate(&self) -> Result<(), BaselineError> {
        if self.full_p95_max_nanos == 0
            || self.incremental_cold_p95_max_nanos == 0
            || self.incremental_warm_p95_max_nanos == 0
            || self.minimum_coverage_percent > 100
        {
            return Err(BaselineError::InvalidValue("budget_policy"));
        }
        Ok(())
    }

    fn budget_for(&self, band: BenchmarkBand) -> u64 {
        match band {
            BenchmarkBand::Full => self.full_p95_max_nanos,
            BenchmarkBand::IncrementalCold => self.incremental_cold_p95_max_nanos,
            BenchmarkBand::IncrementalWarm => self.incremental_warm_p95_max_nanos,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum EvidenceStatus {
    Pass,
    Inconclusive,
}

impl EvidenceStatus {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Inconclusive => "INCONCLUSIVE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkSliceReport {
    pub scale: FixtureScale,
    pub band: BenchmarkBand,
    pub case_count: usize,
    pub incomplete_count: usize,
    pub hit_count: usize,
    pub zero_hit_count: usize,
    pub fallback_count: usize,
    pub security_mismatch_count: usize,
    pub coverage_percent: u8,
    pub p50_nanos: u64,
    pub p95_nanos: u64,
    pub budget_nanos: u64,
    pub budget_within_limit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PerformanceReport {
    pub version: String,
    pub status: EvidenceStatus,
    pub case_count: usize,
    pub repository_count: usize,
    pub incomplete_case_count: usize,
    pub slices: Vec<BenchmarkSliceReport>,
    pub fallback_reason_counts: BTreeMap<String, usize>,
    pub security_mismatch_count: usize,
    pub missing_security_comparison_count: usize,
    pub fault_injection_count: usize,
    pub diagnostics: Vec<String>,
    pub observed_fixture_timing_count: usize,
    pub observed_fixture_timing_slices: Vec<String>,
    pub fixture_timing_digest: Option<String>,
    pub report_digest: String,
}

impl PerformanceReport {
    pub fn evaluate(cases: &[BenchmarkCase], policy: &BudgetPolicy) -> Result<Self, BaselineError> {
        policy.validate()?;
        if cases.is_empty() {
            return Err(BaselineError::InvalidValue("benchmark_cases"));
        }

        let mut ids = BTreeSet::new();
        let mut repositories = BTreeSet::new();
        let mut grouped: BTreeMap<(FixtureScale, BenchmarkBand), Vec<&BenchmarkCase>> = BTreeMap::new();
        let mut fallback_counts_by_slice = BTreeMap::<(FixtureScale, BenchmarkBand), usize>::new();
        let mut fallback_reason_counts = BTreeMap::new();
        let mut security_mismatch_count = 0;
        let mut missing_security_comparison_count = 0;
        let mut fault_injection_count = 0;
        let incomplete_case_count = cases.iter().filter(|case| !case.coverage.complete).count();

        for case in cases {
            if !ids.insert(case.case_id.clone()) {
                return Err(BaselineError::DuplicateCase(case.case_id.clone()));
            }
            repositories.insert(case.repository_digest.clone());
            if case.fallback.is_fallback() {
                *fallback_counts_by_slice.entry((case.scale, case.band())).or_insert(0) += 1;
            }
            if !is_fault_injection(case) {
                grouped.entry((case.scale, case.band())).or_default().push(case);
            }
            for reason in &case.fallback.reasons {
                *fallback_reason_counts.entry(reason.code().to_string()).or_insert(0) += 1;
            }
            if case.route == ExecutionRoute::Incremental {
                match case.security_comparison.as_ref() {
                    Some(comparison) if !comparison.equivalent() => security_mismatch_count += 1,
                    Some(_) => {}
                    None => missing_security_comparison_count += 1,
                }
            }
            if is_fault_injection(case) {
                fault_injection_count += 1;
            }
        }

        let mut slices = Vec::new();
        let mut diagnostics = Vec::new();
        for scale in FixtureScale::ALL {
            for band in BenchmarkBand::ALL {
                let entries = grouped.get(&(scale, band)).cloned().unwrap_or_default();
                if entries.is_empty() {
                    diagnostics.push(format!("missing-fixture:{}:{}", scale.code(), band.code()));
                    continue;
                }
                let mut durations = entries.iter().map(|case| case.total_elapsed_nanos()).collect::<Vec<_>>();
                durations.sort_unstable();
                let p50_nanos = percentile(&durations, 50);
                let p95_nanos = percentile(&durations, 95);
                let planned = entries.iter().map(|case| case.coverage.planned_units).sum::<usize>();
                let accounted = entries.iter().map(|case| case.coverage.accounted_units).sum::<usize>();
                let coverage_percent = if planned == 0 {
                    0
                } else {
                    ((accounted.saturating_mul(100)) / planned).min(100) as u8
                };
                let hit_count = entries.iter().filter(|case| case.rule_match == RuleMatch::Hit).count();
                let zero_hit_count = entries.iter().filter(|case| case.rule_match == RuleMatch::ZeroHit).count();
                let fallback_count = fallback_counts_by_slice.get(&(scale, band)).copied().unwrap_or(0);
                let incomplete_count = entries.iter().filter(|case| !case.coverage.complete).count();
                let budget_nanos = policy.budget_for(band);
                let budget_within_limit = p95_nanos <= budget_nanos
                    && coverage_percent >= policy.minimum_coverage_percent
                    && incomplete_count == 0;
                if !budget_within_limit {
                    diagnostics.push(format!("budget-or-coverage:{}:{}", scale.code(), band.code()));
                }
                slices.push(BenchmarkSliceReport {
                    scale,
                    band,
                    case_count: entries.len(),
                    incomplete_count,
                    hit_count,
                    zero_hit_count,
                    fallback_count,
                    security_mismatch_count: entries
                        .iter()
                        .filter(|case| case.security_equivalent() == Some(false))
                        .count(),
                    coverage_percent,
                    p50_nanos,
                    p95_nanos,
                    budget_nanos,
                    budget_within_limit,
                });
            }
        }

        if security_mismatch_count > 0 {
            diagnostics.push("incremental-vs-full-security-mismatch: no-authoritative-pass".into());
        }
        if missing_security_comparison_count > 0 {
            diagnostics.push("incremental-vs-full-comparison-missing: no-authoritative-pass".into());
        }
        if fault_injection_count > 0 {
            diagnostics.push(format!("fault-injection-cases:{}: diagnostic-only", fault_injection_count));
        }
        if incomplete_case_count > 0 {
            diagnostics.push(format!("incomplete-coverage-cases:{}: no-authoritative-pass", incomplete_case_count));
        }
        for scale in FixtureScale::ALL {
            if !slices.iter().any(|slice| slice.scale == scale) {
                diagnostics.push(format!("missing-scale:{}", scale.code()));
            }
        }
        let status = if diagnostics.is_empty() {
            EvidenceStatus::Pass
        } else {
            EvidenceStatus::Inconclusive
        };
        diagnostics.sort();
        diagnostics.dedup();
        let mut report = Self {
            version: INCREMENTAL_BASELINE_VERSION.into(),
            status,
            case_count: cases.len(),
            repository_count: repositories.len(),
            incomplete_case_count,
            slices,
            fallback_reason_counts,
            security_mismatch_count,
            missing_security_comparison_count,
            fault_injection_count,
            diagnostics,
            observed_fixture_timing_count: 0,
            observed_fixture_timing_slices: Vec::new(),
            fixture_timing_digest: None,
            report_digest: String::new(),
        };
        report.report_digest = report.compute_digest()?;
        Ok(report)
    }

    pub fn allows_authoritative_pass(&self) -> bool {
        self.validate_integrity().is_ok()
            && self.status == EvidenceStatus::Pass
            && self.security_mismatch_count == 0
            && self.missing_security_comparison_count == 0
            && self.incomplete_case_count == 0
            && self.observed_fixture_timing_count == FixtureScale::ALL.len() * BenchmarkBand::ALL.len() * 3
            && self.observed_fixture_timing_slices == expected_fixture_timing_slices()
            && self.fixture_timing_digest.is_some()
            && self.slices.len() == FixtureScale::ALL.len() * BenchmarkBand::ALL.len()
            && self.slices.iter().all(|slice| slice.budget_within_limit)
    }

    pub fn validate_integrity(&self) -> Result<(), BaselineError> {
        validate_digest("report_digest", &self.report_digest)?;
        if self.version != INCREMENTAL_BASELINE_VERSION {
            return Err(BaselineError::InvalidValue("report_version"));
        }
        if self.observed_fixture_timing_count == 0 {
            if !self.observed_fixture_timing_slices.is_empty() || self.fixture_timing_digest.is_some() {
                return Err(BaselineError::InvalidValue("fixture_timing_provenance"));
            }
        } else {
            if self.observed_fixture_timing_count != FixtureScale::ALL.len() * BenchmarkBand::ALL.len() * 3
                || self.observed_fixture_timing_slices != expected_fixture_timing_slices()
            {
                return Err(BaselineError::InvalidValue("fixture_timing_provenance"));
            }
            validate_digest("fixture_timing_digest", self.fixture_timing_digest.as_deref().ok_or(BaselineError::InvalidValue("fixture_timing_digest"))?)?;
        }
        let expected_digest = self.compute_digest()?;
        if expected_digest != self.report_digest {
            return Err(BaselineError::DigestMismatch("report_digest"));
        }
        Ok(())
    }

    pub fn attach_fixture_timing(
        &mut self,
        samples: &[FixtureTimingSample],
    ) -> Result<(), BaselineError> {
        let mut counts = BTreeMap::<String, usize>::new();
        let mut identities = BTreeSet::new();
        for sample in samples {
            FixtureTimingSample::new(sample.scale, sample.band, sample.repetition, sample.elapsed_nanos)?;
            let key = format!("{}/{}", sample.scale.code(), sample.band.code());
            let count = counts.entry(key.clone()).or_insert(0);
            *count += 1;
            if !identities.insert((sample.scale, sample.band, sample.repetition)) {
                return Err(BaselineError::DuplicateTimingSlice(key));
            }
        }
        let expected = expected_fixture_timing_slices();
        if samples.len() != expected.len() * 3 || counts.len() != expected.len() || counts.values().any(|count| *count != 3) {
            return Err(BaselineError::InvalidValue("fixture_timing_slices"));
        }
        let observed_fixture_timing_slices = counts.keys().cloned().collect::<Vec<_>>();
        if observed_fixture_timing_slices != expected {
            return Err(BaselineError::InvalidValue("fixture_timing_slices"));
        }
        let timing_digest = fixture_timing_digest(samples);
        self.observed_fixture_timing_count = samples.len();
        self.observed_fixture_timing_slices = observed_fixture_timing_slices;
        self.fixture_timing_digest = Some(timing_digest);
        self.report_digest = self.compute_digest()?;
        Ok(())
    }

    pub fn to_markdown(&self, policy: &BudgetPolicy) -> String {
        let mut output = String::new();
        output.push_str("# Incremental Scan Performance Baseline\n\n");
        output.push_str(&format!("- Version: `{}`\n", self.version));
        output.push_str(&format!("- Status: `{}`\n", self.status.code()));
        output.push_str(&format!("- Synthetic repositories: `{}`\n", self.repository_count));
        output.push_str(&format!("- Cases: `{}`\n", self.case_count));
        output.push_str(&format!("- Observed fixture timing samples: `{}`\n", self.observed_fixture_timing_count));
        output.push_str(&format!("- Observed fixture timing slices: `{}`\n", self.observed_fixture_timing_slices.join(", ")));
        if let Some(digest) = &self.fixture_timing_digest {
            output.push_str(&format!("- Fixture timing digest: `{digest}`\n"));
        }
        output.push_str("- Disclosure: digest-only stage inputs; no source, path, credentials or external SCM data.\n\n");
        output.push_str("| Scale | Band | Cases | Incomplete | Rule hit | Zero hit | Fallbacks | Coverage | P50 (ns) | P95 (ns) | Budget |\n");
        output.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
        for slice in &self.slices {
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {}% | {} | {} | {} |\n",
                slice.scale.code(),
                slice.band.code(),
                slice.case_count,
                slice.incomplete_count,
                slice.hit_count,
                slice.zero_hit_count,
                slice.fallback_count,
                slice.coverage_percent,
                slice.p50_nanos,
                slice.p95_nanos,
                if slice.budget_within_limit { "PASS" } else { "INCONCLUSIVE" },
            ));
        }
        output.push_str("\n## Fallback reasons\n\n");
        for reason in [
            FallbackReason::BuildConfigurationChanged,
            FallbackReason::RuleBundleChanged,
            FallbackReason::PolicyChanged,
            FallbackReason::ResolverChanged,
            FallbackReason::DependencyGraphUncertain,
            FallbackReason::GeneratedSourceChanged,
            FallbackReason::GeneratedSourceUncertain,
            FallbackReason::CacheCorrupt,
            FallbackReason::CacheExpired,
            FallbackReason::CacheIdentityMismatch,
            FallbackReason::CacheIncomplete,
            FallbackReason::MergeBaseUnverifiable,
            FallbackReason::ScopeBudgetExceeded,
            FallbackReason::BuildFailed,
            FallbackReason::PlanNotAuthoritative,
            FallbackReason::PlanIncomplete,
            FallbackReason::PlanWidenedToFull,
        ] {
            output.push_str(&format!("- `{}`: `{}`\n", reason.code(), self.fallback_reason_counts.get(reason.code()).copied().unwrap_or(0)));
        }
        output.push_str("\n## Safety boundary\n\n");
        output.push_str("- A cache anomaly, unverifiable graph/merge-base, changed build input, or scope budget breach widens the requested incremental route to `FULL`.\n");
        output.push_str("- Incremental output is diagnostic fast feedback until a complete full authority result is paired with it.\n");
        output.push_str("- Any incremental/full semantic or completeness digest mismatch is `INCONCLUSIVE`; it can never become an authoritative PASS.\n");
        output.push_str("- Authority guard: `no-authoritative-pass` is emitted for missing or mismatched safety evidence.\n");
        output.push_str("- A budget breach, missing fixture, injected build failure, or cache fault is evidence requiring review, not a security-threshold relaxation.\n");
        output.push_str(&format!("\n- Budget policy: full P95 `{}`, incremental cold P95 `{}`, incremental warm P95 `{}`, minimum coverage `{}`%.\n", policy.full_p95_max_nanos, policy.incremental_cold_p95_max_nanos, policy.incremental_warm_p95_max_nanos, policy.minimum_coverage_percent));
        output.push_str(&format!("- Report digest: `{}`\n", self.report_digest));
        if !self.diagnostics.is_empty() {
            output.push_str("\n## Diagnostics\n\n");
            for diagnostic in &self.diagnostics {
                output.push_str(&format!("- `{diagnostic}`\n"));
            }
        }
        output
    }

    fn compute_digest(&self) -> Result<String, BaselineError> {
        let mut unsigned = self.clone();
        unsigned.report_digest.clear();
        let bytes = serde_json::to_vec(&unsigned).map_err(|error| BaselineError::Serialization(error.to_string()))?;
        Ok(hex_digest(&Sha256::digest(bytes)))
    }
}

impl<'de> Deserialize<'de> for PerformanceReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WirePerformanceReport {
            version: String,
            status: EvidenceStatus,
            case_count: usize,
            repository_count: usize,
            incomplete_case_count: usize,
            slices: Vec<BenchmarkSliceReport>,
            fallback_reason_counts: BTreeMap<String, usize>,
            security_mismatch_count: usize,
            missing_security_comparison_count: usize,
            fault_injection_count: usize,
            diagnostics: Vec<String>,
            observed_fixture_timing_count: usize,
            observed_fixture_timing_slices: Vec<String>,
            fixture_timing_digest: Option<String>,
            report_digest: String,
        }

        let raw = WirePerformanceReport::deserialize(deserializer)?;
        let report = Self {
            version: raw.version,
            status: raw.status,
            case_count: raw.case_count,
            repository_count: raw.repository_count,
            incomplete_case_count: raw.incomplete_case_count,
            slices: raw.slices,
            fallback_reason_counts: raw.fallback_reason_counts,
            security_mismatch_count: raw.security_mismatch_count,
            missing_security_comparison_count: raw.missing_security_comparison_count,
            fault_injection_count: raw.fault_injection_count,
            diagnostics: raw.diagnostics,
            observed_fixture_timing_count: raw.observed_fixture_timing_count,
            observed_fixture_timing_slices: raw.observed_fixture_timing_slices,
            fixture_timing_digest: raw.fixture_timing_digest,
            report_digest: raw.report_digest,
        };
        report.validate_integrity().map_err(de::Error::custom)?;
        Ok(report)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BaselineError {
    #[error("invalid value: {0}")]
    InvalidValue(&'static str),
    #[error("invalid digest: {0}")]
    InvalidDigest(&'static str),
    #[error("plan and completeness report do not match")]
    PlanMismatch,
    #[error("stage is duplicated: {0:?}")]
    DuplicateStage(Stage),
    #[error("benchmark case is duplicated: {0}")]
    DuplicateCase(String),
    #[error("benchmark case route does not match fallback request")]
    RouteMismatch,
    #[error("benchmark case does not contain all required stage measurements")]
    MissingStage,
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("fixture timing slice is duplicated: {0}")]
    DuplicateTimingSlice(String),
    #[error("digest does not match canonical content: {0}")]
    DigestMismatch(&'static str),
}

fn validate_digest(name: &'static str, value: &str) -> Result<(), BaselineError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(BaselineError::InvalidDigest(name));
    }
    Ok(())
}

fn append_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn hex_digest(digest: &[u8]) -> String {
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

/// Returns a one-way input summary suitable for stage telemetry. Callers may
/// pass an operational label or identity; the original value is never stored
/// by this crate.
pub fn digest_summary(value: &str) -> String {
    hex_digest(&Sha256::digest(value.as_bytes()))
}

fn digest_text(value: &str) -> String {
    digest_summary(value)
}

fn percentile(values: &[u64], percentile: u64) -> u64 {
    debug_assert!(!values.is_empty());
    let rank = ((values.len() as u64 * percentile).saturating_add(99) / 100).max(1) as usize;
    values[rank.min(values.len()) - 1]
}

fn expected_fixture_timing_slices() -> Vec<String> {
    FixtureScale::ALL
        .into_iter()
        .flat_map(|scale| BenchmarkBand::ALL.into_iter().map(move |band| format!("{}/{}", scale.code(), band.code())))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn fixture_timing_digest(samples: &[FixtureTimingSample]) -> String {
    let mut canonical = samples.to_vec();
    canonical.sort_by_key(|sample| (sample.scale, sample.band, sample.repetition));
    let mut bytes = INCREMENTAL_BASELINE_VERSION.as_bytes().to_vec();
    bytes.extend_from_slice(b"/fixture-timing\0");
    for sample in canonical {
        append_string(&mut bytes, sample.scale.code());
        append_string(&mut bytes, sample.band.code());
        bytes.push(sample.repetition);
        bytes.extend_from_slice(&sample.elapsed_nanos.to_be_bytes());
    }
    hex_digest(&Sha256::digest(bytes))
}

/// Deterministic local fixtures used by the acceptance script and tests.
/// Names intentionally describe scale only; no external repository identity is
/// inferred from them and all persisted identifiers are digests.
pub fn synthetic_fixture_matrix() -> Result<Vec<BenchmarkCase>, BaselineError> {
    let mut cases = Vec::new();
    for (index, scale) in FixtureScale::ALL.into_iter().enumerate() {
        let repository_digest = digest_text(&format!("synthetic-java-repository-{index}"));
        let plan = PlanObservation::new(digest_text(&format!("plan-{index}")), PlanMode::Module, PlanAuthority::GateEligible, 100, true)?;
        let full_fallback = assess_fallback(FallbackInputs {
            requested_route: ExecutionRoute::Full,
            plan: plan.clone(),
            cache: CacheObservation::NotUsed,
            build: BuildOutcome::Success,
            build_configuration_changed: false,
            rule_bundle_changed: false,
            policy_changed: false,
            resolver_changed: false,
            dependency_graph: DependencyGraphState::Proven,
            generated_source: GeneratedSourceState::Stable,
            merge_base: MergeBaseState::Verifiable,
            affected_unit_count: 20,
            scope_budget: 100,
        })?;
        for repetition in 0..3 {
            cases.push(BenchmarkCase::new(
                format!("{index}-full-{repetition}"),
                repository_digest.clone(),
                scale,
                ExecutionRoute::Full,
                CacheObservation::NotUsed,
                CacheObservation::NotUsed,
                RuleMatch::Hit,
                BuildOutcome::Success,
                full_fallback.clone(),
                fixture_telemetry(index as u64 + 10 + repetition),
                CoverageObservation::new(100, 100, true)?,
                None,
            )?);
            for (warm, rule_match) in [(false, RuleMatch::Hit), (true, RuleMatch::ZeroHit)] {
                let cache = if warm { CacheObservation::WarmHit } else { CacheObservation::ColdMiss };
                let fallback = assess_fallback(FallbackInputs {
                    requested_route: ExecutionRoute::Incremental,
                    plan: plan.clone(),
                    cache,
                    build: BuildOutcome::Success,
                    build_configuration_changed: false,
                    rule_bundle_changed: false,
                    policy_changed: false,
                    resolver_changed: false,
                    dependency_graph: DependencyGraphState::Proven,
                    generated_source: GeneratedSourceState::Stable,
                    merge_base: MergeBaseState::Verifiable,
                    affected_unit_count: 20 + index,
                    scope_budget: 100,
                })?;
                let semantic = digest_text("equivalent-semantic-result");
                let completeness = digest_text("equivalent-completeness");
            cases.push(BenchmarkCase::new(
                format!("{index}-incremental-{}-{repetition}", if warm { "warm-zero-hit" } else { "cold-hit" }),
                repository_digest.clone(),
                scale,
                ExecutionRoute::Incremental,
                cache,
                cache,
                rule_match,
                BuildOutcome::Success,
                fallback,
                fixture_telemetry(index as u64 + if warm { 4 } else { 7 }),
                CoverageObservation::new(100, 100, true)?,
                Some(SecurityComparison::new(semantic.clone(), semantic, completeness.clone(), completeness)?),
            )?);
            }
        }
    }

    let base_plan = PlanObservation::new(digest_text("injection-plan"), PlanMode::Module, PlanAuthority::GateEligible, 100, true)?;
    let injection_specs = [
        ("build-config", FallbackReason::BuildConfigurationChanged),
        ("rule-bundle", FallbackReason::RuleBundleChanged),
        ("policy", FallbackReason::PolicyChanged),
        ("resolver", FallbackReason::ResolverChanged),
        ("dependency-graph", FallbackReason::DependencyGraphUncertain),
        ("generated-source", FallbackReason::GeneratedSourceChanged),
        ("generated-source-uncertain", FallbackReason::GeneratedSourceUncertain),
        ("cache-corrupt", FallbackReason::CacheCorrupt),
        ("cache-expired", FallbackReason::CacheExpired),
        ("cache-identity", FallbackReason::CacheIdentityMismatch),
        ("cache-incomplete", FallbackReason::CacheIncomplete),
        ("merge-base", FallbackReason::MergeBaseUnverifiable),
        ("scope-budget", FallbackReason::ScopeBudgetExceeded),
        ("build-failed", FallbackReason::BuildFailed),
        ("plan-authority", FallbackReason::PlanNotAuthoritative),
        ("plan-incomplete", FallbackReason::PlanIncomplete),
        ("plan-widened", FallbackReason::PlanWidenedToFull),
    ];
    for (index, (label, reason)) in injection_specs.into_iter().enumerate() {
        let mut inputs = FallbackInputs {
            requested_route: ExecutionRoute::Incremental,
            plan: base_plan.clone(),
            cache: CacheObservation::ColdMiss,
            build: BuildOutcome::Success,
            build_configuration_changed: false,
            rule_bundle_changed: false,
            policy_changed: false,
            resolver_changed: false,
            dependency_graph: DependencyGraphState::Proven,
            generated_source: GeneratedSourceState::Stable,
            merge_base: MergeBaseState::Verifiable,
            affected_unit_count: 20,
            scope_budget: 100,
        };
        match reason {
            FallbackReason::BuildConfigurationChanged => inputs.build_configuration_changed = true,
            FallbackReason::RuleBundleChanged => inputs.rule_bundle_changed = true,
            FallbackReason::PolicyChanged => inputs.policy_changed = true,
            FallbackReason::ResolverChanged => inputs.resolver_changed = true,
            FallbackReason::DependencyGraphUncertain => inputs.dependency_graph = DependencyGraphState::Uncertain,
            FallbackReason::GeneratedSourceChanged => inputs.generated_source = GeneratedSourceState::Changed,
            FallbackReason::GeneratedSourceUncertain => inputs.generated_source = GeneratedSourceState::Uncertain,
            FallbackReason::CacheCorrupt => inputs.cache = CacheObservation::Corrupt,
            FallbackReason::CacheExpired => inputs.cache = CacheObservation::Expired,
            FallbackReason::CacheIdentityMismatch => inputs.cache = CacheObservation::IdentityMismatch,
            FallbackReason::CacheIncomplete => inputs.cache = CacheObservation::Incomplete,
            FallbackReason::MergeBaseUnverifiable => inputs.merge_base = MergeBaseState::Unverifiable,
            FallbackReason::ScopeBudgetExceeded => inputs.affected_unit_count = 101,
            FallbackReason::BuildFailed => inputs.build = BuildOutcome::Failed,
            FallbackReason::PlanNotAuthoritative => inputs.plan.authority = PlanAuthority::PreviewOnly,
            FallbackReason::PlanIncomplete => inputs.plan.complete = false,
            FallbackReason::PlanWidenedToFull => inputs.plan.mode = PlanMode::Full,
        }
        let fallback = assess_fallback(inputs)?;
        let semantic = digest_text("injection-semantic-result");
        let completeness = digest_text("injection-completeness");
        cases.push(BenchmarkCase::new(
            format!("injection-{label}"),
            digest_text(&format!("synthetic-injection-repository-{index}")),
            FixtureScale::Large,
            ExecutionRoute::Incremental,
            CacheObservation::ColdMiss,
            if matches!(reason, FallbackReason::CacheCorrupt) {
                CacheObservation::Corrupt
            } else if matches!(reason, FallbackReason::CacheExpired) {
                CacheObservation::Expired
            } else if matches!(reason, FallbackReason::CacheIdentityMismatch) {
                CacheObservation::IdentityMismatch
            } else if matches!(reason, FallbackReason::CacheIncomplete) {
                CacheObservation::Incomplete
            } else {
                CacheObservation::ColdMiss
            },
            if index % 2 == 0 { RuleMatch::Hit } else { RuleMatch::ZeroHit },
            if reason == FallbackReason::BuildFailed { BuildOutcome::Failed } else { BuildOutcome::Success },
            fallback,
            fixture_telemetry(30 + index as u64),
            CoverageObservation::new(100, if reason == FallbackReason::BuildFailed { 0 } else { 100 }, reason != FallbackReason::BuildFailed)?,
            Some(SecurityComparison::new(semantic.clone(), semantic, completeness.clone(), completeness)?),
        )?);
    }
    Ok(cases)
}

pub fn default_budget_policy() -> BudgetPolicy {
    BudgetPolicy {
        full_p95_max_nanos: 100_000,
        incremental_cold_p95_max_nanos: 100_000,
        incremental_warm_p95_max_nanos: 100_000,
        minimum_coverage_percent: 100,
    }
}

fn fixture_telemetry(seed: u64) -> StageTelemetry {
    let mut measurements = Vec::new();
    for (index, stage) in Stage::ALL.into_iter().enumerate() {
        let input_digest = digest_text(&format!("stage-input-{seed}-{}", stage.code()));
        let elapsed_nanos = seed.saturating_mul(10).saturating_add((index as u64 + 1) * 3);
        measurements.push(StageMeasurement { stage, elapsed_nanos, input_digest, outcome: StageOutcome::Completed });
    }
    StageTelemetry::from_measurements(measurements).expect("synthetic fixture stage telemetry is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(authority: PlanAuthority, mode: PlanMode, complete: bool) -> PlanObservation {
        PlanObservation::new(digest_text("test-plan"), mode, authority, 10, complete).unwrap()
    }

    fn stable_inputs() -> FallbackInputs {
        FallbackInputs {
            requested_route: ExecutionRoute::Incremental,
            plan: plan(PlanAuthority::GateEligible, PlanMode::Module, true),
            cache: CacheObservation::WarmHit,
            build: BuildOutcome::Success,
            build_configuration_changed: false,
            rule_bundle_changed: false,
            policy_changed: false,
            resolver_changed: false,
            dependency_graph: DependencyGraphState::Proven,
            generated_source: GeneratedSourceState::Stable,
            merge_base: MergeBaseState::Verifiable,
            affected_unit_count: 3,
            scope_budget: 10,
        }
    }

    #[test]
    fn every_safety_input_has_a_structured_full_fallback_reason() {
        let reasons = [
            FallbackReason::BuildConfigurationChanged,
            FallbackReason::RuleBundleChanged,
            FallbackReason::PolicyChanged,
            FallbackReason::ResolverChanged,
            FallbackReason::DependencyGraphUncertain,
            FallbackReason::GeneratedSourceChanged,
            FallbackReason::GeneratedSourceUncertain,
            FallbackReason::CacheCorrupt,
            FallbackReason::CacheExpired,
            FallbackReason::CacheIdentityMismatch,
            FallbackReason::CacheIncomplete,
            FallbackReason::MergeBaseUnverifiable,
            FallbackReason::ScopeBudgetExceeded,
            FallbackReason::BuildFailed,
            FallbackReason::PlanNotAuthoritative,
            FallbackReason::PlanIncomplete,
            FallbackReason::PlanWidenedToFull,
        ];
        for expected in reasons {
            let mut inputs = stable_inputs();
            match expected {
                FallbackReason::BuildConfigurationChanged => inputs.build_configuration_changed = true,
                FallbackReason::RuleBundleChanged => inputs.rule_bundle_changed = true,
                FallbackReason::PolicyChanged => inputs.policy_changed = true,
                FallbackReason::ResolverChanged => inputs.resolver_changed = true,
                FallbackReason::DependencyGraphUncertain => inputs.dependency_graph = DependencyGraphState::Uncertain,
                FallbackReason::GeneratedSourceChanged => inputs.generated_source = GeneratedSourceState::Changed,
                FallbackReason::GeneratedSourceUncertain => inputs.generated_source = GeneratedSourceState::Uncertain,
                FallbackReason::CacheCorrupt => inputs.cache = CacheObservation::Corrupt,
                FallbackReason::CacheExpired => inputs.cache = CacheObservation::Expired,
                FallbackReason::CacheIdentityMismatch => inputs.cache = CacheObservation::IdentityMismatch,
                FallbackReason::CacheIncomplete => inputs.cache = CacheObservation::Incomplete,
                FallbackReason::MergeBaseUnverifiable => inputs.merge_base = MergeBaseState::Unverifiable,
                FallbackReason::ScopeBudgetExceeded => inputs.affected_unit_count = 11,
                FallbackReason::BuildFailed => inputs.build = BuildOutcome::Failed,
                FallbackReason::PlanNotAuthoritative => inputs.plan.authority = PlanAuthority::PreviewOnly,
                FallbackReason::PlanIncomplete => inputs.plan.complete = false,
                FallbackReason::PlanWidenedToFull => inputs.plan.mode = PlanMode::Full,
            }
            let decision = assess_fallback(inputs).unwrap();
            assert!(decision.is_fallback(), "{}", expected.code());
            assert!(decision.has_reason(expected), "{}", expected.code());
            assert_eq!(decision.effective_route, ExecutionRoute::Full);
        }
    }

    #[test]
    fn cold_miss_and_warm_hit_are_not_safety_fallbacks() {
        for cache in [CacheObservation::ColdMiss, CacheObservation::WarmHit] {
            let mut inputs = stable_inputs();
            inputs.cache = cache;
            let decision = assess_fallback(inputs).unwrap();
            assert!(!decision.is_fallback());
            assert!(decision.fast_feedback);
            assert!(!decision.allows_authoritative_gate());
            assert!(decision.reasons.is_empty());
        }
    }

    #[test]
    fn stage_timer_uses_monotonic_elapsed_duration_and_digest_only_input() {
        let digest = digest_text("resolver-input");
        let timer = StageTimer::start(Stage::Resolve, digest.clone()).unwrap();
        let measurement = timer.finish();
        assert_eq!(measurement.stage, Stage::Resolve);
        assert_eq!(measurement.input_digest, digest);
        let elapsed_nanos = measurement.elapsed_nanos;
        let mut telemetry = StageTelemetry::new();
        telemetry.record(measurement).unwrap();
        assert_eq!(telemetry.total_elapsed_nanos(), elapsed_nanos);
    }

    #[test]
    fn fixture_matrix_covers_three_scales_cold_warm_hit_zero_hit_and_faults() {
        let cases = synthetic_fixture_matrix().unwrap();
        let scales = cases.iter().map(|case| case.scale).collect::<BTreeSet<_>>();
        assert_eq!(scales.len(), 3);
        assert!(cases.iter().any(|case| case.cache == CacheObservation::ColdMiss));
        assert!(cases.iter().any(|case| case.cache == CacheObservation::WarmHit));
        assert!(cases.iter().any(|case| case.rule_match == RuleMatch::Hit));
        assert!(cases.iter().any(|case| case.rule_match == RuleMatch::ZeroHit));
        assert!(cases.iter().any(|case| case.build == BuildOutcome::Failed));
        assert!(cases.iter().any(|case| case.cache == CacheObservation::Corrupt));
        for reason in [
            FallbackReason::BuildConfigurationChanged,
            FallbackReason::RuleBundleChanged,
            FallbackReason::PolicyChanged,
            FallbackReason::ResolverChanged,
            FallbackReason::DependencyGraphUncertain,
            FallbackReason::GeneratedSourceChanged,
            FallbackReason::GeneratedSourceUncertain,
            FallbackReason::CacheCorrupt,
            FallbackReason::CacheExpired,
            FallbackReason::CacheIdentityMismatch,
            FallbackReason::CacheIncomplete,
            FallbackReason::MergeBaseUnverifiable,
            FallbackReason::ScopeBudgetExceeded,
            FallbackReason::BuildFailed,
            FallbackReason::PlanNotAuthoritative,
            FallbackReason::PlanIncomplete,
            FallbackReason::PlanWidenedToFull,
        ] {
            assert!(cases.iter().any(|case| case.fallback.has_reason(reason)), "{}", reason.code());
        }
    }

    #[test]
    fn report_has_deterministic_percentiles_and_marks_fault_evidence_inconclusive() {
        let cases = synthetic_fixture_matrix().unwrap();
        let report = PerformanceReport::evaluate(&cases, &default_budget_policy()).unwrap();
        let second = PerformanceReport::evaluate(&cases, &default_budget_policy()).unwrap();
        assert_eq!(report, second);
        assert_eq!(report.repository_count, 20);
        assert_eq!(report.status, EvidenceStatus::Inconclusive);
        assert!(!report.allows_authoritative_pass());
        assert_eq!(report.slices.len(), 9);
        let large_cold = report.slices.iter().find(|slice| slice.scale == FixtureScale::Large && slice.band == BenchmarkBand::IncrementalCold).unwrap();
        assert_eq!(large_cold.case_count, 3);
        assert_eq!(large_cold.fallback_count, 17);
        assert!(report.incomplete_case_count > 0);
        assert!(report.slices.iter().all(|slice| slice.p50_nanos <= slice.p95_nanos));
        assert_eq!(report.fallback_reason_counts.len(), 17);
        assert!(report.report_digest.len() == 64);
    }

    #[test]
    fn fixture_timing_provenance_requires_unique_complete_three_by_three_slices() {
        let mut report = PerformanceReport::evaluate(&synthetic_fixture_matrix().unwrap(), &default_budget_policy()).unwrap();
        let incomplete = [FixtureTimingSample::new(FixtureScale::Small, BenchmarkBand::Full, 0, 1).unwrap()];
        assert!(report.attach_fixture_timing(&incomplete).is_err());
        let invalid = [FixtureTimingSample { scale: FixtureScale::Small, band: BenchmarkBand::Full, repetition: 0, elapsed_nanos: 0 }];
        assert!(report.attach_fixture_timing(&invalid).is_err());
        let mut slices = Vec::new();
        for scale in FixtureScale::ALL {
            for band in BenchmarkBand::ALL {
                for _ in 0..3 {
                    let repetition = slices.iter().filter(|sample: &&FixtureTimingSample| sample.scale == scale && sample.band == band).count() as u8;
                    slices.push(FixtureTimingSample::new(scale, band, repetition, 1).unwrap());
                }
            }
        }
        report.attach_fixture_timing(&slices).unwrap();
        assert_eq!(report.observed_fixture_timing_count, 27);
        assert_eq!(report.observed_fixture_timing_slices, expected_fixture_timing_slices());
        assert!(report.fixture_timing_digest.is_some());
    }

    #[test]
    fn security_mismatch_can_never_be_authoritative_pass() {
        let mut cases = synthetic_fixture_matrix().unwrap();
        let case = cases.iter_mut().find(|case| case.route == ExecutionRoute::Incremental).unwrap();
        let comparison = case.security_comparison.as_mut().unwrap();
        comparison.incremental_semantic_result_digest = digest_text("different");
        let report = PerformanceReport::evaluate(&cases, &default_budget_policy()).unwrap();
        assert_eq!(report.security_mismatch_count, 1);
        assert_eq!(report.status, EvidenceStatus::Inconclusive);
        assert!(!report.allows_authoritative_pass());
    }

    #[test]
    fn budget_breach_is_inconclusive_not_a_security_relaxation() {
        let cases = synthetic_fixture_matrix().unwrap();
        let policy = BudgetPolicy {
            full_p95_max_nanos: 1,
            ..default_budget_policy()
        };
        let report = PerformanceReport::evaluate(&cases, &policy).unwrap();
        assert_eq!(report.status, EvidenceStatus::Inconclusive);
        assert!(report.diagnostics.iter().any(|diagnostic| diagnostic.starts_with("budget-or-coverage:SMALL:FULL")));
        let invalid = r#"{"planned_units": 10, "accounted_units": 10, "complete": false}"#;
        let decoded = serde_json::from_str::<CoverageObservation>(invalid).unwrap();
        assert!(!decoded.is_complete());
        let invalid_complete = r#"{"planned_units": 10, "accounted_units": 9, "complete": true}"#;
        assert!(serde_json::from_str::<CoverageObservation>(invalid_complete).is_err());
    }
}
