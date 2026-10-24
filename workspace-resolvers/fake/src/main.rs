//! Reference fake Workspace Resolver over the ERF Workspace Resolution
//! Contract v0. Resolution logic lives in the lib (shared with tests).
//! Knobs (env):
//! - `FAKE_SUPPORTED_MAJOR=<n>`  — protocol major accepted (default 0)
//! - `FAKE_BAD_CORRELATION=1`    — answers Hello with a mismatched correlation
//! - `FAKE_PROTOCOL_MINOR=<n>`   — answers Hello with a compatible minor
//! - `FAKE_RESOLVE_DELAY_MS=<n>` — delays resolution before answering
//! - `FAKE_CRASH_ON_RESOLVE=1`   — exits before returning a resolution
//! - `FAKE_MALFORMED_ON_RESOLVE=1` — emits a malformed protobuf frame
//! - `FAKE_OVERSIZED_ON_RESOLVE=1` — emits a frame over a small client limit
//! - `FAKE_UNKNOWN_OPTIONAL_ON_HELLO=1` — appends an additive unknown field

use erf_contracts::workspace::v0 as wb;
use erf_framing::{FrameReader, FrameWriter};
use prost::Message;
use std::process::exit;
use std::sync::mpsc::{RecvError, TryRecvError, sync_channel};

fn main() {
    let supported_major: u32 = std::env::var("FAKE_SUPPORTED_MAJOR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let bad_correlation = std::env::var("FAKE_BAD_CORRELATION").is_ok();
    let protocol_minor: u32 = std::env::var("FAKE_PROTOCOL_MINOR")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let resolve_delay_ms = std::env::var("FAKE_RESOLVE_DELAY_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let crash_on_resolve = std::env::var("FAKE_CRASH_ON_RESOLVE").is_ok();
    let malformed_on_resolve = std::env::var("FAKE_MALFORMED_ON_RESOLVE").is_ok();
    let oversized_on_resolve = std::env::var("FAKE_OVERSIZED_ON_RESOLVE").is_ok();
    let unknown_optional_on_hello = std::env::var("FAKE_UNKNOWN_OPTIONAL_ON_HELLO").is_ok();

    // Keep stdin intake bounded so a slow resolver cannot accumulate an
    // unbounded in-memory request queue.
    let (tx, rx) = sync_channel::<Option<Vec<u8>>>(8);
    let _reader = std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut reader =
            FrameReader::new(stdin.lock(), erf_framing::DEFAULT_MAX_FRAME_BYTES);
        loop {
            match reader.read_frame() {
                Ok(bytes) => {
                    if tx.send(Some(bytes)).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ = tx.send(None);
                    break;
                }
            }
        }
    });

    let stdout = std::io::stdout();
    let mut writer =
        FrameWriter::new(stdout.lock(), erf_framing::DEFAULT_MAX_FRAME_BYTES);

    let mut acknowledged = false;
    loop {
        let frame = match rx.recv() {
            Ok(Some(bytes)) => bytes,
            Ok(None) | Err(RecvError) => {
                let _ = writer.write_frame(
                    &wb::WrEnvelope {
                        correlation_id: 0,
                        payload: Some(wb::wr_envelope::Payload::Completion(
                            wb::Completion {
                                status: wb::completion::Status::Completed as i32,
                                reason: "session end".into(),
                            },
                        )),
                    }
                    .encode_to_vec());
                return;
            }
        };
        let envelope = match wb::WrEnvelope::decode(frame.as_slice()) {
            Ok(e) => e,
            Err(e) => {
                let _ = writer.write_frame(
                    &wb::WrEnvelope {
                        correlation_id: 0,
                        payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError {
                            code: wb::protocol_error::Code::MalformedFrame as i32,
                            detail: e.to_string(),
                        })),
                    }
                    .encode_to_vec());
                exit(1);
            }
        };
        let correlation = envelope.correlation_id;
        match envelope.payload {
            Some(wb::wr_envelope::Payload::Hello(hello)) => {
                if hello.protocol_major != supported_major {
                    let _ = writer.write_frame(
                        &wb::WrEnvelope {
                            correlation_id: correlation,
                            payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError {
                                code: wb::protocol_error::Code::ProtocolMajorMismatch as i32,
                                detail: format!(
                                    "runtime speaks major {}, resolver supports major {}",
                                    hello.protocol_major, supported_major
                                ),
                            })),
                        }
                        .encode_to_vec());
                    continue;
                }
                let facts = erf_fake_resolver::facts();
                let missing: Vec<String> = hello
                    .required_capabilities
                    .iter()
                    .filter(|c| !facts.get(*c).copied().unwrap_or(false))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    let _ = writer.write_frame(
                        &wb::WrEnvelope {
                            correlation_id: correlation,
                            payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError {
                                code: wb::protocol_error::Code::UnsupportedCapability as i32,
                                detail: format!("missing required capabilities: {missing:?}"),
                            })),
                        }
                        .encode_to_vec());
                    continue;
                }
                let ack_correlation = if bad_correlation { 999 } else { correlation };
                let mut encoded = wb::WrEnvelope {
                        correlation_id: ack_correlation,
                        payload: Some(wb::wr_envelope::Payload::HelloAck(wb::HelloAck {
                            protocol_major: hello.protocol_major,
                            protocol_minor,
                            resolver_name: "erf-fake-resolver".into(),
                            resolver_version: env!("CARGO_PKG_VERSION").into(),
                            facts,
                            accepted_limits: hello.limits,
                        })),
                    }
                    .encode_to_vec();
                if unknown_optional_on_hello {
                    // Field 15 is deliberately outside the v0 schema. The
                    // client must ignore this additive future-minor field and
                    // continue the handshake.
                    encoded.extend_from_slice(&[0x78, 0x01]);
                }
                let _ = writer.write_frame(&encoded);
                acknowledged = true;
            }
            Some(wb::wr_envelope::Payload::Resolve(resolve)) => {
                if !acknowledged {
                    let _ = writer.write_frame(
                        &wb::WrEnvelope {
                            correlation_id: correlation,
                            payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError {
                                code: wb::protocol_error::Code::Uncorrelatable as i32,
                                detail: "resolve before handshake".into(),
                            })),
                        }
                        .encode_to_vec());
                    continue;
                }
                if resolve_delay_ms > 0 {
                    let deadline = std::time::Instant::now()
                        + std::time::Duration::from_millis(resolve_delay_ms);
                    let mut cancelled = false;
                    while std::time::Instant::now() < deadline {
                        match rx.try_recv() {
                            Ok(Some(bytes)) => {
                                let pending = match wb::WrEnvelope::decode(bytes.as_slice()) {
                                    Ok(envelope) => envelope,
                                    Err(error) => {
                                        let _ = writer.write_frame(
                                            &wb::WrEnvelope {
                                                correlation_id: 0,
                                                payload: Some(wb::wr_envelope::Payload::Error(
                                                    wb::ProtocolError {
                                                        code: wb::protocol_error::Code::MalformedFrame as i32,
                                                        detail: error.to_string(),
                                                    },
                                                )),
                                            }
                                            .encode_to_vec(),
                                        );
                                        exit(1);
                                    }
                                };
                                if let Some(wb::wr_envelope::Payload::Cancel(_)) = pending.payload {
                                    let _ = writer.write_frame(
                                        &wb::WrEnvelope {
                                            correlation_id: pending.correlation_id,
                                            payload: Some(wb::wr_envelope::Payload::CancelAck(
                                                wb::CancelAck { accepted: true },
                                            )),
                                        }
                                        .encode_to_vec(),
                                    );
                                    let _ = writer.write_frame(
                                        &wb::WrEnvelope {
                                            correlation_id: 0,
                                            payload: Some(wb::wr_envelope::Payload::Completion(
                                                wb::Completion {
                                                    status: wb::completion::Status::Cancelled as i32,
                                                    reason: "resolution cancelled in flight".into(),
                                                },
                                            )),
                                        }
                                        .encode_to_vec(),
                                    );
                                    cancelled = true;
                                    break;
                                }
                            }
                            Ok(None) | Err(TryRecvError::Disconnected) => return,
                            Err(TryRecvError::Empty) => {
                                std::thread::sleep(std::time::Duration::from_millis(5));
                            }
                        }
                    }
                    if cancelled {
                        continue;
                    }
                }
                if crash_on_resolve {
                    exit(1);
                }
                if malformed_on_resolve {
                    let _ = writer.write_frame(&[0xff]);
                    exit(1);
                }
                if oversized_on_resolve {
                    let oversized = vec![0u8; 2048];
                    let _ = writer.write_frame(&oversized);
                    exit(1);
                }
                // A sandboxed Runner speaks in the container-visible mount
                // name. The fixture façade executes this fake binary on the
                // host, so translate that virtual path back to its mounted
                // host directory before resolving.
                let requested_root = std::path::Path::new(&resolve.repo_root);
                let host_root = if requested_root == std::path::Path::new("/workspace") {
                    std::env::var_os("ERF_REPO_ROOT")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| requested_root.to_path_buf())
                } else {
                    requested_root.to_path_buf()
                };
                let descriptor = erf_fake_resolver::resolve(&host_root);
                match descriptor {
                    Ok(descriptor) => {
                        let _ = writer.write_frame(
                            &wb::WrEnvelope {
                                correlation_id: correlation,
                                payload: Some(wb::wr_envelope::Payload::ResolveResponse(
                                    wb::ResolveResponse {
                                        workspace_descriptor: Some(descriptor),
                                    },
                                )),
                            }
                            .encode_to_vec());
                    }
                    Err(e) => {
                        let _ = writer.write_frame(
                            &wb::WrEnvelope {
                                correlation_id: correlation,
                                payload: Some(wb::wr_envelope::Payload::Error(
                                    wb::ProtocolError {
                                        code: wb::protocol_error::Code::RepoRootUnreadable
                                            as i32,
                                        detail: e.to_string(),
                                    },
                                )),
                            }
                            .encode_to_vec());
                    }
                }
            }
            Some(wb::wr_envelope::Payload::Cancel(_)) => {
                let _ = writer.write_frame(
                    &wb::WrEnvelope {
                        correlation_id: correlation,
                        payload: Some(wb::wr_envelope::Payload::CancelAck(wb::CancelAck {
                            accepted: true,
                        })),
                    }
                    .encode_to_vec());
            }
            Some(other) => {
                let _ = writer.write_frame(
                    &wb::WrEnvelope {
                        correlation_id: 0,
                        payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError {
                            code: wb::protocol_error::Code::Uncorrelatable as i32,
                            detail: format!("unexpected request frame: {other:?}"),
                        })),
                    }
                    .encode_to_vec());
            }
            None => {
                let _ = writer.write_frame(
                    &wb::WrEnvelope {
                        correlation_id: 0,
                        payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError {
                            code: wb::protocol_error::Code::MalformedFrame as i32,
                            detail: "envelope without payload".into(),
                        })),
                    }
                    .encode_to_vec());
                exit(1);
            }
        }
    }
}
