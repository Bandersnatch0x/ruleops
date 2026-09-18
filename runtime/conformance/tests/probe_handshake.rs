use erf_contracts::backend::v0 as pb;
use erf_host_client::{HostConfig, HostHandle};

#[test]
#[ignore = "manual Java host probe; requires a separately built shaded jar"]
fn handshake_missing_capability() {
    let jar = "/home/kali/文档/ruleops/backend-hosts/java/target/erf-java-host-0.1.0-shaded.jar";
    let mut command = std::process::Command::new("java");
    command.arg("-jar").arg(jar);
    let cfg = HostConfig {
        required_capabilities: vec!["java.erf-nonexistent-capability".to_string()],
        limits: pb::Limits {
            max_frame_bytes: erf_framing::DEFAULT_MAX_FRAME_BYTES,
            max_events_per_execute: 10_000,
            deadline_ms: 0,
        },
        ..Default::default()
    };
    match HostHandle::spawn(&mut command, &cfg) {
        Ok(_) => println!("handshake unexpectedly succeeded"),
        Err(e) => println!("handshake failed: {e}"),
    }
}
