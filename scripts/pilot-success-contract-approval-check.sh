#!/usr/bin/env bash
# Regression evidence for the current and opposite Pilot Success Contract states.
# Usage: scripts/pilot-success-contract-approval-check.sh OUT_DIR
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: pilot-success-contract-approval-check.sh <output-dir>}"
current="$repo_root/specs/pilot/pilot-success-contract.yaml"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$out_dir"

run_check() {
  local name="$1"
  local contract="$2"
  "$repo_root/scripts/pilot-success-contract-check.sh" \
    "$out_dir/$name" "$contract" >"$out_dir/$name.log" 2>&1
}

expect_failure() {
  local name="$1"
  local contract="$2"
  if run_check "$name" "$contract"; then
    echo "$name unexpectedly passed" >&2
    exit 1
  fi
}

current_status="$(sed -n 's/^  status: //p' "$current")"
case "$current_status" in
  PENDING_HUMAN_APPROVAL)
    pending="$current"
    approved="$work/approved.yaml"
    sed \
      -e 's/REQUIRED_HUMAN_INPUT/approved-governance-value/g' \
      -e 's/^  status: PENDING_HUMAN_APPROVAL$/  status: APPROVED/' \
      "$pending" >"$approved"
    ;;
  APPROVED)
    approved="$current"
    pending="$work/pending.yaml"
    awk '
      /^    security:/ { in_security = 1 }
      /^    operations:/ { in_security = 0 }
      /^    outcomes:/ { in_security = 0 }
      /^  status: APPROVED$/ { print "  status: PENDING_HUMAN_APPROVAL"; next }
      /^      baseline:/ { print "      baseline: REQUIRED_HUMAN_INPUT"; next }
      /^      comparison:/ { print "      comparison: REQUIRED_HUMAN_INPUT"; next }
      /^      tolerance:/ { print "      tolerance: REQUIRED_HUMAN_INPUT"; next }
      /^      threshold:/ && !in_security { print "      threshold: REQUIRED_HUMAN_INPUT"; next }
      /^    selection:/ { print "    selection: REQUIRED_HUMAN_INPUT"; next }
      /^    strata:/ { print "    strata: REQUIRED_HUMAN_INPUT"; next }
      /^    comparisonMethod:/ { print "    comparisonMethod: REQUIRED_HUMAN_INPUT"; next }
      /^    start:/ { print "    start: REQUIRED_HUMAN_INPUT"; next }
      /^    end:/ { print "    end: REQUIRED_HUMAN_INPUT"; next }
      /^    decisionOwner:/ { print "    decisionOwner: REQUIRED_HUMAN_INPUT"; next }
      /^    acceptanceMethod:/ { print "    acceptanceMethod: REQUIRED_HUMAN_INPUT"; next }
      /^    approvalReference:/ { print "    approvalReference: REQUIRED_HUMAN_INPUT"; next }
      { print }
    ' "$approved" >"$pending"
    ;;
  *)
    echo "unsupported current contract status: $current_status" >&2
    exit 1
    ;;
esac

run_check pending "$pending"
rg -q 'pilot-contract=PASS status=PENDING_HUMAN_APPROVAL' "$out_dir/pending.log"

run_check approved "$approved"
rg -q 'pilot-contract=PASS status=APPROVED' "$out_dir/approved.log"
rg -q 'Status: APPROVED' "$out_dir/approved/pilot-success-contract.md"

cp "$approved" "$work/missing-input.yaml"
sed -i '0,/^      baseline:/s|^      baseline:.*|      baseline: REQUIRED_HUMAN_INPUT|' "$work/missing-input.yaml"
expect_failure missing-input "$work/missing-input.yaml"

cp "$approved" "$work/wrong-authority.yaml"
sed -i 's/^  authority: SHADOW$/  authority: BLOCKING/' "$work/wrong-authority.yaml"
expect_failure wrong-authority "$work/wrong-authority.yaml"

cp "$approved" "$work/wrong-security-threshold.yaml"
sed -i 's/^      threshold: 0$/      threshold: 1/' "$work/wrong-security-threshold.yaml"
expect_failure wrong-security-threshold "$work/wrong-security-threshold.yaml"

cp "$approved" "$work/missing-approval-reference.yaml"
sed -i '/^[[:space:]]*approvalReference:/d' "$work/missing-approval-reference.yaml"
expect_failure missing-approval-reference "$work/missing-approval-reference.yaml"

cp "$approved" "$work/unsupported-status.yaml"
sed -i 's/^  status: APPROVED$/  status: DRAFT/' "$work/unsupported-status.yaml"
expect_failure unsupported-status "$work/unsupported-status.yaml"

cat > "$out_dir/pilot-success-contract-approval.md" <<EOF
# Pilot Success Contract State Validation Evidence

- Current asset: \`specs/pilot/pilot-success-contract.yaml\`
- Current state: \`$current_status\`
- Opposite-state fixture: generated only in a temporary directory for validator testing
- Authority in every accepted state: \`SHADOW\`

| Check | Result |
| --- | --- |
| Current contract is accepted | PASS |
| Opposite contract state is accepted without repository mutation | PASS |
| Missing human input is rejected | PASS |
| Wrong authority is rejected | PASS |
| False-pass threshold change is rejected | PASS |
| Missing approval reference is rejected | PASS |
| Unsupported status is rejected | PASS |

The opposite-state fixture is synthetic validator input, not a human approval or
live pilot result. The repository contract remains Shadow-only.
EOF

echo "pilot-contract-state=PASS evidence=$out_dir/pilot-success-contract-approval.md"
