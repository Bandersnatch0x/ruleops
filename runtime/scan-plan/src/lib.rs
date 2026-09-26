//! Frozen incremental ScanPlan and result-cache boundary.
//!
//! This module consumes the immutable `AffectedScope` produced by the Runner
//! scope seam. It owns plan selection, explicit preview coverage, authority
//! eligibility, deterministic identities and a digest-addressed cache of
//! result summaries. It does not parse Git, execute build logic, upload
//! source, or grant SCM/Gate authority.

use erf_affected_scope::{AffectedScope, AnalysisKind, ScopeLevel};
use rf_gate::Completeness;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const SCAN_PLAN_VERSION: &str = "scan-plan/v1";
pub const CACHE_RECORD_VERSION: &str = "scan-result-cache/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum PlanMode {
    Full,
    Module,
    IncrementalPreview,
}

impl PlanMode {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Full => "FULL",
            Self::Module => "MODULE",
            Self::IncrementalPreview => "INCREMENTAL_PREVIEW",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum PlanAuthority {
    GateEligible,
    PreviewOnly,
}

impl PlanAuthority {
    pub const fn code(self) -> &'static str {
        match self {
            Self::GateEligible => "GATE_ELIGIBLE",
            Self::PreviewOnly => "PREVIEW_ONLY",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum PlanReasonCode {
    WidenedToFull,
    WidenedToModule,
    PreviewIsNotAuthoritative,
    UnevaluatedScope,
    IncompleteExecution,
    RequiredUnitMissing,
    UnsupportedUnit,
    NotApplicableUnit,
}

impl PlanReasonCode {
    pub const fn code(self) -> &'static str {
        match self {
            Self::WidenedToFull => "widened-to-full",
            Self::WidenedToModule => "widened-to-module",
            Self::PreviewIsNotAuthoritative => "preview-not-authoritative",
            Self::UnevaluatedScope => "unevaluated-scope",
            Self::IncompleteExecution => "incomplete-execution",
            Self::RequiredUnitMissing => "required-unit-missing",
            Self::UnsupportedUnit => "unsupported-unit",
            Self::NotApplicableUnit => "not-applicable-unit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct PlanReason {
    pub code: PlanReasonCode,
    pub detail: String,
}

impl PlanReason {
    fn new(code: PlanReasonCode, detail: impl Into<String>) -> Self {
        Self { code, detail: detail.into() }
    }
}

/// Inputs that are fixed before plan creation. The fields are intentionally
/// explicit rather than a free-form map so a caller cannot accidentally omit
/// an identity that participates in cache invalidation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlanIdentity {
    pub repository: String,
    pub base: String,
    pub head: String,
    pub tree: String,
    pub scope_digest: String,
    pub rule_bundle_digest: String,
    pub policy_digest: String,
    pub resolver_identity: String,
    pub backend_identity: String,
    pub runner_identity: String,
    pub protocol_major: u32,
    pub canonical_set_version: String,
}

impl PlanIdentity {
    pub fn validate(&self) -> Result<(), PlanError> {
        for (name, value) in [
            ("repository", &self.repository),
            ("base", &self.base),
            ("head", &self.head),
            ("tree", &self.tree),
            ("resolver_identity", &self.resolver_identity),
            ("backend_identity", &self.backend_identity),
            ("runner_identity", &self.runner_identity),
            ("canonical_set_version", &self.canonical_set_version),
        ] {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(PlanError::InvalidIdentity(name));
            }
        }
        for (name, value) in [
            ("scope_digest", &self.scope_digest),
            ("rule_bundle_digest", &self.rule_bundle_digest),
            ("policy_digest", &self.policy_digest),
        ] {
            if !is_digest(value) {
                return Err(PlanError::InvalidDigest(name));
            }
        }
        Ok(())
    }
}

/// One rule/backend/module or rule/backend/file execution unit. The Rule
/// identity is carried by the opaque `unit_id`; the plan never derives it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct AnalysisUnit {
    pub unit_id: String,
    pub backend: AnalysisKind,
    pub module_id: String,
    pub path: Option<String>,
}

impl AnalysisUnit {
    fn validate(&self) -> Result<(), PlanError> {
        if self.unit_id.trim().is_empty() || self.module_id.trim().is_empty() {
            return Err(PlanError::InvalidUnit(self.unit_id.clone()));
        }
        if self.unit_id.chars().any(char::is_control) || self.module_id.chars().any(char::is_control) {
            return Err(PlanError::InvalidUnit(self.unit_id.clone()));
        }
        if let Some(path) = &self.path {
            if path.is_empty()
                || path.starts_with('/')
                || path.contains('\\')
                || path.split('/').any(|part| part.is_empty() || part == "." || part == "..")
            {
                return Err(PlanError::InvalidUnit(self.unit_id.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ScanPlanRequest {
    pub identity: PlanIdentity,
    pub affected_scope: AffectedScope,
    pub requested_mode: PlanMode,
    pub units: Vec<AnalysisUnit>,
}

/// Immutable plan frozen before backend execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanPlan {
    identity: PlanIdentity,
    requested_mode: PlanMode,
    mode: PlanMode,
    authority: PlanAuthority,
    scope_level: ScopeLevel,
    workspace_graph_digest: String,
    all_units: Vec<AnalysisUnit>,
    planned_units: Vec<AnalysisUnit>,
    excluded_unit_ids: Vec<String>,
    unevaluated_unit_ids: Vec<String>,
    reasons: Vec<PlanReason>,
    digest: String,
}

impl ScanPlan {
    pub fn freeze(request: ScanPlanRequest) -> Result<Self, PlanError> {
        request.identity.validate()?;
        if request.identity.scope_digest != request.affected_scope.digest() {
            return Err(PlanError::ScopeDigestMismatch);
        }
        if request.units.is_empty() {
            return Err(PlanError::NoUnits);
        }

        let mut all_units = request.units;
        for unit in &all_units {
            unit.validate()?;
        }
        all_units.sort();
        if all_units.windows(2).any(|pair| pair[0].unit_id == pair[1].unit_id) {
            return Err(PlanError::DuplicateUnit(all_units.windows(2).find_map(|pair| (pair[0].unit_id == pair[1].unit_id).then(|| pair[0].unit_id.clone())).unwrap_or_default()));
        }

        let mut mode = request.requested_mode;
        let mut reasons = Vec::new();
        if request.affected_scope.classification() == ScopeLevel::Full && mode != PlanMode::Full {
            mode = PlanMode::Full;
            reasons.push(PlanReason::new(PlanReasonCode::WidenedToFull, "AffectedScope contains an unproven full-repository impact"));
        } else if request.requested_mode == PlanMode::Module && request.affected_scope.classification() == ScopeLevel::FileCandidate {
            reasons.push(PlanReason::new(PlanReasonCode::WidenedToModule, "module mode is the minimum safe closure for this request"));
        }
        if mode == PlanMode::IncrementalPreview {
            reasons.push(PlanReason::new(PlanReasonCode::PreviewIsNotAuthoritative, "incremental preview never grants authoritative Gate PASS"));
        }

        let planned_units = all_units
            .iter()
            .filter(|unit| unit_is_selected(unit, &request.affected_scope, mode))
            .cloned()
            .collect::<Vec<_>>();
        if planned_units.is_empty() {
            return Err(PlanError::NoSelectedUnits);
        }
        let planned_ids = planned_units.iter().map(|unit| unit.unit_id.clone()).collect::<BTreeSet<_>>();
        let all_ids = all_units.iter().map(|unit| unit.unit_id.clone()).collect::<BTreeSet<_>>();
        let excluded_unit_ids = all_ids.difference(&planned_ids).cloned().collect::<Vec<_>>();
        let unevaluated_unit_ids = if mode == PlanMode::IncrementalPreview { excluded_unit_ids.clone() } else { Vec::new() };
        let authority = if mode == PlanMode::IncrementalPreview { PlanAuthority::PreviewOnly } else { PlanAuthority::GateEligible };
        let mut plan = Self {
            identity: request.identity,
            requested_mode: request.requested_mode,
            mode,
            authority,
            scope_level: request.affected_scope.classification(),
            workspace_graph_digest: request.affected_scope.workspace_graph_digest().to_string(),
            all_units,
            planned_units,
            excluded_unit_ids,
            unevaluated_unit_ids,
            reasons: normalize_reasons(reasons),
            digest: String::new(),
        };
        plan.digest = hex_digest(&Sha256::digest(plan.canonical_bytes()));
        Ok(plan)
    }

    pub fn identity(&self) -> &PlanIdentity { &self.identity }
    pub fn requested_mode(&self) -> PlanMode { self.requested_mode }
    pub fn mode(&self) -> PlanMode { self.mode }
    pub fn authority(&self) -> PlanAuthority { self.authority }
    pub fn scope_level(&self) -> ScopeLevel { self.scope_level }
    pub fn workspace_graph_digest(&self) -> &str { &self.workspace_graph_digest }
    pub fn all_units(&self) -> &[AnalysisUnit] { &self.all_units }
    pub fn planned_units(&self) -> &[AnalysisUnit] { &self.planned_units }
    pub fn all_unit_ids(&self) -> Vec<String> { self.all_units.iter().map(|unit| unit.unit_id.clone()).collect() }
    pub fn planned_unit_ids(&self) -> Vec<String> { self.planned_units.iter().map(|unit| unit.unit_id.clone()).collect() }
    pub fn excluded_unit_ids(&self) -> &[String] { &self.excluded_unit_ids }
    pub fn unevaluated_unit_ids(&self) -> &[String] { &self.unevaluated_unit_ids }
    pub fn reasons(&self) -> &[PlanReason] { &self.reasons }
    pub fn digest(&self) -> &str { &self.digest }
    pub fn digest_bytes(&self) -> [u8; 32] { Sha256::digest(self.canonical_bytes()).into() }

    /// The runtime adapter uses this before sending units to a Host. It keeps
    /// the Host's request equal to the already-frozen denominator.
    pub fn validate_execution_unit_ids(&self, ids: &[String]) -> Result<(), PlanError> {
        let expected = self.planned_unit_ids();
        if ids != expected {
            return Err(PlanError::ExecutionUnitMismatch { expected, received: ids.to_vec() });
        }
        Ok(())
    }

    pub fn cache_key(&self, result_identity: &ResultIdentity) -> Result<CacheKey, PlanError> {
        result_identity.validate()?;
        let result_identity_digest = result_identity.digest();
        let mut bytes = b"scan-cache-key/v1\0".to_vec();
        append_string(&mut bytes, &self.digest);
        append_string(&mut bytes, &result_identity_digest);
        Ok(CacheKey {
            plan_digest: self.digest.clone(),
            result_identity_digest,
            digest: hex_digest(&Sha256::digest(bytes)),
        })
    }

    pub fn account(&self, accounts: &[UnitAccount], observation_errors: usize) -> Result<CompletenessReport, PlanError> {
        let planned = self.planned_units.iter().map(|unit| unit.unit_id.as_str()).collect::<BTreeSet<_>>();
        let mut seen = BTreeSet::new();
        let mut evaluated = Vec::new();
        let mut failed_units = Vec::new();
        let mut unsupported_units = Vec::new();
        let mut not_applicable_units = Vec::new();
        for account in accounts {
            if !planned.contains(account.unit_id.as_str()) {
                return Err(PlanError::UnknownAccountUnit(account.unit_id.clone()));
            }
            if !seen.insert(account.unit_id.clone()) {
                return Err(PlanError::DuplicateAccount(account.unit_id.clone()));
            }
            evaluated.push(account.unit_id.clone());
            match account.outcome {
                UnitOutcome::Executed => {}
                UnitOutcome::Failed => failed_units.push(account.unit_id.clone()),
                UnitOutcome::Unsupported => unsupported_units.push(account.unit_id.clone()),
                UnitOutcome::NotApplicable => not_applicable_units.push(account.unit_id.clone()),
            }
        }
        evaluated.sort();
        failed_units.sort();
        unsupported_units.sort();
        not_applicable_units.sort();
        let missing_unit_ids = planned.iter().filter(|unit_id| !seen.contains(**unit_id)).map(|unit_id| (*unit_id).to_string()).collect::<Vec<_>>();
        let failed = failed_units.len() + unsupported_units.len() + not_applicable_units.len();
        let gate_observation_errors = observation_errors + unsupported_units.len() + not_applicable_units.len();
        let complete = missing_unit_ids.is_empty()
            && failed == 0
            && gate_observation_errors == 0
            && self.unevaluated_unit_ids.is_empty()
            && self.authority == PlanAuthority::GateEligible;
        let gate = Completeness {
            planned: self.planned_units.len(),
            accounted: evaluated.len(),
            failed,
            missing: missing_unit_ids.len(),
            observation_errors: gate_observation_errors,
            complete,
        };
        let state = if self.planned_units.is_empty() {
            CoverageState::NotApplicable
        } else if !failed_units.is_empty() || !unsupported_units.is_empty() || !not_applicable_units.is_empty() || observation_errors > 0 {
            CoverageState::Failed
        } else if !missing_unit_ids.is_empty() || !self.unevaluated_unit_ids.is_empty() || self.authority == PlanAuthority::PreviewOnly {
            CoverageState::Partial
        } else {
            CoverageState::Complete
        };
        let mut reasons = self.reasons.clone();
        if !missing_unit_ids.is_empty() { reasons.push(PlanReason::new(PlanReasonCode::RequiredUnitMissing, join_ids(&missing_unit_ids))); }
        if !unsupported_units.is_empty() { reasons.push(PlanReason::new(PlanReasonCode::UnsupportedUnit, join_ids(&unsupported_units))); }
        if !not_applicable_units.is_empty() { reasons.push(PlanReason::new(PlanReasonCode::NotApplicableUnit, join_ids(&not_applicable_units))); }
        if observation_errors > 0 { reasons.push(PlanReason::new(PlanReasonCode::IncompleteExecution, format!("observation-errors={observation_errors}"))); }
        if !self.unevaluated_unit_ids.is_empty() { reasons.push(PlanReason::new(PlanReasonCode::UnevaluatedScope, join_ids(&self.unevaluated_unit_ids))); }
        let reasons = normalize_reasons(reasons);
        Ok(CompletenessReport {
            plan_digest: self.digest.clone(),
            mode: self.mode,
            authority: self.authority,
            scope_level: self.scope_level,
            gate,
            state,
            evaluated_unit_ids: evaluated,
            missing_unit_ids,
            unevaluated_unit_ids: self.unevaluated_unit_ids.clone(),
            failed_unit_ids: failed_units,
            unsupported_unit_ids: unsupported_units,
            not_applicable_unit_ids: not_applicable_units,
            reasons,
        })
    }

    fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = SCAN_PLAN_VERSION.as_bytes().to_vec();
        bytes.push(0);
        append_identity(&mut bytes, &self.identity);
        append_string(&mut bytes, self.requested_mode.code());
        append_string(&mut bytes, self.mode.code());
        append_string(&mut bytes, self.authority.code());
        append_string(&mut bytes, self.scope_level.code());
        append_string(&mut bytes, &self.workspace_graph_digest);
        append_units(&mut bytes, &self.all_units);
        append_units(&mut bytes, &self.planned_units);
        append_strings(&mut bytes, &self.excluded_unit_ids);
        append_strings(&mut bytes, &self.unevaluated_unit_ids);
        append_reasons(&mut bytes, &self.reasons);
        bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum UnitOutcome {
    Executed,
    Failed,
    Unsupported,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UnitAccount {
    pub unit_id: String,
    pub outcome: UnitOutcome,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum CoverageState {
    Complete,
    Partial,
    Failed,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletenessReport {
    plan_digest: String,
    mode: PlanMode,
    authority: PlanAuthority,
    scope_level: ScopeLevel,
    gate: Completeness,
    state: CoverageState,
    evaluated_unit_ids: Vec<String>,
    missing_unit_ids: Vec<String>,
    unevaluated_unit_ids: Vec<String>,
    failed_unit_ids: Vec<String>,
    unsupported_unit_ids: Vec<String>,
    not_applicable_unit_ids: Vec<String>,
    reasons: Vec<PlanReason>,
}

impl CompletenessReport {
    pub fn plan_digest(&self) -> &str { &self.plan_digest }
    pub fn mode(&self) -> PlanMode { self.mode }
    pub fn authority(&self) -> PlanAuthority { self.authority }
    pub fn scope_level(&self) -> ScopeLevel { self.scope_level }
    pub fn gate(&self) -> &Completeness { &self.gate }
    pub fn state(&self) -> CoverageState { self.state }
    pub fn evaluated_unit_ids(&self) -> &[String] { &self.evaluated_unit_ids }
    pub fn missing_unit_ids(&self) -> &[String] { &self.missing_unit_ids }
    pub fn unevaluated_unit_ids(&self) -> &[String] { &self.unevaluated_unit_ids }
    pub fn failed_unit_ids(&self) -> &[String] { &self.failed_unit_ids }
    pub fn unsupported_unit_ids(&self) -> &[String] { &self.unsupported_unit_ids }
    pub fn not_applicable_unit_ids(&self) -> &[String] { &self.not_applicable_unit_ids }
    pub fn reasons(&self) -> &[PlanReason] { &self.reasons }

    pub fn allows_authoritative_gate(&self) -> bool {
        self.authority == PlanAuthority::GateEligible && self.gate.complete && self.unevaluated_unit_ids.is_empty()
    }
}

/// Result identities are kept outside the plan itself because semantic result,
/// Baseline, Exception and TrustedContext become known at different stages.
/// They still participate in the cache key as one closed, deterministic tuple.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResultIdentity {
    pub semantic_result_digest: String,
    pub baseline_digest: String,
    pub exception_digest: String,
    pub unified_result_digest: Option<String>,
    pub trusted_context_digest: Option<String>,
}

impl ResultIdentity {
    pub fn validate(&self) -> Result<(), PlanError> {
        for (name, value) in [
            ("semantic_result_digest", &self.semantic_result_digest),
            ("baseline_digest", &self.baseline_digest),
            ("exception_digest", &self.exception_digest),
        ] {
            if !is_digest(value) { return Err(PlanError::InvalidDigest(name)); }
        }
        for (name, value) in [
            ("unified_result_digest", &self.unified_result_digest),
            ("trusted_context_digest", &self.trusted_context_digest),
        ] {
            if let Some(value) = value {
                if !is_digest(value) { return Err(PlanError::InvalidDigest(name)); }
            }
        }
        Ok(())
    }

    pub fn digest(&self) -> String {
        let mut bytes = b"scan-result-identity/v1\0".to_vec();
        append_string(&mut bytes, &self.semantic_result_digest);
        append_string(&mut bytes, &self.baseline_digest);
        append_string(&mut bytes, &self.exception_digest);
        append_optional_string(&mut bytes, self.unified_result_digest.as_deref());
        append_optional_string(&mut bytes, self.trusted_context_digest.as_deref());
        hex_digest(&Sha256::digest(bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    pub plan_digest: String,
    pub result_identity_digest: String,
    pub digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CacheDecision {
    Pass,
    Fail,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletenessSnapshot {
    pub plan_digest: String,
    pub mode: PlanMode,
    pub authority: PlanAuthority,
    pub scope_level: String,
    pub state: CoverageState,
    pub planned: usize,
    pub accounted: usize,
    pub failed: usize,
    pub missing: usize,
    pub observation_errors: usize,
    pub complete: bool,
    pub evaluated_unit_ids: Vec<String>,
    pub missing_unit_ids: Vec<String>,
    pub unevaluated_unit_ids: Vec<String>,
    pub reasons: Vec<PlanReason>,
}

impl CompletenessSnapshot {
    fn from_report(report: &CompletenessReport) -> Self {
        Self {
            plan_digest: report.plan_digest.clone(),
            mode: report.mode,
            authority: report.authority,
            scope_level: report.scope_level.code().to_string(),
            state: report.state,
            planned: report.gate.planned,
            accounted: report.gate.accounted,
            failed: report.gate.failed,
            missing: report.gate.missing,
            observation_errors: report.gate.observation_errors,
            complete: report.gate.complete,
            evaluated_unit_ids: report.evaluated_unit_ids.clone(),
            missing_unit_ids: report.missing_unit_ids.clone(),
            unevaluated_unit_ids: report.unevaluated_unit_ids.clone(),
            reasons: report.reasons.clone(),
        }
    }

    fn allows_authoritative_pass(&self) -> bool {
        self.authority == PlanAuthority::GateEligible
            && self.mode != PlanMode::IncrementalPreview
            && self.state == CoverageState::Complete
            && self.complete
            && self.missing == 0
            && self.unevaluated_unit_ids.is_empty()
            && self.failed == 0
            && self.observation_errors == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheEntry {
    pub version: String,
    pub key: CacheKey,
    pub result_identity: ResultIdentity,
    pub completeness: CompletenessSnapshot,
    pub decision: CacheDecision,
    pub record_digest: String,
    pub created_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
}

impl CacheEntry {
    pub fn from_report(
        plan: &ScanPlan,
        result_identity: ResultIdentity,
        report: &CompletenessReport,
        decision: CacheDecision,
        created_at_epoch_secs: u64,
        expires_at_epoch_secs: u64,
    ) -> Result<Self, PlanError> {
        if expires_at_epoch_secs <= created_at_epoch_secs {
            return Err(PlanError::InvalidCacheLifetime);
        }
        if report.plan_digest != plan.digest {
            return Err(PlanError::ReportPlanMismatch);
        }
        let key = plan.cache_key(&result_identity)?;
        let completeness = CompletenessSnapshot::from_report(report);
        if decision == CacheDecision::Pass && !completeness.allows_authoritative_pass() {
            return Err(PlanError::NonAuthoritativePass);
        }
        let mut entry = Self { version: CACHE_RECORD_VERSION.into(), key, result_identity, completeness, decision, record_digest: String::new(), created_at_epoch_secs, expires_at_epoch_secs };
        entry.record_digest = entry.compute_record_digest()?;
        Ok(entry)
    }

    fn validate_against(&self, plan: &ScanPlan, result_identity: &ResultIdentity, now_epoch_secs: u64) -> Result<(), CacheMissReason> {
        if self.version != CACHE_RECORD_VERSION { return Err(CacheMissReason::Corrupt); }
        if !self.has_valid_record_digest() { return Err(CacheMissReason::Corrupt); }
        if self.expires_at_epoch_secs <= self.created_at_epoch_secs { return Err(CacheMissReason::Corrupt); }
        if now_epoch_secs < self.created_at_epoch_secs { return Err(CacheMissReason::NotYetValid); }
        if now_epoch_secs >= self.expires_at_epoch_secs { return Err(CacheMissReason::Stale); }
        let expected = plan.cache_key(result_identity).map_err(|_| CacheMissReason::Corrupt)?;
        if self.key != expected { return Err(CacheMissReason::InputIdentityMismatch("cache key".into())); }
        if self.completeness.plan_digest != plan.digest { return Err(CacheMissReason::PlanMismatch); }
        if &self.result_identity != result_identity { return Err(CacheMissReason::InputIdentityMismatch("result identity".into())); }
        if self.decision == CacheDecision::Pass && !self.completeness.allows_authoritative_pass() {
            return Err(CacheMissReason::NonAuthoritativePass);
        }
        Ok(())
    }

    fn compute_record_digest(&self) -> Result<String, PlanError> {
        let mut unsigned = self.clone();
        unsigned.record_digest.clear();
        let bytes = serde_json::to_vec(&unsigned).map_err(|error| PlanError::CacheSerialization(error.to_string()))?;
        Ok(hex_digest(&Sha256::digest(bytes)))
    }

    fn has_valid_record_digest(&self) -> bool {
        is_digest(&self.record_digest) && self.compute_record_digest().ok().as_deref() == Some(self.record_digest.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheLookup {
    Hit(CacheEntry),
    Miss(CacheMissReason),
}

/// A bounded plan-level probe used before execution. The cache stores only
/// summaries, so this probe never pretends to supply Findings; it only tells
/// the Runner whether a valid summary for this frozen plan exists and whether
/// an unsafe/corrupt cache state must widen the route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheProbe {
    WarmHit,
    ColdMiss,
    Expired,
    Corrupt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheMissReason {
    NotFound,
    Corrupt,
    UnsafePath,
    NotYetValid,
    Stale,
    InputIdentityMismatch(String),
    PlanMismatch,
    Incomplete,
    NonAuthoritativePass,
}

#[derive(Debug, Clone)]
pub struct FileScanCache {
    root: PathBuf,
    max_record_bytes: u64,
}

impl FileScanCache {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self { root: root.as_ref().to_path_buf(), max_record_bytes: 4 * 1024 * 1024 }
    }

    pub fn root(&self) -> &Path { &self.root }

    pub fn put(&self, entry: &CacheEntry) -> Result<(), PlanError> {
        validate_cache_entry(entry)?;
        let bytes = serde_json::to_vec(entry).map_err(|error| PlanError::CacheSerialization(error.to_string()))?;
        if bytes.len() as u64 > self.max_record_bytes { return Err(PlanError::CacheTooLarge); }
        let path = self.object_path(&entry.key.digest)?;
        let parent = path.parent().ok_or(PlanError::CacheUnsafePath)?;
        fs::create_dir_all(parent).map_err(|error| PlanError::CacheIo(error.to_string()))?;
        if path.exists() {
            let existing = self.read_entry(&path)?;
            if existing == *entry { return Ok(()); }
            return Err(PlanError::CacheImmutableConflict);
        }
        let temp = parent.join(format!(".{}.{}.tmp", entry.key.digest, unique_suffix()));
        let result = (|| {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|error| PlanError::CacheIo(error.to_string()))?;
            file.write_all(&bytes).map_err(|error| PlanError::CacheIo(error.to_string()))?;
            file.sync_all().map_err(|error| PlanError::CacheIo(error.to_string()))?;
            match fs::hard_link(&temp, &path) {
                Ok(()) => { let _ = fs::remove_file(&temp); Ok(()) }
                Err(_) if path.exists() => {
                    let existing = self.read_entry(&path)?;
                    let _ = fs::remove_file(&temp);
                    if existing == *entry { Ok(()) } else { Err(PlanError::CacheImmutableConflict) }
                }
                Err(error) => Err(PlanError::CacheIo(error.to_string())),
            }
        })();
        if result.is_err() { let _ = fs::remove_file(&temp); }
        result
    }

    pub fn lookup(&self, plan: &ScanPlan, result_identity: &ResultIdentity, now_epoch_secs: u64) -> Result<CacheLookup, PlanError> {
        let key = plan.cache_key(result_identity)?;
        let path = self.object_path(&key.digest)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(CacheLookup::Miss(CacheMissReason::NotFound)),
            Err(error) => return Err(PlanError::CacheIo(error.to_string())),
        };
        if !metadata.file_type().is_file() { return Ok(CacheLookup::Miss(CacheMissReason::UnsafePath)); }
        if metadata.len() > self.max_record_bytes { return Ok(CacheLookup::Miss(CacheMissReason::Corrupt)); }
        let entry = match self.read_entry(&path) {
            Ok(entry) => entry,
            Err(PlanError::CacheCorrupt) => return Ok(CacheLookup::Miss(CacheMissReason::Corrupt)),
            Err(error) => return Err(error),
        };
        match entry.validate_against(plan, result_identity, now_epoch_secs) {
            Ok(()) => Ok(CacheLookup::Hit(entry)),
            Err(reason) => Ok(CacheLookup::Miss(reason)),
        }
    }

    /// Probe all bounded cache objects for a plan digest when the result
    /// identity is not known until Gate has normalized the current findings.
    /// This is intentionally a summary probe, not a Finding cache lookup.
    pub fn probe_plan(&self, plan: &ScanPlan, now_epoch_secs: u64) -> Result<CacheProbe, PlanError> {
        const MAX_PROBE_ENTRIES: usize = 4096;
        let object_root = self.root.join("sha256");
        let metadata = match fs::symlink_metadata(&object_root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(CacheProbe::ColdMiss),
            Err(error) => return Err(PlanError::CacheIo(error.to_string())),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Ok(CacheProbe::Corrupt);
        }
        let mut scanned = 0usize;
        let mut expired = false;
        for entry in fs::read_dir(&object_root).map_err(|error| PlanError::CacheIo(error.to_string()))? {
            scanned += 1;
            if scanned > MAX_PROBE_ENTRIES {
                return Ok(CacheProbe::Corrupt);
            }
            let entry = entry.map_err(|error| PlanError::CacheIo(error.to_string()))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| PlanError::CacheIo(error.to_string()))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > self.max_record_bytes {
                return Ok(CacheProbe::Corrupt);
            }
            let cache_entry = match self.read_entry(&path) {
                Ok(entry) => entry,
                Err(PlanError::CacheCorrupt | PlanError::CacheUnsafePath) => return Ok(CacheProbe::Corrupt),
                Err(error) => return Err(error),
            };
            if cache_entry.completeness.plan_digest != plan.digest {
                continue;
            }
            match cache_entry.validate_against(plan, &cache_entry.result_identity, now_epoch_secs) {
                Ok(()) => return Ok(CacheProbe::WarmHit),
                Err(CacheMissReason::Stale | CacheMissReason::NotYetValid) => expired = true,
                Err(_) => return Ok(CacheProbe::Corrupt),
            }
        }
        Ok(if expired { CacheProbe::Expired } else { CacheProbe::ColdMiss })
    }

    fn object_path(&self, digest: &str) -> Result<PathBuf, PlanError> {
        if !is_digest(digest) { return Err(PlanError::CacheUnsafePath); }
        Ok(self.root.join("sha256").join(format!("{digest}.json")))
    }

    fn read_entry(&self, path: &Path) -> Result<CacheEntry, PlanError> {
        let metadata = fs::symlink_metadata(path).map_err(|error| PlanError::CacheIo(error.to_string()))?;
        if !metadata.file_type().is_file() { return Err(PlanError::CacheUnsafePath); }
        if metadata.len() > self.max_record_bytes { return Err(PlanError::CacheCorrupt); }
        let mut file = File::open(path).map_err(|error| PlanError::CacheIo(error.to_string()))?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut bytes).map_err(|error| PlanError::CacheIo(error.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|_| PlanError::CacheCorrupt)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PlanError {
    #[error("invalid plan identity: {0}")]
    InvalidIdentity(&'static str),
    #[error("invalid digest identity: {0}")]
    InvalidDigest(&'static str),
    #[error("AffectedScope digest does not match PlanIdentity")]
    ScopeDigestMismatch,
    #[error("the ScanPlan has no analysis units")]
    NoUnits,
    #[error("analysis unit is invalid: {0}")]
    InvalidUnit(String),
    #[error("analysis unit is duplicated: {0}")]
    DuplicateUnit(String),
    #[error("the requested scope selected no analysis units")]
    NoSelectedUnits,
    #[error("execution unit ids do not match the frozen plan: expected {expected:?}, received {received:?}")]
    ExecutionUnitMismatch { expected: Vec<String>, received: Vec<String> },
    #[error("execution reported an unknown unit: {0}")]
    UnknownAccountUnit(String),
    #[error("execution reported a duplicate unit: {0}")]
    DuplicateAccount(String),
    #[error("report belongs to another ScanPlan")]
    ReportPlanMismatch,
    #[error("a cache record cannot make an incomplete or preview result PASS")]
    NonAuthoritativePass,
    #[error("cache record expiry must be after its creation time")]
    InvalidCacheLifetime,
    #[error("cache record is too large")]
    CacheTooLarge,
    #[error("cache path is unsafe")]
    CacheUnsafePath,
    #[error("cache entry is corrupt")]
    CacheCorrupt,
    #[error("cache entry for a digest is immutable")]
    CacheImmutableConflict,
    #[error("cache I/O failed: {0}")]
    CacheIo(String),
    #[error("cache serialization failed: {0}")]
    CacheSerialization(String),
}

fn unit_is_selected(unit: &AnalysisUnit, scope: &AffectedScope, mode: PlanMode) -> bool {
    if mode == PlanMode::Full { return true; }
    let backend = scope.backend(unit.backend);
    match mode {
        PlanMode::Full => true,
        PlanMode::Module => backend.level() == ScopeLevel::Full || backend.module_ids().iter().any(|id| id == &unit.module_id),
        PlanMode::IncrementalPreview => match backend.level() {
            ScopeLevel::Full => true,
            ScopeLevel::Module => backend.module_ids().iter().any(|id| id == &unit.module_id),
            ScopeLevel::FileCandidate => unit.path.as_ref().is_some_and(|path| backend.paths().iter().any(|candidate| candidate == path)),
        },
    }
}

fn validate_cache_entry(entry: &CacheEntry) -> Result<(), PlanError> {
    if entry.version != CACHE_RECORD_VERSION || !is_digest(&entry.key.plan_digest) || !is_digest(&entry.key.result_identity_digest) || !is_digest(&entry.key.digest) {
        return Err(PlanError::CacheCorrupt);
    }
    entry.result_identity.validate()?;
    if entry.expires_at_epoch_secs <= entry.created_at_epoch_secs { return Err(PlanError::InvalidCacheLifetime); }
    if entry.completeness.plan_digest != entry.key.plan_digest { return Err(PlanError::CacheCorrupt); }
    if !entry.has_valid_record_digest() { return Err(PlanError::CacheCorrupt); }
    if entry.decision == CacheDecision::Pass && !entry.completeness.allows_authoritative_pass() { return Err(PlanError::NonAuthoritativePass); }
    Ok(())
}

fn normalize_reasons(mut reasons: Vec<PlanReason>) -> Vec<PlanReason> {
    reasons.sort();
    reasons.dedup();
    reasons
}

fn join_ids(ids: &[String]) -> String { ids.join(",") }

fn append_identity(bytes: &mut Vec<u8>, identity: &PlanIdentity) {
    for value in [
        &identity.repository,
        &identity.base,
        &identity.head,
        &identity.tree,
        &identity.scope_digest,
        &identity.rule_bundle_digest,
        &identity.policy_digest,
        &identity.resolver_identity,
        &identity.backend_identity,
        &identity.runner_identity,
        &identity.canonical_set_version,
    ] { append_string(bytes, value); }
    bytes.extend_from_slice(&identity.protocol_major.to_be_bytes());
}

fn append_units(bytes: &mut Vec<u8>, units: &[AnalysisUnit]) {
    bytes.extend_from_slice(&(units.len() as u64).to_be_bytes());
    for unit in units {
        append_string(bytes, &unit.unit_id);
        append_string(bytes, unit.backend.code());
        append_string(bytes, &unit.module_id);
        append_optional_string(bytes, unit.path.as_deref());
    }
}

fn append_reasons(bytes: &mut Vec<u8>, reasons: &[PlanReason]) {
    bytes.extend_from_slice(&(reasons.len() as u64).to_be_bytes());
    for reason in reasons {
        append_string(bytes, reason.code.code());
        append_string(bytes, &reason.detail);
    }
}

fn append_strings(bytes: &mut Vec<u8>, values: &[String]) {
    bytes.extend_from_slice(&(values.len() as u64).to_be_bytes());
    for value in values { append_string(bytes, value); }
}

fn append_optional_string(bytes: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => { bytes.push(1); append_string(bytes, value); }
        None => bytes.push(0),
    }
}

fn append_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn is_digest(value: &str) -> bool { value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) }

fn hex_digest(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

fn unique_suffix() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_nanos()).unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erf_affected_scope::AffectedScopeResolver;
    use erf_git_scope::{GitChangeRange, GitChangeResolver};
    use std::process::{Command, Output};

    fn fixture() -> (PathBuf, AffectedScope) {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let repo = std::env::temp_dir().join(format!("erf-scan-plan-{stamp}-{}", std::process::id()));
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "--quiet"]);
        git(&repo, &["config", "user.email", "erf-tests@example.invalid"]);
        git(&repo, &["config", "user.name", "ERF Tests"]);
        write(&repo, "pom.xml", "<project><artifactId>root</artifactId></project>");
        write(&repo, "src/main/java/App.java", "class App {}\n");
        let base = commit(&repo, "base");
        write(&repo, "src/main/java/App.java", "class App { int changed; }\n");
        let head = commit(&repo, "change");
        let range = GitChangeRange::new(base, head).unwrap();
        let changed = GitChangeResolver::new(&repo).resolve(&range).unwrap();
        let scope = AffectedScopeResolver::new(&repo).resolve(&changed).unwrap();
        (repo, scope)
    }

    fn git(repo: &Path, args: &[&str]) -> Output {
        let output = Command::new("git").arg("-C").arg(repo).args(args).output().unwrap();
        assert!(output.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&output.stderr));
        output
    }

    fn write(repo: &Path, path: &str, text: &str) {
        let path = repo.join(path);
        if let Some(parent) = path.parent() { fs::create_dir_all(parent).unwrap(); }
        fs::write(path, text).unwrap();
    }

    fn commit(repo: &Path, message: &str) -> String {
        git(repo, &["add", "."]);
        git(repo, &["commit", "--quiet", "-m", message]);
        String::from_utf8(git(repo, &["rev-parse", "HEAD"]).stdout).unwrap().trim().into()
    }

    fn identity(scope: &AffectedScope) -> PlanIdentity {
        PlanIdentity {
            repository: "org/repo".into(),
            base: "base-commit".into(),
            head: "head-commit".into(),
            tree: "tree-commit".into(),
            scope_digest: scope.digest().into(),
            rule_bundle_digest: "a".repeat(64),
            policy_digest: "b".repeat(64),
            resolver_identity: "resolver/static@1".into(),
            backend_identity: "java/pmd-spotbugs@1".into(),
            runner_identity: "runner@1".into(),
            protocol_major: 0,
            canonical_set_version: "canonical-finding-set-v1".into(),
        }
    }

    fn units() -> Vec<AnalysisUnit> {
        vec![
            AnalysisUnit { unit_id: "root:pmd-ast".into(), backend: AnalysisKind::PmdAst, module_id: "root".into(), path: Some("src/main/java/App.java".into()) },
            AnalysisUnit { unit_id: "root:pmd-type".into(), backend: AnalysisKind::PmdTypeResolution, module_id: "root".into(), path: None },
            AnalysisUnit { unit_id: "root:spotbugs".into(), backend: AnalysisKind::SpotBugsBytecode, module_id: "root".into(), path: None },
            AnalysisUnit { unit_id: "other:pmd-ast".into(), backend: AnalysisKind::PmdAst, module_id: "other".into(), path: Some("other/src/main/java/Other.java".into()) },
        ]
    }

    fn result_identity() -> ResultIdentity {
        ResultIdentity { semantic_result_digest: "c".repeat(64), baseline_digest: "d".repeat(64), exception_digest: "e".repeat(64), unified_result_digest: Some("f".repeat(64)), trusted_context_digest: Some("1".repeat(64)) }
    }

    #[test]
    fn freezes_deterministic_preview_and_records_unevaluated_range() {
        let (repo, scope) = fixture();
        let request = || ScanPlanRequest { identity: identity(&scope), affected_scope: scope.clone(), requested_mode: PlanMode::IncrementalPreview, units: units() };
        let plans = (0..10).map(|_| ScanPlan::freeze(request()).unwrap()).collect::<Vec<_>>();
        let first = &plans[0];
        assert!(plans.iter().all(|plan| plan == first));
        assert!(plans.iter().all(|plan| plan.digest() == first.digest()));
        assert_eq!(first.mode(), PlanMode::IncrementalPreview);
        assert_eq!(first.authority(), PlanAuthority::PreviewOnly);
        assert!(!first.unevaluated_unit_ids().is_empty());
        assert!(first.reasons().iter().any(|reason| reason.code == PlanReasonCode::PreviewIsNotAuthoritative));
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn full_and_module_plans_are_gate_eligible_only_after_exact_accounting() {
        let (repo, scope) = fixture();
        let full = ScanPlan::freeze(ScanPlanRequest { identity: identity(&scope), affected_scope: scope.clone(), requested_mode: PlanMode::Full, units: units() }).unwrap();
        let accounts = full.planned_unit_ids().into_iter().map(|unit_id| UnitAccount { unit_id, outcome: UnitOutcome::Executed, reason: None }).collect::<Vec<_>>();
        let report = full.account(&accounts, 0).unwrap();
        assert!(report.allows_authoritative_gate());
        assert_eq!(report.state(), CoverageState::Complete);
        let missing = full.account(&accounts[..2], 0).unwrap();
        assert!(!missing.allows_authoritative_gate());
        assert_eq!(missing.state(), CoverageState::Partial);
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn cache_key_changes_for_every_authority_input_and_result_context() {
        let (repo, scope) = fixture();
        let plan = ScanPlan::freeze(ScanPlanRequest { identity: identity(&scope), affected_scope: scope.clone(), requested_mode: PlanMode::Full, units: units() }).unwrap();
        let base = plan.cache_key(&result_identity()).unwrap();
        let mut identity_changed = plan.identity().clone();
        for field in ["repository", "base", "head", "tree", "scope_digest", "rule_bundle_digest", "policy_digest", "resolver_identity", "backend_identity", "runner_identity", "canonical_set_version"] {
            let mut changed_plan = plan.clone();
            match field {
                "repository" => changed_plan.identity.repository.push('x'), "base" => changed_plan.identity.base.push('x'), "head" => changed_plan.identity.head.push('x'), "tree" => changed_plan.identity.tree.push('x'),
                "scope_digest" => changed_plan.identity.scope_digest = "9".repeat(64), "rule_bundle_digest" => changed_plan.identity.rule_bundle_digest = "9".repeat(64), "policy_digest" => changed_plan.identity.policy_digest = "9".repeat(64),
                "resolver_identity" => changed_plan.identity.resolver_identity.push('x'), "backend_identity" => changed_plan.identity.backend_identity.push('x'), "runner_identity" => changed_plan.identity.runner_identity.push('x'), "canonical_set_version" => changed_plan.identity.canonical_set_version.push('x'), _ => unreachable!(),
            }
            changed_plan.digest = hex_digest(&Sha256::digest(changed_plan.canonical_bytes()));
            assert_ne!(base.digest, changed_plan.cache_key(&result_identity()).unwrap().digest, "field {field}");
        }
        for slot in 0..5 {
            let mut changed = result_identity();
            match slot { 0 => changed.semantic_result_digest = "9".repeat(64), 1 => changed.baseline_digest = "9".repeat(64), 2 => changed.exception_digest = "9".repeat(64), 3 => changed.unified_result_digest = Some("9".repeat(64)), 4 => changed.trusted_context_digest = Some("9".repeat(64)), _ => unreachable!() }
            assert_ne!(base.digest, plan.cache_key(&changed).unwrap().digest, "result slot {slot}");
        }
        identity_changed.protocol_major = 1;
        let changed_plan = ScanPlan::freeze(ScanPlanRequest { identity: identity_changed, affected_scope: scope.clone(), requested_mode: PlanMode::Full, units: units() }).unwrap();
        assert_ne!(base.digest, changed_plan.cache_key(&result_identity()).unwrap().digest);
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn file_cache_survives_restart_rechecks_identity_and_rejects_stale_or_corrupt_records() {
        let (repo, scope) = fixture();
        let plan = ScanPlan::freeze(ScanPlanRequest { identity: identity(&scope), affected_scope: scope.clone(), requested_mode: PlanMode::Full, units: units() }).unwrap();
        let accounts = plan.planned_unit_ids().into_iter().map(|unit_id| UnitAccount { unit_id, outcome: UnitOutcome::Executed, reason: None }).collect::<Vec<_>>();
        let report = plan.account(&accounts, 0).unwrap();
        let entry = CacheEntry::from_report(&plan, result_identity(), &report, CacheDecision::Pass, 10, 20).unwrap();
        let root = std::env::temp_dir().join(format!("erf-scan-cache-{}", unique_suffix()));
        FileScanCache::new(&root).put(&entry).unwrap();
        let reopened = FileScanCache::new(&root);
        assert_eq!(reopened.probe_plan(&plan, 11).unwrap(), CacheProbe::WarmHit);
        assert_eq!(reopened.probe_plan(&plan, 20).unwrap(), CacheProbe::Expired);
        assert_eq!(reopened.lookup(&plan, &result_identity(), 9), Ok(CacheLookup::Miss(CacheMissReason::NotYetValid)));
        assert_eq!(reopened.lookup(&plan, &result_identity(), 11), Ok(CacheLookup::Hit(entry.clone())));
        assert_eq!(reopened.lookup(&plan, &result_identity(), 20), Ok(CacheLookup::Miss(CacheMissReason::Stale)));
        let mut changed_result = result_identity();
        changed_result.baseline_digest = "9".repeat(64);
        assert_eq!(reopened.lookup(&plan, &changed_result, 11), Ok(CacheLookup::Miss(CacheMissReason::NotFound)));
        let path = root.join("sha256").join(format!("{}.json", entry.key.digest));
        let mut tampered = entry.clone();
        tampered.decision = CacheDecision::Fail;
        fs::write(&path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert_eq!(reopened.lookup(&plan, &result_identity(), 11), Ok(CacheLookup::Miss(CacheMissReason::Corrupt)));
        fs::write(path, b"not-json").unwrap();
        assert_eq!(reopened.lookup(&plan, &result_identity(), 11), Ok(CacheLookup::Miss(CacheMissReason::Corrupt)));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn preview_or_missing_execution_cannot_be_stored_as_pass() {
        let (repo, scope) = fixture();
        let preview = ScanPlan::freeze(ScanPlanRequest { identity: identity(&scope), affected_scope: scope.clone(), requested_mode: PlanMode::IncrementalPreview, units: units() }).unwrap();
        let accounts = preview.planned_unit_ids().into_iter().map(|unit_id| UnitAccount { unit_id, outcome: UnitOutcome::Executed, reason: None }).collect::<Vec<_>>();
        let report = preview.account(&accounts, 0).unwrap();
        assert_eq!(report.state(), CoverageState::Partial);
        assert_eq!(CacheEntry::from_report(&preview, result_identity(), &report, CacheDecision::Pass, 1, 2), Err(PlanError::NonAuthoritativePass));
        let full = ScanPlan::freeze(ScanPlanRequest { identity: identity(&scope), affected_scope: scope.clone(), requested_mode: PlanMode::Full, units: units() }).unwrap();
        let incomplete = full.account(&[], 0).unwrap();
        assert_eq!(CacheEntry::from_report(&full, result_identity(), &incomplete, CacheDecision::Pass, 1, 2), Err(PlanError::NonAuthoritativePass));
        fs::remove_dir_all(repo).unwrap();
    }

}
