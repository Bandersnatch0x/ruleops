mod delivery;

use delivery::load_delivery;
use erf_incremental_baseline::{digest_summary, Stage, StageTimer};
use erf_project_config::{apply_precedence, load_config, process_environment, validate_output_path, validate_workspace_paths, CONFIG_RELATIVE_PATH};
use erf_scan_output::{ScanDiagnostic, ScanFailure, ScanReport, RetryClass};
use erf_trust::{SignedAsset, TrustStore, DSSE_PAYLOAD_TYPE};
use rf_runner::{scan, ExecutionLock, IncrementalScanRequest, ProjectScanScope, ScanConfig, ScanError, SignedExecutionLock};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use sha2::{Digest, Sha256};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
    Sarif,
}

impl OutputFormat {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "sarif" => Ok(Self::Sarif),
            _ => Err(format!("unsupported --format: {value}; expected text, json, or sarif")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProgressMode {
    Text,
    Json,
    None,
}

impl ProgressMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "none" => Ok(Self::None),
            _ => Err(format!("unsupported --progress: {value}; expected text, json, or none")),
        }
    }
}

fn scan_error_exit_code(error: &ScanError) -> i32 {
    match error {
        ScanError::Trust(_) => 4,
        ScanError::Resolve(error) => resolver_error_exit_code(error),
        ScanError::Host(error) => host_error_exit_code(error),
        ScanError::Sandbox(_) => 3,
        ScanError::Delivery(detail) => delivery_error_exit_code(detail),
        ScanError::Plan(_) | ScanError::RuleBundle(_) => 2,
    }
}

fn delivery_error_exit_code(detail: &str) -> i32 {
    if detail.starts_with("delivery-class=trust;") {
        return 4;
    }
    if detail.starts_with("delivery-class=input;") {
        return 2;
    }
    if detail.starts_with("delivery-class=inconclusive;") {
        return 3;
    }
    if detail.starts_with("delivery-class=fetch;") {
        return 3;
    }
    if [
        "SIGNATURE_INVALID",
        "TRUST_METADATA_UNAVAILABLE",
        "REVOCATION_SNAPSHOT_UNAVAILABLE",
        "REFERENCED_PACK_UNAVAILABLE",
        "TRUSTED_CONTEXT_INVALID",
        "verification context is required",
        "signed RuleBundle verification failed",
        "runtime Pack verification failed",
        "delivery scans require an active Execution identity",
        "revocation snapshot",
    ]
    .iter()
    .any(|marker| detail.contains(marker)) {
        return 4;
    }
    if [
        "invalid delivery descriptor",
        "invalid delivery endpoint",
        "delivery response is invalid",
        "descriptor does not match",
        "compatibility or platform does not match",
        "input does not match descriptor",
    ]
    .iter()
    .any(|marker| detail.contains(marker)) {
        return 2;
    }
    3
}

fn resolver_error_exit_code(error: &erf_resolver_client::ResolverError) -> i32 {
    match error {
        erf_resolver_client::ResolverError::Protocol { .. }
        | erf_resolver_client::ResolverError::MajorMismatch { .. }
        | erf_resolver_client::ResolverError::MissingCapabilities { .. }
        | erf_resolver_client::ResolverError::Uncorrelatable { .. }
        | erf_resolver_client::ResolverError::Violation(_) => 2,
        erf_resolver_client::ResolverError::Spawn(_)
        | erf_resolver_client::ResolverError::Transport(_)
        | erf_resolver_client::ResolverError::DeadlineExceeded { .. }
        | erf_resolver_client::ResolverError::Hangup { .. }
        | erf_resolver_client::ResolverError::Io(_) => 3,
    }
}

fn host_error_exit_code(error: &erf_host_client::HostError) -> i32 {
    match error {
        erf_host_client::HostError::Protocol { .. }
        | erf_host_client::HostError::MajorMismatch { .. }
        | erf_host_client::HostError::MissingCapabilities { .. }
        | erf_host_client::HostError::Uncorrelatable { .. }
        | erf_host_client::HostError::EventFlood { .. }
        | erf_host_client::HostError::Violation(_) => 2,
        erf_host_client::HostError::Spawn(_)
        | erf_host_client::HostError::Transport(_)
        | erf_host_client::HostError::DeadlineExceeded { .. }
        | erf_host_client::HostError::Hangup { .. }
        | erf_host_client::HostError::Io(_) => 3,
    }
}

fn emit_progress(report: &ScanReport, mode: ProgressMode) {
    if mode != ProgressMode::Json {
        return;
    }
    for event in &report.progress {
        let value = serde_json::json!({
            "apiVersion": erf_scan_output::SCAN_OUTPUT_API_VERSION,
            "kind": "ScanProgress",
            "schemaVersion": erf_scan_output::SCAN_OUTPUT_VERSION,
            "event": event,
        });
        eprintln!("{}", serde_json::to_string(&value).expect("progress event is serializable"));
    }
}

fn emit_report(
    result: &rf_runner::ScanResult,
    gate: &rf_gate::GatePreview,
    report: &ScanReport,
    format: OutputFormat,
    output: Option<&str>,
) -> Result<(), String> {
    let text = match format {
        OutputFormat::Text => {
            // Keep the established V0.1 lines stable for existing CI wrappers;
            // the structured projection follows them in the same stream.
            let legacy = format!(
                "rule_id={}\nplanned={} executed={} failed={} attempt_failure={:?}\nplan_digest={:x?}\nfindings={} trust={} attempt_transcript_digest={:x?}\ngate={:?} completeness={:?} canonical_set={} semantic_result_digest={:x?} inputs={}\n{}{}",
                result.rule_id,
                result.planned,
                result.executed,
                result.failed,
                result.attempt_failure,
                result.plan_digest,
                gate.findings.len(),
                result.trust,
                result.attempt_transcript_digest,
                gate.decision,
                gate.completeness,
                gate.canonical_set_version,
                gate.semantic_result_digest,
                gate.input_summary,
                result.plan_completeness.as_ref().map(|plan| format!("scan_plan_mode={:?} authority={:?} coverage={:?} plan_digest={}\n", plan.mode(), plan.authority(), plan.state(), plan.plan_digest())).unwrap_or_default(),
                result.fallback_decision.as_ref().filter(|decision| !decision.allows_authoritative_gate()).map(|_| "incremental_route=DIAGNOSTIC_ONLY requires_authoritative_full=true\n").unwrap_or_default(),
            );
            legacy + &report.render_text()
        }
        OutputFormat::Json => serde_json::to_string_pretty(&report.to_json()).map_err(|error| error.to_string())? + "\n",
        OutputFormat::Sarif => serde_json::to_string_pretty(&report.to_sarif()).map_err(|error| error.to_string())? + "\n",
    };
    write_output(output, &text)
}

fn emit_failure(
    failure: &ScanFailure,
    format: OutputFormat,
    output: Option<&str>,
) -> Result<(), String> {
    let text = match format {
        OutputFormat::Text => failure.render_text(),
        OutputFormat::Json => serde_json::to_string_pretty(&failure.to_json()).map_err(|error| error.to_string())? + "\n",
        OutputFormat::Sarif => serde_json::to_string_pretty(&failure.to_sarif()).map_err(|error| error.to_string())? + "\n",
    };
    write_output(output, &text)
}

fn emit_failure_progress(failure: &ScanFailure, mode: ProgressMode, format: OutputFormat) {
    match mode {
        ProgressMode::Json => {
            for event in &failure.progress {
                let value = serde_json::json!({
                    "apiVersion": erf_scan_output::SCAN_OUTPUT_API_VERSION,
                    "kind": "ScanProgress",
                    "schemaVersion": erf_scan_output::SCAN_OUTPUT_VERSION,
                    "event": event,
                });
                eprintln!("{}", serde_json::to_string(&value).expect("progress event is serializable"));
            }
        }
        ProgressMode::Text if format != OutputFormat::Text => {
            for event in &failure.progress {
                eprintln!("progress stage={} outcome={} elapsed_nanos={}", event.stage, event.outcome, event.elapsed_nanos);
            }
        }
        ProgressMode::Text | ProgressMode::None => {}
    }
}

fn emit_failure_with_progress(
    failure: &ScanFailure,
    mode: ProgressMode,
    format: OutputFormat,
    output: Option<&str>,
) -> Result<(), String> {
    emit_failure_progress(failure, mode, format);
    emit_failure(failure, format, output)
}

fn write_output(output: Option<&str>, text: &str) -> Result<(), String> {
    if let Some(path) = output {
        fs::write(path, text).map_err(|error| format!("cannot write scan output {path}: {error}"))
    } else {
        print!("{text}");
        Ok(())
    }
}

fn exit_structured_failure(
    format: OutputFormat,
    code: &'static str,
    stage: &'static str,
    message: &'static str,
    detail: impl Into<String>,
    exit_code: i32,
    retry: RetryClass,
    next_step: &'static str,
) -> ! {
    let detail = detail.into();
    let failure = ScanFailure::from_diagnostic(
        ScanDiagnostic::new(code, stage, message, retry, next_step).with_detail(&detail),
        exit_code,
    );
    let text = match format {
        OutputFormat::Text => failure.render_text(),
        OutputFormat::Json => serde_json::to_string_pretty(&failure.to_json()).expect("ScanFailure is serializable") + "\n",
        OutputFormat::Sarif => serde_json::to_string_pretty(&failure.to_sarif()).expect("SARIF failure is serializable") + "\n",
    };
    if format == OutputFormat::Text {
        eprintln!("{text}");
    } else {
        print!("{text}");
    }
    std::process::exit(exit_code)
}

fn exit_preflight_failure(
    format: OutputFormat,
    code: &'static str,
    detail: impl Into<String>,
    exit_code: i32,
) -> ! {
    let (message, retry, next_step) = match code {
        "RUNTIME_ASSET_MISSING" => (
            "the installed runtime release is missing a required execution asset",
            RetryClass::FixInput,
            "install a complete rf runtime release or supply explicit advanced harness overrides",
        ),
        "RUNTIME_ASSET_UNTRUSTED" => (
            "the installed runtime asset does not match the verified Runtime Pack",
            RetryClass::RequiresTrust,
            "install the Runtime Pack selected by the signed delivery descriptor and retry the scan",
        ),
        "DELIVERY_INCONCLUSIVE" => (
            "the selected execution mode has no verified RuleBundle delivery input",
            RetryClass::RequiresTrust,
            "provide a verified Candidate/cache delivery input or use the connected reference path",
        ),
        "EXECUTION_LOCK_INVALID" => (
            "the execution lock is invalid",
            RetryClass::FixInput,
            "repair the execution lock and retry the scan",
        ),
        "OUTPUT_PATH_INVALID" => (
            "the requested scan artifact path is invalid",
            RetryClass::FixInput,
            "write the artifact below the repository without traversing symlinks",
        ),
        "CONFIG_PATH_INVALID" | "CONFIG_INVALID" => (
            "the project scan configuration is invalid",
            RetryClass::FixInput,
            "repair .ruleops/config.yaml or its explicit override and rerun validation",
        ),
        "DELIVERY_INVALID" => (
            "the RuleBundle delivery descriptor is invalid",
            RetryClass::FixInput,
            "repair the signed delivery descriptor and retry the scan",
        ),
        _ => (
            "scan preflight input is invalid",
            RetryClass::FixInput,
            "correct the command or project configuration and retry the scan",
        ),
    };
    exit_structured_failure(format, code, "input", message, detail, exit_code, retry, next_step)
}

const RUNTIME_HOME_ENV: &str = "RULEOPS_RUNTIME_HOME";
const DELIVERY_DESCRIPTOR_ENV: &str = "RULEOPS_DELIVERY_DESCRIPTOR";
const RUNTIME_RELEASE_TRUST_ROOT_ENV: &str = "RULEOPS_RUNTIME_RELEASE_TRUST_ROOT_PEM";
const RUNTIME_RELEASE_SIGNING_ROLE: &str = "runtime-release-signing";
const BACKEND_HOST_ASSETS: &[&str] = &["rf-backend-host", "ruleops-backend-host", "erf-backend-host"];
const WORKSPACE_RESOLVER_ASSETS: &[&str] = &["rf-workspace-resolver", "ruleops-workspace-resolver", "erf-workspace-resolver"];

fn scan_config_overrides(args: &[String]) -> Result<erf_project_config::ConfigOverrides, String> {
    let mut overrides = erf_project_config::ConfigOverrides::default();
    overrides.language = unique_scan_value(args, "--language")?;
    overrides.build_system = unique_scan_value(args, "--build-system")?;
    overrides.execution_mode = unique_scan_value(args, "--execution-mode")?;
    overrides.cache_dir = unique_scan_value(args, "--cache-dir")?;
    overrides.trust_state_path = unique_scan_value(args, "--trust-state-path")?;
    overrides.modules = unique_scan_csv(args, "--module")?;
    overrides.source_roots = unique_scan_csv(args, "--source-root")?;
    overrides.test_roots = unique_scan_csv(args, "--test-root")?;
    overrides.generated_roots = unique_scan_csv(args, "--generated-root")?;
    overrides.vendor_roots = unique_scan_csv(args, "--vendor-root")?;
    overrides.include = unique_scan_csv(args, "--include")?;
    overrides.exclude = unique_scan_csv(args, "--exclude")?;
    Ok(overrides)
}

fn unique_scan_value(args: &[String], name: &str) -> Result<Option<String>, String> {
    let mut value = None;
    let mut index = 1;
    while index < args.len() {
        if args[index] == name {
            let next = args.get(index + 1).ok_or_else(|| format!("missing value for {name}"))?;
            if value.is_some() {
                return Err(format!("configuration option {name} was repeated"));
            }
            value = Some(next.clone());
            index += 2;
        } else if scan_option_takes_value(args[index].as_str()) {
            index += 2;
        } else {
            index += 1;
        }
    }
    Ok(value)
}

fn scan_option_takes_value(option: &str) -> bool {
    matches!(
        option,
        "--repo-root"
            | "--project-config"
            | "--host"
            | "--host-arg"
            | "--candidate"
            | "--delivery-descriptor"
            | "--rule-id"
            | "--resolver"
            | "--runtime-home"
            | "--resolver-arg"
            | "--deadline-ms"
            | "--baseline-kind"
            | "--baseline-fingerprint"
            | "--exception-fingerprint"
            | "--blocking-rule-id"
            | "--execution-lock"
            | "--format"
            | "--progress"
            | "--output"
            | "--language"
            | "--build-system"
            | "--execution-mode"
            | "--cache-dir"
            | "--trust-state-path"
            | "--module"
            | "--source-root"
            | "--test-root"
            | "--generated-root"
            | "--vendor-root"
            | "--include"
            | "--exclude"
            | "--base"
            | "--head"
    )
}

fn unique_scan_csv(args: &[String], name: &str) -> Result<Option<Vec<String>>, String> {
    Ok(unique_scan_value(args, name)?.map(|value| value.split(',').map(|item| item.trim().to_string()).collect()))
}

fn runtime_asset_candidates(runtime_home: Option<&Path>, names: &[&str]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = runtime_home {
        roots.push(home.to_path_buf());
    }
    if let Some(home) = env::var_os(RUNTIME_HOME_ENV).map(PathBuf::from) {
        roots.push(home);
    }
    if let Ok(executable) = env::current_exe() {
        if let Some(parent) = executable.parent() {
            roots.push(parent.to_path_buf());
            if let Some(package_root) = parent.parent() {
                roots.push(package_root.to_path_buf());
            }
        }
    }
    let mut candidates = Vec::new();
    for root in roots {
        for directory in [root.clone(), root.join("bin")] {
            for name in names {
                candidates.push(directory.join(name));
            }
        }
    }
    candidates
}

fn is_runtime_asset(path: &Path) -> bool {
    fs::metadata(path).map(|metadata| metadata.is_file()).unwrap_or(false)
}

fn runtime_command(
    explicit: Option<String>,
    runtime_home: Option<&Path>,
    names: &[&str],
    role: &str,
    allow_reference_fixture: bool,
) -> Result<String, String> {
    if let Some(command) = explicit {
        if command.trim().is_empty() {
            return Err(format!("{role} command is empty"));
        }
        return Ok(command);
    }
    if let Some(candidate) = runtime_asset_candidates(runtime_home, names).into_iter().find(|path| is_runtime_asset(path)) {
        validate_runtime_manifest_asset(runtime_home, &candidate, role, allow_reference_fixture)?;
        return Ok(candidate.to_string_lossy().into_owned());
    }
    if let Some(path_value) = env::var_os("PATH") {
        for directory in env::split_paths(&path_value) {
            for name in names {
                let candidate = directory.join(name);
                if is_runtime_asset(&candidate) {
                    validate_runtime_manifest_asset(runtime_home, &candidate, role, allow_reference_fixture)?;
                    let canonical = fs::canonicalize(&candidate)
                        .map_err(|error| format!("cannot resolve {role} asset from PATH: {error}"))?;
                    return Ok(canonical.to_string_lossy().into_owned());
                }
            }
        }
    }
    Err(format!(
        "RUNTIME_ASSET_MISSING {role}; install an rf release containing {} and {}, or use explicit --host/--resolver only for an advanced local harness",
        BACKEND_HOST_ASSETS[0], WORKSPACE_RESOLVER_ASSETS[0]
    ))
}

fn validate_runtime_manifest_asset(
    runtime_home: Option<&Path>,
    candidate: &Path,
    role: &str,
    allow_reference_fixture: bool,
) -> Result<(), String> {
    let mut roots = Vec::new();
    if let Some(home) = runtime_home {
        roots.push(home.to_path_buf());
    }
    if let Some(home) = env::var_os(RUNTIME_HOME_ENV).map(PathBuf::from) {
        roots.push(home);
    }
    roots.extend(candidate.ancestors().map(Path::to_path_buf));
    roots.dedup();
    let Some(home) = roots.iter().find(|root| root.join("runtime-manifest.json").is_file()) else {
        if allow_reference_fixture
            && roots.iter().any(|root| root.join("Cargo.toml").is_file() && root.join(".git").exists())
        {
            // A development checkout has no release manifest by design; its
            // explicit harness overrides remain available to local tests.
            return Ok(());
        }
        return Err(format!("runtime manifest is missing for {role}; install a packaged rf release"));
    };
    let manifest_path = home.join("runtime-manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("runtime manifest cannot be read: {error}"))?;
    let manifest = serde_json::from_str::<serde_json::Value>(&manifest_text)
        .map_err(|error| format!("runtime manifest is invalid JSON: {error}"))?;
    let key = match role {
        "Backend Host" => "backendHost",
        "Workspace Resolver" => "workspaceResolver",
        other => return Err(format!("runtime manifest role is unsupported: {other}")),
    };
    let asset = manifest
        .get("assets")
        .and_then(|assets| assets.get(key))
        .ok_or_else(|| format!("runtime manifest is missing assets.{key}"))?;
    let declared_path = asset
        .get("path")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("runtime manifest assets.{key}.path is missing"))?;
    let expected_path = home.join(declared_path);
    if fs::canonicalize(&expected_path).ok() != fs::canonicalize(candidate).ok() {
        return Err(format!("runtime manifest assets.{key}.path does not identify the selected asset"));
    }
    let declared_digest = asset
        .get("digest")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("runtime manifest assets.{key}.digest is missing"))?;
    let actual_digest = format!("sha256:{:x}", Sha256::digest(fs::read(candidate).map_err(|error| format!("runtime asset cannot be read: {error}"))?));
    if declared_digest != actual_digest {
        return Err(format!("runtime manifest digest mismatch for {role}"));
    }
    let compatibility = asset
        .get("compatibilityIdentity")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if compatibility.is_empty() {
        return Err(format!("runtime manifest compatibility identity is missing for {role}"));
    }
    let verification = asset
        .get("verification")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if verification != "RELEASE_ARTIFACT"
        && !(allow_reference_fixture && verification == "REFERENCE_FIXTURE")
    {
        return Err(format!("runtime manifest verification status is invalid for {role}"));
    }
    Ok(())
}

fn runtime_manifest_home(runtime_home: Option<&Path>, candidates: &[&Path]) -> Result<PathBuf, String> {
    let mut roots = Vec::new();
    if let Some(home) = runtime_home {
        roots.push(home.to_path_buf());
    }
    if let Some(home) = env::var_os(RUNTIME_HOME_ENV).map(PathBuf::from) {
        roots.push(home);
    }
    for candidate in candidates {
        roots.extend(candidate.ancestors().map(Path::to_path_buf));
    }
    roots.dedup();
    roots
        .into_iter()
        .find(|root| root.join("runtime-manifest.json").is_file())
        .ok_or_else(|| "runtime manifest is missing for the delivery-bound execution assets".into())
}

fn runtime_manifest_digest(value: &Value, field: &str) -> Result<[u8; 32], String> {
    let value = value
        .as_str()
        .ok_or_else(|| format!("{field} must be a sha256 digest string"))?;
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| format!("{field} must use the sha256: prefix"))?;
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(format!("{field} must be lowercase 64-character hexadecimal"));
    }
    let mut digest = [0u8; 32];
    for (index, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        digest[index] = u8::from_str_radix(std::str::from_utf8(chunk).unwrap_or_default(), 16)
            .map_err(|_| format!("{field} contains invalid hexadecimal"))?;
    }
    Ok(digest)
}

fn hex_digest_bytes(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn cache_digest(label: &str, values: impl Iterator<Item = String>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ruleops-cache-input-v1");
    hasher.update([0]);
    hasher.update(label.as_bytes());
    hasher.update([0]);
    for value in values {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    let digest: [u8; 32] = hasher.finalize().into();
    hex_digest_bytes(&digest)
}

fn baseline_cache_digest(baseline: Option<&rf_gate::Baseline>) -> String {
    let bytes = baseline
        .map(rf_gate::Baseline::canonical_payload)
        .unwrap_or_else(|| b"erf-gate-baseline-none-v1 ".to_vec());
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    hex_digest_bytes(&digest)
}

fn git_repository_identity(repo: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|error| format!("cannot start git repository identity lookup: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git repository identity lookup failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let remote = String::from_utf8_lossy(&output.stdout).trim().trim_end_matches('/').trim_end_matches(".git").to_string();
    let remote = if let Some((host, path)) = remote.split_once(':') {
        if host.contains('@') { format!("{}/{}", host.rsplit('@').next().unwrap_or(host), path) } else { remote }
    } else {
        remote
    };
    Ok(remote.strip_prefix("https://").or_else(|| remote.strip_prefix("http://")).unwrap_or(&remote).trim_matches('/').to_string())
}

fn runtime_manifest_asset(
    manifest: &Value,
    home: &Path,
    key: &str,
) -> Result<(PathBuf, [u8; 32]), String> {
    let asset = manifest
        .get("assets")
        .and_then(Value::as_object)
        .and_then(|assets| assets.get(key))
        .ok_or_else(|| format!("runtime manifest is missing assets.{key}"))?;
    let asset = asset
        .as_object()
        .ok_or_else(|| format!("runtime manifest assets.{key} must be an object"))?;
    let relative = asset
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("runtime manifest assets.{key}.path is missing"))?;
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(format!("runtime manifest assets.{key}.path escapes the package"));
    }
    let path = home.join(relative_path);
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(format!("runtime asset {key} uses a symlinked package path"));
        }
    }
    let canonical_home = fs::canonicalize(home).map_err(|error| format!("runtime package cannot be resolved: {error}"))?;
    let canonical_path = fs::canonicalize(&path).map_err(|error| format!("runtime asset {key} cannot be resolved: {error}"))?;
    if !canonical_path.starts_with(&canonical_home)
        || !fs::symlink_metadata(&canonical_path).map(|metadata| metadata.is_file()).unwrap_or(false)
    {
        return Err(format!("runtime asset {key} is not a package-local regular file"));
    }
    let declared_digest = runtime_manifest_digest(
        asset.get("digest").ok_or_else(|| format!("runtime manifest assets.{key}.digest is missing"))?,
        &format!("runtime manifest assets.{key}.digest"),
    )?;
    let actual_digest: [u8; 32] = Sha256::digest(
        fs::read(&canonical_path).map_err(|error| format!("runtime asset {key} cannot be read: {error}"))?,
    )
    .into();
    if actual_digest != declared_digest {
        return Err(format!("runtime manifest digest mismatch for assets.{key}"));
    }
    let package_platform = manifest
        .get("platform")
        .and_then(Value::as_str)
        .ok_or_else(|| "runtime manifest platform is missing".to_string())?;
    let asset_platform = asset
        .get("platform")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("runtime manifest assets.{key}.platform is missing"))?;
    if asset_platform != package_platform {
        return Err(format!("runtime manifest platform mismatch for assets.{key}"));
    }
    let compatibility = asset
        .get("compatibilityIdentity")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("runtime manifest assets.{key}.compatibilityIdentity is missing"))?;
    let _ = compatibility;
    if asset.get("verification").and_then(Value::as_str) != Some("RELEASE_ARTIFACT") {
        return Err(format!("runtime manifest verification status is invalid for assets.{key}"));
    }
    Ok((canonical_path, actual_digest))
}

fn runtime_manifest_signing_payload(manifest: &Value) -> Result<Vec<u8>, String> {
    const PAYLOAD_FIELDS: &[&str] = &[
        "apiVersion",
        "kind",
        "schemaVersion",
        "protocolVersion",
        "platform",
        "entrypoint",
        "cli",
        "projectConfig",
        "backendHost",
        "workspaceResolver",
        "assets",
    ];
    let object = manifest
        .as_object()
        .ok_or_else(|| "runtime manifest must be a JSON object".to_string())?;
    let mut payload = BTreeMap::new();
    for field in PAYLOAD_FIELDS {
        payload.insert(
            (*field).to_string(),
            object
                .get(*field)
                .cloned()
                .ok_or_else(|| format!("runtime manifest signing field {field} is missing"))?,
        );
    }
    serde_json::to_vec(&payload).map_err(|error| format!("runtime manifest signing payload cannot be canonicalized: {error}"))
}

fn decode_base64(value: &str, field: &str) -> Result<Vec<u8>, String> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return Err(format!("{field} is not valid base64"));
    }
    let mut decoded = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        let last = index + 1 == bytes.len() / 4;
        if !last && chunk.contains(&b'=') {
            return Err(format!("{field} has padding before its final block"));
        }
        let Some(first) = base64_value(chunk[0]) else {
            return Err(format!("{field} contains an invalid character"));
        };
        let Some(second) = base64_value(chunk[1]) else {
            return Err(format!("{field} contains an invalid character"));
        };
        decoded.push((first << 2) | (second >> 4));
        if chunk[2] == b'=' {
            if chunk[3] != b'=' {
                return Err(format!("{field} has invalid padding"));
            }
            continue;
        }
        let Some(third) = base64_value(chunk[2]) else {
            return Err(format!("{field} contains an invalid character"));
        };
        decoded.push((second << 4) | (third >> 2));
        if chunk[3] == b'=' {
            continue;
        }
        let Some(fourth) = base64_value(chunk[3]) else {
            return Err(format!("{field} contains an invalid character"));
        };
        decoded.push((third << 6) | fourth);
    }
    Ok(decoded)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decode_runtime_public_key_pem(value: &str, field: &str) -> Result<Vec<u8>, String> {
    let value = value.trim();
    let body = value
        .strip_prefix("-----BEGIN PUBLIC KEY-----")
        .and_then(|body| body.strip_suffix("-----END PUBLIC KEY-----"))
        .ok_or_else(|| format!("{field} must be a SubjectPublicKeyInfo PEM"))?;
    let encoded: String = body.lines().map(str::trim).collect();
    let der = decode_base64(&encoded, field)?;
    const ED25519_SPKI_PREFIX: &[u8] = b"\x30\x2a\x30\x05\x06\x03\x2b\x65\x70\x03\x21\x00";
    if der.len() != ED25519_SPKI_PREFIX.len() + 32 || !der.starts_with(ED25519_SPKI_PREFIX) {
        return Err(format!("{field} is not an Ed25519 public key"));
    }
    Ok(der[ED25519_SPKI_PREFIX.len()..].to_vec())
}

fn verify_runtime_manifest_signature(manifest: &Value) -> Result<(), String> {
    let signature = manifest
        .get("signature")
        .and_then(Value::as_object)
        .ok_or_else(|| "runtime manifest signature is missing".to_string())?;
    if signature.get("status").and_then(Value::as_str) != Some("RELEASE_ARTIFACT") {
        return Err("runtime manifest signature status is not RELEASE_ARTIFACT".into());
    }
    if signature.get("algorithm").and_then(Value::as_str) != Some("ed25519-dsse") {
        return Err("runtime manifest signature algorithm is not ed25519-dsse".into());
    }
    if signature.get("payloadType").and_then(Value::as_str) != Some(DSSE_PAYLOAD_TYPE) {
        return Err("runtime manifest signature payload type is unsupported".into());
    }
    let key_id = signature
        .get("keyId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "runtime manifest signature keyId is missing".to_string())?;
    let payload = runtime_manifest_signing_payload(manifest)?;
    let expected_payload_digest: [u8; 32] = Sha256::digest(&payload).into();
    let declared_payload_digest = runtime_manifest_digest(
        signature
            .get("payloadDigest")
            .ok_or_else(|| "runtime manifest signature payloadDigest is missing".to_string())?,
        "runtime manifest signature.payloadDigest",
    )?;
    if declared_payload_digest != expected_payload_digest {
        return Err("runtime manifest signature payload digest does not match the manifest".into());
    }
    let manifest_key = decode_runtime_public_key_pem(
        signature
            .get("publicKeyPem")
            .and_then(Value::as_str)
            .ok_or_else(|| "runtime manifest signature publicKeyPem is missing".to_string())?,
        "runtime manifest signature.publicKeyPem",
    )?;
    let root_pem = env::var(RUNTIME_RELEASE_TRUST_ROOT_ENV).map_err(|_| {
        format!("{RUNTIME_RELEASE_TRUST_ROOT_ENV} is required to verify the packaged Runtime Manifest")
    })?;
    let trusted_key = decode_runtime_public_key_pem(&root_pem, RUNTIME_RELEASE_TRUST_ROOT_ENV)?;
    if manifest_key != trusted_key {
        return Err("runtime manifest signer is not anchored in the configured release trust root".into());
    }
    let signature_bytes = decode_base64(
        signature
            .get("signatureBase64")
            .and_then(Value::as_str)
            .ok_or_else(|| "runtime manifest signature signatureBase64 is missing".to_string())?,
        "runtime manifest signature.signatureBase64",
    )?;
    let mut trust_store = TrustStore::default();
    trust_store
        .insert_public_key_bytes(key_id, &trusted_key)
        .map_err(|error| format!("runtime manifest trust root is invalid: {error}"))?;
    trust_store
        .authorize_role(RUNTIME_RELEASE_SIGNING_ROLE, key_id)
        .map_err(|error| format!("runtime manifest signing role is invalid: {error}"))?;
    trust_store
        .verify(
            &SignedAsset {
                payload_type: DSSE_PAYLOAD_TYPE.into(),
                payload,
                key_id: key_id.into(),
                signature: signature_bytes,
            },
            RUNTIME_RELEASE_SIGNING_ROLE,
        )
        .map_err(|error| format!("runtime manifest signature verification failed: {error}"))?;
    Ok(())
}

fn validate_delivery_runtime_assets(
    delivery: &delivery::LoadedDelivery,
    runtime_home: Option<&Path>,
    host_command: &Path,
    resolver_command: &Path,
) -> Result<(), String> {
    let home = runtime_manifest_home(runtime_home, &[host_command, resolver_command])?;
    let manifest_text = fs::read_to_string(home.join("runtime-manifest.json"))
        .map_err(|error| format!("runtime manifest cannot be read: {error}"))?;
    let manifest = serde_json::from_str::<Value>(&manifest_text)
        .map_err(|error| format!("runtime manifest is invalid JSON: {error}"))?;
    verify_runtime_manifest_signature(&manifest)?;
    let (wrapper_path, _) = runtime_manifest_asset(&manifest, &home, "backendHost")?;
    let (jar_path, jar_digest) = runtime_manifest_asset(&manifest, &home, "backendHostJar")?;
    let (resolver_path, resolver_digest) = runtime_manifest_asset(&manifest, &home, "workspaceResolver")?;
    if fs::canonicalize(host_command).ok().as_deref() != Some(wrapper_path.as_path())
        || fs::canonicalize(resolver_command).ok().as_deref() != Some(resolver_path.as_path())
    {
        return Err("selected runtime command does not match the packaged Runtime Pack".into());
    }
    let jar_name = jar_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "backend Runtime Pack JAR has no valid file name".to_string())?;
    let wrapper_text = fs::read_to_string(&wrapper_path)
        .map_err(|error| format!("backend Host wrapper cannot be read: {error}"))?;
    if !wrapper_text.contains(jar_name) {
        return Err("backend Host wrapper does not bind the packaged Java Host JAR".into());
    }
    let verification = delivery
        .config
        .verification
        .as_ref()
        .ok_or_else(|| "delivery verification context is missing".to_string())?;
    let backend_pack = verification
        .packs
        .iter()
        .find(|pack| pack.kind == rf_runner::BundlePackKind::BackendRuntime)
        .ok_or_else(|| "backend Runtime Pack verification context is missing".to_string())?;
    let resolver_pack = verification
        .packs
        .iter()
        .find(|pack| pack.kind == rf_runner::BundlePackKind::WorkspaceResolver)
        .ok_or_else(|| "workspace Resolver Pack verification context is missing".to_string())?;
    if backend_pack.asset_digest != jar_digest {
        return Err("verified backend Runtime Pack does not bind the installed Java Host JAR".into());
    }
    if resolver_pack.asset_digest != resolver_digest {
        return Err("verified workspace Resolver Pack does not bind the installed resolver".into());
    }
    Ok(())
}

fn validate_delivery_execution_lock_trust(
    delivery: &delivery::LoadedDelivery,
    signed_lock: &rf_runner::SignedExecutionLock,
) -> Result<(), String> {
    let verification = delivery
        .config
        .verification
        .as_ref()
        .ok_or_else(|| "delivery verification context is missing".to_string())?;
    let key_id = signed_lock.asset.key_id.as_str();
    let delegated_key = verification
        .trust_store
        .public_key_bytes(key_id)
        .ok_or_else(|| "ExecutionLock signer is not present in the external trust delegation".to_string())?;
    let lock_key = signed_lock
        .trust_store
        .public_key_bytes(key_id)
        .ok_or_else(|| "ExecutionLock signer public key is missing".to_string())?;
    if delegated_key != lock_key || !verification.trust_store.role_authorized(&signed_lock.role, key_id) {
        return Err("ExecutionLock signer is not authorized by the external trust delegation".into());
    }
    if signed_lock.role != "execution-lock-signing" {
        return Err("ExecutionLock signer role is not execution-lock-signing".into());
    }
    let metadata = verification
        .trust_metadata
        .as_ref()
        .ok_or_else(|| "delivery trust metadata is missing for ExecutionLock verification".to_string())?;
    signed_lock.verify_with_store_at(
        &verification.trust_store,
        Some(metadata),
        delivery.config.now_epoch_secs,
    )?;
    Ok(())
}

fn validate_delivery_runner_release(
    runtime_home: Option<&Path>,
    lock: &ExecutionLock,
) -> Result<(), String> {
    let current_exe = env::current_exe().map_err(|error| format!("current rf-cli executable cannot be resolved: {error}"))?;
    let home = runtime_manifest_home(runtime_home, &[current_exe.as_path()])?;
    let manifest = serde_json::from_str::<Value>(
        &fs::read_to_string(home.join("runtime-manifest.json"))
            .map_err(|error| format!("runtime manifest cannot be read: {error}"))?,
    )
    .map_err(|error| format!("runtime manifest is invalid JSON: {error}"))?;
    let (cli_path, cli_digest) = runtime_manifest_asset(&manifest, &home, "cli")?;
    if fs::canonicalize(&current_exe).ok().as_deref() != Some(cli_path.as_path()) {
        return Err("the running rf-cli is not the packaged RunnerRelease asset".into());
    }
    if lock.runner_release_digest != hex_digest_bytes(&cli_digest) {
        return Err("ExecutionLock runnerReleaseDigest does not match the packaged rf-cli digest".into());
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("scan") {
        eprintln!("usage: rf scan --repo-root <path> --project-config <file> (--rule-id <id> | --candidate <dir> | --delivery-descriptor <file>) [--base <revision> --head <revision>] [--runtime-home <dir>] [--host <command>] [--host-arg <arg>] [--resolver <command>] [--deadline-ms <n>] [--execution-lock <file>] [--baseline-kind <debt|comparison|quality>] [--baseline-fingerprint <fp>] [--exception-fingerprint <fp>] [--blocking-rule-id <id>] [--format text|json|sarif] [--progress text|json|none] [--output <file>] [PROJECT_CONFIG_OVERRIDES]");
        std::process::exit(2);
    }
    let mut repo_root = None;
    let mut project_config_path = None;
    let mut host_command = None;
    let mut host_args = Vec::new();
    let mut rule_bundle = None;
    let mut delivery_descriptor_path = None;
    let mut rule_id = None;
    let mut resolver_command = None;
    let mut runtime_home = None;
    let mut resolver_args = Vec::new();
    let mut host_deadline_ms = 0;
    let mut baseline_kind = None;
    let mut baseline_fingerprints = BTreeSet::new();
    let mut exception_fingerprints = BTreeSet::new();
    let mut blocking_rule_ids = BTreeSet::new();
    let mut execution_lock_path = None;
    let mut output_format = OutputFormat::Text;
    let mut progress_mode = ProgressMode::Text;
    let mut output_path = None;
    let mut base_revision = None;
    let mut head_revision = None;
    let mut artifact_reference = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--repo-root" if i + 1 < args.len() => { repo_root = Some(args[i + 1].clone()); i += 2; }
            "--project-config" if i + 1 < args.len() => { project_config_path = Some(args[i + 1].clone()); i += 2; }
            "--host" if i + 1 < args.len() => { host_command = Some(args[i + 1].clone()); i += 2; }
            "--host-arg" if i + 1 < args.len() => { host_args.push(args[i + 1].clone()); i += 2; }
            "--candidate" if i + 1 < args.len() => { rule_bundle = Some(args[i + 1].clone()); i += 2; }
            "--delivery-descriptor" if i + 1 < args.len() => { delivery_descriptor_path = Some(PathBuf::from(&args[i + 1])); i += 2; }
            "--rule-id" if i + 1 < args.len() => { rule_id = Some(args[i + 1].clone()); i += 2; }
            "--resolver" if i + 1 < args.len() => { resolver_command = Some(args[i + 1].clone()); i += 2; }
            "--runtime-home" if i + 1 < args.len() => { runtime_home = Some(PathBuf::from(&args[i + 1])); i += 2; }
            "--resolver-arg" if i + 1 < args.len() => { resolver_args.push(args[i + 1].clone()); i += 2; }
            "--deadline-ms" if i + 1 < args.len() => {
                host_deadline_ms = match args[i + 1].parse() {
                    Ok(value) => value,
                    Err(_) => exit_preflight_failure(output_format, "CLI_INPUT_INVALID", "invalid --deadline-ms", 2),
                };
                i += 2;
            }
            "--baseline-kind" if i + 1 < args.len() => { baseline_kind = Some(args[i + 1].clone()); i += 2; }
            "--baseline-fingerprint" if i + 1 < args.len() => { baseline_fingerprints.insert(args[i + 1].clone()); i += 2; }
            "--exception-fingerprint" if i + 1 < args.len() => { exception_fingerprints.insert(args[i + 1].clone()); i += 2; }
            "--blocking-rule-id" if i + 1 < args.len() => { blocking_rule_ids.insert(args[i + 1].clone()); i += 2; }
            "--execution-lock" if i + 1 < args.len() => { execution_lock_path = Some(args[i + 1].clone()); i += 2; }
            "--format" if i + 1 < args.len() => {
                output_format = match OutputFormat::parse(&args[i + 1]) {
                    Ok(value) => value,
                    Err(error) => exit_preflight_failure(OutputFormat::Text, "CLI_INPUT_INVALID", error, 2),
                };
                i += 2;
            }
            "--progress" if i + 1 < args.len() => {
                progress_mode = match ProgressMode::parse(&args[i + 1]) {
                    Ok(value) => value,
                    Err(error) => exit_preflight_failure(output_format, "CLI_INPUT_INVALID", error, 2),
                };
                i += 2;
            }
            "--output" if i + 1 < args.len() => { output_path = Some(args[i + 1].clone()); i += 2; }
            "--base" if i + 1 < args.len() => {
                if base_revision.is_some() { exit_preflight_failure(output_format, "CLI_INPUT_INVALID", "--base was repeated", 2); }
                base_revision = Some(args[i + 1].clone()); i += 2;
            }
            "--head" if i + 1 < args.len() => {
                if head_revision.is_some() { exit_preflight_failure(output_format, "CLI_INPUT_INVALID", "--head was repeated", 2); }
                head_revision = Some(args[i + 1].clone()); i += 2;
            }
            "--language" | "--build-system" | "--execution-mode" | "--cache-dir" | "--trust-state-path"
            | "--module" | "--source-root" | "--test-root" | "--generated-root" | "--vendor-root"
            | "--include" | "--exclude" if i + 1 < args.len() => { i += 2; }
            other => exit_preflight_failure(output_format, "CLI_INPUT_INVALID", format!("unexpected argument: {other}"), 2),
        }
    }
    let Some(repo_root) = repo_root else { exit_preflight_failure(output_format, "CLI_INPUT_INVALID", "missing --repo-root", 2) };
    if base_revision.is_some() != head_revision.is_some() {
        exit_preflight_failure(output_format, "CLI_INPUT_INVALID", "--base and --head must be supplied together", 2);
    }
    let repo_path = PathBuf::from(&repo_root);
    let config_path = project_config_path.map(PathBuf::from).unwrap_or_else(|| repo_path.join(CONFIG_RELATIVE_PATH));
    let canonical_repo = match fs::canonicalize(&repo_path) {
        Ok(path) => path,
        Err(error) => exit_preflight_failure(output_format, "CONFIG_PATH_INVALID", format!("cannot resolve repository root: {error}"), 2),
    };
    let canonical_config = match fs::canonicalize(&config_path) {
        Ok(path) => path,
        Err(error) => exit_preflight_failure(output_format, "CONFIG_PATH_INVALID", format!("cannot resolve project config: {error}"), 2),
    };
    if !canonical_config.starts_with(&canonical_repo) {
        exit_preflight_failure(output_format, "CONFIG_PATH_INVALID", "project config is outside the repository root", 2);
    }
    let config_overrides = match scan_config_overrides(&args) {
        Ok(overrides) => overrides,
        Err(error) => exit_preflight_failure(output_format, "CONFIG_INVALID", error.to_string(), 2),
    };
    let (project_config, _) = match load_config(&repo_path, Some(&config_path)) {
        Ok(value) => value,
        Err(error) => exit_preflight_failure(output_format, "CONFIG_INVALID", error.to_string(), 2),
    };
    let (effective_config, _) = match apply_precedence(&project_config, &config_overrides, &process_environment()) {
        Ok(value) => value,
        Err(error) => exit_preflight_failure(output_format, "CONFIG_INVALID", error.to_string(), 2),
    };
    if let Err(error) = validate_workspace_paths(&repo_path, &effective_config) {
        exit_preflight_failure(output_format, "CONFIG_INVALID", error.to_string(), 2);
    }
    let identity = effective_config.identity();
    std::env::set_var("RULEOPS_EFFECTIVE_CONFIG_DIGEST", &identity.config_digest);
    std::env::set_var("RULEOPS_WORKSPACE_DESCRIPTOR_DIGEST", &identity.workspace_descriptor_digest);
    std::env::set_var("RULEOPS_SCAN_PLAN_DIGEST", &identity.scan_plan_digest);
    std::env::set_var("RULEOPS_EXECUTION_MODE", effective_config.execution_mode.as_str());
    let expected_delivery_profile = match effective_config.execution_mode {
        erf_project_config::ExecutionMode::Connected => "connected",
        erf_project_config::ExecutionMode::Offline => "enterpriseAirGapped",
        erf_project_config::ExecutionMode::Standalone => "standaloneDeveloper",
    };
    let delivery_descriptor_path = delivery_descriptor_path.or_else(|| {
        env::var_os(DELIVERY_DESCRIPTOR_ENV).map(PathBuf::from)
    });
    let now_epoch_secs = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(error) => exit_preflight_failure(
            output_format,
            "DELIVERY_INVALID",
            format!("system clock is before Unix epoch: {error}"),
            2,
        ),
    };
    let loaded_delivery = match delivery_descriptor_path.as_deref() {
        Some(path) => match load_delivery(path, &canonical_repo, expected_delivery_profile, now_epoch_secs) {
            Ok(delivery) => Some(delivery),
            Err(error) if error.contains("delivery trust") || error.contains("DELIVERY_TRUST_ROOT") => {
                exit_preflight_failure(output_format, "DELIVERY_TRUST_INVALID", error, 4)
            }
            Err(error) => exit_preflight_failure(output_format, "DELIVERY_INVALID", error, 2),
        },
        None => None,
    };
    if loaded_delivery.is_some() && rule_bundle.is_some() {
        exit_preflight_failure(
            output_format,
            "CLI_INPUT_INVALID",
            "--delivery-descriptor cannot be combined with --candidate; the descriptor is the canonical delivery input",
            2,
        );
    }
    if loaded_delivery.is_some()
        && (host_command.is_some()
            || resolver_command.is_some()
            || !host_args.is_empty()
            || !resolver_args.is_empty())
    {
        exit_preflight_failure(
            output_format,
            "CLI_INPUT_INVALID",
            "--delivery-descriptor cannot be combined with internal --host/--resolver overrides",
            2,
        );
    }
    let delivery_rule_id = loaded_delivery.as_ref().map(|delivery| delivery.rule_id.clone());
    if let (Some(delivery_rule_id), Some(cli_rule_id)) = (delivery_rule_id.as_ref(), rule_id.as_ref()) {
        if delivery_rule_id != cli_rule_id {
            exit_preflight_failure(
                output_format,
                "CLI_INPUT_INVALID",
                "the --rule-id does not match the delivery descriptor ruleId",
                2,
            );
        }
    }
    let descriptor_execution_lock_path = loaded_delivery
        .as_ref()
        .and_then(|delivery| delivery.execution_lock_path.clone());
    let execution_lock_path = execution_lock_path.map(|path| {
        let path = PathBuf::from(path);
        let canonical = match fs::canonicalize(&path) {
            Ok(path) => path,
            Err(error) => exit_preflight_failure(
                output_format,
                "EXECUTION_LOCK_INVALID",
                format!("cannot resolve --execution-lock: {error}"),
                2,
            ),
        };
        if !canonical.starts_with(&canonical_repo) {
            exit_preflight_failure(
                output_format,
                "EXECUTION_LOCK_INVALID",
                "execution lock is outside the repository root",
                2,
            );
        }
        canonical
    });
    if let (Some(cli_path), Some(descriptor_path)) = (execution_lock_path.as_ref(), descriptor_execution_lock_path.as_ref()) {
        if cli_path != descriptor_path {
            exit_preflight_failure(
                output_format,
                "CLI_INPUT_INVALID",
                "the --execution-lock does not match the delivery descriptor executionLockPath",
                2,
            );
        }
    }
    let execution_lock_path = execution_lock_path
        .or(descriptor_execution_lock_path)
        .map(|path| path.to_string_lossy().into_owned());
    let project_scope = ProjectScanScope {
        modules: effective_config.modules.clone(),
        roots: effective_config
            .source_roots
            .iter()
            .chain(effective_config.test_roots.iter())
            .chain(effective_config.generated_roots.iter())
            .chain(effective_config.vendor_roots.iter())
            .cloned()
            .collect(),
        include: effective_config.include.clone(),
        exclude: effective_config.exclude.clone(),
    };
    if let Some(output) = output_path.as_ref() {
        let candidate = PathBuf::from(output);
        let candidate = if candidate.is_absolute() {
            candidate
        } else {
            match std::env::current_dir() {
                Ok(current) => current.join(candidate),
                Err(error) => exit_preflight_failure(output_format, "OUTPUT_PATH_INVALID", format!("cannot resolve output directory: {error}"), 2),
            }
        };
        if let Err(error) = validate_output_path(&canonical_repo, &candidate) {
            exit_preflight_failure(output_format, "OUTPUT_PATH_INVALID", error.to_string(), 2);
        }
        artifact_reference = Some(
            candidate
                .strip_prefix(&canonical_repo)
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| candidate.to_string_lossy().replace('\\', "/")),
        );
        output_path = Some(candidate.to_string_lossy().into_owned());
    }
    if effective_config.execution_mode != erf_project_config::ExecutionMode::Connected
        && rule_bundle.is_none()
        && loaded_delivery.is_none()
    {
        exit_preflight_failure(
            output_format,
            "DELIVERY_INCONCLUSIVE",
            format!("execution mode {} requires a verified Candidate or delivery/cache input", effective_config.execution_mode.as_str()),
            3,
        );
    }
    let allow_reference_fixture = loaded_delivery.is_none();
    let host_command = match runtime_command(
        host_command,
        runtime_home.as_deref(),
        BACKEND_HOST_ASSETS,
        "Backend Host",
        allow_reference_fixture,
    ) {
        Ok(command) => command,
        Err(error) => exit_preflight_failure(output_format, "RUNTIME_ASSET_MISSING", error, 2),
    };
    let resolver_command = match runtime_command(
        resolver_command,
        runtime_home.as_deref(),
        WORKSPACE_RESOLVER_ASSETS,
        "Workspace Resolver",
        allow_reference_fixture,
    ) {
        Ok(command) => command,
        Err(error) => exit_preflight_failure(output_format, "RUNTIME_ASSET_MISSING", error, 2),
    };
    if let Some(delivery) = loaded_delivery.as_ref() {
        if let Err(error) = validate_delivery_runtime_assets(
            delivery,
            runtime_home.as_deref(),
            Path::new(&host_command),
            Path::new(&resolver_command),
        ) {
            exit_preflight_failure(output_format, "RUNTIME_ASSET_UNTRUSTED", error, 4);
        }
    }
    let rule_id = if loaded_delivery.is_some() {
        delivery_rule_id.expect("loaded delivery always carries a rule id")
    } else {
        match (rule_bundle.is_some(), rule_id) {
            (true, Some(_)) => {
                exit_preflight_failure(output_format, "CLI_INPUT_INVALID", "--rule-id cannot be combined with --candidate; Candidate manifest is authoritative", 2);
            }
            (true, None) => String::new(),
            (false, Some(rule_id)) if !rule_id.trim().is_empty() => rule_id,
            (false, Some(_)) => {
                exit_preflight_failure(output_format, "CLI_INPUT_INVALID", "--rule-id cannot be empty", 2);
            }
            (false, None) => {
                exit_preflight_failure(output_format, "CLI_INPUT_INVALID", "missing --rule-id when --candidate is absent", 2);
            }
        }
    };
    let (execution_lock, signed_execution_lock) = execution_lock_path.map(|path| {
        let path_ref = std::path::Path::new(&path);
        match SignedExecutionLock::from_file(path_ref) {
            Ok(lock) => (None, Some(lock)),
            Err(signed_error) => match ExecutionLock::from_file(path_ref) {
                Ok(lock) => (Some(lock), None),
                Err(unsigned_error) => exit_preflight_failure(
                    output_format,
                    "EXECUTION_LOCK_INVALID",
                    format!("signed parse failed: {signed_error}; unsigned parse failed: {unsigned_error}"),
                    2,
                ),
            },
        }
    }).unwrap_or((None, None));
    if loaded_delivery.is_some() && execution_lock.is_some() {
        exit_preflight_failure(
            output_format,
            "EXECUTION_LOCK_UNTRUSTED",
            "standard delivery scans require a signed ExecutionLock",
            4,
        );
    }
    if let (Some(delivery), Some(signed_lock)) = (loaded_delivery.as_ref(), signed_execution_lock.as_ref()) {
        if let Err(error) = validate_delivery_execution_lock_trust(delivery, signed_lock) {
            exit_preflight_failure(output_format, "EXECUTION_LOCK_UNTRUSTED", error, 4);
        }
        if let Err(error) = validate_delivery_runner_release(runtime_home.as_deref(), &signed_lock.lock) {
            exit_preflight_failure(output_format, "EXECUTION_LOCK_UNTRUSTED", error, 4);
        }
    }
    let incremental = match (base_revision, head_revision) {
        (Some(base), Some(head)) => {
            let rule_bundle_digest = if let Some(delivery) = loaded_delivery.as_ref() {
                hex_digest_bytes(&delivery.config.descriptor.bundle_digest)
            } else if let Some(lock) = execution_lock.as_ref() {
                lock.rule_bundle_digest.trim_start_matches("sha256:").to_string()
            } else if let Some(candidate) = rule_bundle.as_ref() {
                match rf_runner::bundle_payload_digest(Path::new(candidate)) {
                    Ok(digest) => digest,
                    Err(error) => exit_preflight_failure(output_format, "INCREMENTAL_INPUT_INVALID", error.to_string(), 2),
                }
            } else {
                exit_preflight_failure(output_format, "INCREMENTAL_INPUT_INVALID", "incremental scans require a Candidate, delivery descriptor, or ExecutionLock bundle identity", 2);
            };
            if rule_bundle_digest.len() != 64 || !rule_bundle_digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                exit_preflight_failure(output_format, "INCREMENTAL_INPUT_INVALID", "RuleBundle identity is not a lowercase SHA-256 digest", 2);
            }
            let repository = match git_repository_identity(&canonical_repo) {
                Ok(value) if !value.is_empty() => value,
                Ok(_) | Err(_) => exit_preflight_failure(output_format, "INCREMENTAL_INPUT_INVALID", "incremental scans require an origin repository identity", 2),
            };
            let runner_identity = match env::current_exe().and_then(fs::read) {
                Ok(bytes) => {
                    let digest: [u8; 32] = Sha256::digest(bytes).into();
                    hex_digest_bytes(&digest)
                }
                Err(error) => exit_preflight_failure(output_format, "INCREMENTAL_INPUT_INVALID", format!("cannot measure rf-cli: {error}"), 2),
            };
            let resolver_identity = execution_lock
                .as_ref()
                .map(|lock| format!("{}/{}", lock.resolver_id, lock.resolver_version))
                .unwrap_or_else(|| resolver_command.clone());
            let backend_identity = execution_lock
                .as_ref()
                .map(|lock| format!("{}/{}", lock.backend_id, lock.backend_version))
                .unwrap_or_else(|| env::var("ERF_BACKEND").unwrap_or_else(|_| "pmd-java".into()));
            Some(IncrementalScanRequest {
                repository,
                base,
                head,
                rule_bundle_digest,
                policy_digest: identity.config_digest.clone(),
                resolver_identity,
                backend_identity,
                runner_identity,
                canonical_set_version: rf_gate::CANONICAL_SET_VERSION.into(),
            })
        }
        (None, None) => None,
        _ => unreachable!("base/head were validated as a pair"),
    };
    let incremental_requested = incremental.is_some();
    let result = scan(&ScanConfig {
        repo_root,
        // With a Candidate, rf-runner derives identity from the validated
        // manifest instead of trusting this CLI value.
        rule_id,
        rule_bundle,
        bundle_delivery: loaded_delivery.map(|delivery| delivery.config),
        resolver_command,
        resolver_args,
        host_command,
        host_args,
        host_deadline_ms,
        resolver_deadline_ms: host_deadline_ms,
        max_staging_bytes: 64 * 1024 * 1024,
        execution_lock,
        signed_execution_lock,
        resolver_sandbox_policy: None,
        sandbox_policy: None,
        workspace_attestation: None,
        frozen_plan: None,
        incremental,
        scan_cache_dir: Some(effective_config.resolved_cache_dir(&canonical_repo).to_string_lossy().into_owned()),
        fallback_inputs: None,
        project_scope: Some(project_scope),
    });
    match result {
        Ok(mut result) => {
            let baseline = baseline_kind.map(|kind| {
                let kind = match kind.as_str() {
                    "debt" => rf_gate::BaselineKind::Debt,
                    "comparison" => rf_gate::BaselineKind::Comparison,
                    "quality" => rf_gate::BaselineKind::QualityBenchmark,
                    other => exit_preflight_failure(output_format, "CLI_INPUT_INVALID", format!("invalid --baseline-kind: {other}"), 2),
                };
                rf_gate::Baseline { kind, finding_fingerprints: baseline_fingerprints }
            });
            let policy = (!blocking_rule_ids.is_empty()).then_some(rf_gate::GatePolicy { blocking_rule_ids });
            let gate_timer = StageTimer::start(Stage::Gate, digest_summary(&format!("rf-cli-gate:{:?}", result.plan_digest)))
                .expect("gate telemetry input digest is always valid");
            let delivery_descriptor_digest = result
                .bundle_delivery_identity
                .as_ref()
                .map(|identity| identity.descriptor_digest.as_str());
            let gate = rf_gate::preview_with_policy_and_baseline_and_exceptions_and_delivery_and_observation_errors(
                &result.planned_unit_ids,
                &result.unit_statuses,
                &result.committed_findings,
                baseline.as_ref(),
                &exception_fingerprints,
                policy.as_ref(),
                delivery_descriptor_digest,
                result.resolver_required_unresolved_facts.len(),
            );
            let Some(mut telemetry) = result.stage_telemetry.take() else {
                let failure = ScanFailure::from_diagnostic(
                    ScanDiagnostic::new(
                        "TELEMETRY_MISSING",
                        "gate",
                        "scan did not return the required stage telemetry",
                        RetryClass::FixInput,
                        "rerun with a Runner that emits all required stage summaries",
                    ),
                    3,
                );
                if output_format == OutputFormat::Text {
                    println!("gate_telemetry=INCONCLUSIVE missing_stage_telemetry=true");
                }
                if let Err(error) = emit_failure_with_progress(&failure, progress_mode, output_format, output_path.as_deref()) {
                    eprintln!("{error}");
                    std::process::exit(2);
                }
                std::process::exit(3);
            };
            if let Err(error) = telemetry.replace_measurement(gate_timer.finish()) {
                let failure = ScanFailure::from_diagnostic(
                    ScanDiagnostic::new(
                        "TELEMETRY_INVALID",
                        "gate",
                        "scan stage telemetry could not be completed",
                        RetryClass::FixInput,
                        "rerun with a compatible Runner and inspect the telemetry contract",
                    )
                    .with_detail(&error.to_string()),
                    2,
                );
                if let Err(write_error) = emit_failure_with_progress(&failure, progress_mode, output_format, output_path.as_deref()) {
                    eprintln!("{write_error}");
                }
                std::process::exit(2);
            }
            if !telemetry.has_all_stage_summaries() {
                let failure = ScanFailure::from_diagnostic(
                    ScanDiagnostic::new(
                        "TELEMETRY_INCOMPLETE",
                        "gate",
                        "scan stage telemetry is missing one or more required stages",
                        RetryClass::FixInput,
                        "rerun with a Runner that emits resolve through gate summaries",
                    ),
                    3,
                );
                if output_format == OutputFormat::Text {
                    println!("gate_telemetry=INCONCLUSIVE missing_stage_telemetry=true");
                }
                if let Err(error) = emit_failure_with_progress(&failure, progress_mode, output_format, output_path.as_deref()) {
                    eprintln!("{error}");
                    std::process::exit(2);
                }
                std::process::exit(3);
            }
            result.stage_telemetry = Some(telemetry);
            let mut report = match ScanReport::from_scan(&result, &gate) {
                Ok(report) => report,
                Err(error) => {
                    let failure = ScanFailure::from_diagnostic(
                        ScanDiagnostic::new(
                            "OUTPUT_INVALID",
                            "output",
                            "the scan result could not be projected to the closed output contract",
                            RetryClass::FixInput,
                            "inspect the Runner/Gate result contract before consuming output",
                        )
                        .with_detail(&error.to_string()),
                        2,
                    );
                    if let Err(write_error) = emit_failure_with_progress(&failure, progress_mode, output_format, output_path.as_deref()) {
                        eprintln!("{write_error}");
                    }
                    std::process::exit(2);
                }
            };
            report.set_artifact_path(artifact_reference.as_deref());
            if incremental_requested {
                let cache_write = result
                    .scan_plan
                    .as_ref()
                    .zip(result.plan_completeness.as_ref())
                    .map(|(plan, completeness)| {
                        let baseline_digest = baseline_cache_digest(baseline.as_ref());
                        let exception_digest = cache_digest(
                            "exceptions",
                            exception_fingerprints.iter().cloned(),
                        );
                        let result_identity = erf_scan_plan::ResultIdentity {
                            semantic_result_digest: hex_digest_bytes(&gate.semantic_result_digest),
                            baseline_digest,
                            exception_digest,
                            unified_result_digest: Some(report.result_digest.clone()),
                            trusted_context_digest: None,
                        };
                        let decision = match gate.decision {
                            rf_gate::GateDecision::Pass => erf_scan_plan::CacheDecision::Pass,
                            rf_gate::GateDecision::Fail => erf_scan_plan::CacheDecision::Fail,
                            rf_gate::GateDecision::Inconclusive => erf_scan_plan::CacheDecision::Inconclusive,
                        };
                        let now = now_epoch_secs;
                        let expires = now.saturating_add(24 * 60 * 60);
                        let entry = erf_scan_plan::CacheEntry::from_report(
                            plan,
                            result_identity,
                            completeness,
                            decision,
                            now,
                            expires,
                        )
                        .map_err(|error| format!("cannot build scan cache record: {error}"))?;
                        erf_scan_plan::FileScanCache::new(effective_config.resolved_cache_dir(&canonical_repo))
                            .put(&entry)
                            .map_err(|error| format!("cannot persist scan cache record: {error}"))
                    });
                if let Some(Err(error)) = cache_write {
                    eprintln!("scan cache write skipped: {error}");
                }
            }
            emit_progress(&report, progress_mode);
            if progress_mode == ProgressMode::Text && output_format != OutputFormat::Text {
                for event in &report.progress {
                    eprintln!("progress stage={} outcome={} elapsed_nanos={}", event.stage, event.outcome, event.elapsed_nanos);
                }
            }
            if let Err(error) = emit_report(&result, &gate, &report, output_format, output_path.as_deref()) {
                eprintln!("{error}");
                std::process::exit(2);
            }
            let exit_code = if result.plan_completeness.as_ref().is_some_and(|plan| !plan.allows_authoritative_gate())
                || result.fallback_decision.as_ref().is_some_and(|decision| !decision.allows_authoritative_gate())
                || gate.decision == rf_gate::GateDecision::Inconclusive
                || !gate.completeness.complete
            {
                3
            } else {
                0
            };
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
        }
        Err(error) => {
            let exit_code = scan_error_exit_code(&error);
            // Structured formats expose only the closed ScanFailure contract;
            // its detailDigest preserves diagnostics without leaking endpoint,
            // filesystem, or process details to CI logs. Human text remains
            // useful for local debugging, subject to the existing progress
            // mode boundary.
            if output_format == OutputFormat::Text && progress_mode != ProgressMode::Json {
                eprintln!("scan failed atomically: {error}");
            }
            let failure = ScanFailure::from_scan_error(&error, exit_code);
            if let Err(write_error) = emit_failure_with_progress(&failure, progress_mode, output_format, output_path.as_deref()) {
                eprintln!("{write_error}");
                std::process::exit(2);
            }
            std::process::exit(exit_code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_exit_codes_preserve_contract_categories() {
        assert_eq!(scan_error_exit_code(&ScanError::RuleBundle("bad candidate".into())), 2);
        assert_eq!(scan_error_exit_code(&ScanError::Plan("bad plan".into())), 2);
        assert_eq!(scan_error_exit_code(&ScanError::Sandbox("sandbox unavailable".into())), 3);
        assert_eq!(scan_error_exit_code(&ScanError::Trust("invalid signature".into())), 4);
        assert_eq!(scan_error_exit_code(&ScanError::Delivery("connected runner must fetch the missing bundle from the server".into())), 3);
        assert_eq!(scan_error_exit_code(&ScanError::Delivery("delivery resolution failed: offline delivery is inconclusive: SIGNATURE_INVALID".into())), 4);
        assert_eq!(scan_error_exit_code(&ScanError::Delivery("invalid delivery descriptor field: platform".into())), 2);
        assert_eq!(scan_error_exit_code(&ScanError::Delivery("delivery-class=trust; wording can change".into())), 4);
        assert_eq!(scan_error_exit_code(&ScanError::Delivery("delivery-class=input; wording can change".into())), 2);
        assert_eq!(scan_error_exit_code(&ScanError::Delivery("delivery-class=inconclusive; wording can change".into())), 3);
        assert_eq!(host_error_exit_code(&erf_host_client::HostError::MajorMismatch { requested: 0, acked: 1 }), 2);
        assert_eq!(host_error_exit_code(&erf_host_client::HostError::Hangup { status: "crashed".into() }), 3);
        assert_eq!(resolver_error_exit_code(&erf_resolver_client::ResolverError::Violation("bad frame".into())), 2);
    }

    #[test]
    fn scan_configuration_overrides_are_collected_without_parsing_nested_process_args() {
        let args = vec![
            "scan".into(),
            "--repo-root".into(),
            "/workspace".into(),
            "--host-arg".into(),
            "--include".into(),
            "--include".into(),
            "src/**".into(),
            "--execution-mode".into(),
            "offline".into(),
        ];
        let overrides = scan_config_overrides(&args).unwrap();
        assert_eq!(overrides.include, Some(vec!["src/**".into()]));
        assert_eq!(overrides.execution_mode.as_deref(), Some("offline"));
    }

    #[test]
    fn explicit_runtime_asset_override_is_preserved_for_advanced_harnesses() {
        assert_eq!(
            runtime_command(Some("/opt/erf/bin/backend".into()), None, BACKEND_HOST_ASSETS, "Backend Host", false).unwrap(),
            "/opt/erf/bin/backend"
        );
    }
}
