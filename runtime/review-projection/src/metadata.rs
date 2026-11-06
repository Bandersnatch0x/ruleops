//! Digest-only P2 review metadata projections.
//!
//! This module adds optional provider draft/rescore metadata, immutable base
//! rules/config binding identity, and advisory impact-score projections to
//! the existing [`crate::ReviewProjectionAdapter`] boundary.  It is an
//! adapter seam rather than a network client: all values are local metadata,
//! source-bearing fields are rejected, and `PREPARATION_ONLY` /
//! `LOCAL_UNATTESTED` remains fixed throughout.

use super::{
    canonical_json, digest_json, reject_forbidden_fields, sha256_digest, valid_digest,
    DashboardSnapshot, ReviewProjectionError, TelemetryProjection, PREPARATION_AUTHORITY,
    PREPARATION_MODE,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const REVIEW_METADATA_VERSION: &str = "review-projection-metadata-v1";
pub const PROVIDER_DRAFT_VERSION: &str = "review-provider-draft-v1";
pub const RESCORE_JOB_VERSION: &str = "review-rescore-job-v1";
pub const BASE_RULES_CONFIG_VERSION: &str = "review-base-rules-config-v1";
pub const IMPACT_CONTEXT_VERSION: &str = "code-graph-impact-context-v1";
pub const SCORE_PROJECTION_VERSION: &str = "finding-score-projection-v1";

const SCORING_WEIGHTS: [(FindingSeverity, u64); 5] = [
    (FindingSeverity::Info, 1),
    (FindingSeverity::Low, 2),
    (FindingSeverity::Medium, 5),
    (FindingSeverity::High, 8),
    (FindingSeverity::Critical, 13),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Github,
    Gitlab,
}

impl Provider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Gitlab => "gitlab",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FindingSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl FindingSeverity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }

    pub const fn weight(self) -> u64 {
        match self {
            Self::Info => 1,
            Self::Low => 2,
            Self::Medium => 5,
            Self::High => 8,
            Self::Critical => 13,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FindingDisposition {
    Open,
    Kept,
    Resolved,
    Dismissed,
}

impl FindingDisposition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::Kept => "KEPT",
            Self::Resolved => "RESOLVED",
            Self::Dismissed => "DISMISSED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderFindingMetadata {
    #[serde(rename = "findingId")]
    pub finding_id: String,
    #[serde(rename = "ruleDigest")]
    pub rule_digest: String,
    #[serde(rename = "locationDigest")]
    pub location_digest: String,
    pub severity: FindingSeverity,
    pub disposition: FindingDisposition,
}

impl ProviderFindingMetadata {
    pub fn new(
        finding_id: impl Into<String>,
        rule_digest: impl Into<String>,
        location_digest: impl Into<String>,
        severity: FindingSeverity,
        disposition: FindingDisposition,
    ) -> Result<Self, ReviewProjectionError> {
        let value = Self {
            finding_id: finding_id.into(),
            rule_digest: rule_digest.into(),
            location_digest: location_digest.into(),
            severity,
            disposition,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ReviewProjectionError> {
        validate_text(&self.finding_id, "provider.findings.finding_id")?;
        validate_digest(&self.rule_digest, "provider.findings.rule_digest")?;
        validate_digest(&self.location_digest, "provider.findings.location_digest")?;
        Ok(())
    }
}

/// Provider-neutral draft metadata.  It deliberately stores the title/body
/// as digests rather than accepting either source field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDraftMetadata {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "draftId")]
    pub draft_id: String,
    pub provider: Provider,
    #[serde(rename = "repositoryDigest")]
    pub repository_digest: String,
    #[serde(rename = "changeRequestId")]
    pub change_request_id: String,
    #[serde(rename = "changeRequestKind")]
    pub change_request_kind: String,
    #[serde(rename = "changeRequestNumber")]
    pub change_request_number: u64,
    #[serde(rename = "titleDigest")]
    pub title_digest: String,
    #[serde(rename = "bodyDigest")]
    pub body_digest: String,
    #[serde(rename = "baseSha")]
    pub base_sha: String,
    #[serde(rename = "headSha")]
    pub head_sha: String,
    pub findings: Vec<ProviderFindingMetadata>,
    #[serde(rename = "projectionDigest")]
    pub projection_digest: String,
}

impl ProviderDraftMetadata {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        draft_id: impl Into<String>,
        provider: Provider,
        repository_digest: impl Into<String>,
        change_request_number: u64,
        title_digest: impl Into<String>,
        body_digest: impl Into<String>,
        base_sha: impl Into<String>,
        head_sha: impl Into<String>,
        findings: Vec<ProviderFindingMetadata>,
    ) -> Result<Self, ReviewProjectionError> {
        let change_request_number = change_request_number;
        let value = Self {
            schema_version: PROVIDER_DRAFT_VERSION.to_owned(),
            draft_id: draft_id.into(),
            provider,
            repository_digest: repository_digest.into(),
            change_request_id: change_request_number.to_string(),
            change_request_kind: "change-request".to_owned(),
            change_request_number,
            title_digest: title_digest.into(),
            body_digest: body_digest.into(),
            base_sha: base_sha.into(),
            head_sha: head_sha.into(),
            findings,
            projection_digest: String::new(),
        };
        value.validate_without_projection_digest()?;
        let projection_digest = metadata_digest_json(&value.projection_value_without_digest()?)?;
        Ok(Self { projection_digest, ..value })
    }

    pub fn validate(&self) -> Result<(), ReviewProjectionError> {
        self.validate_without_projection_digest()?;
        validate_digest(&self.projection_digest, "provider.projection_digest")?;
        let expected = metadata_digest_json(&self.projection_value_without_digest()?)?;
        if expected != self.projection_digest {
            return Err(ReviewProjectionError::InvalidField("provider.projection_digest"));
        }
        Ok(())
    }

    fn validate_without_projection_digest(&self) -> Result<(), ReviewProjectionError> {
        if self.schema_version != PROVIDER_DRAFT_VERSION {
            return Err(ReviewProjectionError::InvalidField("provider.schema_version"));
        }
        validate_text(&self.draft_id, "provider.draft_id")?;
        validate_digest(&self.repository_digest, "provider.repository_digest")?;
        validate_text(&self.change_request_id, "provider.change_request_id")?;
        if self.change_request_id.parse::<u64>().ok() != Some(self.change_request_number) {
            return Err(ReviewProjectionError::InvalidField("provider.change_request_id"));
        }
        if self.change_request_kind != "change-request" {
            return Err(ReviewProjectionError::InvalidField("provider.change_request_kind"));
        }
        validate_digest(&self.title_digest, "provider.title_digest")?;
        validate_digest(&self.body_digest, "provider.body_digest")?;
        validate_hex_sha(&self.base_sha, "provider.base_sha")?;
        validate_hex_sha(&self.head_sha, "provider.head_sha")?;
        if self.base_sha == self.head_sha {
            return Err(ReviewProjectionError::InvalidField("provider.revision"));
        }
        for finding in &self.findings {
            finding.validate()?;
        }
        let value = serde_json::to_value(self).map_err(serialization_error)?;
        reject_metadata_fields(&value)
    }

    fn projection_value_without_digest(&self) -> Result<Value, ReviewProjectionError> {
        let mut value = serde_json::to_value(self).map_err(serialization_error)?;
        value
            .as_object_mut()
            .expect("provider draft is an object")
            .remove("projectionDigest");
        Ok(value)
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("provider draft metadata is serializable")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderScorerMetadata {
    pub provider: String,
    pub model: String,
    #[serde(rename = "modelVersion")]
    pub model_version: String,
    #[serde(rename = "policyDigest")]
    pub policy_digest: String,
}

impl ProviderScorerMetadata {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        model_version: impl Into<String>,
        policy_digest: impl Into<String>,
    ) -> Result<Self, ReviewProjectionError> {
        let value = Self {
            provider: provider.into(),
            model: model.into(),
            model_version: model_version.into(),
            policy_digest: policy_digest.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ReviewProjectionError> {
        validate_text(&self.provider, "rescore.scorer.provider")?;
        validate_text(&self.model, "rescore.scorer.model")?;
        validate_text(&self.model_version, "rescore.scorer.model_version")?;
        validate_digest(&self.policy_digest, "rescore.scorer.policy_digest")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RescoreStatus {
    Queued,
    Running,
    Succeeded,
    Inconclusive,
    Rejected,
}

impl RescoreStatus {
    pub const fn requires_result(self) -> bool {
        matches!(self, Self::Succeeded | Self::Inconclusive)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RescoreResult {
    #[serde(rename = "resultDigest")]
    pub result_digest: String,
    #[serde(rename = "scoreDigest")]
    pub score_digest: String,
}

impl RescoreResult {
    pub fn new(
        result_digest: impl Into<String>,
        score_digest: impl Into<String>,
    ) -> Result<Self, ReviewProjectionError> {
        let value = Self { result_digest: result_digest.into(), score_digest: score_digest.into() };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), ReviewProjectionError> {
        validate_digest(&self.result_digest, "rescore.result.result_digest")?;
        validate_digest(&self.score_digest, "rescore.result.score_digest")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRescoreMetadata {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "rescoreJobId")]
    pub rescore_job_id: String,
    #[serde(rename = "draftDigest")]
    pub draft_digest: String,
    #[serde(rename = "baselineScoreDigest")]
    pub baseline_score_digest: String,
    #[serde(rename = "independentRescore")]
    pub independent_rescore: bool,
    pub scorer: ProviderScorerMetadata,
    #[serde(rename = "requestedAt")]
    pub requested_at: String,
    pub status: RescoreStatus,
    pub result: Option<RescoreResult>,
    #[serde(rename = "metadataDigest")]
    pub metadata_digest: String,
}

impl ProviderRescoreMetadata {
    pub fn new(
        rescore_job_id: impl Into<String>,
        draft_digest: impl Into<String>,
        baseline_score_digest: impl Into<String>,
        scorer: ProviderScorerMetadata,
        requested_at: impl Into<String>,
        status: RescoreStatus,
        result: Option<RescoreResult>,
    ) -> Result<Self, ReviewProjectionError> {
        let value = Self {
            schema_version: RESCORE_JOB_VERSION.to_owned(),
            rescore_job_id: rescore_job_id.into(),
            draft_digest: draft_digest.into(),
            baseline_score_digest: baseline_score_digest.into(),
            independent_rescore: true,
            scorer,
            requested_at: requested_at.into(),
            status,
            result,
            metadata_digest: String::new(),
        };
        value.validate_without_metadata_digest()?;
        let metadata_digest = digest_json(&value.metadata_value_without_digest()?)?;
        Ok(Self { metadata_digest, ..value })
    }

    pub fn validate(&self) -> Result<(), ReviewProjectionError> {
        self.validate_without_metadata_digest()?;
        validate_digest(&self.metadata_digest, "rescore.metadata_digest")?;
        let expected = digest_json(&self.metadata_value_without_digest()?)?;
        if expected != self.metadata_digest {
            return Err(ReviewProjectionError::InvalidField("rescore.metadata_digest"));
        }
        Ok(())
    }

    fn validate_without_metadata_digest(&self) -> Result<(), ReviewProjectionError> {
        if self.schema_version != RESCORE_JOB_VERSION {
            return Err(ReviewProjectionError::InvalidField("rescore.schema_version"));
        }
        validate_text(&self.rescore_job_id, "rescore.rescore_job_id")?;
        validate_digest(&self.draft_digest, "rescore.draft_digest")?;
        validate_digest(&self.baseline_score_digest, "rescore.baseline_score_digest")?;
        if !self.independent_rescore {
            return Err(ReviewProjectionError::InvalidField("rescore.independent_rescore"));
        }
        self.scorer.validate()?;
        validate_text(&self.requested_at, "rescore.requested_at")?;
        if self.status.requires_result() != self.result.is_some() {
            return Err(ReviewProjectionError::InvalidField("rescore.result"));
        }
        if let Some(result) = &self.result {
            result.validate()?;
            if result.score_digest == self.baseline_score_digest {
                return Err(ReviewProjectionError::InvalidField("rescore.result.score_digest"));
            }
        }
        let value = serde_json::to_value(self).map_err(serialization_error)?;
        reject_forbidden_fields(&value)
    }

    fn metadata_value_without_digest(&self) -> Result<Value, ReviewProjectionError> {
        let mut value = serde_json::to_value(self).map_err(serialization_error)?;
        value
            .as_object_mut()
            .expect("rescore metadata is an object")
            .remove("metadataDigest");
        Ok(value)
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("rescore metadata is serializable")
    }
}

/// Immutable identity of a repository base revision and its rules/config
/// inputs.  `binding_digest` is SHA-256 over canonical sorted-key JSON of all
/// five identity/input fields, matching the HTTP contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseRulesConfigBinding {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "repositoryDigest")]
    pub repository_digest: String,
    #[serde(rename = "baseRef")]
    pub base_ref: String,
    #[serde(rename = "baseSha")]
    pub base_sha: String,
    #[serde(rename = "rulesDigest")]
    pub rules_digest: String,
    #[serde(rename = "configDigest")]
    pub config_digest: String,
    #[serde(rename = "bindingDigest")]
    pub binding_digest: String,
}

impl BaseRulesConfigBinding {
    pub fn new(
        repository_digest: impl Into<String>,
        base_ref: impl Into<String>,
        base_sha: impl Into<String>,
        rules_digest: impl Into<String>,
        config_digest: impl Into<String>,
    ) -> Result<Self, ReviewProjectionError> {
        let mut value = Self {
            schema_version: BASE_RULES_CONFIG_VERSION.to_owned(),
            repository_digest: repository_digest.into(),
            base_ref: base_ref.into(),
            base_sha: base_sha.into(),
            rules_digest: rules_digest.into(),
            config_digest: config_digest.into(),
            binding_digest: String::new(),
        };
        value.validate_without_binding_digest()?;
        value.binding_digest = value.expected_binding_digest()?;
        Ok(value)
    }

    pub fn expected_binding_digest(&self) -> Result<String, ReviewProjectionError> {
        let value = serde_json::json!({
            "baseRef": self.base_ref,
            "baseSha": self.base_sha,
            "configDigest": self.config_digest,
            "repositoryDigest": self.repository_digest,
            "rulesDigest": self.rules_digest,
        });
        Ok(sha256_digest(canonical_json(&value).as_bytes()))
    }

    pub fn validate(&self) -> Result<(), ReviewProjectionError> {
        self.validate_without_binding_digest()?;
        validate_digest(&self.binding_digest, "base_rules.binding_digest")?;
        if self.binding_digest != self.expected_binding_digest()? {
            return Err(ReviewProjectionError::InvalidField("base_rules.binding_digest"));
        }
        let value = serde_json::to_value(self).map_err(serialization_error)?;
        reject_forbidden_fields(&value)
    }

    fn validate_without_binding_digest(&self) -> Result<(), ReviewProjectionError> {
        if self.schema_version != BASE_RULES_CONFIG_VERSION {
            return Err(ReviewProjectionError::InvalidField("base_rules.schema_version"));
        }
        validate_digest(&self.repository_digest, "base_rules.repository_digest")?;
        validate_base_ref(&self.base_ref)?;
        validate_hex_sha(&self.base_sha, "base_rules.base_sha")?;
        validate_digest(&self.rules_digest, "base_rules.rules_digest")?;
        validate_digest(&self.config_digest, "base_rules.config_digest")
    }

    pub fn stale_against(&self, current: &Self) -> bool {
        self.repository_digest != current.repository_digest
            || self.base_ref != current.base_ref
            || self.base_sha != current.base_sha
            || self.rules_digest != current.rules_digest
            || self.config_digest != current.config_digest
    }

    pub fn ensure_fresh(&self, current: &Self) -> Result<(), ReviewProjectionError> {
        self.validate()?;
        current.validate()?;
        if self.stale_against(current) {
            Err(ReviewProjectionError::StaleRevision)
        } else {
            Ok(())
        }
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("base rules metadata is serializable")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactContext {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "graphDigest")]
    pub graph_digest: String,
    #[serde(rename = "graphRevisionDigest")]
    pub graph_revision_digest: String,
    #[serde(rename = "impactedNodeCount")]
    pub impacted_node_count: u64,
    #[serde(rename = "impactedEdgeCount")]
    pub impacted_edge_count: u64,
    #[serde(rename = "impactDigest")]
    pub impact_digest: String,
}

impl ImpactContext {
    pub fn new(
        graph_digest: impl Into<String>,
        graph_revision_digest: impl Into<String>,
        impacted_node_count: u64,
        impacted_edge_count: u64,
        impact_digest: impl Into<String>,
    ) -> Result<Self, ReviewProjectionError> {
        let value = Self {
            schema_version: IMPACT_CONTEXT_VERSION.to_owned(),
            graph_digest: graph_digest.into(),
            graph_revision_digest: graph_revision_digest.into(),
            impacted_node_count,
            impacted_edge_count,
            impact_digest: impact_digest.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ReviewProjectionError> {
        if self.schema_version != IMPACT_CONTEXT_VERSION {
            return Err(ReviewProjectionError::InvalidField("impact_context.schema_version"));
        }
        validate_digest(&self.graph_digest, "impact_context.graph_digest")?;
        validate_digest(&self.graph_revision_digest, "impact_context.graph_revision_digest")?;
        validate_digest(&self.impact_digest, "impact_context.impact_digest")?;
        let value = serde_json::to_value(self).map_err(serialization_error)?;
        reject_forbidden_fields(&value)
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("impact context is serializable")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactFinding {
    #[serde(rename = "findingId")]
    pub finding_id: String,
    pub severity: FindingSeverity,
    #[serde(rename = "findingDigest")]
    pub finding_digest: String,
}

impl ImpactFinding {
    pub fn new(
        finding_id: impl Into<String>,
        severity: FindingSeverity,
        finding_digest: impl Into<String>,
    ) -> Result<Self, ReviewProjectionError> {
        let value = Self { finding_id: finding_id.into(), severity, finding_digest: finding_digest.into() };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), ReviewProjectionError> {
        validate_text(&self.finding_id, "impact.findings.finding_id")?;
        validate_digest(&self.finding_digest, "impact.findings.finding_digest")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactScoreProjection {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "projectionDigest")]
    pub projection_digest: String,
    #[serde(rename = "findingSetDigest")]
    pub finding_set_digest: String,
    #[serde(rename = "findingCount")]
    pub finding_count: u64,
    #[serde(rename = "severityWeights")]
    pub severity_weights: BTreeMap<String, u64>,
    #[serde(rename = "rawScore")]
    pub raw_score: u64,
    #[serde(rename = "normalizedScore")]
    pub normalized_score: u64,
    #[serde(rename = "scoreLabel")]
    pub score_label: String,
    #[serde(rename = "displayOnly")]
    pub display_only: bool,
    #[serde(rename = "derivedFrom")]
    pub derived_from: String,
    #[serde(rename = "gateDecision")]
    pub gate_decision: String,
}

impl ImpactScoreProjection {
    /// Build a deterministic score from digest-only finding metadata.  The
    /// normalized score is `min(rawScore * 2, 100)`; multiplying gives the
    /// bounded display range while preserving severity ordering.
    pub fn from_findings(findings: &[ImpactFinding]) -> Result<Self, ReviewProjectionError> {
        for finding in findings {
            finding.validate()?;
        }
        let finding_set = serde_json::to_value(findings).map_err(serialization_error)?;
        let finding_set_digest = digest_json(&finding_set)?;
        let raw_score = findings.iter().try_fold(0u64, |total, finding| {
            total.checked_add(finding.severity.weight()).ok_or(ReviewProjectionError::InvalidField("impact.raw_score"))
        })?;
        let normalized_score = raw_score.saturating_mul(2).min(100);
        let score_label = score_label(normalized_score).to_owned();
        let mut value = Self {
            schema_version: SCORE_PROJECTION_VERSION.to_owned(),
            projection_digest: String::new(),
            finding_set_digest,
            finding_count: findings.len() as u64,
            severity_weights: severity_weights(),
            raw_score,
            normalized_score,
            score_label,
            display_only: true,
            derived_from: "findings-only".to_owned(),
            gate_decision: "unchanged".to_owned(),
        };
        value.validate_without_projection_digest()?;
        value.projection_digest = digest_json(&value.projection_value_without_digest()?)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), ReviewProjectionError> {
        self.validate_without_projection_digest()?;
        validate_digest(&self.projection_digest, "impact.projection_digest")?;
        if self.projection_digest != digest_json(&self.projection_value_without_digest()?)? {
            return Err(ReviewProjectionError::InvalidField("impact.projection_digest"));
        }
        Ok(())
    }

    fn validate_without_projection_digest(&self) -> Result<(), ReviewProjectionError> {
        if self.schema_version != SCORE_PROJECTION_VERSION {
            return Err(ReviewProjectionError::InvalidField("impact.schema_version"));
        }
        validate_digest(&self.finding_set_digest, "impact.finding_set_digest")?;
        if self.severity_weights != severity_weights() {
            return Err(ReviewProjectionError::InvalidField("impact.severity_weights"));
        }
        if self.finding_count == 0 && self.raw_score != 0 {
            return Err(ReviewProjectionError::InvalidField("impact.raw_score"));
        }
        if self.normalized_score > 100 || self.normalized_score != self.raw_score.saturating_mul(2).min(100) {
            return Err(ReviewProjectionError::InvalidField("impact.normalized_score"));
        }
        if self.score_label != score_label(self.normalized_score) {
            return Err(ReviewProjectionError::InvalidField("impact.score_label"));
        }
        if !self.display_only || self.derived_from != "findings-only" || self.gate_decision != "unchanged" {
            return Err(ReviewProjectionError::InvalidField("impact.authority"));
        }
        let value = serde_json::to_value(self).map_err(serialization_error)?;
        reject_forbidden_fields(&value)
    }

    fn projection_value_without_digest(&self) -> Result<Value, ReviewProjectionError> {
        let mut value = serde_json::to_value(self).map_err(serialization_error)?;
        value
            .as_object_mut()
            .expect("impact score projection is an object")
            .remove("projectionDigest");
        Ok(value)
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("impact score projection is serializable")
    }
}

/// All optional P2 metadata associated with one local review projection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewProjectionMetadata {
    pub provider_draft: Option<ProviderDraftMetadata>,
    pub rescore_job: Option<ProviderRescoreMetadata>,
    pub base_rules_binding: Option<BaseRulesConfigBinding>,
    pub impact_context: Option<ImpactContext>,
    pub score_projection: Option<ImpactScoreProjection>,
}

impl ReviewProjectionMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn validate(&self) -> Result<(), ReviewProjectionError> {
        if let Some(draft) = &self.provider_draft {
            draft.validate()?;
        }
        if let Some(job) = &self.rescore_job {
            job.validate()?;
            if let Some(draft) = &self.provider_draft {
                if job.draft_digest != draft.projection_digest {
                    return Err(ReviewProjectionError::InvalidField("metadata.rescore_job.draft_digest"));
                }
            }
        }
        if let Some(binding) = &self.base_rules_binding {
            binding.validate()?;
        }
        if let Some(context) = &self.impact_context {
            context.validate()?;
        }
        if let Some(score) = &self.score_projection {
            score.validate()?;
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<Value, ReviewProjectionError> {
        self.validate()?;
        let mut object = Map::new();
        object.insert("schemaVersion".to_owned(), Value::String(REVIEW_METADATA_VERSION.to_owned()));
        if let Some(value) = &self.provider_draft {
            object.insert("providerDraft".to_owned(), value.to_json());
        }
        if let Some(value) = &self.rescore_job {
            object.insert("rescoreJob".to_owned(), value.to_json());
        }
        if let Some(value) = &self.base_rules_binding {
            object.insert("baseRulesBinding".to_owned(), value.to_json());
        }
        if let Some(value) = &self.impact_context {
            object.insert("impactContext".to_owned(), value.to_json());
        }
        if let Some(value) = &self.score_projection {
            object.insert("scoreProjection".to_owned(), value.to_json());
        }
        object.insert("authority".to_owned(), Value::String(PREPARATION_AUTHORITY.to_owned()));
        object.insert("mode".to_owned(), Value::String(PREPARATION_MODE.to_owned()));
        let value_without_digest = Value::Object(object.clone());
        object.insert(
            "projectionDigest".to_owned(),
            Value::String(metadata_digest_json(&value_without_digest)?),
        );
        Ok(Value::Object(object))
    }

    pub fn projection_digest(&self) -> Result<String, ReviewProjectionError> {
        let value = self.to_json()?;
        Ok(value["projectionDigest"].as_str().expect("metadata digest").to_owned())
    }
}

/// Runtime adapter for optional metadata.  It enriches serialized telemetry
/// or dashboard snapshots, leaving the core structs and their authority
/// unchanged.  No method performs I/O or provider/SCM/Gate operations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewMetadataAdapter {
    metadata: ReviewProjectionMetadata,
}

impl ReviewMetadataAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn metadata(&self) -> &ReviewProjectionMetadata {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut ReviewProjectionMetadata {
        &mut self.metadata
    }

    pub fn set_provider_draft(&mut self, value: ProviderDraftMetadata) -> Result<(), ReviewProjectionError> {
        value.validate()?;
        self.metadata.provider_draft = Some(value);
        self.metadata.validate()
    }

    pub fn set_rescore_job(&mut self, value: ProviderRescoreMetadata) -> Result<(), ReviewProjectionError> {
        value.validate()?;
        self.metadata.rescore_job = Some(value);
        self.metadata.validate()
    }

    pub fn set_base_rules_binding(&mut self, value: BaseRulesConfigBinding) -> Result<(), ReviewProjectionError> {
        value.validate()?;
        self.metadata.base_rules_binding = Some(value);
        self.metadata.validate()
    }

    pub fn set_impact_context(&mut self, value: Option<ImpactContext>) -> Result<(), ReviewProjectionError> {
        if let Some(value) = &value {
            value.validate()?;
        }
        self.metadata.impact_context = value;
        self.metadata.validate()
    }

    pub fn set_score_projection(&mut self, value: ImpactScoreProjection) -> Result<(), ReviewProjectionError> {
        value.validate()?;
        self.metadata.score_projection = Some(value);
        self.metadata.validate()
    }

    pub fn to_json(&self) -> Result<Value, ReviewProjectionError> {
        self.metadata.to_json()
    }

    pub fn enrich_telemetry(&self, telemetry: &TelemetryProjection) -> Result<Value, ReviewProjectionError> {
        let mut value = telemetry.to_json();
        value
            .as_object_mut()
            .expect("telemetry projection is an object")
            .insert("reviewMetadata".to_owned(), self.to_json()?);
        Ok(value)
    }

    pub fn enrich_snapshot(&self, snapshot: &DashboardSnapshot) -> Result<Value, ReviewProjectionError> {
        let mut value = snapshot.to_json();
        value
            .as_object_mut()
            .expect("dashboard snapshot is an object")
            .insert("reviewMetadata".to_owned(), self.to_json()?);
        Ok(value)
    }
}

/// Name used by callers that prefer the explicit projection terminology.
pub type ReviewProjectionMetadataAdapter = ReviewMetadataAdapter;

fn severity_weights() -> BTreeMap<String, u64> {
    SCORING_WEIGHTS
        .into_iter()
        .map(|(severity, weight)| (severity.as_str().to_owned(), weight))
        .collect()
}

fn score_label(normalized_score: u64) -> &'static str {
    match normalized_score {
        0 => "NONE",
        1..=24 => "LOW",
        25..=49 => "MEDIUM",
        50..=74 => "HIGH",
        _ => "CRITICAL",
    }
}

fn validate_text(value: &str, field: &'static str) -> Result<(), ReviewProjectionError> {
    if value.is_empty() || value.chars().any(char::is_control) {
        Err(ReviewProjectionError::InvalidField(field))
    } else {
        Ok(())
    }
}

fn validate_digest(value: &str, field: &'static str) -> Result<(), ReviewProjectionError> {
    if valid_digest(value) {
        Ok(())
    } else {
        Err(ReviewProjectionError::InvalidDigest { field, value: value.to_owned() })
    }
}

fn validate_hex_sha(value: &str, field: &'static str) -> Result<(), ReviewProjectionError> {
    if (value.len() == 40 || value.len() == 64)
        && value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(ReviewProjectionError::InvalidField(field))
    }
}

fn validate_base_ref(value: &str) -> Result<(), ReviewProjectionError> {
    validate_text(value, "base_rules.base_ref")?;
    let valid_prefix = ["refs/heads/", "refs/tags/", "refs/remotes/"]
        .iter()
        .any(|prefix| value.starts_with(prefix));
    if valid_prefix && !value.chars().any(char::is_whitespace) {
        Ok(())
    } else {
        Err(ReviewProjectionError::InvalidField("base_rules.base_ref"))
    }
}

fn serialization_error(error: serde_json::Error) -> ReviewProjectionError {
    ReviewProjectionError::Serialization(error.to_string())
}

// Provider draft metadata has a digest-only `findings` collection.  The
// ledger's stricter payload guard treats that key as source-bearing because it
// protects arbitrary event payloads, so this adapter uses a local allow-list
// that permits the metadata collection while still rejecting every source or
// prompt field recursively.
fn reject_metadata_fields(value: &Value) -> Result<(), ReviewProjectionError> {
    const FORBIDDEN: &[&str] = &[
        "source", "sourceCode", "sourceContent", "sourceSnippet", "diff", "patch",
        "rawPrompt", "rawPrompts", "prompt", "body", "comment", "comments", "snippet",
        "fullFinding", "rawFindingPayload",
    ];
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if FORBIDDEN.iter().any(|forbidden| *forbidden == key) {
                    return Err(ReviewProjectionError::ForbiddenField(key.clone()));
                }
                reject_metadata_fields(child)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                reject_metadata_fields(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn metadata_digest_json(value: &Value) -> Result<String, ReviewProjectionError> {
    reject_metadata_fields(value)?;
    Ok(sha256_digest(canonical_json(value).as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn digest(ch: char) -> String {
        format!("sha256:{}", ch.to_string().repeat(64))
    }

    fn finding(
        id: &str,
        severity: FindingSeverity,
    ) -> ImpactFinding {
        ImpactFinding::new(id, severity, digest('f')).unwrap()
    }

    #[test]
    fn provider_draft_and_rescore_are_digest_only_and_bound() {
        let provider_finding = ProviderFindingMetadata::new(
            "finding-1",
            digest('a'),
            digest('b'),
            FindingSeverity::High,
            FindingDisposition::Open,
        )
        .unwrap();
        let draft = ProviderDraftMetadata::new(
            "draft-1",
            Provider::Github,
            digest('c'),
            42,
            digest('d'),
            digest('e'),
            "a".repeat(40),
            "b".repeat(40),
            vec![provider_finding],
        )
        .unwrap();
        assert_eq!(draft.schema_version, PROVIDER_DRAFT_VERSION);
        assert!(draft.validate().is_ok());
        assert_eq!(draft.to_json()["changeRequestId"], "42");

        let scorer = ProviderScorerMetadata::new("local", "reviewer", "1", digest('f')).unwrap();
        let result = RescoreResult::new(digest('1'), digest('2')).unwrap();
        let job = ProviderRescoreMetadata::new(
            "job-1",
            draft.projection_digest.clone(),
            digest('3'),
            scorer,
            "2026-11-06T00:00:00Z",
            RescoreStatus::Succeeded,
            Some(result),
        )
        .unwrap();
        assert!(job.validate().is_ok());
        let mut metadata = ReviewProjectionMetadata::new();
        metadata.provider_draft = Some(draft);
        metadata.rescore_job = Some(job);
        assert!(metadata.to_json().is_ok());
        let mut tampered = metadata.clone();
        tampered.rescore_job.as_mut().unwrap().draft_digest = digest('9');
        assert!(tampered.to_json().is_err());
        assert!(ReviewProjectionMetadata::new()
            .to_json()
            .unwrap()["sourceContent"]
            .is_null());
    }

    #[test]
    fn base_rules_binding_digest_and_stale_policy_are_fail_closed() {
        let binding = BaseRulesConfigBinding::new(
            digest('a'),
            "refs/heads/main",
            "a".repeat(40),
            digest('b'),
            digest('c'),
        )
        .unwrap();
        assert_eq!(binding.binding_digest, binding.expected_binding_digest().unwrap());
        binding.validate().unwrap();
        let current = BaseRulesConfigBinding::new(
            digest('a'),
            "refs/heads/main",
            "b".repeat(40),
            digest('b'),
            digest('c'),
        )
        .unwrap();
        assert!(binding.stale_against(&current));
        assert!(matches!(binding.ensure_fresh(&current), Err(ReviewProjectionError::StaleRevision)));
        let mut broken = binding.clone();
        broken.binding_digest = digest('9');
        assert!(broken.validate().is_err());
    }

    #[test]
    fn impact_score_is_optional_advisory_and_deterministic() {
        let findings = vec![
            finding("info", FindingSeverity::Info),
            finding("high", FindingSeverity::High),
            finding("critical", FindingSeverity::Critical),
        ];
        let score = ImpactScoreProjection::from_findings(&findings).unwrap();
        assert_eq!(score.raw_score, 22);
        assert_eq!(score.normalized_score, 44);
        assert_eq!(score.score_label, "MEDIUM");
        assert!(score.display_only);
        assert_eq!(score.derived_from, "findings-only");
        let context = ImpactContext::new(digest('a'), digest('b'), 4, 6, digest('c')).unwrap();
        let mut metadata = ReviewProjectionMetadata::new();
        metadata.impact_context = Some(context);
        metadata.score_projection = Some(score.clone());
        metadata.to_json().unwrap();
        let without_graph = ReviewProjectionMetadata { score_projection: Some(score), ..Default::default() };
        assert!(without_graph.to_json().is_ok());
    }

    #[test]
    fn adapter_enriches_existing_projection_without_changing_boundary() {
        let binding = BaseRulesConfigBinding::new(
            digest('a'),
            "refs/heads/main",
            "a".repeat(40),
            digest('b'),
            digest('c'),
        )
        .unwrap();
        let mut adapter = ReviewMetadataAdapter::new();
        adapter.set_base_rules_binding(binding).unwrap();
        let value = adapter.to_json().unwrap();
        assert_eq!(value["mode"], PREPARATION_MODE);
        assert_eq!(value["authority"], PREPARATION_AUTHORITY);
        assert!(value["projectionDigest"].as_str().unwrap().starts_with("sha256:"));
        let mut forbidden = adapter.metadata().clone();
        forbidden.base_rules_binding.as_mut().unwrap().base_ref = "refs/heads/main\ndiff".into();
        assert!(forbidden.to_json().is_err());
        assert!(adapter.enrich_snapshot(&fake_snapshot()).is_ok());
    }

    // A minimal snapshot fixture is intentionally assembled through the public
    // constructors so this test remains independent of transport or I/O.
    fn fake_snapshot() -> DashboardSnapshot {
        let telemetry = TelemetryProjection::new(
            super::super::QueueCounts::default(),
            super::super::VersionChain {
                review_run_id: "run".into(),
                base_digest: digest('a'),
                head_digest: digest('b'),
                status: super::super::ReviewRunState::Planned,
                stale: false,
            },
            super::super::FindingSummary::default(),
            super::super::CoverageSummary::default(),
            super::super::UsageSummary {
                duration_ms: 0,
                model_turns: 0,
                tool_calls: 0,
                denied_tool_calls: 0,
                estimated_cost: digest('c'),
            },
            super::super::AuditSummary {
                event_count: 0,
                head_event_digest: super::super::GENESIS_EVENT_DIGEST.into(),
                last_event_digest: super::super::GENESIS_EVENT_DIGEST.into(),
            },
        )
        .unwrap();
        DashboardSnapshot::from_telemetry(&telemetry, 1).unwrap()
    }

    #[test]
    fn forbidden_source_fields_are_rejected_even_when_nested() {
        let value = json!({"providerDraft": {"findings": [{"sourceContent": "secret"}]}});
        assert!(reject_forbidden_fields(&value).is_err());
    }
}
