#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: pilot-metrics-baseline-check.sh <output-dir>}"
baseline="$repo_root/specs/pilot/pilot-metrics-baseline.yaml"
ledger="$repo_root/runtime/pilot-metrics/src/lib.rs"
mkdir -p "$out_dir"
cd "$repo_root"

[ -s "$baseline" ] || { echo "missing pilot metrics baseline" >&2; exit 1; }
rg -q '^kind: PilotMetricsBaseline$' "$baseline"
rg -q '^  status: DEFINITION_ONLY$' "$baseline"
rg -q '^  authority: SHADOW$' "$baseline"
rg -q '^  standardsInventory: 100$' "$baseline"
rg -q '^  candidateRuleIntents: 20$' "$baseline"
rg -q '^  shadowRepositories: 3$' "$baseline"
rg -q '^    window: HUMAN_INPUT_REQUIRED$' "$baseline"
rg -q '^    sample: HUMAN_INPUT_REQUIRED$' "$baseline"
rg -q '^    acceptanceThresholds: HUMAN_INPUT_REQUIRED$' "$baseline"
rg -q '^  disclosure: DIGEST_ONLY$' "$baseline"
rg -q '^  missingDataBehavior: no-data-or-invalid-data-is-not-a-pass$' "$baseline"
rg -q '^  externalTransport: none$' "$baseline"

expected_ids=(
  new-violation-trend
  historical-debt-trend
  complete-rate
  inconclusive-rate
  false-positive-regression
  rollback-count
  active-exception-count
  oldest-active-exception-age
  scan-latency
  resource-consumption
  false-pass-count
)
metric_count="$(rg -c '^    - id:' "$baseline")"
[ "$metric_count" -eq 11 ] || { echo "expected eleven metrics, got $metric_count" >&2; exit 1; }
for metric in "${expected_ids[@]}"; do
  rg -q "^    - id: $metric$" "$baseline" || { echo "missing metric: $metric" >&2; exit 1; }
done
duplicate_ids="$(rg '^    - id:' "$baseline" | awk '{print $3}' | sort | uniq -d || true)"
[ -z "$duplicate_ids" ] || { echo "duplicate metric identity: $duplicate_ids" >&2; exit 1; }

for metric in "${expected_ids[@]}"; do
  record="$(awk -v id="$metric" '
    $0 == "    - id: " id { found=1 }
    found && /^    - id:/ && $0 != "    - id: " id { exit }
    found { print }
  ' "$baseline")"
  [ -n "$record" ] || { echo "missing metric record: $metric" >&2; exit 1; }
  printf '%s\n' "$record" | rg -q '^      record: (run|feedback|governance)$'
  printf '%s\n' "$record" | rg -q '^      field: [a-z][a-z0-9_-]*$'
  printf '%s\n' "$record" | rg -q '^      unit: [a-z][a-z0-9-]*$'
  printf '%s\n' "$record" | rg -q '^      aggregation: [a-z][a-z0-9-]*$'
  printf '%s\n' "$record" | rg -q '^      missingData: INCONCLUSIVE$'
  printf '%s\n' "$record" | rg -q '^      disclosure: DIGEST_ONLY$'
done

for metric in new-violation-trend historical-debt-trend complete-rate inconclusive-rate scan-latency resource-consumption false-pass-count; do
  record="$(awk -v id="$metric" '
    $0 == "    - id: " id { found=1 }
    found && /^    - id:/ && $0 != "    - id: " id { exit }
    found { print }
  ' "$baseline")"
  printf '%s\n' "$record" | rg -q '^      record: run$'
done
record="$(awk '/^    - id: false-positive-regression$/{found=1} found && /^    - id:/ && $0 != "    - id: false-positive-regression"{exit} found{print}' "$baseline")"
printf '%s\n' "$record" | rg -q '^      record: feedback$'
for metric in rollback-count active-exception-count oldest-active-exception-age; do
  record="$(awk -v id="$metric" '
    $0 == "    - id: " id { found=1 }
    found && /^    - id:/ && $0 != "    - id: " id { exit }
    found { print }
  ' "$baseline")"
  printf '%s\n' "$record" | rg -q '^      record: governance$'
done

for field in new_finding_count historical_finding_count complete decision regression_outcome rollback_count active_exception_count oldest_active_exception_age_secs latency_millis peak_memory_bytes; do
  rg -q "$field" "$ledger" || { echo "ledger does not expose baseline field: $field" >&2; exit 1; }
done
rg -q 'ResultTrust' "$ledger" && rg -q 'GateDecision' "$ledger" || { echo "ledger does not expose trust-and-decision boundary" >&2; exit 1; }
! rg -n '^  (threshold|target|acceptance):|https?://|sourcePath|fullFinding|upload' "$baseline"

cat > "$out_dir/pilot-metrics-baseline.md" <<EOF
# Pilot Metrics Baseline Evidence

- Acceptance command: scripts/pilot-metrics-baseline-check.sh $out_dir
- Source: specs/pilot/pilot-metrics-baseline.yaml
- Status: definition-only; no live outcome claimed
- Scope: Java, 100 Standards, 20 candidate RuleIntents, 3 Shadow repositories
- Fixed metric denominator: 11
- Missing data: INCONCLUSIVE / not a PASS
- Disclosure: digest-only local journal

| Check | Evidence | Result |
| --- | --- | --- |
| Fixed eleven-metric denominator and unique identities | baseline metric list | PASS |
| Every metric declares source record, field, unit and aggregation | per-metric schema | PASS |
| Missing-data and disclosure boundaries are fail-closed | per-metric fields | PASS |
| Fields map to the Pilot Metrics ledger | runtime/pilot-metrics/src/lib.rs | PASS |
| Human window, sample and thresholds remain unfilled | HUMAN_INPUT_REQUIRED boundary | PASS |
| No external transport or full Finding claim | baseline boundaries | PASS |

This is a measurement definition baseline. It is not a live pilot result and
does not supply the human decisions still required by PilotSuccessContract.
EOF

echo "pilot-metrics-baseline=PASS evidence=$out_dir/pilot-metrics-baseline.md"
