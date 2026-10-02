//! Digest-addressed RuleBundle delivery and cache semantics.
//!
//! This crate deliberately sits between the signed RuleBundle verifier and a
//! transport or registry. It does not fetch remote content, mint trust, or
//! turn a cache hit into Gate authority. Callers must supply the verification
//! evidence that was produced by the existing trust and execution seams.

use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use thiserror::Error;

pub const DELIVERY_PROTOCOL_VERSION: &str = "rulefactory.io/v1";
pub const DELIVERY_DESCRIPTOR_VERSION: &str = "rule-bundle-delivery-v1";
pub const MAX_BUNDLE_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_CACHE_ENTRIES: usize = 256;
pub const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const CACHE_LOCK_FILE: &str = ".cache.lock";
const CACHE_LOCK_MARKER: &str = ".cache.lock.d";
const CACHE_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentProfile {
    Connected,
    EnterpriseAirGapped,
    StandaloneDeveloper,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleDeliveryDescriptor {
    pub protocol_version: String,
    pub descriptor_version: String,
    pub bundle_digest: [u8; 32],
    pub bundle_schema_version: String,
    pub compatibility_identity: String,
    pub platform: String,
    pub rule_release_digest: [u8; 32],
    pub policy_profile_digest: [u8; 32],
    pub execution_lock_digest: [u8; 32],
    pub ontology_contract_pack_digest: [u8; 32],
    pub runtime_pack_digests: Vec<[u8; 32]>,
    pub artifact_statement_digest: [u8; 32],
    pub revocation_snapshot_digest: [u8; 32],
    pub issued_at_epoch_secs: u64,
    pub expires_at_epoch_secs: u64,
}

impl BundleDeliveryDescriptor {
    pub fn from_rule_bundle(
        manifest: &erf_rule_bundle::RuleBundleManifest,
        bundle_bytes: &[u8],
        runtime_pack_digests: Vec<[u8; 32]>,
        artifact_statement_digest: [u8; 32],
        revocation_snapshot_digest: [u8; 32],
        issued_at_epoch_secs: u64,
        expires_at_epoch_secs: u64,
    ) -> Result<Self, DeliveryError> {
        let bundle_digest: [u8; 32] = Sha256::digest(bundle_bytes).into();
        if manifest.bundle_digest != bundle_digest {
            return Err(DeliveryError::InvalidDescriptor("bundle_digest"));
        }
        let descriptor = Self {
            protocol_version: DELIVERY_PROTOCOL_VERSION.into(),
            descriptor_version: DELIVERY_DESCRIPTOR_VERSION.into(),
            bundle_digest,
            bundle_schema_version: manifest.bundle_version.clone(),
            compatibility_identity: manifest.compatibility_identity.clone(),
            platform: manifest.platform.clone(),
            rule_release_digest: manifest.rule_release_digest,
            policy_profile_digest: manifest.policy_profile_digest,
            execution_lock_digest: manifest.execution_lock_digest,
            ontology_contract_pack_digest: manifest.ontology_contract_pack_digest,
            runtime_pack_digests,
            artifact_statement_digest,
            revocation_snapshot_digest,
            issued_at_epoch_secs,
            expires_at_epoch_secs,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    pub fn validate(&self) -> Result<(), DeliveryError> {
        if self.protocol_version != DELIVERY_PROTOCOL_VERSION {
            return Err(DeliveryError::InvalidDescriptor("protocol_version"));
        }
        if self.descriptor_version != DELIVERY_DESCRIPTOR_VERSION {
            return Err(DeliveryError::InvalidDescriptor("descriptor_version"));
        }
        for (name, digest) in [
            ("bundle_digest", self.bundle_digest),
            ("rule_release_digest", self.rule_release_digest),
            ("policy_profile_digest", self.policy_profile_digest),
            ("execution_lock_digest", self.execution_lock_digest),
            ("ontology_contract_pack_digest", self.ontology_contract_pack_digest),
            ("artifact_statement_digest", self.artifact_statement_digest),
            ("revocation_snapshot_digest", self.revocation_snapshot_digest),
        ] {
            if digest == [0; 32] {
                return Err(DeliveryError::InvalidDescriptor(name));
            }
        }
        if self.runtime_pack_digests.is_empty()
            || self.runtime_pack_digests.contains(&[0; 32])
        {
            return Err(DeliveryError::InvalidDescriptor("runtime_pack_digests"));
        }
        for (name, value) in [
            ("bundle_schema_version", self.bundle_schema_version.as_str()),
            ("compatibility_identity", self.compatibility_identity.as_str()),
            ("platform", self.platform.as_str()),
        ] {
            if value.is_empty() || value.chars().any(char::is_control) {
                return Err(DeliveryError::InvalidDescriptor(name));
            }
        }
        if self.bundle_schema_version != erf_rule_bundle::RULE_BUNDLE_SCHEMA_VERSION {
            return Err(DeliveryError::InvalidDescriptor("bundle_schema_version"));
        }
        if self.expires_at_epoch_secs <= self.issued_at_epoch_secs {
            return Err(DeliveryError::InvalidDescriptor("expiry_window"));
        }
        Ok(())
    }

    /// Stable bytes for signing/transport metadata. This is a closed format:
    /// adding a field requires a new descriptor version.
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = b"erf-rule-bundle-delivery-v1\0".to_vec();
        append_string(&mut payload, &self.protocol_version);
        append_string(&mut payload, &self.descriptor_version);
        for digest in [
            self.bundle_digest,
            self.rule_release_digest,
            self.policy_profile_digest,
            self.execution_lock_digest,
            self.ontology_contract_pack_digest,
        ] {
            payload.extend_from_slice(&digest);
        }
        append_string(&mut payload, &self.bundle_schema_version);
        append_string(&mut payload, &self.compatibility_identity);
        append_string(&mut payload, &self.platform);
        payload.extend_from_slice(&(self.runtime_pack_digests.len() as u64).to_be_bytes());
        for digest in &self.runtime_pack_digests {
            payload.extend_from_slice(digest);
        }
        payload.extend_from_slice(&self.artifact_statement_digest);
        payload.extend_from_slice(&self.revocation_snapshot_digest);
        payload.extend_from_slice(&self.issued_at_epoch_secs.to_be_bytes());
        payload.extend_from_slice(&self.expires_at_epoch_secs.to_be_bytes());
        payload
    }

    pub fn descriptor_digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_payload()).into()
    }
}

pub trait BundleCache {
    fn put(&self, descriptor: &BundleDeliveryDescriptor, bytes: &[u8]) -> Result<(), CacheError>;

    fn get(&self, bundle_digest: &[u8; 32]) -> Result<Option<Vec<u8>>, CacheError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBundle {
    pub bytes: Vec<u8>,
    pub bundle_digest: [u8; 32],
    pub profile: DeploymentProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationEvidence {
    pub signature_valid: bool,
    pub trust_metadata_fresh: bool,
    pub revocation_snapshot_fresh: bool,
    pub referenced_packs_available: bool,
    pub trusted_context_valid: bool,
}

impl VerificationEvidence {
    pub fn first_failure(self) -> Option<&'static str> {
        [
            (!self.signature_valid, "SIGNATURE_INVALID"),
            (!self.trust_metadata_fresh, "TRUST_METADATA_UNAVAILABLE"),
            (!self.revocation_snapshot_fresh, "REVOCATION_SNAPSHOT_UNAVAILABLE"),
            (!self.referenced_packs_available, "REFERENCED_PACK_UNAVAILABLE"),
            (!self.trusted_context_valid, "TRUSTED_CONTEXT_INVALID"),
        ]
        .into_iter()
        .find_map(|(failed, code)| failed.then_some(code))
    }
}

/// Resolve a previously verified delivery from cache. A connected runner may
/// turn a miss into a server fetch request; offline profiles must return an
/// explicit INCONCLUSIVE outcome instead of using an unknown old asset.
pub fn resolve_cached<C: BundleCache>(
    cache: &C,
    descriptor: &BundleDeliveryDescriptor,
    profile: DeploymentProfile,
    evidence: VerificationEvidence,
    now_epoch_secs: u64,
) -> Result<ResolvedBundle, DeliveryError> {
    descriptor.validate()?;
    if now_epoch_secs < descriptor.issued_at_epoch_secs {
        return Err(DeliveryError::Inconclusive("DELIVERY_NOT_YET_VALID"));
    }
    if now_epoch_secs >= descriptor.expires_at_epoch_secs {
        return Err(DeliveryError::Inconclusive("BUNDLE_EXPIRED"));
    }
    if let Some(code) = evidence.first_failure() {
        return Err(DeliveryError::Inconclusive(code));
    }
    let Some(bytes) = cache.get(&descriptor.bundle_digest)? else {
        return match profile {
            DeploymentProfile::Connected => Err(DeliveryError::ServerFetchRequired),
            DeploymentProfile::EnterpriseAirGapped | DeploymentProfile::StandaloneDeveloper => {
                Err(DeliveryError::Inconclusive("CACHE_MISS"))
            }
        };
    };
    let actual: [u8; 32] = Sha256::digest(&bytes).into();
    if actual != descriptor.bundle_digest {
        return Err(DeliveryError::Cache(CacheError::Corrupt));
    }
    Ok(ResolvedBundle { bytes, bundle_digest: descriptor.bundle_digest, profile })
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum DeliveryError {
    #[error("invalid delivery descriptor field: {0}")]
    InvalidDescriptor(&'static str),
    #[error("offline delivery is inconclusive: {0}")]
    Inconclusive(&'static str),
    #[error("connected runner must fetch the missing bundle from the server")]
    ServerFetchRequired,
    #[error(transparent)]
    Cache(#[from] CacheError),
}

impl DeliveryError {
    pub fn gate_status(&self) -> &'static str {
        match self {
            Self::ServerFetchRequired => "RETRY",
            Self::InvalidDescriptor(_) | Self::Inconclusive(_) | Self::Cache(_) => "INCONCLUSIVE",
        }
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CacheError {
    #[error("bundle digest is zero")]
    ZeroDigest,
    #[error("bundle exceeds the configured cache limit")]
    TooLarge,
    #[error("bundle bytes do not match the content digest")]
    DigestMismatch,
    #[error("content-addressed cache entry is corrupt")]
    Corrupt,
    #[error("content-addressed cache path is not a regular file")]
    UnsafePath,
    #[error("cache entry already contains different bytes")]
    ImmutableConflict,
    #[error("content-addressed cache has reached its bounded disk capacity")]
    Capacity,
    #[error("cache I/O failed: {0}")]
    Io(String),
}

#[derive(Debug, Clone)]
pub struct FileBundleCache {
    root: PathBuf,
}

impl FileBundleCache {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self { root: root.as_ref().to_path_buf() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn object_path(&self, digest: &[u8; 32]) -> Result<PathBuf, CacheError> {
        if *digest == [0; 32] {
            return Err(CacheError::ZeroDigest);
        }
        Ok(self.root.join("sha256").join(hex_digest(digest)))
    }

    fn ensure_directories(&self) -> Result<PathBuf, CacheError> {
        ensure_directory(&self.root)?;
        let objects = self.root.join("sha256");
        ensure_directory(&objects)?;
        Ok(objects)
    }

    fn read_object(&self, path: &Path, digest: &[u8; 32]) -> Result<Vec<u8>, CacheError> {
        let metadata = fs::symlink_metadata(path).map_err(|error| CacheError::Io(error.to_string()))?;
        if !metadata.file_type().is_file() {
            return Err(CacheError::UnsafePath);
        }
        if metadata.len() > MAX_BUNDLE_BYTES as u64 {
            return Err(CacheError::TooLarge);
        }
        let file = open_read_file(path).map_err(|error| CacheError::Io(error.to_string()))?;
        let mut bytes = Vec::with_capacity(metadata.len().min(MAX_BUNDLE_BYTES as u64) as usize);
        file.take(MAX_BUNDLE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| CacheError::Io(error.to_string()))?;
        if bytes.len() > MAX_BUNDLE_BYTES {
            return Err(CacheError::TooLarge);
        }
        if Sha256::digest(&bytes).as_slice() != digest.as_slice() {
            return Err(CacheError::Corrupt);
        }
        Ok(bytes)
    }
}

impl BundleCache for FileBundleCache {
    fn put(&self, descriptor: &BundleDeliveryDescriptor, bytes: &[u8]) -> Result<(), CacheError> {
        descriptor.validate().map_err(|_| CacheError::DigestMismatch)?;
        if bytes.len() > MAX_BUNDLE_BYTES {
            return Err(CacheError::TooLarge);
        }
        let actual: [u8; 32] = Sha256::digest(bytes).into();
        if actual != descriptor.bundle_digest {
            return Err(CacheError::DigestMismatch);
        }
        let path = self.object_path(&descriptor.bundle_digest)?;
        let parent = self.ensure_directories()?;
        // The capacity check and immutable hard-link publication are one
        // transaction.  An OS advisory lock is required here because the
        // cache is shared by multiple Runner processes, not just threads.
        let _lock = CacheLock::acquire(&parent)?;
        cleanup_stale_temps(&parent)?;
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if !metadata.file_type().is_file() {
                return Err(CacheError::UnsafePath);
            }
            let existing = self.read_object(&path, &descriptor.bundle_digest)?;
            if existing != bytes {
                return Err(CacheError::ImmutableConflict);
            }
            return Ok(());
        }

        let (entries, used_bytes) = cache_usage(&parent)?;
        if entries >= MAX_CACHE_ENTRIES
            || used_bytes > MAX_CACHE_BYTES.saturating_sub(bytes.len() as u64)
        {
            return Err(CacheError::Capacity);
        }

        let temp = parent.join(format!(
            ".{}.{}.tmp",
            hex_digest(&descriptor.bundle_digest),
            unique_suffix()
        ));
        let write_result = (|| {
            let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)
                .map_err(|error| CacheError::Io(error.to_string()))?;
            file.write_all(bytes).map_err(|error| CacheError::Io(error.to_string()))?;
            file.sync_all().map_err(|error| CacheError::Io(error.to_string()))?;
            match fs::hard_link(&temp, &path) {
                Ok(()) => {
                    let _ = fs::remove_file(&temp);
                    Ok(())
                }
                Err(_error) if path.exists() => {
                    let existing = self.read_object(&path, &descriptor.bundle_digest)?;
                    let _ = fs::remove_file(&temp);
                    if existing == bytes { Ok(()) } else { Err(CacheError::ImmutableConflict) }
                }
                Err(error) => Err(CacheError::Io(error.to_string())),
            }
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        write_result
    }

    fn get(&self, bundle_digest: &[u8; 32]) -> Result<Option<Vec<u8>>, CacheError> {
        let path = self.object_path(bundle_digest)?;
        let _ = self.ensure_directories()?;
        match fs::symlink_metadata(&path) {
            Ok(_) => self.read_object(&path, bundle_digest).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(CacheError::Io(error.to_string())),
        }
    }
}

fn ensure_directory(path: &Path) -> Result<(), CacheError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(CacheError::UnsafePath)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Err(create_error) = fs::create_dir(path) {
                if create_error.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(CacheError::Io(create_error.to_string()));
                }
            }
            let metadata = fs::symlink_metadata(path).map_err(|metadata_error| CacheError::Io(metadata_error.to_string()))?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                Err(CacheError::UnsafePath)
            } else {
                Ok(())
            }
        }
        Err(error) => Err(CacheError::Io(error.to_string())),
    }
}

fn cache_usage(parent: &Path) -> Result<(usize, u64), CacheError> {
    let mut entries = 0usize;
    let mut used_bytes = 0u64;
    for entry in fs::read_dir(parent).map_err(|error| CacheError::Io(error.to_string()))? {
        let entry = entry.map_err(|error| CacheError::Io(error.to_string()))?;
        let name = entry.file_name();
        if name == CACHE_LOCK_FILE || name == CACHE_LOCK_MARKER {
            continue;
        }
        if name.to_string_lossy().starts_with('.') {
            return Err(CacheError::UnsafePath);
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| CacheError::Io(error.to_string()))?;
        if !metadata.file_type().is_file() {
            return Err(CacheError::UnsafePath);
        }
        entries = entries.saturating_add(1);
        used_bytes = used_bytes.saturating_add(metadata.len());
    }
    Ok((entries, used_bytes))
}

fn cleanup_stale_temps(parent: &Path) -> Result<(), CacheError> {
    for entry in fs::read_dir(parent).map_err(|error| CacheError::Io(error.to_string()))? {
        let entry = entry.map_err(|error| CacheError::Io(error.to_string()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == CACHE_LOCK_FILE || name == CACHE_LOCK_MARKER || !is_cache_temp_name(&name) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| CacheError::Io(error.to_string()))?;
        if metadata.file_type().is_symlink() || metadata.is_file() {
            fs::remove_file(entry.path()).map_err(|error| CacheError::Io(error.to_string()))?;
        } else {
            return Err(CacheError::UnsafePath);
        }
    }
    Ok(())
}

fn is_cache_temp_name(name: &str) -> bool {
    name.starts_with('.') && name.ends_with(".tmp")
}

struct CacheLock {
    file: File,
    marker: PathBuf,
}

impl CacheLock {
    fn acquire(parent: &Path) -> Result<Self, CacheError> {
        let marker = parent.join(CACHE_LOCK_MARKER);
        let deadline = Instant::now() + CACHE_LOCK_TIMEOUT;
        loop {
            match fs::create_dir(&marker) {
                Ok(()) => break,
                Err(error) if error.kind() == ErrorKind::AlreadyExists && Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    return Err(CacheError::Io(format!("cache lock is held beyond {:?}", CACHE_LOCK_TIMEOUT)));
                }
                Err(error) => return Err(CacheError::Io(error.to_string())),
            }
        }
        let path = parent.join(CACHE_LOCK_FILE);
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                let _ = fs::remove_dir(&marker);
                return Err(CacheError::UnsafePath);
            }
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW);
        let file = match options.open(&path) {
            Ok(file) => file,
            Err(error) => {
                let _ = fs::remove_dir(&marker);
                return Err(CacheError::Io(error.to_string()));
            }
        };
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                let _ = fs::remove_dir(&marker);
                return Err(CacheError::Io(error.to_string()));
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            let _ = fs::remove_dir(&marker);
            return Err(CacheError::UnsafePath);
        }
        #[cfg(unix)]
        {
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if result != 0 {
                let _ = fs::remove_dir(&marker);
                return Err(CacheError::Io(std::io::Error::last_os_error().to_string()));
            }
        }
        Ok(Self { file, marker })
    }
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
        }
        let _ = fs::remove_dir(&self.marker);
    }
}

fn open_read_file(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    options.open(path)
}

fn append_string(payload: &mut Vec<u8>, value: &str) {
    payload.extend_from_slice(&(value.len() as u64).to_be_bytes());
    payload.extend_from_slice(value.as_bytes());
}

fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{}-{nanos}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn digest(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn descriptor(bytes: &[u8]) -> BundleDeliveryDescriptor {
        let bundle_digest: [u8; 32] = Sha256::digest(bytes).into();
        BundleDeliveryDescriptor {
            protocol_version: DELIVERY_PROTOCOL_VERSION.into(),
            descriptor_version: DELIVERY_DESCRIPTOR_VERSION.into(),
            bundle_digest,
            bundle_schema_version: "rule-bundle-v0".into(),
            compatibility_identity: "java21-pmd7".into(),
            platform: "linux/amd64".into(),
            rule_release_digest: digest(1),
            policy_profile_digest: digest(2),
            execution_lock_digest: digest(3),
            ontology_contract_pack_digest: digest(4),
            runtime_pack_digests: vec![digest(5)],
            artifact_statement_digest: digest(6),
            revocation_snapshot_digest: digest(7),
            issued_at_epoch_secs: 10,
            expires_at_epoch_secs: 100,
        }
    }

    fn evidence() -> VerificationEvidence {
        VerificationEvidence {
            signature_valid: true,
            trust_metadata_fresh: true,
            revocation_snapshot_fresh: true,
            referenced_packs_available: true,
            trusted_context_valid: true,
        }
    }

    #[test]
    fn descriptor_is_closed_and_content_addressed() {
        let bytes = b"signed-rule-bundle";
        let descriptor = descriptor(bytes);
        assert!(descriptor.validate().is_ok());
        assert_ne!(descriptor.descriptor_digest(), [0; 32]);
        let mut changed = descriptor.clone();
        changed.platform = "linux/arm64".into();
        assert_ne!(descriptor.descriptor_digest(), changed.descriptor_digest());
        changed.bundle_schema_version = "rule-bundle-v9".into();
        assert_eq!(changed.validate(), Err(DeliveryError::InvalidDescriptor("bundle_schema_version")));
        changed.bundle_schema_version = "rule-bundle-v0".into();
        changed.expires_at_epoch_secs = 10;
        assert_eq!(changed.validate(), Err(DeliveryError::InvalidDescriptor("expiry_window")));
    }

    #[test]
    fn descriptor_constructor_binds_rule_bundle_manifest_inputs() {
        let manifest = erf_rule_bundle::RuleBundleManifest {
            bundle_version: "rule-bundle-v0".into(),
            bundle_digest: Sha256::digest(b"signed-rule-bundle").into(),
            rule_release_digest: digest(1),
            policy_profile_digest: digest(2),
            execution_lock_digest: digest(3),
            ontology_contract_pack_digest: digest(4),
            compatibility_identity: "java21-pmd7".into(),
            platform: "linux/amd64".into(),
        };
        let descriptor = BundleDeliveryDescriptor::from_rule_bundle(
            &manifest,
            b"signed-rule-bundle",
            vec![digest(5)],
            digest(6),
            digest(7),
            10,
            100,
        )
        .unwrap();
        assert_eq!(descriptor.rule_release_digest, manifest.rule_release_digest);
        let expected: [u8; 32] = Sha256::digest(b"signed-rule-bundle").into();
        assert_eq!(descriptor.bundle_digest, expected);
    }

    #[test]
    fn file_cache_survives_reopen_and_never_overwrites_a_digest() {
        let root = std::env::temp_dir().join(format!("erf-cache-{}", unique_suffix()));
        let bytes = b"signed-rule-bundle";
        let descriptor = descriptor(bytes);
        FileBundleCache::new(&root).put(&descriptor, bytes).unwrap();
        assert_eq!(FileBundleCache::new(&root).get(&descriptor.bundle_digest).unwrap(), Some(bytes.to_vec()));
        assert_eq!(FileBundleCache::new(&root).put(&descriptor, b"different"), Err(CacheError::DigestMismatch));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_cache_applies_a_bounded_entry_budget() {
        let root = std::env::temp_dir().join(format!("erf-cache-budget-{}", unique_suffix()));
        let cache = FileBundleCache::new(&root);
        for index in 0..MAX_CACHE_ENTRIES {
            let bytes = format!("bundle-{index}").into_bytes();
            let descriptor = descriptor(&bytes);
            cache.put(&descriptor, &bytes).unwrap();
        }
        let bytes = b"bundle-over-budget";
        assert_eq!(cache.put(&descriptor(bytes), bytes), Err(CacheError::Capacity));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupted_cache_entry_is_rejected_after_restart() {
        let root = std::env::temp_dir().join(format!("erf-cache-corrupt-{}", unique_suffix()));
        let bytes = b"signed-rule-bundle";
        let descriptor = descriptor(bytes);
        let cache = FileBundleCache::new(&root);
        cache.put(&descriptor, bytes).unwrap();
        fs::write(root.join("sha256").join(hex_digest(&descriptor.bundle_digest)), b"tampered").unwrap();
        assert_eq!(cache.get(&descriptor.bundle_digest), Err(CacheError::Corrupt));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_temporary_entries_are_removed_before_capacity_is_measured() {
        let root = std::env::temp_dir().join(format!("erf-cache-stale-temp-{}", unique_suffix()));
        let cache = FileBundleCache::new(&root);
        let parent = cache.ensure_directories().unwrap();
        fs::write(parent.join(".dead-writer.tmp"), vec![0u8; 1024]).unwrap();
        let bytes = b"signed-rule-bundle";
        cache.put(&descriptor(bytes), bytes).unwrap();
        assert!(!parent.join(".dead-writer.tmp").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cache_survives_an_actual_child_process_restart() {
        let root = std::env::temp_dir().join(format!("erf-cache-child-{}", unique_suffix()));
        let bytes = b"signed-rule-bundle";
        let descriptor = descriptor(bytes);
        FileBundleCache::new(&root).put(&descriptor, bytes).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::cache_child_process_probe", "--nocapture"])
            .env("ERF_CACHE_CHILD_ROOT", &root)
            .status()
            .unwrap();
        assert!(status.success());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cache_child_process_probe() {
        let Some(root) = std::env::var_os("ERF_CACHE_CHILD_ROOT") else {
            return;
        };
        let bytes = b"signed-rule-bundle";
        let descriptor = descriptor(bytes);
        assert_eq!(
            FileBundleCache::new(root).get(&descriptor.bundle_digest).unwrap(),
            Some(bytes.to_vec())
        );
    }

    #[test]
    fn offline_profiles_fail_closed_while_connected_profile_requests_fetch() {
        let root = std::env::temp_dir().join(format!("erf-cache-miss-{}", unique_suffix()));
        let descriptor = descriptor(b"signed-rule-bundle");
        let cache = FileBundleCache::new(&root);
        assert_eq!(
            resolve_cached(&cache, &descriptor, DeploymentProfile::Connected, evidence(), 20),
            Err(DeliveryError::ServerFetchRequired)
        );
        assert_eq!(
            resolve_cached(&cache, &descriptor, DeploymentProfile::EnterpriseAirGapped, evidence(), 20),
            Err(DeliveryError::Inconclusive("CACHE_MISS"))
        );
        fs::remove_dir_all(root).unwrap_or(());
    }

    #[test]
    fn expiry_and_verification_gaps_are_inconclusive() {
        let root = std::env::temp_dir().join(format!("erf-cache-evidence-{}", unique_suffix()));
        let bytes = b"signed-rule-bundle";
        let descriptor = descriptor(bytes);
        let cache = FileBundleCache::new(&root);
        cache.put(&descriptor, bytes).unwrap();
        assert_eq!(
            resolve_cached(&cache, &descriptor, DeploymentProfile::StandaloneDeveloper, evidence(), 100),
            Err(DeliveryError::Inconclusive("BUNDLE_EXPIRED"))
        );
        let mut missing = evidence();
        missing.revocation_snapshot_fresh = false;
        assert_eq!(
            resolve_cached(&cache, &descriptor, DeploymentProfile::StandaloneDeveloper, missing, 20),
            Err(DeliveryError::Inconclusive("REVOCATION_SNAPSHOT_UNAVAILABLE"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolver_rechecks_bytes_from_a_custom_cache_adapter() {
        struct LyingCache;
        impl BundleCache for LyingCache {
            fn put(&self, _descriptor: &BundleDeliveryDescriptor, _bytes: &[u8]) -> Result<(), CacheError> {
                Ok(())
            }

            fn get(&self, _bundle_digest: &[u8; 32]) -> Result<Option<Vec<u8>>, CacheError> {
                Ok(Some(b"not-the-addressed-bytes".to_vec()))
            }
        }

        let descriptor = descriptor(b"signed-rule-bundle");
        assert_eq!(
            resolve_cached(&LyingCache, &descriptor, DeploymentProfile::Connected, evidence(), 20),
            Err(DeliveryError::Cache(CacheError::Corrupt))
        );
    }

    #[test]
    fn digest_path_has_no_user_controlled_components() {
        let root = std::env::temp_dir().join(format!("erf-cache-path-{}", unique_suffix()));
        let cache = FileBundleCache::new(&root);
        assert_eq!(cache.get(&[0; 32]), Err(CacheError::ZeroDigest));
        fs::remove_dir_all(root).unwrap_or(());
    }

    #[cfg(unix)]
    #[test]
    fn cache_rejects_a_symlinked_objects_directory() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("erf-cache-symlink-{}", unique_suffix()));
        let target = std::env::temp_dir().join(format!("erf-cache-symlink-target-{}", unique_suffix()));
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&root).unwrap();
        symlink(&target, root.join("sha256")).unwrap();
        let error = FileBundleCache::new(&root).put(&descriptor(b"symlinked"), b"symlinked").unwrap_err();
        assert_eq!(error, CacheError::UnsafePath);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }
}
