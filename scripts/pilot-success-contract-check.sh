#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: pilot-success-contract-check.sh <output-dir> [contract-file]}"
contract_arg="${2:-specs/pilot/pilot-success-contract.yaml}"
case "$contract_arg" in
  /*) contract="$contract_arg" ;;
  *) contract="$repo_root/$contract_arg" ;;
esac
mkdir -p "$out_dir"
cd "$repo_root"

[ -f "$contract" ] || { echo "PilotSuccessContract missing: $contract" >&2; exit 1; }
rg -q '^  id: pilot-success-contract-v0$' "$contract"
rg -q '^  version: 1.0.0$' "$contract"
status_count="$(rg -c '^  status: (PENDING_HUMAN_APPROVAL|APPROVED)$' "$contract" || true)"
status_count="${status_count:-0}"
[ "$status_count" -eq 1 ] || {
  echo "expected exactly one supported PilotSuccessContract status, got $status_count" >&2
  exit 1
}
status="$(sed -n 's/^  status: //p' "$contract")"
rg -q '^  authority: SHADOW$' "$contract"
rg -q '^    standardsInventory: 100$' "$contract"
rg -q '^    candidateRuleIntents: 20$' "$contract"
rg -q '^    shadowRepositories: 3$' "$contract"
rg -q '^      metric: false-pass-count$' "$contract"
rg -q '^      threshold: 0$' "$contract"
rg -q '^      failureBehavior: any-injection-or-incomplete-run-is-not-pass$' "$contract"

required_human_inputs=14
actual_human_inputs="$(rg -c 'REQUIRED_HUMAN_INPUT' "$contract" || true)"
actual_human_inputs="${actual_human_inputs:-0}"

require_field_values() {
  local field="$1"
  local expected="$2"
  awk -v key="$field" -v expected="$expected" '
    function trim(value) {
      sub(/^[[:space:]]+/, "", value)
      sub(/[[:space:]]+$/, "", value)
      return value
    }
    $0 ~ ("^[[:space:]]+" key ":[[:space:]]*") {
      count++
      value = $0
      sub(("^[[:space:]]+" key ":[[:space:]]*"), "", value)
      value = trim(value)
      if (value == "" || value ~ /^#/ || value == "REQUIRED_HUMAN_INPUT" || value == "\"\"" || value == "''" || value == "null" || value == "~") {
        invalid = 1
      }
    }
    END { exit !(count == expected && !invalid) }
  ' "$contract"
}

case "$status" in
  PENDING_HUMAN_APPROVAL)
    [ "$actual_human_inputs" -eq "$required_human_inputs" ] || {
      echo "expected $required_human_inputs required human inputs, got $actual_human_inputs" >&2
      exit 1
    }
    for field in baseline comparison tolerance selection strata comparisonMethod start end decisionOwner acceptanceMethod approvalReference; do
      rg -q "^[[:space:]]+$field: REQUIRED_HUMAN_INPUT$" "$contract" || {
        echo "missing pending human field: $field" >&2
        exit 1
      }
    done
    [ "$(rg -c '^      threshold: REQUIRED_HUMAN_INPUT$' "$contract" || true)" -eq 3 ] || {
      echo "pending contract must contain three human-owned threshold fields" >&2
      exit 1
    }
    ;;
  APPROVED)
    [ "$actual_human_inputs" -eq 0 ] || {
      echo "approved contract still contains REQUIRED_HUMAN_INPUT fields" >&2
      exit 1
    }
    for field in baseline comparison tolerance selection strata comparisonMethod start end decisionOwner acceptanceMethod approvalReference; do
      require_field_values "$field" 1 || {
        echo "approved contract has missing or empty field: $field" >&2
        exit 1
      }
    done
    require_field_values threshold 4 || {
      echo "approved contract must contain three supplied thresholds and the fixed security threshold" >&2
      exit 1
    }
    ;;
  *)
    echo "unsupported PilotSuccessContract status: $status" >&2
    exit 1
    ;;
esac

cat > "$out_dir/pilot-success-contract.md" <<EOF
# Pilot Success Contract Boundary Evidence

- Contract: $contract_arg
- Version: pilot-success-contract-v0 / 1.0.0
- Status: $status
- Authority: Shadow-only
- Scope: 100 Standard inventory, 20 candidate RuleIntents, 3 Shadow repositories
- Required human inputs: $actual_human_inputs
- Security invariant: false-pass threshold is 0; injection or incomplete runs cannot pass

| Check | Evidence | Result |
| --- | --- | --- |
| Contract identity and version are stable | contract metadata | PASS |
| Contract state is supported and explicit | status field | PASS |
| Scope matches the current inventory/candidate/Shadow boundary | scope fields | PASS |
| Security false-pass invariant is zero | security measure | PASS |
| Efficiency, quality, operations, outcomes, sample, window, and adjudication fields are structurally valid | contract fields | PASS |

This evidence validates contract shape and safety invariants. It is not a live
pilot measurement and does not create protected-branch authority.
EOF

echo "pilot-contract=PASS status=$status contract=$contract_arg evidence=$out_dir/pilot-success-contract.md"
