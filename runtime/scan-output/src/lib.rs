//! Deterministic, privacy-preserving projection of one real Runner execution.
//!
//! This crate is a presentation boundary. It consumes the Runner's already
//! normalized Gate preview and stage facts; it never executes a backend,
//! changes a Gate decision, submits governance requests, or writes SCM state.

use erf_host_client::HostError;
use erf_incremental_baseline::{Stage, StageOutcome, StageTelemetry};
use erf_resolver_client::ResolverError;
use erf_thin_client::{FindingLocation, FindingView, CLIENT_API_VERSION, FINDING_VIEW_KIND};
use rf_gate::{Finding, FindingLocationKind, GateDecision, GatePreview};
use rf_runner::{ScanError, ScanResult};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;

pub const SCAN_OUTPUT_API_VERSION: &str = CLIENT_API_VERSION;
pub const SCAN_RESULT_KIND: &str = "ScanResult";
pub const SCAN_FAILURE_KIND: &str = "ScanFailure";
pub const SCAN_OUTPUT_VERSION: &str = "scan-output-v1";
pub const SARIF_VERSION: &str = "2.1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GateState {
    Pass,
    Fail,
    Inconclusive,
}

impl From<GateDecision> for GateState {
    fn from(value: GateDecision) -> Self {
        match value {
            GateDecision::Pass => Self::Pass,
            GateDecision::Fail => Self::Fail,
            GateDecision::Inconclusive => Self::Inconclusive,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorityState {
    LocalUnattested,
    Trusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DisplayStatus {
    Pass,
    Fail,
    Inconclusive,
    LocalUnattested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RetryClass {
    Never,
    SafeRetry,
    FixInput,
    RequiresTrust,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanDiagnostic {
    pub code: String,
    pub stage: String,
    pub message: String,
    pub retry: RetryClass,
    #[serde(rename = "nextStep")]
    pub next_step: String,
    #[serde(rename = "detailDigest", skip_serializing_if = "Option::is_none")]
    pub detail_digest: Option<String>,
    #[serde(rename = "reasonCodes", skip_serializing_if = "Vec::is_empty")]
    pub reason_codes: Vec<String>,
}

impl ScanDiagnostic {
    pub fn new(
        code: impl Into<String>,
        stage: impl Into<String>,
        message: impl Into<String>,
        retry: RetryClass,
        next_step: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            stage: stage.into(),
            message: message.into(),
            retry,
            next_step: next_step.into(),
            detail_digest: None,
            reason_codes: Vec::new(),
        }
    }

    pub fn with_detail(mut self, detail: &str) -> Self {
        self.detail_digest = Some(digest_text(detail));
        self
    }

    fn with_reasons(mut self, reasons: impl IntoIterator<Item = String>) -> Self {
        self.reason_codes = reasons.into_iter().collect();
        self.reason_codes.sort();
        self.reason_codes.dedup();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProgressEvent {
    pub stage: String,
    pub outcome: String,
    #[serde(rename = "elapsedNanos")]
    pub elapsed_nanos: u64,
    #[serde(rename = "inputDigest")]
    pub input_digest: String,
}

impl ProgressEvent {
    pub fn from_stage(stage: Stage, outcome: StageOutcome, elapsed_nanos: u64, input_digest: &str) -> Self {
        Self {
            stage: stage.code().into(),
            outcome: match outcome {
                StageOutcome::Completed => "COMPLETED",
                StageOutcome::Failed => "FAILED",
                StageOutcome::Skipped => "SKIPPED",
            }
            .into(),
            elapsed_nanos,
            input_digest: input_digest.into(),
        }
    }

    pub fn machine_json(&self) -> Value {
        serde_json::to_value(self).expect("ProgressEvent is serializable")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutputCompleteness {
    pub planned: usize,
    pub accounted: usize,
    pub failed: usize,
    pub missing: usize,
    #[serde(rename = "observationErrors")]
    pub observation_errors: usize,
    pub complete: bool,
}

impl From<&rf_gate::Completeness> for OutputCompleteness {
    fn from(value: &rf_gate::Completeness) -> Self {
        Self {
            planned: value.planned,
            accounted: value.accounted,
            failed: value.failed,
            missing: value.missing,
            observation_errors: value.observation_errors,
            complete: value.complete,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutputLocation {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(rename = "identityDigest")]
    pub identity_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindingRecord {
    #[serde(rename = "findingId")]
    pub finding_id: String,
    #[serde(rename = "ruleId")]
    pub rule_id: String,
    pub fingerprint: String,
    #[serde(rename = "findingDigest")]
    pub finding_digest: String,
    pub message: String,
    pub location: OutputLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanReport {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub status: DisplayStatus,
    #[serde(rename = "gateDecision")]
    pub gate_decision: GateState,
    pub authority: AuthorityState,
    pub trust: String,
    #[serde(rename = "ruleId")]
    pub rule_id: String,
    pub planned: usize,
    pub executed: usize,
    pub failed: usize,
    pub completeness: OutputCompleteness,
    #[serde(rename = "planDigest")]
    pub plan_digest: String,
    #[serde(rename = "effectiveConfigDigest")]
    pub effective_config_digest: String,
    #[serde(rename = "workspaceDescriptorDigest")]
    pub workspace_descriptor_digest: String,
    /// Digest returned by the actual Workspace Resolver. The similarly named
    /// project identity above is the deterministic config projection; this
    /// field binds the emitted result to the descriptor that was consumed by
    /// the Runner.
    #[serde(rename = "resolvedWorkspaceDescriptorDigest")]
    pub resolved_workspace_descriptor_digest: String,
    #[serde(rename = "artifactPath", skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
    #[serde(rename = "bundleDigest", skip_serializing_if = "Option::is_none")]
    pub bundle_digest: Option<String>,
    #[serde(rename = "bundleDeliveryDescriptorDigest", skip_serializing_if = "Option::is_none")]
    pub bundle_delivery_descriptor_digest: Option<String>,
    #[serde(rename = "bundleCompatibilityIdentity", skip_serializing_if = "Option::is_none")]
    pub bundle_compatibility_identity: Option<String>,
    #[serde(rename = "bundlePlatform", skip_serializing_if = "Option::is_none")]
    pub bundle_platform: Option<String>,
    #[serde(rename = "configScanPlanDigest")]
    pub config_scan_plan_digest: String,
    #[serde(rename = "semanticResultDigest")]
    pub semantic_result_digest: String,
    #[serde(rename = "findingViews")]
    pub finding_views: Vec<FindingView>,
    #[serde(rename = "findingRecords")]
    pub finding_records: Vec<FindingRecord>,
    pub diagnostics: Vec<ScanDiagnostic>,
    pub progress: Vec<ProgressEvent>,
    #[serde(rename = "resultDigest")]
    pub result_digest: String,
}

impl ScanReport {
    pub fn from_scan(result: &ScanResult, preview: &GatePreview) -> Result<Self, OutputError> {
        let config_identity = config_identity_from_environment()?;
        Self::from_scan_with_identity(result, preview, config_identity)
    }

    pub fn from_scan_with_identity(
        result: &ScanResult,
        preview: &GatePreview,
        config_identity: (String, String, String),
    ) -> Result<Self, OutputError> {
        validate_config_identity(&config_identity)?;
        let authority = match result.trust {
            "LOCAL_UNATTESTED" => AuthorityState::LocalUnattested,
            "TRUSTED" => AuthorityState::Trusted,
            _ => return Err(OutputError::UnknownTrust(result.trust.into())),
        };
        let gate_decision = GateState::from(preview.decision);
        let status = match preview.decision {
            GateDecision::Inconclusive => DisplayStatus::Inconclusive,
            GateDecision::Pass if authority == AuthorityState::LocalUnattested => DisplayStatus::LocalUnattested,
            GateDecision::Pass => DisplayStatus::Pass,
            GateDecision::Fail => DisplayStatus::Fail,
        };
        let mut finding_records = Vec::with_capacity(preview.findings.len());
        let mut finding_views = Vec::new();
        for finding in &preview.findings {
            let (record, view) = finding_record(finding)?;
            if let Some(view) = view {
                finding_views.push(view);
            }
            finding_records.push(record);
        }
        let progress = stage_progress(result.stage_telemetry.as_ref());
        let mut diagnostics = Vec::new();
        if let Some(failure) = &result.attempt_failure {
            push_diagnostic(
                &mut diagnostics,
                ScanDiagnostic::new(
                    "INCOMPLETE_EXECUTION",
                    failed_stage(result.stage_telemetry.as_ref()),
                    "the scan did not establish a complete execution",
                    RetryClass::SafeRetry,
                    "inspect the diagnostic and rerun after correcting the failed stage",
                )
                .with_detail(failure),
            );
        }
        if !result.resolver_unresolved_facts.is_empty() {
            let required = result.resolver_required_unresolved_facts.len();
            push_diagnostic(
                &mut diagnostics,
                ScanDiagnostic::new(
                    "UNRESOLVED_WORKSPACE_FACTS",
                    "resolver",
                    "the Workspace Resolver reported facts that were not statically determined",
                    RetryClass::FixInput,
                    "provide the required workspace facts or use a verified build export",
                )
                .with_reasons(result.resolver_unresolved_facts.iter().cloned())
                .with_detail(&format!("required_for_selected_backend={required}")),
            );
        }
        if !preview.completeness.complete {
            push_diagnostic(
                &mut diagnostics,
                ScanDiagnostic::new(
                    "INCOMPLETE_REQUIRED_SCOPE",
                    "gate",
                    "required analysis scope is incomplete",
                    RetryClass::SafeRetry,
                    "restore the missing or failed units and rerun the scan",
                ),
            );
        }
        if let Some(plan) = &result.plan_completeness {
            if !plan.allows_authoritative_gate() {
                push_diagnostic(
                    &mut diagnostics,
                    ScanDiagnostic::new(
                        "PLAN_NOT_AUTHORITATIVE",
                        "gate",
                        "the frozen scan plan cannot authorize this result",
                        RetryClass::FixInput,
                        "run the authoritative FULL plan before consuming this result",
                    ),
                );
            }
        }
        if let Some(fallback) = &result.fallback_decision {
            if !fallback.allows_authoritative_gate() {
                push_diagnostic(
                    &mut diagnostics,
                    ScanDiagnostic::new(
                        "INCREMENTAL_DIAGNOSTIC_ONLY",
                        "affected-scope",
                        "incremental feedback is diagnostic-only until an authoritative FULL run completes",
                        RetryClass::RequiresTrust,
                        "consume the fast feedback locally and wait for the authoritative FULL result",
                    )
                    .with_reasons(fallback.reasons.iter().map(|reason| reason.code().to_owned())),
                );
            }
        }
        match preview.decision {
            GateDecision::Fail => push_diagnostic(
                &mut diagnostics,
                ScanDiagnostic::new(
                    "FINDINGS_PRESENT",
                    "gate",
                    "the Gate preview contains unexempted findings",
                    RetryClass::Never,
                    "review findingViews and remediate or submit a governed request",
                ),
            ),
            GateDecision::Inconclusive => push_diagnostic(
                &mut diagnostics,
                ScanDiagnostic::new(
                    "GATE_INCONCLUSIVE",
                    "gate",
                    "the Gate preview is not complete enough to decide",
                    RetryClass::SafeRetry,
                    "resolve the incomplete execution before using the result",
                ),
            ),
            GateDecision::Pass => {}
        }
        if authority == AuthorityState::LocalUnattested {
            push_diagnostic(
                &mut diagnostics,
                ScanDiagnostic::new(
                    "AUTHORITY_LOCAL_UNATTESTED",
                    "trust",
                    "the scan is a local, unattested preview and is not a production Gate proof",
                    RetryClass::RequiresTrust,
                    "use the Trusted CI path when an authoritative Gate result is required",
                ),
            );
        }
        diagnostics.sort_by(|left, right| left.code.cmp(&right.code));
        let mut report = Self {
            api_version: SCAN_OUTPUT_API_VERSION.into(),
            kind: SCAN_RESULT_KIND.into(),
            schema_version: SCAN_OUTPUT_VERSION.into(),
            status,
            gate_decision,
            authority,
            trust: result.trust.into(),
            rule_id: result.rule_id.clone(),
            planned: result.planned,
            executed: result.executed,
            failed: result.failed,
            completeness: (&preview.completeness).into(),
            plan_digest: digest_bytes(&result.plan_digest),
            effective_config_digest: config_identity.0,
            workspace_descriptor_digest: config_identity.1,
            resolved_workspace_descriptor_digest: resolved_descriptor_digest(&result.workspace_descriptor_digest)?,
            artifact_path: None,
            bundle_digest: result.bundle_delivery_identity.as_ref().map(|identity| identity.bundle_digest.clone()),
            bundle_delivery_descriptor_digest: result.bundle_delivery_identity.as_ref().map(|identity| identity.descriptor_digest.clone()),
            bundle_compatibility_identity: result.bundle_delivery_identity.as_ref().map(|identity| identity.compatibility_identity.clone()),
            bundle_platform: result.bundle_delivery_identity.as_ref().map(|identity| identity.platform.clone()),
            config_scan_plan_digest: config_identity.2,
            semantic_result_digest: digest_bytes(&preview.semantic_result_digest),
            finding_views,
            finding_records,
            diagnostics,
            progress,
            result_digest: String::new(),
        };
        report.result_digest = report.digest();
        Ok(report)
    }

    /// Attach the repository-local artifact entrance after output validation.
    /// The path is operational metadata, not scan semantics, so it is omitted
    /// from `resultDigest` below.
    pub fn set_artifact_path(&mut self, path: Option<&str>) {
        self.artifact_path = path.map(str::to_owned);
        self.result_digest = self.digest();
    }

    pub fn digest(&self) -> String {
        let mut value = serde_json::to_value(self).expect("ScanReport is serializable");
        if let Some(object) = value.as_object_mut() {
            object.remove("resultDigest");
            object.remove("artifactPath");
            // Keep stage outcome and input identity in the result digest, but
            // exclude only wall-clock timing so repeated scans remain
            // semantically replayable.
            if let Some(progress) = object.get_mut("progress").and_then(Value::as_array_mut) {
                for event in progress {
                    if let Some(event) = event.as_object_mut() {
                        event.remove("elapsedNanos");
                    }
                }
            }
        }
        digest_value(&value)
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("ScanReport is serializable")
    }

    pub fn to_sarif(&self) -> Value {
        let mut rule_map = BTreeMap::<String, Value>::new();
        for finding in &self.finding_records {
            rule_map.entry(finding.rule_id.clone()).or_insert_with(|| {
                serde_json::json!({
                    "id": finding.rule_id.clone(),
                    "shortDescription": {"text": finding.rule_id.clone()}
                })
            });
        }
        let results = self
            .finding_records
            .iter()
            .map(|finding| {
                let mut result = serde_json::json!({
                    "ruleId": finding.rule_id.clone(),
                    "level": "warning",
                    "message": {"text": finding.message.clone()},
                    "partialFingerprints": {"erfFindingFingerprint": finding.fingerprint.clone()},
                    "properties": {
                        "findingDigest": finding.finding_digest.clone(),
                        "findingId": finding.finding_id.clone(),
                        "erfAuthority": serde_json::to_value(self.authority).expect("authority serializable"),
                        "erfGateDecision": serde_json::to_value(self.gate_decision).expect("gate state serializable")
                    }
                });
                if finding.location.kind == "SOURCE_SPAN" {
                    if let (Some(path), Some(line), Some(column)) = (&finding.location.path, finding.location.line, finding.location.column) {
                        result["locations"] = serde_json::json!([{
                            "physicalLocation": {
                                "artifactLocation": {"uri": path},
                                "region": {"startLine": line, "startColumn": column}
                            }
                        }]);
                    }
                }
                result
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "version": SARIF_VERSION,
            "runs": [{
                "tool": {"driver": {
                    "name": "erf",
                    "informationUri": "https://rulefactory.io",
                    "rules": rule_map.into_values().collect::<Vec<_>>()
                }},
                "results": results,
                "properties": {
                    "erfStatus": serde_json::to_value(self.status).expect("status serializable"),
                    "erfGateDecision": serde_json::to_value(self.gate_decision).expect("gate state serializable"),
                    "erfAuthority": serde_json::to_value(self.authority).expect("authority serializable"),
                    "erfEffectiveConfigDigest": self.effective_config_digest,
                    "erfWorkspaceDescriptorDigest": self.workspace_descriptor_digest,
                    "erfResolvedWorkspaceDescriptorDigest": self.resolved_workspace_descriptor_digest,
                    "erfConfigScanPlanDigest": self.config_scan_plan_digest,
                    "erfSemanticResultDigest": self.semantic_result_digest,
                    "erfBundleDigest": self.bundle_digest,
                    "erfBundleDeliveryDescriptorDigest": self.bundle_delivery_descriptor_digest,
                    "erfBundleCompatibilityIdentity": self.bundle_compatibility_identity,
                    "erfBundlePlatform": self.bundle_platform,
                    "erfArtifactPath": self.artifact_path,
                    "erfResultDigest": self.result_digest,
                    "erfDiagnosticCodes": self.diagnostics.iter().map(|diagnostic| diagnostic.code.clone()).collect::<Vec<_>>()
                }
            }]
        })
    }

    pub fn render_text(&self) -> String {
        let mut lines = vec![
            format!("status={:?} gate={:?} authority={:?} trust={}", self.status, self.gate_decision, self.authority, self.trust),
            format!("rule_id={} planned={} executed={} failed={} complete={}", self.rule_id, self.planned, self.executed, self.failed, self.completeness.complete),
            format!("semantic_result_digest={} result_digest={}", self.semantic_result_digest, self.result_digest),
        ];
        lines.push(format!("effective_config_digest={}", self.effective_config_digest));
        lines.push(format!("workspace_descriptor_digest={}", self.workspace_descriptor_digest));
        lines.push(format!("resolved_workspace_descriptor_digest={}", self.resolved_workspace_descriptor_digest));
        if let Some(path) = &self.artifact_path {
            lines.push(format!("artifact_path={path}"));
        }
        if let Some(bundle_digest) = &self.bundle_digest {
            lines.push(format!("bundle_digest={bundle_digest}"));
            lines.push(format!("bundle_delivery_descriptor_digest={}", self.bundle_delivery_descriptor_digest.as_deref().unwrap_or("")));
            lines.push(format!("bundle_compatibility_identity={}", self.bundle_compatibility_identity.as_deref().unwrap_or("")));
            lines.push(format!("bundle_platform={}", self.bundle_platform.as_deref().unwrap_or("")));
        }
        lines.push(format!("config_scan_plan_digest={}", self.config_scan_plan_digest));
        for event in &self.progress {
            lines.push(format!("progress stage={} outcome={} elapsed_nanos={} input_digest={}", event.stage, event.outcome, event.elapsed_nanos, event.input_digest));
        }
        for finding in &self.finding_records {
            let location = match (&finding.location.path, finding.location.line, finding.location.column) {
                (Some(path), Some(line), Some(column)) => format!("{path}:{line}:{column}"),
                (Some(path), _, _) => path.clone(),
                (None, _, _) => format!("{}:identity={}", finding.location.kind, finding.location.identity_digest),
            };
            lines.push(format!("finding id={} rule={} location={} digest={} message={}", finding.finding_id, finding.rule_id, location, finding.finding_digest, finding.message));
        }
        for diagnostic in &self.diagnostics {
            lines.push(format!("diagnostic code={} stage={} retry={:?} message={} next_step={}", diagnostic.code, diagnostic.stage, diagnostic.retry, diagnostic.message, diagnostic.next_step));
        }
        lines.join("\n") + "\n"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScanFailure {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    #[serde(rename = "exitCode")]
    pub exit_code: i32,
    pub diagnostics: Vec<ScanDiagnostic>,
    pub progress: Vec<ProgressEvent>,
    #[serde(rename = "diagnosticDigest")]
    pub diagnostic_digest: String,
}

impl ScanFailure {
    pub fn from_diagnostic(diagnostic: ScanDiagnostic, exit_code: i32) -> Self {
        let progress = vec![ProgressEvent {
            stage: failure_progress_stage(&diagnostic.stage),
            outcome: "FAILED".into(),
            elapsed_nanos: 0,
            input_digest: digest_text(&diagnostic.code),
        }];
        let mut failure = Self {
            api_version: SCAN_OUTPUT_API_VERSION.into(),
            kind: SCAN_FAILURE_KIND.into(),
            schema_version: SCAN_OUTPUT_VERSION.into(),
            exit_code,
            diagnostics: vec![diagnostic],
            progress,
            diagnostic_digest: String::new(),
        };
        failure.diagnostic_digest = digest_value(&serde_json::json!({
            "kind": failure.kind.clone(),
            "exitCode": failure.exit_code,
            "diagnostics": failure.diagnostics.clone()
        }));
        failure
    }

    pub fn from_scan_error(error: &ScanError, exit_code: i32) -> Self {
        Self::from_diagnostic(diagnostic_from_scan_error(error), exit_code)
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("ScanFailure is serializable")
    }

    pub fn to_sarif(&self) -> Value {
        serde_json::json!({
            "version": SARIF_VERSION,
            "runs": [{
                "tool": {"driver": {"name": "erf"}},
                "results": [],
                "invocations": [{
                    "executionSuccessful": false,
                    "properties": {
                        "erfExitCode": self.exit_code,
                        "erfDiagnosticDigest": self.diagnostic_digest,
                        "erfDiagnosticCodes": self.diagnostics.iter().map(|diagnostic| diagnostic.code.clone()).collect::<Vec<_>>(),
                        "erfDiagnostics": self.diagnostics
                    }
                }],
                "properties": {
                    "erfStatus": "ERROR",
                    "erfExitCode": self.exit_code,
                    "erfDiagnosticDigest": self.diagnostic_digest
                }
            }]
        })
    }

    pub fn render_text(&self) -> String {
        let diagnostic = &self.diagnostics[0];
        let progress = self
            .progress
            .iter()
            .map(|event| format!("progress stage={} outcome={} elapsed_nanos={} input_digest={}\n", event.stage, event.outcome, event.elapsed_nanos, event.input_digest))
            .collect::<String>();
        format!(
            "status=ERROR exit_code={} diagnostic code={} stage={} retry={:?} message={} next_step={} diagnostic_digest={}\n{}",
            self.exit_code, diagnostic.code, diagnostic.stage, diagnostic.retry, diagnostic.message, diagnostic.next_step, self.diagnostic_digest, progress
        )
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OutputError {
    #[error("unknown execution trust level: {0}")]
    UnknownTrust(String),
    #[error("finding cannot be projected to a digest-only output: {0}")]
    InvalidFinding(&'static str),
    #[error("output serialization failed: {0}")]
    Serialization(String),
    #[error("required project identity is missing or invalid: {0}")]
    MissingConfigIdentity(&'static str),
    #[error("resolved WorkspaceDescriptor digest must be exactly 32 bytes")]
    InvalidWorkspaceDescriptorDigest,
}

fn finding_record(finding: &Finding) -> Result<(FindingRecord, Option<FindingView>), OutputError> {
    let kind = match finding.location_kind {
        FindingLocationKind::SourceSpan => "SOURCE_SPAN",
        FindingLocationKind::Symbol => "SYMBOL",
        FindingLocationKind::BytecodeMember => "BYTECODE_MEMBER",
        FindingLocationKind::File => "FILE",
    };
    let source_view = match (finding.location_kind, finding.line, finding.column) {
        (FindingLocationKind::SourceSpan, Some(line), Some(column)) => Some(FindingView {
            api_version: CLIENT_API_VERSION.into(),
            kind: FINDING_VIEW_KIND.into(),
            finding_id: finding.fingerprint.clone(),
            rule_id: finding.rule_id.clone(),
            fingerprint: finding.fingerprint.clone(),
            message: finding.message.clone(),
            location: FindingLocation { path: finding.path.clone(), line, column },
        }),
        (FindingLocationKind::SourceSpan, _, _) => return Err(OutputError::InvalidFinding("source finding has no positive coordinates")),
        _ => None,
    };
    let finding_digest = source_view
        .as_ref()
        .map(FindingView::digest)
        .unwrap_or_else(|| digest_text(&format!("{}\0{}\0{}\0{}", finding.fingerprint, finding.rule_id, finding.semantic_anchor, finding.message)));
    let record = FindingRecord {
        finding_id: finding.fingerprint.clone(),
        rule_id: finding.rule_id.clone(),
        fingerprint: finding.fingerprint.clone(),
        finding_digest,
        message: finding.message.clone(),
        location: OutputLocation {
            kind: kind.into(),
            path: matches!(finding.location_kind, FindingLocationKind::SourceSpan | FindingLocationKind::File).then(|| finding.path.clone()),
            line: finding.line,
            column: finding.column,
            identity_digest: digest_text(&finding.semantic_anchor),
        },
    };
    Ok((record, source_view))
}

fn stage_progress(telemetry: Option<&StageTelemetry>) -> Vec<ProgressEvent> {
    telemetry
        .map(|telemetry| {
            telemetry
                .measurements()
                .iter()
                .map(|measurement| ProgressEvent::from_stage(measurement.stage, measurement.outcome, measurement.elapsed_nanos, &measurement.input_digest))
                .collect()
        })
        .unwrap_or_default()
}

fn config_identity_from_environment() -> Result<(String, String, String), OutputError> {
    let value = |name: &'static str| {
        let value = std::env::var(name).map_err(|_| OutputError::MissingConfigIdentity(name))?;
        if !value.starts_with("sha256:") || value.len() != "sha256:".len() + 64 || !value["sha256:".len()..].bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(OutputError::MissingConfigIdentity(name));
        }
        Ok(value)
    };
    let identity = (
        value("RULEOPS_EFFECTIVE_CONFIG_DIGEST")?,
        value("RULEOPS_WORKSPACE_DESCRIPTOR_DIGEST")?,
        value("RULEOPS_SCAN_PLAN_DIGEST")?,
    );
    validate_config_identity(&identity)?;
    Ok(identity)
}

fn validate_config_identity(identity: &(String, String, String)) -> Result<(), OutputError> {
    for (name, value) in [
        ("effectiveConfigDigest", &identity.0),
        ("workspaceDescriptorDigest", &identity.1),
        ("configScanPlanDigest", &identity.2),
    ] {
        if !value.starts_with("sha256:") || value.len() != "sha256:".len() + 64 || !value["sha256:".len()..].bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(OutputError::MissingConfigIdentity(name));
        }
    }
    Ok(())
}

fn failure_progress_stage(stage: &str) -> String {
    match stage {
        "resolve" | "affected-scope" | "compile" | "backend" | "cache" | "gate" => stage.into(),
        // A ScanFailure can happen before Runner stage execution (input,
        // trust, sandbox, or output). Its diagnostic retains the precise
        // category; the terminal progress event uses the closed stage set.
        _ => "gate".into(),
    }
}

fn failed_stage(telemetry: Option<&StageTelemetry>) -> String {
    telemetry
        .and_then(|telemetry| telemetry.measurements().iter().find(|measurement| measurement.outcome == StageOutcome::Failed).map(|measurement| measurement.stage.code().to_owned()))
        .unwrap_or_else(|| "backend".into())
}

fn push_diagnostic(diagnostics: &mut Vec<ScanDiagnostic>, diagnostic: ScanDiagnostic) {
    if !diagnostics.iter().any(|existing| existing.code == diagnostic.code) {
        diagnostics.push(diagnostic);
    }
}

fn diagnostic_from_scan_error(error: &ScanError) -> ScanDiagnostic {
    match error {
        ScanError::Trust(detail) => ScanDiagnostic::new(
            "TRUST_FAILURE",
            "trust",
            "execution trust verification failed",
            RetryClass::RequiresTrust,
            "use a valid signed execution lock and current trust metadata",
        )
        .with_detail(detail),
        ScanError::Sandbox(detail) => ScanDiagnostic::new(
            "SANDBOX_FAILURE",
            "sandbox",
            "the required sandbox could not be established",
            RetryClass::FixInput,
            "repair the sandbox provider or run only in the explicitly local profile",
        )
        .with_detail(detail),
        ScanError::RuleBundle(detail) => ScanDiagnostic::new(
            "RULE_BUNDLE_INVALID",
            "input",
            "the supplied RuleBundle/Candidate is invalid",
            RetryClass::FixInput,
            "regenerate or validate the Candidate before scanning",
        )
        .with_detail(detail),
        ScanError::Delivery(detail) => delivery_diagnostic(detail),
        ScanError::Plan(detail) => ScanDiagnostic::new(
            "PLAN_INVALID",
            "input",
            "the scan plan or execution completeness contract is invalid",
            RetryClass::FixInput,
            "freeze a valid plan and retry the scan",
        )
        .with_detail(detail),
        ScanError::Resolve(error) => resolver_diagnostic(error),
        ScanError::Host(error) => host_diagnostic(error),
    }
}

fn delivery_diagnostic(detail: &str) -> ScanDiagnostic {
    let (class, raw_detail) = detail
        .strip_prefix("delivery-class=")
        .and_then(|value| value.split_once("; "))
        .map_or((None, detail), |(class, detail)| (Some(class), detail));
    if class == Some("trust") {
        return delivery_trust_diagnostic(raw_detail);
    }
    if class == Some("input") {
        return delivery_input_diagnostic(raw_detail);
    }
    if class == Some("fetch") {
        return delivery_fetch_diagnostic(raw_detail);
    }
    if raw_detail.contains("SERVER_FETCH_REQUIRED") || raw_detail.contains("connected runner must fetch") {
        return delivery_fetch_diagnostic(raw_detail);
    }
    if delivery_detail_is_trust_related(raw_detail) {
        return delivery_trust_diagnostic(raw_detail);
    }
    if delivery_detail_is_input_related(raw_detail) {
        return delivery_input_diagnostic(raw_detail);
    }
    ScanDiagnostic::new(
        "DELIVERY_INCONCLUSIVE",
        "delivery",
        "RuleBundle delivery could not establish a consumable bundle",
        RetryClass::SafeRetry,
        "restore the required delivery or cache inputs and retry the scan",
    )
    .with_detail(raw_detail)
}

fn delivery_fetch_diagnostic(detail: &str) -> ScanDiagnostic {
    ScanDiagnostic::new(
        "DELIVERY_SERVER_FETCH_REQUIRED",
        "delivery",
        "the connected delivery cache missed and requires a Server fetch",
        RetryClass::SafeRetry,
        "restore the RuleOps Server connection and retry the scan",
    )
    .with_detail(detail)
}

fn delivery_trust_diagnostic(detail: &str) -> ScanDiagnostic {
    ScanDiagnostic::new(
        "DELIVERY_TRUST_INCONCLUSIVE",
        "trust",
        "RuleBundle delivery cannot establish the required trust closure",
        RetryClass::RequiresTrust,
        "restore valid signature, trust metadata, revocation, Pack, and execution identity evidence",
    )
    .with_detail(detail)
}

fn delivery_input_diagnostic(detail: &str) -> ScanDiagnostic {
    ScanDiagnostic::new(
        "DELIVERY_INVALID",
        "input",
        "the RuleBundle delivery descriptor or endpoint is invalid",
        RetryClass::FixInput,
        "repair the delivery descriptor or endpoint and retry the scan",
    )
    .with_detail(detail)
}

fn delivery_detail_is_trust_related(detail: &str) -> bool {
    [
        "SIGNATURE_INVALID",
        "TRUST_METADATA_UNAVAILABLE",
        "REVOCATION_SNAPSHOT_UNAVAILABLE",
        "REFERENCED_PACK_UNAVAILABLE",
        "TRUSTED_CONTEXT_INVALID",
        "verification context is required",
        "signed RuleBundle verification failed",
        "runtime Pack verification failed",
        "execution identity",
        "revocation snapshot",
        "signature",
    ]
    .iter()
    .any(|marker| detail.contains(marker))
}

fn delivery_detail_is_input_related(detail: &str) -> bool {
    [
        "invalid delivery descriptor",
        "invalid delivery endpoint",
        "delivery response is invalid",
        "descriptor does not match",
        "compatibility or platform does not match",
        "input does not match descriptor",
    ]
    .iter()
    .any(|marker| detail.contains(marker))
}

fn resolver_diagnostic(error: &ResolverError) -> ScanDiagnostic {
    match error {
        ResolverError::DeadlineExceeded { deadline_ms } => ScanDiagnostic::new(
            "TIMEOUT",
            "resolve",
            "workspace resolution exceeded its deadline",
            RetryClass::SafeRetry,
            "increase the resolver deadline or inspect the workspace resolver",
        )
        .with_detail(&deadline_ms.to_string()),
        ResolverError::Protocol { .. } | ResolverError::MajorMismatch { .. } | ResolverError::MissingCapabilities { .. } | ResolverError::Uncorrelatable { .. } | ResolverError::Violation(_) => ScanDiagnostic::new(
            "RESOLVER_CONTRACT",
            "resolve",
            "workspace resolver violated the versioned protocol",
            RetryClass::FixInput,
            "use a resolver with the required protocol and capabilities",
        )
        .with_detail(&error.to_string()),
        ResolverError::Spawn(_) | ResolverError::Transport(_) | ResolverError::Hangup { .. } | ResolverError::Io(_) => ScanDiagnostic::new(
            "RESOLVER_UNAVAILABLE",
            "resolve",
            "workspace resolver was unavailable",
            RetryClass::SafeRetry,
            "check the resolver executable and rerun the scan",
        )
        .with_detail(&error.to_string()),
    }
}

fn host_diagnostic(error: &HostError) -> ScanDiagnostic {
    match error {
        HostError::DeadlineExceeded { deadline_ms } => ScanDiagnostic::new(
            "TIMEOUT",
            "backend",
            "backend execution exceeded its deadline",
            RetryClass::SafeRetry,
            "inspect the backend or increase the explicit execution deadline",
        )
        .with_detail(&deadline_ms.to_string()),
        HostError::Protocol { .. } | HostError::MajorMismatch { .. } | HostError::MissingCapabilities { .. } | HostError::Uncorrelatable { .. } | HostError::EventFlood { .. } | HostError::Violation(_) => ScanDiagnostic::new(
            "HOST_CONTRACT",
            "backend",
            "backend host violated the versioned protocol",
            RetryClass::FixInput,
            "use a compatible Backend Host and inspect its structured stderr",
        )
        .with_detail(&error.to_string()),
        HostError::Spawn(_) | HostError::Transport(_) | HostError::Hangup { .. } | HostError::Io(_) => ScanDiagnostic::new(
            "HOST_UNAVAILABLE",
            "backend",
            "backend host was unavailable",
            RetryClass::SafeRetry,
            "check the host executable and rerun the scan",
        )
        .with_detail(&error.to_string()),
    }
}

pub fn digest_text(text: &str) -> String {
    digest_bytes(text.as_bytes())
}

pub fn digest_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex(&Sha256::digest(bytes)))
}

fn resolved_descriptor_digest(bytes: &[u8]) -> Result<String, OutputError> {
    if bytes.len() != 32 {
        return Err(OutputError::InvalidWorkspaceDescriptorDigest);
    }
    // The Runner already returns the SHA-256 digest of the canonical
    // WorkspaceDescriptor. Preserve that identity; hashing the digest bytes
    // again would emit a different, non-resolvable identity.
    Ok(format!("sha256:{}", hex(bytes)))
}

fn digest_value(value: &Value) -> String {
    let canonical = canonical_json(value);
    let bytes = serde_json::to_vec(&canonical).expect("canonical JSON is serializable");
    digest_bytes(&bytes)
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort_unstable();
            let mut result = Map::new();
            for key in keys {
                result.insert(key.clone(), canonical_json(&object[key]));
            }
            Value::Object(result)
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical_json).collect()),
        other => other.clone(),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use erf_incremental_baseline::{StageMeasurement, StageTelemetry};

    fn finding() -> Finding {
        Finding {
            fingerprint_version: rf_gate::FINGERPRINT_VERSION,
            fingerprint: "finding-v0:stable".into(),
            rule_id: "corp.java.sample".into(),
            location_kind: FindingLocationKind::SourceSpan,
            path: "src/Main.java".into(),
            line: Some(12),
            column: Some(5),
            semantic_anchor: "source-span|src/Main.java|10|20".into(),
            message: "System.exit is forbidden".into(),
        }
    }

    fn preview() -> GatePreview {
        let finding = finding();
        GatePreview {
            decision: GateDecision::Fail,
            findings: vec![finding.clone()],
            completeness: rf_gate::Completeness { planned: 1, accounted: 1, failed: 0, missing: 0, observation_errors: 0, complete: true },
            canonical_set_version: rf_gate::CANONICAL_SET_VERSION,
            trust: "LOCAL_UNATTESTED",
            input_summary: "test".into(),
            assessments: vec![rf_gate::FindingAssessment { fingerprint: finding.fingerprint, baseline: rf_gate::BaselineState::New, exception: rf_gate::ExceptionDecision::NotApplicable }],
            semantic_result_digest: [7; 32],
        }
    }

    fn result() -> ScanResult {
        let mut telemetry = StageTelemetry::new();
        for stage in Stage::ALL {
            telemetry.record(StageMeasurement::new(stage, 1, "a".repeat(64)).unwrap()).unwrap();
        }
        ScanResult {
            rule_id: "corp.java.sample".into(),
            planned: 1,
            executed: 1,
            failed: 0,
            attempt_failure: None,
            committed_findings: Vec::new(),
            attempt_transcript_digest: vec![1; 32],
            trust: "LOCAL_UNATTESTED",
            plan_digest: vec![2; 32],
            workspace_descriptor_digest: vec![3; 32],
            resolver_unresolved_facts: Vec::new(),
            resolver_required_unresolved_facts: Vec::new(),
            planned_unit_ids: vec!["unit".into()],
            unit_statuses: vec![],
            scan_plan: None,
            plan_authority: None,
            plan_completeness: None,
            fallback_decision: None,
            stage_telemetry: Some(telemetry),
            bundle_delivery_identity: None,
        }
    }

    fn make_report() -> ScanReport {
        ScanReport::from_scan_with_identity(
            &result(),
            &preview(),
            ("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
             "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
             "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".into()),
        )
        .unwrap()
    }

    #[test]
    fn real_scan_projection_contains_location_and_stable_identity() {
        let report = make_report();
        assert_eq!(report.finding_views.len(), 1);
        assert_eq!(report.finding_records[0].location.path.as_deref(), Some("src/Main.java"));
        assert_eq!(report.finding_records[0].location.line, Some(12));
        assert_eq!(report.finding_records[0].location.column, Some(5));
        assert_eq!(report.finding_records[0].finding_id, "finding-v0:stable");
        assert_eq!(report.digest(), report.result_digest);
        assert_eq!(report.digest(), make_report().digest());
        assert_eq!(report.resolved_workspace_descriptor_digest, format!("sha256:{}", "03".repeat(32)));
    }

    #[test]
    fn artifact_path_is_an_explicit_non_semantic_output_entrance() {
        let mut report = make_report();
        let semantic_digest = report.digest();
        report.set_artifact_path(Some(".ruleops/scan.json"));
        assert_eq!(report.artifact_path.as_deref(), Some(".ruleops/scan.json"));
        assert_eq!(report.result_digest, semantic_digest);
        assert_eq!(report.to_json()["artifactPath"], ".ruleops/scan.json");
    }


    #[test]
    fn status_and_reason_boundary_is_explicit() {
        let report = make_report();
        assert_eq!(report.status, DisplayStatus::Fail);
        assert_eq!(report.gate_decision, GateState::Fail);
        assert_eq!(report.authority, AuthorityState::LocalUnattested);
        let codes: Vec<_> = report.diagnostics.iter().map(|diagnostic| diagnostic.code.as_str()).collect();
        assert!(codes.contains(&"AUTHORITY_LOCAL_UNATTESTED"));
        assert!(codes.contains(&"FINDINGS_PRESENT"));
    }

    #[test]
    fn machine_progress_is_closed_and_sarif_has_no_source_content() {
        let report = make_report();
        assert_eq!(report.progress.len(), 6);
        let sarif = report.to_sarif().to_string();
        assert!(sarif.contains("src/Main.java"));
        assert!(!sarif.contains("sourceCode"));
        assert!(!sarif.contains("System.exit is forbidden\n"));
        assert_eq!(report.finding_views[0].location.path, "src/Main.java");
    }

    #[test]
    fn failures_get_stable_stage_retry_and_next_step() {
        let error = ScanError::Host(HostError::DeadlineExceeded { deadline_ms: 100 });
        let failure = ScanFailure::from_scan_error(&error, 3);
        assert_eq!(failure.diagnostics[0].code, "TIMEOUT");
        assert_eq!(failure.diagnostics[0].stage, "backend");
        assert_eq!(failure.diagnostics[0].retry, RetryClass::SafeRetry);
        assert!(!failure.diagnostics[0].next_step.is_empty());
        assert_eq!(failure.progress[0].outcome, "FAILED");
    }

    #[test]
    fn delivery_failures_keep_availability_trust_and_input_distinct() {
        let fetch = ScanFailure::from_scan_error(
            &ScanError::Delivery("connected runner must fetch the missing bundle from the server".into()),
            3,
        );
        assert_eq!(fetch.diagnostics[0].code, "DELIVERY_SERVER_FETCH_REQUIRED");
        assert_eq!(fetch.diagnostics[0].retry, RetryClass::SafeRetry);

        let trust = ScanFailure::from_scan_error(
            &ScanError::Delivery("delivery resolution failed: SIGNATURE_INVALID".into()),
            4,
        );
        assert_eq!(trust.diagnostics[0].code, "DELIVERY_TRUST_INCONCLUSIVE");
        assert_eq!(trust.diagnostics[0].retry, RetryClass::RequiresTrust);

        let input = ScanFailure::from_scan_error(
            &ScanError::Delivery("invalid delivery descriptor field: platform".into()),
            2,
        );
        assert_eq!(input.diagnostics[0].code, "DELIVERY_INVALID");
        assert_eq!(input.diagnostics[0].retry, RetryClass::FixInput);

        let prefixed_trust = ScanFailure::from_scan_error(
            &ScanError::Delivery("delivery-class=trust; implementation wording changed".into()),
            4,
        );
        assert_eq!(prefixed_trust.diagnostics[0].code, "DELIVERY_TRUST_INCONCLUSIVE");

        let prefixed_input = ScanFailure::from_scan_error(
            &ScanError::Delivery("delivery-class=input; implementation wording changed".into()),
            2,
        );
        assert_eq!(prefixed_input.diagnostics[0].code, "DELIVERY_INVALID");

        let prefixed_fetch = ScanFailure::from_scan_error(
            &ScanError::Delivery("delivery-class=fetch; implementation wording changed".into()),
            3,
        );
        assert_eq!(prefixed_fetch.diagnostics[0].code, "DELIVERY_SERVER_FETCH_REQUIRED");

        let descriptor_mismatch = ScanFailure::from_scan_error(
            &ScanError::Delivery("delivery-class=input; compatibility or platform does not match".into()),
            2,
        );
        assert_eq!(descriptor_mismatch.diagnostics[0].code, "DELIVERY_INVALID");
    }

    #[test]
    fn failure_sarif_is_a_sarif_document_and_terminal_stage_is_closed() {
        let failure = ScanFailure::from_scan_error(&ScanError::Trust("bad signature".into()), 4);
        let sarif = failure.to_sarif();
        assert_eq!(sarif["version"], SARIF_VERSION);
        assert!(sarif["runs"][0]["results"].as_array().unwrap().is_empty());
        assert_eq!(sarif["runs"][0]["invocations"][0]["executionSuccessful"], false);
        assert_eq!(failure.diagnostics[0].stage, "trust");
        assert_eq!(failure.progress[0].stage, "gate");
    }

    #[test]
    fn result_digest_excludes_only_elapsed_time() {
        let report = make_report();
        let mut elapsed_changed = report.clone();
        elapsed_changed.progress[0].elapsed_nanos += 99;
        assert_eq!(elapsed_changed.digest(), report.digest());
        elapsed_changed.progress[0].input_digest = digest_text("different-stage-input");
        assert_ne!(elapsed_changed.digest(), report.digest());
    }
}
