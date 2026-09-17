//! Minimal scan orchestration slice for Phase 1.

use erf_contracts::{backend::v0 as host_pb, workspace::v0 as workspace_pb};
use erf_host_client::{ExecEvent, HostConfig, HostHandle, HostError};
use erf_resolver_client::{ResolverConfig, ResolverHandle, ResolverError};
use prost::Message;
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
    pub max_staging_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub planned: usize,
    pub executed: usize,
    pub failed: usize,
    pub committed_findings: Vec<host_pb::BackendObservation>,
    pub attempt_transcript_digest: Vec<u8>,
    pub trust: &'static str,
    pub plan_digest: Vec<u8>,
    pub planned_unit_ids: Vec<String>,
    pub unit_statuses: Vec<host_pb::UnitStatus>,
}

/// Runs one scan attempt. Findings are moved into the result only after every
/// planned unit has exactly one terminal status and ExecuteComplete agrees.
pub fn scan(config: &ScanConfig) -> Result<ScanResult, ScanError> {
    let ruleset_path = if let Some(bundle) = &config.rule_bundle {
        Some(validate_rule_bundle(Path::new(bundle))?)
    } else {
        None
    };
    let mut resolver_command = Command::new(&config.resolver_command);
    resolver_command.args(&config.resolver_args);
    let resolver_config = ResolverConfig {
        required_capabilities: vec!["ecosystem.static-layout".into()],
        ..Default::default()
    };
    let (mut resolver, _) = ResolverHandle::spawn(&mut resolver_command, &resolver_config)?;
    let descriptor = resolver.resolve(&config.repo_root)?;
    let _ = resolver.complete()?;

    // Freeze the global denominator before asking the host to refine it.
    let units: Vec<host_pb::AnalysisUnit> = descriptor
        .modules
        .iter()
        .map(|module| host_pb::AnalysisUnit {
            unit_id: format!("{}:{}", module.module_id, config.rule_id),
            language: module.language.clone(),
            files: module.files.iter().map(|f| f.path.clone()).collect(),
        })
        .collect();
    let planned_ids: Vec<String> = units.iter().map(|u| u.unit_id.clone()).collect();
    let planned = planned_ids.len();
    let plan_digest = digest_plan(&units);

    let mut host_command = Command::new(&config.host_command);
    host_command.args(&config.host_args);
    // The descriptor paths are relative to the resolved snapshot, not the
    // runtime process cwd. Bind that identity into the host session.
    host_command.env("ERF_REPO_ROOT", &config.repo_root);
    if let Some(ruleset) = ruleset_path {
        host_command.env("ERF_RULESET", ruleset);
        host_command.env("ERF_RULE_BUNDLE", config.rule_bundle.as_deref().unwrap_or_default());
    }
    let host_config = HostConfig {
        required_capabilities: vec!["java.ast".into()],
        ..Default::default()
    };
    let (mut host, _) = HostHandle::spawn(&mut host_command, &host_config)?;
    let plan = host.plan(units)?;
    let returned: Vec<String> = plan.planned.iter().map(|u| u.unit_id.clone()).collect();
    if returned != planned_ids || !plan.unsupported.is_empty() {
        return Err(ScanError::Plan(format!(
            "host refinement changed frozen plan: expected {planned_ids:?}, got {returned:?}, unsupported={:?}",
            plan.unsupported
        )));
    }

    let mut stream = host.execute(planned_ids.clone())?;
    let mut statuses = Vec::new();
    let mut staging = AttemptStaging::new(config.max_staging_bytes)?;
    let completion = loop {
        match stream.next_event()? {
            Some(ExecEvent::Observation(observation)) => {
                staging.append(&observation)?;
            }
            Some(ExecEvent::UnitStatus(status)) => {
                statuses.push(status);
            }
            Some(ExecEvent::Complete(complete)) => {
                break complete;
            }
            None => return Err(ScanError::Plan("execute ended without ExecuteComplete".into())),
        }
    };
    let attempt_transcript_digest = stream.transcript_digest();
    let _ = host.complete()?;

    let mut status_ids: Vec<String> = statuses.iter().map(|s| s.unit_id.clone()).collect();
    status_ids.sort();
    let mut expected_ids = planned_ids.clone();
    expected_ids.sort();
    if status_ids != expected_ids || completion.units_reported as usize != planned {
        return Err(ScanError::Plan(format!(
            "incomplete attempt: planned={planned_ids:?}, status={status_ids:?}, reported={}",
            completion.units_reported
        )));
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
        planned,
        executed,
        failed,
        committed_findings,
        attempt_transcript_digest,
        trust: "LOCAL_UNATTESTED",
        plan_digest,
        planned_unit_ids: planned_ids,
        unit_statuses: statuses,
    })
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

fn validate_rule_bundle(root: &Path) -> Result<String, ScanError> {
    let manifest = root.join("manifest.json");
    let ruleset = root.join("ruleset.xml");
    if !manifest.is_file() || !ruleset.is_file() {
        return Err(ScanError::RuleBundle(format!(
            "candidate requires manifest.json and ruleset.xml: {}", root.display()
        )));
    }
    let manifest_text = fs::read_to_string(&manifest)
        .map_err(|e| ScanError::RuleBundle(format!("cannot read manifest: {e}")))?;
    let expected = manifest_digest(&manifest_text)
        .ok_or_else(|| ScanError::RuleBundle("manifest has no valid 64-character digest".into()))?;
    let actual = bundle_payload_digest(root)?;
    if expected != actual {
        return Err(ScanError::RuleBundle(format!(
            "content digest mismatch: manifest={expected}, actual={actual}"
        )));
    }
    Ok(ruleset.to_string_lossy().into_owned())
}

fn manifest_digest(text: &str) -> Option<String> {
    let marker = "\"digest\"";
    let start = text.find(marker)?;
    let rest = &text[start + marker.len()..];
    let colon = rest.find(':')?;
    let quoted = rest[colon + 1..].trim_start().strip_prefix('"')?;
    let end = quoted.find('"')?;
    let value = &quoted[..end];
    (value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
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
        fs::write(root.join("manifest.json"), format!("{{\"digest\":\"{digest}\"}}\n")).unwrap();
        assert!(validate_rule_bundle(&root).is_ok());
        fs::write(root.join("ruleset.xml"), b"ruleset-tampered").unwrap();
        assert!(validate_rule_bundle(&root).is_err());
        fs::remove_dir_all(root).unwrap();
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
