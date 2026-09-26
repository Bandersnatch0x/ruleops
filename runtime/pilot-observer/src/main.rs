use erf_feedback::FeedbackClass;
use erf_incremental_baseline::{digest_summary, Stage, StageTimer};
use erf_pilot_metrics::{ObservationMode, PilotMetricsLedger, RegressionOutcome};
use erf_project_config::{apply_precedence, load_config, process_environment, ConfigOverrides};
use erf_unified_result::{AuditEventKind, AuditTrail, ResultInputs, UnifiedResult};
use rf_gate::{Baseline, BaselineKind, Completeness, GateDecision, GatePolicy, GatePreview};
use rf_runner::{scan, ProjectScanScope, ScanConfig, ScanError};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const JOURNAL_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

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
    SyntheticShadow,
    LiveShadow,
}

impl Default for Mode {
    fn default() -> Self { Self::LiveShadow }
}

impl Mode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "local-simulation" => Ok(Self::LocalSimulation),
            "synthetic-shadow" => Ok(Self::SyntheticShadow),
            "live-shadow" => Ok(Self::LiveShadow),
            _ => Err(format!("invalid --mode: {value} (expected local-simulation, synthetic-shadow or live-shadow)")),
        }
    }

    fn metric_mode(self) -> ObservationMode {
        match self {
            Self::LocalSimulation => ObservationMode::LocalSimulation,
            Self::SyntheticShadow => ObservationMode::SyntheticShadow,
            Self::LiveShadow => ObservationMode::LiveShadow,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::LocalSimulation => "LOCAL_SIMULATION",
            Self::SyntheticShadow => "SYNTHETIC_SHADOW",
            Self::LiveShadow => "LIVE_SHADOW",
        }
    }
}

#[derive(Debug, Default)]
struct ScanOptions {
    repository: String,
    commit: String,
    repo_root: String,
    project_config: Option<PathBuf>,
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
    registered_sample: Option<PathBuf>,
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
        "usage:\n  erf-pilot-observer baseline --repository ID --commit SHA --repo-root PATH --project-config FILE --candidate DIR --host CMD --resolver CMD --output FILE [--host-arg ARG] [--resolver-arg ARG]\n  erf-pilot-observer run --repository ID --commit SHA --repo-root PATH --project-config FILE --candidate DIR --host CMD --resolver CMD --journal FILE --mode local-simulation|synthetic-shadow|live-shadow [options]\n  erf-pilot-observer feedback --journal FILE --measurement-id ID --classification valid|documentation|scope|false-positive --regression pending|clean|finding [options]\n  erf-pilot-observer governance --journal FILE --measurement-id ID --rollback-count N --active-exception-count N --oldest-exception-age-secs N [options]\n  erf-pilot-observer report --journal FILE --output FILE --window-start EPOCH --window-end EPOCH --expected-repositories N --mode local-simulation|synthetic-shadow|live-shadow [--registered-sample FILE] [--latency-baseline-millis N] [--memory-baseline-bytes N]"
    );
}

fn parse_scan_options(args: Vec<String>, requires_journal: bool) -> Result<ScanOptions, String> {
    let mut options = ScanOptions { mode: Mode::LiveShadow, actor: "pilot-observer".into(), reason: "observation run".into(), ..ScanOptions::default() };
    let mut mode_supplied = false;
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
            "--project-config" => options.project_config = Some(PathBuf::from(take()?)),
            "--candidate" => options.candidate = take()?,
            "--host" => options.host = take()?,
            "--host-arg" => options.host_args.push(take()?),
            "--resolver" => options.resolver = take()?,
            "--resolver-arg" => options.resolver_args.push(take()?),
            "--baseline" => options.baseline = Some(PathBuf::from(take()?)),
            "--exception-fingerprint" => { options.exceptions.insert(take()?); }
            "--blocking-rule-id" => { options.blocking_rules.insert(take()?); }
            "--mode" => { options.mode = Mode::parse(&take()?)?; mode_supplied = true; }
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
    if requires_journal && !mode_supplied { return Err("run requires --mode".into()); }
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
            "--registered-sample" => options.registered_sample = Some(PathBuf::from(value)),
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
    let mode = options.mode.ok_or("report requires --mode")?;
    if matches!(mode, Mode::LiveShadow | Mode::SyntheticShadow) && options.registered_sample.is_none() {
        return Err(format!("{} report requires --registered-sample", mode.as_str()));
    }
    if options.window_start >= options.window_end { return Err("window start must be before window end".into()); }
    if options.expected_repositories == 0 { return Err("expected repositories must be positive".into()); }
    Ok(options)
}

fn run_baseline(options: ScanOptions) -> Result<(), String> {
    let started = Instant::now();
    let project_scope = project_scope_for(&options)?;
    let mut scan = execute_scan(&options, project_scope.as_ref()).map_err(|error| format!("cannot establish baseline: {error}"))?;
    let preview = preview_for(&mut scan, &options, None);
    if !preview.completeness.complete { return Err("baseline scan was incomplete".into()); }
    let mut lines = vec![
        "format=erf-pilot-baseline-v0".to_string(),
        format!("repository={}", options.repository),
        format!("commit={}", options.commit),
        format!("rule_id={}", scan.rule_id),
        format!("rule_bundle_digest={}", candidate_digest(Path::new(&options.candidate))?),
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
    let project_scope = project_scope_for(&options)?;
    let started = Instant::now();
    let scan_result = execute_scan(&options, project_scope.as_ref());
    let elapsed = elapsed_millis(started);
    let peak_memory = read_rss_bytes().max(1);
    let (scan, preview, failure) = match scan_result {
        Ok(mut scan) => {
            let preview = preview_for(&mut scan, &options, baseline.as_ref());
            (Some(scan), preview, None)
        }
        Err(error) => {
            let message = error.to_string();
            let preview = incomplete_preview(&error);
            (None, preview, Some(message))
        }
    };
    let result = local_result(&options, scan.as_ref(), &preview, failure.as_deref());
    let measurement_id = options.measurement_id.clone().unwrap_or_else(|| format!("{}:{}:{}", options.mode.as_str(), options.repository, options.commit));
    let admission = with_journal_lock(&journal, || {
        let mut ledger = restore_ledger(&journal)?;
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
        Ok(match outcome {
            erf_pilot_metrics::MetricOutcome::Admitted(_) => "ADMITTED",
            erf_pilot_metrics::MetricOutcome::Idempotent(_) => "IDEMPOTENT",
        })
    })?;
    println!("observation={} mode={} repository={} commit={} measurement_id={} run_identity_digest={} rule_bundle_digest={} decision={:?} complete={} new_findings={} historical_findings={} elapsed_ms={} peak_memory_bytes={} journal={}",
        admission,
        options.mode.as_str(), options.repository, options.commit, measurement_id, hex_digest(&digest_bytes(&measurement_id)),
        result.inputs.rule_bundle_digest, result.decision, result.completeness.complete,
        result.new_finding_count, result.historical_finding_count, elapsed, peak_memory, journal.display());
    if result.decision == GateDecision::Inconclusive || !result.completeness.complete {
        eprintln!("observation recorded as INCONCLUSIVE");
        std::process::exit(3);
    }
    Ok(())
}

fn record_feedback(options: (PathBuf, String, FeedbackClass, RegressionOutcome, String, String, u64)) -> Result<(), String> {
    let (journal, measurement_id, classification, regression, actor, reason, observed_at) = options;
    with_journal_lock(&journal, || {
        let mut ledger = restore_ledger(&journal)?;
        let operation = ledger.last_operation_sequence() + 1;
        ledger.record_feedback(&measurement_id, classification, regression, observed_at, &actor, &reason, operation)
            .map_err(|error| format!("cannot record feedback: {error}"))?;
        atomic_write(&journal, &ledger.export_journal().map_err(|error| format!("cannot export journal: {error}"))?)
    })?;
    println!("feedback=ADMITTED measurement_id={} journal={}", measurement_id, journal.display());
    Ok(())
}

fn record_governance(options: (PathBuf, String, u64, u64, u64, String, String, u64)) -> Result<(), String> {
    let (journal, measurement_id, rollback, exceptions, oldest, actor, reason, observed_at) = options;
    with_journal_lock(&journal, || {
        let mut ledger = restore_ledger(&journal)?;
        let operation = ledger.last_operation_sequence() + 1;
        ledger.record_governance(&measurement_id, rollback, exceptions, oldest, observed_at, &actor, &reason, operation)
            .map_err(|error| format!("cannot record governance: {error}"))?;
        atomic_write(&journal, &ledger.export_journal().map_err(|error| format!("cannot export journal: {error}"))?)
    })?;
    println!("governance=ADMITTED measurement_id={} journal={}", measurement_id, journal.display());
    Ok(())
}

fn write_report(options: ReportOptions) -> Result<(), String> {
    let ledger = with_journal_lock(&options.journal, || restore_ledger(&options.journal))?;
    let mode = options.mode.ok_or("report requires --mode")?;
    let runs = ledger.run_trend().iter().filter(|run| {
        run.observed_at_epoch_secs >= options.window_start
            && run.observed_at_epoch_secs <= options.window_end
            && run.mode == mode.metric_mode()
    }).collect::<Vec<_>>();
    if runs.is_empty() {
        return Err(format!("no observations for {} in the requested window", mode.as_str()));
    }
    let registered = options.registered_sample.as_deref().map(|path| read_registered_sample(path, mode)).transpose()?;
    let repository_count = runs.iter().map(|run| run.repository_digest).collect::<BTreeSet<_>>().len();
    if repository_count != options.expected_repositories {
        return Err(format!("{} repository identities observed, expected {}", repository_count, options.expected_repositories));
    }
    if let Some(sample) = registered.as_ref() {
        validate_registered_identities(&runs, sample, mode, options.expected_repositories)?;
    }
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
    let regression_feedback = ledger.feedback_measurements().iter()
        .filter(|feedback| feedback.regression_outcome != RegressionOutcome::NotApplicable)
        .collect::<Vec<_>>();
    let regression_status = if regression_feedback.is_empty() {
        "INCONCLUSIVE"
    } else if regression_feedback.iter().any(|feedback| feedback.regression_outcome == RegressionOutcome::FindingDetected) {
        "FAIL"
    } else if regression_feedback.iter().any(|feedback| feedback.regression_outcome == RegressionOutcome::Pending) {
        "INCONCLUSIVE"
    } else {
        "PASS"
    };
    checks.push(("precision/recall/false-positive regression", regression_status));
    let engineering_overall = if checks.iter().any(|(_, status)| *status == "FAIL") { "FAIL" } else if checks.iter().any(|(_, status)| *status == "INCONCLUSIVE") { "INCONCLUSIVE" } else { "PASS" };
    let engineering_qualification = if complete_count == runs.len() && false_pass_count == 0 { "PASS" } else { "FAIL" };
    let product_outcome = if mode == Mode::LiveShadow { engineering_overall } else { "INCONCLUSIVE" };
    let mut report = String::new();
    report.push_str("# Pilot Observation Window Report\n\n");
    report.push_str(&format!("- Window: `{}` through `{}` (inclusive)\n", options.window_start, options.window_end));
    report.push_str(&format!("- ObservationMode: `{}`\n- Engineering qualification: `{engineering_qualification}`\n- Engineering gate: `{engineering_overall}`\n- Product outcome: `{product_outcome}`\n- Overall: `{product_outcome}`\n- Product eligibility: `{}`\n- Authority: `SHADOW` only\n- Runs: `{}`\n- Repositories observed: `{repository_count}`\n- PASS/FAIL/INCONCLUSIVE: `{pass_count}/{fail_count}/{inconclusive_count}`\n- Complete rate: `{}`\n- Inconclusive rate: `{}`\n- p95 latency millis: `{}`\n- Peak memory bytes: `{max_memory}`\n- New finding trend delta: `{new_trend}`\n- Historical finding trend delta: `{historical_trend}`\n\n", mode.as_str(), if mode == Mode::LiveShadow { "LIVE_SHADOW" } else { "ENGINEERING_ONLY" }, runs.len(), format_ratio(complete_rate), format_ratio(inconclusive_rate), p95_latency.map(|value| value.to_string()).unwrap_or_else(|| "INCONCLUSIVE".into())));
    report.push_str("## Observation identities\n\n");
    report.push_str("| ObservationMode | Run identity | Run identity digest | Repository identity | Commit identity | Rule-bundle digest |\n| --- | --- | --- | --- | --- | --- |\n");
    for run in &runs {
        report.push_str(&format!("| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` |\n", run.mode.as_str(), run.measurement_id, hex_digest(&run.run_identity_digest), hex_digest(&run.repository_digest), hex_digest(&run.commit_digest), hex_digest(&run.rule_bundle_digest)));
    }
    report.push('\n');
    report.push_str("| Check | Result |\n| --- | --- |\n");
    for (name, status) in checks { report.push_str(&format!("| {name} | {status} |\n")); }
    report.push_str("\nThe report is mode-scoped. LOCAL_SIMULATION and SYNTHETIC_SHADOW are engineering qualification evidence only and are forced to Product outcome INCONCLUSIVE; they cannot enter LIVE_SHADOW product aggregation. Every row binds ObservationMode, run identity, repository identity, commit identity and rule-bundle digest. This report contains digest-only aggregates. It does not publish SCM status, upload source or full Findings, or authorize protected-branch enforcement. Missing or unmeasured data remains INCONCLUSIVE.\n");
    atomic_write(&options.output, report)?;
    println!("window-report={} engineering={} mode={} evidence={}", product_outcome, engineering_overall, mode.as_str(), options.output.display());
    match engineering_overall {
        "PASS" if product_outcome == "PASS" => Ok(()),
        "INCONCLUSIVE" => {
            eprintln!("observation window is INCONCLUSIVE");
            std::process::exit(3);
        }
        "PASS" => {
            eprintln!("engineering qualification passed; product outcome remains INCONCLUSIVE for {}", mode.as_str());
            std::process::exit(3);
        }
        _ => {
            eprintln!("observation window is FAIL");
            std::process::exit(1);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegisteredSample {
    identities: BTreeSet<([u8; 32], [u8; 32])>,
    rule_bundle_digest: [u8; 32],
}

fn read_registered_sample(path: &Path, expected_mode: Mode) -> Result<RegisteredSample, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("cannot read registered sample: {error}"))?;
    let mut lines = text.lines();
    if lines.next() != Some("format=erf-pilot-sample-v1") {
        return Err("registered sample format is invalid".into());
    }
    let expected_mode_line = format!("mode={}", expected_mode.as_str());
    if lines.next() != Some(expected_mode_line.as_str()) {
        return Err(format!("registered sample mode does not match {}", expected_mode.as_str()));
    }
    let mut identities = BTreeSet::new();
    let mut rule_bundle_digest = None;
    for line in lines {
        if line.trim().is_empty() { continue; }
        if let Some(value) = line.strip_prefix("rule-bundle=") {
            if rule_bundle_digest.is_some() { return Err("registered sample contains duplicate rule-bundle identity".into()); }
            rule_bundle_digest = Some(parse_fixed_digest(value)?);
            continue;
        }
        let value = line.strip_prefix("sample=").ok_or("registered sample contains an unknown record")?;
        let (repository, commit) = value.split_once('\t').ok_or("registered sample record must contain repository and commit")?;
        if repository.trim().is_empty() || commit.trim().is_empty() || repository.chars().any(char::is_control) || commit.chars().any(char::is_control) {
            return Err("registered sample contains an invalid repository or commit".into());
        }
        if !identities.insert((digest_bytes(repository), digest_bytes(commit))) {
            return Err("registered sample contains a duplicate repository/commit identity".into());
        }
    }
    if identities.is_empty() { return Err("registered sample is empty".into()); }
    Ok(RegisteredSample { identities, rule_bundle_digest: rule_bundle_digest.ok_or("registered sample is missing rule-bundle identity")? })
}

fn validate_registered_identities(
    runs: &[&erf_pilot_metrics::RunMeasurement],
    registered: &RegisteredSample,
    mode: Mode,
    expected_repositories: usize,
) -> Result<(), String> {
    if let Some(run) = runs.iter().find(|run| run.mode != mode.metric_mode()) {
        return Err(format!("{} report received a {} observation", mode.as_str(), run.mode.as_str()));
    }
    if let Some(run) = runs.iter().find(|run| run.rule_bundle_digest != registered.rule_bundle_digest) {
        return Err(format!("{} observation rule-bundle digest does not match the registered bundle: {}", mode.as_str(), hex_digest(&run.rule_bundle_digest)));
    }
    let observed = runs.iter().map(|run| (run.repository_digest, run.commit_digest)).collect::<BTreeSet<_>>();
    let registered_repositories = registered.identities.iter().map(|(repository, _)| *repository).collect::<BTreeSet<_>>();
    if registered_repositories.len() != expected_repositories {
        return Err(format!("registered {} sample has {} repository identities, expected {}", mode.as_str(), registered_repositories.len(), expected_repositories));
    }
    if let Some(identity) = observed.difference(&registered.identities).next() {
        return Err(format!("{} observation identity is not registered: repository={} commit={}", mode.as_str(), hex_digest(&identity.0), hex_digest(&identity.1)));
    }
    if let Some(identity) = registered.identities.difference(&observed).next() {
        return Err(format!("{} registered identity was not observed: repository={} commit={}", mode.as_str(), hex_digest(&identity.0), hex_digest(&identity.1)));
    }
    Ok(())
}

fn execute_scan(options: &ScanOptions, project_scope: Option<&ProjectScanScope>) -> Result<rf_runner::ScanResult, ScanError> {
    scan(&ScanConfig {
        repo_root: options.repo_root.clone(),
        rule_id: String::new(),
        rule_bundle: Some(options.candidate.clone()),
        bundle_delivery: None,
        resolver_command: options.resolver.clone(),
        resolver_args: options.resolver_args.clone(),
        host_command: options.host.clone(),
        host_args: options.host_args.clone(),
        host_deadline_ms: 0,
        resolver_deadline_ms: 0,
        max_staging_bytes: 64 * 1024 * 1024,
        execution_lock: None,
        signed_execution_lock: None,
        resolver_sandbox_policy: None,
        sandbox_policy: None,
        workspace_attestation: None,
        frozen_plan: None,
        incremental: None,
        scan_cache_dir: None,
        fallback_inputs: None,
        project_scope: project_scope.cloned(),
    })
}

fn project_scope_for(options: &ScanOptions) -> Result<Option<ProjectScanScope>, String> {
    let Some(config_path) = options.project_config.as_deref() else {
        return Ok(None);
    };
    let repo_root = Path::new(&options.repo_root);
    let canonical_repo = fs::canonicalize(repo_root).map_err(|error| format!("cannot resolve repository root: {error}"))?;
    let requested_config = if config_path.is_absolute() { config_path.to_path_buf() } else { repo_root.join(config_path) };
    let canonical_config = fs::canonicalize(&requested_config).map_err(|error| format!("cannot resolve project config: {error}"))?;
    if !canonical_config.starts_with(&canonical_repo) {
        return Err("project config is outside the repository root".into());
    }
    let (project, _) = load_config(&canonical_repo, Some(&canonical_config)).map_err(|error| format!("cannot load project config: {error}"))?;
    let (project, _) = apply_precedence(&project, &ConfigOverrides::default(), &process_environment())
        .map_err(|error| format!("cannot resolve project config: {error}"))?;
    let scope = ProjectScanScope {
        modules: project.modules,
        roots: project.source_roots,
        include: project.include,
        exclude: project.exclude,
    };
    scope.validate().map_err(|error| format!("invalid project scan scope: {error}"))?;
    Ok(Some(scope))
}

fn preview_for(scan: &mut rf_runner::ScanResult, options: &ScanOptions, baseline: Option<&BTreeSet<String>>) -> GatePreview {
    let baseline = baseline.map(|fingerprints| Baseline { kind: BaselineKind::Comparison, finding_fingerprints: fingerprints.clone() });
    let policy = (!options.blocking_rules.is_empty()).then_some(GatePolicy { blocking_rule_ids: options.blocking_rules.clone() });
    let gate_timer = StageTimer::start(Stage::Gate, digest_summary(&format!("pilot-observer-gate:{:?}", scan.plan_digest)))
        .expect("gate telemetry input digest is always valid");
    let mut preview = rf_gate::preview_with_policy_and_baseline_and_exceptions_and_delivery_and_observation_errors(&scan.planned_unit_ids, &scan.unit_statuses, &scan.committed_findings, baseline.as_ref(), &options.exceptions, policy.as_ref(), None, scan.resolver_required_unresolved_facts.len());
    let telemetry_error = if let Some(mut telemetry) = scan.stage_telemetry.take() {
        match telemetry.replace_measurement(gate_timer.finish()) {
            Ok(()) if telemetry.has_all_stage_summaries() => {
                scan.stage_telemetry = Some(telemetry);
                None
            }
            Ok(()) => Some("missing stage summary before Gate preview".into()),
            Err(error) => Some(error.to_string()),
        }
    } else {
        Some("missing stage telemetry before Gate preview".into())
    };
    if let Some(error) = telemetry_error {
        preview.decision = GateDecision::Inconclusive;
        preview.completeness.complete = false;
        preview.completeness.observation_errors = preview.completeness.observation_errors.saturating_add(1);
        preview.input_summary = format!("Gate telemetry rejected: {error}");
    }
    if scan.fallback_decision.as_ref().is_some_and(|decision| !decision.allows_authoritative_gate()) {
        preview.decision = GateDecision::Inconclusive;
        preview.completeness.complete = false;
        preview.completeness.observation_errors = preview.completeness.observation_errors.saturating_add(1);
        preview.input_summary = "incremental route is diagnostic-only until a FULL authority result is paired".into();
    }
    preview
}

fn incomplete_preview(error: &ScanError) -> GatePreview {
    let mut preview = rf_gate::preview(&[], &[], &[]);
    preview.decision = GateDecision::Inconclusive;
    preview.completeness = Completeness { planned: 0, accounted: 0, failed: 0, missing: 0, observation_errors: 1, complete: false };
    preview.input_summary = format!("observation error: {error}");
    preview
}

fn local_result(options: &ScanOptions, scan: Option<&rf_runner::ScanResult>, preview: &GatePreview, failure: Option<&str>) -> UnifiedResult {
    let candidate = Path::new(&options.candidate);
    let rule_bundle_digest = candidate_digest(candidate).unwrap_or_else(|error| digest_text(&format!("invalid-candidate:path={}:error={error}", candidate.display())));
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

struct JournalLock {
    path: PathBuf,
}

impl JournalLock {
    fn acquire(journal: &Path) -> Result<Self, String> {
        if let Some(parent) = journal.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("cannot create journal directory: {error}"))?;
        }
        let file_name = journal.file_name().ok_or("journal path must name a file")?;
        let lock_path = journal.with_file_name(format!("{}.lock", file_name.to_string_lossy()));
        let deadline = Instant::now() + JOURNAL_LOCK_TIMEOUT;
        loop {
            match OpenOptions::new().write(true).create_new(true).open(&lock_path) {
                Ok(mut file) => {
                    let lock = Self { path: lock_path };
                    if let Err(error) = writeln!(file, "pid={}", std::process::id()) {
                        drop(lock);
                        return Err(format!("cannot write journal lock: {error}"));
                    }
                    if let Err(error) = file.sync_all() {
                        drop(lock);
                        return Err(format!("cannot sync journal lock: {error}"));
                    }
                    return Ok(lock);
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists && Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    return Err(format!("journal lock is held beyond {:?}: {}", JOURNAL_LOCK_TIMEOUT, lock_path.display()));
                }
                Err(error) => return Err(format!("cannot acquire journal lock: {error}")),
            }
        }
    }
}

impl Drop for JournalLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn with_journal_lock<T>(journal: &Path, operation: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let _lock = JournalLock::acquire(journal)?;
    operation()
}

fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|error| format!("cannot create evidence directory: {error}"))?; }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&temp).map_err(|error| format!("cannot create temporary evidence file: {error}"))?;
        file.write_all(contents.as_ref()).map_err(|error| format!("cannot write evidence: {error}"))?;
        file.sync_all().map_err(|error| format!("cannot sync evidence: {error}"))?;
        fs::rename(&temp, path).map_err(|error| format!("cannot publish evidence atomically: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
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
    hex_digest(&digest_bytes(value))
}

fn digest_bytes(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

fn parse_fixed_digest(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("registered sample rule-bundle digest must be 64 hexadecimal characters".into());
    }
    let mut digest = [0; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        digest[index] = (hex_nibble(chunk[0]) << 4) | hex_nibble(chunk[1]);
    }
    Ok(digest)
}

fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => unreachable!("parse_fixed_digest only accepts hexadecimal input"),
    }
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
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

    #[test]
    fn percentile_is_ceil_p95_and_deterministic() {
        assert_eq!(percentile(vec![1, 2, 3, 4, 5]), Some(5));
        assert_eq!(percentile(vec![5, 1, 3, 2, 4]), Some(5));
        assert_eq!(percentile(Vec::new()), None);
    }

    #[test]
    fn mode_names_are_explicit() {
        assert_eq!(Mode::LocalSimulation.as_str(), "LOCAL_SIMULATION");
        assert_eq!(Mode::SyntheticShadow.as_str(), "SYNTHETIC_SHADOW");
        assert_eq!(Mode::LiveShadow.as_str(), "LIVE_SHADOW");
    }

    #[test]
    fn journal_lock_serializes_competing_writers() {
        let directory = std::env::temp_dir().join(format!(
            "erf-pilot-observer-lock-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let journal = directory.join("journal.bin");
        let first = JournalLock::acquire(&journal).unwrap();
        let acquired = Arc::new(AtomicBool::new(false));
        let thread_acquired = Arc::clone(&acquired);
        let thread_journal = journal.clone();
        let waiter = std::thread::spawn(move || {
            let second = JournalLock::acquire(&thread_journal).unwrap();
            thread_acquired.store(true, Ordering::Release);
            drop(second);
        });
        std::thread::sleep(Duration::from_millis(50));
        assert!(!acquired.load(Ordering::Acquire));
        drop(first);
        waiter.join().unwrap();
        assert!(acquired.load(Ordering::Acquire));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn registered_sample_positive_and_mode_or_commit_mismatch_cases_fail_closed() {
        let path = std::env::temp_dir().join(format!("erf-pilot-sample-{}.tsv", std::process::id()));
        fs::write(&path, format!("format=erf-pilot-sample-v1\nmode=LIVE_SHADOW\nrule-bundle={}\nsample=org/repo\tabc123\n", "03".repeat(32))).unwrap();
        let registered = read_registered_sample(&path, Mode::LiveShadow).unwrap();
        let good = erf_pilot_metrics::RunMeasurement {
            measurement_id: "run:1".into(),
            run_identity_digest: digest_bytes("run:1"),
            mode: ObservationMode::LiveShadow,
            repository_digest: digest_bytes("org/repo"),
            commit_digest: digest_bytes("abc123"),
            rule_bundle_digest: [3; 32],
            unified_result_digest: [4; 32],
            trust: erf_unified_result::ResultTrust::LocalUnattested,
            decision: GateDecision::Pass,
            complete: true,
            new_finding_count: 0,
            historical_finding_count: 0,
            latency_millis: 1,
            peak_memory_bytes: 1,
            observed_at_epoch_secs: 1,
        };
        assert!(validate_registered_identities(&[&good], &registered, Mode::LiveShadow, 1).is_ok());
        let wrong_commit = erf_pilot_metrics::RunMeasurement { commit_digest: digest_bytes("different"), ..good.clone() };
        assert!(validate_registered_identities(&[&wrong_commit], &registered, Mode::LiveShadow, 1).is_err());
        let wrong_bundle = erf_pilot_metrics::RunMeasurement { rule_bundle_digest: [4; 32], ..good.clone() };
        assert!(validate_registered_identities(&[&wrong_bundle], &registered, Mode::LiveShadow, 1).is_err());
        let wrong_mode = erf_pilot_metrics::RunMeasurement { mode: ObservationMode::LocalSimulation, ..good };
        assert!(validate_registered_identities(&[&wrong_mode], &registered, Mode::LiveShadow, 1).is_err());
        assert!(read_registered_sample(&path, Mode::SyntheticShadow).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn registered_sample_positive_and_mismatch_cases_cover_each_observation_mode() {
        for (index, (mode, metric_mode)) in [
            (Mode::LocalSimulation, ObservationMode::LocalSimulation),
            (Mode::SyntheticShadow, ObservationMode::SyntheticShadow),
            (Mode::LiveShadow, ObservationMode::LiveShadow),
        ].into_iter().enumerate() {
            let path = std::env::temp_dir().join(format!("erf-pilot-sample-{}-{index}.tsv", std::process::id()));
            fs::write(&path, format!("format=erf-pilot-sample-v1\nmode={}\nrule-bundle={}\nsample=org/repo-{index}\tcommit-{index}\n", mode.as_str(), "05".repeat(32))).unwrap();
            let registered = read_registered_sample(&path, mode).unwrap();
            let run = erf_pilot_metrics::RunMeasurement {
                measurement_id: format!("run:{index}"),
                run_identity_digest: digest_bytes(&format!("run:{index}")),
                mode: metric_mode,
                repository_digest: digest_bytes(&format!("org/repo-{index}")),
                commit_digest: digest_bytes(&format!("commit-{index}")),
                rule_bundle_digest: [5; 32],
                unified_result_digest: [6; 32],
                trust: erf_unified_result::ResultTrust::LocalUnattested,
                decision: GateDecision::Pass,
                complete: true,
                new_finding_count: 0,
                historical_finding_count: 0,
                latency_millis: 1,
                peak_memory_bytes: 1,
                observed_at_epoch_secs: 1,
            };
            assert!(validate_registered_identities(&[&run], &registered, mode, 1).is_ok());
            let mismatched_mode = erf_pilot_metrics::RunMeasurement { mode: match metric_mode { ObservationMode::LocalSimulation => ObservationMode::LiveShadow, _ => ObservationMode::LocalSimulation }, ..run.clone() };
            assert!(validate_registered_identities(&[&mismatched_mode], &registered, mode, 1).is_err());
            let mismatched_bundle = erf_pilot_metrics::RunMeasurement { rule_bundle_digest: [7; 32], ..run };
            assert!(validate_registered_identities(&[&mismatched_bundle], &registered, mode, 1).is_err());
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn project_config_scope_is_loaded_for_observation() {
        let directory = std::env::temp_dir().join(format!(
            "erf-pilot-observer-scope-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        let repo = directory.join("repo");
        fs::create_dir_all(repo.join(".ruleops")).unwrap();
        let config = repo.join(".ruleops/config.yaml");
        fs::write(
            &config,
            "version: ruleops.config/v1\nlanguage: java\nbuild_system: maven\nmodules: [\"app\"]\nsource_roots: [\"app/src/main/java\"]\ntest_roots: []\ngenerated_roots: []\nvendor_roots: []\ninclude: [\"app/src/main/java/**\"]\nexclude: [\".ruleops/**\"]\nexecution_mode: connected\ncache_dir: .ruleops/cache/bundles\ntrust_state_path: .ruleops/trust-state\n",
        )
        .unwrap();
        let options = ScanOptions {
            repo_root: repo.display().to_string(),
            project_config: Some(config),
            ..ScanOptions::default()
        };
        let scope = project_scope_for(&options).unwrap().unwrap();
        assert_eq!(scope.modules, vec!["app"]);
        assert_eq!(scope.roots, vec!["app/src/main/java"]);
        assert_eq!(scope.include, vec!["app/src/main/java/**"]);
        assert_eq!(scope.exclude, vec![".ruleops/**"]);
        fs::remove_dir_all(directory).unwrap();
    }
}
