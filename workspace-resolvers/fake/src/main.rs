//! Reference fake Workspace Resolver over the ERF Workspace Resolution
//! Contract v0. Resolution logic lives in the lib (shared with tests).
//! Knobs (env):
//! - `FAKE_SUPPORTED_MAJOR=<n>`  — protocol major accepted (default 0)
//! - `FAKE_BAD_CORRELATION=1`    — answers Hello with a mismatched correlation

use erf_contracts::workspace::v0 as wb;
use erf_framing::{FrameReader, FrameWriter};
use prost::Message;
use std::process::exit;
use std::sync::mpsc::{RecvError, channel};

fn send(writer: &mut FrameWriter<std::io::StdoutLock<'static>>, envelope: &wb::WrEnvelope) -> bool {
    if let Err(e) = writer.write_frame(&envelope.encode_to_vec()) {
        eprintln!("{{\"level\":\"error\",\"msg\":\"write failed: {e}\"}}");
        return false;
    }
    true
}

fn main() {
    let supported_major: u32 = std::env::var("FAKE_SUPPORTED_MAJOR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let bad_correlation = std::env::var("FAKE_BAD_CORRELATION").is_ok();

    let (tx, rx) = channel::<Option<Vec<u8>>>();
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
                    .filter(|c| !facts.contains_key(*c))
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
                let _ = writer.write_frame(
                    &wb::WrEnvelope {
                        correlation_id: ack_correlation,
                        payload: Some(wb::wr_envelope::Payload::HelloAck(wb::HelloAck {
                            protocol_major: hello.protocol_major,
                            protocol_minor: 1,
                            resolver_name: "erf-fake-resolver".into(),
                            resolver_version: env!("CARGO_PKG_VERSION").into(),
                            facts,
                            accepted_limits: hello.limits,
                        })),
                    }
                    .encode_to_vec());
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
                let descriptor = erf_fake_resolver::resolve(std::path::Path::new(&resolve.repo_root));
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
