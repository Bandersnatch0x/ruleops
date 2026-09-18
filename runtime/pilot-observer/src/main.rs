use erf_feedback::FeedbackClass;
use erf_pilot_metrics::{ObservationMode, PilotMetricsLedger, RegressionOutcome};
use erf_unified_result::{AuditEventKind, AuditTrail, ResultInputs, UnifiedResult};
use rf_gate::{Baseline, BaselineKind, Completeness, GateDecision, GatePolicy, GatePreview};
use rf_runner::{scan, ScanConfig, ScanError};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandKind {
    Baseline,
    Run,
    Feedback,
    Governance,
    Report,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    LocalSimulation,
    LiveShadow,
}

impl Default for Mode {
    fn default() -> Self { Self::LiveShadow }
}

impl Mode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "local-simulation" => Ok(Self::LocalSimulation),
            "live-shadow" => Ok(Self::LiveShadow),
            _ => Err(format!("invalid --mode: {value} (expected local-simulation or live-shadow)")),
        }
    }

    fn metric_mode(self) -> ObservationMode {
        match self {
            Self::LocalSimulation => ObservationMode::LocalSimulation,
            Self::LiveShadow => ObservationMode::LiveShadow,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::LocalSimulation => "LOCAL_SIMULATION",
            Self::LiveShadow => "LIVE_SHADOW",
        }
    }
}

#[derive(Debug, Default)]
struct ScanOptions {
    repository: String,
    commit: String,
    repo_root: String,
    candidate: String,
    host: String,
    host_args: Vec<String>,
    resolver: String,
    resolver_args: Vec<String>,
    baseline: Option<PathBuf>,
    exceptions: BTreeSet<String>,
    blocking_rules: BTreeSet<String>,
    mode: Mode,
    policy_digest: Option<String>,
    execution_lock_digest: Option<String>,
    runner_release_digest: Option<String>,
    journal: Option<PathBuf>,
    output: Option<PathBuf>,
    measurement_id: Option<String>,
    observed_at: Option<u64>,
    actor: String,
    reason: String,
}

#[derive(Debug, Default)]
struct ReportOptions {
    journal: PathBuf,
    output: PathBuf,
    mode: Option<Mode>,
    window_start: u64,
    window_end: u64,
    expected_repositories: usize,
    latency_baseline_millis: Option<u64>,
    memory_baseline_bytes: Option<u64>,
}

fn main() -> ExitCode {
    match run_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pilot observer failed: {error}");
            ExitCode::from(2)
        }
    }
}

fn run_main() -> Result<(), String> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args[0] == "--help" || args[0] == "help" {
        print_usage();
        return Ok(());
    }
    let command = match args.remove(0).as_str() {
        "baseline" => CommandKind::Baseline,
        "run" => CommandKind::Run,
        "feedback" => CommandKind::Feedback,
        "governance" => CommandKind::Governance,
        "report" => CommandKind::Report,
        value => return Err(format!("unknown command: {value}")),
    };
    match command {
        CommandKind::Baseline => run_baseline(parse_scan_options(args, false)?),
        CommandKind::Run => run_observation(parse_scan_options(args, true)?),
        CommandKind::Feedback => record_feedback(parse_feedback_options(args)?),
        CommandKind::Governance => record_governance(parse_governance_options(args)?),
        CommandKind::Report => write_report(parse_report_options(args)?),
    }
}

fn print_usage() {
    println!(
        "usage:\n  erf-pilot-observer baseline --repository ID --commit SHA --repo-root PATH --candidate DIR --host CMD --resolver CMD --output FILE [--host-arg ARG] [--resolver-arg ARG]\n  erf-pilot-observer run --repository ID --commit SHA --repo-root PATH --candidate DIR --host CMD --resolver CMD --journal FILE --mode local-simulation|live-shadow [options]\n  erf-pilot-observer feedback --journal FILE --measurement-id ID --classification valid|documentation|scope|false-positive --regression pending|clean|finding [options]\n  erf-pilot-observer governance --journal FILE --measurement-id ID --rollback-count N --active-exception-count N --oldest-exception-age-secs N [options]\n  erf-pilot-observer report --journal FILE --output FILE --window-start EPOCH --window-end EPOCH --expected-repositories N [--mode local-simulation|live-shadow] [--latency-baseline-millis N] [--memory-baseline-bytes N]"
    );
}

fn parse_scan_options(args: Vec<String>, requires_journal: bool) -> Result<ScanOptions, String> {
    let mut options = ScanOptions { mode: Mode::LiveShadow, actor: "pilot-observer".into(), reason: "observation run".into(), ..ScanOptions::default() };
    let mut index = 0;
    while index < args.len() {
        let flag = &args[index];
        let mut take = || {
            index += 1;
            args.get(index).cloned().ok_or_else(|| format!("missing value for {flag}"))
        };
        match flag.as_str() {
            "--repository" => options.repository = take()?,
            "--commit" => options.commit = take()?,
            "--repo-root" => options.repo_root = take()?,
            "--candidate" => options.candidate = take()?,
            "--host" => options.host = take()?,
            "--host-arg" => options.host_args.push(take()?),
            "--resolver" => options.resolver = take()?,
            "--resolver-arg" => options.resolver_args.push(take()?),
            "--baseline" => options.baseline = Some(PathBuf::from(take()?)),
            "--exception-fingerprint" => { options.exceptions.insert(take()?); }
            "--blocking-rule-id" => { options.blocking_rules.insert(take()?); }
            "--mode" => options.mode = Mode::parse(&take()?)?,
            "--policy-digest" => options.policy_digest = Some(take()?),
            "--execution-lock-digest" => options.execution_lock_digest = Some(take()?),
            "--runner-release-digest" => options.runner_release_digest = Some(take()?),
            "--journal" => options.journal = Some(PathBuf::from(take()?)),
            "--output" => options.output = Some(PathBuf::from(take()?)),
            "--measurement-id" => options.measurement_id = Some(take()?),
            "--observed-at" => options.observed_at = Some(parse_u64(&take()?, "--observed-at")?),
            "--actor" => options.actor = take()?,
            "--reason" => options.reason = take()?,
            "--help" => { print_usage(); return Err("help requested".into()); }
            other => return Err(format!("unexpected argument: {other}")),
        }
        index += 1;
    }
    for (name, value) in [
        ("--repository", &options.repository),
        ("--commit", &options.commit),
        ("--repo-root", &options.repo_root),
        ("--candidate", &options.candidate),
        ("--host", &options.host),
        ("--resolver", &options.resolver),
    ] {
        if value.trim().is_empty() { return Err(format!("missing {name}")); }
    }
    if requires_journal && options.journal.is_none() { return Err("run requires --journal".into()); }
    if options.output.is_none() && !requires_journal { return Err("baseline requires --output".into()); }
    Ok(options)
}

fn parse_feedback_options(args: Vec<String>) -> Result<(PathBuf, String, FeedbackClass, RegressionOutcome, String, String, u64), String> {
    let mut journal = None;
    let mut measurement_id = None;
    let mut classification = None;
    let mut regression = RegressionOutcome::NotApplicable;
    let mut actor = "pilot-reviewer".to_string();
    let mut reason = "human pilot review".to_string();
    let mut observed_at = now_epoch();
    parse_simple_options(args, |flag, value| match flag {
        "--journal" => { journal = Some(PathBuf::from(value)); Ok(()) },
        "--measurement-id" => { measurement_id = Some(value); Ok(()) },
        "--classification" => { classification = Some(parse_classification(&value)?); Ok(()) },
        "--regression" => { regression = parse_regression(&value)?; Ok(()) },
        "--actor" => { actor = value; Ok(()) },
        "--reason" => { reason = value; Ok(()) },
        "--observed-at" => { observed_at = parse_u64(&value, "--observed-at")?; Ok(()) },
        _ => return Err(format!("unexpected argument: {flag}")),
    })?;
    Ok((journal.ok_or("feedback requires --journal")?, measurement_id.ok_or("feedback requires --measurement-id")?, classification.ok_or("feedback requires --classification")?, regression, actor, reason, observed_at))
}

fn parse_governance_options(args: Vec<String>) -> Result<(PathBuf, String, u64, u64, u64, String, String, u64), String> {
    let mut journal = None;
    let mut measurement_id = None;
    let mut rollback = None;
    let mut exceptions = None;
    let mut oldest = None;
    let mut actor = "pilot-governance".to_string();
    let mut reason = "pilot governance snapshot".to_string();
    let mut observed_at = now_epoch();
    parse_simple_options(args, |flag, value| match flag {
        "--journal" => { journal = Some(PathBuf::from(value)); Ok(()) },
        "--measurement-id" => { measurement_id = Some(value); Ok(()) },
        "--rollback-count" => { rollback = Some(parse_u64(&value, "--rollback-count")?); Ok(()) },
        "--active-exception-count" => { exceptions = Some(parse_u64(&value, "--active-exception-count")?); Ok(()) },
        "--oldest-exception-age-secs" => { oldest = Some(parse_u64(&value, "--oldest-exception-age-secs")?); Ok(()) },
        "--actor" => { actor = value; Ok(()) },
        "--reason" => { reason = value; Ok(()) },
        "--observed-at" => { observed_at = parse_u64(&value, "--observed-at")?; Ok(()) },
        _ => return Err(format!("unexpected argument: {flag}")),
    })?;
    Ok((journal.ok_or("governance requires --journal")?, measurement_id.ok_or("governance requires --measurement-id")?, rollback.ok_or("governance requires --rollback-count")?, exceptions.ok_or("governance requires --active-exception-count")?, oldest.ok_or("governance requires --oldest-exception-age-secs")?, actor, reason, observed_at))
}

fn parse_simple_options<F>(args: Vec<String>, mut apply: F) -> Result<(), String>
where
    F: FnMut(&str, String) -> Result<(), String>,
{
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        let value = args.get(index).cloned().ok_or_else(|| format!("missing value for {flag}"))?;
        index += 1;
        apply(flag, value)?;
    }
    Ok(())
}

fn parse_report_options(args: Vec<String>) -> Result<ReportOptions, String> {
    let mut options = ReportOptions::default();
    let mut index = 0;
    while index < args.len() {
        let flag = &args[index];
        index += 1;
        let value = args.get(index).cloned().ok_or_else(|| format!("missing value for {flag}"))?;
        index += 1;
        match flag.as_str() {
            "--journal" => options.journal = PathBuf::from(value),
            "--output" => options.output = PathBuf::from(value),
            "--mode" => options.mode = Some(Mode::parse(&value)?),
            "--window-start" => options.window_start = parse_u64(&value, flag)?,
            "--window-end" => options.window_end = parse_u64(&value, flag)?,
            "--expected-repositories" => options.expected_repositories = parse_u64(&value, flag)? as usize,
            "--latency-baseline-millis" => options.latency_baseline_millis = Some(parse_u64(&value, flag)?),
            "--memory-baseline-bytes" => options.memory_baseline_bytes = Some(parse_u64(&value, flag)?),
            _ => return Err(format!("unexpected argument: {flag}")),
        }
    }
    if options.journal.as_os_str().is_empty() { return Err("report requires --journal".into()); }
    if options.output.as_os_str().is_empty() { return Err("report requires --output".into()); }
    if options.window_start >= options.window_end { return Err("window start must be before window end".into()); }
    if options.expected_repositories == 0 { return Err("expected repositories must be positive".into()); }
    Ok(options)
}

fn run_baseline(options: ScanOptions) -> Result<(), String> {
    let started = Instant::now();
    let scan = execute_scan(&options).map_err(|error| format!("cannot establish baseline: {error}"))?;
    let preview = preview_for(&scan, &options, None);
    if !preview.completeness.complete { return Err("baseline scan was incomplete".into()); }
    let mut lines = vec![
        "format=erf-pilot-baseline-v0".to_string(),
        format!("repository={}", options.repository),
        format!("commit={}", options.commit),
        format!("rule_id={}", scan.rule_id),
    ];
    for finding in &preview.findings {
        lines.push(format!("fingerprint={}", finding.fingerprint));
    }
    let output = options.output.expect("baseline output validated");
    let contents = lines.join("\n") + "\n";
    atomic_write(&output, contents.as_bytes())?;
    println!("baseline=PASS repository={} findings={} elapsed_ms={} evidence={}", options.repository, preview.findings.len(), elapsed_millis(started), output.display());
    Ok(())
}

fn run_observation(options: ScanOptions) -> Result<(), String> {
    let journal = options.journal.clone().expect("journal validated");
    let baseline = options.baseline.as_deref().map(read_baseline).transpose()?;
    let started = Instant::now();
    let scan_result = execute_scan(&options);
    let elapsed = elapsed_millis(started);
    let peak_memory = read_rss_bytes().max(1);
    let (scan, preview, failure) = match scan_result {
        Ok(scan) => {
            let preview = preview_for(&scan, &options, baseline.as_ref());
            (Some(scan), preview, None)
        }
        Err(error) => {
            let message = error.to_string();
            let preview = incomplete_preview(&error);
            (None, preview, Some(message))
        }
    };
    let result = local_result(&options, scan.as_ref(), &preview, failure.as_deref());
    let mut ledger = if journal.is_file() {
        PilotMetricsLedger::from_journal(&fs::read(&journal).map_err(|error| format!("cannot read journal: {error}"))?)
            .map_err(|error| format!("cannot restore journal: {error}"))?
    } else {
        PilotMetricsLedger::new()
    };
    let measurement_id = options.measurement_id.clone().unwrap_or_else(|| format!("{}:{}:{}", options.mode.as_str(), options.repository, options.commit));
    let operation = ledger.last_operation_sequence() + 1;
    let outcome = ledger.record_result_with_mode(
        &measurement_id,
        options.mode.metric_mode(),
        &result,
        elapsed,
        peak_memory,
        options.observed_at.unwrap_or_else(now_epoch),
        &options.actor,
        &options.reason,
        operation,
    ).map_err(|error| format!("cannot record observation: {error}"))?;
    atomic_write(&journal, &ledger.export_journal().map_err(|error| format!("cannot export journal: {error}"))?)?;
    println!("observation={} mode={} repository={} commit={} measurement_id={} decision={:?} complete={} new_findings={} historical_findings={} elapsed_ms={} peak_memory_bytes={} journal={}",
        match outcome { erf_pilot_metrics::MetricOutcome::Admitted(_) => "ADMITTED", erf_pilot_metrics::MetricOutcome::Idempotent(_) => "IDEMPOTENT" },
        options.mode.as_str(), options.repository, options.commit, measurement_id, result.decision, result.completeness.complete,
        result.new_finding_count, result.historical_finding_count, elapsed, peak_memory, journal.display());
    if result.decision == GateDecision::Inconclusive || !result.completeness.complete {
        eprintln!("observation recorded as INCONCLUSIVE");
        std::process::exit(3);
    }
    Ok(())
}

fn record_feedback(options: (PathBuf, String, FeedbackClass, RegressionOutcome, String, String, u64)) -> Result<(), String> {
    let (journal, measurement_id, classification, regression, actor, reason, observed_at) = options;
    let mut ledger = restore_ledger(&journal)?;
    let operation = ledger.last_operation_sequence() + 1;
    ledger.record_feedback(&measurement_id, classification, regression, observed_at, &actor, &reason, operation)
        .map_err(|error| format!("cannot record feedback: {error}"))?;
    atomic_write(&journal, &ledger.export_journal().map_err(|error| format!("cannot export journal: {error}"))?)?;
    println!("feedback=ADMITTED measurement_id={} journal={}", measurement_id, journal.display());
    Ok(())
}

fn record_governance(options: (PathBuf, String, u64, u64, u64, String, String, u64)) -> Result<(), String> {
    let (journal, measurement_id, rollback, exceptions, oldest, actor, reason, observed_at) = options;
    let mut ledger = restore_ledger(&journal)?;
    let operation = ledger.last_operation_sequence() + 1;
    ledger.record_governance(&measurement_id, rollback, exceptions, oldest, observed_at, &actor, &reason, operation)
        .map_err(|error| format!("cannot record governance: {error}"))?;
    atomic_write(&journal, &ledger.export_journal().map_err(|error| format!("cannot export journal: {error}"))?)?;
    println!("governance=ADMITTED measurement_id={} journal={}", measurement_id, journal.display());
    Ok(())
}

fn write_report(options: ReportOptions) -> Result<(), String> {
    let ledger = restore_ledger(&options.journal)?;
    let runs = ledger.run_trend().iter().filter(|run| {
        run.observed_at_epoch_secs >= options.window_start
            && run.observed_at_epoch_secs <= options.window_end
            && options.mode.map(|mode| run.mode == mode.metric_mode()).unwrap_or(true)
    }).collect::<Vec<_>>();
    let mode = options.mode.map(Mode::as_str).unwrap_or("UNSPECIFIED");
    let repository_count = runs.iter().map(|run| run.repository_digest).collect::<BTreeSet<_>>().len();
    let complete_count = runs.iter().filter(|run| run.complete).count();
    let inconclusive_count = runs.iter().filter(|run| run.decision == GateDecision::Inconclusive).count();
    let pass_count = runs.iter().filter(|run| run.decision == GateDecision::Pass).count();
    let fail_count = runs.iter().filter(|run| run.decision == GateDecision::Fail).count();
    let false_pass_count = runs.iter().filter(|run| run.decision == GateDecision::Pass && !run.complete).count();
    let complete_rate = ratio(complete_count, runs.len());
    let inconclusive_rate = ratio(inconclusive_count, runs.len());
    let p95_latency = percentile(runs.iter().map(|run| run.latency_millis).collect());
    let max_memory = runs.iter().map(|run| run.peak_memory_bytes).max().unwrap_or(0);
    let (new_first, new_last, historical_first, historical_last) = trend_endpoints(&runs);
    let new_trend = new_last as i64 - new_first as i64;
    let historical_trend = historical_last as i64 - historical_first as i64;
    let mut checks = Vec::new();
    checks.push(check("repository coverage", repository_count >= options.expected_repositories && !runs.is_empty()));
    checks.push(check("complete-rate >= 0.99", complete_rate.map(|value| value >= 0.99).unwrap_or(false)));
    checks.push(check("inconclusive-rate <= 0.01", inconclusive_rate.map(|value| value <= 0.01).unwrap_or(false)));
    checks.push(check("false-pass-count = 0", !runs.is_empty() && false_pass_count == 0));
    checks.push(check("new-violation-trend <= 0", !runs.is_empty() && new_trend <= 0));
    checks.push(check("historical-debt-trend <= 0", !runs.is_empty() && historical_trend <= 0));
    checks.push(match options.latency_baseline_millis {
        Some(baseline) if baseline > 0 => check("p95 latency <= baseline x2", p95_latency.map(|value| value <= baseline.saturating_mul(2)).unwrap_or(false)),
        _ => ("p95 latency <= baseline x2", "INCONCLUSIVE"),
    });
    checks.push(match options.memory_baseline_bytes {
        Some(baseline) if baseline > 0 => check("peak memory <= baseline x1.5", max_memory > 0 && max_memory <= baseline.saturating_mul(3) / 2),
        _ => ("peak memory <= baseline x1.5", "INCONCLUSIVE"),
    });
    checks.push(("precision/recall/false-positive regression", if ledger.feedback_measurements().is_empty() { "INCONCLUSIVE" } else { "INCONCLUSIVE" }));
    let overall = if checks.iter().any(|(_, status)| *status == "FAIL") { "FAIL" } else if checks.iter().any(|(_, status)| *status == "INCONCLUSIVE") { "INCONCLUSIVE" } else { "PASS" };
    let mut report = String::new();
    report.push_str("# Pilot Observation Window Report\n\n");
    report.push_str(&format!("- Window: `{}` through `{}` (inclusive)\n", options.window_start, options.window_end));
    report.push_str(&format!("- Mode: `{mode}`\n- Overall: `{overall}`\n- Authority: `SHADOW` only\n- Runs: `{}`\n- Repositories observed: `{repository_count}`\n- PASS/FAIL/INCONCLUSIVE: `{pass_count}/{fail_count}/{inconclusive_count}`\n- Complete rate: `{}`\n- Inconclusive rate: `{}`\n- p95 latency millis: `{}`\n- Peak memory bytes: `{max_memory}`\n- New finding trend delta: `{new_trend}`\n- Historical finding trend delta: `{historical_trend}`\n\n", runs.len(), format_ratio(complete_rate), format_ratio(inconclusive_rate), p95_latency.map(|value| value.to_string()).unwrap_or_else(|| "INCONCLUSIVE".into())));
    report.push_str("| Check | Result |\n| --- | --- |\n");
    for (name, status) in checks { report.push_str(&format!("| {name} | {status} |\n")); }
    report.push_str("\nThis report contains digest-only aggregates. It does not publish SCM status, upload source or full Findings, or authorize protected-branch enforcement. Missing or unmeasured data remains INCONCLUSIVE.\n");
    atomic_write(&options.output, report)?;
    println!("window-report={} evidence={}", overall, options.output.display());
    match overall {
        "PASS" => Ok(()),
        "INCONCLUSIVE" => {
            eprintln!("observation window is INCONCLUSIVE");
            std::process::exit(3);
        }
        _ => {
            eprintln!("observation window is FAIL");
            std::process::exit(1);
        }
    }
}

fn execute_scan(options: &ScanOptions) -> Result<rf_runner::ScanResult, ScanError> {
    scan(&ScanConfig {
        repo_root: options.repo_root.clone(),
        rule_id: String::new(),
        rule_bundle: Some(options.candidate.clone()),
        resolver_command: options.resolver.clone(),
        resolver_args: options.resolver_args.clone(),
        host_command: options.host.clone(),
        host_args: options.host_args.clone(),
        host_deadline_ms: 0,
        max_staging_bytes: 64 * 1024 * 1024,
        execution_lock: None,
        signed_execution_lock: None,
        sandbox_policy: None,
    })
}

fn preview_for(scan: &rf_runner::ScanResult, options: &ScanOptions, baseline: Option<&BTreeSet<String>>) -> GatePreview {
    let baseline = baseline.map(|fingerprints| Baseline { kind: BaselineKind::Comparison, finding_fingerprints: fingerprints.clone() });
    let policy = (!options.blocking_rules.is_empty()).then_some(GatePolicy { blocking_rule_ids: options.blocking_rules.clone() });
    rf_gate::preview_with_policy_and_baseline_and_exceptions(&scan.planned_unit_ids, &scan.unit_statuses, &scan.committed_findings, baseline.as_ref(), &options.exceptions, policy.as_ref())
}

fn incomplete_preview(error: &ScanError) -> GatePreview {
    let mut preview = rf_gate::preview(&[], &[], &[]);
    preview.decision = GateDecision::Inconclusive;
    preview.completeness = Completeness { planned: 0, accounted: 0, failed: 0, missing: 0, observation_errors: 1, complete: false };
    preview.input_summary = format!("observation error: {error}");
    preview
}

fn local_result(options: &ScanOptions, scan: Option<&rf_runner::ScanResult>, preview: &GatePreview, failure: Option<&str>) -> UnifiedResult {
    let rule_bundle_digest = candidate_digest(Path::new(&options.candidate)).unwrap_or_else(|_| digest_text("invalid-candidate"));
    let workspace_digest = scan.map(|value| value.workspace_descriptor_digest.as_slice()).filter(|value| value.len() == 32).map(hex_digest).unwrap_or_else(|| digest_text(&format!("workspace:{}:{}", options.repository, options.commit)));
    let plan_digest = scan.map(|value| fixed_digest(&value.plan_digest, "plan")).unwrap_or_else(|| fixed_digest(failure.unwrap_or("plan").as_bytes(), "plan"));
    let attempt_digest = scan.map(|value| fixed_digest(&value.attempt_transcript_digest, "attempt")).unwrap_or_else(|| fixed_digest(failure.unwrap_or("attempt").as_bytes(), "attempt"));
    let mut audit = AuditTrail::new();
    audit.append(AuditEventKind::ScanPlanned, plan_digest);
    audit.append(AuditEventKind::ExecutionCompleted, attempt_digest);
    audit.append(AuditEventKind::FindingSetComputed, preview.semantic_result_digest);
    audit.append(AuditEventKind::GateEvaluated, preview.semantic_result_digest);
    UnifiedResult::from_preview(ResultInputs {
        repository: options.repository.clone(),
        commit: options.commit.clone(),
        policy_digest: options.policy_digest.clone().unwrap_or_else(|| digest_text("pilot-policy-v0")),
        rule_bundle_digest,
        execution_lock_digest: options.execution_lock_digest.clone().unwrap_or_else(|| digest_text(&format!("shadow-lock:{}", options.mode.as_str()))),
        runner_release_digest: options.runner_release_digest.clone().unwrap_or_else(|| digest_text("pilot-runner-v0")),
        workspace_descriptor_digest: workspace_digest,
    }, preview, audit).expect("observer constructs a valid local UnifiedResult")
}

fn read_baseline(path: &Path) -> Result<BTreeSet<String>, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("cannot read baseline: {error}"))?;
    let mut fingerprints = BTreeSet::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("fingerprint=") {
            if value.trim().is_empty() || value.chars().any(char::is_control) { return Err("baseline has an invalid fingerprint".into()); }
            fingerprints.insert(value.to_string());
        }
    }
    Ok(fingerprints)
}

fn candidate_digest(path: &Path) -> Result<String, String> {
    let text = fs::read_to_string(path.join("manifest.json")).map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    value.get("digest").and_then(Value::as_str).filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())).map(str::to_string).ok_or_else(|| "candidate digest is missing or invalid".into())
}

fn restore_ledger(path: &Path) -> Result<PilotMetricsLedger, String> {
    if !path.is_file() { return Ok(PilotMetricsLedger::new()); }
    PilotMetricsLedger::from_journal(&fs::read(path).map_err(|error| format!("cannot read journal: {error}"))?).map_err(|error| format!("cannot restore journal: {error}"))
}

fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|error| format!("cannot create evidence directory: {error}"))?; }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = fs::File::create(&temp).map_err(|error| format!("cannot create temporary evidence file: {error}"))?;
    file.write_all(contents.as_ref()).map_err(|error| format!("cannot write evidence: {error}"))?;
    file.sync_all().map_err(|error| format!("cannot sync evidence: {error}"))?;
    fs::rename(&temp, path).map_err(|error| format!("cannot publish evidence atomically: {error}"))
}

fn parse_u64(value: &str, flag: &str) -> Result<u64, String> {
    value.parse().map_err(|_| format!("invalid value for {flag}: {value}"))
}

fn parse_classification(value: &str) -> Result<FeedbackClass, String> {
    match value {
        "valid" => Ok(FeedbackClass::ConfirmedValidFinding),
        "documentation" => Ok(FeedbackClass::DocumentationGap),
        "scope" => Ok(FeedbackClass::ScopeIssue),
        "false-positive" => Ok(FeedbackClass::ConfirmedFalsePositive),
        _ => Err(format!("invalid classification: {value}")),
    }
}

fn parse_regression(value: &str) -> Result<RegressionOutcome, String> {
    match value {
        "pending" => Ok(RegressionOutcome::Pending),
        "clean" => Ok(RegressionOutcome::Clean),
        "finding" => Ok(RegressionOutcome::FindingDetected),
        "n/a" => Ok(RegressionOutcome::NotApplicable),
        _ => Err(format!("invalid regression outcome: {value}")),
    }
}

fn now_epoch() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_secs()).unwrap_or(0)
}

fn elapsed_millis(started: Instant) -> u64 {
    started.elapsed().as_millis().max(1) as u64
}

fn read_rss_bytes() -> u64 {
    let Ok(text) = fs::read_to_string("/proc/self/status") else { return 1; };
    text.lines().find_map(|line| line.strip_prefix("VmRSS:").and_then(|value| value.split_whitespace().next()).and_then(|value| value.parse::<u64>().ok()).map(|kb| kb.saturating_mul(1024))).unwrap_or(1)
}

fn digest_text(value: &str) -> String {
    let digest: [u8; 32] = Sha256::digest(value.as_bytes()).into();
    hex_digest(&digest)
}

fn fixed_digest(value: &[u8], label: &str) -> [u8; 32] {
    if value.len() == 32 { return value.try_into().expect("length checked"); }
    Sha256::digest([label.as_bytes(), value].concat()).into()
}

fn hex_digest(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
    (denominator > 0).then_some(numerator as f64 / denominator as f64)
}

fn format_ratio(value: Option<f64>) -> String {
    value.map(|value| format!("{value:.4}")).unwrap_or_else(|| "INCONCLUSIVE".into())
}

fn percentile(mut values: Vec<u64>) -> Option<u64> {
    if values.is_empty() { return None; }
    values.sort_unstable();
    let index = ((values.len() as f64 * 0.95).ceil() as usize).saturating_sub(1).min(values.len() - 1);
    Some(values[index])
}

fn trend_endpoints(runs: &[&erf_pilot_metrics::RunMeasurement]) -> (u64, u64, u64, u64) {
    let mut grouped: HashMap<[u8; 32], Vec<&erf_pilot_metrics::RunMeasurement>> = HashMap::new();
    for run in runs { grouped.entry(run.repository_digest).or_default().push(*run); }
    let mut first_new = 0;
    let mut last_new = 0;
    let mut first_historical = 0;
    let mut last_historical = 0;
    for values in grouped.values_mut() {
        values.sort_by_key(|run| run.observed_at_epoch_secs);
        first_new += values.first().map(|run| run.new_finding_count as u64).unwrap_or(0);
        last_new += values.last().map(|run| run.new_finding_count as u64).unwrap_or(0);
        first_historical += values.first().map(|run| run.historical_finding_count as u64).unwrap_or(0);
        last_historical += values.last().map(|run| run.historical_finding_count as u64).unwrap_or(0);
    }
    (first_new, last_new, first_historical, last_historical)
}

fn check(name: &'static str, passed: bool) -> (&'static str, &'static str) {
    if passed { (name, "PASS") } else { (name, "FAIL") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_is_ceil_p95_and_deterministic() {
        assert_eq!(percentile(vec![1, 2, 3, 4, 5]), Some(5));
        assert_eq!(percentile(vec![5, 1, 3, 2, 4]), Some(5));
        assert_eq!(percentile(Vec::new()), None);
    }

    #[test]
    fn mode_names_are_explicit() {
        assert_eq!(Mode::LocalSimulation.as_str(), "LOCAL_SIMULATION");
        assert_eq!(Mode::LiveShadow.as_str(), "LIVE_SHADOW");
    }
}
