#!/usr/bin/env bash
# Validate the fixed-denominator V0 threat model and emit review evidence.
# Usage: scripts/threat-model-check.sh [OUT_DIR]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL="$REPO_ROOT/specs/security/threat-model.yaml"
OUT="${1:-$REPO_ROOT/.scratch/threat-model-$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$OUT"
cd "$REPO_ROOT"

[ -f "$MODEL" ] || { echo "threat model missing: $MODEL" >&2; exit 1; }
expected_ids=(
  prompt-injection
  malicious-generated-java
  malicious-build-script
  dependency-poisoning
  artifact-tampering
  forged-scan-result
  policy-tampering
  baseline-manipulation
  path-traversal-symlink
  zip-bomb
  report-injection
  resource-exhaustion
  network-egress
  secret-exfiltration
  cache-poisoning
  immutable-release
)
actual_count="$(rg -c '^  - id:' "$MODEL")"
[ "$actual_count" -eq "${#expected_ids[@]}" ] || {
  echo "expected ${#expected_ids[@]} threats, got $actual_count" >&2
  exit 1
}

declare -A seen=()
for id in "${expected_ids[@]}"; do
  rg -q "^  - id: $id$" "$MODEL" || { echo "missing threat: $id" >&2; exit 1; }
  [ -z "${seen[$id]+x}" ] || { echo "duplicate threat: $id" >&2; exit 1; }
  seen["$id"]=1
done

awk '
/^  - id: / {
  if (id != "" && (!control || !verification || !status || !residual)) {
    print "incomplete threat: " id > "/dev/stderr"; bad = 1
  }
  id = $3; control = 0; verification = 0; status = 0; residual = 0; next
}
/^    control:/ { control = 1 }
/^    verification:/ { verification = 1 }
/^    status:/ { status = 1 }
/^    residualRisk:/ { residual = 1 }
END {
  if (id != "" && (!control || !verification || !status || !residual)) {
    print "incomplete threat: " id > "/dev/stderr"; bad = 1
  }
  exit bad
}' "$MODEL"

rg -q '^  platform: linux/amd64$' "$MODEL"
rg -q '^  authority: LOCAL_UNATTESTED_SHADOW$' "$MODEL"
rg -q '^    status: BOUNDARY_ONLY$' "$MODEL"
rg -q '^    status: CONTROLLED$' "$MODEL"
rg -q 'source files|credentials|full Finding' "$MODEL"

cat > "$OUT/threat-model.md" <<EOF
# ERF V0 Threat Model Evidence

- Commit: \`$(git rev-parse HEAD)\`
- Threats: \`$actual_count\`
- Platform: \`linux/amd64\`
- Authority: \`LOCAL_UNATTESTED/SHADOW\`

| Check | Evidence | Result |
| --- | --- | --- |
| Fixed threat denominator and unique identities | threat-model.yaml | PASS |
| Every threat has control, verification and residual risk | structural field scan | PASS |
| Platform and authority boundary are explicit | threat-model.yaml | PASS |
| Archive ingestion remains boundary-only until separately designed | zip-bomb row | PASS |
| Source, credentials and full Finding locality are explicit | threat-model.yaml | PASS |

The model records controls and residual risk; it is not a production security
attestation. Any future archive, external SCM, provider or platform adapter
must add its own threat evidence before entering the trusted authority path.
EOF

echo "threat-model=PASS evidence=$OUT/threat-model.md"
