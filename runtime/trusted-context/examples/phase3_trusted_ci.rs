use ed25519_dalek::{Signer, SigningKey};
use erf_bootstrap::{RunnerBootstrap, RunnerMeasurement, RunnerRelease, LINUX_AMD64, RUNNER_RELEASE_ASSET_TYPE, RUNNER_RELEASE_SIGNING_ROLE};
use erf_conformance::{run_acceptance, ConformanceEvidence, SuiteConfig, SuiteReport, CONFORMANCE_SIGNING_ROLE, TRUSTED_PLATFORM};
use erf_packs::{OciPackRelease, OciPackVerifier, PackMeasurement, BACKEND_RUNTIME_PACK_ASSET_TYPE, LINUX_AMD64 as PACK_LINUX_AMD64, RUNTIME_PACK_SIGNING_ROLE, WORKSPACE_RESOLVER_PACK_ASSET_TYPE};
use erf_status::{PublishOutcome, PublishedState, StatusPublisher, StatusPublisherContext, StatusPublishError};
use erf_trust::{ArtifactStatement, SignedAsset, TrustMetadata, TrustStore, DSSE_PAYLOAD_TYPE};
use erf_trusted_context::{TrustedContextError, TrustedContextExpectations, TrustedContextInput, TrustedContextVerifier, EXECUTION_LOCK_SIGNING_ROLE, GATE_SIGNING_ROLE};
use rf_gate::{Completeness, GateAttestation, GateDecision};
use rf_runner::{ExecutionLock, SignedExecutionLock};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::Path;

const REPOSITORY: &str = "org/ruleops-demo";
const COMMIT: &str = "phase3-fixed-commit";
const BACKEND_ID: &str = "pmd-java";
const BACKEND_VERSION: &str = "7.9.0";
const RESOLVER_ID: &str = "maven-static";
const RESOLVER_VERSION: &str = "0.1.0";
const SANDBOX_PROVIDER: &str = "rootless-oci-docker-v0";

struct Fixture {
    lock: SignedExecutionLock,
    runner: RunnerMeasurement,
    backend: PackMeasurement,
    resolver: PackMeasurement,
    evidence: ConformanceEvidence,
    signed_evidence: SignedAsset,
    gate: GateAttestation,
    signed_gate: SignedAsset,
    trust_store: TrustStore,
    gate_key: SigningKey,
    verifier: TrustedContextVerifier,
    report: SuiteReport,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("phase3 trusted CI harness failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let output = env::args().nth(1).ok_or("usage: phase3-trusted-ci OUT_DIR")?;
    fs::create_dir_all(&output)?;
    let fixture = build_fixture()?;
    let mut checks = Vec::<(&str, bool, String)>::new();

    checks.push((
        "conformance",
        fixture.report.all_passed(),
        format!("{} checks passed; suite_digest={}", fixture.report.checks.len(), hex_digest(&fixture.report.suite_digest())),
    ));

    let context = fixture.verifier.verify(input(&fixture, None, None, None, None, None, None, None));
    checks.push((
        "trusted_context",
        context.is_ok(),
        match context {
            Ok(context) => format!("accepted {:?} for {}/{}", context.decision, context.repository, context.commit),
            Err(error) => format!("unexpected rejection: {error}"),
        },
    ));

    let policy_digest = "f".repeat(64);
    let mut publisher = StatusPublisher::new(StatusPublisherContext::new(REPOSITORY, COMMIT, &policy_digest));
    let pass = publisher.publish(&fixture.gate, &fixture.signed_gate, &fixture.trust_store, 150, 1)?;
    checks.push(("status_pass", published_state(&pass) == Some(PublishedState::Success), format!("{pass:?}")));
    let idempotent = publisher.publish(&fixture.gate, &fixture.signed_gate, &fixture.trust_store, 150, 1)?;
    checks.push(("status_idempotent", matches!(idempotent, PublishOutcome::Idempotent(_)), format!("{idempotent:?}")));

    let failed_gate = gate_for(&fixture.lock.lock, GateDecision::Fail, true, 200);
    let signed_failed_gate = sign_gate(&failed_gate, &fixture.gate_key);
    let failed = publisher.publish(&failed_gate, &signed_failed_gate, &fixture.trust_store, 250, 2)?;
    checks.push(("status_fail", published_state(&failed) == Some(PublishedState::Failure), format!("{failed:?}")));

    let inconclusive_gate = gate_for(&fixture.lock.lock, GateDecision::Inconclusive, false, 300);
    let signed_inconclusive_gate = sign_gate(&inconclusive_gate, &fixture.gate_key);
    let inconclusive = publisher.publish(&inconclusive_gate, &signed_inconclusive_gate, &fixture.trust_store, 350, 3)?;
    checks.push(("status_inconclusive", published_state(&inconclusive) == Some(PublishedState::ActionRequired), format!("{inconclusive:?}")));

    let stale = publisher.publish(&failed_gate, &signed_failed_gate, &fixture.trust_store, 250, 2);
    checks.push(("status_stale_sequence", matches!(stale, Err(StatusPublishError::StaleRun { .. })), format!("{stale:?}")));
    let sequence_conflict_gate = gate_for(&fixture.lock.lock, GateDecision::Fail, true, 400);
    let signed_sequence_conflict_gate = sign_gate(&sequence_conflict_gate, &fixture.gate_key);
    let sequence_conflict = publisher.publish(&sequence_conflict_gate, &signed_sequence_conflict_gate, &fixture.trust_store, 450, 3);
    checks.push(("status_sequence_conflict", matches!(sequence_conflict, Err(StatusPublishError::SequenceConflict { .. })), format!("{sequence_conflict:?}")));

    let mut changed_lock = fixture.lock.clone();
    changed_lock.lock.backend_version = "7.9.1".into();
    checks.push(rejected("tampered_execution_lock", fixture.verifier.verify(input(&fixture, Some(&changed_lock), None, None, None, None, None, None))));

    let mut changed_runner = fixture.runner.clone();
    changed_runner.artifact_digest[0] ^= 1;
    checks.push(rejected("tampered_runner_measurement", fixture.verifier.verify(input(&fixture, None, Some(&changed_runner), None, None, None, None, None))));

    let mut changed_backend = fixture.backend.clone();
    changed_backend.manifest_digest[0] ^= 1;
    checks.push(rejected("tampered_backend_pack_measurement", fixture.verifier.verify(input(&fixture, None, None, Some(&changed_backend), None, None, None, None))));

    let mut changed_evidence = fixture.evidence.clone();
    changed_evidence.suite_version = "0.2.1".into();
    checks.push(rejected("tampered_conformance_evidence", fixture.verifier.verify(input(&fixture, None, None, None, Some(&changed_evidence), None, None, None))));

    let mut changed_gate = fixture.gate.clone();
    changed_gate.commit = "other-commit".into();
    checks.push(rejected("tampered_gate_attestation", fixture.verifier.verify(input(&fixture, None, None, None, None, Some(&changed_gate), None, None))));

    let wrong_commit = TrustedContextVerifier::new(TrustedContextExpectations { commit: "other-commit".into(), ..fixture.verifier.expected.clone() });
    checks.push(rejected("cross_commit_context", wrong_commit.verify(input(&fixture, None, None, None, None, None, None, None))));

    let expired = TrustMetadata { version: 1, expires_at_epoch_secs: 150, revoked_key_ids: BTreeSet::new() };
    checks.push(rejected("expired_gate_metadata", fixture.verifier.verify(input(&fixture, None, None, None, None, None, None, Some(&expired)))));

    let revoked = TrustMetadata { version: 2, expires_at_epoch_secs: 1000, revoked_key_ids: ["gate-key".into()].into_iter().collect() };
    checks.push(rejected("revoked_gate_key", fixture.verifier.verify(input(&fixture, None, None, None, None, None, None, Some(&revoked)))));

    let incomplete_pass = gate_for(&fixture.lock.lock, GateDecision::Pass, false, 500);
    let signed_incomplete_pass = sign_gate(&incomplete_pass, &fixture.gate_key);
    let incomplete_result = publisher.publish(&incomplete_pass, &signed_incomplete_pass, &fixture.trust_store, 550, 4);
    checks.push((
        "incomplete_pass_not_publishable",
        matches!(incomplete_result, Err(StatusPublishError::IncompletePass)),
        format!("{incomplete_result:?}"),
    ));

    let passed = checks.iter().filter(|(_, passed, _)| *passed).count();
    let mut report = String::from("# Phase 3 Trusted CI Slice Evidence\n\n");
    report.push_str(&format!("- Repository: `{REPOSITORY}`\n- Commit: `{COMMIT}`\n- Platform: `{TRUSTED_PLATFORM}`\n- Runner digest: `{}`\n- Backend pack digest: `{}`\n- Resolver pack digest: `{}`\n- ExecutionLock digest: `{}`\n- Conformance suite digest: `{}`\n- Authority: `TRUSTED_CONTEXT_VERIFIED` locally; no SCM publication\n\n", hex_digest(&fixture.runner.artifact_digest), hex_digest(&fixture.backend.manifest_digest), hex_digest(&fixture.resolver.manifest_digest), fixture.lock.lock.lock_digest, hex_digest(&fixture.report.suite_digest())));
    report.push_str("| Check | Result | Detail |\n| --- | --- | --- |\n");
    for (name, passed, detail) in &checks {
        report.push_str(&format!("| `{name}` | {} | {} |\n", if *passed { "PASS" } else { "FAIL" }, detail.replace('|', "\\|")));
    }
    report.push_str(&format!("\nPassed {passed}/{} checks.\n\nThis harness verifies local trust closure and status mapping only. It does not pull a registry, launch an OCI runtime, call an SCM API, provision credentials, or mutate a protected branch.\n", checks.len()));
    fs::write(Path::new(&output).join("phase3-trusted-ci.md"), report)?;
    println!("trusted-ci={passed}/{} evidence={}/phase3-trusted-ci.md", checks.len(), output);
    if passed != checks.len() {
        return Err("one or more trusted CI checks failed".into());
    }
    Ok(())
}

fn build_fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let runner_key = SigningKey::from_bytes(&[31; 32]);
    let runner_artifact = b"erf-runner-release-phase3-v1";
    let runner_digest: [u8; 32] = Sha256::digest(runner_artifact).into();
    let runner_statement = artifact_statement(runner_digest, RUNNER_RELEASE_ASSET_TYPE, RUNNER_RELEASE_SIGNING_ROLE, "runner-release/v0", 31, 32);
    let signed_runner_statement = runner_statement.sign_with("runner-key", |bytes| runner_key.sign(bytes).to_bytes().to_vec());
    let runner_release = RunnerRelease { version: "runner-0.1.0".into(), platform: LINUX_AMD64.into(), artifact_statement: runner_statement, signed_statement: signed_runner_statement };
    let runner_store = role_store("runner-key", &runner_key, RUNNER_RELEASE_SIGNING_ROLE);
    let runner = RunnerBootstrap::default().verify_and_measure(&runner_release, runner_artifact, &runner_store, None, 100)?;

    let backend = pack_measurement(br#"{"schemaVersion":2,"config":"backend"}"#, BACKEND_RUNTIME_PACK_ASSET_TYPE, 41, 42)?;
    let resolver = pack_measurement(br#"{"schemaVersion":2,"config":"resolver"}"#, WORKSPACE_RESOLVER_PACK_ASSET_TYPE, 43, 44)?;

    let mut suite_config = SuiteConfig::default();
    suite_config.host_program = env::var("ERF_FAKE_HOST").unwrap_or_else(|_| "target/debug/erf-fake-host".into());
    suite_config.cancel_probe_delay_ms = 20;
    let report = run_acceptance(&suite_config);
    if !report.all_passed() {
        return Err("the conformance suite did not pass".into());
    }

    let gate_key = SigningKey::from_bytes(&[51; 32]);
    let evidence_key = SigningKey::from_bytes(&[52; 32]);
    let evidence = ConformanceEvidence::from_report(&report, hex_digest(&runner.artifact_digest), hex_digest(&backend.manifest_digest), hex_digest(&resolver.manifest_digest), SANDBOX_PROVIDER, "e".repeat(64), TRUSTED_PLATFORM)?;
    let signed_evidence = evidence.sign_with("evidence-key", |bytes| evidence_key.sign(bytes).to_bytes().to_vec())?;

    let mut lock = ExecutionLock {
        rule_bundle_digest: "a".repeat(64),
        runner_release_digest: hex_digest(&runner.artifact_digest),
        backend_pack_digest: hex_digest(&backend.manifest_digest),
        resolver_pack_digest: hex_digest(&resolver.manifest_digest),
        sandbox_provider_id: SANDBOX_PROVIDER.into(),
        sandbox_policy_digest: "e".repeat(64),
        conformance_suite_digest: hex_digest(&report.suite_digest()),
        resolver_id: RESOLVER_ID.into(),
        resolver_version: RESOLVER_VERSION.into(),
        backend_id: BACKEND_ID.into(),
        backend_version: BACKEND_VERSION.into(),
        protocol_major: erf_host_client::PROTOCOL_MAJOR,
        canonical_set_version: rf_gate::CANONICAL_SET_VERSION.into(),
        lock_digest: String::new(),
    };
    lock.lock_digest = lock.content_digest();
    let lock_key = SigningKey::from_bytes(&[53; 32]);
    let lock_payload = lock.canonical_payload();
    let signed_lock = SignedExecutionLock {
        asset: SignedAsset { payload_type: DSSE_PAYLOAD_TYPE.into(), payload: lock_payload.clone(), key_id: "lock-key".into(), signature: lock_key.sign(&erf_trust::pae(DSSE_PAYLOAD_TYPE, &lock_payload)).to_bytes().to_vec() },
        lock,
        role: EXECUTION_LOCK_SIGNING_ROLE.into(),
        trust_store: role_store("lock-key", &lock_key, EXECUTION_LOCK_SIGNING_ROLE),
        metadata: None,
    };

    let gate = gate_for(&signed_lock.lock, GateDecision::Pass, true, 100);
    let signed_gate = sign_gate(&gate, &gate_key);
    let mut trust_store = TrustStore::default();
    trust_store.insert_key("evidence-key", evidence_key.verifying_key())?;
    trust_store.authorize_role(CONFORMANCE_SIGNING_ROLE, "evidence-key")?;
    trust_store.insert_key("gate-key", gate_key.verifying_key())?;
    trust_store.authorize_role(GATE_SIGNING_ROLE, "gate-key")?;

    Ok(Fixture {
        lock: signed_lock,
        runner,
        backend,
        resolver,
        evidence,
        signed_evidence,
        gate,
        signed_gate,
        trust_store,
        gate_key,
        verifier: TrustedContextVerifier::new(TrustedContextExpectations { repository: REPOSITORY.into(), commit: COMMIT.into(), policy_digest: "f".repeat(64), backend_id: BACKEND_ID.into(), backend_version: BACKEND_VERSION.into(), resolver_id: RESOLVER_ID.into(), resolver_version: RESOLVER_VERSION.into(), platform: TRUSTED_PLATFORM.into() }),
        report,
    })
}

fn artifact_statement(subject_digest: [u8; 32], asset_type: &str, signing_role: &str, schema_version: &str, provenance: u8, sbom: u8) -> ArtifactStatement {
    ArtifactStatement { subject_digest, asset_type: asset_type.into(), schema_version: schema_version.into(), signing_role: signing_role.into(), provenance_digest: [provenance; 32], sbom_digest: [sbom; 32], compatibility_identity: LINUX_AMD64.into() }
}

fn pack_measurement(manifest: &[u8], asset_type: &str, provenance: u8, sbom: u8) -> Result<PackMeasurement, Box<dyn std::error::Error>> {
    let key = SigningKey::from_bytes(&[provenance; 32]);
    let digest: [u8; 32] = Sha256::digest(manifest).into();
    let statement = artifact_statement(digest, asset_type, RUNTIME_PACK_SIGNING_ROLE, "oci-pack/v0", provenance, sbom);
    let release = OciPackRelease { manifest_media_type: erf_packs::OCI_MANIFEST_MEDIA_TYPE.into(), platform: PACK_LINUX_AMD64.into(), signed_statement: statement.sign_with("pack-key", |bytes| key.sign(bytes).to_bytes().to_vec()), artifact_statement: statement };
    let store = role_store("pack-key", &key, RUNTIME_PACK_SIGNING_ROLE);
    Ok(OciPackVerifier::new(asset_type).verify_and_measure(&release, manifest, &store, None, 100)?)
}

fn role_store(key_id: &str, key: &SigningKey, role: &str) -> TrustStore {
    let mut store = TrustStore::default();
    store.insert_key(key_id, key.verifying_key()).expect("deterministic key is valid");
    store.authorize_role(role, key_id).expect("deterministic role is valid");
    store
}

fn gate_for(lock: &ExecutionLock, decision: GateDecision, complete: bool, issued_at: u64) -> GateAttestation {
    GateAttestation {
        repository: REPOSITORY.into(),
        commit: COMMIT.into(),
        policy_digest: "f".repeat(64),
        rule_bundle_digest: lock.rule_bundle_digest.clone(),
        execution_lock_digest: lock.lock_digest.clone(),
        runner_release_digest: lock.runner_release_digest.clone(),
        workspace_descriptor_digest: "1".repeat(64),
        sandbox_provider_id: lock.sandbox_provider_id.clone(),
        sandbox_policy_digest: lock.sandbox_policy_digest.clone(),
        semantic_result_digest: [9; 32],
        completeness: if complete { Completeness { planned: 1, accounted: 1, failed: 0, missing: 0, observation_errors: 0, complete: true } } else { Completeness { planned: 1, accounted: 0, failed: 0, missing: 1, observation_errors: 0, complete: false } },
        decision,
        exception_grant_digests: Vec::new(),
        issued_at_epoch_secs: issued_at,
        expires_at_epoch_secs: 1000,
    }
}

fn sign_gate(gate: &GateAttestation, key: &SigningKey) -> SignedAsset {
    gate.sign_with("gate-key", |bytes| key.sign(bytes).to_bytes().to_vec())
}

fn input<'a>(fixture: &'a Fixture, lock: Option<&'a SignedExecutionLock>, runner: Option<&'a RunnerMeasurement>, backend: Option<&'a PackMeasurement>, evidence: Option<&'a ConformanceEvidence>, gate: Option<&'a GateAttestation>, conformance_metadata: Option<&'a TrustMetadata>, gate_metadata: Option<&'a TrustMetadata>) -> TrustedContextInput<'a> {
    TrustedContextInput {
        signed_execution_lock: lock.unwrap_or(&fixture.lock),
        runner_measurement: runner.unwrap_or(&fixture.runner),
        backend_pack_measurement: backend.unwrap_or(&fixture.backend),
        resolver_pack_measurement: &fixture.resolver,
        conformance_evidence: evidence.unwrap_or(&fixture.evidence),
        signed_conformance_evidence: &fixture.signed_evidence,
        conformance_metadata,
        gate_attestation: gate.unwrap_or(&fixture.gate),
        signed_gate_attestation: &fixture.signed_gate,
        gate_metadata,
        trust_store: &fixture.trust_store,
        now_epoch_secs: 150,
    }
}

fn rejected<T: std::fmt::Debug>(name: &'static str, result: Result<T, TrustedContextError>) -> (&'static str, bool, String) {
    (name, result.is_err(), format!("{result:?}"))
}

fn published_state(outcome: &PublishOutcome) -> Option<PublishedState> {
    match outcome {
        PublishOutcome::Published(status) | PublishOutcome::Idempotent(status) => Some(status.state),
    }
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
