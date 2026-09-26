#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="$REPO_ROOT/.scratch/phase3-timeout-$STAMP"
REPEAT_REPO="$EVIDENCE_DIR/repo"
CLI="$REPO_ROOT/target/debug/rf-cli"
RESOLVER="$REPO_ROOT/target/debug/erf-fake-resolver"
HOST="$REPO_ROOT/target/debug/erf-fake-host"

mkdir -p "$REPEAT_REPO/src/main/java"
printf '%s\n' 'public class TimeoutSample { void run() { } }' > "$REPEAT_REPO/src/main/java/TimeoutSample.java"
printf '%s\n' '<project></project>' > "$REPEAT_REPO/pom.xml"
cargo run --quiet -p erf-project-config -- init \
  --repo-root "$REPEAT_REPO" \
  --non-interactive \
  > "$EVIDENCE_DIR/config-init.txt"
cargo build --quiet -p rf-cli -p erf-fake-host -p erf-fake-resolver > "$EVIDENCE_DIR/build.txt" 2>&1

set +e
FAKE_DELAY_MS=200 "$CLI" scan \
  --repo-root "$REPEAT_REPO" --project-config "$REPEAT_REPO/.ruleops/config.yaml" \
  --rule-id corp.java.no-system-exit --resolver "$RESOLVER" --host "$HOST" --deadline-ms 20 \
  > "$EVIDENCE_DIR/timeout.txt" 2>&1
timeout_status=$?
set -e
if [ "$timeout_status" -ne 3 ]; then
  echo "timeout case exited $timeout_status; expected exit 3" >&2
  sed -n '1,30p' "$EVIDENCE_DIR/timeout.txt" >&2
  exit 1
fi

rg -q 'planned=1 executed=0 failed=0' "$EVIDENCE_DIR/timeout.txt"
rg -q 'attempt_failure=Some\("request deadline of 20 ms exceeded"\)' "$EVIDENCE_DIR/timeout.txt"
rg -q 'gate=Inconclusive' "$EVIDENCE_DIR/timeout.txt"
rg -q 'missing: 1' "$EVIDENCE_DIR/timeout.txt"
rg -q 'findings=0' "$EVIDENCE_DIR/timeout.txt"

{
  printf '%s\n' '# Phase 3 timeout evidence'
  printf '\n%s\n' '- Injection: `FAKE_DELAY_MS=200` with `--deadline-ms 20` -> exit 3.'
  printf '%s\n' '- Expected: frozen planned unit, no committed findings, Gate `Inconclusive`.'
  printf '%s\n\n' '- Observed:'
  sed -n '/^planned=/p;/^findings=/p;/^gate=/p' "$EVIDENCE_DIR/timeout.txt"
} > "$EVIDENCE_DIR/phase3-timeout.md"

printf 'timeout-to-inconclusive=PASS evidence=%s\n' "$EVIDENCE_DIR/phase3-timeout.md"
