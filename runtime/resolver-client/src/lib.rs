//! Rust client for the ERF Workspace Resolution Contract v0
//! (`erf.workspace.v0`). Deliberately a separate client from the Backend Host
//! client (ADR-0050): both share only the framing crate and the envelope
//! conventions, never message types.

use erf_contracts::workspace::v0 as wb;
use erf_framing::{FrameError, FrameReader, FrameWriter};
use prost::Message;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

pub const PROTOCOL_MAJOR: u32 = 0;
pub const PROTOCOL_MINOR: u32 = 1;

#[derive(Debug, Clone)]
pub struct ResolverConfig {
    pub required_capabilities: Vec<String>,
    pub limits: wb::Limits,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            required_capabilities: Vec::new(),
            limits: wb::Limits {
                max_frame_bytes: erf_framing::DEFAULT_MAX_FRAME_BYTES,
                deadline_ms: 0,
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResolverError {
    #[error("failed to spawn resolver: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("transport failure: {0}")]
    Transport(#[from] FrameError),
    #[error("resolver protocol error {code:?}: {detail}")]
    Protocol {
        code: wb::protocol_error::Code,
        detail: String,
    },
    #[error("resolution deadline of {deadline_ms} ms exceeded")]
    DeadlineExceeded { deadline_ms: u64 },
    #[error("resolver exited without a Completion frame (exit status: {status})")]
    Hangup { status: String },
    #[error("resolver answered HelloAck with protocol major {acked}, requested {requested}")]
    MajorMismatch { requested: u32, acked: u32 },
    #[error("resolver lacks required capabilities: {missing:?}")]
    MissingCapabilities { missing: Vec<String> },
    #[error("resolver sent a frame for unknown correlation id {correlation_id}")]
    Uncorrelatable { correlation_id: u64 },
    #[error("resolver violated the protocol: {0}")]
    Violation(String),
    #[error("io failure: {0}")]
    Io(#[from] std::io::Error),
}

pub struct ResolverHandle {
    child: Child,
    writer: Option<FrameWriter<ChildStdin>>,
    rx: Receiver<Result<wb::WrEnvelope, FrameError>>,
    next_correlation: u64,
    limits: wb::Limits,
}

impl Drop for ResolverHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_reader(
    stdout: ChildStdout,
    max_frame_bytes: u64,
) -> Receiver<Result<wb::WrEnvelope, FrameError>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut reader = FrameReader::new(stdout, max_frame_bytes);
        loop {
            let outcome = reader.read_frame().and_then(|bytes| {
                wb::WrEnvelope::decode(bytes.as_slice())
                    .map_err(|e| FrameError::MalformedProtobuf(e.to_string()))
            });
            let terminal = matches!(&outcome, Err(FrameError::Eof))
                || matches!(&outcome, Err(FrameError::MalformedProtobuf(_)));
            if tx.send(outcome).is_err() {
                break;
            }
            if terminal {
                break;
            }
        }
    });
    rx
}

impl ResolverHandle {
    /// Spawns the resolver process and performs the Hello/HelloAck handshake.
    pub fn spawn(
        command: &mut Command,
        config: &ResolverConfig,
    ) -> Result<(Self, wb::HelloAck), ResolverError> {
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = command.spawn().map_err(ResolverError::Spawn)?;
        let stdout: ChildStdout = child.stdout.take().expect("stdout piped");
        let stdin: ChildStdin = child.stdin.take().expect("stdin piped");
        let rx = spawn_reader(stdout, config.limits.max_frame_bytes);
        let mut handle = Self {
            child,
            writer: Some(FrameWriter::new(stdin, config.limits.max_frame_bytes)),
            rx,
            next_correlation: 1,
            limits: config.limits.clone(),
        };
        let ack = handle.handshake(config)?;
        Ok((handle, ack))
    }

    fn handshake(&mut self, config: &ResolverConfig) -> Result<wb::HelloAck, ResolverError> {
        let correlation = self.next_correlation();
        self.write_frame(&wb::WrEnvelope {
            correlation_id: correlation,
            payload: Some(wb::wr_envelope::Payload::Hello(wb::Hello {
                protocol_major: PROTOCOL_MAJOR,
                protocol_minor: PROTOCOL_MINOR,
                required_capabilities: config.required_capabilities.clone(),
                optional_capabilities: vec![],
                limits: Some(config.limits.clone()),
            })),
        })?;
        let frame = self.next_envelope()?;
        if frame.correlation_id != correlation {
            return Err(ResolverError::Uncorrelatable {
                correlation_id: frame.correlation_id,
            });
        }
        match frame.payload {
            Some(wb::wr_envelope::Payload::HelloAck(ack)) => {
                if ack.protocol_major != PROTOCOL_MAJOR {
                    return Err(ResolverError::MajorMismatch {
                        requested: PROTOCOL_MAJOR,
                        acked: ack.protocol_major,
                    });
                }
                let missing: Vec<String> = config
                    .required_capabilities
                    .iter()
                    .filter(|c| !ack.facts.contains_key(*c))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    return Err(ResolverError::MissingCapabilities { missing });
                }
                if let Some(limits) = &ack.accepted_limits {
                    if limits.max_frame_bytes > self.limits.max_frame_bytes {
                        return Err(ResolverError::Violation(format!(
                            "resolver accepted max_frame_bytes {} above the runtime limit {}",
                            limits.max_frame_bytes, self.limits.max_frame_bytes
                        )));
                    }
                }
                Ok(ack)
            }
            Some(wb::wr_envelope::Payload::Error(e)) => Err(protocol_error(e)),
            Some(other) => Err(ResolverError::Violation(format!(
                "expected HelloAck, got {other:?}"
            ))),
            None => Err(ResolverError::Violation("empty envelope".into())),
        }
    }

    /// Resolves the project structure facts for the given snapshot root.
    pub fn resolve(&mut self, repo_root: &str) -> Result<wb::WorkspaceDescriptor, ResolverError> {
        let correlation = self.next_correlation();
        self.write_frame(&wb::WrEnvelope {
            correlation_id: correlation,
            payload: Some(wb::wr_envelope::Payload::Resolve(wb::ResolveRequest {
                repo_root: repo_root.into(),
            })),
        })?;
        let frame = self.next_envelope()?;
        if frame.correlation_id != correlation {
            return Err(ResolverError::Uncorrelatable {
                correlation_id: frame.correlation_id,
            });
        }
        match frame.payload {
            Some(wb::wr_envelope::Payload::ResolveResponse(r)) => r
                .workspace_descriptor
                .ok_or_else(|| ResolverError::Violation("ResolveResponse without descriptor".into())),
            Some(wb::wr_envelope::Payload::Error(e)) => Err(protocol_error(e)),
            Some(other) => Err(ResolverError::Violation(format!(
                "expected ResolveResponse, got {other:?}"
            ))),
            None => Err(ResolverError::Violation("empty envelope".into())),
        }
    }

    /// Half-closes stdin, then waits for the terminal Completion frame.
    pub fn complete(mut self) -> Result<wb::Completion, ResolverError> {
        let _ = self.writer.take(); // dropping closes the pipe: session end signal
        loop {
            let frame = self.next_envelope()?;
            match frame.payload {
                Some(wb::wr_envelope::Payload::Completion(c)) => {
                    let exit = self.child.wait()?;
                    if c.status == wb::completion::Status::Completed as i32 && !exit.success() {
                        return Err(ResolverError::Hangup {
                            status: exit.to_string(),
                        });
                    }
                    return Ok(c);
                }
                Some(wb::wr_envelope::Payload::Error(e)) => return Err(protocol_error(e)),
                Some(other) => {
                    return Err(ResolverError::Violation(format!(
                        "expected Completion, got {other:?}"
                    )))
                }
                None => return Err(ResolverError::Violation("empty envelope".into())),
            }
        }
    }

    fn write_frame(&mut self, envelope: &wb::WrEnvelope) -> Result<(), ResolverError> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| ResolverError::Violation("session stdin already half-closed".into()))?;
        writer.write_frame(&envelope.encode_to_vec())?;
        Ok(())
    }

    fn next_envelope(&mut self) -> Result<wb::WrEnvelope, ResolverError> {
        let outcome = if self.limits.deadline_ms > 0 {
            match self
                .rx
                .recv_timeout(Duration::from_millis(self.limits.deadline_ms))
            {
                Ok(v) => v,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(ResolverError::DeadlineExceeded {
                        deadline_ms: self.limits.deadline_ms,
                    });
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(ResolverError::Transport(FrameError::Eof));
                }
            }
        } else {
            self.rx.recv().map_err(|_| ResolverError::Transport(FrameError::Eof))?
        };
        match outcome {
            Ok(envelope) => Ok(envelope),
            Err(FrameError::Eof) => Err(ResolverError::Transport(FrameError::Eof)),
            Err(other) => Err(ResolverError::Transport(other)),
        }
    }

    fn next_correlation(&mut self) -> u64 {
        let id = self.next_correlation;
        self.next_correlation += 1;
        id
    }
}

fn protocol_error(e: wb::ProtocolError) -> ResolverError {
    ResolverError::Protocol {
        code: wb::protocol_error::Code::try_from(e.code)
            .unwrap_or(wb::protocol_error::Code::Unspecified),
        detail: e.detail,
    }
}
