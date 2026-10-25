//! Versioned, backend-agnostic Contract Conformance Suite
//! (spec §11.3): a runner that drives any Backend Host executable through
//! negotiation, framing robustness, execution accounting and failure
//! injection. Each check spawns a fresh host session and reports
//! pass/fail with evidence; the report is bound to a suite version so pack
//! certification can reference the exact suite identity (ADR-0070).
//!
//! Suite versioning: SUITE_VERSION changes whenever checks are added,
//! removed or tightened. Suite major/minor alignment with the contract
//! versions is asserted at runtime.

use erf_contracts::{backend::v0 as pb, workspace::v0 as wb};
use erf_framing::{FrameReader, FrameWriter};
use erf_host_client::{ExecEvent, HostConfig, HostError, HostHandle};
use erf_resolver_client::{ResolverConfig, ResolverError, ResolverHandle};
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

pub const SUITE_VERSION: &str = "0.4.1";
pub const SUITE_MAJOR: u32 = 0;

/// The acceptance report is a fixed release artifact. Evidence validation
/// rejects both Host-only reports and reports that selectively omit any other
/// check from this ordered set.
pub const EXPECTED_ACCEPTANCE_CHECKS: &[&str] = &[
    "golden_messages_bidirectional",
    "negotiation_and_describe",
    "required_capability_unmet_fails",
    "major_mismatch_refused",
    "malformed_frame_rejected",
    "oversized_frame_rejected",
    "execute_round_trip_completeness",
    "execute_attempt_accounting",
    "execute_attempt_accounting_rejects_tampering",
    "deadline_is_failure",
    "cooperative_cancel_accounts_units",
    "crash_is_incomplete_failure",
    "resolver_golden_messages_bidirectional",
    "resolver_minor_compatibility",
    "resolver_negotiation_and_resolution",
    "resolver_required_capability_unmet_fails",
    "resolver_major_mismatch_refused",
    "resolver_resolution_deterministic",
    "resolver_malformed_frame_rejected",
    "resolver_unknown_optional_field_ignored",
    "resolver_oversized_frame_rejected",
    "resolver_deadline_is_failure",
    "resolver_cooperative_cancel_accounts_request",
    "resolver_crash_is_incomplete_failure",
    "resolver_retry_after_failure",
    "resolver_safe_static_symlink_rejected",
    "resolver_backpressure_is_bounded",
    "local_canonical_finding_set",
    "local_ten_repeat_semantic_digest",
    "local_message_independence",
    "local_baseline_exception_orthogonality",
    "local_incomplete_never_passes",
    "local_malformed_typed_observation_never_passes",
];

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
    /// Cooperative-cancel delay in ms. Zero is rejected by the fixed suite;
    /// every evidence report must exercise cancellation.
    pub cancel_probe_delay_ms: u64,
    /// Workspace Resolver executable and its independent contract settings.
    /// Resolver checks are always part of the suite; leaving this empty makes
    /// the report fail closed and prevents it from becoming evidence.
    pub resolver_program: String,
    pub resolver_args: Vec<String>,
    pub resolver_envs: Vec<(String, String)>,
    pub resolver_required_capabilities: Vec<String>,
    pub resolver_limits: wb::Limits,
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

    fn resolver_command(&self) -> Command {
        let mut command = Command::new(&self.resolver_program);
        command.args(&self.resolver_args);
        for (k, v) in &self.resolver_envs {
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
            cancel_probe_delay_ms: 150,
            resolver_program: String::new(),
            resolver_args: Vec::new(),
            resolver_envs: Vec::new(),
            resolver_required_capabilities: vec!["ecosystem.static-layout".to_string()],
            resolver_limits: wb::Limits {
                max_frame_bytes: erf_framing::DEFAULT_MAX_FRAME_BYTES,
                deadline_ms: 0,
            },
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
        // The check names are useful evidence labels, but are not sufficient
        // as a suite identity: changing an oracle while retaining its name
        // must invalidate every previously issued ConformanceEvidence. Bind
        // the compiled suite source, every runtime crate used by the suite,
        // the generated-contract build recipe, lockfile and wire schemas.
        digest.update(b"erf-conformance-suite-v2\0");
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../framing/Cargo.toml")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../framing/src/lib.rs")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../host-client/Cargo.toml")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../host-client/src/lib.rs")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../resolver-client/Cargo.toml")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../resolver-client/src/lib.rs")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../gate/Cargo.toml")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../gate/src/lib.rs")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../trust/Cargo.toml")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../trust/src/lib.rs")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../contracts/rust/Cargo.toml")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../contracts/rust/build.rs")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../contracts/rust/src/lib.rs")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../contracts/proto/backend_host.proto")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../contracts/proto/workspace_resolution.proto")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/golden/hello_v0.hex")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/golden/hello_ack_v0.hex")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/golden/workspace_hello_v0.hex")));
        digest.update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/golden/workspace_hello_ack_v0.hex")));
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
pub const VERIFICATION_SIGNING_ROLE: &str = "verification-signing";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceEvidence {
    pub suite_version: String,
    pub protocol_major: u32,
    pub suite_digest: [u8; 32],
    pub suite_release_digest: String,
    pub verification_worker_release_digest: String,
    pub runner_release_digest: String,
    pub backend_pack_digest: String,
    pub resolver_pack_digest: String,
    pub sandbox_provider_id: String,
    pub sandbox_policy_digest: String,
    pub resolver_sandbox_provider_id: String,
    pub resolver_sandbox_policy_digest: String,
    pub platform: String,
    pub checks: Vec<CheckResult>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EvidenceError {
    #[error("conformance suite version mismatch: expected {expected}, got {received}")]
    SuiteVersionMismatch { expected: String, received: String },
    #[error("conformance protocol major mismatch: expected {expected}, got {received}")]
    ProtocolMajorMismatch { expected: u32, received: u32 },
    #[error("conformance report contains no checks")]
    EmptyChecks,
    #[error("conformance report contains a failed check")]
    FailedCheck,
    #[error("conformance evidence does not contain the exact acceptance check set")]
    CheckSetMismatch,
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
    /// Aggregates an already executed report. The independent Verification
    /// Worker remains the authority that decides whether this report may be
    /// signed; this constructor only validates the exact suite contract and
    /// identity bindings before signing or verification.
    pub fn from_report(
        report: &SuiteReport,
        suite_release_digest: impl Into<String>,
        verification_worker_release_digest: impl Into<String>,
        runner_release_digest: impl Into<String>,
        backend_pack_digest: impl Into<String>,
        resolver_pack_digest: impl Into<String>,
        sandbox_provider_id: impl Into<String>,
        sandbox_policy_digest: impl Into<String>,
        resolver_sandbox_provider_id: impl Into<String>,
        resolver_sandbox_policy_digest: impl Into<String>,
        platform: impl Into<String>,
    ) -> Result<Self, EvidenceError> {
        let evidence = Self {
            suite_version: report.suite_version.clone(),
            protocol_major: report.protocol_major,
            suite_digest: report.suite_digest(),
            suite_release_digest: suite_release_digest.into(),
            verification_worker_release_digest: verification_worker_release_digest.into(),
            runner_release_digest: runner_release_digest.into(),
            backend_pack_digest: backend_pack_digest.into(),
            resolver_pack_digest: resolver_pack_digest.into(),
            sandbox_provider_id: sandbox_provider_id.into(),
            sandbox_policy_digest: sandbox_policy_digest.into(),
            resolver_sandbox_provider_id: resolver_sandbox_provider_id.into(),
            resolver_sandbox_policy_digest: resolver_sandbox_policy_digest.into(),
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
            &self.suite_release_digest,
            &self.verification_worker_release_digest,
            &self.runner_release_digest,
            &self.backend_pack_digest,
            &self.resolver_pack_digest,
            &self.sandbox_provider_id,
            &self.sandbox_policy_digest,
            &self.resolver_sandbox_provider_id,
            &self.resolver_sandbox_policy_digest,
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
        if self.suite_version != SUITE_VERSION {
            return Err(EvidenceError::SuiteVersionMismatch {
                expected: SUITE_VERSION.into(),
                received: self.suite_version.clone(),
            });
        }
        if self.protocol_major != SUITE_MAJOR {
            return Err(EvidenceError::ProtocolMajorMismatch {
                expected: SUITE_MAJOR,
                received: self.protocol_major,
            });
        }
        if self.checks.is_empty() {
            return Err(EvidenceError::EmptyChecks);
        }
        if self.checks.iter().any(|check| !check.passed) {
            return Err(EvidenceError::FailedCheck);
        }
        let received: Vec<&str> = self.checks.iter().map(|check| check.name).collect();
        if received.as_slice() != EXPECTED_ACCEPTANCE_CHECKS {
            return Err(EvidenceError::CheckSetMismatch);
        }
        if self.platform != TRUSTED_PLATFORM {
            return Err(EvidenceError::PlatformMismatch { expected: TRUSTED_PLATFORM.into(), received: self.platform.clone() });
        }
        for (name, value) in [
            ("suite_release_digest", &self.suite_release_digest),
            ("verification_worker_release_digest", &self.verification_worker_release_digest),
            ("runner_release_digest", &self.runner_release_digest),
            ("backend_pack_digest", &self.backend_pack_digest),
            ("resolver_pack_digest", &self.resolver_pack_digest),
            ("sandbox_policy_digest", &self.sandbox_policy_digest),
            ("resolver_sandbox_policy_digest", &self.resolver_sandbox_policy_digest),
        ] {
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(EvidenceError::InvalidDigest(name));
            }
        }
        if self.sandbox_provider_id.is_empty() {
            return Err(EvidenceError::EmptyIdentity("sandbox_provider_id"));
        }
        if self.resolver_sandbox_provider_id.is_empty() {
            return Err(EvidenceError::EmptyIdentity("resolver_sandbox_provider_id"));
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
    checks.push(golden_messages_bidirectional(config));
    eprintln!("[suite] check negotiation_and_describe");
    checks.push(negotiation_and_describe(config));
    eprintln!("[suite] check required_capability_unmet_fails");
    checks.push(required_capability_unmet_fails(config));
    eprintln!("[suite] check major_mismatch_refused");
    checks.push(major_mismatch_refused(config));
    eprintln!("[suite] check malformed_frame_rejected");
    checks.push(malformed_frame_rejected(config));
    eprintln!("[suite] check oversized_frame_rejected");
    checks.push(oversized_frame_rejected(config));
    eprintln!("[suite] check execute_round_trip_completeness");
    checks.push(execute_round_trip_completeness(config));
    eprintln!("[suite] check execute_attempt_accounting");
    checks.push(execute_attempt_accounting(config));
    eprintln!("[suite] check execute_attempt_accounting_rejects_tampering");
    checks.push(execute_attempt_accounting_rejects_tampering(config));
    eprintln!("[suite] check deadline_is_failure");
    checks.push(deadline_is_failure(config));
    eprintln!("[suite] check cooperative_cancel_accounts_units");
    checks.push(cooperative_cancel_accounts_units(config));
    eprintln!("[suite] check crash_is_incomplete_failure");
    checks.push(crash_is_incomplete_failure(config));
    eprintln!("[suite] check resolver_golden_messages_bidirectional");
    checks.push(resolver_golden_messages_bidirectional(config));
    eprintln!("[suite] check resolver_minor_compatibility");
    checks.push(resolver_minor_compatibility(config));
    eprintln!("[suite] check resolver_negotiation_and_resolution");
    checks.push(resolver_negotiation_and_resolution(config));
    eprintln!("[suite] check resolver_required_capability_unmet_fails");
    checks.push(resolver_required_capability_unmet_fails(config));
    eprintln!("[suite] check resolver_major_mismatch_refused");
    checks.push(resolver_major_mismatch_refused(config));
    eprintln!("[suite] check resolver_resolution_deterministic");
    checks.push(resolver_resolution_deterministic(config));
    eprintln!("[suite] check resolver_malformed_frame_rejected");
    checks.push(resolver_malformed_frame_rejected(config));
    eprintln!("[suite] check resolver_unknown_optional_field_ignored");
    checks.push(resolver_unknown_optional_field_ignored(config));
    eprintln!("[suite] check resolver_oversized_frame_rejected");
    checks.push(resolver_oversized_frame_rejected(config));
    eprintln!("[suite] check resolver_deadline_is_failure");
    checks.push(resolver_deadline_is_failure(config));
    eprintln!("[suite] check resolver_cooperative_cancel_accounts_request");
    checks.push(resolver_cooperative_cancel_accounts_request(config));
    eprintln!("[suite] check resolver_crash_is_incomplete_failure");
    checks.push(resolver_crash_is_incomplete_failure(config));
    eprintln!("[suite] check resolver_retry_after_failure");
    checks.push(resolver_retry_after_failure(config));
    eprintln!("[suite] check resolver_safe_static_symlink_rejected");
    checks.push(resolver_safe_static_symlink_rejected(config));
    eprintln!("[suite] check resolver_backpressure_is_bounded");
    checks.push(resolver_backpressure_is_bounded());
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
/// probe then compares a deterministic semantic projection of the actual
/// response. A raw byte comparison would incorrectly reject protobuf map
/// entries whose wire order is not significant.
fn golden_messages_bidirectional(config: &SuiteConfig) -> CheckResult {
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
    let mut command = config.command();
    let probe = (|| -> Result<(), String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("host golden probe spawn failed: {error}"))?;
        let mut stdin = child.stdin.take().ok_or("host golden probe stdin unavailable")?;
        let mut stdout = child.stdout.take().ok_or("host golden probe stdout unavailable")?;
        let request_bytes = decode_hex(fixtures[0].1)?;
        let request = pb::HostEnvelope::decode(request_bytes.as_slice())
            .map_err(|error| format!("host golden request decode failed: {error}"))?;
        let expected_response_bytes = decode_hex(fixtures[1].1)?;
        let expected_response = pb::HostEnvelope::decode(expected_response_bytes.as_slice())
            .map_err(|error| format!("host golden response fixture decode failed: {error}"))?;
        let expected_ack = match expected_response.payload.as_ref() {
            Some(pb::host_envelope::Payload::HelloAck(ack)) => ack,
            _ => return Err("host golden response fixture was not HelloAck".into()),
        };
        let requested_limits = match request.payload.as_ref() {
            Some(pb::host_envelope::Payload::Hello(hello)) => hello
                .limits
                .as_ref()
                .ok_or("host golden request omitted limits")?,
            _ => return Err("host golden request was not Hello".into()),
        };
        write_frame(&mut stdin, &request_bytes)?;
        let response = read_frame(&mut stdout)?;
        let envelope = pb::HostEnvelope::decode(response.as_slice())
            .map_err(|error| format!("host golden response decode failed: {error}"))?;
        if envelope.correlation_id != request.correlation_id {
            return Err("host golden response correlation id did not echo the request".into());
        }
        match envelope.payload.as_ref() {
            Some(pb::host_envelope::Payload::HelloAck(ack)) => {
                let expected_projection = host_ack_projection(expected_ack, expected_ack)?;
                let actual_projection = host_ack_projection(ack, expected_ack)?;
                if actual_projection != expected_projection {
                    return Err("host golden response semantic projection differed from the fixture".into());
                }
                if ack.protocol_major != erf_host_client::PROTOCOL_MAJOR {
                    return Err(format!("host golden response used major {}", ack.protocol_major));
                }
                let accepted = ack
                    .accepted_limits
                    .as_ref()
                    .ok_or("host golden response omitted limits")?;
                if accepted.max_frame_bytes > requested_limits.max_frame_bytes
                    || accepted.max_events_per_execute > requested_limits.max_events_per_execute
                    || (requested_limits.deadline_ms > 0
                        && accepted.deadline_ms > requested_limits.deadline_ms)
                {
                    return Err("host golden response exceeded fixture limits".into());
                }
                if accepted.max_frame_bytes == 0 || accepted.max_events_per_execute == 0 {
                    return Err("host golden response accepted a zero execution limit".into());
                }
            }
            Some(other) => return Err(format!("host golden response was {other:?}")),
            None => return Err("host golden response had no payload".into()),
        }
        let _ = child.kill();
        let _ = child.wait();
        Ok(())
    })();
    match probe {
        Ok(()) => check(name, true, "fixtures decode canonically and drive the configured host".into()),
        Err(error) => check(name, false, error),
    }
}

/// Resolver golden envelopes are kept separate from Backend Host fixtures so
/// a schema change in one contract cannot silently certify the other.
fn resolver_golden_messages_bidirectional(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_golden_messages_bidirectional";
    let fixtures = [
        ("WorkspaceHello", include_str!("../golden/workspace_hello_v0.hex")),
        (
            "WorkspaceHelloAck",
            include_str!("../golden/workspace_hello_ack_v0.hex"),
        ),
    ];
    for (label, text) in fixtures {
        let bytes = match decode_hex(text) {
            Ok(bytes) => bytes,
            Err(detail) => return check(name, false, format!("{label} fixture: {detail}")),
        };
        let envelope = match wb::WrEnvelope::decode(bytes.as_slice()) {
            Ok(envelope) => envelope,
            Err(error) => return check(name, false, format!("{label} decode: {error}")),
        };
        if envelope.encode_to_vec() != bytes {
            return check(name, false, format!("{label} is not canonical protobuf"));
        }
    }
    let mut command = config.resolver_command();
    let probe = (|| -> Result<(), String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("resolver golden probe spawn failed: {error}"))?;
        let mut stdin = child.stdin.take().ok_or("resolver golden probe stdin unavailable")?;
        let mut stdout = child.stdout.take().ok_or("resolver golden probe stdout unavailable")?;
        let request_bytes = decode_hex(fixtures[0].1)?;
        let request = wb::WrEnvelope::decode(request_bytes.as_slice())
            .map_err(|error| format!("resolver golden request decode failed: {error}"))?;
        let requested_limits = match request.payload.as_ref() {
            Some(wb::wr_envelope::Payload::Hello(hello)) => hello
                .limits
                .as_ref()
                .ok_or("resolver golden request omitted limits")?,
            _ => return Err("resolver golden request was not Hello".into()),
        };
        let expected_response_bytes = decode_hex(fixtures[1].1)?;
        let expected_response = wb::WrEnvelope::decode(expected_response_bytes.as_slice())
            .map_err(|error| format!("resolver golden response fixture decode failed: {error}"))?;
        let expected_ack = match expected_response.payload.as_ref() {
            Some(wb::wr_envelope::Payload::HelloAck(ack)) => ack,
            _ => return Err("resolver golden response fixture was not HelloAck".into()),
        };
        write_frame(&mut stdin, &request_bytes)?;
        let response = read_frame(&mut stdout)?;
        let envelope = wb::WrEnvelope::decode(response.as_slice())
            .map_err(|error| format!("resolver golden response decode failed: {error}"))?;
        if envelope.correlation_id != request.correlation_id {
            return Err("resolver golden response correlation id did not use the handshake identity".into());
        }
        match envelope.payload.as_ref() {
            Some(wb::wr_envelope::Payload::HelloAck(ack)) => {
                let expected_projection = resolver_ack_projection(expected_ack, expected_ack)?;
                let actual_projection = resolver_ack_projection(ack, expected_ack)?;
                if actual_projection != expected_projection {
                    return Err("resolver golden response semantic projection differed from the fixture".into());
                }
                if ack.protocol_major != erf_resolver_client::PROTOCOL_MAJOR {
                    return Err(format!("resolver golden response used major {}", ack.protocol_major));
                }
                let accepted = ack
                    .accepted_limits
                    .as_ref()
                    .ok_or("resolver golden response omitted limits")?;
                if accepted.max_frame_bytes > requested_limits.max_frame_bytes
                    || (requested_limits.deadline_ms > 0
                        && accepted.deadline_ms > requested_limits.deadline_ms)
                {
                    return Err("resolver golden response exceeded fixture limits".into());
                }
            }
            Some(other) => return Err(format!("resolver golden response was {other:?}")),
            None => return Err("resolver golden response had no payload".into()),
        }
        let _ = child.kill();
        let _ = child.wait();
        Ok(())
    })();
    match probe {
        Ok(()) => check(name, true, "fixtures decode canonically and drive the configured resolver".into()),
        Err(error) => check(name, false, error),
    }
}

fn host_ack_projection(actual: &pb::HelloAck, expected: &pb::HelloAck) -> Result<Vec<u8>, String> {
    let actual_capabilities = actual
        .capabilities
        .as_ref()
        .ok_or("host golden response omitted capabilities")?;
    let expected_capabilities = expected
        .capabilities
        .as_ref()
        .ok_or("host golden fixture omitted capabilities")?;
    if actual.protocol_major != expected.protocol_major || actual.protocol_minor != expected.protocol_minor {
        return Err(format!(
            "host golden response protocol {}.{} differed from fixture {}.{}",
            actual.protocol_major,
            actual.protocol_minor,
            expected.protocol_major,
            expected.protocol_minor
        ));
    }
    if actual_capabilities.languages != expected_capabilities.languages {
        return Err("host golden response languages differed from the fixture".into());
    }
    if actual_capabilities.rule_kinds != expected_capabilities.rule_kinds {
        return Err("host golden response rule kinds differed from the fixture".into());
    }
    let mut fact_names: Vec<&str> = expected_capabilities.facts.keys().map(String::as_str).collect();
    fact_names.sort_unstable();
    let mut projection = b"erf-host-hello-ack-projection-v1\0".to_vec();
    projection.extend_from_slice(&actual.protocol_major.to_be_bytes());
    projection.extend_from_slice(&actual.protocol_minor.to_be_bytes());
    append_string_list(&mut projection, &actual_capabilities.languages);
    append_string_list(&mut projection, &actual_capabilities.rule_kinds);
    projection.extend_from_slice(&(fact_names.len() as u64).to_be_bytes());
    for name in fact_names {
        let actual_value = actual_capabilities
            .facts
            .get(name)
            .ok_or_else(|| format!("host golden response omitted required fact {name}"))?;
        let expected_value = expected_capabilities.facts.get(name).copied().unwrap_or(false);
        if *actual_value != expected_value {
            return Err(format!("host golden response fact {name} differed from the fixture"));
        }
        append_string(&mut projection, name);
        projection.push(u8::from(*actual_value));
    }
    Ok(projection)
}

fn resolver_ack_projection(actual: &wb::HelloAck, expected: &wb::HelloAck) -> Result<Vec<u8>, String> {
    if actual.protocol_major != expected.protocol_major || actual.protocol_minor != expected.protocol_minor {
        return Err(format!(
            "resolver golden response protocol {}.{} differed from fixture {}.{}",
            actual.protocol_major,
            actual.protocol_minor,
            expected.protocol_major,
            expected.protocol_minor
        ));
    }
    let mut fact_names: Vec<&str> = expected.facts.keys().map(String::as_str).collect();
    fact_names.sort_unstable();
    let mut projection = b"erf-resolver-hello-ack-projection-v1\0".to_vec();
    projection.extend_from_slice(&actual.protocol_major.to_be_bytes());
    projection.extend_from_slice(&actual.protocol_minor.to_be_bytes());
    projection.extend_from_slice(&(fact_names.len() as u64).to_be_bytes());
    for name in fact_names {
        let actual_value = actual
            .facts
            .get(name)
            .ok_or_else(|| format!("resolver golden response omitted required fact {name}"))?;
        let expected_value = expected.facts.get(name).copied().unwrap_or(false);
        if *actual_value != expected_value {
            return Err(format!("resolver golden response fact {name} differed from the fixture"));
        }
        append_string(&mut projection, name);
        projection.push(u8::from(*actual_value));
    }
    Ok(projection)
}

fn append_string_list(payload: &mut Vec<u8>, values: &[String]) {
    payload.extend_from_slice(&(values.len() as u64).to_be_bytes());
    for value in values {
        append_string(payload, value);
    }
}

fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> Result<(), String> {
    let length = u32::try_from(payload.len()).map_err(|_| "golden frame is too large".to_string())?;
    writer
        .write_all(&length.to_be_bytes())
        .and_then(|_| writer.write_all(payload))
        .and_then(|_| writer.flush())
        .map_err(|error| format!("golden frame write failed: {error}"))
}

fn read_frame<R: Read>(reader: &mut R) -> Result<Vec<u8>, String> {
    let mut header = [0u8; 4];
    reader
        .read_exact(&mut header)
        .map_err(|error| format!("golden frame read failed: {error}"))?;
    let length = u32::from_be_bytes(header) as usize;
    if length > erf_framing::DEFAULT_MAX_FRAME_BYTES as usize {
        return Err("golden response exceeded the default frame limit".into());
    }
    let mut payload = vec![0u8; length];
    reader
        .read_exact(&mut payload)
        .map_err(|error| format!("golden response was truncated: {error}"))?;
    Ok(payload)
}

fn resolver_minor_compatibility(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_minor_compatibility";
    let mut command = config.resolver_command();
    command.env("FAKE_PROTOCOL_MINOR", "0");
    let outcome = (|| -> Result<(), ResolverError> {
        let (resolver, ack) = ResolverHandle::spawn(
            &mut command,
            &resolver_config(
                config.resolver_required_capabilities.clone(),
                config.resolver_limits.clone(),
            ),
        )?;
        if ack.protocol_major != erf_resolver_client::PROTOCOL_MAJOR || ack.protocol_minor != 0 {
            return Err(ResolverError::Violation(format!(
                "compatible minor negotiation returned {}.{}",
                ack.protocol_major, ack.protocol_minor
            )));
        }
        let completion = resolver.complete()?;
        if completion.status != wb::completion::Status::Completed as i32 {
            return Err(ResolverError::Violation(format!(
                "minor-compatible session completed with status {}",
                completion.status
            )));
        }
        Ok(())
    })();
    match outcome {
        Ok(()) => check(name, true, "compatible protocol minor was accepted".into()),
        Err(error) => check(name, false, format!("minor compatibility failed: {error}")),
    }
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
    // Drive a real host process: complete a valid handshake at a 1 KiB cap,
    // then put a 2 KiB frame on its stdin. This exercises the receiver-side
    // framing guard rather than merely asking the client writer to reject it.
    command.env("FAKE_MAX_FRAME_BYTES", "1024");
    let mut child = match command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return check(name, false, format!("spawn failed: {error}")),
    };
    let outcome = (|| -> Result<(), String> {
        let stdin = child.stdin.take().ok_or("host stdin was not piped")?;
        let stdout = child.stdout.take().ok_or("host stdout was not piped")?;
        let mut writer = FrameWriter::new(stdin, 4096);
        let limits = pb::Limits {
            max_frame_bytes: 1024,
            max_events_per_execute: config.limits.max_events_per_execute,
            deadline_ms: config.limits.deadline_ms,
        };
        let hello = pb::HostEnvelope {
            correlation_id: 1,
            payload: Some(pb::host_envelope::Payload::Hello(pb::Hello {
                protocol_major: erf_host_client::PROTOCOL_MAJOR,
                protocol_minor: erf_host_client::PROTOCOL_MINOR,
                required_capabilities: vec![],
                optional_capabilities: vec![],
                limits: Some(limits),
            })),
        };
        writer
            .write_frame(&hello.encode_to_vec())
            .map_err(|error| format!("hello write failed: {error}"))?;
        let mut reader = FrameReader::new(stdout, erf_framing::DEFAULT_MAX_FRAME_BYTES);
        let ack = reader
            .read_frame()
            .map_err(|error| format!("hello ack read failed: {error}"))?;
        let ack = pb::HostEnvelope::decode(ack.as_slice())
            .map_err(|error| format!("hello ack decode failed: {error}"))?;
        if !matches!(ack.payload, Some(pb::host_envelope::Payload::HelloAck(_))) {
            return Err(format!("expected HelloAck, got {:?}", ack.payload));
        }
        writer
            .write_frame(&vec![0u8; 2048])
            .map_err(|error| format!("oversized probe write failed: {error}"))?;
        let response = reader
            .read_frame()
            .map_err(|error| format!("oversized error read failed: {error}"))?;
        let response = pb::HostEnvelope::decode(response.as_slice())
            .map_err(|error| format!("oversized error decode failed: {error}"))?;
        match response.payload {
            Some(pb::host_envelope::Payload::Error(error))
                if error.code == pb::protocol_error::Code::OversizedFrame as i32 => Ok(()),
            other => Err(format!("expected OVERSIZED_FRAME, got {other:?}")),
        }
    })();
    if outcome.is_err() {
        let _ = child.kill();
    }
    let status = child.wait();
    match (outcome, status) {
        (Ok(()), Ok(status)) if !status.success() => {
            check(name, true, "host process rejected an oversized inbound frame".into())
        }
        (Ok(()), Ok(status)) => check(name, false, format!("host accepted oversized frame with {status}")),
        (Err(error), _) => check(name, false, error),
        (_, Err(error)) => check(name, false, format!("wait failed: {error}")),
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
                ..Default::default()
            },
            pb::AnalysisUnit {
                unit_id: planned[1].clone(),
                language: "java".into(),
                files: vec![],
                ..Default::default()
            },
            pb::AnalysisUnit {
                unit_id: planned[2].clone(),
                language: "java".into(),
                files: vec![],
                ..Default::default()
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

/// Drives the same deterministic three-unit attempt used by the completeness
/// probe, but keeps the terminal accounting facts as first-class evidence.
/// `HostHandle::next_event` already rejects a mismatched count or digest; this
/// check makes the rejection boundary visible to the conformance report and
/// records the digest so independent host implementations can be compared.
fn execute_attempt_accounting(config: &SuiteConfig) -> CheckResult {
    let name = "execute_attempt_accounting";
    let mut command = config.command();
    command
        .env_remove("ERF_DELAY_MS")
        .env_remove("FAKE_DELAY_MS");
    let cfg = config_with(config);
    let outcome = (|| -> Result<String, HostError> {
        let (mut host, _) = HostHandle::spawn(&mut command, &cfg)?;
        let planned = ["unit-1", "unit-2", "unit-3"];
        host.plan(
            planned
                .iter()
                .map(|unit_id| pb::AnalysisUnit {
                    unit_id: (*unit_id).into(),
                    language: "java".into(),
                    files: vec![],
                    ..Default::default()
                })
                .collect(),
        )?;
        let mut stream = host.execute(planned.iter().map(|id| (*id).to_string()).collect())?;
        let mut observations = 0u64;
        let mut statuses = 0u64;
        let complete = loop {
            match stream.next_event()? {
                Some(ExecEvent::Observation(_)) => observations += 1,
                Some(ExecEvent::UnitStatus(_)) => statuses += 1,
                Some(ExecEvent::Complete(complete)) => break complete,
                None => return Err(HostError::Violation("stream ended before ExecuteComplete".into())),
            }
        };
        if complete.units_reported != statuses {
            return Err(HostError::Violation(format!(
                "units_reported={} but observed {} UnitStatus events",
                complete.units_reported, statuses
            )));
        }
        if complete.observation_count != observations {
            return Err(HostError::Violation(format!(
                "observation_count={} but observed {} BackendObservation events",
                complete.observation_count, observations
            )));
        }
        if complete.attempt_digest.len() != 32 {
            return Err(HostError::Violation(format!(
                "attempt_digest has {} bytes, expected 32",
                complete.attempt_digest.len()
            )));
        }
        let completion = host.complete()?;
        if completion.status != pb::completion::Status::Completed as i32 {
            return Err(HostError::Violation(format!(
                "non-completed terminal status {}",
                completion.status
            )));
        }
        Ok(format!(
            "units_reported={} observation_count={} attempt_digest={}",
            complete.units_reported,
            complete.observation_count,
            hex_digest(&complete.attempt_digest)
        ))
    })();
    match outcome {
        Ok(detail) => check(name, true, detail),
        Err(error) => check(name, false, format!("{error}")),
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The reference Hosts expose deterministic accounting fault injection. Keep
/// the two independent checks here so a host cannot satisfy the count check
/// while emitting a stale digest (or vice versa), and so the client-side
/// rejection remains part of the shared Rust/Node suite rather than only a
/// unit-test seam.
fn execute_attempt_accounting_rejects_tampering(config: &SuiteConfig) -> CheckResult {
    let name = "execute_attempt_accounting_rejects_tampering";
    let outcome = (|| -> Result<String, HostError> {
        for (knob, marker) in [
            ("FAKE_BAD_OBSERVATION_COUNT", "observation_count"),
            ("FAKE_BAD_ATTEMPT_DIGEST", "attempt_digest"),
        ] {
            let mut command = config.command();
            command
                .env(knob, "1")
                .env_remove("ERF_DELAY_MS")
                .env_remove("FAKE_DELAY_MS");
            let cfg = config_with(config);
            let (mut host, _) = HostHandle::spawn(&mut command, &cfg)?;
            host.plan(vec![pb::AnalysisUnit {
                unit_id: "tamper-unit".into(),
                language: "java".into(),
                files: vec![],
                ..Default::default()
            }])?;
            let mut stream = host.execute(vec!["tamper-unit".into()])?;
            loop {
                match stream.next_event() {
                    Err(HostError::Violation(detail)) if detail.contains(marker) => break,
                    Err(error) => {
                        return Err(HostError::Violation(format!(
                            "{knob} produced the wrong failure: {error}"
                        )))
                    }
                    Ok(Some(_)) => continue,
                    Ok(None) => {
                        return Err(HostError::Violation(format!(
                            "{knob} ended without rejecting tampered accounting"
                        )))
                    }
                }
            }
        }
        Ok("wrong observation_count and attempt_digest were both rejected".into())
    })();
    match outcome {
        Ok(detail) => check(name, true, detail),
        Err(error) => check(name, false, format!("{error}")),
    }
}

fn cooperative_cancel_accounts_units(config: &SuiteConfig) -> CheckResult {
    let name = "cooperative_cancel_accounts_units";
    if config.cancel_probe_delay_ms == 0 {
        return check(name, false, "cancel probe requires a configured delay".into());
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
                ..Default::default()
            },
            pb::AnalysisUnit {
                unit_id: "unit-2".into(),
                language: "java".into(),
                files: vec![],
                ..Default::default()
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
    // Leave enough budget for a cold process handshake. The injected delay
    // must still exceed the request deadline for both the Rust and Node hosts.
    command.env("FAKE_DELAY_MS", "1000").env("ERF_DELAY_MS", "1000");
    let mut limits = config.limits.clone();
    limits.deadline_ms = 250;
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
            ..Default::default()
        }])?;
        let mut stream = host.execute(vec!["deadline-unit".into()])?;
        match stream.next_event() {
            Err(HostError::DeadlineExceeded { deadline_ms: 250 }) => Ok(()),
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
            ..Default::default()
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

/// The resolver half of the shared suite uses a disposable, ordinary Maven
/// layout. The fixture is intentionally created by the suite rather than by
/// an implementation-owned integration test so every resolver is driven by
/// the same request sequence and oracle.
struct ResolverFixture {
    root: PathBuf,
}

impl Drop for ResolverFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn resolver_fixture() -> Result<ResolverFixture, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("erf-conformance-resolver-{}-{nonce}", std::process::id()));
    fs::create_dir_all(root.join("src/main/java")).map_err(|error| error.to_string())?;
    fs::create_dir_all(root.join("module-a/src/main/java")).map_err(|error| error.to_string())?;
    fs::create_dir_all(root.join("module-b/src/main/java")).map_err(|error| error.to_string())?;
    fs::write(
        root.join("pom.xml"),
        br#"<project xmlns="http://maven.apache.org/POM/4.0.0"><modelVersion>4.0.0</modelVersion><groupId>org.erf</groupId><artifactId>conformance-fixture</artifactId><version>1.0</version></project>"#,
    )
    .map_err(|error| error.to_string())?;
    fs::write(root.join("src/main/java/App.java"), b"class App { void run() {} }\n")
        .map_err(|error| error.to_string())?;
    fs::write(
        root.join("module-a/src/main/java/A.java"),
        b"class A { void run() {} }\n",
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        root.join("module-b/src/main/java/B.java"),
        b"class B { void run() {} }\n",
    )
    .map_err(|error| error.to_string())?;
    Ok(ResolverFixture { root })
}

fn resolver_config(required_capabilities: Vec<String>, limits: wb::Limits) -> ResolverConfig {
    ResolverConfig {
        required_capabilities,
        limits,
    }
}

fn resolver_spawn(
    config: &SuiteConfig,
    required_capabilities: Vec<String>,
    limits: wb::Limits,
) -> Result<(ResolverHandle, wb::HelloAck), ResolverError> {
    let mut command = config.resolver_command();
    ResolverHandle::spawn(&mut command, &resolver_config(required_capabilities, limits))
}

fn resolver_resolve_once(config: &SuiteConfig, root: &Path) -> Result<wb::WorkspaceDescriptor, String> {
    let (mut resolver, _) = resolver_spawn(
        config,
        config.resolver_required_capabilities.clone(),
        config.resolver_limits.clone(),
    )
    .map_err(|error| error.to_string())?;
    let descriptor = resolver
        .resolve(root.to_str().ok_or_else(|| "fixture path is not UTF-8".to_string())?)
        .map_err(|error| error.to_string())?;
    let completion = resolver.complete().map_err(|error| error.to_string())?;
    if completion.status != wb::completion::Status::Completed as i32 {
        return Err(format!("resolver completed with status {}", completion.status));
    }
    Ok(descriptor)
}

fn resolver_negotiation_and_resolution(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_negotiation_and_resolution";
    let fixture = match resolver_fixture() {
        Ok(fixture) => fixture,
        Err(error) => return check(name, false, format!("fixture failed: {error}")),
    };
    let outcome = (|| -> Result<String, String> {
        let (mut resolver, ack) = resolver_spawn(
            config,
            config.resolver_required_capabilities.clone(),
            config.resolver_limits.clone(),
        )
        .map_err(|error| error.to_string())?;
        if ack.facts.get("ecosystem.static-layout") != Some(&true) {
            return Err("handshake omitted ecosystem.static-layout".into());
        }
        let descriptor = resolver
            .resolve(fixture.root.to_str().ok_or_else(|| "fixture path is not UTF-8".to_string())?)
            .map_err(|error| error.to_string())?;
        if descriptor.mode != wb::WorkspaceResolutionMode::SafeStatic as i32 {
            return Err(format!("unexpected resolution mode {}", descriptor.mode));
        }
        let module_count = descriptor.modules.len();
        let file_count: usize = descriptor.modules.iter().map(|module| module.files.len()).sum();
        if module_count == 0 || file_count == 0 {
            return Err(format!("empty descriptor: modules={module_count} files={file_count}"));
        }
        let completion = resolver.complete().map_err(|error| error.to_string())?;
        if completion.status != wb::completion::Status::Completed as i32 {
            return Err(format!("resolver completed with status {}", completion.status));
        }
        Ok(format!("{} modules and {} files resolved", module_count, file_count))
    })();
    match outcome {
        Ok(detail) => check(name, true, detail),
        Err(error) => check(name, false, error),
    }
}

fn resolver_required_capability_unmet_fails(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_required_capability_unmet_fails";
    let mut command = config.resolver_command();
    let resolver_config = ResolverConfig {
        required_capabilities: vec!["ecosystem.erf-nonexistent-capability".into()],
        limits: config.resolver_limits.clone(),
    };
    match ResolverHandle::spawn(&mut command, &resolver_config) {
        Err(ResolverError::Protocol {
            code: wb::protocol_error::Code::UnsupportedCapability,
            ..
        })
        | Err(ResolverError::MissingCapabilities { .. }) => {
            check(name, true, "handshake failed explicitly".into())
        }
        Err(error) => check(name, false, format!("wrong failure mode: {error}")),
        Ok(_) => check(name, false, "handshake succeeded despite unknown capability".into()),
    }
}

fn resolver_major_mismatch_refused(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_major_mismatch_refused";
    let mut command = config.resolver_command();
    command.env("FAKE_SUPPORTED_MAJOR", "1");
    let resolver_config = resolver_config(
        config.resolver_required_capabilities.clone(),
        config.resolver_limits.clone(),
    );
    match ResolverHandle::spawn(&mut command, &resolver_config) {
        Err(ResolverError::Protocol {
            code: wb::protocol_error::Code::ProtocolMajorMismatch,
            ..
        })
        | Err(ResolverError::MajorMismatch { .. }) => {
            check(name, true, "refused execution on major mismatch".into())
        }
        Err(error) => check(name, false, format!("wrong failure mode: {error}")),
        Ok(_) => check(name, false, "major mismatch was accepted".into()),
    }
}

fn resolver_resolution_deterministic(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_resolution_deterministic";
    let fixture = match resolver_fixture() {
        Ok(fixture) => fixture,
        Err(error) => return check(name, false, format!("fixture failed: {error}")),
    };
    let first = resolver_resolve_once(config, &fixture.root);
    let second = resolver_resolve_once(config, &fixture.root);
    match (first, second) {
        (Ok(first), Ok(second)) if first == second => {
            check(name, true, "identical snapshot produced identical descriptor".into())
        }
        (Ok(first), Ok(second)) => check(
            name,
            false,
            format!(
                "descriptor changed between runs: first={:x?} second={:x?}",
                first.descriptor_sha256, second.descriptor_sha256
            ),
        ),
        (Err(first), Err(second)) => check(name, false, format!("both resolutions failed: {first}; {second}")),
        (Err(first), Ok(_)) | (Ok(_), Err(first)) => {
            check(name, false, format!("one resolution failed: {first}"))
        }
    }
}

fn resolver_malformed_frame_rejected(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_malformed_frame_rejected";
    let fixture = match resolver_fixture() {
        Ok(fixture) => fixture,
        Err(error) => return check(name, false, format!("fixture failed: {error}")),
    };
    let mut command = config.resolver_command();
    command.env("FAKE_MALFORMED_ON_RESOLVE", "1");
    let outcome = (|| -> Result<(), ResolverError> {
        let (mut resolver, _) = ResolverHandle::spawn(
            &mut command,
            &resolver_config(
                config.resolver_required_capabilities.clone(),
                config.resolver_limits.clone(),
            ),
        )?;
        match resolver.resolve(fixture.root.to_str().expect("fixture path is UTF-8")) {
            Err(ResolverError::Transport(erf_framing::FrameError::MalformedProtobuf(_))) => Ok(()),
            Err(error) => Err(error),
            Ok(_) => Err(ResolverError::Violation("malformed resolver frame was accepted".into())),
        }
    })();
    match outcome {
        Ok(()) => check(name, true, "malformed resolver protobuf was rejected".into()),
        Err(error) => check(name, false, format!("wrong malformed-frame result: {error}")),
    }
}

fn resolver_unknown_optional_field_ignored(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_unknown_optional_field_ignored";
    let mut command = config.resolver_command();
    command.env("FAKE_UNKNOWN_OPTIONAL_ON_HELLO", "1");
    let resolver_config = resolver_config(
        config.resolver_required_capabilities.clone(),
        config.resolver_limits.clone(),
    );
    match ResolverHandle::spawn(&mut command, &resolver_config) {
        Ok((resolver, _)) => match resolver.complete() {
            Ok(completion) if completion.status == wb::completion::Status::Completed as i32 => {
                check(name, true, "real resolver handshake ignored an additive unknown field".into())
            }
            Ok(completion) => check(name, false, format!("resolver completed with status {}", completion.status)),
            Err(error) => check(name, false, format!("resolver completion failed: {error}")),
        },
        Err(error) => check(name, false, format!("unknown optional field was not ignored: {error}")),
    }
}

fn resolver_oversized_frame_rejected(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_oversized_frame_rejected";
    let fixture = match resolver_fixture() {
        Ok(fixture) => fixture,
        Err(error) => return check(name, false, format!("fixture failed: {error}")),
    };
    let mut command = config.resolver_command();
    command.env("FAKE_OVERSIZED_ON_RESOLVE", "1");
    let mut limits = config.resolver_limits.clone();
    limits.max_frame_bytes = 1024;
    let outcome = (|| -> Result<(), ResolverError> {
        let (mut resolver, _) = ResolverHandle::spawn(
            &mut command,
            &resolver_config(config.resolver_required_capabilities.clone(), limits),
        )?;
        match resolver.resolve(fixture.root.to_str().expect("fixture path is UTF-8")) {
            Err(ResolverError::Transport(erf_framing::FrameError::Oversized { size: 2048, max: 1024 })) => Ok(()),
            Err(error) => Err(error),
            Ok(_) => Err(ResolverError::Violation("oversized resolver frame was accepted".into())),
        }
    })();
    match outcome {
        Ok(()) => check(name, true, "resolver frame limit rejected a 2048-byte response".into()),
        Err(error) => check(name, false, format!("wrong oversized-frame result: {error}")),
    }
}

fn resolver_deadline_is_failure(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_deadline_is_failure";
    let fixture = match resolver_fixture() {
        Ok(fixture) => fixture,
        Err(error) => return check(name, false, format!("fixture failed: {error}")),
    };
    let mut command = config.resolver_command();
    // Leave enough budget for a cold process handshake while keeping the
    // resolver delay well beyond the request deadline. The contract applies
    // this limit to the handshake as well as resolution requests.
    command.env("FAKE_RESOLVE_DELAY_MS", "5000");
    let mut limits = config.resolver_limits.clone();
    limits.deadline_ms = 2_000;
    let outcome = (|| -> Result<(), ResolverError> {
        let (mut resolver, _) = ResolverHandle::spawn(
            &mut command,
            &resolver_config(config.resolver_required_capabilities.clone(), limits),
        )?;
        match resolver.resolve(fixture.root.to_str().expect("fixture path is UTF-8")) {
            Err(ResolverError::DeadlineExceeded { deadline_ms: 2_000 }) => Ok(()),
            Err(error) => Err(error),
            Ok(_) => Err(ResolverError::Violation("resolution unexpectedly completed".into())),
        }
    })();
    match outcome {
        Ok(()) => check(name, true, "resolution deadline classified as failure".into()),
        Err(error) => check(name, false, format!("wrong deadline failure: {error}")),
    }
}

fn resolver_cooperative_cancel_accounts_request(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_cooperative_cancel_accounts_request";
    let fixture = match resolver_fixture() {
        Ok(fixture) => fixture,
        Err(error) => return check(name, false, format!("fixture failed: {error}")),
    };
    let outcome = (|| -> Result<String, ResolverError> {
        let mut command = config.resolver_command();
        command.env("FAKE_RESOLVE_DELAY_MS", "200");
        let (mut resolver, _) = ResolverHandle::spawn(
            &mut command,
            &resolver_config(
                config.resolver_required_capabilities.clone(),
                config.resolver_limits.clone(),
            ),
        )?;
        let cancel = resolver.resolve_and_cancel(fixture.root.to_str().expect("fixture path is UTF-8"))?;
        if !cancel.accepted {
            return Err(ResolverError::Violation("resolver rejected cooperative cancel".into()));
        }
        let completion = resolver.complete()?;
        if completion.status != wb::completion::Status::Cancelled as i32 {
            return Err(ResolverError::Violation("cancel left the session incomplete".into()));
        }
        Ok("in-flight resolution cancellation was acknowledged and accounted".into())
    })();
    match outcome {
        Ok(detail) => check(name, true, detail),
        Err(error) => check(name, false, format!("{error}")),
    }
}

fn resolver_crash_is_incomplete_failure(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_crash_is_incomplete_failure";
    let fixture = match resolver_fixture() {
        Ok(fixture) => fixture,
        Err(error) => return check(name, false, format!("fixture failed: {error}")),
    };
    let mut command = config.resolver_command();
    command.env("FAKE_CRASH_ON_RESOLVE", "1");
    let outcome = (|| -> Result<(), ResolverError> {
        let resolver_config = resolver_config(
            config.resolver_required_capabilities.clone(),
            config.resolver_limits.clone(),
        );
        let (mut resolver, _) = ResolverHandle::spawn(&mut command, &resolver_config)?;
        match resolver.resolve(fixture.root.to_str().expect("fixture path is UTF-8")) {
            Err(ResolverError::Transport(erf_framing::FrameError::Eof))
            | Err(ResolverError::Transport(erf_framing::FrameError::UnexpectedEof { .. })) => Ok(()),
            Err(error) => Err(error),
            Ok(_) => Err(ResolverError::Violation("crashed resolver returned a descriptor".into())),
        }
    })();
    match outcome {
        Ok(()) => check(name, true, "resolver crash classified as incomplete resolution".into()),
        Err(error) => check(name, false, format!("wrong crash failure: {error}")),
    }
}

fn resolver_retry_after_failure(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_retry_after_failure";
    let fixture = match resolver_fixture() {
        Ok(fixture) => fixture,
        Err(error) => return check(name, false, format!("fixture failed: {error}")),
    };
    let outcome = (|| -> Result<(), ResolverError> {
        let (mut resolver, _) = resolver_spawn(
            config,
            config.resolver_required_capabilities.clone(),
            config.resolver_limits.clone(),
        )?;
        match resolver.resolve("/nonexistent/erf-conformance-retry") {
            Err(ResolverError::Protocol {
                code: wb::protocol_error::Code::RepoRootUnreadable,
                ..
            }) => {}
            Err(error) => return Err(error),
            Ok(_) => return Err(ResolverError::Violation("invalid root unexpectedly resolved".into())),
        }
        let descriptor = resolver.resolve(fixture.root.to_str().expect("fixture path is UTF-8"))?;
        if descriptor.modules.is_empty() {
            return Err(ResolverError::Violation("retry returned an empty descriptor".into()));
        }
        let completion = resolver.complete()?;
        if completion.status != wb::completion::Status::Completed as i32 {
            return Err(ResolverError::Violation(format!(
                "retry session completed with status {}",
                completion.status
            )));
        }
        Ok(())
    })();
    match outcome {
        Ok(()) => check(name, true, "request-scoped resolver failure was retried successfully".into()),
        Err(error) => check(name, false, format!("retry failed: {error}")),
    }
}

fn resolver_safe_static_symlink_rejected(config: &SuiteConfig) -> CheckResult {
    let name = "resolver_safe_static_symlink_rejected";
    #[cfg(not(unix))]
    {
        return check(name, false, "SAFE_STATIC symbolic-link probe requires Unix".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let fixture = match resolver_fixture() {
            Ok(fixture) => fixture,
            Err(error) => return check(name, false, format!("fixture failed: {error}")),
        };
        let outside = fixture
            .root
            .parent()
            .expect("fixture has a parent")
            .join("outside.java");
        let link = fixture.root.join("src/main/java/escape.java");
        if let Err(error) = fs::write(&outside, b"class Outside {}\n") {
            return check(name, false, format!("outside fixture failed: {error}"));
        }
        if let Err(error) = symlink(&outside, &link) {
            let _ = fs::remove_file(&outside);
            return check(name, false, format!("symlink fixture failed: {error}"));
        }
        let outcome = (|| -> Result<(), ResolverError> {
            let (mut resolver, _) = resolver_spawn(
                config,
                config.resolver_required_capabilities.clone(),
                config.resolver_limits.clone(),
            )?;
            match resolver.resolve(fixture.root.to_str().expect("fixture path is UTF-8")) {
                Err(ResolverError::Protocol {
                    code: wb::protocol_error::Code::RepoRootUnreadable,
                    ..
                }) => Ok(()),
                Err(error) => Err(error),
                Ok(_) => Err(ResolverError::Violation("symlink escape was resolved".into())),
            }
        })();
        let _ = fs::remove_file(&outside);
        match outcome {
            Ok(()) => check(name, true, "SAFE_STATIC symbolic-link input was rejected".into()),
            Err(error) => check(name, false, format!("wrong SAFE_STATIC result: {error}")),
        }
    }
}

fn resolver_backpressure_is_bounded() -> CheckResult {
    let name = "resolver_backpressure_is_bounded";
    struct WouldBlock;
    impl std::io::Write for WouldBlock {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "bounded probe"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    match erf_framing::FrameWriter::new(WouldBlock, 1024).write_frame(&[1, 2, 3]) {
        Err(erf_framing::FrameError::Io(error))
            if error.kind() == std::io::ErrorKind::WouldBlock => {
                check(name, true, "resolver transport propagated backpressure without buffering".into())
            }
        Ok(()) => check(name, false, "backpressure probe unexpectedly wrote a frame".into()),
        Err(error) => check(name, false, format!("wrong backpressure result: {error}")),
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
        let checks = EXPECTED_ACCEPTANCE_CHECKS
            .iter()
            .map(|&name| CheckResult { name, passed: true, detail: "acceptance check passed".into() })
            .collect();
        SuiteReport {
            suite_version: SUITE_VERSION.into(),
            protocol_major: SUITE_MAJOR,
            checks,
        }
    }

    fn evidence_and_store() -> (ConformanceEvidence, erf_trust::TrustStore) {
        let evidence = ConformanceEvidence::from_report(
            &report(),
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64),
            "d".repeat(64),
            "e".repeat(64),
            "rootless-oci-docker-v0",
            "f".repeat(64),
            "rootless-oci-docker-v0",
            "1".repeat(64),
            TRUSTED_PLATFORM,
        ).unwrap();
        let key = SigningKey::from_bytes(&[41; 32]);
        let mut store = erf_trust::TrustStore::default();
        store.insert_key("conformance-key", key.verifying_key()).unwrap();
        store.authorize_role(VERIFICATION_SIGNING_ROLE, "conformance-key").unwrap();
        (evidence, store)
    }

    fn test_sign(evidence: &ConformanceEvidence, key_id: &str, key: &SigningKey) -> erf_trust::SignedAsset {
        let payload = evidence.canonical_payload();
        erf_trust::SignedAsset {
            payload_type: erf_trust::DSSE_PAYLOAD_TYPE.into(),
            payload: payload.clone(),
            key_id: key_id.into(),
            signature: key
                .sign(&erf_trust::pae(erf_trust::DSSE_PAYLOAD_TYPE, &payload))
                .to_bytes()
                .to_vec(),
        }
    }

    #[test]
    fn evidence_binds_suite_results_and_execution_identities() {
        let (evidence, store) = evidence_and_store();
        let key = SigningKey::from_bytes(&[41; 32]);
        let asset = test_sign(&evidence, "conformance-key", &key);
        assert!(evidence.verify_signed(&asset, &store, VERIFICATION_SIGNING_ROLE).is_ok());
        let mut changed = evidence.clone();
        changed.backend_pack_digest = "e".repeat(64);
        assert_eq!(changed.verify_signed(&asset, &store, VERIFICATION_SIGNING_ROLE), Err(EvidenceError::PayloadMismatch));
    }

    #[test]
    fn evidence_rejects_failed_checks_and_stale_or_revoked_metadata() {
        let mut failed = report();
        failed.checks[0].passed = false;
        assert_eq!(ConformanceEvidence::from_report(&failed, "a".repeat(64), "b".repeat(64), "c".repeat(64), "d".repeat(64), "e".repeat(64), "provider", "f".repeat(64), "provider", "1".repeat(64), TRUSTED_PLATFORM), Err(EvidenceError::FailedCheck));
        let (evidence, store) = evidence_and_store();
        let key = SigningKey::from_bytes(&[41; 32]);
        let asset = test_sign(&evidence, "conformance-key", &key);
        let expired = erf_trust::TrustMetadata { version: 1, expires_at_epoch_secs: 10, revoked_key_ids: BTreeSet::new() };
        assert_eq!(evidence.verify_signed_with_metadata(&asset, &store, VERIFICATION_SIGNING_ROLE, &expired, 10), Err(EvidenceError::Trust(erf_trust::TrustError::MetadataExpired)));
    }

    #[test]
    fn evidence_rejects_wrong_suite_identity_before_signing() {
        let mut wrong_version = report();
        wrong_version.suite_version = "0.3.0".into();
        assert_eq!(
            ConformanceEvidence::from_report(
                &wrong_version,
                "a".repeat(64),
                "b".repeat(64),
                "c".repeat(64),
                "d".repeat(64),
                "e".repeat(64),
                "provider",
                "f".repeat(64),
                "provider",
                "1".repeat(64),
                TRUSTED_PLATFORM,
            ),
            Err(EvidenceError::SuiteVersionMismatch {
                expected: SUITE_VERSION.into(),
                received: "0.3.0".into(),
            })
        );

        let mut wrong_major = report();
        wrong_major.protocol_major = SUITE_MAJOR + 1;
        assert_eq!(
            ConformanceEvidence::from_report(
                &wrong_major,
                "a".repeat(64),
                "b".repeat(64),
                "c".repeat(64),
                "d".repeat(64),
                "e".repeat(64),
                "provider",
                "f".repeat(64),
                "provider",
                "1".repeat(64),
                TRUSTED_PLATFORM,
            ),
            Err(EvidenceError::ProtocolMajorMismatch {
                expected: SUITE_MAJOR,
                received: SUITE_MAJOR + 1,
            })
        );
    }

    #[test]
    fn suite_digest_changes_when_check_set_changes() {
        let first = report();
        let mut changed = report();
        changed.checks.push(CheckResult { name: "new_check", passed: true, detail: "new".into() });
        assert_ne!(first.suite_digest(), changed.suite_digest());
    }
}
