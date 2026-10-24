//! Contract tests: the Rust resolver client drives the reference fake
//! resolver through handshake, resolution of a sample layout, and the
//! rejection paths (major mismatch, missing capabilities, unreadable root,
//! uncorrelatable responses).

use erf_contracts::workspace::v0 as wb;
use prost::Message as _;
use erf_resolver_client::{ResolverConfig, ResolverError, ResolverHandle};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::symlink;

fn fake_resolver_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_erf-fake-resolver"))
}

fn config_with(required: &[&str]) -> ResolverConfig {
    ResolverConfig {
        required_capabilities: required.iter().map(|s| s.to_string()).collect(),
        limits: wb::Limits {
            max_frame_bytes: erf_framing::DEFAULT_MAX_FRAME_BYTES,
            deadline_ms: 0,
        },
    }
}

/// Sample layout: default module (src/main/java) + one nested module.
fn prepare_layout(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("src/main/java/app")).expect("mkdir default");
    std::fs::create_dir_all(repo.join("core/src/main/java/core")).expect("mkdir core");
    std::fs::write(repo.join("src/main/java/app/A.java"), "public class A {}\n").expect("A");
    std::fs::write(repo.join("core/src/main/java/core/B.java"), "public class B {}\n").expect("B");
    repo
}

#[test]
fn resolve_sample_layout_into_content_addressed_descriptor() {
    let dir = std::env::temp_dir().join(format!("erf-wr-{}", std::process::id()));
    let repo = prepare_layout(&dir);

    let (mut resolver, ack) =
        ResolverHandle::spawn(&mut fake_resolver_command(), &config_with(&["ecosystem.static-layout"]))
            .expect("handshake");
    assert_eq!(ack.resolver_name, "erf-fake-resolver");

    let descriptor = resolver
        .resolve(repo.to_str().expect("utf8"))
        .expect("resolve");
    assert_eq!(
        descriptor.mode,
        wb::WorkspaceResolutionMode::SafeStatic as i32
    );
    assert_eq!(descriptor.modules.len(), 2, "default + core modules");
    let a_entry = descriptor
        .modules
        .iter()
        .flat_map(|m| m.files.iter())
        .find(|f| f.path.ends_with("app/A.java"))
        .expect("A.java in descriptor");
    let bytes = std::fs::read(repo.join("src/main/java/app/A.java")).expect("read");
    assert_eq!(a_entry.blob_sha256, Sha256::digest(&bytes).to_vec());

    // Digest re-verification: sha256 over the descriptor minus the digest field.
    let mut blank = descriptor.clone();
    blank.descriptor_sha256 = vec![];
    assert_eq!(
        descriptor.descriptor_sha256,
        Sha256::digest(blank.encode_to_vec()).to_vec()
    );
    // Static resolution never invents build outputs.
    assert!(descriptor
        .unresolved_facts
        .iter()
        .any(|f| f.contains("build-output")));

    let completion = resolver.complete().expect("completion");
    assert_eq!(completion.status, wb::completion::Status::Completed as i32);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unreadable_repo_root_is_a_structured_error() {
    let (mut resolver, _) =
        ResolverHandle::spawn(&mut fake_resolver_command(), &config_with(&[])).expect("handshake");
    let err = resolver
        .resolve("/nonexistent/erf-probe-root")
        .expect_err("must fail structured");
    match err {
        ResolverError::Protocol {
            code: wb::protocol_error::Code::RepoRootUnreadable,
            ..
        } => {}
        other => panic!("expected RepoRootUnreadable, got: {other:?}"),
    }
}

#[test]
fn major_mismatch_is_refused() {
    let result = ResolverHandle::spawn(
        &mut {
            let mut c = fake_resolver_command();
            c.env("FAKE_SUPPORTED_MAJOR", "1");
            c
        },
        &config_with(&[]),
    );
    match result {
        Err(ResolverError::Protocol {
            code: wb::protocol_error::Code::ProtocolMajorMismatch,
            ..
        }) => {}
        Err(other) => panic!("expected major mismatch, got: {other:?}"),
        Ok(_) => panic!("handshake unexpectedly succeeded"),
    }
}

#[test]
fn uncorrelatable_ack_is_rejected() {
    let result = ResolverHandle::spawn(
        &mut {
            let mut c = fake_resolver_command();
            c.env("FAKE_BAD_CORRELATION", "1");
            c
        },
        &config_with(&[]),
    );
    match result {
        Err(ResolverError::Uncorrelatable { correlation_id }) => assert_eq!(correlation_id, 999),
        Err(other) => panic!("expected uncorrelatable rejection, got: {other:?}"),
        Ok(_) => panic!("handshake unexpectedly succeeded"),
    }
}

#[test]
fn cancel_is_acked_symmetrically() {
    let (mut resolver, _) =
        ResolverHandle::spawn(&mut fake_resolver_command(), &config_with(&[])).expect("handshake");
    let ack = resolver.cancel().expect("cancel ack");
    assert!(ack.accepted);
    let completion = resolver.complete().expect("completion");
    assert_eq!(completion.status, wb::completion::Status::Completed as i32);
}

#[test]
fn missing_capability_fails_explicitly() {
    let result = ResolverHandle::spawn(
        &mut fake_resolver_command(),
        &config_with(&["ecosystem.gradle"]),
    );
    match result {
        Err(ResolverError::Protocol {
            code: wb::protocol_error::Code::UnsupportedCapability,
            ..
        }) => {}
        Err(ResolverError::MissingCapabilities { missing }) => {
            assert_eq!(missing, vec!["ecosystem.gradle".to_string()])
        }
        Err(other) => panic!("expected explicit capability failure, got: {other:?}"),
        Ok(_) => panic!("handshake unexpectedly succeeded"),
    }
}

#[test]
fn false_capability_fails_explicitly() {
    let result = ResolverHandle::spawn(
        &mut {
            let mut c = fake_resolver_command();
            c.env("FAKE_FALSE_CAPABILITY", "ecosystem.static-layout");
            c
        },
        &config_with(&["ecosystem.static-layout"]),
    );
    match result {
        Err(ResolverError::Protocol {
            code: wb::protocol_error::Code::UnsupportedCapability,
            ..
        }) => {}
        Err(ResolverError::MissingCapabilities { missing }) => {
            assert_eq!(missing, vec!["ecosystem.static-layout".to_string()])
        }
        Err(other) => panic!("expected explicit false-capability failure, got: {other:?}"),
        Ok(_) => panic!("handshake unexpectedly succeeded with false capability"),
    }
}

#[test]
fn maven_and_gradle_build_metadata_is_detected_without_execution() {
    let dir = std::env::temp_dir().join(format!("erf-build-adapter-{}", std::process::id()));
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("app/src/main/java/app")).expect("mkdir");
    std::fs::write(repo.join("pom.xml"), "<project/>").expect("pom");
    std::fs::write(repo.join("settings.gradle"), "rootProject.name='sample'").expect("gradle");
    std::fs::write(repo.join("app/src/main/java/app/App.java"), "class App {}\n").expect("java");
    let descriptor = erf_fake_resolver::resolve(&repo).expect("resolve");
    assert!(descriptor.unresolved_facts.iter().any(|fact| fact == "build-system:maven"));
    assert!(descriptor.unresolved_facts.iter().any(|fact| fact == "build-system:gradle"));
    assert!(descriptor.unresolved_facts.iter().any(|fact| fact.contains("dependency-classpath")));
    assert_eq!(descriptor.modules.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(unix)]
#[test]
fn resolver_rejects_symbolic_links_in_the_workspace_snapshot() {
    let dir = std::env::temp_dir().join(format!("erf-wr-symlink-{}", std::process::id()));
    let repo = dir.join("repo");
    let outside = dir.join("outside.java");
    std::fs::create_dir_all(repo.join("src/main/java/app")).expect("mkdir");
    std::fs::write(&outside, "class Outside {}\n").expect("outside");
    symlink(&outside, repo.join("src/main/java/app/Outside.java")).expect("symlink");

    let error = erf_fake_resolver::resolve(&repo).expect_err("workspace symlink must fail closed");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    let _ = std::fs::remove_dir_all(dir);
}
