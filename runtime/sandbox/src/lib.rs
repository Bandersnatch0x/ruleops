//! Replaceable sandbox boundary for trusted backend/resolver processes.
//!
//! The provider builds a rootless OCI invocation with default-deny flags. It
//! never falls back to an ordinary child process when preflight fails.

use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::{Command, Output};

pub const PROVIDER_ID: &str = "rootless-oci-docker-v0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxPolicy {
    pub image: String,
    pub entrypoint: String,
    pub entrypoint_args: Vec<String>,
    pub repository_root: PathBuf,
    pub rule_bundle: PathBuf,
    pub seccomp_profile: PathBuf,
    pub memory_mb: u64,
    pub cpus_millis: u64,
    pub pids_limit: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSandbox {
    pub provider_id: &'static str,
    pub policy_digest: String,
    pub command: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SandboxError {
    #[error("sandbox image is empty")]
    EmptyImage,
    #[error("sandbox entrypoint is empty")]
    EmptyEntrypoint,
    #[error("sandbox path is not absolute: {0}")]
    NonAbsolutePath(PathBuf),
    #[error("sandbox path does not exist or has the wrong type: {0}")]
    InvalidPath(PathBuf),
    #[error("sandbox resource limit must be non-zero")]
    InvalidLimit,
    #[error("sandbox provider binary is unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("sandbox provider daemon rejected preflight: {0}")]
    PreflightRejected(String),
}

#[derive(Debug, Clone)]
pub struct DockerSandboxProvider {
    pub docker_binary: PathBuf,
}

impl Default for DockerSandboxProvider {
    fn default() -> Self { Self { docker_binary: PathBuf::from("docker") } }
}

impl DockerSandboxProvider {
    pub fn validate_policy(policy: &SandboxPolicy) -> Result<(), SandboxError> {
        if policy.image.trim().is_empty() { return Err(SandboxError::EmptyImage); }
        if policy.entrypoint.trim().is_empty() { return Err(SandboxError::EmptyEntrypoint); }
        if policy.memory_mb == 0 || policy.cpus_millis == 0 || policy.pids_limit == 0 { return Err(SandboxError::InvalidLimit); }
        for path in [&policy.repository_root, &policy.rule_bundle, &policy.seccomp_profile] {
            if !path.is_absolute() { return Err(SandboxError::NonAbsolutePath(path.clone())); }
        }
        if !policy.repository_root.is_dir() || !policy.rule_bundle.is_dir() || !policy.seccomp_profile.is_file() {
            return Err(SandboxError::InvalidPath(policy.seccomp_profile.clone()));
        }
        Ok(())
    }

    pub fn prepare(&self, policy: &SandboxPolicy) -> Result<PreparedSandbox, SandboxError> {
        Self::validate_policy(policy)?;
        let mut command = vec![self.docker_binary.to_string_lossy().into_owned(), "run".into(), "--rm".into(), "--init".into(), "--read-only".into(), "--network=none".into(), "--cap-drop=ALL".into(), "--security-opt=no-new-privileges".into(), format!("--security-opt=seccomp={}", policy.seccomp_profile.display()), format!("--pids-limit={}", policy.pids_limit), format!("--memory={}m", policy.memory_mb), format!("--cpus={:.3}", policy.cpus_millis as f64 / 1000.0), "--tmpfs".into(), "/tmp:rw,noexec,nosuid,nodev".into(), "--mount".into(), format!("type=bind,src={},dst=/workspace,readonly", policy.repository_root.display()), "--mount".into(), format!("type=bind,src={},dst=/rule-bundle,readonly", policy.rule_bundle.display()), "--env".into(), "ERF_REPO_ROOT=/workspace".into(), "--env".into(), "ERF_RULE_BUNDLE=/rule-bundle".into(), "--env".into(), "ERF_RULESET=/rule-bundle/ruleset.xml".into(), policy.image.clone(), policy.entrypoint.clone()];
        command.extend(policy.entrypoint_args.clone());
        Ok(PreparedSandbox { provider_id: PROVIDER_ID, policy_digest: policy_digest(policy), command })
    }

    /// Checks the actual provider/daemon. A failed check is terminal for a
    /// trusted execution; callers must not run the command unsandboxed.
    pub fn preflight(&self) -> Result<Output, SandboxError> {
        let output = Command::new(&self.docker_binary).args(["info", "--format", "{{json .SecurityOptions}}"]).output().map_err(|error| SandboxError::ProviderUnavailable(error.to_string()))?;
        if !output.status.success() { return Err(SandboxError::PreflightRejected(String::from_utf8_lossy(&output.stderr).trim().to_string())); }
        let security_options = String::from_utf8_lossy(&output.stdout);
        if !security_options.to_ascii_lowercase().contains("rootless") {
            return Err(SandboxError::PreflightRejected("provider daemon did not report rootless mode".into()));
        }
        Ok(output)
    }
}

fn policy_digest(policy: &SandboxPolicy) -> String {
    let mut digest = Sha256::new();
    digest.update(b"erf-sandbox-policy-v0\0");
    for value in [&policy.image, &policy.entrypoint, &policy.repository_root.to_string_lossy().into_owned(), &policy.rule_bundle.to_string_lossy().into_owned(), &policy.seccomp_profile.to_string_lossy().into_owned()] {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value.as_bytes());
    }
    for value in [policy.memory_mb, policy.cpus_millis, policy.pids_limit] { digest.update(value.to_be_bytes()); }
    for value in &policy.entrypoint_args { digest.update((value.len() as u64).to_be_bytes()); digest.update(value.as_bytes()); }
    hex(&digest.finalize())
}

fn hex(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn provider_fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("test-fixtures").join(name)
    }

    fn policy(root: &Path) -> SandboxPolicy {
        let repo = root.join("repo");
        let bundle = root.join("bundle");
        let seccomp = root.join("seccomp.json");
        fs::create_dir_all(&repo).unwrap(); fs::create_dir_all(&bundle).unwrap(); fs::write(&seccomp, b"{}").unwrap();
        SandboxPolicy { image: "erf/java-host@sha256:abc".into(), entrypoint: "/usr/local/bin/host".into(), entrypoint_args: vec!["--stdio".into()], repository_root: repo, rule_bundle: bundle, seccomp_profile: seccomp, memory_mb: 512, cpus_millis: 1000, pids_limit: 64 }
    }

    #[test]
    fn prepared_command_contains_default_deny_controls() {
        let root = std::env::temp_dir().join(format!("erf-sandbox-{}", std::process::id()));
        let prepared = DockerSandboxProvider::default().prepare(&policy(&root)).unwrap();
        assert_eq!(prepared.provider_id, PROVIDER_ID);
        for flag in ["--read-only", "--network=none", "--cap-drop=ALL", "--security-opt=no-new-privileges", "--pids-limit=64", "--memory=512m", "--tmpfs", "--mount"] { assert!(prepared.command.iter().any(|value| value == flag || value.starts_with(&format!("{flag}="))), "missing {flag}"); }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_policy_fails_closed() {
        let root = std::env::temp_dir().join(format!("erf-sandbox-invalid-{}", std::process::id()));
        let mut value = policy(&root); value.memory_mb = 0;
        assert_eq!(DockerSandboxProvider::validate_policy(&value), Err(SandboxError::InvalidLimit));
        value.memory_mb = 512; value.seccomp_profile = root.join("missing.json");
        assert!(matches!(DockerSandboxProvider::validate_policy(&value), Err(SandboxError::InvalidPath(_))));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preflight_rejects_daemon_without_rootless_mode() {
        let provider = DockerSandboxProvider { docker_binary: provider_fixture("docker-no-rootless.sh") };
        assert!(matches!(provider.preflight(), Err(SandboxError::PreflightRejected(message)) if message.contains("rootless")));
    }

    #[test]
    fn preflight_accepts_provider_reporting_rootless_mode() {
        let provider = DockerSandboxProvider { docker_binary: provider_fixture("docker-rootless.sh") };
        let output = provider.preflight().unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("rootless"));
    }

    #[test]
    fn preflight_rejects_unavailable_provider_without_fallback() {
        let path = std::env::temp_dir().join(format!("erf-sandbox-provider-missing-{}", std::process::id()));
        let provider = DockerSandboxProvider { docker_binary: path };
        assert!(matches!(provider.preflight(), Err(SandboxError::ProviderUnavailable(_))));
    }

    #[test]
    fn prepared_command_binds_inputs_readonly_and_keeps_tmpfs_isolated() {
        let root = std::env::temp_dir().join(format!("erf-sandbox-mounts-{}", std::process::id()));
        let policy = policy(&root);
        let prepared = DockerSandboxProvider::default().prepare(&policy).unwrap();
        let repo_mount = format!("type=bind,src={},dst=/workspace,readonly", policy.repository_root.display());
        let bundle_mount = format!("type=bind,src={},dst=/rule-bundle,readonly", policy.rule_bundle.display());
        assert!(prepared.command.iter().any(|value| value == &repo_mount));
        assert!(prepared.command.iter().any(|value| value == &bundle_mount));
        assert!(prepared.command.iter().any(|value| value == "/tmp:rw,noexec,nosuid,nodev"));
        assert!(prepared.command.iter().any(|value| value == "--security-opt=no-new-privileges"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn policy_digest_is_stable_and_binds_enforcement_limits() {
        let root = std::env::temp_dir().join(format!("erf-sandbox-digest-{}", std::process::id()));
        let policy = policy(&root);
        let first = DockerSandboxProvider::default().prepare(&policy).unwrap();
        let second = DockerSandboxProvider::default().prepare(&policy).unwrap();
        assert_eq!(first.policy_digest, second.policy_digest);
        let mut changed = policy.clone();
        changed.pids_limit = 128;
        let changed_digest = DockerSandboxProvider::default().prepare(&changed).unwrap().policy_digest;
        assert_ne!(first.policy_digest, changed_digest);
        fs::remove_dir_all(root).unwrap();
    }
}
