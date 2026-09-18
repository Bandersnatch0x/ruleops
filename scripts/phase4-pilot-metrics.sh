#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: phase4-pilot-metrics.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

cargo test -q -p erf-pilot-metrics > "$out_dir/pilot-metrics-tests.log"
grep -q 'test result: ok\. 8 passed; 0 failed' "$out_dir/pilot-metrics-tests.log"
! rg -n 'Command::new|reqwest|ureq|std::net|std::process' runtime/pilot-metrics/src

cat > "$out_dir/phase4-pilot-metrics.md" <<EOF
# Phase 4 Pilot Measurement Ledger Evidence

- Acceptance command: scripts/phase4-pilot-metrics.sh $out_dir
- Implementation: runtime/pilot-metrics
- Focused tests: PASS (8/8)
- Persistence/transport: external adapters remain out of scope

| Check | Evidence | Result |
| --- | --- | --- |
| Validated UnifiedResult produces digest-only run measurements | pilot-metrics-tests.log | PASS |
| New/historical, completeness, tri-state, latency and resource counters aggregate | pilot-metrics-tests.log | PASS |
| Run trend preserves deterministic insertion order | pilot-metrics-tests.log | PASS |
| False-positive regression and governance snapshots aggregate | pilot-metrics-tests.log | PASS |
| Replay, conflict, audit and invalid-input behavior fail closed | pilot-metrics-tests.log | PASS |
| Digest-only journal round-trips, rejects tampering/truncation, and preserves sequence guards | pilot-metrics-tests.log | PASS |

The ledger and its replayable journal define a measurement seam; this evidence
is not a live pilot measurement. Product outcomes still require an
independently selected pilot and a pre-registered observation window.
EOF

echo "pilot-metrics=PASS evidence=$out_dir/phase4-pilot-metrics.md"
