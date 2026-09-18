#!/usr/bin/env bash
# Reproducible Phase 2 independent acceptance evidence.
# Usage: scripts/phase2-independent-acceptance.sh [OUT_DIR]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$REPO_ROOT/.scratch/phase2-acceptance-$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$OUT"

cd "$REPO_ROOT"
cargo build --quiet -p erf-conformance -p erf-fake-host
target/debug/erf-conformance \
  --host target/debug/erf-fake-host \
  --cancel-window-ms 150 \
  > >(tee "$OUT/fake-host-acceptance.log") 2>&1

rg -q '^16/16 checks passed$' "$OUT/fake-host-acceptance.log"

cat > "$OUT/phase2-acceptance.md" <<EOF
# Phase 2 independent acceptance

- Commit: \`$(git rev-parse HEAD)\`
- Suite: \`$(sed -n 's/^suite version //p' "$OUT/fake-host-acceptance.log" | tail -n 1)\`
- Host: reference Fake Host (fresh process per protocol check)
- Trust: \`LOCAL_UNATTESTED\`
- Authority: \`SHADOW\` only

## Result

All 16 checks passed, including protocol failure injection and the independent
local Gate checks for typed location normalization, canonical Finding ordering
and deduplication, message-independent SemanticResultDigest, orthogonal
Baseline/Exception decisions, malformed observations, and incomplete execution.

The Java PMD and SpotBugs representative paths remain separately exercised by
the Rust client integration tests; the local Gate oracle consumes their shared
typed BackendObservation contract and does not import either engine.

Full console evidence: \`fake-host-acceptance.log\`.
EOF

echo "acceptance=PASS checks=16 evidence=$OUT/phase2-acceptance.md"
