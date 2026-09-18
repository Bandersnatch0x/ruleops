//! Suite runner CLI: drives a host executable through the conformance suite
//! and prints a per-check report bound to the suite version.
//! Exit: 0 all checks pass, 1 failures, 2 usage.
//!
//! Usage: erf-conformance --host <COMMAND...> [--cancel-window-ms <n>]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut host: Option<String> = None;
    let mut host_args: Vec<String> = Vec::new();
    let mut cancel_window_ms: u64 = 0;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--host" if i + 1 < args.len() => {
                host = Some(args[i + 1].clone());
                i += 2;
            }
            "--host-arg" if i + 1 < args.len() => {
                host_args.push(args[i + 1].clone());
                i += 2;
            }
            "--cancel-window-ms" if i + 1 < args.len() => {
                cancel_window_ms = args[i + 1].parse().expect("cancel window ms");
                i += 2;
            }
            other => {
                eprintln!("unexpected argument: {other}");
                std::process::exit(2);
            }
        }
    }
    let Some(host) = host else {
        eprintln!("usage: erf-conformance --host <COMMAND> [--cancel-window-ms <n>]");
        std::process::exit(2);
    };

    let config = erf_conformance::SuiteConfig {
        host_program: host,
        host_args,
        cancel_probe_delay_ms: cancel_window_ms,
        ..Default::default()
    };
    eprintln!("running suite against {} ...", config.host_program);
    let report = erf_conformance::run_acceptance(&config);

    eprintln!("suite version {}", report.suite_version);
    let mut failures = 0;
    for check in &report.checks {
        let mark = if check.passed { "PASS" } else { "FAIL" };
        if !check.passed {
            failures += 1;
        }
        eprintln!("{mark} {:45} {}", check.name, check.detail);
    }
    eprintln!(
        "{}/{} checks passed",
        report.checks.len() - failures,
        report.checks.len()
    );
    std::process::exit(if failures == 0 { 0 } else { 1 });
}
