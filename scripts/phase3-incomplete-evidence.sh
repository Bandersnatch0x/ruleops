#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="$REPO_ROOT/.scratch/phase3-incomplete-$STAMP"
REPEAT_REPO="$EVIDENCE_DIR/repo"
CLI="$REPO_ROOT/target/debug/rf-cli"
RESOLVER="$REPO_ROOT/target/debug/erf-fake-resolver"
HOST="$REPO_ROOT/target/debug/erf-fake-host"

mkdir -p "$REPEAT_REPO/src/main/java"
printf '%s\n' 'public class IncompleteSample { void run() { } }' > "$REPEAT_REPO/src/main/java/IncompleteSample.java"
cargo build --quiet -p rf-cli -p erf-fake-host -p erf-fake-resolver > "$EVIDENCE_DIR/build.txt" 2>&1

run_scan() {
  local name="$1"
  shift
  local output="$EVIDENCE_DIR/$name.txt"
  local status=0
  "$@" > "$output" 2>&1 || status=$?
  if [ "$status" -ne 3 ]; then
    echo "$name exited with $status; expected structured INCONCLUSIVE exit 3" >&2
    sed -n '1,30p' "$output" >&2
    exit 1
  fi
  return 0
}

run_scan crash env FAKE_CRASH_ON_EXECUTE=1 "$CLI" scan \
  --repo-root "$REPEAT_REPO" --rule-id corp.java.no-system-exit --resolver "$RESOLVER" --host "$HOST"
run_scan failed-unit env FAKE_FAIL_UNIT=default:corp.java.no-system-exit "$CLI" scan \
  --repo-root "$REPEAT_REPO" --rule-id corp.java.no-system-exit --resolver "$RESOLVER" --host "$HOST"

rg -q 'gate=Inconclusive' "$EVIDENCE_DIR/crash.txt"
rg -q 'planned=1 executed=0 failed=0' "$EVIDENCE_DIR/crash.txt"
rg -q 'missing: 1' "$EVIDENCE_DIR/crash.txt"
rg -q 'gate=Inconclusive' "$EVIDENCE_DIR/failed-unit.txt"
rg -q 'planned=1 executed=0 failed=1' "$EVIDENCE_DIR/failed-unit.txt"
rg -q 'failed: 1' "$EVIDENCE_DIR/failed-unit.txt"

{
  printf '%s\n' '# Phase 3 incomplete execution evidence'
  printf '\n%s\n' '- Crash injection: `FAKE_CRASH_ON_EXECUTE=1` -> exit 3, Gate `Inconclusive`, planned unit missing.'
  printf '%s\n' '- Failed-unit injection: `FAKE_FAIL_UNIT=default:corp.java.no-system-exit` -> exit 3, Gate `Inconclusive`, failed unit accounted.'
  printf '%s\n\n' '- No partial findings were committed in either case.'
  printf '%s\n' '## Crash result'
  sed -n '/^planned=/p;/^findings=/p;/^gate=/p' "$EVIDENCE_DIR/crash.txt"
  printf '%s\n' '## Failed-unit result'
  sed -n '/^planned=/p;/^findings=/p;/^gate=/p' "$EVIDENCE_DIR/failed-unit.txt"
} > "$EVIDENCE_DIR/phase3-incomplete.md"

printf 'incomplete-execution=PASS evidence=%s\n' "$EVIDENCE_DIR/phase3-incomplete.md"
