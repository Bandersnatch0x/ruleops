use erf_contracts::workspace::v0 as wb;
use erf_resolver_client::{ResolverConfig, ResolverHandle};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::process::Command;

fn repo() -> PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("erf-static-protocol-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn gradle_adapter_speaks_workspace_resolution_contract() {
    let root = repo();
    write(&root.join("settings.gradle"), "include ':service'\n");
    write(&root.join("service/src/main/java/app/Service.java"), "class Service {}\n");
    let mut command = Command::new(env!("CARGO_BIN_EXE_erf-static-resolver"));
    command.args(["--kind", "gradle"]);
    let config = ResolverConfig {
        required_capabilities: vec!["ecosystem.gradle.safe-static".into()],
        limits: wb::Limits {
            max_frame_bytes: erf_framing::DEFAULT_MAX_FRAME_BYTES,
            deadline_ms: 0,
        },
    };
    let (mut resolver, ack) = ResolverHandle::spawn(&mut command, &config).expect("handshake");
    assert_eq!(ack.resolver_name, "erf-gradle-static-resolver");
    let descriptor = resolver.resolve(root.to_str().unwrap()).expect("resolve");
    assert_eq!(descriptor.mode, wb::WorkspaceResolutionMode::SafeStatic as i32);
    assert_eq!(descriptor.modules[0].module_id, "service");
    assert_eq!(descriptor.modules[0].files.len(), 1);
    assert_eq!(resolver.complete().unwrap().status, wb::completion::Status::Completed as i32);
    fs::remove_dir_all(root).unwrap();
}
