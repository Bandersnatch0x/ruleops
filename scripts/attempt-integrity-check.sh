#!/usr/bin/env bash
# Local, read-only Attempt integrity dashboard acceptance.
# Usage: scripts/attempt-integrity-check.sh [OUT_DIR]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:-$repo_root/.scratch/attempt-integrity-$(date -u +%Y%m%dT%H%M%SZ)}"
case "$out_dir" in
  /*) ;;
  *) out_dir="$repo_root/$out_dir" ;;
esac
mkdir -p "$out_dir"
out_dir="$(cd "$out_dir" && pwd -P)"
cd "$repo_root"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required for local Attempt integrity acceptance" >&2
  exit 2
fi

cargo test -p erf-host-client --lib attempt_integrity --quiet \
  >"$out_dir/attempt-integrity-tests.log" 2>&1

cat >"$out_dir/attempt-integrity.md" <<'EOF'
# Attempt integrity dashboard acceptance

- Scope: read-only planned/status/completion/observation/attempt-digest reconciliation
- Protocol changes: none
- Trust/authority: none (local evidence only)
- Test command: cargo test -p erf-host-client --lib attempt_integrity --quiet
- Result: PASS

The dashboard reports INCONCLUSIVE when ExecuteComplete or the computed
digest is missing, and FAIL_CLOSED for duplicate, missing, or contradictory
planned/status/completion/digest facts. Raw observations are not retained.
EOF

echo "attempt-integrity=PASS evidence=$out_dir/attempt-integrity.md"
