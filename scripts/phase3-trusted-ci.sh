#!/usr/bin/env bash
# Reproducible Phase 3 trusted CI closure and status publication harness.
# Usage: scripts/phase3-trusted-ci.sh OUT_DIR
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-}"
[ -n "$OUT" ] || { echo "usage: $0 OUT_DIR" >&2; exit 2; }
mkdir -p "$OUT"
cd "$ROOT"
exec > >(tee "$OUT/console.log") 2>&1

echo "ERF Phase 3 Trusted CI Slice"
echo "commit=$(git rev-parse HEAD)"
echo "platform=linux/amd64"

cargo build -q -p erf-fake-host
ERF_FAKE_HOST="$ROOT/target/debug/erf-fake-host" \
  cargo run -q -p erf-trusted-context --example phase3_trusted_ci -- "$OUT"

report="$OUT/phase3-trusted-ci.md"
[ -s "$report" ] || { echo "missing trusted CI report" >&2; exit 1; }
grep -q '^Passed 17/17 checks\.' "$report"
[ "$(grep -c '| PASS |' "$report")" -eq 17 ]
echo "trusted-ci=PASS evidence=$report"
