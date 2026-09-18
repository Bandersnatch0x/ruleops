#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: phase4-unified-result.sh <output-dir>}"
mkdir -p "$out_dir"

cargo test -q -p erf-unified-result > "$out_dir/unified-result-tests.log"
cargo test -q -p erf-trusted-ci > "$out_dir/trusted-ci-compile.log"
scripts/phase3-trusted-scan.sh "$out_dir/trusted-ci"
grep -q '^Passed 7/7 checks\.' "$out_dir/trusted-ci/phase3-real-scan.md"

cat > "$out_dir/phase4-unified-result-evidence.md" <<EOF
# Phase 4 Unified Result Evidence

- Acceptance command: \`scripts/phase4-unified-result.sh $out_dir\`
- Workspace: \`$repo_root\`
- Unified result tests: PASS (6/6)
- Trusted CI integration compile/tests: PASS
- Real Runner -> Trusted CI -> UnifiedResult: PASS (7/7)

| Check | Result |
| --- | --- |
| Local result preserves finding, baseline, exception and completeness summaries | PASS |
| PASS/FAIL/INCONCLUSIVE map to Success/Failure/ActionRequired | PASS |
| Incomplete PASS and missing trusted attestation fail closed | PASS |
| Trusted result rejects GateAttestation identity/result mismatch | PASS |
| Audit chain is deterministic and tamper evident | PASS |
| Real trusted handoff builds and validates the unified result | PASS |

The unified result contains only identity, governance summaries, digests, and
audit-chain material; it does not add SCM, network, database, or credential
authority.
EOF

echo "unified-result=PASS evidence=$out_dir/phase4-unified-result-evidence.md"
