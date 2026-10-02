#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/git-change-scope-check}"
CARGO_MANIFEST="$REPO_ROOT/runtime/git-scope/Cargo.toml"
IMPLEMENTATION="$REPO_ROOT/runtime/git-scope/src/lib.rs"
DECISION="$REPO_ROOT/docs/adr/0145-git-changed-set-scope.md"
CONFLICT_DECISION="$REPO_ROOT/docs/adr/0155-reject-unmerged-worktree-before-changed-set.md"
mkdir -p "$OUT_DIR"
STATIC_LOG="$OUT_DIR/static-check.log"
TEST_LOG="$OUT_DIR/git-change-scope-tests.log"
REAL_CONFLICT_LOG="$OUT_DIR/real-conflict-tests.log"
static_failures=()
static_status=STATIC_ONLY
behavior_status=BLOCKED
behavior_result="not run"
test_command="(cd $REPO_ROOT && cargo test --package erf-git-scope --all-targets)"
real_conflict_command="(cd $REPO_ROOT && cargo test --package erf-git-scope resolver_rejects_real_unmerged_merge_and_rebase_before_resolving_changed_set -- --nocapture)"

record_input() {
  local path="$1"
  [ -s "$path" ] || static_failures+=("missing input file: $path")
}

digest_or_missing() {
  local path="$1"
  if [ -s "$path" ]; then
    sha256sum "$path" | awk '{print $1}'
  else
    printf 'missing'
  fi
}

for input in "$CARGO_MANIFEST" "$IMPLEMENTATION" "$DECISION" "$CONFLICT_DECISION"; do
  record_input "$input"
done
cargo_digest="$(digest_or_missing "$CARGO_MANIFEST")"
implementation_digest="$(digest_or_missing "$IMPLEMENTATION")"
decision_digest="$(digest_or_missing "$DECISION")"
conflict_decision_digest="$(digest_or_missing "$CONFLICT_DECISION")"
: >"$STATIC_LOG"

check_static() {
  local needle="$1"
  shift
  local command_repr="rg -F -n -- $(printf '%q' "$needle")"
  local file
  for file in "$@"; do command_repr+=" $(printf '%q' "$file")"; done
  printf '$ %s\n' "$command_repr" >>"$STATIC_LOG"
  if rg -F -n -- "$needle" "$@" >>"$STATIC_LOG" 2>&1; then
    printf 'status=PASS\n' >>"$STATIC_LOG"
  else
    static_failures+=("missing [$needle] in [$*]")
    printf 'status=BLOCKED\n' >>"$STATIC_LOG"
  fi
}

for needle in \
  'pub struct GitChangeRange' \
  'pub struct ChangedSet' \
  'parse_git_name_status_z' \
  'ShallowRepository' \
  'RejectUnmergedWorktreeBeforeChangedSet' \
  'UnmergedWorktree' \
  'reject_unmerged_worktree' \
  'ls-files' \
  '--diff-filter=U' \
  'CHANGED_SET_VERSION' \
  'resolver_rejects_real_unmerged_merge_and_rebase_before_resolving_changed_set' \
  'resolver_rejects_a_real_multiple_merge_base_range'; do
  check_static "$needle" "$IMPLEMENTATION"
done
check_static 'Status: Accepted' "$CONFLICT_DECISION"

write_report() {
  cat >"$OUT_DIR/git-change-scope.md" <<EOF
# Git ChangedSet Evidence

- evidenceSchema: feature-evidence-v1
- feature: git-changed-set-scope
- status: $behavior_status
- generatedAt: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- inputIdentity:
  - cargo: runtime/git-scope/Cargo.toml (sha256:$cargo_digest)
  - implementation: runtime/git-scope/src/lib.rs (sha256:$implementation_digest)
  - decision: docs/adr/0145-git-changed-set-scope.md (sha256:$decision_digest)
  - conflictDecision: docs/adr/0155-reject-unmerged-worktree-before-changed-set.md (sha256:$conflict_decision_digest)
- checks:
  - name: static-boundary-symbols
    status: $static_status
    evidence: $STATIC_LOG
    command: repository-local rg assertions recorded in $STATIC_LOG
  - name: changed-set-and-real-conflict-tests
    status: $behavior_status
    evidence: $TEST_LOG
    command: $test_command
  - name: real-merge-rebase-conflict-fixture
    status: $behavior_status
    evidence: $REAL_CONFLICT_LOG
    command: $real_conflict_command
- commands:
  - static: each repository-local rg invocation is recorded verbatim in $STATIC_LOG
  - behavior: $test_command
  - realConflict: $real_conflict_command
- logs:
  - static: $STATIC_LOG
  - behavior: $TEST_LOG
  - realConflict: $REAL_CONFLICT_LOG

| Check | Evidence | Status |
| --- | --- | --- |
| Explicit range, merge-base, parser, path and digest boundary symbols | source/ADR rg assertions | $static_status |
| Clean/empty/missing/shallow/parser/multi-base and real conflict behavior | $test_command | $behavior_status |
| Actual merge and rebase conflict rejected before revision/diff | $real_conflict_command | $behavior_status |

- explicit range: base/head are required; no implicit HEAD~1 fallback
- fail-closed: unmerged index/worktree is rejected before revision resolution or diff; shallow repository, missing revision, ambiguous merge-base and parser conflict status remain rejected
- privacy: error identity names only the rejected area, not paths or source content
- staticResult: ${static_failures[*]:-all assertions passed}
- behaviorResult: $behavior_result
- keyFixtures: parser_rejects_conflicts_malformed_scores_and_escape_paths; resolver_uses_merge_base_and_returns_a_stable_empty_set_and_changes; resolver_refuses_missing_revision_and_shallow_repository; resolver_rejects_real_unmerged_merge_and_rebase_before_resolving_changed_set; resolver_rejects_a_real_multiple_merge_base_range
- authority: this read-only scope fact does not authorize scanning, Gate, SCM writes, remote access or production decisions
- decision: docs/adr/0145-git-changed-set-scope.md
- conflictDecision: docs/adr/0155-reject-unmerged-worktree-before-changed-set.md
EOF
}

if ((${#static_failures[@]} > 0)); then
  static_status=BLOCKED
  behavior_result="not run: static input assertion failed"
  printf 'staticFailure=%s\n' "${static_failures[@]}" >>"$STATIC_LOG"
  write_report
  cat "$STATIC_LOG" >&2
  exit 1
fi
printf '%s\n' 'static assertions complete: all passed' >>"$STATIC_LOG"

if (cd "$REPO_ROOT" && cargo test --package erf-git-scope --all-targets) >"$TEST_LOG" 2>&1; then
  if ! rg -q '^running [1-9][0-9]* tests$' "$TEST_LOG" \
    || rg -q 'test result:.*[1-9][0-9]* ignored' "$TEST_LOG"; then
    behavior_result="full Cargo test did not provide nonzero, non-ignored executable evidence"
    write_report
    cat "$TEST_LOG" >&2
    exit 1
  fi
  behavior_status=PASS
  test_summary="$(rg 'test result:' "$TEST_LOG" | paste -sd ';' - || true)"
  behavior_result="full=$test_summary"
  for fixture in \
    'parser_rejects_conflicts_malformed_scores_and_escape_paths' \
    'resolver_uses_merge_base_and_returns_a_stable_empty_set_and_changes' \
    'resolver_refuses_missing_revision_and_shallow_repository' \
    'resolver_rejects_real_unmerged_merge_and_rebase_before_resolving_changed_set' \
    'resolver_rejects_a_real_multiple_merge_base_range'; do
    rg -F -q -- "$fixture ... ok" "$TEST_LOG" || {
      behavior_status=BLOCKED
      behavior_result="required fixture did not pass or was missing from test log: $fixture"
      write_report
      cat "$TEST_LOG" >&2
      exit 1
    }
  done
else
  behavior_result="cargo test failed; see $TEST_LOG"
  write_report
  cat "$TEST_LOG" >&2
  exit 1
fi

if (cd "$REPO_ROOT" && cargo test --package erf-git-scope \
    resolver_rejects_real_unmerged_merge_and_rebase_before_resolving_changed_set -- --nocapture) \
    >"$REAL_CONFLICT_LOG" 2>&1; then
  if ! rg -F -q '1 passed; 0 failed; 0 ignored' "$REAL_CONFLICT_LOG"; then
    behavior_status=BLOCKED
    behavior_result="real conflict fixture command ran without one passing test"
    write_report
    cat "$REAL_CONFLICT_LOG" >&2
    exit 1
  fi
else
  behavior_status=BLOCKED
  behavior_result="real conflict fixture command failed; see $REAL_CONFLICT_LOG"
  write_report
  cat "$REAL_CONFLICT_LOG" >&2
  exit 1
fi

write_report
echo "git-change-scope=PASS evidence=$OUT_DIR/git-change-scope.md evidenceLevel=EXECUTABLE"
