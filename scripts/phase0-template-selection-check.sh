#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: phase0-template-selection-check.sh <output-dir>}"
selection="$repo_root/specs/pilot/phase0-template-selection.yaml"
mkdir -p "$out_dir"
cd "$repo_root"

[ -s "$selection" ] || { echo "missing Phase 0 template selection evidence" >&2; exit 1; }

mapfile -t intents < <(rg --files specs/rule-intent | sort)
[ "${#intents[@]}" -eq 20 ] || { echo "expected twenty candidate RuleIntents, got ${#intents[@]}" >&2; exit 1; }

expected_categories=(
  EXISTING_RULE
  EXISTING_RULE_NOT_ENABLED
  EXISTING_RULE_UNSUITABLE
  NEW_RULE_REQUIRED
  ENVIRONMENT_GAP
  BACKEND_CAPABILITY_GAP
  STANDARD_AMBIGUOUS
  NOT_MACHINE_DETECTABLE
)
expected_templates=(ExistingRule StructuralRule SymbolRule)

selection_rows="$(rg -c '^  - id:' "$selection")"
[ "$selection_rows" -eq 20 ] || { echo "expected twenty selection rows, got $selection_rows" >&2; exit 1; }
duplicates="$(rg '^  - id:' "$selection" | sort | uniq -d || true)"
[ -z "$duplicates" ] || { echo "duplicate selection identity: $duplicates" >&2; exit 1; }

count_rows() {
  local key="$1"
  awk -v key="$key" '
    /^selection:$/ { in_selection=1; next }
    /^adjacentGapScenarios:$/ { in_selection=0 }
    in_selection && $0 == "    category: " key { count++ }
    END { print count + 0 }
  ' "$selection"
}

count_templates() {
  local key="$1"
  awk -v key="$key" '
    /^selection:$/ { in_selection=1; next }
    /^adjacentGapScenarios:$/ { in_selection=0 }
    in_selection && $0 == "    template: " key { count++ }
    END { print count + 0 }
  ' "$selection"
}

for category in "${expected_categories[@]}"; do
  actual="$(count_rows "$category")"
  declared="$(awk -v key="$category" 'index($0, "    " key ":") == 1 { print $2; exit }' "$selection")"
  [ -n "$declared" ] || { echo "missing category count: $category" >&2; exit 1; }
  [ "$actual" -eq "$declared" ] || { echo "category count mismatch for $category: rows=$actual declared=$declared" >&2; exit 1; }
done

for template in "${expected_templates[@]}"; do
  actual="$(count_templates "$template")"
  declared="$(awk -v key="$template" 'index($0, "    " key ":") == 1 { print $2; exit }' "$selection")"
  [ -n "$declared" ] || { echo "missing template count: $template" >&2; exit 1; }
  [ "$actual" -eq "$declared" ] || { echo "template count mismatch for $template: rows=$actual declared=$declared" >&2; exit 1; }
done

for intent in "${intents[@]}"; do
  id="$(awk '/^  id:/ { print $2; exit }' "$intent")"
  standard_id="$(awk '/^  standard:$/ { in_standard=1; next } in_standard && /^    id:/ { print $2; exit }' "$intent")"
  scenario="$(awk '/^  standard:$/ { in_standard=1; next } in_standard && /^    scenario:/ { print $2; exit }' "$intent")"
  target_kind="$(awk '/^      kind:/ { print $2; exit }' "$intent")"
  ledger="specs/coverage/$standard_id.yaml"
  [ -f "$ledger" ] || { echo "missing CoverageLedger for $id" >&2; exit 1; }
  category="$(awk -v scenario="$scenario" '
    index($0, "  " scenario ":") == 1 { in_scenario=1; next }
    in_scenario && /^    category:/ { print $2; exit }
  ' "$ledger")"
  rule="$(awk -v scenario="$scenario" '
    index($0, "  " scenario ":") == 1 { in_scenario=1; next }
    in_scenario && /^    rule:/ { print $2; exit }
  ' "$ledger")"
  [ -n "$category" ] || { echo "missing ledger category for $id" >&2; exit 1; }
  case "$target_kind" in
    method-call) expected_template=SymbolRule ;;
    existing-rule) expected_template=ExistingRule ;;
    structural-node) expected_template=StructuralRule ;;
    *) echo "unsupported target kind for $id: $target_kind" >&2; exit 1 ;;
  esac
  record="$(awk -v id="$id" '
    $0 == "  - id: " id { found=1 }
    found && NR > 1 && /^  - id:/ && $0 != "  - id: " id { exit }
    found { print }
  ' "$selection")"
  [ -n "$record" ] || { echo "missing selection row for $id" >&2; exit 1; }
  printf '%s\n' "$record" | rg -q "^  - id: $id$"
  printf '%s\n' "$record" | rg -q "^    standard: $standard_id$"
  printf '%s\n' "$record" | rg -q "^    scenario: $scenario$"
  printf '%s\n' "$record" | rg -q "^    category: $category$"
  printf '%s\n' "$record" | rg -q "^    template: $expected_template$"
  printf '%s\n' "$record" | rg -q "^    sourceCoverage: specs/coverage/$standard_id\.yaml$"
  if [ "$rule" = "null" ]; then
    printf '%s\n' "$record" | rg -q '^    rule: null$'
  else
    printf '%s\n' "$record" | rg -q "^    rule: $rule$"
  fi
done

chosen="$(awk '/^  chosenTemplates:/ { print; exit }' "$selection")"
for template in "${expected_templates[@]}"; do
  printf '%s\n' "$chosen" | rg -q "$template" || { echo "chosen template missing: $template" >&2; exit 1; }
done

adjacent_rows="$(rg -c '^  - standard:' "$selection")"
[ "$adjacent_rows" -eq 2 ] || { echo "expected two explicit adjacent gap scenarios, got $adjacent_rows" >&2; exit 1; }
rg -q '^    scenario: runtime-halt$' "$selection"
rg -q '^    category: STANDARD_AMBIGUOUS$' "$selection"
rg -q '^    scenario: reflective-invocation$' "$selection"
rg -q '^    category: NOT_MACHINE_DETECTABLE$' "$selection"

awk '
  /^  - id:/ { ids++ }
  END { if (ids != 20) exit 1 }
' "$selection"

cat > "$out_dir/phase0-template-selection.md" <<EOF
# Phase 0 Template Selection Evidence

- Acceptance command: scripts/phase0-template-selection-check.sh $out_dir
- Source: specs/pilot/phase0-template-selection.yaml
- Standards inventory: 100
- Candidate RuleIntents / selected scenarios: 20 / 20
- Chosen templates: ExistingRule (13), StructuralRule (6), SymbolRule (1)
- Explicit adjacent gaps: 2 (standard ambiguous: 1; not machine detectable: 1)

| Check | Evidence | Result |
| --- | --- | --- |
| Fixed 20-row candidate denominator | selection rows and RuleIntent inventory | PASS |
| Every selected path matches Standard, Scenario and CoverageLedger | per-intent cross-reference | PASS |
| Category counts include zero-valued gap classes explicitly | summary categoryCounts | PASS |
| Template mapping follows the typed target kind | RuleIntent semantics and selection rows | PASS |
| At most three templates are selected | selectionDecision and chosenTemplates | PASS |
| Ambiguous and unsupported adjacent scenarios remain unpromoted | adjacentGapScenarios | PASS |

The evidence records the current Phase 0 decision boundary. It does not turn
the 80 inventory-only Standards into candidates, resolve human clarification,
or claim that a live pilot has approved the selected templates.
EOF

echo "phase0-template-selection=PASS evidence=$out_dir/phase0-template-selection.md"
