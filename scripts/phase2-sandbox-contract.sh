#!/usr/bin/env bash
# Reproducible Phase 2 sandbox contract and failure-injection evidence.
# Usage: scripts/phase2-sandbox-contract.sh [OUT_DIR]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$REPO_ROOT/.scratch/phase2-sandbox-$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$OUT"
cd "$REPO_ROOT"

cargo test -p erf-sandbox -- --nocapture \
  > >(tee "$OUT/sandbox-contract.log") 2>&1
rg -q 'test result: ok\.' "$OUT/sandbox-contract.log"

cat > "$OUT/phase2-sandbox-evidence.md" <<EOF
# Phase 2 sandbox contract evidence

- Commit: \`$(git rev-parse HEAD)\`
- Provider under test: \`rootless-oci-docker-v0\`
- Trust: \`LOCAL_UNATTESTED\`
- Authority: \`SHADOW\` only

## Result

The sandbox contract tests pass for default-deny command construction,
read-only repository and rule-bundle mounts, isolated writable tmpfs,
seccomp/resource controls, stable policy identity, rootless preflight success,
non-rootless rejection, unavailable-provider rejection, and invalid policy
rejection. Provider failures have no ordinary-process fallback.

The rootless provider output is a local fake used only to exercise the
preflight seam. This evidence does not prove Docker daemon, kernel, seccomp or
OCI enforcement and cannot create trusted execution authority.

Full console evidence: \`sandbox-contract.log\`.
EOF

echo "sandbox=PASS evidence=$OUT/phase2-sandbox-evidence.md"
