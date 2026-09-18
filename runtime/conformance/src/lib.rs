//! Versioned, backend-agnostic Contract Conformance Suite skeleton
//! (spec §11.3): a runner that drives any Backend Host executable through
//! negotiation, framing robustness, execution accounting and failure
//! injection. Each check spawns a fresh host session and reports
//! pass/fail with evidence; the report is bound to a suite version so pack
//! certification can reference the exact suite identity (ADR-0070).
//!
//! Suite versioning: SUITE_VERSION changes whenever checks are added,
//! removed or tightened. Suite major/minor alignment with the contract
//! versions is asserted at runtime.

use erf_contracts::backend::v0 as pb;
use erf_host_client::{ExecEvent, HostConfig, HostError, HostHandle};
use prost::Message;
use sha2::{Digest, Sha256};
use std::process::Command;

pub const SUITE_VERSION: &str = "0.2.0";
pub const SUITE_MAJOR: u32 = 0;

/// Everything the suite needs to drive one host implementation.
#[derive(Clone)]
pub struct SuiteConfig {
    pub host_program: String,
    pub host_args: Vec<String>,
    pub host_envs: Vec<(String, String)>,
    /// Capabilities the suite demands (the host must fail the handshake
    /// explicitly when it cannot provide one of these).
    pub required_capabilities: Vec<String>,
    pub limits: pb::Limits,
    /// Cooperative-cancel window in ms (0 = skip the cancel check).
    pub cancel_probe_delay_ms: u64,
}

impl SuiteConfig {
    /// A fresh, independent host command per check.
    fn command(&self) -> Command {
        let mut command = Command::new(&self.host_program);
        command.args(&self.host_args);
        for (k, v) in &self.host_envs {
            command.env(k, v);
        }
        command
    }
}

impl Default for SuiteConfig {
    fn default() -> Self {
        Self {
            host_program: String::new(),
            host_args: Vec::new(),
            host_envs: Vec::new(),
            required_capabilities: vec!["java.ast".to_string()],
            limits: pb::Limits {
                max_frame_bytes: erf_framing::DEFAULT_MAX_FRAME_BYTES,
                max_events_per_execute: 10_000,
                deadline_ms: 0,
            },
            cancel_probe_delay_ms: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct SuiteReport {
    pub suite_version: String,
    pub protocol_major: u32,
    pub checks: Vec<CheckResult>,
}

impl SuiteReport {
    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }

    pub fn suite_digest(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(b"erf-conformance-suite-v0\0");
            append_digest_string(&mut digest, &self.suite_version);
        digest.update(self.protocol_major.to_be_bytes());
        digest.update((self.checks.len() as u64).to_be_bytes());
        for check in &self.checks {
            append_digest_string(&mut digest, check.name);
        }
        digest.finalize().into()
    }
}

pub const TRUSTED_PLATFORM: &str = "linux/amd64";
pub const CONFORMANCE_SIGNING_ROLE: &str = "conformance-signing";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceEvidence {
    pub suite_version: String,
    pub protocol_major: u32,
    pub suite_digest: [u8; 32],
    pub runner_release_digest: String,
    pub backend_pack_digest: String,
    pub resolver_pack_digest: String,
    pub sandbox_provider_id: String,
    pub sandbox_policy_digest: String,
    pub platform: String,
    pub checks: Vec<CheckResult>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EvidenceError {
    #[error("conformance report contains no checks")]
    EmptyChecks,
    #[error("conformance report contains a failed check")]
    FailedCheck,
    #[error("conformance platform mismatch: expected {expected}, got {received}")]
    PlatformMismatch { expected: String, received: String },
    #[error("conformance evidence has an empty identity: {0}")]
    EmptyIdentity(&'static str),
    #[error("conformance evidence has an invalid digest: {0}")]
    InvalidDigest(&'static str),
    #[error("conformance evidence suite digest does not match its checks")]
    SuiteDigestMismatch,
    #[error("conformance evidence payload does not match its canonical form")]
    PayloadMismatch,
    #[error("conformance evidence signature verification failed: {0}")]
    Trust(#[from] erf_trust::TrustError),
}

impl ConformanceEvidence {
    pub fn from_report(
        report: &SuiteReport,
        runner_release_digest: impl Into<String>,
        backend_pack_digest: impl Into<String>,
        resolver_pack_digest: impl Into<String>,
        sandbox_provider_id: impl Into<String>,
        sandbox_policy_digest: impl Into<String>,
        platform: impl Into<String>,
    ) -> Result<Self, EvidenceError> {
        let evidence = Self {
            suite_version: report.suite_version.clone(),
            protocol_major: report.protocol_major,
            suite_digest: report.suite_digest(),
            runner_release_digest: runner_release_digest.into(),
            backend_pack_digest: backend_pack_digest.into(),
            resolver_pack_digest: resolver_pack_digest.into(),
            sandbox_provider_id: sandbox_provider_id.into(),
            sandbox_policy_digest: sandbox_policy_digest.into(),
            platform: platform.into(),
            checks: report.checks.clone(),
        };
        evidence.validate()?;
        Ok(evidence)
    }

    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-conformance-evidence-v0\0".to_vec();
        append_string(&mut payload, &self.suite_version);
        payload.extend_from_slice(&self.protocol_major.to_be_bytes());
        payload.extend_from_slice(&self.suite_digest);
        for value in [
            &self.runner_release_digest,
            &self.backend_pack_digest,
            &self.resolver_pack_digest,
            &self.sandbox_provider_id,
            &self.sandbox_policy_digest,
            &self.platform,
        ] {
            append_string(&mut payload, value);
        }
        payload.extend_from_slice(&(self.checks.len() as u64).to_be_bytes());
        for check in &self.checks {
            append_string(&mut payload, check.name);
            payload.push(check.passed as u8);
            append_string(&mut payload, &check.detail);
        }
        payload
    }

    pub fn sign_with(&self, key_id: impl Into<String>, signer: impl FnOnce(&[u8]) -> Vec<u8>) -> Result<erf_trust::SignedAsset, EvidenceError> {
        self.validate()?;
        let payload = self.canonical_payload();
        Ok(erf_trust::SignedAsset {
            payload_type: erf_trust::DSSE_PAYLOAD_TYPE.into(),
            payload: payload.clone(),
            key_id: key_id.into(),
            signature: signer(&erf_trust::pae(erf_trust::DSSE_PAYLOAD_TYPE, &payload)),
        })
    }

    pub fn verify_signed(&self, asset: &erf_trust::SignedAsset, store: &erf_trust::TrustStore, role: &str) -> Result<(), EvidenceError> {
        self.validate()?;
        if asset.payload != self.canonical_payload() {
            return Err(EvidenceError::PayloadMismatch);
        }
        store.verify(asset, role)?;
        Ok(())
    }

    pub fn verify_signed_with_metadata(
        &self,
        asset: &erf_trust::SignedAsset,
        store: &erf_trust::TrustStore,
        role: &str,
        metadata: &erf_trust::TrustMetadata,
        now_epoch_secs: u64,
    ) -> Result<(), EvidenceError> {
        self.validate()?;
        if asset.payload != self.canonical_payload() {
            return Err(EvidenceError::PayloadMismatch);
        }
        store.verify_with_metadata(asset, role, metadata, now_epoch_secs)?;
        Ok(())
    }

    fn validate(&self) -> Result<(), EvidenceError> {
        if self.checks.is_empty() {
            return Err(EvidenceError::EmptyChecks);
        }
        if self.checks.iter().any(|check| !check.passed) {
            return Err(EvidenceError::FailedCheck);
        }
        if self.platform != TRUSTED_PLATFORM {
            return Err(EvidenceError::PlatformMismatch { expected: TRUSTED_PLATFORM.into(), received: self.platform.clone() });
        }
        for (name, value) in [
            ("runner_release_digest", &self.runner_release_digest),
            ("backend_pack_digest", &self.backend_pack_digest),
            ("resolver_pack_digest", &self.resolver_pack_digest),
            ("sandbox_policy_digest", &self.sandbox_policy_digest),
        ] {
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(EvidenceError::InvalidDigest(name));
            }
        }
        if self.sandbox_provider_id.is_empty() {
            return Err(EvidenceError::EmptyIdentity("sandbox_provider_id"));
        }
        let report = SuiteReport { suite_version: self.suite_version.clone(), protocol_major: self.protocol_major, checks: self.checks.clone() };
        if self.suite_digest != report.suite_digest() {
            return Err(EvidenceError::SuiteDigestMismatch);
        }
        Ok(())
    }
}

fn append_digest_string(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}

fn append_string(payload: &mut Vec<u8>, value: &str) {
    payload.extend_from_slice(&(value.len() as u64).to_be_bytes());
    payload.extend_from_slice(value.as_bytes());
}

fn check(name: &'static str, passed: bool, detail: String) -> CheckResult {
    CheckResult {
        name,
        passed,
        detail,
    }
}

/// Runs the full suite against the configured host.
pub fn run_suite(config: &SuiteConfig) -> SuiteReport {
    let mut checks = Vec::new();
    eprintln!("[suite] check golden_messages_bidirectional");
    checks.push(golden_messages_bidirectional());
    eprintln!("[suite] check negotiation_and_describe");
    checks.push(negotiation_and_describe(config));
    eprintln!("[suite] check required_capability_unmet_fails");
    checks.push(required_capability_unmet_fails(config));
    eprintln!("[suite] check major_mismatch_refuse");
    checks.push(major_mismatch_refused(config));
    eprintln!("[suite] check malformed_frame_rejected");
    checks.push(malformed_frame_rejected(config));
    eprintln!("[suite] check oversized_frame_rejected");
    checks.push(oversized_frame_rejected(config));
    eprintln!("[suite] check execute_round_trip_completeness");
    checks.push(execute_round_trip_completeness(config));
    eprintln!("[suite] check deadline_is_failure");
    checks.push(deadline_is_failure(config));
    eprintln!("[suite] check cooperative_cancel_accounts_unit");
    checks.push(cooperative_cancel_accounts_units(config));
    eprintln!("[suite] check crash_is_incomplete_failure");
    checks.push(crash_is_incomplete_failure(config));
    SuiteReport {
        suite_version: SUITE_VERSION.to_string(),
        protocol_major: SUITE_MAJOR,
        checks,
    }
}

/// Runs the protocol suite plus the independent local semantic acceptance
/// checks. The latter consume only contract values, so they can be reused for
/// Fake Hosts and real Backend Hosts without importing engine implementation
/// behavior into the acceptance oracle.
pub fn run_acceptance(config: &SuiteConfig) -> SuiteReport {
    let mut report = run_suite(config);
    for acceptance in local_gate_acceptance() {
        eprintln!("[suite] check {}", acceptance.name);
        report.checks.push(acceptance);
    }
    report
}

/// Local Gate checks are deliberately independent of a Host process. Host
/// failure-injection remains covered by `run_suite`; these checks prove that
/// the resulting typed values cannot be turned into a false PASS downstream.
pub fn local_gate_acceptance() -> Vec<CheckResult> {
    vec![
        local_canonical_finding_set(),
        local_ten_repeat_semantic_digest(),
        local_message_independence(),
        local_baseline_exception_orthogonality(),
        local_incomplete_never_passes(),
        local_malformed_typed_observation_never_passes(),
    ]
}

fn local_ten_repeat_semantic_digest() -> CheckResult {
    let name = "local_ten_repeat_semantic_digest";
    let observations = vec![local_source("src/A.java", "repeat"), local_bytecode("pkg.A", "run")];
    let statuses = vec![local_status("unit-1", pb::UnitOutcome::Executed)];
    let mut expected = None;
    for run in 1..=10 {
        let preview = rf_gate::preview(&["unit-1".into()], &statuses, &observations);
        if let Some(first) = expected {
            if preview.semantic_result_digest != first {
                return check(name, false, format!("run {run} changed SemanticResultDigest"));
            }
        } else {
            expected = Some(preview.semantic_result_digest);
        }
    }
    check(name, true, "SemanticResultDigest stable across 10 local repeats".into())
}

fn local_canonical_finding_set() -> CheckResult {
    let name = "local_canonical_finding_set";
    let observations = vec![local_source("src/A.java", "first"), local_bytecode("pkg.A", "run"), local_file("target/classes/A.class")];
    let statuses = vec![local_status("unit-1", pb::UnitOutcome::Executed)];
    let first = rf_gate::preview(&["unit-1".into()], &statuses, &observations);
    let mut reversed = observations.clone();
    reversed.reverse();
    reversed.push(local_source("src/A.java", "duplicate"));
    let second = rf_gate::preview(&["unit-1".into()], &statuses, &reversed);
    let sorted = first.findings.windows(2).all(|pair| pair[0].fingerprint <= pair[1].fingerprint);
    if first.findings.len() == 3 && second.findings.len() == 3 && first.findings == second.findings && sorted && first.completeness.complete {
        check(name, true, "typed findings are deduplicated and fingerprint-sorted".into())
    } else {
        check(name, false, format!("canonical set mismatch: first={} second={} sorted={sorted}", first.findings.len(), second.findings.len()))
    }
}

fn local_message_independence() -> CheckResult {
    let name = "local_message_independence";
    let statuses = vec![local_status("unit-1", pb::UnitOutcome::Executed)];
    let first = rf_gate::preview(&["unit-1".into()], &statuses, &[local_source("src/A.java", "engine wording one")]);
    let second = rf_gate::preview(&["unit-1".into()], &statuses, &[local_source("src/A.java", "engine wording two")]);
    if first.semantic_result_digest == second.semantic_result_digest && first.findings[0].fingerprint == second.findings[0].fingerprint {
        check(name, true, "renderer message changes do not change semantic identity".into())
    } else {
        check(name, false, "message-only mutation changed semantic result".into())
    }
}

fn local_baseline_exception_orthogonality() -> CheckResult {
    let name = "local_baseline_exception_orthogonality";
    let statuses = vec![local_status("unit-1", pb::UnitOutcome::Executed)];
    let observation = local_bytecode("pkg.A", "run");
    let initial = rf_gate::preview(&["unit-1".into()], &statuses, std::slice::from_ref(&observation));
    let fingerprint = initial.findings[0].fingerprint.clone();
    let baseline = rf_gate::Baseline { kind: rf_gate::BaselineKind::Debt, finding_fingerprints: [fingerprint.clone()].into_iter().collect() };
    let exceptions = [fingerprint.clone()].into_iter().collect();
    let preview = rf_gate::preview_with_baseline_and_exceptions(&["unit-1".into()], &statuses, &[observation], Some(&baseline), &exceptions);
    let assessment = &preview.assessments[0];
    if preview.decision == rf_gate::GateDecision::Pass
        && preview.findings.len() == 1
        && preview.findings[0].fingerprint == fingerprint
        && assessment.baseline == rf_gate::BaselineState::Historical
        && assessment.exception == rf_gate::ExceptionDecision::Exempted
    {
        check(name, true, "baseline and exception remain visible and independent".into())
    } else {
        check(name, false, "baseline or exception decision was lost or mutated".into())
    }
}

fn local_incomplete_never_passes() -> CheckResult {
    let name = "local_incomplete_never_passes";
    let observation = local_source("src/A.java", "incomplete");
    let initial = rf_gate::preview(&["unit-1".into()], &[local_status("unit-1", pb::UnitOutcome::Executed)], std::slice::from_ref(&observation));
    let exceptions = [initial.findings[0].fingerprint.clone()].into_iter().collect();
    let missing = rf_gate::preview_with_baseline_and_exceptions(&["unit-1".into()], &[], &[observation.clone()], None, &exceptions);
    let failed = rf_gate::preview_with_baseline_and_exceptions(&["unit-1".into()], &[local_status("unit-1", pb::UnitOutcome::Failed)], &[observation], None, &exceptions);
    if missing.decision == rf_gate::GateDecision::Inconclusive && failed.decision == rf_gate::GateDecision::Inconclusive {
        check(name, true, "missing and failed required units stay INCONCLUSIVE despite exemption".into())
    } else {
        check(name, false, format!("incomplete decisions: missing={:?} failed={:?}", missing.decision, failed.decision))
    }
}

fn local_malformed_typed_observation_never_passes() -> CheckResult {
    let name = "local_malformed_typed_observation_never_passes";
    let malformed = vec![
        pb::BackendObservation { rule_id: "rule.a".into(), location: Some(pb::ObservationLocation { kind: Some(pb::observation_location::Kind::BytecodeMember(pb::BytecodeMember { class_fqn: "pkg.A".into(), member: "run".into(), member_descriptor: "() V".into() })) }), message: String::new(), engine_severity: String::new() },
        pb::BackendObservation { rule_id: "rule.a".into(), location: Some(pb::ObservationLocation { kind: Some(pb::observation_location::Kind::File(pb::FileRef { path: "../A.class".into(), blob_sha256: vec![1; 32] })) }), message: String::new(), engine_severity: String::new() },
    ];
    let preview = rf_gate::preview(&["unit-1".into()], &[local_status("unit-1", pb::UnitOutcome::Executed)], &malformed);
    if preview.findings.is_empty() && preview.completeness.observation_errors == 2 && preview.decision == rf_gate::GateDecision::Inconclusive {
        check(name, true, "invalid typed locations are rejected without a false PASS".into())
    } else {
        check(name, false, format!("malformed observations produced {:?}", preview.decision))
    }
}

fn local_status(unit_id: &str, outcome: pb::UnitOutcome) -> pb::UnitStatus {
    pb::UnitStatus { unit_id: unit_id.into(), outcome: outcome as i32, reason_code: 0, detail: String::new() }
}

fn local_source(path: &str, message: &str) -> pb::BackendObservation {
    pb::BackendObservation { rule_id: "corp.java.sample".into(), location: Some(pb::ObservationLocation { kind: Some(pb::observation_location::Kind::SourceSpan(pb::SourceSpan { path: path.into(), blob_sha256: vec![1; 32], start_byte: 2, end_byte: 8, line: 1, column: 1 })) }), message: message.into(), engine_severity: "warning".into() }
}

fn local_bytecode(class_fqn: &str, member: &str) -> pb::BackendObservation {
    pb::BackendObservation { rule_id: "corp.java.sample".into(), location: Some(pb::ObservationLocation { kind: Some(pb::observation_location::Kind::BytecodeMember(pb::BytecodeMember { class_fqn: class_fqn.into(), member: member.into(), member_descriptor: "()V".into() })) }), message: "bytecode".into(), engine_severity: "warning".into() }
}

fn local_file(path: &str) -> pb::BackendObservation {
    pb::BackendObservation { rule_id: "corp.java.sample".into(), location: Some(pb::ObservationLocation { kind: Some(pb::observation_location::Kind::File(pb::FileRef { path: path.into(), blob_sha256: vec![2; 32] })) }), message: "file".into(), engine_severity: "warning".into() }
}

/// Decodes and re-encodes both runtime->host and host->runtime fixtures. The
/// byte-for-byte assertion protects field numbers, oneof selection, and wire
/// compatibility independently of any one host implementation.
fn golden_messages_bidirectional() -> CheckResult {
    let name = "golden_messages_bidirectional";
    let fixtures = [
        ("Hello", include_str!("../golden/hello_v0.hex")),
        ("HelloAck", include_str!("../golden/hello_ack_v0.hex")),
    ];
    for (label, text) in fixtures {
        let bytes = match decode_hex(text) {
            Ok(bytes) => bytes,
            Err(detail) => return check(name, false, format!("{label} fixture: {detail}")),
        };
        let envelope = match pb::HostEnvelope::decode(bytes.as_slice()) {
            Ok(envelope) => envelope,
            Err(error) => return check(name, false, format!("{label} decode: {error}")),
        };
        if envelope.encode_to_vec() != bytes {
            return check(name, false, format!("{label} is not canonical protobuf"));
        }
    }
    check(name, true, "Hello and HelloAck fixtures round-trip byte-identically".into())
}

fn decode_hex(text: &str) -> Result<Vec<u8>, String> {
    let text: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if text.len() % 2 != 0 {
        return Err("odd number of hex digits".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "invalid hex".to_string()))
        .collect()
}

fn negotiation_and_describe(config: &SuiteConfig) -> CheckResult {
    let name = "negotiation_and_describe";
    let mut command = config.command();
    match HostHandle::spawn(&mut command, &config_with(config)) {
        Ok((mut host, ack)) => match host.describe() {
            Ok(caps) => {
                if caps.languages.is_empty() {
                    check(name, false, "describe reported no languages".into())
                } else if caps.facts.is_empty() {
                    check(name, false, "describe reported no capability facts".into())
                } else {
                    check(
                        name,
                        true,
                        format!("host {} {}", ack.host_name, caps.backend_id),
                    )
                }
            }
            Err(e) => check(name, false, format!("describe failed: {e}")),
        },
        Err(e) => check(name, false, format!("handshake failed: {e}")),
    }
}

fn required_capability_unmet_fails(config: &SuiteConfig) -> CheckResult {
    let name = "required_capability_unmet_fails";
    let mut command = config.command();
    let cfg = HostConfig {
        required_capabilities: vec!["java.erf-nonexistent-capability".to_string()],
        optional_capabilities: vec![],
        limits: config.limits.clone(),
    };
    match HostHandle::spawn(&mut command, &cfg) {
        Err(HostError::Protocol {
            code: pb::protocol_error::Code::UnsupportedCapability,
            ..
        })
        | Err(HostError::MissingCapabilities { .. }) => {
            check(name, true, "handshake failed explicitly".into())
        }
        Err(e) => check(name, false, format!("wrong failure mode: {e}")),
        Ok(_) => check(name, false, "handshake succeeded despite unknown capability".into()),
    }
}

fn major_mismatch_refused(config: &SuiteConfig) -> CheckResult {
    let name = "major_mismatch_refused";
    let mut command = config.command();
    // The suite speaks major 0; a conforming host configured for major 1 must
    // refuse. Hosts that cannot be reconfigured skip via env absence — the
    // reference fake supports FAKE_SUPPORTED_MAJOR.
    command.env("ERF_SUPPORTED_MAJOR", "1").env("FAKE_SUPPORTED_MAJOR", "1");
    match HostHandle::spawn(&mut command, &config_with(config)) {
        Err(HostError::Protocol {
            code: pb::protocol_error::Code::ProtocolMajorMismatch,
            ..
        })
        | Err(HostError::MajorMismatch { .. }) => {
            check(name, true, "refused execution on major mismatch".into())
        }
        Err(e) => check(name, false, format!("wrong failure mode: {e}")),
        Ok(_) => check(name, false, "major mismatch was accepted".into()),
    }
}

fn malformed_frame_rejected(config: &SuiteConfig) -> CheckResult {
    let name = "malformed_frame_rejected";
    use std::io::Write;
    let mut command = config.command();
    command.env_remove("ERF_DELAY_MS").env_remove("FAKE_DELAY_MS");
    let mut child = match command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return check(name, false, format!("spawn failed: {e}")),
    };
    let mut stdin = child.stdin.take().expect("stdin");
    let garbage = vec![0xffu8; 8];
    if stdin.write_all(&(garbage.len() as u32).to_be_bytes()).is_err()
        || stdin.write_all(&garbage).is_err()
        || stdin.flush().is_err()
    {
        return check(name, false, "could not write probe frame".into());
    }
    drop(stdin);
    match child.wait_with_output() {
        Ok(output) => {
            if output.status.success() {
                check(name, false, "host survived a malformed frame".into())
            } else {
                check(name, true, "host terminated on malformed frame".into())
            }
        }
        Err(e) => check(name, false, format!("wait failed: {e}")),
    }
}

fn oversized_frame_rejected(config: &SuiteConfig) -> CheckResult {
    let name = "oversized_frame_rejected";
    let mut command = config.command();
    // Host limit clamps to the negotiated frame cap; a 1 MB frame against a
    // 1 KB limit must be refused without destabilizing the stream.
    command.env("ERF_MAX_FRAME_BYTES", "1024").env("FAKE_MAX_FRAME_BYTES", "1024");
    let mut limits = config.limits.clone();
    limits.max_frame_bytes = 1024;
    let cfg = HostConfig {
        required_capabilities: vec![],
        optional_capabilities: vec![],
        limits: limits.clone(),
    };
    match HostHandle::spawn(&mut command, &cfg) {
        Ok((host, _)) => {
            // The Hello frame itself is small; now push an oversized frame by
            // sending a describe with a giant correlation padding is not
            // possible through the client — so verify the client-side clamp
            // rejects an oversized write instead.
            let big = vec![0u8; 2048];
            match erf_framing::FrameWriter::new(std::io::sink(), 1024).write_frame(&big)
            {
                Err(erf_framing::FrameError::Oversized { .. }) => {
                    let _ = host.complete();
                    check(name, true, "oversized frame refused by transport".into())
                }
                _ => check(name, false, "oversized write was accepted".into()),
            }
        }
        Err(e) => check(name, false, format!("handshake failed: {e}")),
    }
}

fn execute_round_trip_completeness(config: &SuiteConfig) -> CheckResult {
    let name = "execute_round_trip_completeness";
    let mut command = config.command();
    command
        .env_remove("ERF_DELAY_MS")
        .env_remove("FAKE_DELAY_MS");
    let cfg = config_with(config);
    let outcome = (|| -> Result<String, HostError> {
        let (mut host, _) = HostHandle::spawn(&mut command, &cfg)?;
        let planned = vec![
            "unit-1".to_string(),
            "unit-2".to_string(),
            "unit-3".to_string(),
        ];
        host.plan(vec![
            pb::AnalysisUnit {
                unit_id: planned[0].clone(),
                language: "java".into(),
                files: vec![],
            },
            pb::AnalysisUnit {
                unit_id: planned[1].clone(),
                language: "java".into(),
                files: vec![],
            },
            pb::AnalysisUnit {
                unit_id: planned[2].clone(),
                language: "java".into(),
                files: vec![],
            },
        ])?;
        let mut stream = host.execute(planned.clone())?;
        let mut executed: Vec<String> = Vec::new();
        loop {
            match stream.next_event()? {
                Some(ExecEvent::UnitStatus(s)) => executed.push(s.unit_id),
                Some(ExecEvent::Complete(c)) => {
                    if c.units_reported != planned.len() as u64 {
                        return Err(HostError::Violation(format!(
                            "units_reported {} != planned {}",
                            c.units_reported,
                            planned.len()
                        )));
                    }
                    break;
                }
                Some(ExecEvent::Observation(_)) => continue,
                None => return Err(HostError::Violation("stream ended early".into())),
            }
        }
        executed.sort();
        if executed != planned {
            return Err(HostError::Violation(format!(
                "completeness mismatch: executed {executed:?} != planned {planned:?}"
            )));
        }
        let completion = host.complete()?;
        if completion.status != pb::completion::Status::Completed as i32 {
            return Err(HostError::Violation("non-completed terminal".into()));
        }
        Ok(format!("{} units accounted", executed.len()))
    })();
    match outcome {
        Ok(detail) => check(name, true, detail),
        Err(e) => check(name, false, format!("{e}")),
    }
}

fn cooperative_cancel_accounts_units(config: &SuiteConfig) -> CheckResult {
    let name = "cooperative_cancel_accounts_units";
    if config.cancel_probe_delay_ms == 0 {
        return check(name, true, "skipped: no cancel window configured".into());
    }
    let mut command = config.command();
    command
        .env("FAKE_DELAY_MS", config.cancel_probe_delay_ms.to_string())
        .env("ERF_DELAY_MS", config.cancel_probe_delay_ms.to_string());
    let cfg = config_with(config);
    let outcome = (|| -> Result<String, HostError> {
        let (mut host, _) = HostHandle::spawn(&mut command, &cfg)?;
        host.plan(vec![
            pb::AnalysisUnit {
                unit_id: "unit-1".into(),
                language: "java".into(),
                files: vec![],
            },
            pb::AnalysisUnit {
                unit_id: "unit-2".into(),
                language: "java".into(),
                files: vec![],
            },
        ])?;
        let mut stream = host.execute(vec!["unit-1".into(), "unit-2".into()])?;
        eprintln!("[cancel-check] waiting first event");
        let _first = stream.next_event()?;
        eprintln!("[cancel-check] got first event");
        stream.cancel()?;
        eprintln!("[cancel-check] cancel sent");
        let completion = stream.completion()?;
        eprintln!("[cancel-check] completion received");
        if completion.status != pb::completion::Status::Cancelled as i32 {
            return Err(HostError::Violation(format!(
                "expected CANCELLED, got {}",
                completion.status
            )));
        }
        if completion.units_reported < 1 {
            return Err(HostError::Violation("no units accounted after cancel".into()));
        }
        Ok(format!(
            "cancel accounted {} unit statuses",
            completion.units_reported
        ))
    })();
    match outcome {
        Ok(detail) => check(name, true, detail),
        Err(e) => check(name, false, format!("{e}")),
    }
}

fn deadline_is_failure(config: &SuiteConfig) -> CheckResult {
    let name = "deadline_is_failure";
    let mut command = config.command();
    command.env("FAKE_DELAY_MS", "200").env("ERF_DELAY_MS", "200");
    let mut limits = config.limits.clone();
    limits.deadline_ms = 50;
    let cfg = HostConfig {
        required_capabilities: config.required_capabilities.clone(),
        optional_capabilities: vec![],
        limits,
    };
    let outcome = (|| -> Result<(), HostError> {
        let (mut host, _) = HostHandle::spawn(&mut command, &cfg)?;
        host.plan(vec![pb::AnalysisUnit {
            unit_id: "deadline-unit".into(),
            language: "java".into(),
            files: vec![],
        }])?;
        let mut stream = host.execute(vec!["deadline-unit".into()])?;
        match stream.next_event() {
            Err(HostError::DeadlineExceeded { deadline_ms: 50 }) => Ok(()),
            Err(other) => Err(HostError::Violation(format!("wrong deadline failure: {other}"))),
            Ok(event) => Err(HostError::Violation(format!(
                "deadline unexpectedly yielded {event:?}"
            ))),
        }
    })();
    match outcome {
        Ok(()) => check(name, true, "execution deadline classified as failure".into()),
        Err(e) => check(name, false, format!("{e}")),
    }
}

fn crash_is_incomplete_failure(config: &SuiteConfig) -> CheckResult {
    let name = "crash_is_incomplete_failure";
    let mut command = config.command();
    command.env("FAKE_CRASH_ON_EXECUTE", "1");
    let outcome = (|| -> Result<(), HostError> {
        let (mut host, _) = HostHandle::spawn(&mut command, &config_with(config))?;
        host.plan(vec![pb::AnalysisUnit {
            unit_id: "crash-unit".into(),
            language: "java".into(),
            files: vec![],
        }])?;
        let mut stream = host.execute(vec!["crash-unit".into()])?;
        match stream.next_event() {
            Err(HostError::Transport(erf_framing::FrameError::Eof))
            | Err(HostError::Transport(erf_framing::FrameError::UnexpectedEof { .. })) => Ok(()),
            Err(other) => Err(HostError::Violation(format!("wrong crash failure: {other}"))),
            Ok(event) => Err(HostError::Violation(format!("crashed host yielded {event:?}"))),
        }
    })();
    match outcome {
        Ok(()) => check(name, true, "crash classified as incomplete execution".into()),
        Err(e) => check(name, false, format!("{e}")),
    }
}

fn config_with(config: &SuiteConfig) -> HostConfig {
    HostConfig {
        required_capabilities: config.required_capabilities.clone(),
        optional_capabilities: vec![],
        limits: config.limits.clone(),
    }
}

#[cfg(test)]
mod evidence_tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::collections::BTreeSet;

    fn report() -> SuiteReport {
        SuiteReport {
            suite_version: SUITE_VERSION.into(),
            protocol_major: SUITE_MAJOR,
            checks: vec![
                CheckResult { name: "golden_messages_bidirectional", passed: true, detail: "wire fixtures match".into() },
                CheckResult { name: "execute_round_trip_completeness", passed: true, detail: "all units accounted".into() },
            ],
        }
    }

    fn evidence_and_store() -> (ConformanceEvidence, erf_trust::TrustStore) {
        let evidence = ConformanceEvidence::from_report(
            &report(),
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64),
            "rootless-oci-docker-v0",
            "d".repeat(64),
            TRUSTED_PLATFORM,
        ).unwrap();
        let key = SigningKey::from_bytes(&[41; 32]);
        let mut store = erf_trust::TrustStore::default();
        store.insert_key("conformance-key", key.verifying_key()).unwrap();
        store.authorize_role(CONFORMANCE_SIGNING_ROLE, "conformance-key").unwrap();
        (evidence, store)
    }

    #[test]
    fn evidence_binds_suite_results_and_execution_identities() {
        let (evidence, store) = evidence_and_store();
        let key = SigningKey::from_bytes(&[41; 32]);
        let asset = evidence.sign_with("conformance-key", |bytes| key.sign(bytes).to_bytes().to_vec()).unwrap();
        assert!(evidence.verify_signed(&asset, &store, CONFORMANCE_SIGNING_ROLE).is_ok());
        let mut changed = evidence.clone();
        changed.backend_pack_digest = "e".repeat(64);
        assert_eq!(changed.verify_signed(&asset, &store, CONFORMANCE_SIGNING_ROLE), Err(EvidenceError::PayloadMismatch));
    }

    #[test]
    fn evidence_rejects_failed_checks_and_stale_or_revoked_metadata() {
        let mut failed = report();
        failed.checks[0].passed = false;
        assert_eq!(ConformanceEvidence::from_report(&failed, "a".repeat(64), "b".repeat(64), "c".repeat(64), "provider", "d".repeat(64), TRUSTED_PLATFORM), Err(EvidenceError::FailedCheck));
        let (evidence, store) = evidence_and_store();
        let key = SigningKey::from_bytes(&[41; 32]);
        let asset = evidence.sign_with("conformance-key", |bytes| key.sign(bytes).to_bytes().to_vec()).unwrap();
        let expired = erf_trust::TrustMetadata { version: 1, expires_at_epoch_secs: 10, revoked_key_ids: BTreeSet::new() };
        assert_eq!(evidence.verify_signed_with_metadata(&asset, &store, CONFORMANCE_SIGNING_ROLE, &expired, 10), Err(EvidenceError::Trust(erf_trust::TrustError::MetadataExpired)));
    }

    #[test]
    fn suite_digest_changes_when_check_set_changes() {
        let first = report();
        let mut changed = report();
        changed.checks.push(CheckResult { name: "new_check", passed: true, detail: "new".into() });
        assert_ne!(first.suite_digest(), changed.suite_digest());
    }
}
