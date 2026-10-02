#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/server-persistence-check}"
JOURNAL="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/RuleOpsJournal.java"
FILE_JOURNAL="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/FileRuleOpsJournal.java"
STREAM_RECOVERY="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/StreamBoundedJournalRecovery.java"
STORE="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java"
MAIN="$REPO_ROOT/control-plane/server/src/main/java/io/ruleops/server/RuleOpsServerMain.java"
TEST="$REPO_ROOT/control-plane/server/src/test/java/io/ruleops/server/RuleOpsJournalTest.java"
ADR="$REPO_ROOT/docs/adr/0138-append-only-governance-journal.md"
STREAM_ADR="$REPO_ROOT/docs/adr/0153-stream-bounded-journal-recovery.md"
mkdir -p "$OUT_DIR"
TEST_LOG="$OUT_DIR/server-persistence-tests.log"
STATIC_LOG="$OUT_DIR/static-check.log"
TEST_REPORT="$REPO_ROOT/control-plane/server/target/surefire-reports/TEST-io.ruleops.server.RuleOpsJournalTest.xml"
TEST_SENTINEL="$OUT_DIR/maven-started"
static_failures=()

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

for input in "$JOURNAL" "$FILE_JOURNAL" "$STREAM_RECOVERY" "$STORE" "$MAIN" "$TEST" "$ADR" "$STREAM_ADR"; do
  record_input "$input"
done
journal_digest="$(digest_or_missing "$JOURNAL")"
file_journal_digest="$(digest_or_missing "$FILE_JOURNAL")"
stream_recovery_digest="$(digest_or_missing "$STREAM_RECOVERY")"
store_digest="$(digest_or_missing "$STORE")"
main_digest="$(digest_or_missing "$MAIN")"
test_digest="$(digest_or_missing "$TEST")"
adr_digest="$(digest_or_missing "$ADR")"
stream_adr_digest="$(digest_or_missing "$STREAM_ADR")"
static_status=STATIC_ONLY
behavior_status=BLOCKED
behavior_result="not run"
test_command="(cd $REPO_ROOT && mvn -pl control-plane/server -am -Dtest=RuleOpsJournalTest -Dsurefire.failIfNoSpecifiedTests=false test)"
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

required=(
  'interface RuleOpsJournal'
  'class FileRuleOpsJournal'
  'class StreamBoundedJournalRecovery'
  'CodingErrorAction.REPORT'
  'MAX_JOURNAL_BYTES'
  'journal entry is not valid UTF-8'
  'JOURNAL_CORRUPT'
  'JOURNAL_CHAIN_INVALID'
  'JOURNAL_DIGEST_INVALID'
  'journal.append(entry)'
  'new FileRuleOpsJournal(Path.of(args[1]))'
  'fileJournalRebuildsResourcesAndAuditAfterRestart'
  'invalidUtf8JournalFailsClosedWithoutReplacement'
  'trailingJsonTokensFailClosed'
  'appendRejectsFourGiBJournalLimit'
  'appendFailureDoesNotCommitAnInMemoryMutation'
)
for needle in "${required[@]}"; do
  check_static "$needle" "$JOURNAL" "$FILE_JOURNAL" "$STREAM_RECOVERY" "$STORE" "$MAIN" "$TEST" "$ADR" "$STREAM_ADR"
done

write_report() {
  cat > "$OUT_DIR/server-persistence.md" <<EOF
# RuleOps Server Persistence Evidence

- evidenceSchema: feature-evidence-v1
- feature: ruleops-server-persistence
- status: $behavior_status
- generatedAt: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- inputIdentity:
  - journalPort: control-plane/server/src/main/java/io/ruleops/server/RuleOpsJournal.java (sha256:$journal_digest)
  - implementation: control-plane/server/src/main/java/io/ruleops/server/FileRuleOpsJournal.java (sha256:$file_journal_digest)
  - streamRecovery: control-plane/server/src/main/java/io/ruleops/server/StreamBoundedJournalRecovery.java (sha256:$stream_recovery_digest)
  - store: control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java (sha256:$store_digest)
  - entrypoint: control-plane/server/src/main/java/io/ruleops/server/RuleOpsServerMain.java (sha256:$main_digest)
  - tests: control-plane/server/src/test/java/io/ruleops/server/RuleOpsJournalTest.java (sha256:$test_digest)
  - decision: docs/adr/0138-append-only-governance-journal.md (sha256:$adr_digest)
  - streamDecision: docs/adr/0153-stream-bounded-journal-recovery.md (sha256:$stream_adr_digest)
- commands:
  - static: each repository-local rg invocation is recorded verbatim in $STATIC_LOG
  - behavior: $test_command
- logs:
  - static: $STATIC_LOG
  - behavior: $TEST_LOG
- testReport: $TEST_REPORT

| Check | Evidence | Status |
| --- | --- | --- |
| Journal adapter, bounded stream recovery, strict UTF-8 and append ordering exist | source/test rg assertions | $static_status |
| Restart recovery, truncation, empty/oversized/invalid UTF-8/4 GiB limits and append-before-mutation failures | $test_command | $behavior_status |

- staticResult: ${static_failures[*]:-all assertions passed}
- behaviorResult: $behavior_result
- keyNegativeFixtures: truncatedJournalFailsClosedInsteadOfSilentlyDroppingTheTail; invalidUtf8JournalFailsClosedWithoutReplacement; trailingJsonTokensFailClosed; oversizedJournalEntryFailsClosedBeforeUnboundedGrowth; fourGiBJournalLimitIsCheckedWithSparseFixture; appendRejectsFourGiBJournalLimit; emptyJournalLineFailsClosed; appendFailureDoesNotCommitAnInMemoryMutation
- runtimeEntry: RuleOpsServerMain [port] [journal-path]
- decision: docs/adr/0138-append-only-governance-journal.md
- streamDecision: docs/adr/0153-stream-bounded-journal-recovery.md
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
: >"$TEST_SENTINEL"

if (cd "$REPO_ROOT" && mvn -pl control-plane/server -am -Dtest=RuleOpsJournalTest -Dsurefire.failIfNoSpecifiedTests=false test) >"$TEST_LOG" 2>&1; then
  behavior_status=PASS
  behavior_result="exit=0; $(rg -m1 'Tests run:' "$TEST_LOG" || true)"
else
  behavior_status=BLOCKED
  behavior_result="maven test failed; see $TEST_LOG"
  write_report
  cat "$TEST_LOG" >&2
  exit 1
fi

if [ ! -s "$TEST_REPORT" ] || [ ! "$TEST_REPORT" -nt "$TEST_SENTINEL" ]; then
  behavior_status=BLOCKED
  behavior_result="maven reported success but no fresh Surefire report was produced: $TEST_REPORT"
  write_report
  cat "$TEST_LOG" >&2
  exit 1
fi
tests_run="$(sed -n 's/.*tests="\([0-9][0-9]*\)".*/\1/p' "$TEST_REPORT" | head -1)"
if ! [[ "$tests_run" =~ ^[0-9]+$ ]] || [ "$tests_run" -eq 0 ]; then
  behavior_status=BLOCKED
  behavior_result="fresh Surefire report ran zero tests: $TEST_REPORT"
  write_report
  cat "$TEST_LOG" >&2
  exit 1
fi
skipped="$(sed -n 's/.*skipped="\([0-9][0-9]*\)".*/\1/p' "$TEST_REPORT" | head -1)"
if [ "$skipped" != "0" ] || rg -q '<skipped' "$TEST_REPORT"; then
  behavior_status=BLOCKED
  behavior_result="fresh Surefire report contains skipped test cases: $TEST_REPORT"
  write_report
  cat "$TEST_LOG" >&2
  exit 1
fi
for fixture in \
  'fileJournalRebuildsResourcesAndAuditAfterRestart' \
  'truncatedJournalFailsClosedInsteadOfSilentlyDroppingTheTail' \
  'invalidUtf8JournalFailsClosedWithoutReplacement' \
  'trailingJsonTokensFailClosed' \
  'appendRejectsFourGiBJournalLimit' \
  'oversizedJournalEntryFailsClosedBeforeUnboundedGrowth' \
  'fourGiBJournalLimitIsCheckedWithSparseFixture' \
  'emptyJournalLineFailsClosed' \
  'appendFailureDoesNotCommitAnInMemoryMutation'; do
  if ! rg -F -q -- "name=\"$fixture\"" "$TEST_REPORT"; then
    behavior_status=BLOCKED
    behavior_result="fresh Surefire report omitted required test fixture: $fixture"
    write_report
    cat "$TEST_LOG" >&2
    exit 1
  fi
done
behavior_result="exit=0; tests=$tests_run; required recovery fixtures present in $TEST_REPORT"

write_report
echo "server-persistence=PASS evidence=$OUT_DIR/server-persistence.md evidenceLevel=EXECUTABLE"
