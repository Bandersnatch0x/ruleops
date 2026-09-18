//! Minimal scan orchestration slice for Phase 1.

use erf_contracts::{backend::v0 as host_pb, workspace::v0 as workspace_pb};
use erf_host_client::{ExecEvent, HostConfig, HostHandle, HostError};
use erf_resolver_client::{ResolverConfig, ResolverHandle, ResolverError};
use prost::Message;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::process::Command;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("workspace resolution failed: {0}")]
    Resolve(#[from] ResolverError),
    #[error("backend execution failed: {0}")]
    Host(#[from] HostError),
    #[error("scan plan violation: {0}")]
    Plan(String),
    #[error("rule bundle invalid: {0}")]
    RuleBundle(String),
    #[error("trust verification failed: {0}")]
    Trust(String),
    #[error("sandbox execution failed: {0}")]
    Sandbox(String),
}

#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub repo_root: String,
    pub rule_id: String,
    /// Local Phase 1 Candidate directory used as the unsigned RuleBundle.
    pub rule_bundle: Option<String>,
    pub resolver_command: String,
    pub resolver_args: Vec<String>,
    pub host_command: String,
    pub host_args: Vec<String>,
    pub host_deadline_ms: u64,
    pub max_staging_bytes: u64,
    pub execution_lock: Option<ExecutionLock>,
    pub signed_execution_lock: Option<SignedExecutionLock>,
    pub sandbox_policy: Option<erf_sandbox::SandboxPolicy>,
}

/// Deterministic execution identity used by the Phase 3 trusted slice. The
/// current local implementation verifies the lock binding but has no signing
/// authority; callers therefore remain LOCAL_UNATTESTED.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionLock {
    pub rule_bundle_digest: String,
    pub runner_release_digest: String,
    pub backend_pack_digest: String,
    pub resolver_pack_digest: String,
    pub sandbox_provider_id: String,
    pub sandbox_policy_digest: String,
    pub conformance_suite_digest: String,
    pub resolver_id: String,
    pub resolver_version: String,
    pub backend_id: String,
    pub backend_version: String,
    pub protocol_major: u32,
    pub canonical_set_version: String,
    pub lock_digest: String,
}

#[derive(Debug, Clone)]
pub struct SignedExecutionLock {
    pub lock: ExecutionLock,
    pub asset: erf_trust::SignedAsset,
    pub role: String,
    pub trust_store: erf_trust::TrustStore,
    pub metadata: Option<erf_trust::TrustMetadata>,
}

impl ExecutionLock {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-execution-lock-payload-v0\0".to_vec();
        for value in [
            self.rule_bundle_digest.as_str(),
            self.runner_release_digest.as_str(),
            self.backend_pack_digest.as_str(),
            self.resolver_pack_digest.as_str(),
            self.sandbox_provider_id.as_str(),
            self.sandbox_policy_digest.as_str(),
            self.conformance_suite_digest.as_str(),
            self.resolver_id.as_str(),
            self.resolver_version.as_str(),
            self.backend_id.as_str(),
            self.backend_version.as_str(),
            &self.protocol_major.to_string(),
            self.canonical_set_version.as_str(),
            self.lock_digest.as_str(),
        ] {
            payload.extend_from_slice(&(value.len() as u64).to_be_bytes());
            payload.extend_from_slice(value.as_bytes());
        }
        payload
    }

    pub fn content_digest(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(b"erf-execution-lock-v0\0");
        for value in [
            self.rule_bundle_digest.as_str(),
            self.runner_release_digest.as_str(),
            self.backend_pack_digest.as_str(),
            self.resolver_pack_digest.as_str(),
            self.sandbox_provider_id.as_str(),
            self.sandbox_policy_digest.as_str(),
            self.conformance_suite_digest.as_str(),
            self.resolver_id.as_str(),
            self.resolver_version.as_str(),
            self.backend_id.as_str(),
            self.backend_version.as_str(),
            &self.protocol_major.to_string(),
            self.canonical_set_version.as_str(),
        ] {
            digest.update((value.len() as u64).to_be_bytes());
            digest.update(value.as_bytes());
        }
        hex_digest(&digest.finalize())
    }

    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("rule_bundle_digest", &self.rule_bundle_digest),
            ("runner_release_digest", &self.runner_release_digest),
            ("backend_pack_digest", &self.backend_pack_digest),
            ("resolver_pack_digest", &self.resolver_pack_digest),
            ("sandbox_policy_digest", &self.sandbox_policy_digest),
            ("conformance_suite_digest", &self.conformance_suite_digest),
        ] {
            if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("execution lock has invalid {name}"));
            }
        }
        for (name, value) in [
            ("resolver_id", &self.resolver_id),
            ("resolver_version", &self.resolver_version),
            ("backend_id", &self.backend_id),
            ("backend_version", &self.backend_version),
            ("sandbox_provider_id", &self.sandbox_provider_id),
        ] {
            if value.is_empty() { return Err(format!("execution lock has empty {name}")); }
        }
        if self.protocol_major != erf_host_client::PROTOCOL_MAJOR {
            return Err(format!("execution lock protocol major {} does not match runtime {}", self.protocol_major, erf_host_client::PROTOCOL_MAJOR));
        }
        if self.canonical_set_version != rf_gate::CANONICAL_SET_VERSION {
            return Err(format!("execution lock canonical set {} does not match runtime {}", self.canonical_set_version, rf_gate::CANONICAL_SET_VERSION));
        }
        if self.lock_digest != self.content_digest() {
            return Err("execution lock content digest mismatch".into());
        }
        Ok(())
    }

    /// Reads the reviewable key=value lock representation used by the local
    /// CLI. Unknown or duplicate keys are rejected to keep the format closed.
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let values = read_lock_values(path, false)?;
        let lock = lock_from_values(&values)?;
        lock.validate()?;
        Ok(lock)
    }
}

impl SignedExecutionLock {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let values = read_lock_values(path, true)?;
        let lock = lock_from_values(&values)?;
        let get = |key: &str| values.get(key).cloned().ok_or_else(|| format!("missing signed execution lock key: {key}"));
        let public_key = decode_hex(&get("public_key_hex")?)?;
        let signature = decode_hex(&get("signature_hex")?)?;
        let mut trust_store = erf_trust::TrustStore::default();
        trust_store.insert_public_key_bytes(get("key_id")?, &public_key).map_err(|error| error.to_string())?;
        let role = get("role")?;
        trust_store.authorize_role(&role, get("key_id")?).map_err(|error| error.to_string())?;
        let metadata = match (values.get("metadata_version"), values.get("expires_at_epoch_secs")) {
            (None, None) => None,
            (Some(version), Some(expires)) => {
                let version = version.parse().map_err(|_| "invalid trust metadata version".to_string())?;
                let expires_at_epoch_secs = expires.parse().map_err(|_| "invalid trust metadata expiry".to_string())?;
                let revoked_key_ids = values.get("revoked_key_ids").map(|value| value.split(',').map(str::trim).filter(|id| !id.is_empty()).map(str::to_string).collect()).unwrap_or_default();
                Some(erf_trust::TrustMetadata { version, expires_at_epoch_secs, revoked_key_ids })
            }
            _ => return Err("signed execution lock metadata requires version and expiry".into()),
        };
        Ok(Self {
            asset: erf_trust::SignedAsset { payload_type: erf_trust::DSSE_PAYLOAD_TYPE.into(), payload: lock.canonical_payload(), key_id: get("key_id")?, signature },
            lock,
            role,
            trust_store,
            metadata,
        })
    }

    pub fn verify(&self) -> Result<(), String> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|error| format!("trust clock failed: {error}"))?.as_secs();
        self.verify_at(now)
    }

    pub fn verify_at(&self, now_epoch_secs: u64) -> Result<(), String> {
        self.lock.validate()?;
        if self.asset.payload != self.lock.canonical_payload() {
            return Err("signed execution lock payload does not match lock".into());
        }
        if let Some(metadata) = &self.metadata {
            self.trust_store.verify_with_metadata(&self.asset, &self.role, metadata, now_epoch_secs).map_err(|error| error.to_string())?;
        } else {
            self.trust_store.verify(&self.asset, &self.role).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

fn read_lock_values(path: &Path, signed: bool) -> Result<std::collections::BTreeMap<String, String>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("cannot read execution lock: {e}"))?;
    let mut values = std::collections::BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let (key, value) = line.split_once('=').ok_or_else(|| format!("invalid execution lock line: {line}"))?;
        let allowed = matches!(key.trim(), "rule_bundle_digest" | "runner_release_digest" | "backend_pack_digest" | "resolver_pack_digest" | "sandbox_provider_id" | "sandbox_policy_digest" | "conformance_suite_digest" | "resolver_id" | "resolver_version" | "backend_id" | "backend_version" | "protocol_major" | "canonical_set_version" | "lock_digest")
            || signed && matches!(key.trim(), "key_id" | "role" | "public_key_hex" | "signature_hex" | "metadata_version" | "expires_at_epoch_secs" | "revoked_key_ids");
        if !allowed { return Err(format!("unknown execution lock key: {key}")); }
        if values.insert(key.trim().to_string(), value.trim().to_string()).is_some() { return Err(format!("duplicate execution lock key: {key}")); }
    }
    Ok(values)
}

fn lock_from_values(values: &std::collections::BTreeMap<String, String>) -> Result<ExecutionLock, String> {
    let get = |key: &str| values.get(key).cloned().ok_or_else(|| format!("missing execution lock key: {key}"));
    let protocol_major = get("protocol_major")?.parse().map_err(|_| "invalid execution lock protocol_major".to_string())?;
    Ok(ExecutionLock { rule_bundle_digest: get("rule_bundle_digest")?, runner_release_digest: get("runner_release_digest")?, backend_pack_digest: get("backend_pack_digest")?, resolver_pack_digest: get("resolver_pack_digest")?, sandbox_provider_id: get("sandbox_provider_id")?, sandbox_policy_digest: get("sandbox_policy_digest")?, conformance_suite_digest: get("conformance_suite_digest")?, resolver_id: get("resolver_id")?, resolver_version: get("resolver_version")?, backend_id: get("backend_id")?, backend_version: get("backend_version")?, protocol_major, canonical_set_version: get("canonical_set_version")?, lock_digest: get("lock_digest")? })
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if value.len() % 2 != 0 { return Err("hex value has odd length".into()); }
    (0..value.len()).step_by(2).map(|index| u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| "invalid hex value".to_string())).collect()
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub rule_id: String,
    pub planned: usize,
    pub executed: usize,
    pub failed: usize,
    pub attempt_failure: Option<String>,
    pub committed_findings: Vec<host_pb::BackendObservation>,
    pub attempt_transcript_digest: Vec<u8>,
    pub trust: &'static str,
    pub plan_digest: Vec<u8>,
    pub workspace_descriptor_digest: Vec<u8>,
    pub planned_unit_ids: Vec<String>,
    pub unit_statuses: Vec<host_pb::UnitStatus>,
}

/// Runs one scan attempt. Findings are moved into the result only after every
/// planned unit has exactly one terminal status and ExecuteComplete agrees.
pub fn scan(config: &ScanConfig) -> Result<ScanResult, ScanError> {
    if let Some(signed_lock) = &config.signed_execution_lock {
        signed_lock.verify().map_err(ScanError::Trust)?;
        if let Some(unsigned_lock) = &config.execution_lock {
            if unsigned_lock != &signed_lock.lock {
                return Err(ScanError::Trust("unsigned and signed execution locks differ".into()));
            }
        }
    }
    let execution_lock = config.signed_execution_lock.as_ref().map(|lock| &lock.lock).or(config.execution_lock.as_ref());
    let validated_bundle = if let Some(bundle) = &config.rule_bundle {
        Some(validate_rule_bundle(Path::new(bundle))?)
    } else {
        None
    };
    let rule_id = validated_bundle
        .as_ref()
        .map(|bundle| bundle.rule_id.as_str())
        .unwrap_or(config.rule_id.as_str())
        .to_string();
    if rule_id.trim().is_empty() {
        return Err(ScanError::Plan("scan rule identity is empty".into()));
    }
    if let Some(lock) = execution_lock {
        lock.validate().map_err(ScanError::Trust)?;
        if validated_bundle.as_ref().map(|bundle| bundle.digest.as_str()) != Some(lock.rule_bundle_digest.as_str()) {
            return Err(ScanError::Trust("execution lock does not bind the supplied rule bundle".into()));
        }
    }
    let mut resolver_command = Command::new(&config.resolver_command);
    resolver_command.args(&config.resolver_args);
    let resolver_config = ResolverConfig {
        required_capabilities: vec!["ecosystem.static-layout".into()],
        ..Default::default()
    };
    let (mut resolver, _) = ResolverHandle::spawn(&mut resolver_command, &resolver_config)?;
    let descriptor = resolver.resolve(&config.repo_root)?;
    if let Some(lock) = execution_lock {
        if descriptor.resolver_id != lock.resolver_id || descriptor.resolver_version != lock.resolver_version {
            return Err(ScanError::Trust(format!("execution lock resolver mismatch: expected {}/{} got {}/{}", lock.resolver_id, lock.resolver_version, descriptor.resolver_id, descriptor.resolver_version)));
        }
    }
    let _ = resolver.complete()?;

    // Freeze the global denominator before asking the host to refine it.
    let units: Vec<host_pb::AnalysisUnit> = descriptor
        .modules
        .iter()
        .map(|module| host_pb::AnalysisUnit {
            unit_id: format!("{}:{}", module.module_id, rule_id),
            language: module.language.clone(),
            files: module.files.iter().map(|f| f.path.clone()).collect(),
        })
        .collect();
    let planned_ids: Vec<String> = units.iter().map(|u| u.unit_id.clone()).collect();
    let planned = planned_ids.len();
    let plan_digest = digest_plan(&units);

    let sandboxed = config.sandbox_policy.is_some();
    let mut host_command = if let Some(policy) = &config.sandbox_policy {
        if policy.repository_root != Path::new(&config.repo_root)
            || config.rule_bundle.as_ref().map(Path::new) != Some(policy.rule_bundle.as_path())
        {
            return Err(ScanError::Sandbox("sandbox mounts do not match scan inputs".into()));
        }
        let provider = erf_sandbox::DockerSandboxProvider::default();
        provider.preflight().map_err(|error| ScanError::Sandbox(error.to_string()))?;
        let prepared = provider.prepare(policy).map_err(|error| ScanError::Sandbox(error.to_string()))?;
        let mut command = Command::new(&prepared.command[0]);
        command.args(&prepared.command[1..]);
        command
    } else {
        let mut command = Command::new(&config.host_command);
        command.args(&config.host_args);
        command
    };
    // The descriptor paths are relative to the resolved snapshot, not the
    // runtime process cwd. Bind that identity into the host session.
    if !sandboxed {
        host_command.env("ERF_REPO_ROOT", &config.repo_root);
        if let Some(bundle) = &validated_bundle {
            host_command.env("ERF_RULESET", &bundle.ruleset_path);
            host_command.env("ERF_RULE_BUNDLE", config.rule_bundle.as_deref().unwrap_or_default());
        }
    }
    let mut host_config = HostConfig {
        required_capabilities: vec!["java.ast".into()],
        ..Default::default()
    };
    host_config.limits.deadline_ms = config.host_deadline_ms;
    let (mut host, ack) = HostHandle::spawn(&mut host_command, &host_config)?;
    if let Some(lock) = execution_lock {
        let capabilities = ack.capabilities.as_ref().ok_or_else(|| ScanError::Plan("host ack omitted capabilities for execution lock".into()))?;
        if capabilities.backend_id != lock.backend_id || capabilities.backend_version != lock.backend_version {
            return Err(ScanError::Trust(format!("execution lock backend mismatch: expected {}/{} got {}/{}", lock.backend_id, lock.backend_version, capabilities.backend_id, capabilities.backend_version)));
        }
    }
    let plan = host.plan(units)?;
    let returned: Vec<String> = plan.planned.iter().map(|u| u.unit_id.clone()).collect();
    if returned != planned_ids || !plan.unsupported.is_empty() {
        return Err(ScanError::Plan(format!(
            "host refinement changed frozen plan: expected {planned_ids:?}, got {returned:?}, unsupported={:?}",
            plan.unsupported
        )));
    }

    let mut stream = match host.execute(planned_ids.clone()) {
        Ok(stream) => stream,
        Err(error) => {
            return Ok(incomplete_result(
                rule_id.clone(),
                planned_ids,
                Vec::new(),
                plan_digest,
                descriptor.descriptor_sha256.clone(),
                Vec::new(),
                error.to_string(),
            ));
        }
    };
    let mut statuses = Vec::new();
    let mut staging = AttemptStaging::new(config.max_staging_bytes)?;
    let (completion, attempt_failure) = loop {
        match stream.next_event() {
            Ok(Some(ExecEvent::Observation(observation))) => {
        if let Err(error) = staging.append(&observation) {
                    break (None, Some(error.to_string()));
                }
            }
            Ok(Some(ExecEvent::UnitStatus(status))) => {
                statuses.push(status);
            }
            Ok(Some(ExecEvent::Complete(complete))) => {
                break (Some(complete), None);
            }
            Ok(None) => break (None, Some("execute ended without ExecuteComplete".into())),
            Err(error) => break (None, Some(error.to_string())),
        }
    };
    let attempt_transcript_digest = stream.transcript_digest();
    drop(stream);
    if let Some(reason) = attempt_failure {
        return Ok(incomplete_result(rule_id.clone(), planned_ids, statuses, plan_digest, descriptor.descriptor_sha256.clone(), attempt_transcript_digest, reason));
    }
    let completion = completion.expect("successful execute loop must have completion");
    if let Err(error) = host.complete() {
        return Ok(incomplete_result(rule_id.clone(), planned_ids, statuses, plan_digest, descriptor.descriptor_sha256.clone(), attempt_transcript_digest, error.to_string()));
    }

    let mut status_ids: Vec<String> = statuses.iter().map(|s| s.unit_id.clone()).collect();
    status_ids.sort();
    let mut expected_ids = planned_ids.clone();
    expected_ids.sort();
    if status_ids != expected_ids || completion.units_reported as usize != planned {
        let reason = format!(
            "incomplete attempt: planned={planned_ids:?}, status={status_ids:?}, reported={}",
            completion.units_reported);
        return Ok(incomplete_result(rule_id.clone(), planned_ids, statuses, plan_digest, descriptor.descriptor_sha256.clone(), attempt_transcript_digest, reason));
    }
    let failed = statuses
        .iter()
        .filter(|s| s.outcome != host_pb::UnitOutcome::Executed as i32)
        .count();
    let executed = planned - failed;
    // A failed unit invalidates the whole attempt. Keep the completeness
    // facts for governance, but never leak observations from a partial run.
    let committed_findings = if failed == 0 {
        staging.read_observations()?
    } else {
        Vec::new()
    };
    Ok(ScanResult {
        rule_id,
        planned,
        executed,
        failed,
        attempt_failure: None,
        committed_findings,
        attempt_transcript_digest,
        trust: "LOCAL_UNATTESTED",
        plan_digest,
        workspace_descriptor_digest: descriptor.descriptor_sha256,
        planned_unit_ids: planned_ids,
        unit_statuses: statuses,
    })
}

fn incomplete_result(
    rule_id: String,
    planned_ids: Vec<String>,
    statuses: Vec<host_pb::UnitStatus>,
    plan_digest: Vec<u8>,
    workspace_descriptor_digest: Vec<u8>,
    attempt_transcript_digest: Vec<u8>,
    reason: String,
) -> ScanResult {
    let failed = statuses
        .iter()
        .filter(|status| status.outcome != host_pb::UnitOutcome::Executed as i32)
        .count();
    let executed = statuses
        .iter()
        .filter(|status| status.outcome == host_pb::UnitOutcome::Executed as i32)
        .count();
    ScanResult {
        rule_id,
        planned: planned_ids.len(),
        executed,
        failed,
        attempt_failure: Some(reason),
        committed_findings: Vec::new(),
        attempt_transcript_digest,
        trust: "LOCAL_UNATTESTED",
        plan_digest,
        workspace_descriptor_digest,
        planned_unit_ids: planned_ids,
        unit_statuses: statuses,
    }
}

struct AttemptStaging {
    file: File,
    path: PathBuf,
    bytes: u64,
    limit: u64,
}

impl AttemptStaging {
    fn new(limit: u64) -> Result<Self, ScanError> {
        let path = std::env::temp_dir().join(format!(
            "erf-attempt-{}-{}.stage",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| ScanError::Plan(format!("staging clock failed: {e}")))?
                .as_nanos()
        ));
        let file = OpenOptions::new().create_new(true).read(true).write(true).open(&path)
            .map_err(|e| ScanError::Plan(format!("cannot create staging file: {e}")))?;
        Ok(Self { file, path, bytes: 0, limit })
    }

    fn append(&mut self, observation: &host_pb::BackendObservation) -> Result<(), ScanError> {
        let encoded = observation.encode_to_vec();
        let record_bytes = 8u64 + encoded.len() as u64;
        if self.bytes.saturating_add(record_bytes) > self.limit {
            return Err(ScanError::Plan(format!(
                "attempt staging limit exceeded: {} > {} bytes",
                self.bytes.saturating_add(record_bytes), self.limit
            )));
        }
        self.file.write_all(&(encoded.len() as u64).to_be_bytes())
            .and_then(|_| self.file.write_all(&encoded))
            .map_err(|e| ScanError::Plan(format!("cannot append attempt staging: {e}")))?;
        self.bytes += record_bytes;
        Ok(())
    }

    fn read_observations(&mut self) -> Result<Vec<host_pb::BackendObservation>, ScanError> {
        self.file.flush()
            .map_err(|e| ScanError::Plan(format!("cannot flush attempt staging: {e}")))?;
        self.file.seek(SeekFrom::Start(0))
            .map_err(|e| ScanError::Plan(format!("cannot rewind attempt staging: {e}")))?;
        let mut observations = Vec::new();
        loop {
            let mut len = [0u8; 8];
            match self.file.read_exact(&mut len) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(ScanError::Plan(format!("cannot read attempt staging: {e}"))),
            }
            let size = u64::from_be_bytes(len);
            if size > self.limit || size > usize::MAX as u64 {
                return Err(ScanError::Plan("invalid attempt staging record size".into()));
            }
            let mut encoded = vec![0u8; size as usize];
            self.file.read_exact(&mut encoded)
                .map_err(|e| ScanError::Plan(format!("truncated attempt staging: {e}")))?;
            observations.push(host_pb::BackendObservation::decode(encoded.as_slice())
                .map_err(|e| ScanError::Plan(format!("invalid staged observation: {e}")))?);
        }
        Ok(observations)
    }
}

impl Drop for AttemptStaging {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn digest_plan(units: &[host_pb::AnalysisUnit]) -> Vec<u8> {
    let mut digest = Sha256::new();
    digest.update(b"erf-scan-plan-v0\0");
    for unit in units {
        let encoded = unit.encode_to_vec();
        digest.update((encoded.len() as u64).to_be_bytes());
        digest.update(encoded);
    }
    digest.finalize().to_vec()
}

#[derive(Debug)]
struct ValidatedBundle {
    ruleset_path: String,
    digest: String,
    rule_id: String,
}

fn validate_rule_bundle(root: &Path) -> Result<ValidatedBundle, ScanError> {
    let manifest = root.join("manifest.json");
    let ruleset = root.join("ruleset.xml");
    if !manifest.is_file() || !ruleset.is_file() {
        return Err(ScanError::RuleBundle(format!(
            "candidate requires manifest.json and ruleset.xml: {}", root.display()
        )));
    }
    let manifest_text = fs::read_to_string(&manifest)
        .map_err(|e| ScanError::RuleBundle(format!("cannot read manifest: {e}")))?;
    let rule_id = strict_manifest_rule_id(&manifest_text)
        .ok_or_else(|| ScanError::RuleBundle("manifest has no valid intent identity".into()))?;
    let expected = strict_manifest_digest(&manifest_text)
        .ok_or_else(|| ScanError::RuleBundle("manifest has no valid 64-character digest".into()))?;
    let actual = bundle_payload_digest(root)?;
    if expected != actual {
        return Err(ScanError::RuleBundle(format!(
            "content digest mismatch: manifest={expected}, actual={actual}"
        )));
    }
    if let Some(directory) = root.file_name().and_then(|name| name.to_str()) {
        if directory.len() == 64 && directory.bytes().all(|b| b.is_ascii_hexdigit())
            && directory.to_ascii_lowercase() != expected
        {
            return Err(ScanError::RuleBundle(format!(
                "bundle directory digest mismatch: manifest={expected}, directory={directory}"
            )));
        }
    }
    Ok(ValidatedBundle { ruleset_path: ruleset.to_string_lossy().into_owned(), digest: expected, rule_id })
}

fn strict_manifest_rule_id(text: &str) -> Option<String> {
    let manifest = parse_manifest(text)?;
    let value = manifest.get("intent")?.get("id")?.as_str()?;
    is_valid_rule_id(value).then(|| value.to_owned())
}

fn strict_manifest_digest(text: &str) -> Option<String> {
    let manifest = parse_manifest(text)?;
    let value = manifest.get("digest")?.as_str()?;
    (value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

fn parse_manifest(text: &str) -> Option<Value> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    value.is_object().then_some(value)
}

fn is_valid_rule_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else { return false; };
    first.is_ascii_alphanumeric()
        && chars.all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
}

fn bundle_payload_digest(root: &Path) -> Result<String, ScanError> {
    let mut files = Vec::<(String, PathBuf)>::new();
    collect_payload_files(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut digest = Sha256::new();
    for (relative, path) in files {
        let bytes = fs::read(&path).map_err(|e| ScanError::RuleBundle(format!("cannot read {}: {e}", path.display())))?;
        digest.update((relative.len() as u64).to_be_bytes());
        digest.update(relative.as_bytes());
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    Ok(hex_digest(&digest.finalize()))
}

fn collect_payload_files(root: &Path, current: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), ScanError> {
    let entries = fs::read_dir(current)
        .map_err(|e| ScanError::RuleBundle(format!("cannot read {}: {e}", current.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| ScanError::RuleBundle(format!("cannot enumerate bundle: {e}")))?;
        let path = entry.path();
        if path.is_dir() {
            collect_payload_files(root, &path, out)?;
        } else if path.file_name().and_then(|n| n.to_str()) != Some("manifest.json") {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            out.push((relative, path));
        }
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn test_bundle() -> PathBuf {
        std::env::temp_dir().join(format!(
            "erf-bundle-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ))
    }

    #[test]
    fn candidate_digest_rejects_payload_mutation() {
        let root = test_bundle();
        fs::create_dir_all(root.join("fixtures/bad")).unwrap();
        fs::write(root.join("ruleset.xml"), b"ruleset-v1").unwrap();
        fs::write(root.join("fixtures/bad/Bad0.java"), b"class Bad0 {}").unwrap();
        let digest = bundle_payload_digest(&root).unwrap();
        fs::write(root.join("manifest.json"), format!("{{\"intent\":{{\"id\":\"corp.test\"}},\"digest\":\"{digest}\"}}\n")).unwrap();
        let bundle = validate_rule_bundle(&root).unwrap();
        assert_eq!(bundle.rule_id, "corp.test");
        fs::write(root.join("ruleset.xml"), b"ruleset-tampered").unwrap();
        assert!(validate_rule_bundle(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn content_addressed_directory_must_match_manifest() {
        let parent = test_bundle();
        let root = parent.join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        fs::create_dir_all(root.join("fixtures/bad")).unwrap();
        fs::write(root.join("ruleset.xml"), b"ruleset-v1").unwrap();
        fs::write(root.join("fixtures/bad/Bad0.java"), b"class Bad0 {}").unwrap();
        let digest = bundle_payload_digest(&root).unwrap();
        fs::write(root.join("manifest.json"), format!("{{\"intent\":{{\"id\":\"corp.test\"}},\"digest\":\"{digest}\"}}\n")).unwrap();
        let error = validate_rule_bundle(&root).unwrap_err();
        assert!(error.to_string().contains("directory digest mismatch"));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn candidate_manifest_requires_a_closed_intent_identity() {
        let root = test_bundle();
        fs::create_dir_all(root.join("fixtures/bad")).unwrap();
        fs::write(root.join("ruleset.xml"), b"ruleset-v1").unwrap();
        fs::write(root.join("fixtures/bad/Bad0.java"), b"class Bad0 {}").unwrap();
        let digest = bundle_payload_digest(&root).unwrap();
        fs::write(root.join("manifest.json"), format!("{{\"digest\":\"{digest}\"}}\n")).unwrap();
        let error = validate_rule_bundle(&root).unwrap_err();
        assert!(error.to_string().contains("intent identity"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn candidate_manifest_does_not_accept_decoy_identity_or_digest_fields() {
        let root = test_bundle();
        fs::create_dir_all(root.join("fixtures/bad")).unwrap();
        fs::write(root.join("ruleset.xml"), b"ruleset-v1").unwrap();
        fs::write(root.join("fixtures/bad/Bad0.java"), b"class Bad0 {}").unwrap();
        let digest = bundle_payload_digest(&root).unwrap();
        fs::write(
            root.join("manifest.json"),
            format!(
                "{{\"intent\":{{\"version\":\"1.0.0\"}},\"metadata\":{{\"id\":\"corp.decoy\",\"digest\":\"{digest}\"}}}}\n"
            ),
        )
        .unwrap();
        let error = validate_rule_bundle(&root).unwrap_err();
        assert!(error.to_string().contains("intent identity"));

        fs::write(
            root.join("manifest.json"),
            format!(
                "{{\"intent\":{{\"id\":\"corp.test\"}},\"metadata\":{{\"digest\":\"{digest}\"}}}}\n"
            ),
        )
        .unwrap();
        let error = validate_rule_bundle(&root).unwrap_err();
        assert!(error.to_string().contains("64-character digest"));
        fs::remove_dir_all(root).unwrap();
    }

    fn valid_lock() -> ExecutionLock {
        let mut lock = ExecutionLock {
            rule_bundle_digest: "a".repeat(64),
            runner_release_digest: "b".repeat(64),
            backend_pack_digest: "c".repeat(64),
            resolver_pack_digest: "d".repeat(64),
            sandbox_provider_id: "rootless-oci-docker-v0".into(),
            sandbox_policy_digest: "e".repeat(64),
            conformance_suite_digest: "f".repeat(64),
            resolver_id: "resolver".into(),
            resolver_version: "0.1.0".into(),
            backend_id: "pmd-java".into(),
            backend_version: "7.9.0".into(),
            protocol_major: erf_host_client::PROTOCOL_MAJOR,
            canonical_set_version: rf_gate::CANONICAL_SET_VERSION.into(),
            lock_digest: String::new(),
        };
        lock.lock_digest = lock.content_digest();
        lock
    }

    #[test]
    fn execution_lock_is_self_consistent_and_rejects_mutation() {
        let lock = valid_lock();
        assert!(lock.validate().is_ok());
        let mut changed = lock.clone();
        changed.backend_version = "7.9.1".into();
        assert!(changed.validate().is_err());
        let mut changed_pack = lock.clone();
        changed_pack.backend_pack_digest = "9".repeat(64);
        assert!(changed_pack.validate().is_err());
        let mut changed_sandbox = lock.clone();
        changed_sandbox.sandbox_provider_id = "different-provider".into();
        assert!(changed_sandbox.validate().is_err());
    }

    #[test]
    fn execution_lock_file_format_is_closed() {
        let path = test_bundle();
        let lock = valid_lock();
        let text = format!(
            "rule_bundle_digest={}\nrunner_release_digest={}\nbackend_pack_digest={}\nresolver_pack_digest={}\nsandbox_provider_id={}\nsandbox_policy_digest={}\nconformance_suite_digest={}\nresolver_id={}\nresolver_version={}\nbackend_id={}\nbackend_version={}\nprotocol_major={}\ncanonical_set_version={}\nlock_digest={}\n",
            lock.rule_bundle_digest, lock.runner_release_digest, lock.backend_pack_digest,
            lock.resolver_pack_digest, lock.sandbox_provider_id, lock.sandbox_policy_digest,
            lock.conformance_suite_digest, lock.resolver_id, lock.resolver_version,
            lock.backend_id, lock.backend_version, lock.protocol_major,
            lock.canonical_set_version, lock.lock_digest);
        fs::write(&path, text).unwrap();
        assert_eq!(ExecutionLock::from_file(&path).unwrap(), lock);
        fs::write(&path, "unknown=value\n").unwrap();
        assert!(ExecutionLock::from_file(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn signed_execution_lock_binds_canonical_payload_and_role() {
        let lock = valid_lock();
        let key = SigningKey::from_bytes(&[17; 32]);
        let payload = lock.canonical_payload();
        let asset = erf_trust::SignedAsset {
            payload_type: erf_trust::DSSE_PAYLOAD_TYPE.into(),
            signature: key.sign(&erf_trust::pae(erf_trust::DSSE_PAYLOAD_TYPE, &payload)).to_bytes().to_vec(),
            payload,
            key_id: "lock-key".into(),
        };
        let mut trust = erf_trust::TrustStore::default();
        trust.insert_key("lock-key", key.verifying_key()).unwrap();
        trust.authorize_role("execution-lock", "lock-key").unwrap();
        let signed = SignedExecutionLock { lock: lock.clone(), asset, role: "execution-lock".into(), trust_store: trust, metadata: None };
        assert!(signed.verify().is_ok());
        let mut changed = signed.clone();
        changed.role = "policy".into();
        assert!(changed.verify().is_err());
    }

    #[test]
    fn signed_execution_lock_file_round_trips_and_verifies() {
        let path = test_bundle();
        let lock = valid_lock();
        let key = SigningKey::from_bytes(&[18; 32]);
        let payload = lock.canonical_payload();
        let signature = key.sign(&erf_trust::pae(erf_trust::DSSE_PAYLOAD_TYPE, &payload));
        let text = format!(
            "rule_bundle_digest={}\nrunner_release_digest={}\nbackend_pack_digest={}\nresolver_pack_digest={}\nsandbox_provider_id={}\nsandbox_policy_digest={}\nconformance_suite_digest={}\nresolver_id={}\nresolver_version={}\nbackend_id={}\nbackend_version={}\nprotocol_major={}\ncanonical_set_version={}\nlock_digest={}\nkey_id=lock-key\nrole=execution-lock\npublic_key_hex={}\nsignature_hex={}\nmetadata_version=1\nexpires_at_epoch_secs=100\n",
            lock.rule_bundle_digest, lock.runner_release_digest, lock.backend_pack_digest,
            lock.resolver_pack_digest, lock.sandbox_provider_id, lock.sandbox_policy_digest,
            lock.conformance_suite_digest, lock.resolver_id, lock.resolver_version,
            lock.backend_id, lock.backend_version, lock.protocol_major,
            lock.canonical_set_version, lock.lock_digest,
            hex_digest(&key.verifying_key().to_bytes()), hex_digest(&signature.to_bytes()));
        fs::write(&path, text).unwrap();
        let signed = SignedExecutionLock::from_file(&path).unwrap();
        assert!(signed.verify_at(99).is_ok());
        assert!(signed.verify_at(100).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn staging_enforces_byte_limit_before_append() {
        let mut staging = AttemptStaging::new(7).unwrap();
        let observation = host_pb::BackendObservation::default();
        assert!(staging.append(&observation).is_err());
    }
}

#[allow(dead_code)]
fn _workspace_mode_is_closed(mode: i32) -> bool {
    matches!(
        workspace_pb::WorkspaceResolutionMode::try_from(mode),
        Ok(workspace_pb::WorkspaceResolutionMode::SafeStatic)
            | Ok(workspace_pb::WorkspaceResolutionMode::AttestedBuildExport)
    )
}
