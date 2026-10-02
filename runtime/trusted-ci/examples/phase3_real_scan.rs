use ed25519_dalek::{Signer, SigningKey};
use erf_bootstrap::{RunnerBootstrap, RunnerRelease, LINUX_AMD64, RUNNER_RELEASE_ASSET_TYPE, RUNNER_RELEASE_SIGNING_ROLE};
use erf_conformance::{run_acceptance, ConformanceEvidence, SuiteConfig, SuiteReport, VERIFICATION_SIGNING_ROLE, TRUSTED_PLATFORM};
use erf_packs::{OciPackRelease, OciPackVerifier, PackMeasurement, BACKEND_RUNTIME_PACK_ASSET_TYPE, OCI_MANIFEST_MEDIA_TYPE, RUNTIME_PACK_SIGNING_ROLE, WORKSPACE_RESOLVER_PACK_ASSET_TYPE};
use erf_status::{PublishedState, StatusPublisher, StatusPublisherContext};
use erf_trust::{ArtifactStatement, SignedAsset, TrustCheckpoint, TrustStore, DSSE_PAYLOAD_TYPE};
use erf_trust_state::{PersistedTrustState, TrustStateFile};
use erf_trusted_ci::{run, TrustedCiInputs, TrustedCiRequest};
use erf_trusted_context::{TrustedContextExpectations, TrustedContextVerifier, EXECUTION_LOCK_SIGNING_ROLE, GATE_SIGNING_ROLE, TRUST_CHECKPOINT_SIGNING_ROLE};
use erf_verification_worker::sign_verified_evidence;
use rf_runner::{ExecutionLock, ScanConfig, SignedExecutionLock};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const REPOSITORY: &str = "org/ruleops-real-scan";
const COMMIT: &str = "phase3-real-scan-commit";
const POLICY_DIGEST: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

struct Materials {
    lock: SignedExecutionLock,
    runner: erf_bootstrap::RunnerMeasurement,
    backend: PackMeasurement,
    resolver: PackMeasurement,
    evidence: ConformanceEvidence,
    signed_evidence: SignedAsset,
    conformance_metadata: erf_trust::TrustMetadata,
    gate_metadata: erf_trust::TrustMetadata,
    trust_checkpoint: TrustCheckpoint,
    signed_trust_checkpoint: SignedAsset,
    persisted_trust_state: PersistedTrustState,
    trust_store: TrustStore,
    verifier: TrustedContextVerifier,
    gate_key: SigningKey,
}

fn main() {
    if let Err(error) = run_demo() {
        eprintln!("phase3 real scan failed: {error}");
        std::process::exit(1);
    }
}

fn run_demo() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let output = PathBuf::from(args.next().ok_or("usage: phase3-real-scan OUT_DIR REPO_ROOT CANDIDATE HOST RESOLVER")?);
    let repo_root = args.next().ok_or("missing REPO_ROOT")?;
    let candidate = args.next().ok_or("missing CANDIDATE")?;
    let host = args.next().ok_or("missing HOST")?;
    let resolver = args.next().ok_or("missing RESOLVER")?;
    let repository = args.next().unwrap_or_else(|| REPOSITORY.into());
    let commit = args.next().unwrap_or_else(|| COMMIT.into());
    fs::create_dir_all(&output)?;
    prepare_sandbox_fixture(&output)?;
    let candidate_digest = manifest_digest(Path::new(&candidate))?;

    let mut suite_config = SuiteConfig::default();
    suite_config.host_program = host.clone();
    suite_config.resolver_program = resolver.clone();
    suite_config.cancel_probe_delay_ms = 20;
    let report = run_acceptance(&suite_config);
    if !report.all_passed() {
        return Err("the shared conformance suite failed".into());
    }
    let materials = build_materials(&candidate_digest, &report, &repository, &commit, &output, &repo_root, &candidate, &host, &resolver)?;
    let exceptions = BTreeSet::new();
    let issued = current_epoch_secs()?;

    let scan = scan_config(&output, &repo_root, &candidate, &host, &resolver, &materials.lock, false);
    let mut publisher = StatusPublisher::new(StatusPublisherContext::new(&repository, &commit, POLICY_DIGEST));
    let first = run(request(&scan, &materials, &exceptions, &repository, &commit, issued, 1), &mut publisher, |bytes| materials.gate_key.sign(bytes).to_bytes().to_vec())?;

    let mut repeat_identities = vec![(first.scan.plan_digest.clone(), first.scan.workspace_descriptor_digest.clone(), first.preview.semantic_result_digest)];
    for _ in 1..10 {
        let repeat_scan = scan_config(&output, &repo_root, &candidate, &host, &resolver, &materials.lock, false);
        let mut repeat_publisher = StatusPublisher::new(StatusPublisherContext::new(&repository, &commit, POLICY_DIGEST));
        let repeat = run(request(&repeat_scan, &materials, &exceptions, &repository, &commit, issued, 1), &mut repeat_publisher, |bytes| materials.gate_key.sign(bytes).to_bytes().to_vec())?;
        repeat_identities.push((repeat.scan.plan_digest, repeat.scan.workspace_descriptor_digest, repeat.preview.semantic_result_digest));
    }

    // Keep the sandbox policy/ExecutionLock identity unchanged for the
    // negative run; the fixture injects the crash through its inherited env.
    env::set_var("FAKE_CRASH_ON_EXECUTE", "1");
    let failed_scan = scan_config(&output, &repo_root, &candidate, &host, &resolver, &materials.lock, true);
    let mut failed_publisher = StatusPublisher::new(StatusPublisherContext::new(&repository, &commit, POLICY_DIGEST));
    let failed = run(request(&failed_scan, &materials, &exceptions, &repository, &commit, issued, 1), &mut failed_publisher, |bytes| materials.gate_key.sign(bytes).to_bytes().to_vec())?;
    env::remove_var("FAKE_CRASH_ON_EXECUTE");

    let same_digest = repeat_identities.iter().all(|(plan, workspace, digest)| plan == &repeat_identities[0].0 && workspace == &repeat_identities[0].1 && digest == &repeat_identities[0].2);
    let fail_status = matches!(first.published, erf_status::PublishOutcome::Published(ref status) if status.state == PublishedState::Failure);
    let incomplete = failed.scan.attempt_failure.is_some() && !failed.preview.completeness.complete && failed.preview.decision == rf_gate::GateDecision::Inconclusive;
    let action_required = matches!(failed.published, erf_status::PublishOutcome::Published(ref status) if status.state == PublishedState::ActionRequired);
    let unified_trusted = first.unified_result.trust == erf_unified_result::ResultTrust::Trusted
        && first.unified_result.decision == first.gate_attestation.decision
        && first.unified_result.status == erf_unified_result::ResultStatus::from_decision(first.preview.decision);
    let unified_audit_complete = first.unified_result.audit.events().len() == 6 && first.unified_result.validate().is_ok();
    let checks = [("real_scan_is_complete", first.scan.attempt_failure.is_none() && first.preview.completeness.complete), ("real_finding_maps_to_failure", fail_status), ("same_snapshot_ten_repeatable", repeat_identities.len() == 10 && same_digest), ("failed_scan_is_inconclusive", incomplete), ("failed_scan_status_action_required", action_required), ("unified_result_mirrors_trusted_handoff", unified_trusted), ("unified_result_audit_chain_is_complete", unified_audit_complete)];
    let passed = checks.iter().filter(|(_, ok)| *ok).count();
    let mut evidence = format!("# Phase 3 Real Runner Scan Evidence\n\n- Repository: `{repository}`\n- Commit: `{commit}`\n- Candidate digest: `{candidate_digest}`\n- Conformance suite digest: `{}`\n- Repeat count: `{}`\n- First plan digest: `{}`\n- First semantic result digest: `{}`\n- Last semantic result digest: `{}`\n- Failure scan: `{}`\n\n| Check | Result |\n| --- | --- |\n", hex_digest(&report.suite_digest()), repeat_identities.len(), hex_digest_bytes(&first.scan.plan_digest), hex_digest(&first.preview.semantic_result_digest), hex_digest(&repeat_identities.last().expect("ten repeats").2), failed.scan.attempt_failure.as_deref().unwrap_or("no failure"));
    for (name, ok) in checks {
        evidence.push_str(&format!("| `{name}` | {} |\n", if ok { "PASS" } else { "FAIL" }));
    }
    evidence.push_str(&format!("\nPassed {passed}/{} checks.\n\nThe coordinator derives GateDecision and Completeness from the real ScanResult, then crosses TrustedContext before the status model and builds the UnifiedResult from the same trusted handoff. It does not hold SCM credentials.\n", checks.len()));
    fs::write(output.join("phase3-real-scan.md"), evidence)?;
    println!("real-scan={passed}/{} evidence={}/phase3-real-scan.md", checks.len(), output.display());
    if passed != checks.len() {
        return Err("real runner scan acceptance failed".into());
    }
    Ok(())
}

fn request<'a>(scan: &'a ScanConfig, materials: &'a Materials, exceptions: &'a BTreeSet<String>, repository: &str, commit: &str, issued: u64, sequence: u64) -> TrustedCiRequest<'a> {
    TrustedCiRequest {
        scan,
        context: TrustedCiInputs {
            signed_execution_lock: &materials.lock,
            runner_measurement: &materials.runner,
            backend_pack_measurement: &materials.backend,
            resolver_pack_measurement: &materials.resolver,
            conformance_evidence: &materials.evidence,
            signed_conformance_evidence: &materials.signed_evidence,
            conformance_metadata: Some(&materials.conformance_metadata),
            gate_metadata: Some(&materials.gate_metadata),
            trust_checkpoint: &materials.trust_checkpoint,
            signed_trust_checkpoint: &materials.signed_trust_checkpoint,
            persisted_trust_state: &materials.persisted_trust_state,
            trust_checkpoint_role: TRUST_CHECKPOINT_SIGNING_ROLE,
            trust_store: &materials.trust_store,
            verifier: &materials.verifier,
        },
        repository: repository.into(),
        commit: commit.into(),
        policy_digest: POLICY_DIGEST.into(),
        baseline: None,
        exception_fingerprints: exceptions,
        gate_policy: None,
        exception_grant_digests: Vec::new(),
        gate_key_id: "gate-key".into(),
        issued_at_epoch_secs: issued,
        expires_at_epoch_secs: issued.saturating_add(3600),
        run_sequence: sequence,
        workload_issuer: "https://issuer.example".into(),
        workload_audience: "erf-gate".into(),
        workflow: "phase3-real-scan".into(),
        run_identity: format!("run-{sequence}"),
        environment_identity: "linux/amd64".into(),
    }
}

fn scan_config(output: &Path, repo_root: &str, candidate: &str, host: &str, resolver: &str, lock: &SignedExecutionLock, crash: bool) -> ScanConfig {
    let (host_command, host_args) = if crash { ("env".into(), vec!["FAKE_CRASH_ON_EXECUTE=1".into(), host.into()]) } else { (host.into(), Vec::new()) };
    let (host_policy, resolver_policy) = sandbox_policies(output, repo_root, candidate, host, resolver);
    ScanConfig {
        repo_root: repo_root.into(),
        rule_id: "corp.java.no-system-exit".into(),
        rule_bundle: Some(candidate.into()),
        bundle_delivery: None,
        resolver_command: resolver.into(),
        resolver_args: Vec::new(),
        host_command,
        host_args,
        host_deadline_ms: 2_000,
        resolver_deadline_ms: 2_000,
        max_staging_bytes: 64 * 1024 * 1024,
        execution_lock: None,
        signed_execution_lock: Some(lock.clone()),
        resolver_sandbox_policy: Some(resolver_policy),
        sandbox_policy: Some(host_policy),
        workspace_attestation: None,
        frozen_plan: None,
        incremental: None,
        scan_cache_dir: None,
        fallback_inputs: None,
        project_scope: None,
    }
}

fn sandbox_policies(output: &Path, repo_root: &str, candidate: &str, host: &str, resolver: &str) -> (erf_sandbox::SandboxPolicy, erf_sandbox::SandboxPolicy) {
    let repository_root = PathBuf::from(repo_root);
    let rule_bundle = PathBuf::from(candidate);
    let seccomp_profile = output.join("sandbox-fixture/seccomp.json");
    let common = |entrypoint: String, entrypoint_args: Vec<String>| erf_sandbox::SandboxPolicy {
        image: "erf/test-java-host:fixture".into(),
        entrypoint,
        entrypoint_args,
        repository_root: repository_root.clone(),
        rule_bundle: rule_bundle.clone(),
        seccomp_profile: seccomp_profile.clone(),
        memory_mb: 512,
        cpus_millis: 1000,
        pids_limit: 64,
    };
    (common(host.into(), vec!["--stdio".into()]), common(resolver.into(), Vec::new()))
}

/// The phase-3 acceptance uses a tiny local Docker façade so the deterministic
/// fixture still exercises the Runner's mandatory sandbox path without
/// requiring a real container daemon. Production Trusted CI always uses the
/// real rootless provider and fails closed when its preflight is unavailable.
fn prepare_sandbox_fixture(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = output.join("sandbox-fixture");
    fs::create_dir_all(&fixture)?;
    fs::write(fixture.join("seccomp.json"), b"{}")?;
    let docker = fixture.join("docker");
    fs::write(
        &docker,
        r#"#!/bin/sh
if [ "$1" = "info" ]; then
  printf '%s' '[{"Name":"rootless"}]'
  exit 0
fi
[ "$1" = "run" ] || exit 2
repo=""
bundle=""
image_seen=0
entrypoint=""
while [ "$#" -gt 0 ]; do
  arg="$1"
  shift
  case "$arg" in
    type=bind,src=*,dst=/workspace,readonly)
      repo="${arg#type=bind,src=}"
      repo="${repo%,dst=/workspace,readonly}"
      ;;
    type=bind,src=*,dst=/rule-bundle,readonly)
      bundle="${arg#type=bind,src=}"
      bundle="${bundle%,dst=/rule-bundle,readonly}"
      ;;
    erf/test-java-host:fixture)
      image_seen=1
      ;;
    *)
      if [ "$image_seen" = 1 ] && [ -z "$entrypoint" ]; then
        entrypoint="$arg"
        break
      fi
      ;;
  esac
done
[ -n "$repo" ] && [ -n "$bundle" ] && [ -n "$entrypoint" ] || exit 2
export ERF_REPO_ROOT="$repo"
export ERF_RULE_BUNDLE="$bundle"
export ERF_RULESET="$bundle/ruleset.xml"
exec "$entrypoint" "$@"
"#,
    )?;
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&docker)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&docker, permissions)?;
    }
    let current_path = env::var_os("PATH").unwrap_or_default();
    let mut path = OsString::from(fixture);
    path.push(":");
    path.push(current_path);
    env::set_var("PATH", path);
    Ok(())
}

fn build_materials(candidate_digest: &str, report: &SuiteReport, repository: &str, commit: &str, output: &Path, repo_root: &str, candidate: &str, host: &str, resolver_command: &str) -> Result<Materials, Box<dyn std::error::Error>> {
    let now = current_epoch_secs()?;
    let runner_key = SigningKey::from_bytes(&[61; 32]);
    let runner_artifact = b"erf-runner-real-scan-v1";
    let runner_subject: [u8; 32] = Sha256::digest(runner_artifact).into();
    let runner_statement = statement(runner_subject, RUNNER_RELEASE_ASSET_TYPE, RUNNER_RELEASE_SIGNING_ROLE, "runner-release/v0", 61, 62);
    let runner_release = RunnerRelease { version: "runner-0.1.0".into(), platform: LINUX_AMD64.into(), artifact_statement: runner_statement.clone(), signed_statement: runner_statement.sign_with("runner-key", |bytes| runner_key.sign(bytes).to_bytes().to_vec()) };
    let runner = RunnerBootstrap::default().verify_and_measure(&runner_release, runner_artifact, &role_store("runner-key", &runner_key, RUNNER_RELEASE_SIGNING_ROLE), None, now)?;
    let backend = pack(b"{\"pack\":\"backend-real-scan\"}", BACKEND_RUNTIME_PACK_ASSET_TYPE, 63, 64)?;
    let resolver_pack = pack(b"{\"pack\":\"resolver-real-scan\"}", WORKSPACE_RESOLVER_PACK_ASSET_TYPE, 65, 66)?;
    let evidence_key = SigningKey::from_bytes(&[67; 32]);
    let gate_key = SigningKey::from_bytes(&[68; 32]);
    let checkpoint_key = SigningKey::from_bytes(&[70; 32]);
    let (host_policy, resolver_policy) = sandbox_policies(output, repo_root, candidate, host, resolver_command);
    let provider = erf_sandbox::DockerSandboxProvider::default();
    let prepared_host = provider.prepare(&host_policy)?;
    let prepared_resolver = provider.prepare(&resolver_policy)?;
    let evidence = ConformanceEvidence::from_report(report, hex_digest(&report.suite_digest()), "b".repeat(64), hex_digest(&runner.artifact_digest), hex_digest(&backend.manifest_digest), hex_digest(&resolver_pack.manifest_digest), prepared_host.provider_id, prepared_host.policy_digest.clone(), prepared_resolver.provider_id, prepared_resolver.policy_digest.clone(), TRUSTED_PLATFORM)?;
    let signed_evidence = sign_verified_evidence(&evidence, "evidence-key", |bytes| evidence_key.sign(bytes).to_bytes().to_vec())?;
    let trust_checkpoint = TrustCheckpoint {
        repository: repository.into(),
        version: 1,
        role_versions: BTreeMap::from([(String::from(TRUST_CHECKPOINT_SIGNING_ROLE), 1)]),
        snapshot_digest: [71; 32],
        issued_at_epoch_secs: now.saturating_sub(1),
        expires_at_epoch_secs: now.saturating_add(3600),
    };
    let signed_trust_checkpoint = trust_checkpoint.sign_with("checkpoint-key", |bytes| checkpoint_key.sign(bytes).to_bytes().to_vec());
    let mut lock = ExecutionLock { rule_bundle_digest: candidate_digest.into(), runner_release_digest: hex_digest(&runner.artifact_digest), backend_pack_digest: hex_digest(&backend.manifest_digest), resolver_pack_digest: hex_digest(&resolver_pack.manifest_digest), sandbox_provider_id: prepared_host.provider_id.into(), sandbox_policy_digest: prepared_host.policy_digest, resolver_sandbox_provider_id: prepared_resolver.provider_id.into(), resolver_sandbox_policy_digest: prepared_resolver.policy_digest, conformance_suite_digest: hex_digest(&report.suite_digest()), resolver_id: "erf-fake-resolver".into(), resolver_version: "0.1.0".into(), backend_id: "fake-backend".into(), backend_version: "0.1.0".into(), protocol_major: erf_host_client::PROTOCOL_MAJOR, canonical_set_version: rf_gate::CANONICAL_SET_VERSION.into(), lock_digest: String::new() };
    lock.lock_digest = lock.content_digest();
    let lock_key = SigningKey::from_bytes(&[69; 32]);
    let lock_payload = lock.canonical_payload();
    let signed_lock = SignedExecutionLock { lock, asset: SignedAsset { payload_type: DSSE_PAYLOAD_TYPE.into(), payload: lock_payload.clone(), key_id: "lock-key".into(), signature: lock_key.sign(&erf_trust::pae(DSSE_PAYLOAD_TYPE, &lock_payload)).to_bytes().to_vec() }, role: EXECUTION_LOCK_SIGNING_ROLE.into(), trust_store: role_store("lock-key", &lock_key, EXECUTION_LOCK_SIGNING_ROLE), metadata: None };
    let mut trust_store = TrustStore::default();
    trust_store.insert_key("evidence-key", evidence_key.verifying_key())?;
    trust_store.authorize_role(VERIFICATION_SIGNING_ROLE, "evidence-key")?;
    trust_store.insert_key("gate-key", gate_key.verifying_key())?;
    trust_store.authorize_role(GATE_SIGNING_ROLE, "gate-key")?;
    trust_store.insert_key("checkpoint-key", checkpoint_key.verifying_key())?;
    trust_store.authorize_role(TRUST_CHECKPOINT_SIGNING_ROLE, "checkpoint-key")?;
    let trust_metadata = erf_trust::TrustMetadata { version: 1, expires_at_epoch_secs: now.saturating_add(3600), revoked_key_ids: BTreeSet::new() };
    let trust_state_file = TrustStateFile::new(output.join("trust-state.bin"));
    if trust_state_file.load()?.is_none() {
        trust_state_file.initialize_signed_checkpoint(&trust_checkpoint, &signed_trust_checkpoint, &trust_store, TRUST_CHECKPOINT_SIGNING_ROLE, now)?;
    } else {
        trust_state_file.advance_signed_checkpoint(&trust_checkpoint, &signed_trust_checkpoint, &trust_store, TRUST_CHECKPOINT_SIGNING_ROLE, now)?;
    }
    let persisted_trust_state = trust_state_file.load_for_trusted_execution()?;
    Ok(Materials { lock: signed_lock, runner, backend, resolver: resolver_pack, evidence, signed_evidence, conformance_metadata: trust_metadata.clone(), gate_metadata: trust_metadata, trust_checkpoint, signed_trust_checkpoint, persisted_trust_state, trust_store, verifier: TrustedContextVerifier::new(TrustedContextExpectations { repository: repository.into(), commit: commit.into(), policy_digest: POLICY_DIGEST.into(), backend_id: "fake-backend".into(), backend_version: "0.1.0".into(), resolver_id: "erf-fake-resolver".into(), resolver_version: "0.1.0".into(), platform: TRUSTED_PLATFORM.into() }), gate_key })
}

fn statement(subject: [u8; 32], asset_type: &str, role: &str, schema: &str, provenance: u8, sbom: u8) -> ArtifactStatement {
    ArtifactStatement { subject_digest: subject, asset_type: asset_type.into(), schema_version: schema.into(), signing_role: role.into(), provenance_digest: [provenance; 32], sbom_digest: [sbom; 32], compatibility_identity: LINUX_AMD64.into() }
}

fn pack(manifest: &[u8], asset_type: &str, provenance: u8, sbom: u8) -> Result<PackMeasurement, Box<dyn std::error::Error>> {
    let key = SigningKey::from_bytes(&[provenance; 32]);
    let subject: [u8; 32] = Sha256::digest(manifest).into();
    let artifact = statement(subject, asset_type, RUNTIME_PACK_SIGNING_ROLE, "oci-pack/v0", provenance, sbom);
    let release = OciPackRelease { manifest_media_type: OCI_MANIFEST_MEDIA_TYPE.into(), platform: LINUX_AMD64.into(), artifact_statement: artifact.clone(), signed_statement: artifact.sign_with("pack-key", |bytes| key.sign(bytes).to_bytes().to_vec()) };
    Ok(OciPackVerifier::new(asset_type).verify_and_measure(&release, manifest, &role_store("pack-key", &key, RUNTIME_PACK_SIGNING_ROLE), None, 100)?)
}

fn role_store(key_id: &str, key: &SigningKey, role: &str) -> TrustStore {
    let mut store = TrustStore::default();
    store.insert_key(key_id, key.verifying_key()).expect("deterministic key is valid");
    store.authorize_role(role, key_id).expect("deterministic role is valid");
    store
}

fn manifest_digest(candidate: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(candidate.join("manifest.json"))?;
    let marker = "\"digest\"";
    let value = text.split_once(marker).and_then(|(_, rest)| rest.split_once(':')).and_then(|(_, rest)| rest.trim().strip_prefix('"')).and_then(|rest| rest.split_once('"').map(|(value, _)| value.to_string())).ok_or("candidate manifest has no digest")?;
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) { return Err("candidate manifest digest is invalid".into()); }
    Ok(value)
}

fn hex_digest(bytes: &[u8; 32]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }
fn hex_digest_bytes(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

fn current_epoch_secs() -> Result<u64, Box<dyn std::error::Error>> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
