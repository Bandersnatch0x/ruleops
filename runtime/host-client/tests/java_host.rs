//! End-to-end tests: the Rust host client drives the REAL Java Backend Host
//! (PMD 7.x) through the Backend Host Contract. The jar is built by
//! `scripts/java-host-e2e.sh`; tests skip when ERF_JAVA_HOST_JAR is unset.

use erf_contracts::backend::v0 as pb;
use erf_host_client::{ExecEvent, HostConfig, HostError, HostHandle};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

fn java_host_command() -> Option<Command> {
    let jar = std::env::var("ERF_JAVA_HOST_JAR").ok()?;
    let mut cmd = Command::new("java");
    cmd.arg("-jar").arg(jar);
    Some(cmd)
}

fn config_with(required: &[&str]) -> HostConfig {
    HostConfig {
        required_capabilities: required.iter().map(|s| s.to_string()).collect(),
        optional_capabilities: vec![],
        limits: pb::Limits {
            max_frame_bytes: erf_framing::DEFAULT_MAX_FRAME_BYTES,
            max_events_per_execute: 1_000,
            deadline_ms: 0,
        },
    }
}

const RULESET: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ruleset name="erf-e2e"
         xmlns="http://pmd.sourceforge.net/ruleset/2.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://pmd.sourceforge.net/ruleset/2.0.0 https://pmd.sourceforge.io/ruleset_2_0_0.xsd">
  <description>e2e</description>
  <rule name="ErfCorpJavaNoSystemExit"
        language="java"
        message="forbidden call to java.lang.System.exit"
        class="net.sourceforge.pmd.lang.rule.xpath.XPathRule">
    <description>candidate compiled from SymbolRule IR</description>
    <priority>4</priority>
    <properties>
      <property name="xpath">
        <value><![CDATA[//MethodCall[pmd-java:matchesSig('java.lang.System#exit(int)')]]]></value>
      </property>
    </properties>
  </rule>
</ruleset>
"#;

/// Prepares a two-file sample repo and a ruleset; returns (repo_root, ruleset_path).
fn prepare_sample_repo(dir: &Path) -> (PathBuf, PathBuf) {
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("src/main/java")).expect("mkdir");
    std::fs::write(
        repo.join("src/main/java/A.java"),
        "public class A {\n    void run() {\n        System.exit(1);\n    }\n}\n",
    )
    .expect("write A");
    std::fs::write(
        repo.join("src/main/java/B.java"),
        "public class B {\n    int add(int a, int b) {\n        return a + b;\n    }\n}\n",
    )
    .expect("write B");
    let ruleset = dir.join("ruleset.xml");
    std::fs::write(&ruleset, RULESET).expect("write ruleset");
    (repo, ruleset)
}

#[test]
fn java_host_negotiates_describes_and_executes_pmd() {
    let Some(mut command) = java_host_command() else {
        eprintln!("skipping: ERF_JAVA_HOST_JAR not set");
        return;
    };
    let dir = std::env::temp_dir().join(format!("erf-jh-{}", std::process::id()));
    let (repo, ruleset) = prepare_sample_repo(&dir);
    command.env("ERF_REPO_ROOT", &repo).env("ERF_RULESET", &ruleset);

    let (mut host, ack) =
        HostHandle::spawn(&mut command, &config_with(&["java.ast", "java.symbol-resolution"]))
            .expect("handshake with java host");
    assert_eq!(ack.host_name, "erf-java-host");
    assert_eq!(ack.capabilities.as_ref().expect("caps").backend_id, "pmd-java");

    let caps = host.describe().expect("describe");
    assert_eq!(caps.languages, vec!["java".to_string()]);

    let plan = host
        .plan(vec![pb::AnalysisUnit {
            unit_id: "corp.java.no-system-exit::A.java".into(),
            language: "java".into(),
            files: vec!["src/main/java/A.java".into(), "src/main/java/B.java".into()],
        }])
        .expect("plan");
    assert_eq!(plan.planned.len(), 1);
    assert_eq!(plan.planned[0].batch_id, "batch-0");

    let mut stream = host
        .execute(vec!["corp.java.no-system-exit::A.java".into()])
        .expect("execute");
    let mut hits_on_a = 0;
    loop {
        match stream.next_event().expect("event") {
            Some(ExecEvent::Observation(o)) => {
                let span = match o.location.expect("location").kind {
                    Some(pb::observation_location::Kind::SourceSpan(span)) => span,
                    other => panic!("expected SOURCE_SPAN, got {other:?}"),
                };
                assert_eq!(span.path, "src/main/java/A.java");
                let file_bytes = std::fs::read(repo.join(&span.path)).expect("read file");
                let expected_digest: Vec<u8> = Sha256::digest(&file_bytes).to_vec();
                assert_eq!(span.blob_sha256, expected_digest, "blob digest binds content");
                // The violation sits on System.exit's line; the byte range
                // brackets that line.
                assert!(span.end_byte > span.start_byte);
                hits_on_a += 1;
            }
            Some(ExecEvent::UnitStatus(s)) => {
                assert_eq!(s.unit_id, "corp.java.no-system-exit::A.java");
                assert_eq!(s.outcome, pb::UnitOutcome::Executed as i32);
            }
            Some(ExecEvent::Complete(c)) => {
                assert_eq!(c.units_reported, 1);
                break;
            }
            None => panic!("stream ended before ExecuteComplete"),
        }
    }
    assert_eq!(hits_on_a, 1, "exactly one System.exit violation on A.java");

    let completion = host.complete().expect("completion");
    assert_eq!(completion.status, pb::completion::Status::Completed as i32);
    assert_eq!(completion.units_reported, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn java_host_negotiates_and_executes_spotbugs_bytecode() {
    let Some(mut command) = java_host_command() else {
        eprintln!("skipping: ERF_JAVA_HOST_JAR not set");
        return;
    };
    let dir = std::env::temp_dir().join(format!("erf-sb-jh-{}", std::process::id()));
    let repo = dir.join("repo");
    let source = repo.join("src/main/java/Bad.java");
    let classes = repo.join("target/classes");
    std::fs::create_dir_all(source.parent().unwrap()).expect("mkdir source");
    std::fs::create_dir_all(&classes).expect("mkdir classes");
    std::fs::write(&source, "public class Bad { void run() { System.exit(1); } }\n")
        .expect("write source");
    let status = Command::new("javac")
        .args(["-g", "-d"])
        .arg(&classes)
        .arg(&source)
        .status()
        .expect("javac available");
    assert!(status.success(), "javac failed: {status}");
    command.env("ERF_REPO_ROOT", &repo).env("ERF_BACKEND", "spotbugs");

    let (mut host, ack) = HostHandle::spawn(&mut command, &config_with(&["java.bytecode"]))
        .expect("handshake with SpotBugs host");
    let capabilities = ack.capabilities.expect("capabilities");
    assert_eq!(capabilities.backend_id, "spotbugs-java");
    assert_eq!(capabilities.rule_kinds, vec!["BytecodeRule".to_string()]);
    assert_eq!(capabilities.facts.get("java.bytecode"), Some(&true));

    host.plan(vec![pb::AnalysisUnit {
        unit_id: "spotbugs.dm-exit::Bad.class".into(),
        language: "java".into(),
        files: vec!["target/classes/Bad.class".into()],
    }])
    .expect("plan");
    let mut stream = host
        .execute(vec!["spotbugs.dm-exit::Bad.class".into()])
        .expect("execute");
    let mut observations = 0;
    loop {
        match stream.next_event().expect("event") {
            Some(ExecEvent::Observation(observation)) => {
                assert_eq!(observation.rule_id, "spotbugs.dm-exit");
                match observation.location.expect("location").kind {
                    Some(pb::observation_location::Kind::BytecodeMember(member)) => {
                        assert_eq!(member.class_fqn, "Bad");
                    }
                    other => panic!("expected BYTECODE_MEMBER, got {other:?}"),
                }
                observations += 1;
            }
            Some(ExecEvent::UnitStatus(status)) => {
                assert_eq!(status.outcome, pb::UnitOutcome::Executed as i32);
            }
            Some(ExecEvent::Complete(complete)) => {
                assert_eq!(complete.units_reported, 1);
                break;
            }
            None => panic!("stream ended before ExecuteComplete"),
        }
    }
    assert_eq!(observations, 1);
    assert_eq!(host.complete().expect("completion").status, pb::completion::Status::Completed as i32);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn java_host_refuses_major_mismatch() {
    let Some(mut command) = java_host_command() else {
        eprintln!("skipping: ERF_JAVA_HOST_JAR not set");
        return;
    };
    command.env("ERF_SUPPORTED_MAJOR", "1");
    let result = HostHandle::spawn(&mut command, &config_with(&[]));
    match result {
        Err(HostError::Protocol {
            code: pb::protocol_error::Code::ProtocolMajorMismatch,
            ..
        }) => {}
        Err(other) => panic!("expected major mismatch, got: {other:?}"),
        Ok(_) => panic!("handshake unexpectedly succeeded"),
    }
}

#[test]
fn java_host_cooperative_cancel_accounts_units() {
    let Some(mut command) = java_host_command() else {
        eprintln!("skipping: ERF_JAVA_HOST_JAR not set");
        return;
    };
    let dir = std::env::temp_dir().join(format!("erf-jh-cancel-{}", std::process::id()));
    let (repo, ruleset) = prepare_sample_repo(&dir);
    command
        .env("ERF_REPO_ROOT", &repo)
        .env("ERF_RULESET", &ruleset)
        .env("ERF_DELAY_MS", "200");

    let (mut host, _) = HostHandle::spawn(&mut command, &config_with(&[])).expect("handshake");
    host.plan(vec![
        pb::AnalysisUnit {
            unit_id: "u1".into(),
            language: "java".into(),
            files: vec!["src/main/java/A.java".into()],
        },
        pb::AnalysisUnit {
            unit_id: "u2".into(),
            language: "java".into(),
            files: vec!["src/main/java/B.java".into()],
        },
    ])
    .expect("plan");

    let mut stream = host.execute(vec!["u1".into(), "u2".into()]).expect("execute");
    // Cancel while u1's analysis (delayed 200 ms) is in flight.
    stream.cancel().expect("cancel");
    let completion = stream.completion().expect("completion after cancel");
    assert_eq!(completion.status, pb::completion::Status::Cancelled as i32);
    // Completeness: both units accounted (u1 cancelled mid-flight or executed,
    // u2 cancelled before completion).
    assert!(completion.units_reported >= 2, "units: {}", completion.units_reported);
    let _ = std::fs::remove_dir_all(&dir);
}
