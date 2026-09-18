#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: phase4-coverage-ledger.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

mapfile -t all_standards < <(rg --files specs/standard | sort)
mapfile -t intents < <(rg --files specs/rule-intent | sort)
[ "${#all_standards[@]}" -eq 100 ] || { echo "expected one hundred inventory Standards, got ${#all_standards[@]}" >&2; exit 1; }
[ "${#intents[@]}" -eq 20 ] || { echo "expected twenty candidate RuleIntents, got ${#intents[@]}" >&2; exit 1; }
mapfile -t standards < <(
  for intent in "${intents[@]}"; do
    standard_id="$(awk '/^    id:/ {print $2; exit}' "$intent")"
    printf 'specs/standard/%s.yaml\n' "$standard_id"
  done | sort -u
)
mapfile -t ledgers < <(rg --files specs/coverage | sort)
[ "${#standards[@]}" -eq 20 ] || { echo "expected twenty Standards, got ${#standards[@]}" >&2; exit 1; }
[ "${#ledgers[@]}" -eq 20 ] || { echo "expected twenty CoverageLedgers, got ${#ledgers[@]}" >&2; exit 1; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

for standard in "${standards[@]}"; do
  standard_id="$(awk '/^  id:/ {print $2; exit}' "$standard")"
  ledger="specs/coverage/$standard_id.yaml"
  [ -f "$ledger" ] || { echo "missing ledger for $standard_id" >&2; exit 1; }
  ledger_id="$(awk '/^  id:/ {print $2; exit}' "$ledger")"
  [ "$standard_id" = "$ledger_id" ] || { echo "ledger identity mismatch: $standard_id" >&2; exit 1; }
  sed -n '/^    - id:/p' "$standard" | sed 's/^    - id: //' | sort > "$tmp/standard"
  sed -n '/^coverage:/,$p' "$ledger" | sed -n 's/^  \([^ #][^:]*\):$/\1/p' | sort > "$tmp/ledger"
  diff -u "$tmp/standard" "$tmp/ledger" > "$out_dir/$standard_id-scenarios.diff"
  awk '
    /^  [^ #][^:]*:$/ { scenario = $1; sub(/:$/, "", scenario); next }
    /^    category:/ { category = $2; next }
    /^    rule:/ {
      rule = $2
      if (category ~ /^EXISTING_RULE/ && rule == "null") exit 1
      if (category !~ /^EXISTING_RULE/ && rule != "null") exit 1
    }
  ' "$ledger"
done

mvn -q -pl control-plane/coverage -am test > "$out_dir/coverage-module-tests.log"

cat > "$out_dir/phase4-coverage-ledger.md" <<EOF
# Phase 4 Coverage Ledger Evidence

- Acceptance command: scripts/phase4-coverage-ledger.sh $out_dir
- Standard inventory: 100
- Candidate Standard revisions: 20
- CoverageLedger revisions: 20
- Coverage module tests: PASS
- Pilot authority: Shadow-only

| Check | Evidence | Result |
| --- | --- | --- |
| Every candidate Standard has one matching CoverageLedger | per-standard scenario diffs | PASS |
| Every candidate Standard Scenario is represented exactly once | per-standard scenario diffs | PASS |
| Existing-rule and new-rule-required bindings are explicit | CoverageLedger YAML | PASS |
| Coverage category and rule binding constraints are enforced | script validation and module tests | PASS |
| No coverage row silently upgrades an unbound scenario to covered | CoverageLedger YAML | PASS |

The ledgers are authoring evidence for the current twenty-rule pilot pack within
the 100-Standard inventory. They do not claim executable coverage for the 80
inventory-only records or live pilot outcome metrics.
New-rule-required rows remain partial until their governed release path is
separately promoted.
EOF

echo "coverage=PASS evidence=$out_dir/phase4-coverage-ledger.md"
