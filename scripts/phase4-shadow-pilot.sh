#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: phase4-shadow-pilot.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

mapfile -t intents < <(rg --files specs/rule-intent | sort)
[ "${#intents[@]}" -ge 20 ] || { echo "MVP pilot requires at least twenty rule intents, got ${#intents[@]}" >&2; exit 1; }

scripts/authoring-validate.sh specs/standard/*.yaml "${intents[@]}" > "$out_dir/authoring-validation.log"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

for intent in "${intents[@]}"; do
  id="$(basename "$intent" .yaml)"
  compiled="$(scripts/candidate-compile.sh "$intent" "$work/first")"
  candidate="$(printf '%s\n' "$compiled" | sed -n 's/^CANDIDATE //p')"
  [ -n "$candidate" ] || { echo "candidate path missing: $intent" >&2; exit 1; }
  jq -e --arg id "$id" '.intent.id == $id' "$candidate/manifest.json" >/dev/null
  candidate_pack="$out_dir/candidates/$id"
  mkdir -p "$candidate_pack"
  cp -a "$candidate/." "$candidate_pack/"
  if ! scripts/candidate-verify.sh "$candidate" "$out_dir/evidence/$id" > "$out_dir/evidence-$id.log" 2>&1; then
    if rg -q 'VERIFICATION_ABORTED: ruleset load failed' "$out_dir/evidence-$id.log"; then
      echo "[retry] transient PMD ruleset load failure for $id" >> "$out_dir/evidence-$id.log"
      scripts/candidate-verify.sh "$candidate" "$out_dir/evidence/$id" >> "$out_dir/evidence-$id.log" 2>&1
    else
      cat "$out_dir/evidence-$id.log" >&2
      exit 1
    fi
  fi
  bundle="$out_dir/evidence/$id/evidence-bundle.json"
  jq -e '([.fixtures[].category] | unique | sort) == ["bad", "good", "holdout", "near-miss"]' "$bundle" >/dev/null
  jq -e 'all(.fixtures[]; .pass == true)' "$bundle" >/dev/null
  digest="$(jq -r .digest "$candidate/manifest.json")"
  compiled_again="$(scripts/candidate-compile.sh "$intent" "$work/second")"
  candidate_again="$(printf '%s\n' "$compiled_again" | sed -n 's/^CANDIDATE //p')"
  test "$digest" = "$(jq -r .digest "$candidate_again/manifest.json")"
  printf '%s %s\n' "$id" "$digest" >> "$out_dir/digests.txt"
done

cargo build -q -p erf-fake-host -p erf-fake-resolver
pilot_candidate="$out_dir/candidates/corp.java.no-system-exit"
for index in 1 2 3; do
  repo="$out_dir/repositories/repo-$index"
  mkdir -p "$repo/src/main/java"
  printf 'package demo; public final class Demo%d { void run() { System.exit(%d); } }\n' "$index" "$index" > "$repo/src/main/java/Demo$index.java"
  printf '%s\n' '<project><modelVersion>4.0.0</modelVersion></project>' > "$repo/pom.xml"
  cargo run -q -p erf-trusted-ci --example phase3_real_scan -- \
    "$out_dir/shadow-$index" "$repo" "$pilot_candidate" \
    "$repo_root/target/debug/erf-fake-host" \
    "$repo_root/target/debug/erf-fake-resolver" \
    "org/ruleops-shadow-$index" "phase4-shadow-commit-$index" \
    > "$out_dir/shadow-$index.log" 2>&1
  report="$out_dir/shadow-$index/phase3-real-scan.md"
  [ -s "$report" ] || { echo "missing shadow report: $report" >&2; exit 1; }
  rg -q '^Passed 7/7 checks\.' "$report"
  rg -q "^- Repository:.*org/ruleops-shadow-$index.*$" "$report"
done

cat > "$out_dir/phase4-shadow-pilot.md" <<EOF
# Phase 4 Shadow Pilot Evidence

- Commit: \`$(git rev-parse HEAD)\`
- Trust: \`CI_TRUSTED\` inside the handoff harness; pilot authority remains \`SHADOW\`
- RuleIntents: \`${#intents[@]}\`
- Shadow repositories: 3

| Check | Evidence | Result |
| --- | --- | --- |
| Twenty governed Standard/RuleIntent cross-references | \`authoring-validation.log\` | PASS |
| Twenty Candidate -> EvidenceBundle paths | \`evidence/*/evidence-bundle.json\` | PASS |
| Four fixture classes for every candidate | \`evidence/*/evidence-bundle.json\` | PASS |
| Deterministic recompilation for every candidate | \`digests.txt\` | PASS |
| Shadow repository 1 | \`shadow-1/phase3-real-scan.md\` | PASS |
| Shadow repository 2 | \`shadow-2/phase3-real-scan.md\` | PASS |
| Shadow repository 3 | \`shadow-3/phase3-real-scan.md\` | PASS |
| Crash remains INCONCLUSIVE / ActionRequired | each shadow report, 7/7 | PASS |

The three repository identities and commits are distinct. This is qualification
evidence only: protected-branch authority is not enabled by this script.
EOF

echo "shadow-pilot=PASS evidence=$out_dir/phase4-shadow-pilot.md"
