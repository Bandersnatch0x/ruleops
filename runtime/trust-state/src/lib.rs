//! Bounded persistence boundary for offline TrustState.
//!
//! `erf-trust` remains a pure in-memory verifier. This crate owns only the
//! local state file: it uses a deterministic length-delimited format, bounded
//! parsing, fsync and atomic same-directory replacement. Missing state is
//! explicit so callers cannot silently treat a lost rollback floor as trust.

use erf_trust::{SignedAsset, TrustCheckpoint, TrustError, TrustState, TrustStateSnapshot, TrustStore};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;

const MAGIC: &[u8] = b"erf-trust-state-v0\0";
pub const MAX_STATE_BYTES: usize = 64 * 1024;
const MAX_ROLE_COUNT: u64 = 1024;
const MAX_TEXT_BYTES: u64 = 4096;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TrustStateStoreError {
    #[error("trust state is missing")]
    Missing,
    #[error("trust state is already initialized")]
    AlreadyInitialized,
    #[error("trust state exceeds the {limit}-byte limit")]
    TooLarge { limit: usize },
    #[error("trust state encoding is invalid: {0}")]
    InvalidEncoding(&'static str),
    #[error("trust state I/O failed: {0}")]
    Io(String),
    #[error(transparent)]
    InvalidState(#[from] TrustError),
}

/// A trust floor that has crossed the persistence adapter. The inner state is
/// intentionally private so trusted callers cannot manufacture a supposedly
/// persisted floor from `TrustState::default()` or an in-memory mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedTrustState {
    state: TrustState,
}

impl PersistedTrustState {
    pub fn highest_checkpoint_version(&self) -> u64 {
        self.state.highest_checkpoint_version()
    }

    pub fn repository(&self) -> Option<&str> {
        self.state.repository()
    }

    pub fn snapshot(&self) -> TrustStateSnapshot {
        self.state.snapshot()
    }

    /// Exposes the verified state for a read-only trusted verifier. There is
    /// no public constructor; instances come only from `TrustStateFile`.
    pub fn as_trust_state(&self) -> &TrustState {
        &self.state
    }
}

pub fn encode(state: &TrustState) -> Result<Vec<u8>, TrustStateStoreError> {
    let snapshot = state.snapshot();
    if snapshot.highest_role_versions.len() as u64 > MAX_ROLE_COUNT {
        return Err(TrustStateStoreError::InvalidEncoding("too many roles"));
    }
    let mut bytes = MAGIC.to_vec();
    bytes.extend_from_slice(&snapshot.highest_checkpoint_version.to_be_bytes());
    match snapshot.repository {
        None => bytes.push(0),
        Some(repository) => {
            bytes.push(1);
            append_string(&mut bytes, &repository)?;
        }
    }
    bytes.extend_from_slice(&(snapshot.highest_role_versions.len() as u64).to_be_bytes());
    for (role, version) in snapshot.highest_role_versions {
        append_string(&mut bytes, &role)?;
        bytes.extend_from_slice(&version.to_be_bytes());
    }
    if bytes.len() > MAX_STATE_BYTES {
        return Err(TrustStateStoreError::TooLarge { limit: MAX_STATE_BYTES });
    }
    Ok(bytes)
}

pub fn decode(bytes: &[u8]) -> Result<TrustState, TrustStateStoreError> {
    if bytes.len() > MAX_STATE_BYTES {
        return Err(TrustStateStoreError::TooLarge { limit: MAX_STATE_BYTES });
    }
    let mut cursor = Cursor { bytes, offset: 0 };
    if cursor.take(MAGIC.len())? != MAGIC {
        return Err(TrustStateStoreError::InvalidEncoding("unknown state version"));
    }
    let highest_checkpoint_version = cursor.u64()?;
    let repository = match cursor.byte()? {
        0 => None,
        1 => Some(cursor.string()? ),
        _ => return Err(TrustStateStoreError::InvalidEncoding("invalid repository marker")),
    };
    let role_count = cursor.u64()?;
    if role_count > MAX_ROLE_COUNT {
        return Err(TrustStateStoreError::InvalidEncoding("too many roles"));
    }
    let mut highest_role_versions = BTreeMap::new();
    for _ in 0..role_count {
        let role = cursor.string()?;
        let version = cursor.u64()?;
        if highest_role_versions.insert(role, version).is_some() {
            return Err(TrustStateStoreError::InvalidEncoding("duplicate role"));
        }
    }
    if cursor.offset != bytes.len() {
        return Err(TrustStateStoreError::InvalidEncoding("trailing bytes"));
    }
    TrustState::from_snapshot(TrustStateSnapshot { highest_checkpoint_version, repository, highest_role_versions })
        .map_err(TrustStateStoreError::InvalidState)
}

type DirectorySync = Arc<dyn Fn(&Path) -> std::io::Result<()> + Send + Sync>;

pub struct TrustStateFile {
    path: PathBuf,
    directory_sync: DirectorySync,
    write_lock: Mutex<()>,
}

/// Cross-instance/process writer lock. The lock file is intentionally kept in
/// place: removing it during Drop could let a waiting process open a new inode
/// and bypass an existing flock. A stale process therefore does not strand
/// trust state; the kernel releases its advisory lock on process exit.
struct ProcessLock(File);

impl ProcessLock {
    fn acquire(path: &Path) -> std::io::Result<Self> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "state path has no file name"))?
            .to_string_lossy();
        let lock_path = parent.join(format!(".{file_name}.lock"));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(lock_path)?;
        #[cfg(unix)]
        {
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if result != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(Self(file))
    }
}

impl Drop for ProcessLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            let _ = libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

impl TrustStateFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::with_directory_sync(path, Arc::new(sync_directory))
    }

    fn with_directory_sync(
        path: impl Into<PathBuf>,
        directory_sync: DirectorySync,
    ) -> Self {
        Self { path: path.into(), directory_sync, write_lock: Mutex::new(()) }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Option<TrustState>, TrustStateStoreError> {
        let _process_lock = ProcessLock::acquire(&self.path).map_err(|error| TrustStateStoreError::Io(error.to_string()))?;
        self.load_unlocked()
    }

    fn load_unlocked(&self) -> Result<Option<TrustState>, TrustStateStoreError> {
        match fs::read(&self.path) {
            Ok(bytes) => decode(&bytes).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(TrustStateStoreError::Io(error.to_string())),
        }
    }

    pub fn load_required(&self) -> Result<TrustState, TrustStateStoreError> {
        self.load()?.ok_or(TrustStateStoreError::Missing)
    }

    /// Installs the first signed checkpoint. Initialization is explicit so a
    /// caller cannot accidentally treat a missing state file as a recovered
    /// trust floor during normal trusted execution.
    pub fn initialize_signed_checkpoint(
        &self,
        checkpoint: &TrustCheckpoint,
        asset: &SignedAsset,
        store: &TrustStore,
        role: &str,
        now_epoch_secs: u64,
    ) -> Result<PersistedTrustState, TrustStateStoreError> {
        let _guard = self.write_lock.lock().map_err(|_| TrustStateStoreError::Io("trust state write lock is poisoned".into()))?;
        let _process_lock = ProcessLock::acquire(&self.path).map_err(|error| TrustStateStoreError::Io(error.to_string()))?;
        if self.load_unlocked()?.is_some() {
            return Err(TrustStateStoreError::AlreadyInitialized);
        }
        let mut state = TrustState::default();
        state.accept_signed_checkpoint(checkpoint, asset, store, role, now_epoch_secs)?;
        self.save_unlocked(&state)?;
        Ok(PersistedTrustState { state })
    }

    /// Accepts a newer or replayed checkpoint only when the persisted trust
    /// floor already exists. Verification and monotonicity happen before the
    /// atomic replacement; a write failure leaves the previous file intact.
    pub fn advance_signed_checkpoint(
        &self,
        checkpoint: &TrustCheckpoint,
        asset: &SignedAsset,
        store: &TrustStore,
        role: &str,
        now_epoch_secs: u64,
    ) -> Result<PersistedTrustState, TrustStateStoreError> {
        let _guard = self.write_lock.lock().map_err(|_| TrustStateStoreError::Io("trust state write lock is poisoned".into()))?;
        let _process_lock = ProcessLock::acquire(&self.path).map_err(|error| TrustStateStoreError::Io(error.to_string()))?;
        let mut state = self.load_unlocked()?.ok_or(TrustStateStoreError::Missing)?;
        state.accept_signed_checkpoint(checkpoint, asset, store, role, now_epoch_secs)?;
        self.save_unlocked(&state)?;
        Ok(PersistedTrustState { state })
    }

    /// Trusted execution must load an existing rollback floor; it must not
    /// silently bootstrap one while handling a scan.
    pub fn load_for_trusted_execution(&self) -> Result<PersistedTrustState, TrustStateStoreError> {
        Ok(PersistedTrustState { state: self.load_required()? })
    }

    #[cfg(test)]
    fn save(&self, state: &TrustState) -> Result<(), TrustStateStoreError> {
        let _guard = self.write_lock.lock().map_err(|_| TrustStateStoreError::Io("trust state write lock is poisoned".into()))?;
        let _process_lock = ProcessLock::acquire(&self.path).map_err(|error| TrustStateStoreError::Io(error.to_string()))?;
        self.save_unlocked(state)
    }

    fn save_unlocked(&self, state: &TrustState) -> Result<(), TrustStateStoreError> {
        let bytes = encode(state)?;
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = self.path.file_name().ok_or(TrustStateStoreError::InvalidEncoding("state path has no file name"))?.to_string_lossy();
        let temporary = temporary_path(parent, &file_name, "tmp");
        let previous = match fs::read(&self.path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(TrustStateStoreError::Io(error.to_string())),
        };
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let mut file = options.open(&temporary).map_err(|error| TrustStateStoreError::Io(error.to_string()))?;
            file.write_all(&bytes).map_err(|error| TrustStateStoreError::Io(error.to_string()))?;
            file.sync_all().map_err(|error| TrustStateStoreError::Io(error.to_string()))?;
            (self.directory_sync)(parent)
                .map_err(|error| TrustStateStoreError::Io(format!("directory sync before rename failed: {error}")))?;
            fs::rename(&temporary, &self.path).map_err(|error| TrustStateStoreError::Io(error.to_string()))?;
            if let Err(error) = (self.directory_sync)(parent) {
                let rollback = self.restore_previous(parent, &file_name, previous.as_deref());
                let rollback_detail = rollback
                    .err()
                    .map(|rollback_error| format!("; rollback failed: {rollback_error}"))
                    .unwrap_or_default();
                return Err(TrustStateStoreError::Io(format!(
                    "directory sync after rename failed: {error}{rollback_detail}"
                )));
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn restore_previous(&self, parent: &Path, file_name: &str, previous: Option<&[u8]>) -> std::io::Result<()> {
        match previous {
            Some(previous) => {
                let rollback = temporary_path(parent, file_name, "rollback");
                let mut file = OpenOptions::new().write(true).create_new(true).open(&rollback)?;
                file.write_all(previous)?;
                file.sync_all()?;
                fs::rename(rollback, &self.path)?;
                (self.directory_sync)(parent)
            }
            None => {
                match fs::remove_file(&self.path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
                (self.directory_sync)(parent)
            }
        }
    }
}

fn temporary_path(parent: &Path, file_name: &str, kind: &str) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(".{file_name}.{kind}-{}-{sequence}", std::process::id()))
}

fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

fn append_string(bytes: &mut Vec<u8>, value: &str) -> Result<(), TrustStateStoreError> {
    if value.is_empty() || value.len() as u64 > MAX_TEXT_BYTES {
        return Err(TrustStateStoreError::InvalidEncoding("invalid text length"));
    }
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], TrustStateStoreError> {
        let end = self.offset.checked_add(length).ok_or(TrustStateStoreError::InvalidEncoding("offset overflow"))?;
        if end > self.bytes.len() {
            return Err(TrustStateStoreError::InvalidEncoding("truncated state"));
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, TrustStateStoreError> {
        Ok(self.take(1)?[0])
    }

    fn u64(&mut self) -> Result<u64, TrustStateStoreError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().expect("cursor length is exact")))
    }

    fn string(&mut self) -> Result<String, TrustStateStoreError> {
        let length = self.u64()?;
        if length == 0 || length > MAX_TEXT_BYTES {
            return Err(TrustStateStoreError::InvalidEncoding("invalid text length"));
        }
        let bytes = self.take(length as usize)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| TrustStateStoreError::InvalidEncoding("text is not UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use erf_trust::{TrustCheckpoint, TrustError};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn state(version: u64) -> TrustState {
        let checkpoint = checkpoint(version, [7; 32]);
        let mut state = TrustState::default();
        state.accept_checkpoint(&checkpoint, 20).unwrap();
        state
    }

    fn checkpoint(version: u64, snapshot_digest: [u8; 32]) -> TrustCheckpoint {
        TrustCheckpoint {
            repository: "org/repo".into(),
            version,
            role_versions: BTreeMap::from([(String::from("runtime-pack-signing"), version + 1), (String::from("trust-root"), version)]),
            snapshot_digest,
            issued_at_epoch_secs: 10,
            expires_at_epoch_secs: 100,
        }
    }

    fn signed(checkpoint: &TrustCheckpoint) -> (SignedAsset, TrustStore) {
        let key = SigningKey::from_bytes(&[91; 32]);
        let asset = checkpoint.sign_with("checkpoint-key", |bytes| key.sign(bytes).to_bytes().to_vec());
        let mut store = TrustStore::default();
        store.insert_key("checkpoint-key", key.verifying_key()).unwrap();
        store.authorize_role("trust-root", "checkpoint-key").unwrap();
        (asset, store)
    }

    fn temp_path(label: &str) -> PathBuf {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("erf-trust-state-{label}-{}-{unique}", std::process::id()))
    }

    fn fail_directory_sync_on(call: usize) -> DirectorySync {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_sync = Arc::clone(&calls);
        Arc::new(move |_| {
            let current = calls_for_sync.fetch_add(1, Ordering::SeqCst) + 1;
            if current == call {
                Err(std::io::Error::other(format!("injected directory sync failure {current}")))
            } else {
                Ok(())
            }
        })
    }

    #[test]
    fn encoding_is_deterministic_and_round_trips() {
        let original = state(2);
        let first = encode(&original).unwrap();
        let second = encode(&original).unwrap();
        assert_eq!(first, second);
        assert_eq!(decode(&first).unwrap().snapshot(), original.snapshot());
    }

    #[test]
    fn decoder_rejects_unknown_truncated_duplicate_and_oversized_state() {
        let original = encode(&state(2)).unwrap();
        assert_eq!(decode(b"erf-trust-state-v9\0"), Err(TrustStateStoreError::InvalidEncoding("unknown state version")));
        let mut truncated = original.clone();
        truncated.pop();
        assert_eq!(decode(&truncated), Err(TrustStateStoreError::InvalidEncoding("truncated state")));
        let mut duplicate = MAGIC.to_vec();
        duplicate.extend_from_slice(&2_u64.to_be_bytes());
        duplicate.push(1);
        duplicate.extend_from_slice(&8_u64.to_be_bytes());
        duplicate.extend_from_slice(b"org/repo");
        duplicate.extend_from_slice(&2_u64.to_be_bytes());
        for version in [1_u64, 2_u64] {
            duplicate.extend_from_slice(&4_u64.to_be_bytes());
            duplicate.extend_from_slice(b"role");
            duplicate.extend_from_slice(&version.to_be_bytes());
        }
        assert_eq!(decode(&duplicate), Err(TrustStateStoreError::InvalidEncoding("duplicate role")));
        assert_eq!(decode(&vec![0; MAX_STATE_BYTES + 1]), Err(TrustStateStoreError::TooLarge { limit: MAX_STATE_BYTES }));
    }

    #[test]
    fn file_round_trip_replacement_and_missing_are_explicit() {
        let path = temp_path("round-trip");
        let file = TrustStateFile::new(&path);
        assert_eq!(file.load().unwrap(), None);
        assert_eq!(file.load_required(), Err(TrustStateStoreError::Missing));
        let first = state(2);
        file.save(&first).unwrap();
        assert_eq!(file.load_required().unwrap().snapshot(), first.snapshot());
        let second = state(3);
        file.save(&second).unwrap();
        assert_eq!(file.load_required().unwrap().snapshot(), second.snapshot());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn reloaded_state_keeps_checkpoint_rollback_floor() {
        let path = temp_path("rollback");
        let file = TrustStateFile::new(&path);
        file.save(&state(2)).unwrap();
        let mut reloaded = file.load_required().unwrap();
        let rollback = TrustCheckpoint {
            repository: "org/repo".into(),
            version: 1,
            role_versions: BTreeMap::from([(String::from("runtime-pack-signing"), 2), (String::from("trust-root"), 1)]),
            snapshot_digest: [8; 32],
            issued_at_epoch_secs: 10,
            expires_at_epoch_secs: 100,
        };
        assert!(matches!(reloaded.accept_checkpoint(&rollback, 20), Err(TrustError::CheckpointRollback { .. })));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn explicit_initialization_and_advance_are_replayable_but_missing_state_is_not_trusted() {
        let path = temp_path("checkpoint-lifecycle");
        let file = TrustStateFile::new(&path);
        let first = checkpoint(1, [7; 32]);
        let (first_asset, store) = signed(&first);

        assert_eq!(file.load_for_trusted_execution(), Err(TrustStateStoreError::Missing));
        let initialized = file.initialize_signed_checkpoint(&first, &first_asset, &store, "trust-root", 20).unwrap();
        assert_eq!(initialized.highest_checkpoint_version(), 1);
        assert_eq!(file.initialize_signed_checkpoint(&first, &first_asset, &store, "trust-root", 20), Err(TrustStateStoreError::AlreadyInitialized));

        let second = checkpoint(2, [8; 32]);
        let (second_asset, second_store) = signed(&second);
        let advanced = file.advance_signed_checkpoint(&second, &second_asset, &second_store, "trust-root", 20).unwrap();
        assert_eq!(advanced.highest_checkpoint_version(), 2);
        assert_eq!(file.load_for_trusted_execution().unwrap().snapshot(), advanced.snapshot());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn separate_file_handles_cannot_race_initialization() {
        let path = temp_path("checkpoint-concurrent-initialize");
        let checkpoint = checkpoint(1, [7; 32]);
        let (asset, store) = signed(&checkpoint);
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let first_barrier = Arc::clone(&barrier);
        let first_path = path.clone();
        let first_checkpoint = checkpoint.clone();
        let first_asset = asset.clone();
        let first_store = store.clone();
        let first = std::thread::spawn(move || {
            let file = TrustStateFile::new(first_path);
            first_barrier.wait();
            file.initialize_signed_checkpoint(&first_checkpoint, &first_asset, &first_store, "trust-root", 20)
        });
        let second_barrier = Arc::clone(&barrier);
        let second_path = path.clone();
        let second = std::thread::spawn(move || {
            let file = TrustStateFile::new(second_path);
            second_barrier.wait();
            file.initialize_signed_checkpoint(&checkpoint, &asset, &store, "trust-root", 20)
        });
        let results = [first.join().unwrap(), second.join().unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| **result == Err(TrustStateStoreError::AlreadyInitialized)).count(), 1);
        fs::remove_file(&path).unwrap();
        fs::remove_file(path.parent().unwrap().join(format!(".{}.lock", path.file_name().unwrap().to_string_lossy()))).unwrap();
    }

    #[test]
    fn advance_rejects_tampering_and_rollback_without_replacing_existing_state() {
        let path = temp_path("checkpoint-fail-closed");
        let file = TrustStateFile::new(&path);
        let first = checkpoint(2, [7; 32]);
        let (first_asset, store) = signed(&first);
        file.initialize_signed_checkpoint(&first, &first_asset, &store, "trust-root", 20).unwrap();
        let before = file.load_required().unwrap().snapshot();

        let rollback = checkpoint(1, [8; 32]);
        let (rollback_asset, rollback_store) = signed(&rollback);
        assert!(matches!(file.advance_signed_checkpoint(&rollback, &rollback_asset, &rollback_store, "trust-root", 20), Err(TrustStateStoreError::InvalidState(TrustError::CheckpointRollback { .. }))));
        assert_eq!(file.load_required().unwrap().snapshot(), before);

        let mut tampered = first_asset.clone();
        tampered.payload.push(b'x');
        assert!(matches!(file.advance_signed_checkpoint(&first, &tampered, &store, "trust-root", 20), Err(TrustStateStoreError::InvalidState(TrustError::InvalidSignature))));
        assert_eq!(file.load_required().unwrap().snapshot(), before);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_atomic_replace_leaves_existing_target_untouched() {
        let path = temp_path("replace-failure");
        fs::create_dir(&path).unwrap();
        let file = TrustStateFile::new(&path);
        assert!(file.save(&state(2)).is_err());
        assert!(fs::metadata(&path).unwrap().is_dir());
        fs::remove_dir(path).unwrap();
    }

    #[test]
    fn initialize_directory_sync_failure_is_not_reported_as_persisted() {
        let path = temp_path("initialize-directory-sync-failure");
        let file = TrustStateFile::with_directory_sync(&path, fail_directory_sync_on(1));
        let first = checkpoint(1, [7; 32]);
        let (asset, store) = signed(&first);

        let result = file.initialize_signed_checkpoint(&first, &asset, &store, "trust-root", 20);
        assert!(matches!(result, Err(TrustStateStoreError::Io(message)) if message.contains("directory sync before rename failed")));
        assert_eq!(TrustStateFile::new(&path).load(), Ok(None));
        fs::remove_file(path).ok();
    }

    #[test]
    fn advance_directory_sync_failure_preserves_previous_state() {
        let path = temp_path("advance-directory-sync-failure");
        let file = TrustStateFile::new(&path);
        let first = checkpoint(1, [7; 32]);
        let (first_asset, first_store) = signed(&first);
        file.initialize_signed_checkpoint(&first, &first_asset, &first_store, "trust-root", 20).unwrap();
        let before = file.load_required().unwrap().snapshot();

        let failing_file = TrustStateFile::with_directory_sync(&path, fail_directory_sync_on(2));
        let second = checkpoint(2, [8; 32]);
        let (second_asset, second_store) = signed(&second);
        let result = failing_file.advance_signed_checkpoint(&second, &second_asset, &second_store, "trust-root", 20);
        assert!(matches!(result, Err(TrustStateStoreError::Io(message)) if message.contains("directory sync after rename failed")));
        assert_eq!(TrustStateFile::new(&path).load_required().unwrap().snapshot(), before);
        fs::remove_file(path).unwrap();
    }
}
