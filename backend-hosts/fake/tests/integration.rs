//! End-to-end contract tests: the Rust host client drives the reference fake
//! host through negotiation, describe/plan/execute, cancellation and the
//! rejection paths demanded by the contract (malformed/oversized frames,
//! stdout pollution, major mismatch, missing capabilities).

use erf_contracts::backend::v0 as pb;
use erf_host_client::framing::DEFAULT_MAX_FRAME_BYTES;
use erf_host_client::{ExecEvent, HostConfig, HostError, HostHandle};
use std::process::Command;

fn fake_host_command() -> Command {
    let exe = env!("CARGO_BIN_EXE_erf-fake-host");
    Command::new(exe)
}

fn config_with(required: &[&str]) -> HostConfig {
    HostConfig {
        required_capabilities: required.iter().map(|s| s.to_string()).collect(),
        optional_capabilities: vec![],
        limits: pb::Limits {
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            max_events_per_execute: 1_000,
            deadline_ms: 0,
        },
    }
}

fn canned_observation() -> pb::BackendObservation {
    pb::BackendObservation {
        rule_id: "corp.java.no-system-exit".into(),
        location: Some(pb::ObservationLocation {
            kind: Some(pb::observation_location::Kind::SourceSpan(pb::SourceSpan {
                path: "src/main/java/A.java".into(),
                blob_sha256: vec![0u8; 32],
                start_byte: 0,
                end_byte: 17,
                line: 1,
                column: 5,
            })),
        }),
        message: "canned".into(),
        engine_severity: "error".into(),
    }
}

#[test]
fn negotiate_describe_plan_execute_round_trip() {
    let (mut host, handshake) = HostHandle::spawn(
        &mut fake_host_command(),
        &config_with(&["java.ast", "java.symbol-resolution"]),
    )
    .expect("handshake");
    assert_eq!(handshake.ack.host_name, "erf-fake-host");

    let caps = host.describe().expect("describe");
    assert_eq!(caps.backend_id, "fake-backend");
    assert!(caps.facts.contains_key("java.ast"));

    let plan = host
        .plan(vec![
            pb::AnalysisUnit {
                unit_id: "unit-1".into(),
                language: "java".into(),
            },
            pb::AnalysisUnit {
                unit_id: "unit-2".into(),
                language: "java".into(),
            },
        ])
        .expect("plan");
    assert_eq!(plan.planned.len(), 2);
    assert!(plan.unsupported.is_empty());

    let mut stream = host
        .execute(vec!["unit-1".into(), "unit-2".into()])
        .expect("execute");
    let mut observations = 0;
    let mut statuses = 0;
    while let Some(event) = stream.next_event().expect("event") {
        match event {
            ExecEvent::Observation(o) => {
                observations += 1;
                assert_eq!(o.rule_id, "corp.java.no-system-exit");
            }
            ExecEvent::UnitStatus(s) => {
                statuses += 1;
                assert_eq!(s.outcome, pb::UnitOutcome::Executed as i32);
            }
            ExecEvent::Complete(c) => {
                assert_eq!(c.units_reported, 2);
            }
        }
    }
    assert_eq!(observations, 2);
    assert_eq!(statuses, 2);

    let completion = host.complete().expect("completion");
    assert_eq!(completion.status, pb::completion::Status::Completed as i32);
}

#[test]
fn missing_required_capability_fails_explicitly() {
    let result = HostHandle::spawn(
        &mut {
            let mut c = fake_host_command();
            c.env("FAKE_SUPPORTED_MAJOR", "0");
            c
        },
        &config_with(&["java.bytecode"]),
    );
    match result {
        Err(HostError::Protocol {
            code: pb::protocol_error::Code::UnsupportedCapability,
            ..
        }) => {}
        Err(HostError::MissingCapabilities { missing }) => {
            assert_eq!(missing, vec!["java.bytecode".to_string()])
        }
        Err(other) => panic!("expected explicit capability failure, got: {other:?}"),
        Ok(_) => panic!("handshake unexpectedly succeeded"),
    }
}

#[test]
fn protocol_major_mismatch_refuses_execution() {
    let result = HostHandle::spawn(
        &mut {
            let mut c = fake_host_command();
            // Client speaks major 0; host pretends to be a major-1 host.
            c.env("FAKE_SUPPORTED_MAJOR", "1");
            c
        },
        &config_with(&[]),
    );
    match result {
        Err(HostError::Protocol {
            code: pb::protocol_error::Code::ProtocolMajorMismatch,
            ..
        }) => {}
        Err(other) => panic!("expected major mismatch refusal, got: {other:?}"),
        Ok(_) => panic!("handshake unexpectedly succeeded"),
    }
}

#[test]
fn stdout_pollution_is_a_structured_failure() {
    let result = HostHandle::spawn(
        &mut {
            let mut c = fake_host_command();
            c.env("FAKE_POLLUTE_STDOUT", "1");
            c
        },
        &config_with(&[]),
    );
    match result {
        // Pollution bytes "POLLUTION\n" become a giant bogus frame length.
        Err(HostError::Transport(erf_host_client::framing::FrameError::Oversized { .. })) => {}
        Err(HostError::Transport(erf_host_client::framing::FrameError::Io(_))) => {}
        Err(other) => panic!("expected structured transport failure, got: {other:?}"),
        Ok(_) => panic!("handshake unexpectedly succeeded"),
    }
}

#[test]
fn malformed_frame_to_host_yields_protocol_error() {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = fake_host_command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    let mut stdin = child.stdin.take().expect("stdin");
    // A well-formed frame containing garbage protobuf.
    let garbage = vec![0xffu8; 8];
    stdin.write_all(&(garbage.len() as u32).to_be_bytes()).unwrap();
    stdin.write_all(&garbage).unwrap();
    stdin.flush().unwrap();
    drop(stdin);
    let status = child.wait().expect("wait");
    assert!(!status.success(), "host must fail on malformed frames");
}

#[test]
fn oversized_frame_is_refused() {
    // Reader limit far below the frame the writer sends.
    let (a, b) = std::os::unix::net::UnixStream::pair().expect("pair");
    let mut reader =
        erf_host_client::framing::FrameReader::new(a.try_clone().expect("clone"), 8);
    let mut writer = erf_host_client::framing::FrameWriter::new(b, 64);
    writer.write_frame(&[0u8; 16]).expect("write fits writer cap");
    match reader.read_frame() {
        Err(erf_host_client::framing::FrameError::Oversized { size, max }) => {
            assert_eq!(size, 16);
            assert_eq!(max, 8);
        }
        other => panic!("expected Oversized, got: {other:?}"),
    }
}

#[test]
fn cooperative_cancel_terminates_with_accounting() {
    let (mut host, _handshake) = HostHandle::spawn(
        &mut {
            let mut c = fake_host_command();
            // Each event delayed so the cancel lands between events.
            c.env("FAKE_DELAY_MS", "150");
            c
        },
        &config_with(&[]),
    )
    .expect("handshake");
    let plan = host
        .plan(vec![
            pb::AnalysisUnit {
                unit_id: "unit-1".into(),
                language: "java".into(),
            },
            pb::AnalysisUnit {
                unit_id: "unit-2".into(),
                language: "java".into(),
            },
            pb::AnalysisUnit {
                unit_id: "unit-3".into(),
                language: "java".into(),
            },
        ])
        .expect("plan");
    assert_eq!(plan.planned.len(), 3);

    let mut stream = host
        .execute(vec!["unit-1".into(), "unit-2".into(), "unit-3".into()])
        .expect("execute");
    // Read the first unit's observation, then cancel while the host is in
    // its (150 ms) cooperative window before unit-2's first event.
    let first = stream.next_event().expect("first event");
    assert!(matches!(first, Some(ExecEvent::Observation(_))));
    stream.cancel().expect("cancel request");
    let completion = stream.completion().expect("completion after cancel");
    assert_eq!(completion.status, pb::completion::Status::Cancelled as i32);

    // After a cancelled session the process is gone; a new session works.
    let (mut host2, _) = HostHandle::spawn(&mut fake_host_command(), &config_with(&[]))
        .expect("second handshake");
    let _ = canned_observation();
    host2.describe().expect("describe after fresh session");
    let completion = host2.complete().expect("clean end");
    assert_eq!(completion.status, pb::completion::Status::Completed as i32);
}
