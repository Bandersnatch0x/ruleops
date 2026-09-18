#!/usr/bin/env bash
# Reproducible Phase 2 Java Local Slice evidence pack.
# Usage: scripts/phase2-java-local-demo.sh OUT_DIR
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-}"
[ -n "$OUT" ] || { echo "usage: $0 OUT_DIR" >&2; exit 2; }
mkdir -p "$OUT"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$ROOT"
exec > >(tee "$OUT/demo.log") 2>&1

echo "ERF Phase 2 Java Local Slice"
echo "commit=$(git rev-parse HEAD)"
echo "trust=LOCAL_UNATTESTED"

# Keep the Phase 2 exit pack immutable as the first five governed standards.
# Later candidate expansion is qualified by the Phase 4 pilot and must not
# silently redefine this already-published local exit boundary.
intents=(
  specs/rule-intent/corp.java.no-system-exit.yaml
  specs/rule-intent/corp.java.avoid-duplicate-literals.yaml
  specs/rule-intent/corp.java.no-forbidden-method.yaml
  specs/rule-intent/corp.java.avoid-object-allocation-in-loop.yaml
  specs/rule-intent/corp.java.utility-class.yaml
)
[ "${#intents[@]}" -eq 5 ] || { echo "Phase 2 exit requires exactly five frozen rule intents, got ${#intents[@]}" >&2; exit 1; }
for intent in "${intents[@]}"; do
  [ -f "$intent" ] || { echo "frozen Phase 2 rule intent missing: $intent" >&2; exit 1; }
  id="$(basename "$intent" .yaml)"
  echo "== $id: compile -> verify (bad/good/holdout/near-miss) =="
  compiled="$(scripts/candidate-compile.sh "$intent" "$WORK/first")"
  candidate="$(printf '%s\n' "$compiled" | sed -n 's/^CANDIDATE //p')"
  [ -n "$candidate" ] || { echo "candidate path missing: $intent" >&2; exit 1; }
  jq -e --arg id "$id" '.intent.id == $id' "$candidate/manifest.json" >/dev/null \
    || { echo "candidate identity does not match intent filename: $intent" >&2; exit 1; }
  candidate_pack="$OUT/candidates/$id"
  mkdir -p "$candidate_pack"
  cp -a "$candidate/." "$candidate_pack/"
  scripts/candidate-verify.sh "$candidate" "$OUT/evidence/$id"
  bundle="$OUT/evidence/$id/evidence-bundle.json"
  jq -e '([.fixtures[].category] | unique | sort) == ["bad", "good", "holdout", "near-miss"]' "$bundle" >/dev/null
  jq -e '[.fixtures[] | select(.category == "bad")] | length > 0' "$bundle" >/dev/null
  jq -e '[.fixtures[] | select(.category == "good")] | length > 0' "$bundle" >/dev/null
  jq -e '[.fixtures[] | select(.category == "holdout")] | length > 0' "$bundle" >/dev/null
  jq -e '[.fixtures[] | select(.category == "near-miss")] | length > 0' "$bundle" >/dev/null
  jq -e 'all(.fixtures[]; .pass == true)' "$bundle" >/dev/null
  digest="$(jq -r .digest "$candidate/manifest.json")"
  printf '%s %s\n' "$id" "$digest" >> "$OUT/digests.txt"
  compiled_again="$(scripts/candidate-compile.sh "$intent" "$WORK/second")"
  candidate_again="$(printf '%s\n' "$compiled_again" | sed -n 's/^CANDIDATE //p')"
  test "$digest" = "$(jq -r .digest "$candidate_again/manifest.json")"
done

echo "== independent conformance acceptance =="
scripts/phase2-independent-acceptance.sh "$OUT/conformance"
echo "== Java compatibility matrix =="
scripts/check-java-compatibility.sh "$OUT/compatibility"
echo "== SAFE_STATIC Maven/Gradle resolver adapters =="
cargo test -q -p erf-static-resolver -- --nocapture \
  > >(tee "$OUT/resolver-contract.log") 2>&1
rg -q 'test result: ok\.' "$OUT/resolver-contract.log"
echo "== sandbox contract and failure injection =="
scripts/phase2-sandbox-contract.sh "$OUT/sandbox"
echo "== representative Java shadow =="
scripts/phase2-java-shadow.sh "$OUT/shadow"

cat > "$OUT/phase2-evidence.md" <<EOF
# Phase 2 Java Local Slice Evidence

- Commit: \`$(git rev-parse HEAD)\`
- Trust: \`LOCAL_UNATTESTED\`
- Mode: \`SHADOW\`
- Rule count: \`${#intents[@]}\`
- Exit state: \`VERIFIED\` (evidence-complete local maturity; no production authority)

## Reproducible checks

| Check | Evidence | Result |
| --- | --- | --- |
| Five independent Candidate → EvidenceBundle paths | \`evidence/*/evidence-bundle.json\` | PASS |
| Fixture categories | each intent has bad/good/holdout/near-miss classes | PASS |
| Repeatability | \`digests.txt\`; every intent compiled twice with equal digest | PASS |
| Independent conformance | \`conformance/phase2-acceptance.md\` | PASS |
| Compatibility matrix | \`compatibility/java-compatibility.md\` | PASS |
| SAFE_STATIC Maven/Gradle adapters | \`resolver-contract.log\` | PASS |
| Sandbox contract and failure injection | \`sandbox/phase2-sandbox-evidence.md\` | PASS |
| Representative repository shadow | \`shadow/phase2-java-shadow.md\` | PASS |
| Production authority | local, unsigned, shadow-only result | PASS |

Holdout and near-miss samples are kept as governed bad/good fixture pairs;
they are not used to tune thresholds after execution. A reviewer can rerun
\`scripts/phase2-java-local-demo.sh OUT_DIR\` and inspect each manifest and
EvidenceBundle independently.
EOF
echo "evidence=$OUT/phase2-evidence.md"
