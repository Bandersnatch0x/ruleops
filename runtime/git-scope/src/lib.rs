//! Deterministic Git change-range resolution for future incremental planning.
//!
//! The external seam is deliberately small: callers provide an explicit base
//! and head revision and receive a closed, sorted `ChangedSet`. Git command
//! execution, merge-base selection, NUL parsing, path normalization, shallow
//! repository refusal and digest construction stay inside this module. It
//! never writes Git state, contacts an SCM service, uploads source, or grants
//! scan/Gate authority.

use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;

pub const CHANGED_SET_VERSION: &str = "changed-set/v1";

/// Read-only guard that refuses to derive a ChangedSet from an unmerged
/// index/worktree state.
#[derive(Debug, Clone, Copy, Default)]
pub struct RejectUnmergedWorktreeBeforeChangedSet;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GitChangeScopeError {
    #[error("git command {operation} could not start: {detail}")]
    CommandUnavailable { operation: &'static str, detail: String },
    #[error("git command {operation} failed with exit {code:?}: {stderr}")]
    GitCommand { operation: &'static str, code: Option<i32>, stderr: String },
    #[error("git repository is shallow; a complete merge-base cannot be proven")]
    ShallowRepository,
    #[error("git repository state is invalid: {0}")]
    RepositoryState(String),
    #[error("{field} revision is invalid: {reason}")]
    InvalidRevision { field: &'static str, reason: &'static str },
    #[error("{field} revision did not resolve to a commit: {revision}")]
    RevisionNotFound { field: &'static str, revision: String },
    #[error("base and head have no common merge-base")]
    MissingMergeBase,
    #[error("base and head have multiple merge-bases; range is ambiguous")]
    AmbiguousMergeBase,
    #[error("git object id is invalid: {0}")]
    InvalidObjectId(String),
    #[error("git name-status stream is malformed: {0}")]
    MalformedNameStatus(String),
    #[error("git conflict status cannot define a safe ChangedSet: {0}")]
    ConflictingStatus(String),
    #[error("git repository has unmerged index/worktree state; ChangedSet refused before diff ({area})")]
    UnmergedWorktree { area: &'static str },
    #[error("git status is unsupported for a ChangedSet: {0}")]
    UnsupportedStatus(String),
    #[error("changed path is invalid: {0}")]
    InvalidPath(String),
    #[error("changed path is not valid UTF-8")]
    InvalidPathEncoding,
}

/// Explicit revision inputs. There is no implicit `HEAD~1` fallback.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GitChangeRange {
    base: String,
    head: String,
}

impl GitChangeRange {
    pub fn new(base: impl Into<String>, head: impl Into<String>) -> Result<Self, GitChangeScopeError> {
        let base = base.into();
        let head = head.into();
        validate_revision("base", &base)?;
        validate_revision("head", &head)?;
        Ok(Self { base, head })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn head(&self) -> &str {
        &self.head
    }
}

/// Resolved object identities used to bind the ChangedSet to one Git range.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommitRange {
    base_commit: String,
    head_commit: String,
    merge_base: String,
}

impl CommitRange {
    fn new(base_commit: String, head_commit: String, merge_base: String) -> Result<Self, GitChangeScopeError> {
        validate_object_id(&base_commit)?;
        validate_object_id(&head_commit)?;
        validate_object_id(&merge_base)?;
        Ok(Self { base_commit, head_commit, merge_base })
    }

    pub fn base_commit(&self) -> &str {
        &self.base_commit
    }

    pub fn head_commit(&self) -> &str {
        &self.head_commit
    }

    pub fn merge_base(&self) -> &str {
        &self.merge_base
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
}

impl ChangeKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Added => "A",
            Self::Modified => "M",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Copied => "C",
            Self::TypeChanged => "T",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChangedPath {
    kind: ChangeKind,
    path: String,
    old_path: Option<String>,
    similarity: Option<u8>,
}

impl ChangedPath {
    fn new(kind: ChangeKind, path: &[u8], old_path: Option<&[u8]>, similarity: Option<u8>) -> Result<Self, GitChangeScopeError> {
        let path = normalize_repo_relative(path)?;
        let old_path = old_path.map(normalize_repo_relative).transpose()?;
        match kind {
            ChangeKind::Renamed | ChangeKind::Copied => {
                if old_path.is_none() || similarity.is_none() {
                    return Err(GitChangeScopeError::MalformedNameStatus(format!("{} requires old path and similarity", kind.code())));
                }
                if old_path.as_deref() == Some(path.as_str()) {
                    return Err(GitChangeScopeError::InvalidPath("rename/copy source equals destination".into()));
                }
            }
            ChangeKind::Added | ChangeKind::Modified | ChangeKind::Deleted | ChangeKind::TypeChanged => {
                if old_path.is_some() || similarity.is_some() {
                    return Err(GitChangeScopeError::MalformedNameStatus(format!("{} cannot carry rename metadata", kind.code())));
                }
            }
        }
        Ok(Self { kind, path, old_path, similarity })
    }

    pub fn kind(&self) -> ChangeKind {
        self.kind
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn old_path(&self) -> Option<&str> {
        self.old_path.as_deref()
    }

    pub fn similarity(&self) -> Option<u8> {
        self.similarity
    }
}

/// Closed, sorted and digestable output of one explicit Git range.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChangedSet {
    range: CommitRange,
    entries: Vec<ChangedPath>,
}

impl ChangedSet {
    fn new(range: CommitRange, mut entries: Vec<ChangedPath>) -> Self {
        entries.sort_by(changed_path_order);
        Self { range, entries }
    }

    pub fn range(&self) -> &CommitRange {
        &self.range
    }

    pub fn entries(&self) -> &[ChangedPath] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Stable bytes for future ScanPlan/cache identity. Any schema change
    /// requires a new `CHANGED_SET_VERSION` domain separator.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = CHANGED_SET_VERSION.as_bytes().to_vec();
        bytes.push(0);
        append_string(&mut bytes, &self.range.base_commit);
        append_string(&mut bytes, &self.range.head_commit);
        append_string(&mut bytes, &self.range.merge_base);
        bytes.extend_from_slice(&(self.entries.len() as u64).to_be_bytes());
        for entry in &self.entries {
            append_string(&mut bytes, entry.kind.code());
            append_string(&mut bytes, &entry.path);
            match &entry.old_path {
                None => bytes.push(0),
                Some(path) => {
                    bytes.push(1);
                    append_string(&mut bytes, path);
                }
            }
            match entry.similarity {
                None => bytes.push(0),
                Some(score) => {
                    bytes.push(1);
                    bytes.push(score);
                }
            }
        }
        bytes
    }

    pub fn digest_bytes(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_bytes()).into()
    }

    pub fn digest(&self) -> String {
        hex_digest(&self.digest_bytes())
    }
}

/// Git adapter at the external seam. It is read-only and returns a complete
/// ChangedSet or a structured error; it never guesses a range or falls back to
/// the whole repository.
#[derive(Debug, Clone)]
pub struct GitChangeResolver {
    repository_root: PathBuf,
}

impl GitChangeResolver {
    pub fn new(repository_root: impl Into<PathBuf>) -> Self {
        Self { repository_root: repository_root.into() }
    }

    pub fn resolve(&self, range: &GitChangeRange) -> Result<ChangedSet, GitChangeScopeError> {
        self.ensure_repository()?;
        RejectUnmergedWorktreeBeforeChangedSet::check(self)?;
        self.reject_shallow_repository()?;
        let base_commit = self.resolve_commit("base", range.base())?;
        let head_commit = self.resolve_commit("head", range.head())?;
        let merge_base = self.resolve_merge_base(&base_commit, &head_commit)?;
        let commits = CommitRange::new(base_commit, head_commit, merge_base)?;
        let output = self.run_git(
            "diff-name-status",
            vec![
                "diff".into(),
                "--name-status".into(),
                "-z".into(),
                "--find-renames=50%".into(),
                "--find-copies=50%".into(),
                "--find-copies-harder".into(),
                "--no-ext-diff".into(),
                "--no-textconv".into(),
                "--diff-algorithm=myers".into(),
                commits.merge_base.clone(),
                commits.head_commit.clone(),
                "--".into(),
            ],
        )?;
        Ok(ChangedSet::new(commits, parse_git_name_status_z(&output)?))
    }

    fn reject_unmerged_worktree(&self) -> Result<(), GitChangeScopeError> {
        let index = self.run_git(
            "unmerged-index",
            vec!["ls-files".into(), "--unmerged".into(), "-z".into(), "--".into()],
        )?;
        if !index.is_empty() {
            return Err(GitChangeScopeError::UnmergedWorktree { area: "index" });
        }

        let worktree = self.run_git(
            "unmerged-worktree",
            vec![
                "diff".into(),
                "--name-only".into(),
                "--diff-filter=U".into(),
                "--no-ext-diff".into(),
                "--no-textconv".into(),
                "-z".into(),
                "--".into(),
            ],
        )?;
        if !worktree.is_empty() {
            return Err(GitChangeScopeError::UnmergedWorktree { area: "worktree" });
        }
        Ok(())
    }

    fn ensure_repository(&self) -> Result<(), GitChangeScopeError> {
        self.run_git("repository", vec!["rev-parse".into(), "--show-toplevel".into()])?;
        Ok(())
    }

    fn reject_shallow_repository(&self) -> Result<(), GitChangeScopeError> {
        let output = self.run_git("shallow-check", vec!["rev-parse".into(), "--is-shallow-repository".into()])?;
        match std::str::from_utf8(&output).map_err(|error| GitChangeScopeError::RepositoryState(error.to_string()))?.trim() {
            "true" => Err(GitChangeScopeError::ShallowRepository),
            "false" => Ok(()),
            value => Err(GitChangeScopeError::RepositoryState(format!("unexpected shallow state {value:?}"))),
        }
    }

    fn resolve_commit(&self, field: &'static str, revision: &str) -> Result<String, GitChangeScopeError> {
        let revision = format!("{revision}^{{commit}}");
        let output = self.run_git(
            "resolve-base",
            vec!["rev-parse".into(), "--verify".into(), "--quiet".into(), "--end-of-options".into(), revision.clone()],
        ).map_err(|error| match error {
            GitChangeScopeError::GitCommand { .. } => GitChangeScopeError::RevisionNotFound { field, revision: revision.clone() },
            other => other,
        })?;
        let commit = single_line(&output).ok_or(GitChangeScopeError::RevisionNotFound { field, revision })?;
        validate_object_id(&commit)?;
        Ok(commit)
    }

    fn resolve_merge_base(&self, base: &str, head: &str) -> Result<String, GitChangeScopeError> {
        let output = self.run_git("merge-base", vec!["merge-base".into(), "--all".into(), base.into(), head.into()])?;
        let merge_bases: Vec<String> = std::str::from_utf8(&output)
            .map_err(|error| GitChangeScopeError::RepositoryState(error.to_string()))?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        match merge_bases.as_slice() {
            [] => Err(GitChangeScopeError::MissingMergeBase),
            [merge_base] => {
                validate_object_id(merge_base)?;
                Ok(merge_base.clone())
            }
            _ => Err(GitChangeScopeError::AmbiguousMergeBase),
        }
    }

    fn run_git(&self, operation: &'static str, args: Vec<String>) -> Result<Vec<u8>, GitChangeScopeError> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repository_root)
            .args(args)
            .output()
            .map_err(|error| GitChangeScopeError::CommandUnavailable { operation, detail: error.to_string() })?;
        if !output.status.success() {
            return Err(GitChangeScopeError::GitCommand {
                operation,
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        Ok(output.stdout)
    }
}

impl RejectUnmergedWorktreeBeforeChangedSet {
    fn check(resolver: &GitChangeResolver) -> Result<(), GitChangeScopeError> {
        resolver.reject_unmerged_worktree()
    }
}

/// Parse Git's `--name-status -z` stream without invoking Git. NUL framing is
/// required so filenames containing spaces, tabs or newlines cannot alter the
/// record structure. Paths are normalized at this seam and never escape the
/// repository-relative namespace.
pub fn parse_git_name_status_z(bytes: &[u8]) -> Result<Vec<ChangedPath>, GitChangeScopeError> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if !bytes.ends_with(&[0]) {
        return Err(GitChangeScopeError::MalformedNameStatus("stream is not NUL terminated".into()));
    }
    let mut tokens: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    tokens.pop();
    if tokens.iter().any(|token| token.is_empty()) {
        return Err(GitChangeScopeError::MalformedNameStatus("empty record token".into()));
    }

    let mut entries = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let (kind, similarity) = parse_status_token(tokens[index])?;
        index += 1;
        match kind {
            ChangeKind::Renamed | ChangeKind::Copied => {
                if index + 1 >= tokens.len() {
                    return Err(GitChangeScopeError::MalformedNameStatus(format!("{} record is missing old/new path", kind.code())));
                }
                let old_path = tokens[index];
                let new_path = tokens[index + 1];
                index += 2;
                entries.push(ChangedPath::new(kind, new_path, Some(old_path), similarity)?);
            }
            ChangeKind::Added | ChangeKind::Modified | ChangeKind::Deleted | ChangeKind::TypeChanged => {
                if index >= tokens.len() {
                    return Err(GitChangeScopeError::MalformedNameStatus(format!("{} record is missing path", kind.code())));
                }
                let path = tokens[index];
                index += 1;
                entries.push(ChangedPath::new(kind, path, None, None)?);
            }
        }
    }
    Ok(entries)
}

fn parse_status_token(token: &[u8]) -> Result<(ChangeKind, Option<u8>), GitChangeScopeError> {
    if token.is_empty() || token.contains(&b'\t') {
        return Err(GitChangeScopeError::MalformedNameStatus("status token is empty or contains an inline path".into()));
    }
    let code = token[0] as char;
    match code {
        'A' => exact_status(token, ChangeKind::Added),
        'M' => exact_status(token, ChangeKind::Modified),
        'D' => exact_status(token, ChangeKind::Deleted),
        'T' => exact_status(token, ChangeKind::TypeChanged),
        'R' => scored_status(token, ChangeKind::Renamed),
        'C' => scored_status(token, ChangeKind::Copied),
        'U' | 'B' => Err(GitChangeScopeError::ConflictingStatus(String::from_utf8_lossy(token).into_owned())),
        _ => Err(GitChangeScopeError::UnsupportedStatus(String::from_utf8_lossy(token).into_owned())),
    }
}

fn exact_status(token: &[u8], kind: ChangeKind) -> Result<(ChangeKind, Option<u8>), GitChangeScopeError> {
    if token.len() != 1 {
        return Err(GitChangeScopeError::MalformedNameStatus(format!("{} status has an unexpected score", kind.code())));
    }
    Ok((kind, None))
}

fn scored_status(token: &[u8], kind: ChangeKind) -> Result<(ChangeKind, Option<u8>), GitChangeScopeError> {
    if token.len() < 2 || !token[1..].iter().all(u8::is_ascii_digit) {
        return Err(GitChangeScopeError::MalformedNameStatus(format!("{} status requires a numeric similarity score", kind.code())));
    }
    let score = std::str::from_utf8(&token[1..]).ok().and_then(|value| value.parse::<u8>().ok()).ok_or_else(|| GitChangeScopeError::MalformedNameStatus("similarity score is invalid".into()))?;
    if score > 100 {
        return Err(GitChangeScopeError::MalformedNameStatus("similarity score exceeds 100".into()));
    }
    Ok((kind, Some(score)))
}

fn normalize_repo_relative(bytes: &[u8]) -> Result<String, GitChangeScopeError> {
    let path = std::str::from_utf8(bytes).map_err(|_| GitChangeScopeError::InvalidPathEncoding)?;
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') || path.contains('\\') {
        return Err(GitChangeScopeError::InvalidPath(path.into()));
    }
    if path.len() >= 2 && path.as_bytes()[1] == b':' && path.as_bytes()[0].is_ascii_alphabetic() {
        return Err(GitChangeScopeError::InvalidPath(path.into()));
    }
    let mut components = Vec::new();
    for component in path.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." || component.chars().any(char::is_control) {
            return Err(GitChangeScopeError::InvalidPath(path.into()));
        }
        components.push(component);
    }
    if components.is_empty() {
        return Err(GitChangeScopeError::InvalidPath(path.into()));
    }
    Ok(components.join("/"))
}

fn validate_revision(field: &'static str, value: &str) -> Result<(), GitChangeScopeError> {
    if value.is_empty() {
        return Err(GitChangeScopeError::InvalidRevision { field, reason: "empty" });
    }
    if value.starts_with('-') {
        return Err(GitChangeScopeError::InvalidRevision { field, reason: "must not start with a command option" });
    }
    if value.chars().any(|character| character.is_whitespace() || character.is_control()) {
        return Err(GitChangeScopeError::InvalidRevision { field, reason: "contains whitespace or control characters" });
    }
    Ok(())
}

fn validate_object_id(value: &str) -> Result<(), GitChangeScopeError> {
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(GitChangeScopeError::InvalidObjectId(value.into()));
    }
    Ok(())
}

fn single_line(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let line = lines.next()?.to_string();
    lines.next().is_none().then_some(line)
}

fn changed_path_order(left: &ChangedPath, right: &ChangedPath) -> Ordering {
    left.path
        .cmp(&right.path)
        .then_with(|| left.kind.cmp(&right.kind))
        .then_with(|| left.old_path.cmp(&right.old_path))
        .then_with(|| left.similarity.cmp(&right.similarity))
}

fn append_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::process::Output;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn range() -> CommitRange {
        CommitRange::new("a".repeat(40), "b".repeat(40), "c".repeat(40)).unwrap()
    }

    fn temp_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("erf-git-scope-{label}-{}-{nanos}", std::process::id()))
    }

    fn git(repo: &Path, args: &[&str]) -> Output {
        let output = Command::new("git").arg("-C").arg(repo).args(args).output().unwrap();
        assert!(output.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&output.stderr));
        output
    }

    fn git_text(repo: &Path, args: &[&str]) -> String {
        String::from_utf8(git(repo, args).stdout).unwrap().trim().to_string()
    }

    fn init_repo(label: &str) -> PathBuf {
        let repo = temp_path(label);
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "--quiet"]);
        git(&repo, &["config", "user.email", "erf-tests@example.invalid"]);
        git(&repo, &["config", "user.name", "ERF Tests"]);
        repo
    }

    #[test]
    fn parser_supports_a_m_d_r_c_t_and_sorts_digest_deterministically() {
        let first = parse_git_name_status_z(
            b"M\0z.java\0R100\0old.java\0new.java\0C75\0source.java\0copy.java\0A\0./a.java\0D\0dir/../bad.java\0",
        );
        assert!(matches!(first, Err(GitChangeScopeError::InvalidPath(_))));

        let left = parse_git_name_status_z(b"M\0z.java\0R100\0old.java\0new.java\0C75\0source.java\0copy.java\0A\0./a.java\0T\0type.java\0").unwrap();
        let right = parse_git_name_status_z(b"T\0type.java\0A\0a.java\0C75\0source.java\0copy.java\0R100\0old.java\0new.java\0M\0z.java\0").unwrap();
        let left_set = ChangedSet::new(range(), left);
        let right_set = ChangedSet::new(range(), right);
        assert_eq!(left_set, right_set);
        assert_eq!(left_set.digest(), right_set.digest());
        assert_eq!(left_set.entries().len(), 5);
        assert_eq!(left_set.entries()[0].path(), "a.java");
        assert_eq!(left_set.entries()[1].old_path(), Some("source.java"));
    }

    #[test]
    fn parser_rejects_conflicts_malformed_scores_and_escape_paths() {
        assert!(matches!(parse_git_name_status_z(b"U\0conflict.java\0"), Err(GitChangeScopeError::ConflictingStatus(_))));
        assert!(matches!(parse_git_name_status_z(b"R101\0old.java\0new.java\0"), Err(GitChangeScopeError::MalformedNameStatus(_))));
        assert!(matches!(parse_git_name_status_z(b"M\0../outside.java\0"), Err(GitChangeScopeError::InvalidPath(_))));
        assert!(matches!(parse_git_name_status_z(b"M\0C:\\outside.java\0"), Err(GitChangeScopeError::InvalidPath(_))));
        assert!(matches!(parse_git_name_status_z(b"M\0file.java"), Err(GitChangeScopeError::MalformedNameStatus(_))));
    }

    #[test]
    fn resolver_uses_merge_base_and_returns_a_stable_empty_set_and_changes() {
        let repo = init_repo("range");
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src/original.java"), "class Original {}\n").unwrap();
        fs::write(repo.join("src/old.java"), "class Old {}\n").unwrap();
        fs::write(repo.join("src/modified.java"), "class Modified {}\n").unwrap();
        fs::write(repo.join("src/deleted.java"), "class Deleted {}\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "--quiet", "-m", "base"]);
        let base = git_text(&repo, &["rev-parse", "HEAD"]);

        git(&repo, &["mv", "src/old.java", "src/renamed.java"]);
        fs::write(repo.join("src/modified.java"), "class Modified { int changed; }\n").unwrap();
        fs::remove_file(repo.join("src/deleted.java")).unwrap();
        fs::copy(repo.join("src/original.java"), repo.join("src/copied.java")).unwrap();
        fs::write(repo.join("src/added.java"), "class Added {}\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "--quiet", "-m", "head"]);
        let head = git_text(&repo, &["rev-parse", "HEAD"]);

        let resolver = GitChangeResolver::new(&repo);
        let range = GitChangeRange::new(&base, &head).unwrap();
        let changes = resolver.resolve(&range).unwrap();
        assert_eq!(changes.range().base_commit(), base);
        assert_eq!(changes.range().head_commit(), head);
        assert!(!changes.range().merge_base().is_empty());
        assert!(changes.entries().iter().any(|entry| entry.kind() == ChangeKind::Added && entry.path() == "src/added.java"));
        assert!(changes.entries().iter().any(|entry| entry.kind() == ChangeKind::Modified && entry.path() == "src/modified.java"));
        assert!(changes.entries().iter().any(|entry| entry.kind() == ChangeKind::Deleted && entry.path() == "src/deleted.java"));
        assert!(changes.entries().iter().any(|entry| entry.kind() == ChangeKind::Renamed && entry.old_path() == Some("src/old.java") && entry.path() == "src/renamed.java"));
        assert!(changes.entries().iter().any(|entry| entry.kind() == ChangeKind::Copied && entry.old_path() == Some("src/original.java") && entry.path() == "src/copied.java"));
        let repeated = resolver.resolve(&range).unwrap();
        assert_eq!(changes, repeated);
        assert_eq!(changes.digest(), repeated.digest());
        assert!(resolver.resolve(&GitChangeRange::new(&base, &base).unwrap()).unwrap().is_empty());
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn resolver_rejects_real_unmerged_merge_and_rebase_before_resolving_changed_set() {
        let repo = init_repo("real-conflict");
        fs::write(repo.join("conflict.txt"), "base\n").unwrap();
        git(&repo, &["add", "conflict.txt"]);
        git(&repo, &["commit", "--quiet", "-m", "base"]);
        let base_branch = git_text(&repo, &["symbolic-ref", "--short", "HEAD"]);
        let base = git_text(&repo, &["rev-parse", "HEAD"]);
        git(&repo, &["branch", "side"]);

        fs::write(repo.join("conflict.txt"), "ours\n").unwrap();
        git(&repo, &["add", "conflict.txt"]);
        git(&repo, &["commit", "--quiet", "-m", "ours"]);
        let head = git_text(&repo, &["rev-parse", "HEAD"]);

        git(&repo, &["checkout", "--quiet", "side"]);
        fs::write(repo.join("conflict.txt"), "theirs\n").unwrap();
        git(&repo, &["add", "conflict.txt"]);
        git(&repo, &["commit", "--quiet", "-m", "theirs"]);
        git(&repo, &["checkout", "--quiet", &base_branch]);

        let merge = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["merge", "--no-commit", "--no-ff", "side"])
            .output()
            .unwrap();
        assert!(!merge.status.success(), "merge unexpectedly succeeded");

        let resolver = GitChangeResolver::new(&repo);
        let range = GitChangeRange::new(&base, &head).unwrap();
        let error = resolver.resolve(&range).unwrap_err();
        assert!(matches!(error, GitChangeScopeError::UnmergedWorktree { .. }));

        git(&repo, &["merge", "--abort"]);
        git(&repo, &["checkout", "--quiet", "side"]);
        let rebase = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["rebase", &base_branch])
            .output()
            .unwrap();
        assert!(!rebase.status.success(), "rebase unexpectedly succeeded");
        let rebase_error = resolver.resolve(&range).unwrap_err();
        assert!(matches!(rebase_error, GitChangeScopeError::UnmergedWorktree { .. }));

        git(&repo, &["rebase", "--abort"]);
        git(&repo, &["checkout", "--quiet", &base_branch]);
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn resolver_rejects_a_real_multiple_merge_base_range() {
        let repo = init_repo("multiple-merge-base");
        fs::write(repo.join("root.txt"), "root\n").unwrap();
        git(&repo, &["add", "root.txt"]);
        git(&repo, &["commit", "--quiet", "-m", "root"]);
        git(&repo, &["branch", "left"]);
        git(&repo, &["branch", "right"]);

        git(&repo, &["checkout", "--quiet", "left"]);
        fs::write(repo.join("left.txt"), "left\n").unwrap();
        git(&repo, &["add", "left.txt"]);
        git(&repo, &["commit", "--quiet", "-m", "left"]);

        git(&repo, &["checkout", "--quiet", "right"]);
        fs::write(repo.join("right.txt"), "right\n").unwrap();
        git(&repo, &["add", "right.txt"]);
        git(&repo, &["commit", "--quiet", "-m", "right"]);

        git(&repo, &["checkout", "--quiet", "left"]);
        git(&repo, &["merge", "--no-ff", "--quiet", "right", "-m", "left merge"]);
        git(&repo, &["checkout", "--quiet", "right"]);
        git(&repo, &["merge", "--no-ff", "--quiet", "left~1", "-m", "right merge"]);

        let left_tip = git_text(&repo, &["rev-parse", "left"]);
        let right_tip = git_text(&repo, &["rev-parse", "right"]);
        let resolver = GitChangeResolver::new(&repo);
        let range = GitChangeRange::new(left_tip, right_tip).unwrap();
        assert_eq!(resolver.resolve(&range), Err(GitChangeScopeError::AmbiguousMergeBase));

        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn resolver_refuses_missing_revision_and_shallow_repository() {
        let repo = init_repo("missing");
        fs::write(repo.join("README.md"), "base\n").unwrap();
        git(&repo, &["add", "README.md"]);
        git(&repo, &["commit", "--quiet", "-m", "base"]);
        let resolver = GitChangeResolver::new(&repo);
        let missing = GitChangeRange::new("missing-base", "HEAD").unwrap();
        assert!(matches!(resolver.resolve(&missing), Err(GitChangeScopeError::RevisionNotFound { field: "base", .. })));

        let clone = temp_path("shallow");
        let source_url = format!("file://{}", repo.to_string_lossy());
        let clone_output = Command::new("git").args(["clone", "--quiet", "--depth", "1", &source_url, clone.to_str().unwrap()]).output().unwrap();
        assert!(clone_output.status.success(), "clone: {}", String::from_utf8_lossy(&clone_output.stderr));
        let shallow_resolver = GitChangeResolver::new(&clone);
        let same = GitChangeRange::new("HEAD", "HEAD").unwrap();
        assert_eq!(shallow_resolver.resolve(&same), Err(GitChangeScopeError::ShallowRepository));
        fs::remove_dir_all(repo).unwrap();
        fs::remove_dir_all(clone).unwrap();
    }
}
