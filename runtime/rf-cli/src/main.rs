use rf_runner::{scan, ScanConfig};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("scan") {
        eprintln!("usage: rf scan --repo-root <path> --candidate <dir> --host <command> [--host-arg <arg>] [--resolver <command>]");
        std::process::exit(2);
    }
    let mut repo_root = None;
    let mut host_command = None;
    let mut host_args = Vec::new();
    let mut rule_bundle = None;
    let mut resolver_command = "target/debug/erf-fake-resolver".to_string();
    let mut resolver_args = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--repo-root" if i + 1 < args.len() => { repo_root = Some(args[i + 1].clone()); i += 2; }
            "--host" if i + 1 < args.len() => { host_command = Some(args[i + 1].clone()); i += 2; }
            "--host-arg" if i + 1 < args.len() => { host_args.push(args[i + 1].clone()); i += 2; }
            "--candidate" if i + 1 < args.len() => { rule_bundle = Some(args[i + 1].clone()); i += 2; }
            "--resolver" if i + 1 < args.len() => { resolver_command = args[i + 1].clone(); i += 2; }
            "--resolver-arg" if i + 1 < args.len() => { resolver_args.push(args[i + 1].clone()); i += 2; }
            other => { eprintln!("unexpected argument: {other}"); std::process::exit(2); }
        }
    }
    let Some(repo_root) = repo_root else { eprintln!("missing --repo-root"); std::process::exit(2) };
    let Some(host_command) = host_command else { eprintln!("missing --host"); std::process::exit(2) };
    let result = scan(&ScanConfig {
        repo_root,
        rule_id: "corp.java.no-system-exit".into(),
        rule_bundle,
        resolver_command,
        resolver_args,
        host_command,
        host_args,
        max_staging_bytes: 64 * 1024 * 1024,
    });
    match result {
        Ok(result) => {
            let gate = rf_gate::preview(&result.planned_unit_ids, &result.unit_statuses, &result.committed_findings);
            println!("planned={} executed={} failed={}", result.planned, result.executed, result.failed);
            println!("plan_digest={:x?}", result.plan_digest);
            println!("findings={} trust={} attempt_transcript_digest={:x?}", gate.findings.len(), result.trust, result.attempt_transcript_digest);
            println!("gate={:?} completeness={:?} canonical_set={} inputs={}", gate.decision, gate.completeness, gate.canonical_set_version, gate.input_summary);
        }
        Err(error) => {
            eprintln!("scan failed atomically: {error}");
            std::process::exit(1);
        }
    }
}
