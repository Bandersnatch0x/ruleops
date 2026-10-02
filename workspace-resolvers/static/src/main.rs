//! Framed stdio entry point for the SAFE_STATIC Maven/Gradle resolver pack.

use erf_contracts::workspace::v0 as wb;
use erf_framing::{FrameReader, FrameWriter};
use erf_static_resolver::{facts, resolve, BuildSystem};
use prost::Message;
use std::io::Write;

fn send<W: Write>(writer: &mut FrameWriter<W>, envelope: &wb::WrEnvelope) -> bool {
    writer.write_frame(&envelope.encode_to_vec()).is_ok()
}

fn main() {
    let kind = std::env::args().skip(1).collect::<Vec<_>>().windows(2)
        .find(|pair| pair[0] == "--kind").and_then(|pair| BuildSystem::from_name(&pair[1]))
        .or_else(|| std::env::var("ERF_STATIC_RESOLVER_KIND").ok().and_then(|v| BuildSystem::from_name(&v)))
        .unwrap_or(BuildSystem::Maven);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = FrameReader::new(stdin.lock(), erf_framing::DEFAULT_MAX_FRAME_BYTES);
    let mut writer = FrameWriter::new(stdout.lock(), erf_framing::DEFAULT_MAX_FRAME_BYTES);
    let mut acknowledged = false;
    loop {
        let frame = match reader.read_frame() {
            Ok(frame) => frame,
            Err(erf_framing::FrameError::Eof) => {
                let _ = send(&mut writer, &wb::WrEnvelope { correlation_id: 0, payload: Some(wb::wr_envelope::Payload::Completion(wb::Completion { status: wb::completion::Status::Completed as i32, reason: "session end".into() })) });
                return;
            }
            Err(error) => {
                let _ = send(&mut writer, &wb::WrEnvelope { correlation_id: 0, payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError { code: wb::protocol_error::Code::MalformedFrame as i32, detail: error.to_string() })) });
                return;
            }
        };
        let envelope = match wb::WrEnvelope::decode(frame.as_slice()) {
            Ok(value) => value,
            Err(error) => {
                let _ = send(&mut writer, &wb::WrEnvelope { correlation_id: 0, payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError { code: wb::protocol_error::Code::MalformedFrame as i32, detail: error.to_string() })) });
                return;
            }
        };
        let correlation_id = envelope.correlation_id;
        match envelope.payload {
            Some(wb::wr_envelope::Payload::Hello(hello)) => {
                if hello.protocol_major != 0 {
                    let _ = send(&mut writer, &wb::WrEnvelope { correlation_id, payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError { code: wb::protocol_error::Code::ProtocolMajorMismatch as i32, detail: "resolver supports protocol major 0".into() })) });
                    continue;
                }
                let available = facts();
                if let Some(missing) = hello.required_capabilities.iter().find(|name| available.get(*name) != Some(&true)) {
                    let _ = send(&mut writer, &wb::WrEnvelope { correlation_id, payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError { code: wb::protocol_error::Code::UnsupportedCapability as i32, detail: format!("missing required capability: {missing}") })) });
                    continue;
                }
                acknowledged = true;
                let accepted_limits = hello.limits.clone();
                let _ = send(&mut writer, &wb::WrEnvelope { correlation_id, payload: Some(wb::wr_envelope::Payload::HelloAck(wb::HelloAck { protocol_major: 0, protocol_minor: 1, resolver_name: kind.resolver_id().into(), resolver_version: env!("CARGO_PKG_VERSION").into(), facts: available, accepted_limits })) });
            }
            Some(wb::wr_envelope::Payload::Resolve(request)) if acknowledged => {
                let requested_root = std::path::Path::new(&request.repo_root);
                let mounted_root = if requested_root == std::path::Path::new("/workspace") {
                    std::env::var_os("ERF_REPO_ROOT")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| requested_root.to_path_buf())
                } else {
                    requested_root.to_path_buf()
                };
                match resolve(&mounted_root, kind) {
                    Ok(descriptor) => { let _ = send(&mut writer, &wb::WrEnvelope { correlation_id, payload: Some(wb::wr_envelope::Payload::ResolveResponse(wb::ResolveResponse { workspace_descriptor: Some(descriptor) })) }); }
                    Err(error) => { let _ = send(&mut writer, &wb::WrEnvelope { correlation_id, payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError { code: wb::protocol_error::Code::RepoRootUnreadable as i32, detail: error.to_string() })) }); }
                }
            }
            Some(wb::wr_envelope::Payload::Resolve(_)) => {
                let _ = send(&mut writer, &wb::WrEnvelope { correlation_id, payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError { code: wb::protocol_error::Code::Uncorrelatable as i32, detail: "resolve before handshake".into() })) });
            }
            Some(wb::wr_envelope::Payload::Cancel(_)) => {
                let _ = send(&mut writer, &wb::WrEnvelope { correlation_id, payload: Some(wb::wr_envelope::Payload::CancelAck(wb::CancelAck { accepted: true })) });
            }
            _ => {
                let _ = send(&mut writer, &wb::WrEnvelope { correlation_id, payload: Some(wb::wr_envelope::Payload::Error(wb::ProtocolError { code: wb::protocol_error::Code::Internal as i32, detail: "unexpected resolver request".into() })) });
            }
        }
    }
}
