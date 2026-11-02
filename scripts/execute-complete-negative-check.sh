#!/usr/bin/env bash
# Cross-language ExecuteComplete negative conformance for the reference hosts.
# Usage: scripts/execute-complete-negative-check.sh [OUT_DIR]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$REPO_ROOT/.scratch/execute-complete-negative-$(date -u +%Y%m%dT%H%M%SZ)}"
case "$OUT" in
  /*) ;;
  *) OUT="$REPO_ROOT/$OUT" ;;
esac
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd -P)"
cd "$REPO_ROOT"

# The repository's hermetic toolchain is available in the parent workspace in
# the acceptance environment.  Keep the script usable with a normal cargo
# installation as well.
if [[ -f "$REPO_ROOT/../toolchain/ruleops-env.sh" ]]; then
  # shellcheck disable=SC1091
  source "$REPO_ROOT/../toolchain/ruleops-env.sh"
fi
command -v cargo >/dev/null 2>&1 || {
  echo "cargo is required for ExecuteComplete negative conformance" >&2
  exit 2
}
command -v node >/dev/null 2>&1 || {
  echo "node is required for ExecuteComplete negative conformance" >&2
  exit 2
}

# Do not let a caller's injected fixture leak into the positive baseline.
unset FAKE_STATUS_ID_MODE FAKE_BAD_UNITS_REPORTED \
  FAKE_BAD_OBSERVATION_COUNT FAKE_BAD_ATTEMPT_DIGEST

# First run the normal cross-language acceptance.  Its tampering check covers
# mismatched units_reported, observation_count and attempt_digest for both
# reference hosts.
positive_out="$OUT/positive"
scripts/phase2-independent-acceptance.sh "$positive_out" >"$OUT/positive-run.log" 2>&1
for acceptance_log in "$positive_out/fake-host-acceptance.log" "$positive_out/node-fake-acceptance.log"; do
  rg -q '^PASS execute_attempt_accounting_rejects_tampering .*units_reported.*observation_count.*attempt_digest' "$acceptance_log" || {
    echo "missing ExecuteComplete accounting tamper evidence in $acceptance_log" >&2
    exit 1
  }
done

run_status_case() {
  local label="$1"
  local mode="$2"
  shift 2
  local log="$OUT/${label}-${mode}.log"
  local status

  set +e
  FAKE_STATUS_ID_MODE="$mode" target/debug/erf-conformance "$@" >"$log" 2>&1
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "$label accepted malformed status-id mode=$mode" >&2
    return 1
  fi
  rg -q 'FAIL execute_round_trip_completeness|host violated the protocol' "$log" || {
    echo "$label did not fail closed on status-id mode=$mode" >&2
    return 1
  }
  case "$mode" in
    duplicate)
      rg -q '"unit-1", "unit-1"' "$log" || return 1
      ;;
    unknown)
      rg -q 'unit-1-unknown' "$log" || return 1
      ;;
    blank)
      rg -q 'executed \["",' "$log" || return 1
      ;;
  esac
  printf 'PASS %s status-id mode=%s\n' "$label" "$mode"
}

for mode in duplicate unknown blank; do
  run_status_case rust-fake "$mode" \
    --host target/debug/erf-fake-host \
    --resolver target/debug/erf-fake-resolver \
    --cancel-window-ms 150
  run_status_case node-fake "$mode" \
    --host node \
    --host-arg "$REPO_ROOT/backend-hosts/node-fake/host.js" \
    --resolver node \
    --resolver-arg "$REPO_ROOT/backend-hosts/node-fake/resolver.js" \
    --cancel-window-ms 150
done | tee "$OUT/status-id-summary.log"

cat >"$OUT/execute-complete-negative.md" <<'EOF'
# ExecuteComplete cross-language negative conformance

- Rust Fake and Node Fake: duplicate, unknown, and blank `UnitStatus.unit_id`
  each failed the completeness check.
- Both hosts: mismatched `units_reported`, `observation_count`, and
  `attempt_digest` were rejected by the existing ExecuteStream accounting
  boundary.
- Protocol changes: none; fixtures reuse existing ExecuteEvent and
  ExecuteComplete fields.
- Trust/authority: none; reference-host test fixtures only.

Evidence: `status-id-summary.log`, `positive/fake-host-acceptance.log`, and
`positive/node-fake-acceptance.log`.
EOF

echo "execute-complete-negative=PASS evidence=$OUT/execute-complete-negative.md"
