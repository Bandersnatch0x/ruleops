//! Minimal scan orchestration slice for Phase 1.

use erf_contracts::{backend::v0 as host_pb, workspace::v0 as workspace_pb};
use erf_bundle_cache::BundleCache;
use erf_host_client::{ExecEvent, HostConfig, HostHandle, HostError};
use erf_resolver_client::{ResolverConfig, ResolverHandle, ResolverError};
use prost::Message;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::process::Command;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    #[error("rule bundle delivery failed before scan: {0}")]
    Delivery(String),
    #[error("trust verification failed: {0}")]
    Trust(String),
    #[error("sandbox execution failed: {0}")]
    Sandbox(String),
}

/// Stable machine-facing class prefix for the CLI exit mapping. The human
/// detail remains in the string for diagnostics, but callers must classify
/// delivery failures at the boundary instead of guessing from arbitrary
/// wording downstream.
fn classified_delivery(class: &str, detail: impl Into<String>) -> ScanError {
    ScanError::Delivery(format!("delivery-class={class}; {}", detail.into()))
}

fn classify_delivery_detail(detail: &str) -> String {
    let class = if [
        "invalid delivery descriptor",
        "invalid delivery endpoint",
        "delivery response is invalid",
        "descriptor does not match",
        "compatibility or platform does not match",
        "input does not match descriptor",
        "runtime Pack count does not match descriptor",
        "runtime Pack digest does not match descriptor",
        "runtime Pack statement digest does not match descriptor",
        "runtime Pack manifest identity does not match the delivery context",
        "runtime Pack OCI layout verification failed",
    ]
    .iter()
    .any(|marker| detail.contains(marker)) {
        "input"
    } else if [
        "SIGNATURE_INVALID",
        "TRUST_METADATA_UNAVAILABLE",
        "REVOCATION_SNAPSHOT_UNAVAILABLE",
        "REFERENCED_PACK_UNAVAILABLE",
        "TRUSTED_CONTEXT_INVALID",
        "verification context",
        "signed RuleBundle verification failed",
        "runtime Pack verification failed",
        "execution identity",
        "revocation snapshot",
        "signature",
        "digest mismatch",
        "unsafe",
    ]
    .iter()
    .any(|marker| detail.contains(marker)) {
        "trust"
    } else if [
        "SERVER_FETCH_REQUIRED",
        "connected runner must fetch",
        "delivery transport failed",
        "delivery response is too large",
        "server rejected delivery request",
    ]
    .iter()
    .any(|marker| detail.contains(marker)) {
        "fetch"
    } else {
        "inconclusive"
    };
    format!("delivery-class={class}; {detail}")
}

#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub repo_root: String,
    pub rule_id: String,
    /// Optional local Phase 1 Candidate directory. When omitted, a verified
    /// Server delivery is materialized from its bounded canonical payload.
    pub rule_bundle: Option<String>,
    /// Optional Server-to-Runner delivery boundary. When present, resolution
    /// completes before the resolver or backend is started and the returned
    /// bytes are materialized or compared as the canonical RuleBundle payload.
    pub bundle_delivery: Option<BundleDeliveryConfig>,
    pub resolver_command: String,
    pub resolver_args: Vec<String>,
    pub host_command: String,
    pub host_args: Vec<String>,
    pub host_deadline_ms: u64,
    /// Resolver response deadline. A resolver must not outlive this budget
    /// even when the Backend Host uses a separate deadline.
    pub resolver_deadline_ms: u64,
    pub max_staging_bytes: u64,
    pub execution_lock: Option<ExecutionLock>,
    pub signed_execution_lock: Option<SignedExecutionLock>,
    /// Optional independent sandbox for the WorkspaceResolverPack. Trusted CI
    /// must supply this separately from the Backend Host sandbox.
    pub resolver_sandbox_policy: Option<erf_sandbox::SandboxPolicy>,
    pub sandbox_policy: Option<erf_sandbox::SandboxPolicy>,
    /// Optional DSSE WorkspaceAttestation required when the Resolver declares
    /// ATTESTED_BUILD_EXPORT. SAFE_STATIC never consumes this assertion.
    pub workspace_attestation: Option<WorkspaceAttestationVerification>,
    /// A caller-provided immutable plan. When present, the Runner filters the
    /// resolver's units to this exact denominator and reports its explicit
    /// completeness/authority state; it never reconstructs the scope.
    pub frozen_plan: Option<erf_scan_plan::ScanPlan>,
    /// Optional CLI-owned range. The Runner resolves ChangedSet, closes the
    /// affected scope and freezes the plan only after the real descriptor is
    /// available; callers cannot smuggle an unbound plan into execution.
    pub incremental: Option<IncrementalScanRequest>,
    /// Optional content-addressed summary cache for incremental planning.
    /// The Runner probes it before execution; the CLI persists only the
    /// post-Gate summary after the current findings are normalized.
    pub scan_cache_dir: Option<String>,
    /// Optional #106 safety facts. When supplied, the Runner evaluates the
    /// same structured full-fallback decision used by the Pilot baseline and
    /// refuses to execute an incremental request without a FULL plan after a
    /// safety widening.
    pub fallback_inputs: Option<erf_incremental_baseline::FallbackInputs>,
    /// Effective project scope produced by `rf config`. When present, the
    /// resolver may describe more files, but the Runner can execute only the
    /// configured module/root/include set. Omitting this is reserved for
    /// lower-level trusted/pilot callers that already own their scope.
    pub project_scope: Option<ProjectScanScope>,
}

#[derive(Debug, Clone)]
pub struct IncrementalScanRequest {
    pub repository: String,
    pub base: String,
    pub head: String,
    pub rule_bundle_digest: String,
    pub policy_digest: String,
    pub resolver_identity: String,
    pub backend_identity: String,
    pub runner_identity: String,
    pub canonical_set_version: String,
}

#[derive(Debug, Clone)]
pub struct WorkspaceAttestationVerification {
    pub attestation: erf_trust::WorkspaceAttestation,
    pub signed_asset: erf_trust::SignedAsset,
    pub trust_store: erf_trust::TrustStore,
    pub trust_metadata: Option<erf_trust::TrustMetadata>,
    pub now_epoch_secs: u64,
    pub expected_repository: String,
    pub expected_reference: String,
    pub expected_commit_tree_digest: String,
    pub expected_toolchain_identity: String,
    pub expected_workload_issuer: String,
    pub expected_workload_audience: String,
    pub expected_workflow: String,
    pub expected_run_identity: String,
    pub expected_environment_identity: String,
}

impl WorkspaceAttestationVerification {
    fn verify(
        &self,
        descriptor_digest: &[u8],
        execution_lock: &ExecutionLock,
        repo_root: &Path,
    ) -> Result<(), ScanError> {
        let subject: [u8; 32] = descriptor_digest
            .try_into()
            .map_err(|_| ScanError::Trust("WorkspaceAttestation subject digest is not 32 bytes".into()))?;
        let expected = erf_trust::WorkspaceAttestationExpectations {
            subject_digest: subject,
            repository: &self.expected_repository,
            reference: &self.expected_reference,
            commit_tree_digest: &self.expected_commit_tree_digest,
            execution_lock_digest: &execution_lock.lock_digest,
            resolver_pack_digest: &execution_lock.resolver_pack_digest,
            toolchain_identity: &self.expected_toolchain_identity,
            workload_issuer: &self.expected_workload_issuer,
            workload_audience: &self.expected_workload_audience,
            workflow: &self.expected_workflow,
            run_identity: &self.expected_run_identity,
            environment_identity: &self.expected_environment_identity,
        };
        self.attestation
            .verify_signed_with_context(
                &self.signed_asset,
                &self.trust_store,
                self.trust_metadata.as_ref(),
                self.now_epoch_secs,
                &expected,
            )
            .map_err(|error| ScanError::Trust(format!("WorkspaceAttestation verification failed: {error}")))
            .and_then(|()| verify_checkout_binding(
                repo_root,
                &self.expected_repository,
                &self.expected_reference,
                &self.expected_commit_tree_digest,
            ))
}
}

fn verify_checkout_binding(
    repo_root: &Path,
    expected_repository: &str,
    expected_reference: &str,
    expected_tree: &str,
) -> Result<(), ScanError> {
    let git = |args: &[&str]| -> Result<String, ScanError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(args)
            .output()
            .map_err(|error| ScanError::Trust(format!("checkout identity command could not start: {error}")))?;
        if !output.status.success() {
            return Err(ScanError::Trust(format!(
                "checkout identity command failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let status = git(&["status", "--porcelain=v1", "--untracked-files=all"])?;
    if !status.is_empty() {
        return Err(ScanError::Trust("ATTESTED_BUILD_EXPORT requires a clean checkout".into()));
    }
    let head = git(&["rev-parse", "--verify", "HEAD^{commit}"])?;
    let reference = git(&["rev-parse", "--verify", &format!("{}^{{commit}}", expected_reference)])?;
    if head != reference {
        return Err(ScanError::Trust("WorkspaceAttestation reference does not resolve to the checked-out commit".into()));
    }
    let tree = git(&["rev-parse", "--verify", "HEAD^{tree}"])?;
    let expected_tree = expected_tree
        .strip_prefix("git-tree:")
        .or_else(|| expected_tree.strip_prefix("sha1:"))
        .unwrap_or(expected_tree);
    if tree != expected_tree {
        return Err(ScanError::Trust(format!(
            "WorkspaceAttestation commit/tree digest does not match checkout: expected {expected_tree}, actual {tree}"
        )));
    }
    let remote = git(&["remote", "get-url", "origin"])?;
    let actual_repository = normalize_repository_identity(&remote);
    let expected_repository = normalize_repository_identity(expected_repository);
    if actual_repository != expected_repository
        && !actual_repository.ends_with(&format!("/{expected_repository}"))
    {
        return Err(ScanError::Trust(format!(
            "WorkspaceAttestation repository does not match checkout: expected {expected_repository}, actual {actual_repository}"
        )));
    }
    Ok(())
}

fn normalize_repository_identity(value: &str) -> String {
    let mut value = value.trim().trim_end_matches('/').trim_end_matches(".git").to_string();
    if let Some((host, path)) = value.split_once(':') {
        if host.contains('@') {
            value = format!("{}/{}", host.rsplit('@').next().unwrap_or(host), path);
        }
    }
    if let Some(stripped) = value.strip_prefix("https://").or_else(|| value.strip_prefix("http://")) {
        value = stripped.to_string();
    }
    value.trim_matches('/').to_string()
}

fn git_tree_identity(repo_root: &str, commit: &str) -> Result<String, ScanError> {
    let revision = format!("{commit}^{{tree}}");
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "--verify"])
        .arg(&revision)
        .output()
        .map_err(|error| ScanError::Plan(format!("incremental tree identity command could not start: {error}")))?;
    if !output.status.success() {
        return Err(ScanError::Plan(format!(
            "incremental head tree could not be resolved: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let tree = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if tree.is_empty() || !tree.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ScanError::Plan("incremental head tree identity is invalid".into()));
    }
    Ok(tree)
}

fn verify_incremental_checkout(repo_root: &str, requested_head: &str) -> Result<(), ScanError> {
    let git = |args: &[&str]| -> Result<String, ScanError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(args)
            .output()
            .map_err(|error| ScanError::Plan(format!("incremental checkout identity command could not start: {error}")))?;
        if !output.status.success() {
            return Err(ScanError::Plan(format!(
                "incremental checkout identity command failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    if !git(&["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty() {
        return Err(ScanError::Plan(
            "incremental scans require a clean checkout; use an isolated checkout at --head".into(),
        ));
    }
    let expected = git(&["rev-parse", "--verify", &format!("{requested_head}^{{commit}}")])?;
    let actual = git(&["rev-parse", "--verify", "HEAD^{commit}"])?;
    if expected != actual {
        return Err(ScanError::Plan(format!(
            "incremental scan head does not match checkout: requested {expected}, actual {actual}"
        )));
    }
    Ok(())
}

fn unix_now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[derive(Debug, Clone)]
pub struct BundleDeliveryConfig {
    pub endpoint: String,
    pub descriptor: erf_bundle_cache::BundleDeliveryDescriptor,
    pub expected_compatibility_identity: String,
    pub expected_platform: String,
    pub cache_root: String,
    pub profile: erf_bundle_cache::DeploymentProfile,
    pub evidence: erf_bundle_cache::VerificationEvidence,
    /// The caller's local trust boundary. Server booleans are availability
    /// evidence only; this context performs the actual signature and Pack
    /// verification before a scan can start.
    pub verification: Option<BundleDeliveryVerification>,
    pub now_epoch_secs: u64,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct BundleDeliveryVerification {
    pub signed_bundle: erf_rule_bundle::SignedRuleBundle,
    pub manifest_bytes: Vec<u8>,
    pub inputs: erf_rule_bundle::RuleBundleInputs,
    pub trust_store: erf_trust::TrustStore,
    pub trust_metadata: Option<erf_trust::TrustMetadata>,
    pub revocation_snapshot: Option<erf_trust::RevocationSnapshot>,
    pub revocation_snapshot_digest: [u8; 32],
    pub packs: Vec<BundlePackVerification>,
}

#[derive(Debug, Clone)]
pub struct BundlePackVerification {
    pub kind: BundlePackKind,
    pub release: erf_packs::OciPackRelease,
    pub manifest_bytes: Vec<u8>,
    pub manifest_platform: String,
    pub manifest_compatibility_identity: String,
    /// Digest of the concrete executable asset declared by the pack manifest.
    /// The CLI binds this measurement to the installed runtime asset before
    /// spawning the resolver or backend.
    pub asset_digest: [u8; 32],
    pub oci_layout: Option<OciLayoutVerification>,
}

#[derive(Debug, Clone)]
pub struct OciLayoutVerification {
    pub layout_bytes: Vec<u8>,
    pub index_bytes: Vec<u8>,
    pub config_bytes: Vec<u8>,
    pub layer_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundlePackKind {
    BackendRuntime,
    WorkspaceResolver,
}

static BUNDLE_DELIVERY_SINGLE_FLIGHT: OnceLock<erf_bundle_delivery::SingleFlightDelivery> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleDeliveryIdentity {
    pub bundle_digest: String,
    pub descriptor_digest: String,
    pub compatibility_identity: String,
    pub platform: String,
}

impl BundleDeliveryIdentity {
    pub fn from_descriptor(descriptor: &erf_bundle_cache::BundleDeliveryDescriptor) -> Self {
        Self {
            bundle_digest: prefixed_digest(&descriptor.bundle_digest),
            descriptor_digest: prefixed_digest(&descriptor.descriptor_digest()),
            compatibility_identity: descriptor.compatibility_identity.clone(),
            platform: descriptor.platform.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("bundle_digest", self.bundle_digest.as_str()),
            ("descriptor_digest", self.descriptor_digest.as_str()),
        ] {
            let Some(hex) = value.strip_prefix("sha256:") else {
                return Err(format!("delivery identity {name} is not a SHA-256 digest"));
            };
            if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
                return Err(format!("delivery identity {name} is not lowercase hexadecimal"));
            }
        }
        for (name, value) in [
            ("compatibility_identity", self.compatibility_identity.as_str()),
            ("platform", self.platform.as_str()),
        ] {
            if value.is_empty() || value.chars().any(char::is_control) {
                return Err(format!("delivery identity {name} is empty or contains control text"));
            }
        }
        Ok(())
    }
}

impl BundleDeliveryConfig {
    fn resolve(&self) -> Result<(BundleDeliveryIdentity, Vec<u8>), ScanError> {
        self.descriptor.validate().map_err(|error| classified_delivery("input", error.to_string()))?;
        let verification = self.verification.as_ref().ok_or_else(|| {
            classified_delivery("trust", "delivery verification context is required before scan")
        })?;
        verify_delivery_context(
            &self.descriptor,
            verification,
            &self.expected_compatibility_identity,
            &self.expected_platform,
            self.now_epoch_secs,
        )
        .map_err(|error| match error {
            ScanError::Delivery(detail) => ScanError::Delivery(classify_delivery_detail(&detail)),
            other => other,
        })?;
        let resolved = self.resolve_verified_bytes()?;
        let identity = BundleDeliveryIdentity::from_descriptor(&self.descriptor);
        identity.validate().map_err(|error| classified_delivery("input", error))?;
        Ok((identity, resolved.bytes))
    }

    // Called only after verification; cache availability is transport-independent.
    fn resolve_verified_bytes(&self) -> Result<erf_bundle_cache::ResolvedBundle, ScanError> {
        let cache = erf_bundle_cache::FileBundleCache::new(&self.cache_root);
        match erf_bundle_cache::resolve_cached(&cache, &self.descriptor, self.profile, self.evidence, self.now_epoch_secs) {
            Ok(bundle) => Ok(bundle),
            Err(erf_bundle_cache::DeliveryError::ServerFetchRequired) => {
                let client = erf_bundle_delivery::DeliveryClient::new(
                    &self.endpoint, std::time::Duration::from_millis(self.timeout_ms),
                ).map_err(|error| classified_delivery("input", error.to_string()))?;
                BUNDLE_DELIVERY_SINGLE_FLIGHT.get_or_init(Default::default)
                    .resolve(&client, &cache, &self.cache_root, &self.descriptor, self.profile, self.evidence, self.now_epoch_secs)
                    .map_err(|error| ScanError::Delivery(classify_delivery_detail(&error.to_string())))
            }
            Err(error) => Err(ScanError::Delivery(classify_delivery_detail(&error.to_string()))),
        }
    }

}

pub fn verify_delivery_context(
    descriptor: &erf_bundle_cache::BundleDeliveryDescriptor,
    verification: &BundleDeliveryVerification,
    expected_compatibility_identity: &str,
    expected_platform: &str,
    now_epoch_secs: u64,
) -> Result<(), ScanError> {
    if descriptor.compatibility_identity != expected_compatibility_identity
        || descriptor.platform != expected_platform
    {
        return Err(ScanError::Delivery(
            "delivery compatibility or platform does not match the Runner context".into(),
        ));
    }
    let expected = [
        ("rule_release_digest", descriptor.rule_release_digest, verification.inputs.rule_release_digest),
        ("policy_profile_digest", descriptor.policy_profile_digest, verification.inputs.policy_profile_digest),
        ("execution_lock_digest", descriptor.execution_lock_digest, verification.inputs.execution_lock_digest),
        (
            "ontology_contract_pack_digest",
            descriptor.ontology_contract_pack_digest,
            verification.inputs.ontology_contract_pack_digest,
        ),
    ];
    if let Some((name, _, _)) = expected.iter().find(|(_, descriptor_digest, input_digest)| descriptor_digest != input_digest) {
        return Err(ScanError::Delivery(format!(
            "verified RuleBundle input does not match descriptor: {name}"
        )));
    }
    if descriptor.revocation_snapshot_digest != verification.revocation_snapshot_digest {
        return Err(ScanError::Delivery(
            "verified revocation snapshot does not match descriptor".into(),
        ));
    }
    let snapshot = verification.revocation_snapshot.as_ref().ok_or_else(|| {
        ScanError::Delivery("verified revocation snapshot content is missing".into())
    })?;
    if snapshot.subject_digest() != descriptor.revocation_snapshot_digest {
        return Err(ScanError::Delivery(
            "verified revocation snapshot digest does not match descriptor".into(),
        ));
    }
    let mut referenced_assets = vec![
        descriptor.bundle_digest,
        descriptor.rule_release_digest,
        descriptor.policy_profile_digest,
        descriptor.execution_lock_digest,
        descriptor.ontology_contract_pack_digest,
        descriptor.artifact_statement_digest,
    ];
    referenced_assets.extend(descriptor.runtime_pack_digests.iter().copied());
    if referenced_assets.iter().any(|digest| snapshot.is_asset_revoked(digest)) {
        return Err(ScanError::Delivery(
            "verified revocation snapshot revokes a delivered asset".into(),
        ));
    }
    erf_rule_bundle::RuleBundleVerifier::new(expected_compatibility_identity, expected_platform)
    .verify_and_measure_bound_to_digest(
        &verification.signed_bundle,
        &verification.manifest_bytes,
        verification.inputs,
        descriptor.bundle_digest,
        &verification.trust_store,
        verification.trust_metadata.as_ref(),
        now_epoch_secs,
    )
    .map_err(|error| ScanError::Delivery(format!("signed RuleBundle verification failed: {error}")))?;

    if verification.packs.len() != descriptor.runtime_pack_digests.len() {
        return Err(ScanError::Delivery(
            "verified runtime Pack count does not match descriptor".into(),
        ));
    }
    if verification.packs.len() != 2
        || verification.packs[0].kind != BundlePackKind::BackendRuntime
        || verification.packs[1].kind != BundlePackKind::WorkspaceResolver
    {
        return Err(ScanError::Delivery(
            "verified runtime Packs must contain backendRuntime followed by workspaceResolver".into(),
        ));
    }
    for (index, pack) in verification.packs.iter().enumerate() {
        if let Some(layout) = &pack.oci_layout {
            verify_oci_layout(&pack.manifest_bytes, layout).map_err(|error| {
                ScanError::Delivery(format!("runtime Pack OCI layout verification failed: {error}"))
            })?;
        }
        let measurement = match pack.kind {
            BundlePackKind::BackendRuntime => erf_packs::OciPackVerifier::backend_runtime(),
            BundlePackKind::WorkspaceResolver => erf_packs::OciPackVerifier::workspace_resolver(),
        }
        .with_compatibility_identity(expected_compatibility_identity)
        .verify_and_measure(
            &pack.release,
            &pack.manifest_bytes,
            &verification.trust_store,
            verification.trust_metadata.as_ref(),
            now_epoch_secs,
        )
        .map_err(|error| ScanError::Delivery(format!("runtime Pack verification failed: {error}")))?;
        if measurement.manifest_digest != descriptor.runtime_pack_digests[index] {
            return Err(ScanError::Delivery(format!(
                "runtime Pack digest does not match descriptor at index {index}"
            )));
        }
        if pack.manifest_platform != expected_platform
            || pack.manifest_compatibility_identity != expected_compatibility_identity
        {
            return Err(ScanError::Delivery(format!(
                "runtime Pack manifest identity does not match the delivery context at index {index}"
            )));
        }
        if index == 0 && measurement.statement_digest != descriptor.artifact_statement_digest {
            return Err(ScanError::Delivery(
                "runtime Pack statement digest does not match descriptor artifact statement".into(),
            ));
        }
    }
    Ok(())
}

/// Verifies the content-addressed OCI Image Layout carried by a standard
/// delivery.  The descriptor is allowed to transport the four layout files
/// as bytes; the checks below still require the same blob digests, sizes,
/// platform and config identity as a materialized OCI layout.
pub fn verify_oci_layout(manifest_bytes: &[u8], layout: &OciLayoutVerification) -> Result<(), String> {
    let layout_value = serde_json::from_slice::<Value>(&layout.layout_bytes)
        .map_err(|error| format!("oci-layout is invalid JSON: {error}"))?;
    if layout_value.get("imageLayoutVersion").and_then(Value::as_str) != Some("1.0.0") {
        return Err("oci-layout imageLayoutVersion must be 1.0.0".into());
    }
    let index = serde_json::from_slice::<Value>(&layout.index_bytes)
        .map_err(|error| format!("OCI index.json is invalid JSON: {error}"))?;
    if index.get("schemaVersion").and_then(Value::as_u64) != Some(2) {
        return Err("OCI index schemaVersion must be 2".into());
    }
    let manifests = index
        .get("manifests")
        .and_then(Value::as_array)
        .ok_or_else(|| "OCI index manifests is missing".to_string())?;
    if manifests.len() != 1 {
        return Err("OCI index must contain exactly one manifest".into());
    }
    let manifest_digest: [u8; 32] = Sha256::digest(manifest_bytes).into();
    let manifest_digest_text = format!("sha256:{}", hex_digest(&manifest_digest));
    let index_manifest = manifests[0]
        .as_object()
        .ok_or_else(|| "OCI index manifest descriptor must be an object".to_string())?;
    if index_manifest.get("mediaType").and_then(Value::as_str) != Some(erf_packs::OCI_MANIFEST_MEDIA_TYPE)
        || index_manifest.get("digest").and_then(Value::as_str) != Some(manifest_digest_text.as_str())
        || index_manifest.get("size").and_then(Value::as_u64) != Some(manifest_bytes.len() as u64)
    {
        return Err("OCI index manifest descriptor does not bind the supplied manifest".into());
    }
    let platform = index_manifest
        .get("platform")
        .and_then(Value::as_object)
        .ok_or_else(|| "OCI index manifest platform is missing".to_string())?;
    if platform.get("os").and_then(Value::as_str) != Some("linux")
        || platform.get("architecture").and_then(Value::as_str) != Some("amd64")
    {
        return Err("OCI index manifest platform must be linux/amd64".into());
    }

    let manifest = serde_json::from_slice::<Value>(manifest_bytes)
        .map_err(|error| format!("OCI manifest is invalid JSON: {error}"))?;
    let config = manifest
        .get("config")
        .and_then(Value::as_object)
        .ok_or_else(|| "OCI manifest config is missing".to_string())?;
    let config_digest: [u8; 32] = Sha256::digest(&layout.config_bytes).into();
    let config_bytes_digest = format!("sha256:{}", hex_digest(&config_digest));
    if config.get("mediaType").and_then(Value::as_str) != Some("application/vnd.erf.runtime-pack.config.v0")
        || config.get("digest").and_then(Value::as_str) != Some(config_bytes_digest.as_str())
        || config.get("size").and_then(Value::as_u64) != Some(layout.config_bytes.len() as u64)
    {
        return Err("OCI config blob does not match the manifest descriptor".into());
    }
    let layer = manifest
        .get("layers")
        .and_then(Value::as_array)
        .and_then(|layers| (layers.len() == 1).then(|| &layers[0]))
        .and_then(Value::as_object)
        .ok_or_else(|| "OCI manifest must contain exactly one layer".to_string())?;
    let layer_digest: [u8; 32] = Sha256::digest(&layout.layer_bytes).into();
    let layer_bytes_digest = format!("sha256:{}", hex_digest(&layer_digest));
    if layer.get("mediaType").and_then(Value::as_str) != Some("application/vnd.erf.runtime-pack.asset.v0")
        || layer.get("digest").and_then(Value::as_str) != Some(layer_bytes_digest.as_str())
        || layer.get("size").and_then(Value::as_u64) != Some(layout.layer_bytes.len() as u64)
    {
        return Err("OCI layer blob does not match the manifest descriptor".into());
    }
    if manifest.get("assetDigest").and_then(Value::as_str) != Some(layer_bytes_digest.as_str()) {
        return Err("OCI manifest assetDigest does not match its layer".into());
    }
    let config_value = serde_json::from_slice::<Value>(&layout.config_bytes)
        .map_err(|error| format!("OCI config blob is invalid JSON: {error}"))?;
    for field in ["kind", "platform", "compatibilityIdentity"] {
        if config_value.get(field) != manifest.get(field) {
            return Err(format!("OCI config {field} does not match the manifest"));
        }
    }
    let expected_asset_type = match manifest.get("kind").and_then(Value::as_str) {
        Some("backendRuntime") => erf_packs::BACKEND_RUNTIME_PACK_ASSET_TYPE,
        Some("workspaceResolver") => erf_packs::WORKSPACE_RESOLVER_PACK_ASSET_TYPE,
        Some(other) => return Err(format!("unsupported OCI Runtime Pack kind: {other}")),
        None => return Err("OCI manifest kind is missing".into()),
    };
    if config_value.get("assetType").and_then(Value::as_str) != Some(expected_asset_type) {
        return Err("OCI config assetType does not match the manifest kind".into());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectScanScope {
    pub modules: Vec<String>,
    pub roots: Vec<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl ProjectScanScope {
    pub fn validate(&self) -> Result<(), String> {
        if self.modules.is_empty() || self.roots.is_empty() || self.include.is_empty() {
            return Err("project config selected an empty scan scope".into());
        }
        for (field, values) in [("modules", &self.modules), ("roots", &self.roots), ("include", &self.include), ("exclude", &self.exclude)] {
            for value in values {
                if value.trim().is_empty() || value.chars().any(char::is_control) || value.starts_with('/') || value.split('/').any(|part| part == "..") {
                    return Err(format!("project scope has an invalid {field} path: {value}"));
                }
            }
        }
        Ok(())
    }

    fn selects(&self, path: &str) -> bool {
        let path = normalize_scope_path(path);
        let in_module = self.modules.iter().any(|module| path_under(&path, module));
        let in_root = self.roots.iter().any(|root| path_under(&path, root));
        let included = self.include.iter().any(|pattern| glob_match(&normalize_scope_path(pattern), &path));
        let excluded = self.exclude.iter().any(|pattern| glob_match(&normalize_scope_path(pattern), &path));
        in_module && in_root && included && !excluded
    }
}

fn normalize_scope_path(value: &str) -> String {
    value.trim().replace('\\', "/").trim_start_matches("./").trim_end_matches('/').to_string()
}

fn path_under(path: &str, root: &str) -> bool {
    let root = normalize_scope_path(root);
    root.is_empty() || root == "." || path == root || path.starts_with(&format!("{root}/"))
}

/// Small deterministic glob matcher for the closed project-config subset.
/// `*` stays within one path segment; `**` may cross segments.
fn glob_match(pattern: &str, path: &str) -> bool {
    fn matches(pattern: &[u8], path: &[u8]) -> bool {
        if pattern.is_empty() {
            return path.is_empty();
        }
        if pattern.starts_with(b"**") {
            return matches(&pattern[2..], path) || (!path.is_empty() && matches(pattern, &path[1..]));
        }
        if pattern[0] == b'*' {
            return matches(&pattern[1..], path) || (!path.is_empty() && path[0] != b'/' && matches(pattern, &path[1..]));
        }
        !path.is_empty() && pattern[0] == path[0] && matches(&pattern[1..], &path[1..])
    }
    matches(pattern.as_bytes(), path.as_bytes())
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
    pub resolver_sandbox_provider_id: String,
    pub resolver_sandbox_policy_digest: String,
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

pub const MAX_EXECUTION_LOCK_BYTES: u64 = 1024 * 1024;

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
            self.resolver_sandbox_provider_id.as_str(),
            self.resolver_sandbox_policy_digest.as_str(),
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
            self.resolver_sandbox_provider_id.as_str(),
            self.resolver_sandbox_policy_digest.as_str(),
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
            ("resolver_sandbox_policy_digest", &self.resolver_sandbox_policy_digest),
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
            ("resolver_sandbox_provider_id", &self.resolver_sandbox_provider_id),
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
        self.verify_with_store_at(&self.trust_store, self.metadata.as_ref(), now_epoch_secs)?;
        Ok(())
    }

    pub fn verify_with_store_at(
        &self,
        store: &erf_trust::TrustStore,
        metadata: Option<&erf_trust::TrustMetadata>,
        now_epoch_secs: u64,
    ) -> Result<(), String> {
        self.lock.validate()?;
        if self.asset.payload != self.lock.canonical_payload() {
            return Err("signed execution lock payload does not match lock".into());
        }
        if let Some(metadata) = metadata {
            store
                .verify_with_metadata(&self.asset, &self.role, metadata, now_epoch_secs)
                .map_err(|error| error.to_string())?;
        } else {
            store.verify(&self.asset, &self.role).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

fn read_lock_values(path: &Path, signed: bool) -> Result<std::collections::BTreeMap<String, String>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect execution lock: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err("execution lock must be a regular non-symlink file".into());
    }
    if metadata.len() > MAX_EXECUTION_LOCK_BYTES {
        return Err("execution lock exceeds the configured size limit".into());
    }
    let mut text = String::new();
    File::open(path)
        .map_err(|error| format!("cannot read execution lock: {error}"))?
        .take(MAX_EXECUTION_LOCK_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("cannot read execution lock: {error}"))?;
    if text.len() as u64 > MAX_EXECUTION_LOCK_BYTES {
        return Err("execution lock exceeds the configured size limit".into());
    }
    let mut values = std::collections::BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let (key, value) = line.split_once('=').ok_or_else(|| format!("invalid execution lock line: {line}"))?;
        let allowed = matches!(key.trim(), "rule_bundle_digest" | "runner_release_digest" | "backend_pack_digest" | "resolver_pack_digest" | "sandbox_provider_id" | "sandbox_policy_digest" | "resolver_sandbox_provider_id" | "resolver_sandbox_policy_digest" | "conformance_suite_digest" | "resolver_id" | "resolver_version" | "backend_id" | "backend_version" | "protocol_major" | "canonical_set_version" | "lock_digest")
            || signed && matches!(key.trim(), "key_id" | "role" | "public_key_hex" | "signature_hex" | "metadata_version" | "expires_at_epoch_secs" | "revoked_key_ids");
        if !allowed { return Err(format!("unknown execution lock key: {key}")); }
        if values.insert(key.trim().to_string(), value.trim().to_string()).is_some() { return Err(format!("duplicate execution lock key: {key}")); }
    }
    Ok(values)
}

fn lock_from_values(values: &std::collections::BTreeMap<String, String>) -> Result<ExecutionLock, String> {
    let get = |key: &str| values.get(key).cloned().ok_or_else(|| format!("missing execution lock key: {key}"));
    let protocol_major = get("protocol_major")?.parse().map_err(|_| "invalid execution lock protocol_major".to_string())?;
    Ok(ExecutionLock { rule_bundle_digest: get("rule_bundle_digest")?, runner_release_digest: get("runner_release_digest")?, backend_pack_digest: get("backend_pack_digest")?, resolver_pack_digest: get("resolver_pack_digest")?, sandbox_provider_id: get("sandbox_provider_id")?, sandbox_policy_digest: get("sandbox_policy_digest")?, resolver_sandbox_provider_id: get("resolver_sandbox_provider_id")?, resolver_sandbox_policy_digest: get("resolver_sandbox_policy_digest")?, conformance_suite_digest: get("conformance_suite_digest")?, resolver_id: get("resolver_id")?, resolver_version: get("resolver_version")?, backend_id: get("backend_id")?, backend_version: get("backend_version")?, protocol_major, canonical_set_version: get("canonical_set_version")?, lock_digest: get("lock_digest")? })
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
    /// Facts the Resolver could not determine without stronger inputs. The
    /// full list is preserved for diagnostics; only facts required by the
    /// selected backend affect Gate completeness.
    pub resolver_unresolved_facts: Vec<String>,
    pub resolver_required_unresolved_facts: Vec<String>,
    pub planned_unit_ids: Vec<String>,
    pub unit_statuses: Vec<host_pb::UnitStatus>,
    pub scan_plan: Option<erf_scan_plan::ScanPlan>,
    pub plan_authority: Option<erf_scan_plan::PlanAuthority>,
    pub plan_completeness: Option<erf_scan_plan::CompletenessReport>,
    pub fallback_decision: Option<erf_incremental_baseline::FallbackDecision>,
    pub stage_telemetry: Option<erf_incremental_baseline::StageTelemetry>,
    pub bundle_delivery_identity: Option<BundleDeliveryIdentity>,
}

/// Runs one scan attempt. Findings are moved into the result only after every
/// planned unit has exactly one terminal status and ExecuteComplete agrees.
pub fn scan(config: &ScanConfig) -> Result<ScanResult, ScanError> {
    let mut effective_frozen_plan = config.frozen_plan.clone();
    if let Some(scope) = &config.project_scope {
        scope.validate().map_err(ScanError::Plan)?;
    }
    let mut fallback_decision = config
        .fallback_inputs
        .clone()
        .map(erf_incremental_baseline::assess_fallback)
        .transpose()
        .map_err(|error| ScanError::Plan(format!("invalid incremental fallback inputs: {error}")))?;
    let mut incremental_cache_observation = erf_incremental_baseline::CacheObservation::NotUsed;
    if let (Some(plan), Some(inputs)) = (&config.frozen_plan, &config.fallback_inputs) {
        if inputs.plan.plan_digest != plan.digest() {
            return Err(ScanError::Plan("fallback inputs do not bind the frozen ScanPlan".into()));
        }
    }
    if fallback_decision.as_ref().is_some_and(erf_incremental_baseline::FallbackDecision::is_fallback)
        && !config.frozen_plan.as_ref().is_some_and(|plan| plan.mode() == erf_scan_plan::PlanMode::Full)
    {
        return Err(ScanError::Plan("incremental safety fallback requires a FULL ScanPlan".into()));
    }
    if config.incremental.is_some() && config.frozen_plan.is_some() {
        return Err(ScanError::Plan("incremental request cannot be combined with an externally frozen ScanPlan".into()));
    }
    if let Some(request) = &config.incremental {
        verify_incremental_checkout(&config.repo_root, &request.head)?;
    }
    let mut stage_telemetry = erf_incremental_baseline::StageTelemetry::new();
    if let Some(signed_lock) = &config.signed_execution_lock {
        if let Some(delivery) = &config.bundle_delivery {
            let verification = delivery.verification.as_ref().ok_or_else(|| {
                ScanError::Trust("delivery verification context is required for ExecutionLock trust".into())
            })?;
            signed_lock
                .verify_with_store_at(
                    &verification.trust_store,
                    verification.trust_metadata.as_ref(),
                    delivery.now_epoch_secs,
                )
                .map_err(ScanError::Trust)?;
        } else {
            signed_lock.verify().map_err(ScanError::Trust)?;
        }
        if let Some(unsigned_lock) = &config.execution_lock {
            if unsigned_lock != &signed_lock.lock {
                return Err(ScanError::Trust("unsigned and signed execution locks differ".into()));
            }
        }
    }
    if config.bundle_delivery.is_some() && config.signed_execution_lock.is_none() {
        return Err(classified_delivery(
            "trust",
            "delivery scans require a signed Execution identity",
        ));
    }
    let delivered = if let Some(delivery) = &config.bundle_delivery {
        Some(delivery.resolve()?)
    } else {
        None
    };
    let materialized_bundle = match (&config.bundle_delivery, &config.rule_bundle, delivered.as_ref()) {
        (Some(delivery), None, Some((_, bytes))) => Some(materialize_delivered_bundle(
            &delivery.cache_root,
            &config.rule_id,
            &delivery.descriptor,
            bytes,
        ).map_err(|error| match error {
            ScanError::Delivery(detail) => ScanError::Delivery(classify_delivery_detail(&detail)),
            other => other,
        })?),
        _ => None,
    };
    let rule_bundle_path = config.rule_bundle.clone().or(materialized_bundle);
    let execution_lock = config.signed_execution_lock.as_ref().map(|lock| &lock.lock).or(config.execution_lock.as_ref());
    if config.bundle_delivery.is_some() && execution_lock.is_none() {
        return Err(classified_delivery(
            "trust",
            "delivery scans require an active Execution identity",
        ));
    }
    let validated_bundle = if let Some(bundle) = &rule_bundle_path {
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
    let bundle_delivery_identity = if let Some((identity, delivered_bytes)) = delivered {
        let bundle = validated_bundle.as_ref().ok_or_else(|| {
            classified_delivery("trust", "delivery did not produce a validated RuleBundle candidate")
        })?;
        let expected_digest = format!("sha256:{}", bundle.digest);
        if identity.bundle_digest != expected_digest {
            return Err(classified_delivery(
                "trust",
                "delivered bundle digest does not match the local candidate",
            ));
        }
        let canonical_bytes = bundle_payload_bytes(Path::new(rule_bundle_path.as_deref().ok_or_else(|| {
            classified_delivery("trust", "delivery RuleBundle path is missing")
        })?))?;
        if canonical_bytes != delivered_bytes {
            return Err(classified_delivery(
                "trust",
                "delivered bundle bytes do not match the local candidate payload",
            ));
        }
        Some(identity)
    } else {
        None
    };
    if let (Some(delivery), Some(lock)) = (&config.bundle_delivery, execution_lock) {
        let expected_lock_digest = format!("sha256:{}", lock.content_digest());
        if prefixed_digest(&delivery.descriptor.execution_lock_digest) != expected_lock_digest {
            return Err(classified_delivery(
                "trust",
                "delivery execution-lock digest does not match the active execution identity",
            ));
        }
        if delivery.descriptor.runtime_pack_digests.len() != 2
            || lock.backend_pack_digest != hex_digest(&delivery.descriptor.runtime_pack_digests[0])
            || lock.resolver_pack_digest != hex_digest(&delivery.descriptor.runtime_pack_digests[1])
        {
            return Err(classified_delivery(
                "trust",
                "execution identity does not bind the verified Runtime Pack measurements",
            ));
        }
    }
    let resolve_timer = start_stage_timer(erf_incremental_baseline::Stage::Resolve, "resolver", &config.resolver_command)?;
    let resolver_sandbox_identity = if let Some(policy) = &config.resolver_sandbox_policy {
        if policy.repository_root != Path::new(&config.repo_root)
            || rule_bundle_path.as_ref().map(Path::new) != Some(policy.rule_bundle.as_path())
        {
            return Err(ScanError::Sandbox("resolver sandbox mounts do not match scan inputs".into()));
        }
        let provider = erf_sandbox::DockerSandboxProvider::default();
        provider.preflight().map_err(|error| ScanError::Sandbox(error.to_string()))?;
        let prepared = provider.prepare(policy).map_err(|error| ScanError::Sandbox(error.to_string()))?;
        let mut command = Command::new(&prepared.command[0]);
        command.args(&prepared.command[1..]);
        Some((prepared.provider_id.to_owned(), prepared.policy_digest, command))
    } else {
        let mut command = Command::new(&config.resolver_command);
        command.args(&config.resolver_args);
        Some((String::new(), String::new(), command))
    };
    let backend_mode = match execution_lock.map(|lock| lock.backend_id.as_str()) {
        Some("spotbugs-java") => "spotbugs",
        Some("pmd-java") => "pmd",
        Some(backend_id) if config.bundle_delivery.is_some() => {
            return Err(ScanError::Trust(format!(
                "unsupported signed delivery backend identity: {backend_id}"
            )));
        }
        Some(_) | None => {
            if std::env::var("ERF_BACKEND").as_deref() == Ok("spotbugs") {
                "spotbugs"
            } else {
                "pmd"
            }
        }
    };
    let (resolver_provider_id, resolver_policy_digest, mut resolver_command) = resolver_sandbox_identity
        .expect("resolver command identity is always prepared");
    resolver_command.env("ERF_BACKEND", backend_mode);
    let mut resolver_config = ResolverConfig {
        required_capabilities: vec!["ecosystem.static-layout".into()],
        ..Default::default()
    };
    resolver_config.limits.deadline_ms = config.resolver_deadline_ms;
    let (mut resolver, _) = match ResolverHandle::spawn(&mut resolver_command, &resolver_config) {
        Ok(value) => value,
        Err(error) => {
            record_stage_with_outcome(
                &mut stage_telemetry,
                resolve_timer,
                erf_incremental_baseline::StageOutcome::Failed,
            )?;
            return incomplete_result(
                rule_id.clone(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                format!("workspace resolver did not start: {error}"),
                effective_frozen_plan.as_ref(),
                fallback_decision.clone(),
                bundle_delivery_identity.clone(),
                stage_telemetry,
                Vec::new(),
                Vec::new(),
            );
        }
    };
    let resolver_root = if resolver_provider_id.is_empty() { &config.repo_root } else { "/workspace" };
    let descriptor = match resolver.resolve(resolver_root) {
        Ok(descriptor) => descriptor,
        Err(error) => {
            record_stage_with_outcome(
                &mut stage_telemetry,
                resolve_timer,
                erf_incremental_baseline::StageOutcome::Failed,
            )?;
            return incomplete_result(
                rule_id.clone(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                format!("workspace resolution did not complete: {error}"),
                effective_frozen_plan.as_ref(),
                fallback_decision.clone(),
                bundle_delivery_identity.clone(),
                stage_telemetry,
                Vec::new(),
                Vec::new(),
            );
        }
    };
    match workspace_pb::WorkspaceResolutionMode::try_from(descriptor.mode) {
        Ok(workspace_pb::WorkspaceResolutionMode::SafeStatic) => {
            if config.workspace_attestation.is_some() {
                record_stage_with_outcome(
                    &mut stage_telemetry,
                    resolve_timer,
                    erf_incremental_baseline::StageOutcome::Failed,
                )?;
                return Err(ScanError::Trust(
                    "WorkspaceAttestation supplied for SAFE_STATIC resolution".into(),
                ));
            }
        }
        Ok(workspace_pb::WorkspaceResolutionMode::AttestedBuildExport) => {
            let lock = execution_lock.ok_or_else(|| {
                ScanError::Trust(
                    "ATTESTED_BUILD_EXPORT requires a signed ExecutionLock".into(),
                )
            })?;
            let attestation = config.workspace_attestation.as_ref().ok_or_else(|| {
                ScanError::Trust(
                    "ATTESTED_BUILD_EXPORT requires a verified WorkspaceAttestation".into(),
                )
            })?;
            validate_attested_java_context(&descriptor)?;
            attestation.verify(&descriptor.descriptor_sha256, lock, Path::new(&config.repo_root))?;
        }
        Ok(workspace_pb::WorkspaceResolutionMode::Unspecified) | Err(_) => {
            return Err(ScanError::Trust(
                "Workspace Resolver returned an invalid resolution mode".into(),
            ));
        }
    }
    let resolver_unresolved_facts = descriptor.unresolved_facts.clone();
    let resolver_required_unresolved_facts = required_unresolved_facts(&descriptor, backend_mode);
    if let Some(lock) = execution_lock {
        if descriptor.resolver_id != lock.resolver_id || descriptor.resolver_version != lock.resolver_version {
            return Err(ScanError::Trust(format!("execution lock resolver mismatch: expected {}/{} got {}/{}", lock.resolver_id, lock.resolver_version, descriptor.resolver_id, descriptor.resolver_version)));
        }
        if !resolver_provider_id.is_empty()
            && (lock.resolver_sandbox_provider_id != resolver_provider_id
                || lock.resolver_sandbox_policy_digest != resolver_policy_digest)
        {
            return Err(ScanError::Trust("execution lock resolver sandbox identity mismatch".into()));
        }
    }
    let completion = match resolver.complete() {
        Ok(completion) => completion,
        Err(error) => {
            record_stage_with_outcome(
                &mut stage_telemetry,
                resolve_timer,
                erf_incremental_baseline::StageOutcome::Failed,
            )?;
            return incomplete_result(
                rule_id.clone(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                descriptor.descriptor_sha256.clone(),
                Vec::new(),
                format!("workspace resolver completion was unavailable: {error}"),
                effective_frozen_plan.as_ref(),
                fallback_decision.clone(),
                bundle_delivery_identity.clone(),
                stage_telemetry,
                resolver_unresolved_facts.clone(),
                resolver_required_unresolved_facts.clone(),
            );
        }
    };
    if completion.status != workspace_pb::completion::Status::Completed as i32 {
        record_stage_with_outcome(
            &mut stage_telemetry,
            resolve_timer,
            erf_incremental_baseline::StageOutcome::Failed,
        )?;
        return incomplete_result(
            rule_id.clone(),
            Vec::new(),
            Vec::new(),
            descriptor.descriptor_sha256.clone(),
            Vec::new(),
            Vec::new(),
            format!(
                "workspace resolver completed with status {}: {}",
                completion.status, completion.reason
            ),
            effective_frozen_plan.as_ref(),
            fallback_decision.clone(),
            bundle_delivery_identity.clone(),
            stage_telemetry,
            resolver_unresolved_facts.clone(),
            resolver_required_unresolved_facts.clone(),
        );
    }
    record_stage(&mut stage_telemetry, resolve_timer)?;

    // Freeze the global denominator before asking the host to refine it.
    let mut module_ids_for_units = Vec::new();
    let all_units: Vec<host_pb::AnalysisUnit> = descriptor
        .modules
        .iter()
        .filter_map(|module| {
            let files = module
                .files
                .iter()
                .filter(|file| config.project_scope.as_ref().map_or(true, |scope| scope.selects(&file.path)))
                .map(|file| file.path.clone())
                .collect::<Vec<_>>();
            if config.project_scope.is_some() && files.is_empty() {
                None
            } else {
                module_ids_for_units.push(module.module_id.clone());
                Some(host_pb::AnalysisUnit {
                    unit_id: format!("{}:{}", module.module_id, rule_id),
                    language: module.language.clone(),
                    files,
                    classpath_entries: module.classpath_entries.clone(),
                    build_output_roots: module.build_output_roots.clone(),
                    invocation_identity: module.invocation_identity.clone(),
                    classpath_sha256: module.classpath_sha256.clone(),
                    build_output_sha256: module.build_output_sha256.clone(),
                })
            }
        })
        .collect();
    if config.project_scope.is_some() && all_units.is_empty() {
        return Err(ScanError::Plan("project config selected no resolver files".into()));
    }
    if let Some(request) = &config.incremental {
        let range = erf_git_scope::GitChangeRange::new(&request.base, &request.head)
            .map_err(|error| ScanError::Plan(format!("incremental Git range is invalid: {error}")))?;
        let changed = erf_git_scope::GitChangeResolver::new(&config.repo_root)
            .resolve(&range)
            .map_err(|error| ScanError::Plan(format!("incremental ChangedSet could not be resolved: {error}")))?;
        let affected = erf_affected_scope::AffectedScopeResolver::new(&config.repo_root)
            .resolve(&changed)
            .map_err(|error| ScanError::Plan(format!("incremental affected scope could not be resolved: {error}")))?;
        let backend = if request.backend_identity.contains("spotbugs") {
            erf_affected_scope::AnalysisKind::SpotBugsBytecode
        } else {
            erf_affected_scope::AnalysisKind::PmdAst
        };
        let tree = git_tree_identity(&config.repo_root, changed.range().head_commit())?;
        let plan_units = all_units
            .iter()
            .zip(module_ids_for_units.iter())
            .map(|(unit, module_id)| erf_scan_plan::AnalysisUnit {
                unit_id: unit.unit_id.clone(),
                backend,
                module_id: module_id.clone(),
                path: None,
            })
            .collect::<Vec<_>>();
        let plan_identity = erf_scan_plan::PlanIdentity {
            repository: request.repository.clone(),
            base: changed.range().base_commit().to_string(),
            head: changed.range().head_commit().to_string(),
            tree,
            scope_digest: affected.digest().to_string(),
            rule_bundle_digest: request.rule_bundle_digest.clone(),
            policy_digest: request.policy_digest.clone(),
            resolver_identity: request.resolver_identity.clone(),
            backend_identity: request.backend_identity.clone(),
            runner_identity: request.runner_identity.clone(),
            protocol_major: erf_host_client::PROTOCOL_MAJOR,
            canonical_set_version: request.canonical_set_version.clone(),
        };
        let freeze_plan = |requested_mode| {
            erf_scan_plan::ScanPlan::freeze(erf_scan_plan::ScanPlanRequest {
                identity: plan_identity.clone(),
                affected_scope: affected.clone(),
                requested_mode,
                units: plan_units.clone(),
            })
            .map_err(|error| ScanError::Plan(format!("incremental ScanPlan could not be frozen: {error}")))
        };
        let provisional_plan = freeze_plan(erf_scan_plan::PlanMode::Module)?;
        let mut requested_mode = erf_scan_plan::PlanMode::Module;
        if let Some(cache_dir) = &config.scan_cache_dir {
            let probe = erf_scan_plan::FileScanCache::new(cache_dir)
                .probe_plan(&provisional_plan, unix_now_epoch_secs())
                .map_err(|error| ScanError::Plan(format!("incremental scan cache probe failed: {error}")))?;
            match probe {
                erf_scan_plan::CacheProbe::WarmHit => {
                    incremental_cache_observation = erf_incremental_baseline::CacheObservation::WarmHit;
                }
                erf_scan_plan::CacheProbe::ColdMiss => {
                    incremental_cache_observation = erf_incremental_baseline::CacheObservation::ColdMiss;
                }
                erf_scan_plan::CacheProbe::Expired => {
                    incremental_cache_observation = erf_incremental_baseline::CacheObservation::Expired;
                    requested_mode = erf_scan_plan::PlanMode::Full;
                }
                erf_scan_plan::CacheProbe::Corrupt => {
                    incremental_cache_observation = erf_incremental_baseline::CacheObservation::Corrupt;
                    requested_mode = erf_scan_plan::PlanMode::Full;
                }
            }
        }
        let plan = if requested_mode == erf_scan_plan::PlanMode::Full {
            freeze_plan(erf_scan_plan::PlanMode::Full)?
        } else {
            provisional_plan
        };
        let plan_observation = erf_incremental_baseline::PlanObservation::new(
            plan.digest(),
            plan.mode(),
            plan.authority(),
            plan.planned_units().len(),
            true,
        )
        .map_err(|error| ScanError::Plan(format!("incremental plan observation is invalid: {error}")))?;
        let has_reason = |code| affected.reasons().iter().any(|reason| reason.code() == code);
        let fallback_inputs = erf_incremental_baseline::FallbackInputs {
            requested_route: erf_incremental_baseline::ExecutionRoute::Incremental,
            plan: plan_observation,
            cache: incremental_cache_observation,
            build: erf_incremental_baseline::BuildOutcome::Success,
            build_configuration_changed: has_reason(erf_affected_scope::ReasonCode::SharedBuildConfiguration)
                || has_reason(erf_affected_scope::ReasonCode::DynamicBuildGraph)
                || has_reason(erf_affected_scope::ReasonCode::BuildGraphIncomplete),
            rule_bundle_changed: has_reason(erf_affected_scope::ReasonCode::RuleConfigurationChanged),
            policy_changed: has_reason(erf_affected_scope::ReasonCode::PolicyConfigurationChanged),
            resolver_changed: has_reason(erf_affected_scope::ReasonCode::ResolverConfigurationChanged),
            dependency_graph: if has_reason(erf_affected_scope::ReasonCode::DynamicBuildGraph)
                || has_reason(erf_affected_scope::ReasonCode::BuildGraphIncomplete)
            {
                erf_incremental_baseline::DependencyGraphState::Uncertain
            } else {
                erf_incremental_baseline::DependencyGraphState::Proven
            },
            generated_source: if has_reason(erf_affected_scope::ReasonCode::GeneratedSourcesUncertain) {
                erf_incremental_baseline::GeneratedSourceState::Uncertain
            } else {
                erf_incremental_baseline::GeneratedSourceState::Stable
            },
            merge_base: erf_incremental_baseline::MergeBaseState::Verifiable,
            affected_unit_count: plan.planned_units().len(),
            scope_budget: all_units.len().max(1),
        };
        fallback_decision = Some(
            erf_incremental_baseline::assess_fallback(fallback_inputs)
                .map_err(|error| ScanError::Plan(format!("incremental fallback assessment failed: {error}")))?,
        );
        effective_frozen_plan = Some(plan);
    }
    let scope_timer = start_stage_timer(erf_incremental_baseline::Stage::AffectedScope, "affected-scope", &hex_digest(&descriptor.descriptor_sha256))?;
    let (units, plan_digest) = if let Some(frozen_plan) = &effective_frozen_plan {
        let selected = frozen_plan.planned_unit_ids();
        let available = all_units.into_iter().map(|unit| (unit.unit_id.clone(), unit)).collect::<std::collections::BTreeMap<_, _>>();
        if available.len() != selected.len() && frozen_plan.mode() == erf_scan_plan::PlanMode::Full {
            return Err(ScanError::Plan("FULL ScanPlan omitted a resolver analysis unit".into()));
        }
        let units = selected.iter().map(|unit_id| available.get(unit_id).cloned().ok_or_else(|| ScanError::Plan("frozen ScanPlan contains an analysis unit absent from the WorkspaceDescriptor".into()))).collect::<Result<Vec<_>, _>>()?;
        if units.is_empty() { return Err(ScanError::Plan("frozen ScanPlan selected no resolver analysis units".into())); }
        (units, frozen_plan.digest_bytes().to_vec())
    } else {
        (all_units, Vec::new())
    };
    let planned_ids: Vec<String> = units.iter().map(|u| u.unit_id.clone()).collect();
    if let Some(frozen_plan) = &effective_frozen_plan {
        frozen_plan.validate_execution_unit_ids(&planned_ids).map_err(|error| ScanError::Plan(error.to_string()))?;
    }
    let planned = planned_ids.len();
    let plan_digest = if plan_digest.is_empty() { digest_plan(&units) } else { plan_digest };
    record_stage(&mut stage_telemetry, scope_timer)?;

    let sandboxed = config.sandbox_policy.is_some();
    let mut host_command = if let Some(policy) = &config.sandbox_policy {
        if policy.repository_root != Path::new(&config.repo_root)
            || rule_bundle_path.as_ref().map(Path::new) != Some(policy.rule_bundle.as_path())
        {
            return Err(ScanError::Sandbox("sandbox mounts do not match scan inputs".into()));
        }
        let provider = erf_sandbox::DockerSandboxProvider::default();
        provider.preflight().map_err(|error| ScanError::Sandbox(error.to_string()))?;
        let prepared = provider.prepare(policy).map_err(|error| ScanError::Sandbox(error.to_string()))?;
        if let Some(lock) = execution_lock {
            if lock.sandbox_provider_id != prepared.provider_id
                || lock.sandbox_policy_digest != prepared.policy_digest
            {
                return Err(ScanError::Trust("execution lock backend sandbox identity mismatch".into()));
            }
        }
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
        host_command.env("ERF_BACKEND", backend_mode);
        if let Some(bundle) = &validated_bundle {
            host_command.env("ERF_RULESET", &bundle.ruleset_path);
            host_command.env("ERF_RULE_BUNDLE", rule_bundle_path.as_deref().unwrap_or_default());
        }
        if let Some(identity) = &bundle_delivery_identity {
            host_command.env("ERF_RULE_BUNDLE_DIGEST", &identity.bundle_digest);
            host_command.env("ERF_BUNDLE_DELIVERY_DESCRIPTOR_DIGEST", &identity.descriptor_digest);
            host_command.env("ERF_BUNDLE_COMPATIBILITY", &identity.compatibility_identity);
            host_command.env("ERF_BUNDLE_PLATFORM", &identity.platform);
        }
    }
    let required_backend_capability = if backend_mode == "spotbugs" {
        "java.bytecode"
    } else {
        "java.ast"
    };
    let mut host_config = HostConfig {
        required_capabilities: vec![required_backend_capability.into()],
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
    let compile_timer = start_stage_timer(erf_incremental_baseline::Stage::Compile, "compile", &hex_digest(&plan_digest))?;
    let plan = host.plan(units)?;
    record_stage(&mut stage_telemetry, compile_timer)?;
    let returned: Vec<String> = plan.planned.iter().map(|u| u.unit_id.clone()).collect();
    if returned != planned_ids || !plan.unsupported.is_empty() {
        return Err(ScanError::Plan(format!(
            "host refinement changed frozen plan: expected {planned_ids:?}, got {returned:?}, unsupported={:?}",
            plan.unsupported
        )));
    }

    let backend_timer = start_stage_timer(erf_incremental_baseline::Stage::Backend, "backend", &hex_digest(&plan_digest))?;
    let mut stream = match host.execute(planned_ids.clone()) {
        Ok(stream) => stream,
        Err(error) => {
            record_stage_with_outcome(&mut stage_telemetry, backend_timer, erf_incremental_baseline::StageOutcome::Failed)?;
            return incomplete_result(
                rule_id.clone(),
                planned_ids,
                Vec::new(),
                plan_digest,
                descriptor.descriptor_sha256.clone(),
                Vec::new(),
                error.to_string(),
                effective_frozen_plan.as_ref(),
                fallback_decision.clone(),
                bundle_delivery_identity.clone(),
                stage_telemetry,
                resolver_unresolved_facts.clone(),
                resolver_required_unresolved_facts.clone(),
            );
        }
    };
    let mut statuses = Vec::new();
    let mut staging = AttemptStaging::new(config.max_staging_bytes)?;
    let (completion, attempt_failure) = loop {
        match stream.next_event() {
            Ok(Some(ExecEvent::Observation(observation))) => {
                if let Err(error) = validate_observation_blob_identity(&observation, &descriptor) {
                    break (None, Some(format!("observation identity rejected: {error}")));
                }
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
    record_stage_with_outcome(
        &mut stage_telemetry,
        backend_timer,
        if attempt_failure.is_some() {
            erf_incremental_baseline::StageOutcome::Failed
        } else {
            erf_incremental_baseline::StageOutcome::Completed
        },
    )?;
    if let Some(reason) = attempt_failure {
        return incomplete_result(rule_id.clone(), planned_ids, statuses, plan_digest, descriptor.descriptor_sha256.clone(), attempt_transcript_digest, reason, effective_frozen_plan.as_ref(), fallback_decision.clone(), bundle_delivery_identity.clone(), stage_telemetry, resolver_unresolved_facts.clone(), resolver_required_unresolved_facts.clone());
    }
    let completion = completion.expect("successful execute loop must have completion");
    if let Err(error) = host.complete() {
        return incomplete_result(rule_id.clone(), planned_ids, statuses, plan_digest, descriptor.descriptor_sha256.clone(), attempt_transcript_digest, error.to_string(), effective_frozen_plan.as_ref(), fallback_decision.clone(), bundle_delivery_identity.clone(), stage_telemetry, resolver_unresolved_facts.clone(), resolver_required_unresolved_facts.clone());
    }

    let mut status_ids: Vec<String> = statuses.iter().map(|s| s.unit_id.clone()).collect();
    status_ids.sort();
    let mut expected_ids = planned_ids.clone();
    expected_ids.sort();
    if status_ids != expected_ids || completion.units_reported as usize != planned {
        let reason = format!(
            "incomplete attempt: planned={planned_ids:?}, status={status_ids:?}, reported={}",
            completion.units_reported);
        return incomplete_result(rule_id.clone(), planned_ids, statuses, plan_digest, descriptor.descriptor_sha256.clone(), attempt_transcript_digest, reason, effective_frozen_plan.as_ref(), fallback_decision.clone(), bundle_delivery_identity.clone(), stage_telemetry, resolver_unresolved_facts.clone(), resolver_required_unresolved_facts.clone());
    }
    let failed = statuses
        .iter()
        .filter(|s| s.outcome != host_pb::UnitOutcome::Executed as i32)
        .count();
    let executed = planned - failed;
    // A failed unit invalidates the whole attempt. Keep the completeness
    // facts for governance, but never leak observations from a partial run.
    let cache_timer = start_stage_timer(erf_incremental_baseline::Stage::Cache, "cache", &hex_digest(&plan_digest))?;
    let committed_findings = if failed == 0 {
        let findings = staging.read_observations()?;
        record_stage(&mut stage_telemetry, cache_timer)?;
        findings
    } else {
        stage_telemetry
            .record_skipped(erf_incremental_baseline::Stage::Cache, erf_incremental_baseline::digest_summary(&format!("cache-skipped:failed-units:{}", failed)))
            .map_err(|error| ScanError::Plan(format!("cannot record skipped cache stage: {error}")))?;
        Vec::new()
    };
    let gate_timer = start_stage_timer(erf_incremental_baseline::Stage::Gate, "gate", &hex_digest(&plan_digest))?;
    let plan_completeness = completeness_report(
        effective_frozen_plan.as_ref(),
        &statuses,
        resolver_required_unresolved_facts.len(),
    )?;
    record_stage(&mut stage_telemetry, gate_timer)?;
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
        resolver_unresolved_facts,
        resolver_required_unresolved_facts,
        planned_unit_ids: planned_ids,
        unit_statuses: statuses,
        scan_plan: effective_frozen_plan.clone(),
        plan_authority: effective_frozen_plan.as_ref().map(|plan| plan.authority()),
        plan_completeness,
        fallback_decision,
        stage_telemetry: Some(stage_telemetry),
        bundle_delivery_identity,
    })
}

fn start_stage_timer(
    stage: erf_incremental_baseline::Stage,
    label: &str,
    input: &str,
) -> Result<erf_incremental_baseline::StageTimer, ScanError> {
    let input_digest = erf_incremental_baseline::digest_summary(&format!("{label}:{input}"));
    erf_incremental_baseline::StageTimer::start(stage, input_digest)
        .map_err(|error| ScanError::Plan(format!("cannot start {label} telemetry: {error}")))
}

fn record_stage(
    telemetry: &mut erf_incremental_baseline::StageTelemetry,
    timer: erf_incremental_baseline::StageTimer,
) -> Result<(), ScanError> {
    telemetry.finish(timer).map_err(|error| ScanError::Plan(format!("cannot record stage telemetry: {error}")))
}

fn record_stage_with_outcome(
    telemetry: &mut erf_incremental_baseline::StageTelemetry,
    timer: erf_incremental_baseline::StageTimer,
    outcome: erf_incremental_baseline::StageOutcome,
) -> Result<(), ScanError> {
    telemetry
        .finish_with_outcome(timer, outcome)
        .map_err(|error| ScanError::Plan(format!("cannot record stage telemetry: {error}")))
}

fn incomplete_result(
    rule_id: String,
    planned_ids: Vec<String>,
    statuses: Vec<host_pb::UnitStatus>,
    plan_digest: Vec<u8>,
    workspace_descriptor_digest: Vec<u8>,
    attempt_transcript_digest: Vec<u8>,
    reason: String,
    frozen_plan: Option<&erf_scan_plan::ScanPlan>,
    fallback_decision: Option<erf_incremental_baseline::FallbackDecision>,
    bundle_delivery_identity: Option<BundleDeliveryIdentity>,
    mut stage_telemetry: erf_incremental_baseline::StageTelemetry,
    resolver_unresolved_facts: Vec<String>,
    resolver_required_unresolved_facts: Vec<String>,
) -> Result<ScanResult, ScanError> {
    let reason_digest = erf_incremental_baseline::digest_summary(&format!("incomplete:{reason}"));
    for stage in erf_incremental_baseline::Stage::ALL {
        if !stage_telemetry.measurements().iter().any(|measurement| measurement.stage == stage) {
            stage_telemetry
                .record_skipped(stage, reason_digest.clone())
                .map_err(|error| ScanError::Plan(format!("cannot record skipped {stage:?} stage: {error}")))?;
        }
    }
    let failed = statuses
        .iter()
        .filter(|status| status.outcome != host_pb::UnitOutcome::Executed as i32)
        .count();
    let executed = statuses
        .iter()
        .filter(|status| status.outcome == host_pb::UnitOutcome::Executed as i32)
        .count();
    let plan_completeness = completeness_report(
        frozen_plan,
        &statuses,
        1 + resolver_required_unresolved_facts.len(),
    )?;
    Ok(ScanResult {
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
        resolver_unresolved_facts,
        resolver_required_unresolved_facts,
        planned_unit_ids: planned_ids,
        unit_statuses: statuses,
        scan_plan: frozen_plan.cloned(),
        plan_authority: frozen_plan.map(|plan| plan.authority()),
        plan_completeness,
        fallback_decision,
        stage_telemetry: Some(stage_telemetry),
        bundle_delivery_identity,
    })
}

/// Backend observations carry a content digest for source/file locations.
/// Bind that digest to the resolver's immutable workspace descriptor before
/// staging any observation; a backend must not be able to invent a digest or
/// point at a different file version and still reach Gate.
fn validate_observation_blob_identity(
    observation: &host_pb::BackendObservation,
    descriptor: &workspace_pb::WorkspaceDescriptor,
) -> Result<(), &'static str> {
    let location = observation.location.as_ref().ok_or("missing observation location")?;
    let (path, digest) = match location.kind.as_ref() {
        Some(host_pb::observation_location::Kind::SourceSpan(span)) => (&span.path, &span.blob_sha256),
        Some(host_pb::observation_location::Kind::File(file)) => (&file.path, &file.blob_sha256),
        Some(host_pb::observation_location::Kind::Symbol(_))
        | Some(host_pb::observation_location::Kind::BytecodeMember(_)) => return Ok(()),
        None => return Err("missing observation location kind"),
    };
    let mut matches = descriptor
        .modules
        .iter()
        .flat_map(|module| module.files.iter())
        .filter(|file| file.path == *path);
    let expected = matches
        .next()
        .ok_or("observation path is absent from workspace descriptor")?;
    if matches.next().is_some() {
        return Err("observation path is ambiguous in workspace descriptor");
    }
    if expected.blob_sha256.as_slice() != digest.as_slice() {
        return Err("observation blob digest does not match workspace descriptor");
    }
    Ok(())
}

fn completeness_report(
    frozen_plan: Option<&erf_scan_plan::ScanPlan>,
    statuses: &[host_pb::UnitStatus],
    observation_errors: usize,
) -> Result<Option<erf_scan_plan::CompletenessReport>, ScanError> {
    let Some(plan) = frozen_plan else { return Ok(None); };
    let accounts = statuses.iter().map(|status| {
        let outcome = if status.outcome == host_pb::UnitOutcome::Executed as i32 {
            erf_scan_plan::UnitOutcome::Executed
        } else if status.outcome == host_pb::UnitOutcome::NotApplicable as i32 {
            erf_scan_plan::UnitOutcome::NotApplicable
        } else if status.outcome == host_pb::UnitOutcome::Skipped as i32 {
            erf_scan_plan::UnitOutcome::Unsupported
        } else {
            erf_scan_plan::UnitOutcome::Failed
        };
        erf_scan_plan::UnitAccount { unit_id: status.unit_id.clone(), outcome, reason: (!status.detail.is_empty()).then(|| status.detail.clone()) }
    }).collect::<Vec<_>>();
    plan.account(&accounts, observation_errors).map(Some).map_err(|error| ScanError::Plan(error.to_string()))
}

fn validate_attested_java_context(
    descriptor: &workspace_pb::WorkspaceDescriptor,
) -> Result<(), ScanError> {
    for module in &descriptor.modules {
        if module.language != "java" {
            continue;
        }
        if module.invocation_identity.trim().is_empty()
            || module.classpath_sha256.len() != 32
            || module.build_output_sha256.len() != 32
        {
            return Err(ScanError::Trust(format!(
                "ATTESTED_BUILD_EXPORT Java module {} is missing digest-bound classpath, build-output, or invocation context",
                module.module_id
            )));
        }
    }
    Ok(())
}

fn required_unresolved_facts(
    descriptor: &workspace_pb::WorkspaceDescriptor,
    backend_mode: &str,
) -> Vec<String> {
    let mut facts = descriptor
        .unresolved_facts
        .iter()
        .filter(|fact| {
            let fact = fact.as_str();
            match backend_mode {
                // SpotBugs consumes bytecode and a dependency/classpath
                // closure; SAFE_STATIC cannot silently replace either with a
                // local guess.
                "spotbugs" => {
                    fact.contains("build-output")
                        || fact.contains("bytecode")
                        || fact.contains("dependency-classpath")
                        || fact.contains("module-structure")
                        || fact.contains("module-path")
                }
                // PMD can operate from source-only SAFE_STATIC facts, but it
                // still needs a trustworthy module/source-root closure.
                _ => {
                    fact.contains("module-structure")
                        || fact.contains("module-path")
                        || fact.contains("source-root")
                        || fact.contains("java-source-roots")
                        || fact.contains("module-directory")
                }
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    facts.sort();
    facts.dedup();
    facts
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

const MAX_MATERIALIZED_FILES: usize = 4096;
const MAX_MATERIALIZED_PATH_BYTES: usize = 4096;
const MATERIALIZED_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

/// Materializes the transport-neutral canonical payload only after the
/// signed RuleBundle/Pack context has been verified. The materialized tree is
/// content-addressed and kept under the same bounded cache root, so a Runner
/// can cold-fetch from the Server without granting the Server local path or
/// execution authority.
fn materialize_delivered_bundle(
    cache_root: &str,
    rule_id: &str,
    descriptor: &erf_bundle_cache::BundleDeliveryDescriptor,
    bytes: &[u8],
) -> Result<String, ScanError> {
    descriptor
        .validate()
        .map_err(|error| ScanError::Delivery(error.to_string()))?;
    if !is_valid_rule_id(rule_id) {
        return Err(ScanError::Delivery("delivery materialization requires a valid rule identity".into()));
    }
    if bytes.is_empty() || bytes.len() > erf_bundle_cache::MAX_BUNDLE_BYTES {
        return Err(ScanError::Delivery("delivered RuleBundle payload is empty or too large".into()));
    }
    let actual: [u8; 32] = Sha256::digest(bytes).into();
    if actual != descriptor.bundle_digest {
        return Err(ScanError::Delivery("delivered RuleBundle payload digest mismatch".into()));
    }

    // FileBundleCache performs the root/sha256 symlink checks and creates the
    // cache directories before the sibling materialized tree is touched.
    let cache = erf_bundle_cache::FileBundleCache::new(cache_root);
    cache
        .get(&descriptor.bundle_digest)
        .map_err(|error| ScanError::Delivery(format!("delivery cache root is unsafe: {error}")))?;
    let parent = cache.root().join("materialized");
    ensure_materialized_directory(&parent)?;
    let digest = hex_digest(&actual);
    let _lock = MaterializedLock::acquire(&parent, &digest)?;
    let target = parent.join(&digest);
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(ScanError::Delivery("materialized RuleBundle path is unsafe".into()));
        }
        Ok(_) => {
            let validated = validate_rule_bundle(&target)?;
            let canonical = bundle_payload_bytes(&target)?;
            if validated.rule_id != rule_id || canonical != bytes {
                return Err(ScanError::Delivery("materialized RuleBundle does not match the delivered payload".into()));
            }
            return Ok(target.to_string_lossy().into_owned());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(ScanError::Delivery(format!("cannot inspect materialized RuleBundle: {error}"))),
    }
    let (materialized_entries, materialized_bytes) = materialized_usage(&parent)?;
    let required_bytes = bytes.len() as u64 + 1024;
    if materialized_entries >= erf_bundle_cache::MAX_CACHE_ENTRIES
        || materialized_bytes > erf_bundle_cache::MAX_CACHE_BYTES.saturating_sub(required_bytes)
    {
        return Err(ScanError::Delivery("materialized RuleBundle cache capacity is exhausted".into()));
    }

    let temp = parent.join(format!(".{digest}.{}.tmp", unique_suffix()));
    fs::create_dir(&temp)
        .map_err(|error| ScanError::Delivery(format!("cannot create materialized RuleBundle staging directory: {error}")))?;
    let result = (|| {
        let entries = decode_canonical_bundle(bytes)?;
        let mut seen = BTreeSet::new();
        let mut has_ruleset = false;
        for (relative, file_bytes) in entries {
            if !seen.insert(relative.clone()) {
                return Err(ScanError::Delivery("delivered RuleBundle contains a duplicate path".into()));
            }
            if relative == "manifest.json" {
                return Err(ScanError::Delivery("delivered RuleBundle cannot carry a transport manifest".into()));
            }
            if relative == "ruleset.xml" {
                has_ruleset = true;
            }
            let path = temp.join(&relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    ScanError::Delivery(format!("cannot create materialized RuleBundle directory: {error}"))
                })?;
            }
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|error| ScanError::Delivery(format!("cannot materialize RuleBundle file: {error}")))?;
            file.write_all(&file_bytes)
                .and_then(|_| file.sync_all())
                .map_err(|error| ScanError::Delivery(format!("cannot write materialized RuleBundle file: {error}")))?;
        }
        if !has_ruleset {
            return Err(ScanError::Delivery("delivered RuleBundle has no ruleset.xml".into()));
        }
        let manifest = serde_json::json!({
            "intent": {"id": rule_id},
            "digest": digest,
        });
        fs::write(temp.join("manifest.json"), serde_json::to_vec(&manifest).map_err(|error| {
            ScanError::Delivery(format!("cannot encode materialized RuleBundle manifest: {error}"))
        })?)
        .map_err(|error| ScanError::Delivery(format!("cannot write materialized RuleBundle manifest: {error}")))?;
        fs::rename(&temp, &target)
            .map_err(|error| ScanError::Delivery(format!("cannot publish materialized RuleBundle: {error}")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temp);
    }
    result?;
    let validated = validate_rule_bundle(&target)?;
    if validated.rule_id != rule_id || bundle_payload_bytes(&target)? != bytes {
        return Err(ScanError::Delivery("materialized RuleBundle does not match the delivered payload".into()));
    }
    Ok(target.to_string_lossy().into_owned())
}

fn ensure_materialized_directory(path: &Path) -> Result<(), ScanError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(ScanError::Delivery("materialized RuleBundle parent is unsafe".into()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match fs::create_dir(path) {
                Ok(()) => Ok(()),
                Err(create_error) if create_error.kind() == std::io::ErrorKind::AlreadyExists => ensure_materialized_directory(path),
                Err(create_error) => Err(ScanError::Delivery(format!("cannot create materialized RuleBundle parent: {create_error}"))),
            }
        }
        Err(error) => Err(ScanError::Delivery(format!("cannot inspect materialized RuleBundle parent: {error}"))),
    }
}

struct MaterializedLock {
    path: PathBuf,
}

impl MaterializedLock {
    fn acquire(parent: &Path, digest: &str) -> Result<Self, ScanError> {
        let path = parent.join(format!(".{digest}.lock"));
        let deadline = Instant::now() + MATERIALIZED_LOCK_TIMEOUT;
        loop {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    let lock = Self { path };
                    if let Err(error) = writeln!(file, "pid={}", std::process::id()) {
                        drop(lock);
                        return Err(ScanError::Delivery(format!("cannot write materialized RuleBundle lock: {error}")));
                    }
                    if let Err(error) = file.sync_all() {
                        drop(lock);
                        return Err(ScanError::Delivery(format!("cannot sync materialized RuleBundle lock: {error}")));
                    }
                    return Ok(lock);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    return Err(ScanError::Delivery(format!("materialized RuleBundle lock is held beyond {:?}", MATERIALIZED_LOCK_TIMEOUT)));
                }
                Err(error) => return Err(ScanError::Delivery(format!("cannot acquire materialized RuleBundle lock: {error}"))),
            }
        }
    }
}

impl Drop for MaterializedLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn materialized_usage(parent: &Path) -> Result<(usize, u64), ScanError> {
    let mut entries = 0usize;
    let mut used_bytes = 0u64;
    for entry in fs::read_dir(parent)
        .map_err(|error| ScanError::Delivery(format!("cannot inspect materialized RuleBundle cache: {error}")))?
    {
        let entry = entry
            .map_err(|error| ScanError::Delivery(format!("cannot enumerate materialized RuleBundle cache: {error}")))?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| ScanError::Delivery(format!("cannot stat materialized RuleBundle cache: {error}")))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ScanError::Delivery("materialized RuleBundle cache contains an unsafe entry".into()));
        }
        entries = entries.saturating_add(1);
        used_bytes = used_bytes.saturating_add(materialized_tree_bytes(&entry.path())?);
    }
    Ok((entries, used_bytes))
}

fn materialized_tree_bytes(path: &Path) -> Result<u64, ScanError> {
    let mut total = 0u64;
    for entry in fs::read_dir(path)
        .map_err(|error| ScanError::Delivery(format!("cannot read materialized RuleBundle tree: {error}")))?
    {
        let entry = entry
            .map_err(|error| ScanError::Delivery(format!("cannot enumerate materialized RuleBundle tree: {error}")))?;
        let child = entry.path();
        let metadata = fs::symlink_metadata(&child)
            .map_err(|error| ScanError::Delivery(format!("cannot stat materialized RuleBundle tree: {error}")))?;
        if metadata.file_type().is_symlink() {
            return Err(ScanError::Delivery("materialized RuleBundle tree contains a symlink".into()));
        }
        total = if metadata.is_dir() {
            total.saturating_add(materialized_tree_bytes(&child)?)
        } else if metadata.is_file() {
            total.saturating_add(metadata.len())
        } else {
            return Err(ScanError::Delivery("materialized RuleBundle tree contains a non-file entry".into()));
        };
    }
    Ok(total)
}

fn decode_canonical_bundle(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, ScanError> {
    let mut offset = 0usize;
    let mut entries = Vec::new();
    while offset < bytes.len() {
        if entries.len() >= MAX_MATERIALIZED_FILES {
            return Err(ScanError::Delivery("delivered RuleBundle has too many files".into()));
        }
        let path_len = read_u64(bytes, &mut offset)
            .ok_or_else(|| ScanError::Delivery("delivered RuleBundle path header is truncated".into()))?;
        if path_len == 0
            || path_len > MAX_MATERIALIZED_PATH_BYTES as u64
            || path_len > usize::MAX as u64
        {
            return Err(ScanError::Delivery("delivered RuleBundle path length is invalid".into()));
        }
        let path_end = offset.checked_add(path_len as usize).ok_or_else(|| {
            ScanError::Delivery("delivered RuleBundle path length overflows".into())
        })?;
        if path_end > bytes.len() {
            return Err(ScanError::Delivery("delivered RuleBundle path is truncated".into()));
        }
        let relative = String::from_utf8(bytes[offset..path_end].to_vec())
            .map_err(|_| ScanError::Delivery("delivered RuleBundle path is not UTF-8".into()))?;
        offset = path_end;
        if !is_safe_materialized_path(&relative) {
            return Err(ScanError::Delivery("delivered RuleBundle contains an unsafe path".into()));
        }
        let file_len = read_u64(bytes, &mut offset)
            .ok_or_else(|| ScanError::Delivery("delivered RuleBundle file header is truncated".into()))?;
        if file_len > erf_bundle_cache::MAX_BUNDLE_BYTES as u64 || file_len > usize::MAX as u64 {
            return Err(ScanError::Delivery("delivered RuleBundle file is too large".into()));
        }
        let file_end = offset.checked_add(file_len as usize).ok_or_else(|| {
            ScanError::Delivery("delivered RuleBundle file length overflows".into())
        })?;
        if file_end > bytes.len() {
            return Err(ScanError::Delivery("delivered RuleBundle file is truncated".into()));
        }
        entries.push((relative, bytes[offset..file_end].to_vec()));
        offset = file_end;
    }
    if entries.is_empty() {
        return Err(ScanError::Delivery("delivered RuleBundle payload has no files".into()));
    }
    Ok(entries)
}

fn read_u64(bytes: &[u8], offset: &mut usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    let value = u64::from_be_bytes(bytes.get(*offset..end)?.try_into().ok()?);
    *offset = end;
    Some(value)
}

fn is_safe_materialized_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && !path.chars().any(char::is_control)
        && path.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

fn validate_rule_bundle(root: &Path) -> Result<ValidatedBundle, ScanError> {
    let manifest = root.join("manifest.json");
    let ruleset = root.join("ruleset.xml");
    let manifest_metadata = fs::symlink_metadata(&manifest)
        .map_err(|e| ScanError::RuleBundle(format!("cannot stat manifest: {e}")))?;
    let ruleset_metadata = fs::symlink_metadata(&ruleset)
        .map_err(|e| ScanError::RuleBundle(format!("cannot stat ruleset: {e}")))?;
    if !manifest_metadata.is_file()
        || !ruleset_metadata.is_file()
        || manifest_metadata.file_type().is_symlink()
        || ruleset_metadata.file_type().is_symlink()
    {
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

pub fn bundle_payload_digest(root: &Path) -> Result<String, ScanError> {
    let bytes = bundle_payload_bytes(root)?;
    Ok(hex_digest(&Sha256::digest(bytes)))
}

fn bundle_payload_bytes(root: &Path) -> Result<Vec<u8>, ScanError> {
    let mut files = Vec::<(String, PathBuf)>::new();
    collect_payload_files(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut payload = Vec::new();
    for (relative, path) in files {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|e| ScanError::RuleBundle(format!("cannot stat {}: {e}", path.display())))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ScanError::RuleBundle(format!(
                "RuleBundle contains a non-regular or symbolic-link file: {}",
                path.display()
            )));
        }
        if metadata.len() > erf_bundle_cache::MAX_BUNDLE_BYTES as u64
            || payload.len().saturating_add(metadata.len() as usize) > erf_bundle_cache::MAX_BUNDLE_BYTES
        {
            return Err(ScanError::RuleBundle("canonical RuleBundle payload exceeds the delivery limit".into()));
        }
        let bytes = fs::read(&path).map_err(|e| ScanError::RuleBundle(format!("cannot read {}: {e}", path.display())))?;
        payload.extend_from_slice(&(relative.len() as u64).to_be_bytes());
        payload.extend_from_slice(relative.as_bytes());
        payload.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        payload.extend_from_slice(&bytes);
        if payload.len() > erf_bundle_cache::MAX_BUNDLE_BYTES {
            return Err(ScanError::RuleBundle("canonical RuleBundle payload exceeds the delivery limit".into()));
        }
    }
    Ok(payload)
}

fn collect_payload_files(root: &Path, current: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), ScanError> {
    let entries = fs::read_dir(current)
        .map_err(|e| ScanError::RuleBundle(format!("cannot read {}: {e}", current.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| ScanError::RuleBundle(format!("cannot enumerate bundle: {e}")))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| ScanError::RuleBundle(format!("cannot inspect bundle entry: {e}")))?;
        if file_type.is_symlink() {
            return Err(ScanError::RuleBundle(format!(
                "RuleBundle contains a symbolic link: {}",
                path.display()
            )));
        }
        if file_type.is_dir() {
            collect_payload_files(root, &path, out)?;
        } else if file_type.is_file()
            && path.file_name().and_then(|n| n.to_str()) != Some("manifest.json")
        {
            let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            out.push((relative, path));
        }
    }
    Ok(())
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unique_suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    )
}

fn prefixed_digest(bytes: &[u8; 32]) -> String {
    format!("sha256:{}", hex_digest(bytes))
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

    #[test]
    fn delivery_verified_cache_is_transport_independent_for_https_and_offline() {
        use erf_bundle_cache::{BundleCache, DeploymentProfile, FileBundleCache, VerificationEvidence};
        let root = test_bundle();
        let bytes = b"verified-cache-transport-test";
        let descriptor = erf_bundle_cache::BundleDeliveryDescriptor {
            protocol_version: erf_bundle_cache::DELIVERY_PROTOCOL_VERSION.into(),
            descriptor_version: erf_bundle_cache::DELIVERY_DESCRIPTOR_VERSION.into(),
            bundle_digest: Sha256::digest(bytes).into(),
            bundle_schema_version: "rule-bundle-v0".into(),
            compatibility_identity: "java21-pmd7".into(), platform: "linux/amd64".into(),
            rule_release_digest: [1; 32], policy_profile_digest: [2; 32],
            execution_lock_digest: [3; 32], ontology_contract_pack_digest: [4; 32],
            runtime_pack_digests: vec![[5; 32]], artifact_statement_digest: [6; 32],
            revocation_snapshot_digest: [7; 32], issued_at_epoch_secs: 1, expires_at_epoch_secs: 100,
        };
        FileBundleCache::new(&root).put(&descriptor, bytes).unwrap();
        let mut config = BundleDeliveryConfig {
            endpoint: "https://unreachable.invalid/v1".into(),
            expected_compatibility_identity: descriptor.compatibility_identity.clone(),
            expected_platform: descriptor.platform.clone(), descriptor,
            cache_root: root.to_string_lossy().into_owned(), profile: DeploymentProfile::Connected,
            evidence: VerificationEvidence { signature_valid: true, trust_metadata_fresh: true,
                revocation_snapshot_fresh: true, referenced_packs_available: true, trusted_context_valid: true },
            verification: None, now_epoch_secs: 2, timeout_ms: 1,
        };
        // The private post-verification seam must never construct an HTTP client on a hit.
        for profile in [DeploymentProfile::Connected, DeploymentProfile::EnterpriseAirGapped, DeploymentProfile::StandaloneDeveloper] {
            config.profile = profile;
            assert_eq!(config.resolve_verified_bytes().unwrap().bytes, bytes);
        }
        // The public scan path still refuses to use the cache without signed context.
        assert!(config.resolve().unwrap_err().to_string().contains("verification context is required"));
        config.now_epoch_secs = 100;
        assert!(config.resolve_verified_bytes().is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verified_delivery_can_materialize_a_runner_candidate_without_a_local_path() {
        let source = test_bundle();
        fs::create_dir_all(source.join("fixtures/bad")).unwrap();
        fs::write(source.join("ruleset.xml"), b"ruleset-v1").unwrap();
        fs::write(source.join("fixtures/bad/Bad0.java"), b"class Bad0 {}").unwrap();
        let payload = bundle_payload_bytes(&source).unwrap();
        let bundle_digest: [u8; 32] = Sha256::digest(&payload).into();
        let descriptor = erf_bundle_cache::BundleDeliveryDescriptor {
            protocol_version: erf_bundle_cache::DELIVERY_PROTOCOL_VERSION.into(),
            descriptor_version: erf_bundle_cache::DELIVERY_DESCRIPTOR_VERSION.into(),
            bundle_digest,
            bundle_schema_version: "rule-bundle-v0".into(),
            compatibility_identity: "java21-pmd7".into(),
            platform: "linux/amd64".into(),
            rule_release_digest: [1; 32],
            policy_profile_digest: [2; 32],
            execution_lock_digest: [3; 32],
            ontology_contract_pack_digest: [4; 32],
            runtime_pack_digests: vec![[5; 32]],
            artifact_statement_digest: [6; 32],
            revocation_snapshot_digest: [7; 32],
            issued_at_epoch_secs: 1,
            expires_at_epoch_secs: 100,
        };
        let cache_root = test_bundle();
        let materialized = materialize_delivered_bundle(&cache_root.to_string_lossy(), "corp.test", &descriptor, &payload).unwrap();
        let materialized_path = PathBuf::from(&materialized);
        let validated = validate_rule_bundle(&materialized_path).unwrap();
        assert_eq!(validated.rule_id, "corp.test");
        assert_eq!(bundle_payload_bytes(&materialized_path).unwrap(), payload);
        assert_eq!(materialize_delivered_bundle(&cache_root.to_string_lossy(), "corp.test", &descriptor, &payload).unwrap(), materialized);
        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(cache_root).unwrap();
    }

    fn valid_lock() -> ExecutionLock {
        let mut lock = ExecutionLock {
            rule_bundle_digest: "a".repeat(64),
            runner_release_digest: "b".repeat(64),
            backend_pack_digest: "c".repeat(64),
            resolver_pack_digest: "d".repeat(64),
            sandbox_provider_id: "rootless-oci-docker-v0".into(),
            sandbox_policy_digest: "e".repeat(64),
            resolver_sandbox_provider_id: "rootless-oci-docker-v0".into(),
            resolver_sandbox_policy_digest: "1".repeat(64),
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
            "rule_bundle_digest={}\nrunner_release_digest={}\nbackend_pack_digest={}\nresolver_pack_digest={}\nsandbox_provider_id={}\nsandbox_policy_digest={}\nresolver_sandbox_provider_id={}\nresolver_sandbox_policy_digest={}\nconformance_suite_digest={}\nresolver_id={}\nresolver_version={}\nbackend_id={}\nbackend_version={}\nprotocol_major={}\ncanonical_set_version={}\nlock_digest={}\n",
            lock.rule_bundle_digest, lock.runner_release_digest, lock.backend_pack_digest,
            lock.resolver_pack_digest, lock.sandbox_provider_id, lock.sandbox_policy_digest,
            lock.resolver_sandbox_provider_id, lock.resolver_sandbox_policy_digest,
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
            "rule_bundle_digest={}\nrunner_release_digest={}\nbackend_pack_digest={}\nresolver_pack_digest={}\nsandbox_provider_id={}\nsandbox_policy_digest={}\nresolver_sandbox_provider_id={}\nresolver_sandbox_policy_digest={}\nconformance_suite_digest={}\nresolver_id={}\nresolver_version={}\nbackend_id={}\nbackend_version={}\nprotocol_major={}\ncanonical_set_version={}\nlock_digest={}\nkey_id=lock-key\nrole=execution-lock\npublic_key_hex={}\nsignature_hex={}\nmetadata_version=1\nexpires_at_epoch_secs=100\n",
            lock.rule_bundle_digest, lock.runner_release_digest, lock.backend_pack_digest,
            lock.resolver_pack_digest, lock.sandbox_provider_id, lock.sandbox_policy_digest,
            lock.resolver_sandbox_provider_id, lock.resolver_sandbox_policy_digest,
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
    fn project_scope_filters_modules_roots_includes_and_excludes() {
        let scope = ProjectScanScope {
            modules: vec!["app".into()],
            roots: vec!["app/src/main/java".into()],
            include: vec!["app/src/main/java/**".into()],
            exclude: vec!["app/src/main/java/generated/**".into()],
        };
        scope.validate().unwrap();
        assert!(scope.selects("app/src/main/java/App.java"));
        assert!(!scope.selects("app/src/test/java/AppTest.java"));
        assert!(!scope.selects("lib/src/main/java/Lib.java"));
        assert!(!scope.selects("app/src/main/java/generated/Generated.java"));
    }

    #[test]
    fn staging_enforces_byte_limit_before_append() {
        let mut staging = AttemptStaging::new(7).unwrap();
        let observation = host_pb::BackendObservation::default();
        assert!(staging.append(&observation).is_err());
    }


    #[test]
    fn observation_blob_digest_must_match_workspace_descriptor() {
        let descriptor = workspace_pb::WorkspaceDescriptor {
            modules: vec![workspace_pb::ModuleDescriptor {
                files: vec![workspace_pb::FileEntry {
                    path: "src/A.java".into(),
                    blob_sha256: vec![7; 32],
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut observation = host_pb::BackendObservation {
            location: Some(host_pb::ObservationLocation {
                kind: Some(host_pb::observation_location::Kind::SourceSpan(host_pb::SourceSpan {
                    path: "src/A.java".into(),
                    blob_sha256: vec![7; 32],
                    line: 1,
                    column: 1,
                    ..Default::default()
                })),
            }),
            ..Default::default()
        };
        assert!(validate_observation_blob_identity(&observation, &descriptor).is_ok());
        if let Some(location) = observation.location.as_mut() {
            if let Some(host_pb::observation_location::Kind::SourceSpan(span)) = location.kind.as_mut() {
                span.blob_sha256 = vec![8; 32];
            }
        }
        assert_eq!(
            validate_observation_blob_identity(&observation, &descriptor),
            Err("observation blob digest does not match workspace descriptor")
        );
    }

    #[test]
    fn incremental_safety_fallback_requires_a_full_plan_at_runner_boundary() {
        let fallback_inputs = erf_incremental_baseline::FallbackInputs {
            requested_route: erf_incremental_baseline::ExecutionRoute::Incremental,
            plan: erf_incremental_baseline::PlanObservation::new(
                "a".repeat(64),
                erf_scan_plan::PlanMode::Module,
                erf_scan_plan::PlanAuthority::GateEligible,
                1,
                true,
            )
            .unwrap(),
            cache: erf_incremental_baseline::CacheObservation::Corrupt,
            build: erf_incremental_baseline::BuildOutcome::Success,
            build_configuration_changed: false,
            rule_bundle_changed: false,
            policy_changed: false,
            resolver_changed: false,
            dependency_graph: erf_incremental_baseline::DependencyGraphState::Proven,
            generated_source: erf_incremental_baseline::GeneratedSourceState::Stable,
            merge_base: erf_incremental_baseline::MergeBaseState::Verifiable,
            affected_unit_count: 1,
            scope_budget: 1,
        };
        let config = ScanConfig {
            repo_root: ".".into(),
            rule_id: "corp.test".into(),
            rule_bundle: None,
            bundle_delivery: None,
            resolver_command: "unreachable-resolver".into(),
            resolver_args: Vec::new(),
            host_command: "unreachable-host".into(),
            host_args: Vec::new(),
            host_deadline_ms: 0,
            resolver_deadline_ms: 0,
            max_staging_bytes: 1024,
            execution_lock: None,
            signed_execution_lock: None,
            resolver_sandbox_policy: None,
            sandbox_policy: None,
            workspace_attestation: None,
            frozen_plan: None,
            incremental: None,
            scan_cache_dir: None,
            fallback_inputs: Some(fallback_inputs),
            project_scope: None,
        };
        let error = scan(&config).unwrap_err();
        assert!(error.to_string().contains("requires a FULL ScanPlan"));
    }

    #[test]
    fn delivery_failure_is_reported_before_resolver_or_backend_starts() {
        let descriptor = erf_bundle_cache::BundleDeliveryDescriptor {
            protocol_version: erf_bundle_cache::DELIVERY_PROTOCOL_VERSION.into(),
            descriptor_version: erf_bundle_cache::DELIVERY_DESCRIPTOR_VERSION.into(),
            bundle_digest: [1; 32],
            bundle_schema_version: "rule-bundle-v0".into(),
            compatibility_identity: "java21-pmd7".into(),
            platform: "linux/amd64".into(),
            rule_release_digest: [2; 32],
            policy_profile_digest: [3; 32],
            execution_lock_digest: [4; 32],
            ontology_contract_pack_digest: [5; 32],
            runtime_pack_digests: vec![[6; 32]],
            artifact_statement_digest: [7; 32],
            revocation_snapshot_digest: [8; 32],
            issued_at_epoch_secs: 10,
            expires_at_epoch_secs: 100,
        };
        let config = ScanConfig {
            repo_root: ".".into(),
            rule_id: "corp.test".into(),
            rule_bundle: Some("/does/not/exist".into()),
            bundle_delivery: Some(BundleDeliveryConfig {
                endpoint: "https://server.example.invalid/v1".into(),
                descriptor,
                expected_compatibility_identity: "java21-pmd7".into(),
                expected_platform: "linux/amd64".into(),
                cache_root: "/tmp/erf-delivery-test".into(),
                profile: erf_bundle_cache::DeploymentProfile::Connected,
                evidence: VerificationEvidenceForTest::complete(),
                verification: None,
                now_epoch_secs: 20,
                timeout_ms: 100,
            }),
            resolver_command: "/does/not/start".into(),
            resolver_args: Vec::new(),
            host_command: "/does/not/start".into(),
            host_args: Vec::new(),
            host_deadline_ms: 0,
            resolver_deadline_ms: 0,
            max_staging_bytes: 1024,
            execution_lock: None,
            signed_execution_lock: None,
            resolver_sandbox_policy: None,
            sandbox_policy: None,
            workspace_attestation: None,
            frozen_plan: None,
            incremental: None,
            scan_cache_dir: None,
            fallback_inputs: None,
            project_scope: None,
        };
        let error = scan(&config).unwrap_err();
        assert!(matches!(error, ScanError::Delivery(detail) if detail.contains("signed Execution identity")));
    }

    #[test]
    fn delivery_context_rejects_invalid_signed_bundle_before_pack_or_backend() {
        let inputs = erf_rule_bundle::RuleBundleInputs {
            rule_release_digest: [1; 32],
            policy_profile_digest: [2; 32],
            execution_lock_digest: [3; 32],
            ontology_contract_pack_digest: [4; 32],
        };
        let manifest = erf_rule_bundle::RuleBundleManifest {
            bundle_version: "rule-bundle-v0".into(),
            bundle_digest: [9; 32],
            rule_release_digest: inputs.rule_release_digest,
            policy_profile_digest: inputs.policy_profile_digest,
            execution_lock_digest: inputs.execution_lock_digest,
            ontology_contract_pack_digest: inputs.ontology_contract_pack_digest,
            compatibility_identity: "java21-pmd7".into(),
            platform: "linux/amd64".into(),
        };
        let key = SigningKey::from_bytes(&[101; 32]);
        let mut signed_bundle = manifest.sign_with("bundle-key", |bytes| {
            key.sign(bytes).to_bytes().to_vec()
        });
        signed_bundle.signed_manifest.signature[0] ^= 1;
        let mut trust_store = erf_trust::TrustStore::default();
        trust_store.insert_key("bundle-key", key.verifying_key()).unwrap();
        trust_store.authorize_role(erf_rule_bundle::RULE_BUNDLE_SIGNING_ROLE, "bundle-key").unwrap();
        let revocation_snapshot = erf_trust::RevocationSnapshot {
            version: 1,
            issued_at_epoch_secs: 1,
            expires_at_epoch_secs: 100,
            revoked_asset_digests: Default::default(),
        };
        let revocation_snapshot_digest = revocation_snapshot.subject_digest();
        let verification = BundleDeliveryVerification {
            signed_bundle,
            manifest_bytes: manifest.canonical_payload(),
            inputs,
            trust_store,
            trust_metadata: None,
            revocation_snapshot: Some(revocation_snapshot),
            revocation_snapshot_digest,
            packs: Vec::new(),
        };
        let descriptor = erf_bundle_cache::BundleDeliveryDescriptor {
            protocol_version: erf_bundle_cache::DELIVERY_PROTOCOL_VERSION.into(),
            descriptor_version: erf_bundle_cache::DELIVERY_DESCRIPTOR_VERSION.into(),
            bundle_digest: [9; 32],
            bundle_schema_version: "rule-bundle-v0".into(),
            compatibility_identity: "java21-pmd7".into(),
            platform: "linux/amd64".into(),
            rule_release_digest: [1; 32],
            policy_profile_digest: [2; 32],
            execution_lock_digest: [3; 32],
            ontology_contract_pack_digest: [4; 32],
            runtime_pack_digests: vec![[5; 32]],
            artifact_statement_digest: [6; 32],
            revocation_snapshot_digest,
            issued_at_epoch_secs: 1,
            expires_at_epoch_secs: 100,
        };
        let error = verify_delivery_context(
            &descriptor,
            &verification,
            "java21-pmd7",
            "linux/amd64",
            10,
        )
        .unwrap_err();
        assert!(error.to_string().contains("signed RuleBundle verification failed"));
    }

    #[test]
    fn delivery_classification_keeps_descriptor_mismatches_as_input_errors() {
        assert!(classify_delivery_detail("delivery compatibility or platform does not match the Runner context").starts_with("delivery-class=input;"));
        assert!(classify_delivery_detail("verified RuleBundle input does not match descriptor: rule_release_digest").starts_with("delivery-class=input;"));
        assert!(classify_delivery_detail("verified runtime Pack count does not match descriptor").starts_with("delivery-class=input;"));
        assert!(classify_delivery_detail("runtime Pack digest does not match descriptor at index 0").starts_with("delivery-class=input;"));
        assert!(classify_delivery_detail("runtime Pack manifest identity does not match the delivery context at index 0").starts_with("delivery-class=input;"));
        assert!(classify_delivery_detail("runtime Pack OCI layout verification failed: OCI index is invalid").starts_with("delivery-class=input;"));
    }

    #[test]
    fn delivery_classification_keeps_trust_and_fetch_classes_stable() {
        assert!(classify_delivery_detail("signed RuleBundle verification failed: SIGNATURE_INVALID").starts_with("delivery-class=trust;"));
        assert!(classify_delivery_detail("connected runner must fetch the missing bundle from the server").starts_with("delivery-class=fetch;"));
    }

    struct VerificationEvidenceForTest;

    impl VerificationEvidenceForTest {
        fn complete() -> erf_bundle_cache::VerificationEvidence {
            erf_bundle_cache::VerificationEvidence {
                signature_valid: true,
                trust_metadata_fresh: true,
                revocation_snapshot_fresh: true,
                referenced_packs_available: true,
                trusted_context_valid: true,
            }
        }
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
