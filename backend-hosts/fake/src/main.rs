//! Reference fake Backend Host: the executable reference implementation of the
//! ERF Backend Host Contract v0. Serves canned capability/plan/execute
//! responses. Failure-injection knobs (env):
//!
//! - `FAKE_POLLUTE_STDOUT=1`   — writes non-protocol bytes to stdout first
//! - `FAKE_DELAY_MS=<n>`       — sleeps before each execute event; the window
//!   where a pending CancelRequest is detected (cooperative cancellation) and
//!   where a client-side deadline can be observed to fire
//! - `FAKE_SUPPORTED_MAJOR=<n>` — protocol major this fake accepts (default 0)
//! - `FAKE_BAD_CORRELATION=1`  — answers Hello with a mismatched correlation id
//! - `FAKE_CRASH_ON_EXECUTE=1` — exits during execute without Completion
//! - `FAKE_FAIL_UNIT=<id>` — emits a failed UnitStatus for one unit
//! - `FAKE_MAX_FRAME_BYTES=<n>` — receiver frame cap (default: 16 MiB)
//!
//! Deterministic cancellation: with `FAKE_DELAY_MS` set, the fake sleeps
//! before each unit's first event and then checks for a pending CancelRequest;
//! a cancel completes the session with Completion(CANCELLED) plus a
//! UnitStatus(FAILED, CANCELLED_BY_RUNTIME) accounting the interrupted unit.
//!
//! Structure: a reader thread owns stdin and forwards envelopes over an mpsc
//! channel; the main thread dispatches and, during execute, polls the channel
//! between events to detect cooperative cancellation.

use erf_contracts::backend::v0 as pb;
use erf_fake_host::capabilities_for_runtime_backend;
use erf_framing::{FrameError, FrameReader, FrameWriter};
use prost::Message;
use std::io::Write;
use std::process::exit;
use std::sync::mpsc::{Receiver, RecvError, TryRecvError, channel};
use std::time::Duration;

fn main() {
    if std::env::var("FAKE_POLLUTE_STDOUT").is_ok() {
        // Contract violation probe: stdout must carry protocol frames only.
        let _ = std::io::stdout().write_all(b"POLLUTION\n");
        let _ = std::io::stdout().flush();
    }
    let supported_major: u32 = std::env::var("FAKE_SUPPORTED_MAJOR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let delay_ms: u64 = std::env::var("FAKE_DELAY_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let bad_correlation = std::env::var("FAKE_BAD_CORRELATION").is_ok();
    let crash_on_execute = std::env::var("FAKE_CRASH_ON_EXECUTE").is_ok();
    let fail_unit = std::env::var("FAKE_FAIL_UNIT").ok();
    let max_frame_bytes = std::env::var("FAKE_MAX_FRAME_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(erf_framing::DEFAULT_MAX_FRAME_BYTES);

    let (tx, rx) = channel::<Inbound>();
    let _reader = std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut reader = FrameReader::new(stdin.lock(), max_frame_bytes);
        loop {
            match reader.read_frame() {
                Ok(bytes) => {
                    if tx.send(Inbound::Frame(bytes)).is_err() {
                        break;
                    }
                }
                Err(FrameError::Eof) => {
                    let _ = tx.send(Inbound::Eof);
                    break;
                }
                Err(error) => {
                    let _ = tx.send(Inbound::FramingError(error));
                    break;
                }
            }
        }
    });

    let stdout = std::io::stdout();
    let mut writer =
        FrameWriter::new(stdout.lock(), erf_framing::DEFAULT_MAX_FRAME_BYTES);
    let mut session = Session {
        supported_major,
        delay_ms,
        bad_correlation,
        crash_on_execute,
        fail_unit,
        writer: &mut writer,
        rx: &rx,
        cancelled: false,
        units_reported_total: 0,
    };

    loop {
        match rx.recv() {
            Ok(Inbound::Frame(bytes)) => {
                let envelope = match pb::HostEnvelope::decode(bytes.as_slice()) {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = session
                            .fail(pb::protocol_error::Code::MalformedFrame, e.to_string());
                        exit(1);
                    }
                };
                if !session.dispatch(envelope) {
                    return;
                }
            }
            Ok(Inbound::FramingError(error)) => {
                let code = match &error {
                    FrameError::Oversized { .. } => pb::protocol_error::Code::OversizedFrame,
                    FrameError::UnexpectedEof { .. }
                    | FrameError::MalformedProtobuf(_)
                    | FrameError::Eof => pb::protocol_error::Code::MalformedFrame,
                    FrameError::Io(_) => pb::protocol_error::Code::Internal,
                };
                let _ = session.fail(code, error.to_string());
                exit(1);
            }
            Ok(Inbound::Eof) | Err(RecvError) => {
                // stdin half-closed by the runtime: normal session end.
                let total = session.units_reported_total;
                let _ = session.complete(pb::completion::Status::Completed, "session end", total);
                return;
            }
        }
    }
}

enum Inbound {
    Frame(Vec<u8>),
    Eof,
    FramingError(FrameError),
}

struct Session<'a> {
    supported_major: u32,
    delay_ms: u64,
    bad_correlation: bool,
    crash_on_execute: bool,
    fail_unit: Option<String>,
    writer: &'a mut FrameWriter<std::io::StdoutLock<'static>>,
    rx: &'a Receiver<Inbound>,
    cancelled: bool,
    units_reported_total: u64,
}

impl<'a> Session<'a> {
    fn send(&mut self, envelope: pb::HostEnvelope) -> bool {
        self.writer.write_frame(&envelope.encode_to_vec()).is_ok()
    }

    fn error(&mut self, correlation: u64, code: pb::protocol_error::Code, detail: String) -> bool {
        self.send(pb::HostEnvelope {
            correlation_id: correlation,
            payload: Some(pb::host_envelope::Payload::Error(pb::ProtocolError {
                code: code as i32,
                detail,
            })),
        })
    }

    fn fail(&mut self, code: pb::protocol_error::Code, detail: String) -> bool {
        self.error(0, code, detail)
    }

    fn complete(
        &mut self,
        status: pb::completion::Status,
        reason: &str,
        units_reported: u64,
    ) -> bool {
        self.send(pb::HostEnvelope {
            correlation_id: 0,
            payload: Some(pb::host_envelope::Payload::Completion(pb::Completion {
                status: status as i32,
                reason: reason.to_string(),
                units_reported,
            })),
        })
    }

    /// Returns false when the session must terminate.
    fn dispatch(&mut self, envelope: pb::HostEnvelope) -> bool {
        let correlation = envelope.correlation_id;
        match envelope.payload {
            Some(pb::host_envelope::Payload::Hello(hello)) => self.hello(correlation, hello),
            Some(pb::host_envelope::Payload::Describe(_)) => {
                self.send(pb::HostEnvelope {
                    correlation_id: correlation,
                    payload: Some(pb::host_envelope::Payload::DescribeResponse(
                        pb::DescribeResponse {
                            capabilities: Some(capabilities_for_runtime_backend()),
                        },
                    )),
                })
            }
            Some(pb::host_envelope::Payload::Plan(plan)) => {
                self.send(pb::HostEnvelope {
                    correlation_id: correlation,
                    payload: Some(pb::host_envelope::Payload::PlanResponse(pb::PlanResponse {
                        planned: plan
                            .units
                            .iter()
                            .map(|u| pb::PlannedUnit {
                                unit_id: u.unit_id.clone(),
                                batch_id: "batch-0".to_string(),
                            })
                            .collect(),
                        unsupported: vec![],
                    })),
                })
            }
            Some(pb::host_envelope::Payload::Execute(execute)) => {
                self.execute(correlation, execute.unit_ids)
            }
            Some(pb::host_envelope::Payload::Cancel(_)) => {
                // Cooperative cancel outside an execute: acknowledge idempotently.
                self.send(pb::HostEnvelope {
                    correlation_id: correlation,
                    payload: Some(pb::host_envelope::Payload::CancelAck(pb::CancelAck {
                        accepted: true,
                    })),
                })
            }
            Some(pb::host_envelope::Payload::Error(e)) => {
                eprintln!(
                    "{{\"level\":\"error\",\"msg\":\"runtime protocol error\",\"code\":{}}}",
                    e.code
                );
                false
            }
            Some(other) => self.fail(
                pb::protocol_error::Code::Uncorrelatable,
                format!("unexpected request frame: {other:?}"),
            ),
            None => self.fail(
                pb::protocol_error::Code::MalformedFrame,
                "envelope without payload".into(),
            ),
        }
    }

    fn hello(&mut self, correlation: u64, hello: pb::Hello) -> bool {
        if hello.protocol_major != self.supported_major {
            return self.error(
                correlation,
                pb::protocol_error::Code::ProtocolMajorMismatch,
                format!(
                    "runtime speaks major {}, host supports major {}",
                    hello.protocol_major, self.supported_major
                ),
            );
        }
        let facts = &capabilities_for_runtime_backend().facts;
        let missing: Vec<String> = hello
            .required_capabilities
            .iter()
            .filter(|c| !facts.contains_key(*c))
            .cloned()
            .collect();
        if !missing.is_empty() {
            return self.error(
                correlation,
                pb::protocol_error::Code::UnsupportedCapability,
                format!("missing required capabilities: {missing:?}"),
            );
        }
        // Correlation-failure probe: wrong id on the handshake answer.
        let ack_correlation = if self.bad_correlation { 999 } else { correlation };
        self.send(pb::HostEnvelope {
            correlation_id: ack_correlation,
            payload: Some(pb::host_envelope::Payload::HelloAck(pb::HelloAck {
                protocol_major: hello.protocol_major,
                protocol_minor: 1,
                host_name: "erf-fake-host".into(),
                host_version: env!("CARGO_PKG_VERSION").into(),
                capabilities: Some(capabilities_for_runtime_backend()),
                accepted_limits: hello.limits,
            })),
        })
    }

    fn execute(&mut self, correlation: u64, unit_ids: Vec<String>) -> bool {
        if self.crash_on_execute {
            eprintln!("[fake-host] injected crash during execute");
            exit(17);
        }
        self.cancelled = false;
        let mut units_reported: u64 = 0;
        for unit_id in &unit_ids {
            if self.fail_unit.as_deref() == Some(unit_id.as_str()) {
                if !self.unit_status(
                    correlation,
                    unit_id,
                    pb::UnitOutcome::Failed,
                    pb::UnitFailureReason::UnitInternalError,
                    "injected unit failure",
                ) {
                    return false;
                }
                units_reported += 1;
                self.units_reported_total += 1;
                continue;
            }
            if self.delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(self.delay_ms));
                if self.drain_cancel() {
                    return self.finish_cancelled(correlation, unit_id, units_reported);
                }
            }
            if !self.observation(correlation, unit_id) {
                return false;
            }
            if !self.unit_status(
                correlation,
                unit_id,
                pb::UnitOutcome::Executed,
                pb::UnitFailureReason::Unspecified,
                "",
            ) {
                return false;
            }
            units_reported += 1;
            self.units_reported_total += 1;
        }
        self.send(pb::HostEnvelope {
            correlation_id: correlation,
            payload: Some(pb::host_envelope::Payload::ExecuteComplete(
                pb::ExecuteComplete { units_reported },
            )),
        })
    }

    fn observation(&mut self, correlation: u64, unit_id: &str) -> bool {
        self.send(pb::HostEnvelope {
            correlation_id: correlation,
            payload: Some(pb::host_envelope::Payload::ExecuteEvent(pb::ExecuteEvent {
                event: Some(pb::execute_event::Event::Observation(
                    erf_fake_host::canned_observation(unit_id),
                )),
            })),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn unit_status(
        &mut self,
        correlation: u64,
        unit_id: &str,
        outcome: pb::UnitOutcome,
        reason_code: pb::UnitFailureReason,
        detail: &str,
    ) -> bool {
        self.send(pb::HostEnvelope {
            correlation_id: correlation,
            payload: Some(pb::host_envelope::Payload::ExecuteEvent(pb::ExecuteEvent {
                event: Some(pb::execute_event::Event::UnitStatus(pb::UnitStatus {
                    unit_id: unit_id.to_string(),
                    outcome: outcome as i32,
                    reason_code: reason_code as i32,
                    detail: detail.into(),
                })),
            })),
        })
    }

    /// Non-blocking check of the inbound channel for a pending CancelRequest.
    fn drain_cancel(&mut self) -> bool {
        if self.cancelled {
            return true;
        }
        loop {
            match self.rx.try_recv() {
                Ok(Inbound::Frame(bytes)) => {
                    if let Ok(envelope) = pb::HostEnvelope::decode(bytes.as_slice()) {
                        if matches!(
                            envelope.payload,
                            Some(pb::host_envelope::Payload::Cancel(_))
                        ) {
                            self.cancelled = true;
                            let ack = pb::HostEnvelope {
                                correlation_id: envelope.correlation_id,
                                payload: Some(pb::host_envelope::Payload::CancelAck(
                                    pb::CancelAck { accepted: true },
                                )),
                            };
                            let _ = self.send(ack);
                            return true;
                        }
                        // The contract forbids interleaved requests during
                        // execute; anything else here is a protocol error.
                        let _ = self.fail(
                            pb::protocol_error::Code::Uncorrelatable,
                            "unexpected request during execute".into(),
                        );
                        return false;
                    }
                }
                Ok(Inbound::FramingError(error)) => {
                    let code = match &error {
                        FrameError::Oversized { .. } => pb::protocol_error::Code::OversizedFrame,
                        FrameError::UnexpectedEof { .. }
                        | FrameError::MalformedProtobuf(_)
                        | FrameError::Eof => pb::protocol_error::Code::MalformedFrame,
                        FrameError::Io(_) => pb::protocol_error::Code::Internal,
                    };
                    let _ = self.fail(code, error.to_string());
                    return false;
                }
                Ok(Inbound::Eof) | Err(TryRecvError::Disconnected) => return false,
                Err(TryRecvError::Empty) => return false,
            }
        }
    }

    fn finish_cancelled(
        &mut self,
        correlation: u64,
        unit_id: &str,
        units_reported: u64,
    ) -> bool {
        // Cooperative cancellation still accounts the interrupted unit.
        if !self.unit_status(
            correlation,
            unit_id,
            pb::UnitOutcome::Failed,
            pb::UnitFailureReason::CancelledByRuntime,
            "cancelled before completion",
        ) {
            return false;
        }
        self.complete(
            pb::completion::Status::Cancelled,
            "cancelled by runtime",
            units_reported + 1,
        )
    }
}
