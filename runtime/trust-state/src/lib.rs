//! Bounded persistence boundary for offline TrustState.
//!
//! `erf-trust` remains a pure in-memory verifier. This crate owns only the
//! local state file: it uses a deterministic length-delimited format, bounded
//! parsing, fsync and atomic same-directory replacement. Missing state is
//! explicit so callers cannot silently treat a lost rollback floor as trust.

use erf_trust::{TrustError, TrustState, TrustStateSnapshot};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

const MAGIC: &[u8] = b"erf-trust-state-v0\0";
pub const MAX_STATE_BYTES: usize = 64 * 1024;
const MAX_ROLE_COUNT: u64 = 1024;
const MAX_TEXT_BYTES: u64 = 4096;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TrustStateStoreError {
    #[error("trust state is missing")]
    Missing,
    #[error("trust state exceeds the {limit}-byte limit")]
    TooLarge { limit: usize },
    #[error("trust state encoding is invalid: {0}")]
    InvalidEncoding(&'static str),
    #[error("trust state I/O failed: {0}")]
    Io(String),
    #[error(transparent)]
    InvalidState(#[from] TrustError),
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

pub struct TrustStateFile {
    path: PathBuf,
}

impl TrustStateFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Option<TrustState>, TrustStateStoreError> {
        match fs::read(&self.path) {
            Ok(bytes) => decode(&bytes).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(TrustStateStoreError::Io(error.to_string())),
        }
    }

    pub fn load_required(&self) -> Result<TrustState, TrustStateStoreError> {
        self.load()?.ok_or(TrustStateStoreError::Missing)
    }

    pub fn save(&self, state: &TrustState) -> Result<(), TrustStateStoreError> {
        let bytes = encode(state)?;
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = self.path.file_name().ok_or(TrustStateStoreError::InvalidEncoding("state path has no file name"))?.to_string_lossy();
        let temporary = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let mut file = options.open(&temporary).map_err(|error| TrustStateStoreError::Io(error.to_string()))?;
            file.write_all(&bytes).map_err(|error| TrustStateStoreError::Io(error.to_string()))?;
            file.sync_all().map_err(|error| TrustStateStoreError::Io(error.to_string()))?;
            fs::rename(&temporary, &self.path).map_err(|error| TrustStateStoreError::Io(error.to_string()))?;
            if let Ok(directory) = File::open(parent) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
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
    use erf_trust::{TrustCheckpoint, TrustError};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn state(version: u64) -> TrustState {
        let checkpoint = TrustCheckpoint {
            repository: "org/repo".into(),
            version,
            role_versions: BTreeMap::from([(String::from("runtime-pack-signing"), version + 1), (String::from("trust-root"), version)]),
            snapshot_digest: [7; 32],
            issued_at_epoch_secs: 10,
            expires_at_epoch_secs: 100,
        };
        let mut state = TrustState::default();
        state.accept_checkpoint(&checkpoint, 20).unwrap();
        state
    }

    fn temp_path(label: &str) -> PathBuf {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("erf-trust-state-{label}-{}-{unique}", std::process::id()))
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
    fn failed_atomic_replace_leaves_existing_target_untouched() {
        let path = temp_path("replace-failure");
        fs::create_dir(&path).unwrap();
        let file = TrustStateFile::new(&path);
        assert!(file.save(&state(2)).is_err());
        assert!(fs::metadata(&path).unwrap().is_dir());
        fs::remove_dir(path).unwrap();
    }
}
