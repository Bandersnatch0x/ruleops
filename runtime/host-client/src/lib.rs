//! Rust client for the ERF Backend Host Contract v0 (`erf.backend.v0`).
//!
//! Owns the host process lifecycle, the framed stdio protocol, request
//! correlation, per-request deadlines and cooperative cancellation. The
//! runtime never parses backend-private output: anything outside the contract
//! is a structured failure feeding Completeness (spec §11.1).
//!
//! Structure: a reader thread owns the child's stdout and forwards decoded
//! envelopes over an mpsc channel; session calls `recv` on it, applying the
//! negotiated per-request deadline. Dropping the handle kills a still-running
//! host, so a deadline abort cannot leak processes.

use erf_contracts::backend::v0 as pb;
use erf_framing::{FrameError, FrameReader, FrameWriter};
use prost::Message;
use std::io::Read;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, sync_channel};
use std::time::Duration;
use sha2::{Digest, Sha256};

pub const PROTOCOL_MAJOR: u32 = 0;
pub const PROTOCOL_MINOR: u32 = 1;
/// Maximum number of decoded host responses waiting for the runtime.
/// The reader applies backpressure instead of allowing unbounded child output
/// to grow Runtime memory.
pub const RESPONSE_QUEUE_CAPACITY: usize = 8;

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub required_capabilities: Vec<String>,
    pub optional_capabilities: Vec<String>,
    pub limits: pb::Limits,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            required_capabilities: Vec::new(),
            optional_capabilities: Vec::new(),
            limits: pb::Limits {
                max_frame_bytes: erf_framing::DEFAULT_MAX_FRAME_BYTES,
                max_events_per_execute: 100_000,
                deadline_ms: 0,
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("failed to spawn host: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("transport failure: {0}")]
    Transport(#[from] FrameError),
    #[error("host protocol error {code:?}: {detail}")]
    Protocol {
        code: pb::protocol_error::Code,
        detail: String,
    },
    #[error("request deadline of {deadline_ms} ms exceeded")]
    DeadlineExceeded { deadline_ms: u64 },
    #[error("host exited without a Completion frame (exit status: {status})")]
    Hangup { status: String },
    #[error("host answered HelloAck with protocol major {acked}, requested {requested}")]
    MajorMismatch { requested: u32, acked: u32 },
    #[error("host lacks required capabilities: {missing:?}")]
    MissingCapabilities { missing: Vec<String> },
    #[error("host sent a frame for unknown correlation id {correlation_id}")]
    Uncorrelatable { correlation_id: u64 },
    #[error("host exceeded the negotiated {max_events} events-per-execute limit")]
    EventFlood { max_events: u64 },
    #[error("host violated the protocol: {0}")]
    Violation(String),
    #[error("io failure: {0}")]
    Io(#[from] std::io::Error),
}

pub struct HostHandle {
    child: Child,
    // Taken (dropped) on half-close; dropping stdin signals session end.
    writer: Option<FrameWriter<ChildStdin>>,
    rx: Receiver<Result<(Vec<u8>, pb::HostEnvelope), FrameError>>,
    next_correlation: u64,
    limits: pb::Limits,
}

impl Drop for HostHandle {
    fn drop(&mut self) {
        // A session aborted mid-flight (deadline, violation) must not leak the
        // host process.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Reads frames on a dedicated thread, forwarding decoded envelopes. Any
/// transport or decode error terminates the stream after being forwarded once.
fn spawn_reader(
    stdout: ChildStdout,
    max_frame_bytes: u64,
) -> Receiver<Result<(Vec<u8>, pb::HostEnvelope), FrameError>> {
    spawn_reader_from(stdout, max_frame_bytes)
}

fn spawn_reader_from<R>(
    stdout: R,
    max_frame_bytes: u64,
) -> Receiver<Result<(Vec<u8>, pb::HostEnvelope), FrameError>>
where
    R: Read + Send + 'static,
{
    let (tx, rx) = sync_channel(RESPONSE_QUEUE_CAPACITY);
    std::thread::spawn(move || {
        let mut reader = FrameReader::new(stdout, max_frame_bytes);
        loop {
            let outcome = reader.read_frame().and_then(|bytes| {
                let envelope = pb::HostEnvelope::decode(bytes.as_slice())
                    .map_err(|e| FrameError::MalformedProtobuf(e.to_string()))?;
                Ok((bytes, envelope))
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

impl HostHandle {
    /// Spawns the host process and performs the Hello/HelloAck handshake,
    /// enforcing major-version match, required capabilities and limit sanity.
    pub fn spawn(
        command: &mut Command,
        config: &HostConfig,
    ) -> Result<(Self, pb::HelloAck), HostError> {
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = command.spawn().map_err(HostError::Spawn)?;
        let stdout: ChildStdout = child.stdout.take().expect("stdout piped");
        let stdin: ChildStdin = child.stdin.take().expect("stdin piped");
        let rx = spawn_reader(stdout, config.limits.max_frame_bytes);
        let mut handle = Self {
            child,
            writer: Some(FrameWriter::new(stdin, config.limits.max_frame_bytes)),
            rx,
            next_correlation: 1,
            limits: config.limits,
        };
        let ack = handle.handshake(config)?;
        Ok((handle, ack))
    }

    fn handshake(&mut self, config: &HostConfig) -> Result<pb::HelloAck, HostError> {
        let correlation = self.next_correlation();
        let hello = pb::HostEnvelope {
            correlation_id: correlation,
            payload: Some(pb::host_envelope::Payload::Hello(pb::Hello {
                protocol_major: PROTOCOL_MAJOR,
                protocol_minor: PROTOCOL_MINOR,
                required_capabilities: config.required_capabilities.clone(),
                optional_capabilities: config.optional_capabilities.clone(),
                limits: Some(config.limits),
            })),
        };
        self.write_frame(&hello)?;
        let ack_frame = self.next_envelope()?;
        self.check_correlation(&ack_frame, correlation)?;
        match ack_frame.payload {
            Some(pb::host_envelope::Payload::HelloAck(ack)) => {
                if ack.protocol_major != PROTOCOL_MAJOR {
                    return Err(HostError::MajorMismatch {
                        requested: PROTOCOL_MAJOR,
                        acked: ack.protocol_major,
                    });
                }
                let caps = ack.capabilities.as_ref().ok_or_else(|| {
                    HostError::Violation("HelloAck without CapabilityDescriptor".into())
                })?;
                let missing: Vec<String> = config
                    .required_capabilities
                    .iter()
                    .filter(|c| !caps.facts.contains_key(*c))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    return Err(HostError::MissingCapabilities { missing });
                }
                if let Some(limits) = &ack.accepted_limits {
                    if limits.max_frame_bytes > self.limits.max_frame_bytes {
                        return Err(HostError::Violation(format!(
                            "host accepted max_frame_bytes {} above the runtime limit {}",
                            limits.max_frame_bytes, self.limits.max_frame_bytes
                        )));
                    }
                }
                Ok(ack)
            }
            Some(pb::host_envelope::Payload::Error(e)) => Err(protocol_error(e)),
            Some(other) => Err(HostError::Violation(format!(
                "expected HelloAck, got {other:?}"
            ))),
            None => Err(HostError::Violation("empty envelope".into())),
        }
    }

    pub fn describe(&mut self) -> Result<pb::CapabilityDescriptor, HostError> {
        let correlation = self.next_correlation();
        self.request(
            correlation,
            pb::host_envelope::Payload::Describe(pb::DescribeRequest {}),
            |payload| match payload {
                Some(pb::host_envelope::Payload::DescribeResponse(r)) => {
                    r.capabilities.ok_or_else(|| {
                        HostError::Violation("DescribeResponse without capabilities".into())
                    })
                }
                Some(pb::host_envelope::Payload::Error(e)) => Err(protocol_error(e)),
                Some(other) => Err(HostError::Violation(format!(
                    "expected DescribeResponse, got {other:?}"
                ))),
                None => Err(HostError::Violation("empty envelope".into())),
            },
        )
    }

    pub fn plan(&mut self, units: Vec<pb::AnalysisUnit>) -> Result<pb::PlanResponse, HostError> {
        let correlation = self.next_correlation();
        self.request(
            correlation,
            pb::host_envelope::Payload::Plan(pb::PlanRequest { units }),
            |payload| match payload {
                Some(pb::host_envelope::Payload::PlanResponse(r)) => Ok(r),
                Some(pb::host_envelope::Payload::Error(e)) => Err(protocol_error(e)),
                Some(other) => Err(HostError::Violation(format!(
                    "expected PlanResponse, got {other:?}"
                ))),
                None => Err(HostError::Violation("empty envelope".into())),
            },
        )
    }

    /// Starts an execute and returns the event stream. Every requested unit
    /// must be accounted by a UnitStatus (the per-unit completeness fact)
    /// before ExecuteComplete.
    pub fn execute(&mut self, unit_ids: Vec<String>) -> Result<ExecuteStream<'_>, HostError> {
        let correlation = self.next_correlation();
        let envelope = pb::HostEnvelope {
            correlation_id: correlation,
            payload: Some(pb::host_envelope::Payload::Execute(pb::ExecuteRequest {
                unit_ids,
            })),
        };
        self.write_frame(&envelope)?;
        // No synchronous response exists for execute: events (or a
        // ProtocolError) flow through the ExecuteStream.
        Ok(ExecuteStream {
            handle: self,
            correlation,
            events_seen: 0,
            finished: false,
            transcript: Sha256::new(),
        })
    }

    fn request<T>(
        &mut self,
        correlation: u64,
        payload: pb::host_envelope::Payload,
        then: impl FnOnce(Option<pb::host_envelope::Payload>) -> Result<T, HostError>,
    ) -> Result<T, HostError> {
        let envelope = pb::HostEnvelope {
            correlation_id: correlation,
            payload: Some(payload),
        };
        self.write_frame(&envelope)?;
        let frame = self.next_envelope()?;
        self.check_correlation(&frame, correlation)?;
        then(frame.payload)
    }

    fn check_correlation(
        &mut self,
        frame: &pb::HostEnvelope,
        expected: u64,
    ) -> Result<(), HostError> {
        if frame.correlation_id != expected {
            return Err(HostError::Uncorrelatable {
                correlation_id: frame.correlation_id,
            });
        }
        Ok(())
    }

    fn write_frame(&mut self, envelope: &pb::HostEnvelope) -> Result<(), HostError> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| HostError::Violation("session stdin already half-closed".into()))?;
        writer.write_frame(&envelope.encode_to_vec())?;
        Ok(())
    }

    /// Next envelope from the reader thread, applying the negotiated deadline
    /// (limits.deadline_ms; 0 = wait indefinitely).
    fn next_frame(&mut self) -> Result<(Vec<u8>, pb::HostEnvelope), HostError> {
        let outcome = if self.limits.deadline_ms > 0 {
            match self
                .rx
                .recv_timeout(Duration::from_millis(self.limits.deadline_ms))
            {
                Ok(v) => v,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(HostError::DeadlineExceeded {
                        deadline_ms: self.limits.deadline_ms,
                    });
                }
                Err(RecvTimeoutError::Disconnected) => return Err(HostError::Transport(FrameError::Eof)),
            }
        } else {
            self.rx.recv().map_err(|_| HostError::Transport(FrameError::Eof))?
        };
        match outcome {
            Ok(frame) => Ok(frame),
            Err(FrameError::Eof) => Err(HostError::Transport(FrameError::Eof)),
            Err(other) => Err(HostError::Transport(other)),
        }
    }

    fn next_envelope(&mut self) -> Result<pb::HostEnvelope, HostError> {
        Ok(self.next_frame()?.1)
    }

    fn next_correlation(&mut self) -> u64 {
        let id = self.next_correlation;
        self.next_correlation += 1;
        id
    }

    /// Half-closes stdin, then waits for the terminal Completion frame and the
    /// process to exit. A non-zero exit after COMPLETED is a Hangup.
    pub fn complete(mut self) -> Result<pb::Completion, HostError> {
        let _ = self.writer.take(); // dropping closes the pipe: session end signal
        let frame = self.next_envelope()?;
            match frame.payload {
                Some(pb::host_envelope::Payload::Completion(c)) => {
                    let exit = self.child.wait()?;
                    if c.status == pb::completion::Status::Completed as i32 && !exit.success() {
                        Err(HostError::Hangup {
                            status: exit.to_string(),
                        })
                    } else {
                        Ok(c)
                    }
                }
                Some(pb::host_envelope::Payload::Error(e)) => Err(protocol_error(e)),
                Some(other) => {
                    Err(HostError::Violation(format!(
                        "expected Completion, got {other:?}"
                    )))
                }
                None => Err(HostError::Violation("empty envelope".into())),
            }
    }
}

fn protocol_error(e: pb::ProtocolError) -> HostError {
    HostError::Protocol {
        code: pb::protocol_error::Code::try_from(e.code)
            .unwrap_or(pb::protocol_error::Code::Unspecified),
        detail: e.detail,
    }
}

/// Streaming read side of an execute request. Yields observations and unit
/// statuses until ExecuteComplete. `cancel` performs cooperative cancellation.
pub struct ExecuteStream<'a> {
    handle: &'a mut HostHandle,
    correlation: u64,
    events_seen: u64,
    finished: bool,
    transcript: Sha256,
}

#[derive(Debug)]
pub enum ExecEvent {
    Observation(pb::BackendObservation),
    UnitStatus(pb::UnitStatus),
    Complete(pb::ExecuteComplete),
}

impl<'a> ExecuteStream<'a> {
    pub fn correlation_id(&self) -> u64 {
        self.correlation
    }

    /// Next event, or None once ExecuteComplete (or a post-cancel Completion)
    /// has been observed.
    pub fn next_event(&mut self) -> Result<Option<ExecEvent>, HostError> {
        if self.finished {
            return Ok(None);
        }
        let (payload, frame) = self.handle.next_frame()?;
            let mut delimited = Vec::with_capacity(4 + payload.len());
            delimited.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            delimited.extend_from_slice(&payload);
            self.transcript.update(delimited);
            if frame.correlation_id != self.correlation {
                return Err(HostError::Uncorrelatable {
                    correlation_id: frame.correlation_id,
                });
            }
            match frame.payload {
                Some(pb::host_envelope::Payload::ExecuteEvent(ev)) => {
                    self.events_seen += 1;
                    if self.events_seen > self.handle.limits.max_events_per_execute {
                        return Err(HostError::EventFlood {
                            max_events: self.handle.limits.max_events_per_execute,
                        });
                    }
                    match ev.event {
                        Some(pb::execute_event::Event::Observation(o)) => {
                            Ok(Some(ExecEvent::Observation(o)))
                        }
                        Some(pb::execute_event::Event::UnitStatus(u)) => {
                            Ok(Some(ExecEvent::UnitStatus(u)))
                        }
                        None => {
                            Err(HostError::Violation(
                                "ExecuteEvent without event payload".into(),
                            ))
                        }
                    }
                }
                Some(pb::host_envelope::Payload::ExecuteComplete(c)) => {
                    self.finished = true;
                    Ok(Some(ExecEvent::Complete(c)))
                }
                Some(pb::host_envelope::Payload::Completion(c)) => {
                    // Terminal: fatal failure or cancellation finalization.
                    self.finished = true;
                    let status = pb::completion::Status::try_from(c.status)
                        .unwrap_or(pb::completion::Status::Failed);
                    match status {
                        pb::completion::Status::Cancelled => Ok(None),
                        _ => Err(HostError::Violation(format!(
                            "Completion {:?} during execute: {}",
                            status, c.reason
                        ))),
                    }
                }
                Some(pb::host_envelope::Payload::Error(e)) => Err(protocol_error(e)),
                Some(other) => {
                    Err(HostError::Violation(format!(
                        "unexpected frame during execute: {other:?}"
                    )))
                }
                None => Err(HostError::Violation("empty envelope".into())),
            }
    }

    /// Digest of the exact length-delimited response frames received for execute.
    pub fn transcript_digest(&self) -> Vec<u8> {
        self.transcript.clone().finalize().to_vec()
    }

    /// Sends CancelRequest for this execute. Events keep flowing until the
    /// host reaches its next cooperative point and reports Completion.
    pub fn cancel(&mut self) -> Result<(), HostError> {
        let envelope = pb::HostEnvelope {
            correlation_id: self.correlation,
            payload: Some(pb::host_envelope::Payload::Cancel(pb::CancelRequest {})),
        };
        self.handle.write_frame(&envelope)
    }

    /// Drains the stream to the terminal Completion (used after cancel).
    /// Host-initiated terminal frames use correlation id 0.
    pub fn completion(self) -> Result<pb::Completion, HostError> {
        loop {
            let frame = self.handle.next_envelope()?;
            let terminal = matches!(
                frame.payload,
                Some(pb::host_envelope::Payload::Completion(_))
            );
            if frame.correlation_id != self.correlation && !(terminal && frame.correlation_id == 0)
            {
                return Err(HostError::Uncorrelatable {
                    correlation_id: frame.correlation_id,
                });
            }
            match frame.payload {
                Some(pb::host_envelope::Payload::Completion(c)) => return Ok(c),
                Some(pb::host_envelope::Payload::Error(e)) => return Err(protocol_error(e)),
                Some(pb::host_envelope::Payload::ExecuteEvent(_))
                | Some(pb::host_envelope::Payload::ExecuteComplete(_))
                | Some(pb::host_envelope::Payload::CancelAck(_)) => continue,
                Some(other) => {
                    return Err(HostError::Violation(format!(
                        "expected Completion, got {other:?}"
                    )))
                }
                None => return Err(HostError::Violation("empty envelope".into())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc::SyncSender};
    use std::time::Duration;

    struct GateReader {
        bytes: Vec<u8>,
        position: usize,
        pause_at: usize,
        ready: Option<SyncSender<()>>,
        release: Arc<AtomicBool>,
    }

    impl Read for GateReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.position >= self.pause_at {
                if let Some(ready) = self.ready.take() {
                    let _ = ready.send(());
                }
                while !self.release.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
            }
            if self.position == self.bytes.len() {
                return Ok(0);
            }
            let count = buffer.len().min(self.bytes.len() - self.position);
            buffer[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
            self.position += count;
            Ok(count)
        }
    }

    #[test]
    fn response_reader_stops_at_the_bounded_queue_capacity() {
        let mut bytes = Vec::new();
        let mut frame_ends = Vec::new();
        for correlation_id in 1..=RESPONSE_QUEUE_CAPACITY as u64 + 1 {
            let envelope = pb::HostEnvelope {
                correlation_id,
                payload: Some(pb::host_envelope::Payload::CancelAck(pb::CancelAck {
                    accepted: true,
                })),
            };
            FrameWriter::new(&mut bytes, 1024)
                .write_frame(&envelope.encode_to_vec())
                .expect("fixture frame");
            frame_ends.push(bytes.len());
        }
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let release = Arc::new(AtomicBool::new(false));
        let receiver = spawn_reader_from(
            GateReader {
                bytes,
                position: 0,
                pause_at: frame_ends[RESPONSE_QUEUE_CAPACITY - 1],
                ready: Some(ready_tx),
                release: Arc::clone(&release),
            },
            1024,
        );

        ready_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("reader did not reach the ninth-frame pause");
        assert_eq!(receiver.try_iter().count(), RESPONSE_QUEUE_CAPACITY);
        release.store(true, Ordering::Release);
        drop(receiver);
    }
}
