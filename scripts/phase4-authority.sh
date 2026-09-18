#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: phase4-authority.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

cargo test -q -p erf-authority > "$out_dir/authority-tests.log"
grep -q 'test result: ok\. 6 passed; 0 failed' "$out_dir/authority-tests.log"
grep -q 'pub const TRUSTED_PLATFORM: &str = "linux/amd64"' runtime/authority/src/lib.rs
! rg -n 'Command::new|reqwest|ureq|std::net|std::process' runtime/authority/src

cat > "$out_dir/phase4-authority.md" <<EOF
# Phase 4 Protected-Branch Authority Evidence

- Acceptance command: scripts/phase4-authority.sh $out_dir
- Authority implementation: runtime/authority
- Trusted platform: linux/amd64
- Unit tests: PASS (6/6)
- Pilot authority: SHADOW; protected-branch enforcement remains disabled

| Check | Evidence | Result |
| --- | --- | --- |
| LocalUnattested result is rejected | authority-tests.log | PASS |
| Non-linux/amd64 platform is rejected | authority-tests.log | PASS |
| Complete Trusted PASS maps to AllowMerge | authority-tests.log | PASS |
| Complete Trusted FAIL maps to BlockMerge | authority-tests.log | PASS |
| Complete Trusted INCONCLUSIVE maps to ActionRequired | authority-tests.log | PASS |
| Incomplete PASS fails closed and result digest is stable | authority-tests.log | PASS |

The evaluator is transport-neutral and consumes only a validated UnifiedResult
plus an explicit platform string. It introduces no SCM client, network call,
credential handling, persistence, or repository write.
EOF

echo "authority=PASS evidence=$out_dir/phase4-authority.md"
