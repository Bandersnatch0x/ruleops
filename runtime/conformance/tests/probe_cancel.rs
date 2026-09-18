use erf_conformance::SuiteConfig;

#[test]
#[ignore = "manual Java host probe; requires a separately built shaded jar"]
fn cancel_check_against_java_host() {
    let config = SuiteConfig {
        host_program: "java".to_string(),
        host_args: vec![
            "-jar".to_string(),
            "/home/kali/文档/ruleops/backend-hosts/java/target/erf-java-host-0.1.0-shaded.jar".to_string(),
        ],
        cancel_probe_delay_ms: 200,
        ..Default::default()
    };
    let report = erf_conformance::run_suite(&config);
    for check in &report.checks {
        if check.name.contains("cancel") {
            println!("{}: {} ({})", check.name, check.passed, check.detail);
        }
    }
}
