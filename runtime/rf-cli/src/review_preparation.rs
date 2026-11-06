//! Digest-only preparation seam for the P0 review contract.
//!
//! The hosted review surface intentionally stops before Runtime Gate
//! authority. This module validates the review-run-v1 metadata envelope,
//! projects its revision and scope facts into the existing `ChangedSet`,
//! `AffectedScope` and frozen `ScanPlan` types, and retains finding lifecycle
//! state as metadata. It never reads source content, invokes SCM, or writes
//! SCM state.

use erf_affected_scope::{AffectedScope, AffectedScopeError, AffectedScopeResolver, AnalysisKind, ScopeLevel};
use erf_git_scope::{ChangedSet, GitChangeScopeError};
use erf_scan_plan::{AnalysisUnit, PlanAuthority, PlanError, PlanIdentity, PlanMode, ScanPlan, ScanPlanRequest};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

pub const REVIEW_PREPARATION_VERSION: &str = "review-preparation/v1";
pub const PREPARATION_MODE: &str = "PREPARATION_ONLY";
pub const PREPARATION_AUTHORITY: &str = "LOCAL_UNATTESTED";

const FORBIDDEN_KEYS: &[&str] = &[
    "source",
    "sourceContent",
    "diff",
    "patch",
    "rawPrompt",
    "prompt",
    "body",
    "comment",
    "comments",
    "snippet",
    "fullFinding",
];

/// Immutable base/head identity supplied by the review contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionBinding {
    pub base_sha: String,
    pub head_sha: String,
    pub source_ref: String,
    pub target_ref: String,
    pub binding_digest: String,
}

/// One deterministic review scope. Files contain repository-relative paths
/// only; no source text or diff is accepted by this seam.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewScope {
    pub scope_id: String,
    pub files: Vec<String>,
    pub changed_lines: u64,
}

/// Digest-only scope plan from review-run-v1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopePlan {
    pub plan_id: String,
    pub revision_binding_digest: String,
    pub scopes: Vec<ReviewScope>,
    pub max_changed_lines: Option<u64>,
    pub max_files_per_scope: Option<u64>,
    pub plan_digest: String,
}

/// Finding lifecycle metadata retained for incremental review continuity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingLifecycle {
    pub finding_id: String,
    pub state: FindingState,
    pub human_resolved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum FindingState {
    Open,
    Kept,
    Resolved,
    Dismissed,
}

impl FindingState {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "OPEN" => Some(Self::Open),
            "KEPT" => Some(Self::Kept),
            "RESOLVED" => Some(Self::Resolved),
            "DISMISSED" => Some(Self::Dismissed),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::Kept => "KEPT",
            Self::Resolved => "RESOLVED",
            Self::Dismissed => "DISMISSED",
        }
    }
}

/// Optional incremental-review identity. A stale reviewed head is rejected
/// before any runtime projection is produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncrementalReview {
    pub review_run_id: String,
    pub reviewed_head_sha: String,
    pub baseline_review_run_digest: String,
    pub delta_mode: String,
}

/// Parsed review-run-v1 metadata. `ReviewRun::from_json` is the preferred
/// boundary because it checks forbidden source-bearing fields and validates
/// cross-object digest bindings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRun {
    pub revision_binding: RevisionBinding,
    pub scope_plan: ScopePlan,
    pub findings: Vec<FindingLifecycle>,
    pub incremental_review: Option<IncrementalReview>,
    pub mode: String,
    pub authority: String,
}

impl ReviewRun {
    pub fn from_json(value: &Value) -> Result<Self, ReviewPreparationError> {
        reject_forbidden_keys(value)?;
        let object = value
            .as_object()
            .ok_or_else(|| ReviewPreparationError::InvalidInput("review run must be a JSON object".into()))?;
        if let Some(schema) = object.get("schemaVersion") {
            if schema.as_str() != Some("review-run-v1") {
                return Err(ReviewPreparationError::InvalidInput(
                    "schemaVersion must be review-run-v1".into(),
                ));
            }
        }
        let revision_binding = parse_revision_binding(required_object(object, "revisionBinding")?)?;
        let scope_plan = parse_scope_plan(required_object(object, "scopePlan")?)?;
        let findings = object
            .get("findings")
            .map(parse_findings)
            .transpose()?
            .unwrap_or_default();
        let incremental_review = object
            .get("incrementalReview")
            .map(parse_incremental_review)
            .transpose()?;
        let mode = object
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or(PREPARATION_MODE)
            .to_string();
        let authority = object
            .get("authority")
            .and_then(Value::as_str)
            .unwrap_or(PREPARATION_AUTHORITY)
            .to_string();
        let review = Self {
            revision_binding,
            scope_plan,
            findings,
            incremental_review,
            mode,
            authority,
        };
        review.validate()?;
        Ok(review)
    }

    pub fn validate(&self) -> Result<(), ReviewPreparationError> {
        validate_sha(&self.revision_binding.base_sha, "revisionBinding.baseSha")?;
        validate_sha(&self.revision_binding.head_sha, "revisionBinding.headSha")?;
        if self.revision_binding.base_sha == self.revision_binding.head_sha {
            return Err(ReviewPreparationError::InvalidInput(
                "revisionBinding baseSha and headSha must differ".into(),
            ));
        }
        validate_text(&self.revision_binding.source_ref, "revisionBinding.sourceRef")?;
        validate_text(&self.revision_binding.target_ref, "revisionBinding.targetRef")?;
        validate_digest(&self.revision_binding.binding_digest, "revisionBinding.bindingDigest")?;
        validate_text(&self.scope_plan.plan_id, "scopePlan.planId")?;
        validate_digest(
            &self.scope_plan.revision_binding_digest,
            "scopePlan.revisionBindingDigest",
        )?;
        if self.scope_plan.revision_binding_digest != self.revision_binding.binding_digest {
            return Err(ReviewPreparationError::BindingMismatch);
        }
        validate_digest(&self.scope_plan.plan_digest, "scopePlan.planDigest")?;
        if self.scope_plan.scopes.is_empty() {
            return Err(ReviewPreparationError::InvalidInput("scopePlan.scopes must not be empty".into()));
        }
        let mut paths = BTreeSet::new();
        let mut scope_ids = BTreeSet::new();
        let mut total_changed_lines = 0u64;
        for scope in &self.scope_plan.scopes {
            validate_text(&scope.scope_id, "scopePlan.scope.scopeId")?;
            if scope.changed_lines == 0 {
                return Err(ReviewPreparationError::InvalidInput(format!(
                    "scope {} must report positive changedLines",
                    scope.scope_id
                )));
            }
            if !scope_ids.insert(&scope.scope_id) {
                return Err(ReviewPreparationError::InvalidInput(format!(
                    "scope {} is duplicated",
                    scope.scope_id
                )));
            }
            total_changed_lines = total_changed_lines
                .checked_add(scope.changed_lines)
                .ok_or_else(|| ReviewPreparationError::InvalidInput("scope changedLines overflow".into()))?;
            if scope.files.is_empty() {
                return Err(ReviewPreparationError::InvalidInput(format!(
                    "scope {} must contain at least one file",
                    scope.scope_id
                )));
            }
            if let Some(max) = self.scope_plan.max_files_per_scope {
                if max == 0 || scope.files.len() as u64 > max {
                    return Err(ReviewPreparationError::InvalidInput(format!(
                        "scope {} exceeds maxFilesPerScope",
                        scope.scope_id
                    )));
                }
            }
            if let Some(max) = self.scope_plan.max_changed_lines {
                if max == 0 || scope.changed_lines > max {
                    return Err(ReviewPreparationError::InvalidInput(format!(
                        "scope {} exceeds maxChangedLines",
                        scope.scope_id
                    )));
                }
            }
            for path in &scope.files {
                validate_text(path, "scopePlan.scope.file")?;
                let normalized = normalize_review_path(path)?;
                if !paths.insert(normalized.clone()) {
                    return Err(ReviewPreparationError::DuplicatePath(normalized));
                }
            }
        }
        if let Some(max) = self.scope_plan.max_changed_lines {
            if total_changed_lines > max {
                return Err(ReviewPreparationError::InvalidInput(
                    "scope plan exceeds maxChangedLines".into(),
                ));
            }
        }
        for finding in &self.findings {
            validate_text(&finding.finding_id, "findings.findingId")?;
            if finding.human_resolved && finding.state != FindingState::Resolved {
                return Err(ReviewPreparationError::InvalidInput(format!(
                    "finding {} marks humanResolved without RESOLVED state",
                    finding.finding_id
                )));
            }
        }
        let mut finding_ids = BTreeSet::new();
        for finding in &self.findings {
            if !finding_ids.insert(&finding.finding_id) {
                return Err(ReviewPreparationError::InvalidInput(format!(
                    "finding {} is duplicated",
                    finding.finding_id
                )));
            }
        }
        if let Some(incremental) = &self.incremental_review {
            validate_text(&incremental.review_run_id, "incrementalReview.reviewRunId")?;
            validate_sha(&incremental.reviewed_head_sha, "incrementalReview.reviewedHeadSha")?;
            validate_digest(
                &incremental.baseline_review_run_digest,
                "incrementalReview.baselineReviewRunDigest",
            )?;
            if incremental.reviewed_head_sha != self.revision_binding.head_sha {
                return Err(ReviewPreparationError::StaleRevision);
            }
            if !matches!(incremental.delta_mode.as_str(), "FULL" | "CHANGED_SCOPES_ONLY" | "NEW_FINDINGS_ONLY") {
                return Err(ReviewPreparationError::InvalidInput(
                    "incrementalReview.deltaMode is unsupported".into(),
                ));
            }
        }
        if self.mode != PREPARATION_MODE {
            return Err(ReviewPreparationError::AuthorityBoundary(format!(
                "mode must remain {PREPARATION_MODE}"
            )));
        }
        if self.authority != PREPARATION_AUTHORITY {
            return Err(ReviewPreparationError::AuthorityBoundary(format!(
                "authority must remain {PREPARATION_AUTHORITY}"
            )));
        }
        Ok(())
    }

    pub fn changed_paths(&self) -> impl Iterator<Item = &str> {
        self.scope_plan
            .scopes
            .iter()
            .flat_map(|scope| scope.files.iter().map(String::as_str))
    }
}

/// Runtime identities needed to freeze the existing ScanPlan. They are kept
/// explicit so the adapter cannot silently substitute mutable/latest assets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePlanIdentity {
    pub repository: String,
    pub rule_bundle_digest: String,
    pub policy_digest: String,
    pub resolver_identity: String,
    pub backend_identity: String,
    pub runner_identity: String,
    pub protocol_major: u32,
    pub canonical_set_version: String,
}

/// Input to [`ReviewPreparationAdapter::prepare`].
#[derive(Debug, Clone)]
pub struct ReviewPreparationRequest {
    pub repository_root: PathBuf,
    pub review: ReviewRun,
    pub runtime: RuntimePlanIdentity,
}

/// Projection produced by the P0 preparation seam.
#[derive(Debug, Clone)]
pub struct ReviewPreparation {
    review: ReviewRun,
    changed_set: ChangedSet,
    affected_scope: AffectedScope,
    scan_plan: ScanPlan,
    finding_lifecycle_digest: String,
    preparation_digest: String,
}

impl ReviewPreparation {
    pub fn review(&self) -> &ReviewRun {
        &self.review
    }

    pub fn changed_set(&self) -> &ChangedSet {
        &self.changed_set
    }

    pub fn affected_scope(&self) -> &AffectedScope {
        &self.affected_scope
    }

    pub fn scan_plan(&self) -> &ScanPlan {
        &self.scan_plan
    }

    pub fn finding_lifecycle_digest(&self) -> &str {
        &self.finding_lifecycle_digest
    }

    pub fn preparation_digest(&self) -> &str {
        &self.preparation_digest
    }

    pub fn mode(&self) -> &'static str {
        PREPARATION_MODE
    }

    pub fn authority(&self) -> &'static str {
        PREPARATION_AUTHORITY
    }

    /// Stable digest-only projection suitable for a local preparation
    /// manifest. The object intentionally contains no source, diff or full
    /// Finding fields and explicitly records that SCM writeback is forbidden.
    pub fn to_json(&self) -> Value {
        let binding = &self.review.revision_binding;
        let scope_plan = &self.review.scope_plan;
        let findings = self
            .review
            .findings
            .iter()
            .map(|finding| {
                serde_json::json!({
                    "findingId": finding.finding_id,
                    "state": finding.state.code(),
                    "humanResolved": finding.human_resolved,
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "schemaVersion": REVIEW_PREPARATION_VERSION,
            "mode": PREPARATION_MODE,
            "authority": PREPARATION_AUTHORITY,
            "scmWriteback": "forbidden",
            "revisionBinding": {
                "baseSha": binding.base_sha,
                "headSha": binding.head_sha,
                "sourceRef": binding.source_ref,
                "targetRef": binding.target_ref,
                "bindingDigest": binding.binding_digest,
            },
            "scopePlan": {
                "planId": scope_plan.plan_id,
                "revisionBindingDigest": scope_plan.revision_binding_digest,
                "planDigest": scope_plan.plan_digest,
                "scopes": scope_plan.scopes.iter().map(|scope| serde_json::json!({
                    "scopeId": scope.scope_id,
                    "files": scope.files,
                    "changedLines": scope.changed_lines,
                })).collect::<Vec<_>>(),
            },
            "changedSet": {
                "digest": format!("sha256:{}", self.changed_set.digest()),
                "baseSha": self.changed_set.range().base_commit(),
                "headSha": self.changed_set.range().head_commit(),
                "paths": self.changed_set.entries().iter().map(|entry| entry.path()).collect::<Vec<_>>(),
            },
            "affectedScopeDigest": format!("sha256:{}", self.affected_scope.digest()),
            "scanPlan": {
                "digest": format!("sha256:{}", self.scan_plan.digest()),
                "mode": self.scan_plan.mode().code(),
                "authority": self.scan_plan.authority().code(),
                "plannedUnitIds": self.scan_plan.planned_unit_ids(),
            },
            "findingLifecycleDigest": format!("sha256:{}", self.finding_lifecycle_digest),
            "findings": findings,
            "preparationDigest": format!("sha256:{}", self.preparation_digest),
        })
    }
}

pub struct ReviewPreparationAdapter;

impl ReviewPreparationAdapter {
    pub fn prepare(request: ReviewPreparationRequest) -> Result<ReviewPreparation, ReviewPreparationError> {
        request.review.validate()?;
        validate_runtime_identity(&request.runtime)?;
        let paths = request.review.changed_paths().collect::<Vec<_>>();
        let changed_set = ChangedSet::from_digest_only_paths(
            request.review.revision_binding.base_sha.clone(),
            request.review.revision_binding.head_sha.clone(),
            paths,
        )?;
        let affected_scope = AffectedScopeResolver::new(&request.repository_root).resolve(&changed_set)?;
        let scope_for_result = affected_scope.clone();
        let units = preparation_units(&affected_scope);
        if units.is_empty() {
            return Err(ReviewPreparationError::InvalidInput(
                "affected scope produced no preparation units".into(),
            ));
        }
        let identity = PlanIdentity {
            repository: request.runtime.repository,
            base: request.review.revision_binding.base_sha.clone(),
            head: request.review.revision_binding.head_sha.clone(),
            // RevisionBinding carries a commit identity, not a Git tree
            // proof. Bind a namespaced projection digest rather than
            // pretending the commit string is an attested tree identity.
            tree: digest_domain("review-tree-projection/v1", &request.review.revision_binding.head_sha),
            scope_digest: affected_scope.digest().to_string(),
            rule_bundle_digest: normalize_runtime_digest(&request.runtime.rule_bundle_digest, "rule_bundle_digest")?,
            policy_digest: normalize_runtime_digest(&request.runtime.policy_digest, "policy_digest")?,
            resolver_identity: request.runtime.resolver_identity,
            backend_identity: request.runtime.backend_identity,
            runner_identity: request.runtime.runner_identity,
            protocol_major: request.runtime.protocol_major,
            canonical_set_version: request.runtime.canonical_set_version,
        };
        let scan_plan = ScanPlan::freeze(ScanPlanRequest {
            identity,
            affected_scope,
            requested_mode: PlanMode::IncrementalPreview,
            units,
        })?;
        let finding_lifecycle_digest = digest_findings(&request.review.findings);
        let preparation_digest = digest_preparation(&request.review, &changed_set, &scan_plan, &finding_lifecycle_digest);
        Ok(ReviewPreparation {
            review: request.review,
            changed_set,
            affected_scope: scope_for_result,
            scan_plan,
            finding_lifecycle_digest,
            preparation_digest,
        })
    }
}

fn preparation_units(scope: &AffectedScope) -> Vec<AnalysisUnit> {
    let mut units = Vec::new();
    for kind in [AnalysisKind::PmdAst, AnalysisKind::PmdTypeResolution, AnalysisKind::SpotBugsBytecode] {
        let backend = scope.backend(kind);
        match backend.level() {
            ScopeLevel::FileCandidate => {
                let module = scope
                    .affected_module_ids()
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "review".into());
                for path in backend.paths() {
                    units.push(AnalysisUnit {
                        unit_id: format!("review/{}/{}/{}", kind.code(), module, path),
                        backend: kind,
                        module_id: module.clone(),
                        path: Some(path.clone()),
                    });
                }
            }
            ScopeLevel::Module | ScopeLevel::Full => {
                for module in backend.module_ids() {
                    units.push(AnalysisUnit {
                        unit_id: format!("review/{}/{module}", kind.code()),
                        backend: kind,
                        module_id: module.clone(),
                        path: None,
                    });
                }
            }
        }
    }
    units.sort();
    units.dedup_by(|left, right| left.unit_id == right.unit_id);
    units
}

fn digest_findings(findings: &[FindingLifecycle]) -> String {
    let mut findings = findings.to_vec();
    findings.sort_by(|left, right| {
        left.finding_id
            .cmp(&right.finding_id)
            .then_with(|| left.state.cmp(&right.state))
            .then_with(|| left.human_resolved.cmp(&right.human_resolved))
    });
    let mut bytes = b"finding-lifecycle/v1\0".to_vec();
    for finding in findings {
        append_string(&mut bytes, &finding.finding_id);
        append_string(&mut bytes, finding.state.code());
        bytes.push(u8::from(finding.human_resolved));
    }
    hex_digest(&Sha256::digest(bytes))
}

fn digest_preparation(review: &ReviewRun, changed_set: &ChangedSet, plan: &ScanPlan, findings: &str) -> String {
    let mut bytes = b"review-preparation/v1\0".to_vec();
    append_string(&mut bytes, &review.revision_binding.binding_digest);
    append_string(&mut bytes, &review.scope_plan.plan_digest);
    append_string(&mut bytes, &changed_set.digest());
    append_string(&mut bytes, plan.digest());
    append_string(&mut bytes, findings);
    hex_digest(&Sha256::digest(bytes))
}

fn parse_revision_binding(value: &Value) -> Result<RevisionBinding, ReviewPreparationError> {
    let object = value
        .as_object()
        .ok_or_else(|| ReviewPreparationError::InvalidInput("revisionBinding must be an object".into()))?;
    reject_unknown_keys(object, &["baseSha", "headSha", "sourceRef", "targetRef", "bindingDigest"], "revisionBinding")?;
    Ok(RevisionBinding {
        base_sha: required_string(object, "baseSha", "revisionBinding")?,
        head_sha: required_string(object, "headSha", "revisionBinding")?,
        source_ref: required_string(object, "sourceRef", "revisionBinding")?,
        target_ref: required_string(object, "targetRef", "revisionBinding")?,
        binding_digest: required_string(object, "bindingDigest", "revisionBinding")?,
    })
}

fn parse_scope_plan(value: &Value) -> Result<ScopePlan, ReviewPreparationError> {
    let object = value
        .as_object()
        .ok_or_else(|| ReviewPreparationError::InvalidInput("scopePlan must be an object".into()))?;
    reject_unknown_keys(object, &["planId", "revisionBindingDigest", "scopes", "constraints", "planDigest"], "scopePlan")?;
    let scopes = object
        .get("scopes")
        .ok_or_else(|| ReviewPreparationError::InvalidInput("scopePlan.scopes is required".into()))?
        .as_array()
        .ok_or_else(|| ReviewPreparationError::InvalidInput("scopePlan.scopes must be an array".into()))?
        .iter()
        .map(parse_scope)
        .collect::<Result<Vec<_>, _>>()?;
    let (max_changed_lines, max_files_per_scope) = match object.get("constraints") {
        None => (None, None),
        Some(value) => {
            let constraints = value
                .as_object()
                .ok_or_else(|| ReviewPreparationError::InvalidInput("scopePlan.constraints must be an object".into()))?;
            reject_unknown_keys(constraints, &["maxChangedLines", "maxFilesPerScope", "exactlyOneScopePerFile"], "scopePlan.constraints")?;
            if constraints.get("exactlyOneScopePerFile").and_then(Value::as_bool) == Some(false) {
                return Err(ReviewPreparationError::InvalidInput("scopePlan requires exactlyOneScopePerFile".into()));
            }
            (
                constraints.get("maxChangedLines").map(parse_positive_u64).transpose()?,
                constraints.get("maxFilesPerScope").map(parse_positive_u64).transpose()?,
            )
        }
    };
    Ok(ScopePlan {
        plan_id: required_string(object, "planId", "scopePlan")?,
        revision_binding_digest: required_string(object, "revisionBindingDigest", "scopePlan")?,
        scopes,
        max_changed_lines,
        max_files_per_scope,
        plan_digest: required_string(object, "planDigest", "scopePlan")?,
    })
}

fn parse_scope(value: &Value) -> Result<ReviewScope, ReviewPreparationError> {
    let object = value
        .as_object()
        .ok_or_else(|| ReviewPreparationError::InvalidInput("scopePlan.scope must be an object".into()))?;
    reject_unknown_keys(object, &["scopeId", "files", "changedLines"], "scopePlan.scope")?;
    let files = object
        .get("files")
        .ok_or_else(|| ReviewPreparationError::InvalidInput("scopePlan.scope.files is required".into()))?
        .as_array()
        .ok_or_else(|| ReviewPreparationError::InvalidInput("scopePlan.scope.files must be an array".into()))?
        .iter()
        .map(|value| value.as_str().map(ToOwned::to_owned).ok_or_else(|| ReviewPreparationError::InvalidInput("scopePlan.scope.files must contain strings".into())))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ReviewScope {
        scope_id: required_string(object, "scopeId", "scopePlan.scope")?,
        files,
        changed_lines: object
            .get("changedLines")
            .ok_or_else(|| ReviewPreparationError::InvalidInput("scopePlan.scope.changedLines is required".into()))?
            .as_u64()
            .ok_or_else(|| ReviewPreparationError::InvalidInput("scopePlan.scope.changedLines must be an integer".into()))?,
    })
}

fn parse_findings(value: &Value) -> Result<Vec<FindingLifecycle>, ReviewPreparationError> {
    let values = value
        .as_array()
        .ok_or_else(|| ReviewPreparationError::InvalidInput("findings must be an array".into()))?;
    values.iter().map(parse_finding).collect()
}

fn parse_finding(value: &Value) -> Result<FindingLifecycle, ReviewPreparationError> {
    let object = value
        .as_object()
        .ok_or_else(|| ReviewPreparationError::InvalidInput("finding must be an object".into()))?;
    reject_unknown_keys(object, &["findingId", "state", "humanResolved"], "finding")?;
    let state_value = required_string(object, "state", "finding")?;
    let state = FindingState::parse(&state_value)
        .ok_or_else(|| ReviewPreparationError::InvalidInput(format!("unsupported finding state {state_value}")))?;
    Ok(FindingLifecycle {
        finding_id: required_string(object, "findingId", "finding")?,
        state,
        human_resolved: object.get("humanResolved").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn parse_incremental_review(value: &Value) -> Result<IncrementalReview, ReviewPreparationError> {
    let object = value
        .as_object()
        .ok_or_else(|| ReviewPreparationError::InvalidInput("incrementalReview must be an object".into()))?;
    reject_unknown_keys(object, &["reviewRunId", "reviewedHeadSha", "baselineReviewRunDigest", "deltaMode", "findings"], "incrementalReview")?;
    Ok(IncrementalReview {
        review_run_id: required_string(object, "reviewRunId", "incrementalReview")?,
        reviewed_head_sha: required_string(object, "reviewedHeadSha", "incrementalReview")?,
        baseline_review_run_digest: required_string(object, "baselineReviewRunDigest", "incrementalReview")?,
        delta_mode: required_string(object, "deltaMode", "incrementalReview")?,
    })
}

fn required_object<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, ReviewPreparationError> {
    object
        .get(key)
        .ok_or_else(|| ReviewPreparationError::InvalidInput(format!("{key} is required")))
}

fn required_string(object: &Map<String, Value>, key: &str, context: &str) -> Result<String, ReviewPreparationError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| ReviewPreparationError::InvalidInput(format!("{context}.{key} must be a string")))
}

fn parse_positive_u64(value: &Value) -> Result<u64, ReviewPreparationError> {
    let value = value
        .as_u64()
        .ok_or_else(|| ReviewPreparationError::InvalidInput("constraint must be an integer".into()))?;
    (value > 0)
        .then_some(value)
        .ok_or_else(|| ReviewPreparationError::InvalidInput("constraint must be positive".into()))
}

fn reject_unknown_keys(object: &Map<String, Value>, allowed: &[&str], context: &str) -> Result<(), ReviewPreparationError> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(ReviewPreparationError::ForbiddenField(format!("{context}.{key}")));
    }
    Ok(())
}

fn reject_forbidden_keys(value: &Value) -> Result<(), ReviewPreparationError> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if FORBIDDEN_KEYS.contains(&key.as_str()) {
                    return Err(ReviewPreparationError::ForbiddenField(key.clone()));
                }
                reject_forbidden_keys(value)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                reject_forbidden_keys(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn normalize_review_path(path: &str) -> Result<String, ReviewPreparationError> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.chars().any(char::is_control) {
        return Err(ReviewPreparationError::InvalidInput(format!("invalid repository-relative path {path:?}")));
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(ReviewPreparationError::InvalidInput(format!("path escapes repository: {path:?}")));
        }
        parts.push(part);
    }
    if parts.is_empty() {
        return Err(ReviewPreparationError::InvalidInput(format!("invalid repository-relative path {path:?}")));
    }
    Ok(parts.join("/"))
}

fn validate_text(value: &str, field: &str) -> Result<(), ReviewPreparationError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(ReviewPreparationError::InvalidInput(format!("{field} must be non-empty and control-free")));
    }
    Ok(())
}

fn validate_sha(value: &str, field: &str) -> Result<(), ReviewPreparationError> {
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(ReviewPreparationError::InvalidInput(format!("{field} must be lowercase 40/64-character hexadecimal")));
    }
    Ok(())
}

fn validate_digest(value: &str, field: &str) -> Result<(), ReviewPreparationError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(ReviewPreparationError::InvalidInput(format!("{field} must use sha256: prefix")));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(ReviewPreparationError::InvalidInput(format!("{field} must be lowercase SHA-256")));
    }
    Ok(())
}

fn validate_runtime_identity(runtime: &RuntimePlanIdentity) -> Result<(), ReviewPreparationError> {
    let plan = PlanIdentity {
        repository: runtime.repository.clone(),
        base: "a".repeat(40),
        head: "b".repeat(40),
        tree: "c".repeat(40),
        scope_digest: "0".repeat(64),
        rule_bundle_digest: normalize_runtime_digest(&runtime.rule_bundle_digest, "rule_bundle_digest")?,
        policy_digest: normalize_runtime_digest(&runtime.policy_digest, "policy_digest")?,
        resolver_identity: runtime.resolver_identity.clone(),
        backend_identity: runtime.backend_identity.clone(),
        runner_identity: runtime.runner_identity.clone(),
        protocol_major: runtime.protocol_major,
        canonical_set_version: runtime.canonical_set_version.clone(),
    };
    plan.validate().map_err(ReviewPreparationError::Plan)
}

fn normalize_runtime_digest(value: &str, field: &str) -> Result<String, ReviewPreparationError> {
    let value = value.strip_prefix("sha256:").unwrap_or(value);
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(ReviewPreparationError::InvalidInput(format!(
            "{field} must be a lowercase 64-character SHA-256 digest"
        )));
    }
    Ok(value.to_string())
}

fn digest_domain(domain: &str, value: &str) -> String {
    let mut bytes = domain.as_bytes().to_vec();
    bytes.push(0);
    append_string(&mut bytes, value);
    hex_digest(&Sha256::digest(bytes))
}

fn append_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Debug)]
pub enum ReviewPreparationError {
    InvalidInput(String),
    ForbiddenField(String),
    BindingMismatch,
    StaleRevision,
    DuplicatePath(String),
    ChangedSet(GitChangeScopeError),
    AffectedScope(String),
    Plan(PlanError),
    AuthorityBoundary(String),
}

impl fmt::Display for ReviewPreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(detail) => write!(f, "invalid review preparation input: {detail}"),
            Self::ForbiddenField(field) => write!(f, "forbidden source-bearing field: {field}"),
            Self::BindingMismatch => f.write_str("scope plan does not bind the revision binding digest"),
            Self::StaleRevision => f.write_str("incremental review is stale for the bound head revision"),
            Self::DuplicatePath(path) => write!(f, "changed file belongs to more than one scope: {path}"),
            Self::ChangedSet(error) => write!(f, "digest-only ChangedSet projection failed: {error}"),
            Self::AffectedScope(error) => write!(f, "AffectedScope preparation failed: {error}"),
            Self::Plan(error) => write!(f, "ScanPlan preparation failed: {error}"),
            Self::AuthorityBoundary(detail) => write!(f, "review authority boundary violated: {detail}"),
        }
    }
}

impl std::error::Error for ReviewPreparationError {}

impl From<GitChangeScopeError> for ReviewPreparationError {
    fn from(error: GitChangeScopeError) -> Self {
        Self::ChangedSet(error)
    }
}

impl From<AffectedScopeError> for ReviewPreparationError {
    fn from(error: AffectedScopeError) -> Self {
        Self::AffectedScope(error.to_string())
    }
}

impl From<PlanError> for ReviewPreparationError {
    fn from(error: PlanError) -> Self {
        Self::Plan(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn digest() -> String {
        "sha256:".to_owned() + &"0".repeat(64)
    }

    fn sha(value: char) -> String {
        value.to_string().repeat(40)
    }

    fn review_json() -> Value {
        serde_json::json!({
            "schemaVersion": "review-run-v1",
            "revisionBinding": {
                "baseSha": sha('a'),
                "headSha": sha('b'),
                "sourceRef": "refs/heads/feature/review",
                "targetRef": "refs/heads/main",
                "bindingDigest": digest()
            },
            "scopePlan": {
                "planId": "scope-plan-1",
                "revisionBindingDigest": digest(),
                "constraints": {"maxChangedLines": 100, "maxFilesPerScope": 10, "exactlyOneScopePerFile": true},
                "scopes": [{"scopeId": "scope-1", "files": ["src/main/java/A.java"], "changedLines": 4}],
                "planDigest": digest()
            },
            "findings": [
                {"findingId": "finding-1", "state": "OPEN"},
                {"findingId": "finding-2", "state": "RESOLVED", "humanResolved": true}
            ],
            "incrementalReview": {
                "reviewRunId": "run-1",
                "reviewedHeadSha": sha('b'),
                "baselineReviewRunDigest": digest(),
                "deltaMode": "CHANGED_SCOPES_ONLY"
            },
            "mode": PREPARATION_MODE,
            "authority": PREPARATION_AUTHORITY
        })
    }

    fn runtime() -> RuntimePlanIdentity {
        RuntimePlanIdentity {
            repository: "example.invalid/ruleops".into(),
            rule_bundle_digest: digest(),
            policy_digest: digest(),
            resolver_identity: "resolver/1".into(),
            backend_identity: "pmd/1".into(),
            runner_identity: "runner/1".into(),
            protocol_major: 1,
            canonical_set_version: "canonical-finding/v1".into(),
        }
    }

    #[test]
    fn normalizes_prefixed_runtime_digests_before_scan_plan_validation() {
        let root = std::env::temp_dir();
        let input = runtime();
        validate_runtime_identity(&input).unwrap();
        let raw = normalize_runtime_digest(&input.rule_bundle_digest, "rule_bundle_digest").unwrap();
        assert_eq!(raw, "0".repeat(64));
        let plan = PlanIdentity {
            repository: input.repository.clone(),
            base: sha('a'),
            head: sha('b'),
            tree: "c".repeat(40),
            scope_digest: "0".repeat(64),
            rule_bundle_digest: raw,
            policy_digest: normalize_runtime_digest(&input.policy_digest, "policy_digest").unwrap(),
            resolver_identity: input.resolver_identity.clone(),
            backend_identity: input.backend_identity.clone(),
            runner_identity: input.runner_identity.clone(),
            protocol_major: input.protocol_major,
            canonical_set_version: input.canonical_set_version.clone(),
        };
        assert!(plan.validate().is_ok(), "PlanIdentity must consume raw 64-hex digests");
    }

    #[test]
    fn parses_digest_only_review_and_rejects_source_content() {
        let review = ReviewRun::from_json(&review_json()).unwrap();
        assert_eq!(review.revision_binding.head_sha, sha('b'));
        assert_eq!(review.findings[1].state, FindingState::Resolved);
        let mut value = review_json();
        value["findings"][0]["sourceContent"] = Value::String("class A {}".into());
        assert!(matches!(ReviewRun::from_json(&value), Err(ReviewPreparationError::ForbiddenField(_))));
    }

    #[test]
    fn rejects_stale_head_duplicate_files_and_cross_binding() {
        let mut stale = review_json();
        stale["incrementalReview"]["reviewedHeadSha"] = Value::String(sha('c'));
        assert!(matches!(ReviewRun::from_json(&stale), Err(ReviewPreparationError::StaleRevision)));

        let mut duplicate = review_json();
        duplicate["scopePlan"]["scopes"] = serde_json::json!([
            {"scopeId": "one", "files": ["src/A.java"], "changedLines": 1},
            {"scopeId": "two", "files": ["src/A.java"], "changedLines": 1}
        ]);
        assert!(matches!(ReviewRun::from_json(&duplicate), Err(ReviewPreparationError::DuplicatePath(_))));

        let mut mismatch = review_json();
        mismatch["scopePlan"]["revisionBindingDigest"] = Value::String("sha256:".to_owned() + &"1".repeat(64));
        assert!(matches!(ReviewRun::from_json(&mismatch), Err(ReviewPreparationError::BindingMismatch)));
    }

    #[test]
    fn projects_scope_into_preview_plan_without_gate_authority_or_scm_writeback() {
        let root = std::env::temp_dir().join(format!("erf-review-prep-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(root.join("src/main/java")).unwrap();
        fs::write(root.join("pom.xml"), "<project><artifactId>demo</artifactId></project>\n").unwrap();
        fs::write(root.join("src/main/java/A.java"), "class A {}\n").unwrap();
        let request = ReviewPreparationRequest {
            repository_root: root.clone(),
            review: ReviewRun::from_json(&review_json()).unwrap(),
            runtime: runtime(),
        };
        let preparation = ReviewPreparationAdapter::prepare(request).unwrap();
        assert_eq!(preparation.mode(), PREPARATION_MODE);
        assert_eq!(preparation.authority(), PREPARATION_AUTHORITY);
        assert_eq!(preparation.scan_plan().mode(), PlanMode::IncrementalPreview);
        assert_eq!(preparation.scan_plan().authority(), PlanAuthority::PreviewOnly);
        assert_eq!(preparation.changed_set().entries().len(), 1);
        assert_eq!(preparation.changed_set().entries()[0].path(), "src/main/java/A.java");
        let json = preparation.to_json();
        assert_eq!(json["scmWriteback"], "forbidden");
        assert_eq!(json["mode"], PREPARATION_MODE);
        assert_eq!(json["authority"], PREPARATION_AUTHORITY);
        assert!(json.get("sourceContent").is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
