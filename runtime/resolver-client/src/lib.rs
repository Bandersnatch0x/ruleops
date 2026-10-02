//! Rust client for the ERF Workspace Resolution Contract v0
//! (`erf.workspace.v0`). Deliberately a separate client from the Backend Host
//! client (ADR-0050): both share only the framing crate and the envelope
//! conventions, never message types.

use erf_contracts::workspace::v0 as wb;
use erf_framing::{FrameError, FrameReader, FrameWriter};
use prost::Message;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Read;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, sync_channel};
use std::time::Duration;

pub const PROTOCOL_MAJOR: u32 = 0;
pub const PROTOCOL_MINOR: u32 = 1;
/// Maximum number of decoded resolver responses waiting for the runtime.
/// Keeping this explicit makes the memory bound part of the client contract.
pub const RESPONSE_QUEUE_CAPACITY: usize = 8;

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
    spawn_reader_from(stdout, max_frame_bytes)
}

fn spawn_reader_from<R>(
    stdout: R,
    max_frame_bytes: u64,
) -> Receiver<Result<wb::WrEnvelope, FrameError>>
where
    R: Read + Send + 'static,
{
    // A bounded response queue prevents a slow caller from turning resolver
    // output into unbounded Runtime memory growth.
    let (tx, rx) = sync_channel(RESPONSE_QUEUE_CAPACITY);
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
            limits: config.limits,
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
                limits: Some(config.limits),
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
                    if self.limits.deadline_ms > 0
                        && limits.deadline_ms > self.limits.deadline_ms
                    {
                        return Err(ResolverError::Violation(format!(
                            "resolver accepted deadline_ms {} above the runtime limit {}",
                            limits.deadline_ms, self.limits.deadline_ms
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
            Some(wb::wr_envelope::Payload::ResolveResponse(r)) => {
                let descriptor = r.workspace_descriptor.ok_or_else(|| {
                    ResolverError::Violation("ResolveResponse without descriptor".into())
                })?;
                validate_descriptor(&descriptor)?;
                verify_digest(&descriptor)?;
                Ok(descriptor)
            }
            Some(wb::wr_envelope::Payload::Error(e)) => Err(protocol_error(e)),
            Some(other) => Err(ResolverError::Violation(format!(
                "expected ResolveResponse, got {other:?}"
            ))),
            None => Err(ResolverError::Violation("empty envelope".into())),
        }
    }

    /// Starts a resolution and immediately sends cooperative cancellation.
    /// The resolver must correlate the CancelAck with the cancel request while
    /// the original resolution is still in flight; callers then consume the
    /// terminal CANCELLED Completion with [`Self::complete`].
    pub fn resolve_and_cancel(&mut self, repo_root: &str) -> Result<wb::CancelAck, ResolverError> {
        let resolve_correlation = self.next_correlation();
        self.write_frame(&wb::WrEnvelope {
            correlation_id: resolve_correlation,
            payload: Some(wb::wr_envelope::Payload::Resolve(wb::ResolveRequest {
                repo_root: repo_root.into(),
            })),
        })?;
        let cancel_correlation = self.next_correlation();
        self.write_frame(&wb::WrEnvelope {
            correlation_id: cancel_correlation,
            payload: Some(wb::wr_envelope::Payload::Cancel(wb::CancelRequest {})),
        })?;
        loop {
            let frame = self.next_envelope()?;
            match frame.correlation_id {
                correlation if correlation == cancel_correlation => {
                    return match frame.payload {
                        Some(wb::wr_envelope::Payload::CancelAck(ack)) => Ok(ack),
                        Some(wb::wr_envelope::Payload::Error(e)) => Err(protocol_error(e)),
                        Some(other) => Err(ResolverError::Violation(format!(
                            "expected CancelAck, got {other:?}"
                        ))),
                        None => Err(ResolverError::Violation("empty envelope".into())),
                    };
                }
                correlation if correlation == resolve_correlation => match frame.payload {
                    Some(wb::wr_envelope::Payload::ResolveResponse(response)) => {
                        let descriptor = response.workspace_descriptor.ok_or_else(|| {
                            ResolverError::Violation("ResolveResponse without descriptor".into())
                        })?;
                        validate_descriptor(&descriptor)?;
                        verify_digest(&descriptor)?;
                    }
                    Some(wb::wr_envelope::Payload::Error(e)) => return Err(protocol_error(e)),
                    Some(other) => {
                        return Err(ResolverError::Violation(format!(
                            "expected ResolveResponse, got {other:?}"
                        )));
                    }
                    None => return Err(ResolverError::Violation("empty envelope".into())),
                },
                correlation_id => return Err(ResolverError::Uncorrelatable { correlation_id }),
            }
        }
    }

    /// Cooperative cancellation of an in-flight resolution.
    pub fn cancel(&mut self) -> Result<wb::CancelAck, ResolverError> {
        let correlation = self.next_correlation();
        self.write_frame(&wb::WrEnvelope {
            correlation_id: correlation,
            payload: Some(wb::wr_envelope::Payload::Cancel(wb::CancelRequest {})),
        })?;
        let frame = self.next_envelope()?;
        if frame.correlation_id != correlation {
            return Err(ResolverError::Uncorrelatable {
                correlation_id: frame.correlation_id,
            });
        }
        match frame.payload {
            Some(wb::wr_envelope::Payload::CancelAck(ack)) => Ok(ack),
            Some(wb::wr_envelope::Payload::Error(e)) => Err(protocol_error(e)),
            Some(other) => Err(ResolverError::Violation(format!(
                "expected CancelAck, got {other:?}"
            ))),
            None => Err(ResolverError::Violation("empty envelope".into())),
        }
    }

    /// Half-closes stdin, then waits for the terminal Completion frame.
    pub fn complete(mut self) -> Result<wb::Completion, ResolverError> {
        let _ = self.writer.take(); // dropping closes the pipe: session end signal
        let frame = self.next_envelope()?;
            match frame.payload {
                Some(wb::wr_envelope::Payload::Completion(c)) => {
                    let exit = self.child.wait()?;
                    if c.status == wb::completion::Status::Completed as i32 && !exit.success() {
                        Err(ResolverError::Hangup {
                            status: exit.to_string(),
                        })
                    } else {
                        Ok(c)
                    }
                }
                Some(wb::wr_envelope::Payload::Error(e)) => Err(protocol_error(e)),
                Some(other) => {
                    Err(ResolverError::Violation(format!(
                        "expected Completion, got {other:?}"
                    )))
                }
                None => Err(ResolverError::Violation("empty envelope".into())),
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

/// Runtime-side re-verification of the resolver's content address.
fn verify_digest(descriptor: &wb::WorkspaceDescriptor) -> Result<(), ResolverError> {
    let mut blank = descriptor.clone();
    let claimed = std::mem::take(&mut blank.descriptor_sha256);
    let actual = Sha256::digest(blank.encode_to_vec()).to_vec();
    if claimed != actual {
        return Err(ResolverError::Violation(format!(
            "descriptor digest mismatch: claimed {:x?}, computed {actual:x?}",
            claimed
        )));
    }
    Ok(())
}

fn validate_descriptor(descriptor: &wb::WorkspaceDescriptor) -> Result<(), ResolverError> {
    match wb::WorkspaceResolutionMode::try_from(descriptor.mode) {
        Ok(wb::WorkspaceResolutionMode::SafeStatic)
        | Ok(wb::WorkspaceResolutionMode::AttestedBuildExport) => {}
        Ok(wb::WorkspaceResolutionMode::Unspecified) | Err(_) => {
            return Err(ResolverError::Violation(
                "workspace descriptor has an unspecified or unknown resolution mode".into(),
            ));
        }
    }
    validate_identity("resolver_id", &descriptor.resolver_id)?;
    validate_identity("resolver_version", &descriptor.resolver_version)?;
    if descriptor.descriptor_sha256.len() != 32 {
        return Err(ResolverError::Violation(
            "workspace descriptor digest must contain exactly 32 bytes".into(),
        ));
    }

    let mut module_ids = BTreeSet::new();
    let mut unresolved = BTreeSet::new();
    for fact in &descriptor.unresolved_facts {
        validate_text("unresolved_fact", fact)?;
        if !unresolved.insert(fact) {
            return Err(ResolverError::Violation(format!(
                "workspace descriptor repeats unresolved fact {fact:?}"
            )));
        }
    }
    for module in &descriptor.modules {
        validate_identity("module_id", &module.module_id)?;
        validate_identity("module_language", &module.language)?;
        if !module_ids.insert(&module.module_id) {
            return Err(ResolverError::Violation(format!(
                "workspace descriptor repeats module {:?}",
                module.module_id
            )));
        }
        for source_root in &module.source_roots {
            validate_relative_path("source_root", source_root)?;
        }
        let context_present = !module.classpath_entries.is_empty()
            || !module.build_output_roots.is_empty()
            || !module.invocation_identity.is_empty()
            || !module.classpath_sha256.is_empty()
            || !module.build_output_sha256.is_empty();
        if context_present {
            validate_identity("invocation_identity", &module.invocation_identity)?;
            for entry in &module.classpath_entries {
                validate_export_path("classpath_entry", entry)?;
            }
            for entry in &module.build_output_roots {
                validate_export_path("build_output_root", entry)?;
            }
            validate_context_digest(
                "classpath",
                &module.classpath_entries,
                &module.classpath_sha256,
            )?;
            validate_context_digest(
                "build output",
                &module.build_output_roots,
                &module.build_output_sha256,
            )?;
        }
        let mut files = BTreeSet::new();
        for file in &module.files {
            validate_relative_path("file", &file.path)?;
            if file.blob_sha256.len() != 32 {
                return Err(ResolverError::Violation(format!(
                    "file {:?} does not contain a 32-byte blob digest",
                    file.path
                )));
            }
            if !files.insert(&file.path) {
                return Err(ResolverError::Violation(format!(
                    "module {:?} repeats file {:?}",
                    module.module_id, file.path
                )));
            }
        }
    }
    Ok(())
}

fn validate_identity(name: &str, value: &str) -> Result<(), ResolverError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(ResolverError::Violation(format!(
            "workspace descriptor has an invalid {name}"
        )));
    }
    Ok(())
}

fn validate_text(name: &str, value: &str) -> Result<(), ResolverError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(ResolverError::Violation(format!(
            "workspace descriptor has an invalid {name}"
        )));
    }
    Ok(())
}

fn validate_relative_path(name: &str, value: &str) -> Result<(), ResolverError> {
    if value.is_empty()
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains('\\')
        || value.split('/').any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ResolverError::Violation(format!(
            "workspace descriptor has an invalid relative {name} path"
        )));
    }
    Ok(())
}

fn validate_export_path(name: &str, value: &str) -> Result<(), ResolverError> {
    if value.is_empty()
        || value.contains('\\')
        || value.chars().any(char::is_control)
        || value.split('/').enumerate().any(|(index, part)| {
            part == "." || part == ".." || (part.is_empty() && index > 0)
        })
    {
        return Err(ResolverError::Violation(format!(
            "workspace descriptor has an invalid {name} path"
        )));
    }
    Ok(())
}

fn validate_context_digest(
    label: &str,
    entries: &[String],
    supplied: &[u8],
) -> Result<(), ResolverError> {
    if supplied.len() != 32 {
        return Err(ResolverError::Violation(format!(
            "{label} context digest must contain exactly 32 bytes"
        )));
    }
    let mut payload = b"ruleops-java-context-v1".to_vec();
    payload.push(0);
    payload.extend_from_slice(label.as_bytes());
    payload.push(0);
    for entry in entries {
        payload.extend_from_slice(&(entry.len() as u64).to_be_bytes());
        payload.extend_from_slice(entry.as_bytes());
    }
    let expected = Sha256::digest(payload);
    if expected.as_slice() != supplied {
        return Err(ResolverError::Violation(format!(
            "{label} context digest does not match its ordered entries"
        )));
    }
    Ok(())
}

fn protocol_error(e: wb::ProtocolError) -> ResolverError {
    ResolverError::Protocol {
        code: wb::protocol_error::Code::try_from(e.code)
            .unwrap_or(wb::protocol_error::Code::Unspecified),
        detail: e.detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

    struct GateReader {
        bytes: Vec<u8>,
        position: usize,
        pause_at: usize,
        gate: Arc<AtomicBool>,
        release: Arc<AtomicBool>,
    }

    impl Read for GateReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.position >= self.pause_at && !self.gate.load(Ordering::Acquire) {
                self.gate.store(true, Ordering::Release);
                while !self.release.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
            }
            if self.position == self.bytes.len() {
                return Ok(0);
            }
            let count = buffer.len().min(self.bytes.len() - self.position);
            buffer[..count]
                .copy_from_slice(&self.bytes[self.position..self.position + count]);
            self.position += count;
            Ok(count)
        }
    }

    #[test]
    fn response_reader_stops_at_the_bounded_queue_capacity() {
        let mut bytes = Vec::new();
        let mut frame_ends = Vec::new();
        for correlation_id in 1..=RESPONSE_QUEUE_CAPACITY as u64 + 1 {
            let envelope = wb::WrEnvelope {
                correlation_id,
                payload: Some(wb::wr_envelope::Payload::CancelAck(wb::CancelAck {
                    accepted: true,
                })),
            };
            FrameWriter::new(&mut bytes, 1024)
                .write_frame(&envelope.encode_to_vec())
                .expect("fixture frame");
            frame_ends.push(bytes.len());
        }
        let gate = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let receiver = spawn_reader_from(
            GateReader {
                bytes,
                position: 0,
                pause_at: frame_ends[RESPONSE_QUEUE_CAPACITY - 1],
                gate: Arc::clone(&gate),
                release: Arc::clone(&release),
            },
            1024,
        );

        for _ in 0..10_000 {
            if gate.load(Ordering::Acquire) {
                break;
            }
            std::thread::yield_now();
        }
        assert!(
            gate.load(Ordering::Acquire),
            "reader did not reach the ninth-frame pause"
        );

        let queued = receiver.try_iter().count();
        assert_eq!(queued, RESPONSE_QUEUE_CAPACITY);
        release.store(true, Ordering::Release);
        drop(receiver);
    }
}
