#!/usr/bin/env bash
# Reproducible Phase 3 real Runner -> Trusted CI handoff.
# Usage: scripts/phase3-trusted-scan.sh OUT_DIR
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-}"
[ -n "$OUT" ] || { echo "usage: $0 OUT_DIR" >&2; exit 2; }
mkdir -p "$OUT/repository/src/main/java"
cd "$ROOT"
exec > >(tee "$OUT/console.log") 2>&1

printf '%s\n' 'package demo; public final class Demo { void run() {} }' \
  > "$OUT/repository/src/main/java/Demo.java"
printf '%s\n' '<project><modelVersion>4.0.0</modelVersion></project>' \
  > "$OUT/repository/pom.xml"

echo "ERF Phase 3 Real Runner Scan"
echo "commit=$(git rev-parse HEAD)"

compiled="$(scripts/candidate-compile.sh \
  specs/rule-intent/corp.java.no-system-exit.yaml "$OUT/candidate-build")"
candidate="$(printf '%s\n' "$compiled" | sed -n 's/^CANDIDATE //p')"
[ -n "$candidate" ] || { echo "candidate path missing" >&2; exit 1; }
cargo build -q -p erf-fake-host -p erf-fake-resolver
cargo run -q -p erf-trusted-ci --example phase3_real_scan -- \
  "$OUT" "$OUT/repository" "$candidate" \
  "$ROOT/target/debug/erf-fake-host" \
  "$ROOT/target/debug/erf-fake-resolver"

report="$OUT/phase3-real-scan.md"
[ -s "$report" ] || { echo "missing real scan report" >&2; exit 1; }
grep -q '^Passed 7/7 checks\.' "$report"
[ "$(grep -c '| PASS |' "$report")" -eq 7 ]
echo "real-scan=PASS evidence=$report"
