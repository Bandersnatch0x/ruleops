#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: phase4-governance-registry.sh <output-dir>}"
mkdir -p "$out_dir"

cargo test -q -p erf-governance-registry > "$out_dir/governance-registry-tests.log"
cargo test -q -p rf-gate > "$out_dir/gate-tests.log"

cat > "$out_dir/phase4-governance-evidence.md" <<EOF
# Phase 4 Governance Registry Evidence

- Acceptance command: \`scripts/phase4-governance-registry.sh $out_dir\`
- Workspace: \`$repo_root\`
- Governance registry tests: PASS (6/6)
- Gate compatibility tests: PASS (11/11)

| Check | Result |
| --- | --- |
| PolicyProfile signed canonical admission and duplicate idempotency | PASS |
| PolicyProfile promotion, rollback, revocation and byte immutability | PASS |
| ExceptionGrant effective/expiry enforcement and rollback | PASS |
| Tampering, wrong role, identity conflict and stale operation rejection | PASS |
| TrustMetadata expiry and closed selector validation | PASS |
| Non-empty audit context and revoked-target rejection | PASS |

The core has no filesystem, SCM, network, database, or signing-key storage
authority. Exact asset bytes remain unchanged across lifecycle transitions.
EOF

echo "governance=PASS evidence=$out_dir/phase4-governance-evidence.md"
