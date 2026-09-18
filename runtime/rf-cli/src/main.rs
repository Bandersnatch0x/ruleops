use rf_runner::{scan, ExecutionLock, ScanConfig, ScanError};
use std::collections::BTreeSet;

fn scan_error_exit_code(error: &ScanError) -> i32 {
    match error {
        ScanError::Trust(_) => 4,
        ScanError::Resolve(error) => resolver_error_exit_code(error),
        ScanError::Host(error) => host_error_exit_code(error),
        ScanError::Sandbox(_) => 3,
        ScanError::Plan(_) | ScanError::RuleBundle(_) => 2,
    }
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("scan") {
        eprintln!("usage: rf scan --repo-root <path> --host <command> (--rule-id <id> | --candidate <dir>) [--host-arg <arg>] [--resolver <command>] [--deadline-ms <n>] [--execution-lock <file>] [--baseline-kind <debt|comparison|quality>] [--baseline-fingerprint <fp>] [--exception-fingerprint <fp>] [--blocking-rule-id <id>]");
        std::process::exit(2);
    }
    let mut repo_root = None;
    let mut host_command = None;
    let mut host_args = Vec::new();
    let mut rule_bundle = None;
    let mut rule_id = None;
    let mut resolver_command = "target/debug/erf-fake-resolver".to_string();
    let mut resolver_args = Vec::new();
    let mut host_deadline_ms = 0;
    let mut baseline_kind = None;
    let mut baseline_fingerprints = BTreeSet::new();
    let mut exception_fingerprints = BTreeSet::new();
    let mut blocking_rule_ids = BTreeSet::new();
    let mut execution_lock_path = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--repo-root" if i + 1 < args.len() => { repo_root = Some(args[i + 1].clone()); i += 2; }
            "--host" if i + 1 < args.len() => { host_command = Some(args[i + 1].clone()); i += 2; }
            "--host-arg" if i + 1 < args.len() => { host_args.push(args[i + 1].clone()); i += 2; }
            "--candidate" if i + 1 < args.len() => { rule_bundle = Some(args[i + 1].clone()); i += 2; }
            "--rule-id" if i + 1 < args.len() => { rule_id = Some(args[i + 1].clone()); i += 2; }
            "--resolver" if i + 1 < args.len() => { resolver_command = args[i + 1].clone(); i += 2; }
            "--resolver-arg" if i + 1 < args.len() => { resolver_args.push(args[i + 1].clone()); i += 2; }
            "--deadline-ms" if i + 1 < args.len() => { host_deadline_ms = args[i + 1].parse().unwrap_or_else(|_| { eprintln!("invalid --deadline-ms"); std::process::exit(2); }); i += 2; }
            "--baseline-kind" if i + 1 < args.len() => { baseline_kind = Some(args[i + 1].clone()); i += 2; }
            "--baseline-fingerprint" if i + 1 < args.len() => { baseline_fingerprints.insert(args[i + 1].clone()); i += 2; }
            "--exception-fingerprint" if i + 1 < args.len() => { exception_fingerprints.insert(args[i + 1].clone()); i += 2; }
            "--blocking-rule-id" if i + 1 < args.len() => { blocking_rule_ids.insert(args[i + 1].clone()); i += 2; }
            "--execution-lock" if i + 1 < args.len() => { execution_lock_path = Some(args[i + 1].clone()); i += 2; }
            other => { eprintln!("unexpected argument: {other}"); std::process::exit(2); }
        }
    }
    let Some(repo_root) = repo_root else { eprintln!("missing --repo-root"); std::process::exit(2) };
    let Some(host_command) = host_command else { eprintln!("missing --host"); std::process::exit(2) };
    let rule_id = match (rule_bundle.is_some(), rule_id) {
        (true, Some(_)) => {
            eprintln!("--rule-id cannot be combined with --candidate; Candidate manifest is authoritative");
            std::process::exit(2);
        }
        (true, None) => String::new(),
        (false, Some(rule_id)) if !rule_id.trim().is_empty() => rule_id,
        (false, Some(_)) => {
            eprintln!("--rule-id cannot be empty");
            std::process::exit(2);
        }
        (false, None) => {
            eprintln!("missing --rule-id when --candidate is absent");
            std::process::exit(2);
        }
    };
    let (execution_lock, signed_execution_lock) = execution_lock_path.map(|path| {
        let path_ref = std::path::Path::new(&path);
        let text = std::fs::read_to_string(path_ref).unwrap_or_default();
        if text.lines().any(|line| line.trim_start().starts_with("signature_hex=")) {
            match rf_runner::SignedExecutionLock::from_file(path_ref) {
                Ok(lock) => (None, Some(lock)),
                Err(error) => { eprintln!("invalid signed execution lock: {error}"); std::process::exit(2); }
            }
        } else {
            match ExecutionLock::from_file(path_ref) {
                Ok(lock) => (Some(lock), None),
                Err(error) => { eprintln!("invalid execution lock: {error}"); std::process::exit(2); }
            }
        }
    }).unwrap_or((None, None));
    let result = scan(&ScanConfig {
        repo_root,
        // With a Candidate, rf-runner derives identity from the validated
        // manifest instead of trusting this CLI value.
        rule_id,
        rule_bundle,
        resolver_command,
        resolver_args,
        host_command,
        host_args,
        host_deadline_ms,
        max_staging_bytes: 64 * 1024 * 1024,
        execution_lock,
        signed_execution_lock,
        sandbox_policy: None,
    });
    match result {
        Ok(result) => {
            let baseline = baseline_kind.map(|kind| {
                let kind = match kind.as_str() {
                    "debt" => rf_gate::BaselineKind::Debt,
                    "comparison" => rf_gate::BaselineKind::Comparison,
                    "quality" => rf_gate::BaselineKind::QualityBenchmark,
                    other => { eprintln!("invalid --baseline-kind: {other}"); std::process::exit(2); }
                };
                rf_gate::Baseline { kind, finding_fingerprints: baseline_fingerprints }
            });
            let policy = (!blocking_rule_ids.is_empty()).then_some(rf_gate::GatePolicy { blocking_rule_ids });
            let gate = rf_gate::preview_with_policy_and_baseline_and_exceptions(
                &result.planned_unit_ids,
                &result.unit_statuses,
                &result.committed_findings,
                baseline.as_ref(),
                &exception_fingerprints,
                policy.as_ref());
            println!("rule_id={}", result.rule_id);
            println!("planned={} executed={} failed={} attempt_failure={:?}", result.planned, result.executed, result.failed, result.attempt_failure);
            println!("plan_digest={:x?}", result.plan_digest);
            println!("findings={} trust={} attempt_transcript_digest={:x?}", gate.findings.len(), result.trust, result.attempt_transcript_digest);
            println!("gate={:?} completeness={:?} canonical_set={} semantic_result_digest={:x?} inputs={}", gate.decision, gate.completeness, gate.canonical_set_version, gate.semantic_result_digest, gate.input_summary);
            if gate.decision == rf_gate::GateDecision::Inconclusive || !gate.completeness.complete {
                std::process::exit(3);
            }
        }
        Err(error) => {
            eprintln!("scan failed atomically: {error}");
            std::process::exit(scan_error_exit_code(&error));
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
        assert_eq!(host_error_exit_code(&erf_host_client::HostError::MajorMismatch { requested: 0, acked: 1 }), 2);
        assert_eq!(host_error_exit_code(&erf_host_client::HostError::Hangup { status: "crashed".into() }), 3);
        assert_eq!(resolver_error_exit_code(&erf_resolver_client::ResolverError::Violation("bad frame".into())), 2);
    }
}
