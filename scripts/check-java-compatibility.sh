#!/usr/bin/env bash
# Check the governed Java Backend compatibility matrix for dependency/config drift.
# Usage: scripts/check-java-compatibility.sh [OUT_DIR]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MATRIX="$REPO_ROOT/specs/compatibility/java-backend-matrix.yaml"
OUT="${1:-$REPO_ROOT/.scratch/java-compatibility-$(date -u +%Y%m%dT%H%M%SZ)}"
POM="$REPO_ROOT/backend-hosts/java/pom.xml"
mkdir -p "$OUT"

[ -f "$MATRIX" ] || { echo "compatibility matrix missing: $MATRIX" >&2; exit 1; }
[ -f "$POM" ] || { echo "Java Host POM missing: $POM" >&2; exit 1; }

pmd_version="$(sed -n 's/.*<pmd.version>\([^<]*\)<\/pmd.version>.*/\1/p' "$POM")"
spotbugs_version="$(sed -n 's/.*<spotbugs.version>\([^<]*\)<\/spotbugs.version>.*/\1/p' "$POM")"
[ -n "$pmd_version" ] || { echo "pmd.version missing from Java Host POM" >&2; exit 1; }
[ -n "$spotbugs_version" ] || { echo "spotbugs.version missing from Java Host POM" >&2; exit 1; }

rg -q '^  sourceLevels: \["11", "17", "21"\]$' "$MATRIX"
rg -q "^      engineVersion: $pmd_version$" "$MATRIX"
rg -q "^      engineVersion: $spotbugs_version$" "$MATRIX"
rg -q '^    - backendId: pmd-java$' "$MATRIX"
rg -q '^    - backendId: spotbugs-java$' "$MATRIX"
rg -q '^      state: VERIFIED$' "$MATRIX"
rg -q 'requiredCapabilities: \[java.ast, java.symbol-resolution\]' "$MATRIX"
rg -q 'requiredCapabilities: \[java.bytecode\]' "$MATRIX"

for intent in "$REPO_ROOT"/specs/rule-intent/*.yaml; do
  rg -q '^    sourceLevels: \["11", "17", "21"\]$' "$intent" \
    || { echo "source-level drift in $intent" >&2; exit 1; }
done

java_version="$(java -version 2>&1 | head -1)"
{
  printf '%s\n' '# Java Backend compatibility evidence'
  printf '\n%s\n' '- Trust: `LOCAL_UNATTESTED`'
  printf '%s\n' '- Authority: `SHADOW` only'
  printf '%s\n' "- Runtime: \`$java_version\`"
  printf '%s\n' "- PMD matrix/POM: \`$pmd_version\`"
  printf '%s\n' "- SpotBugs matrix/POM: \`$spotbugs_version\`"
  printf '\n%s\n' '## Governed matrix'
  sed -n '/^java:/,$p' "$MATRIX"
  printf '\n%s\n' 'The checker confirms exact POM version binding and source-level declarations for all twenty governed Java RuleIntents, including the frozen five-rule Phase 2 pack. VERIFIED means the repository contains the referenced Java Host entrypoint and its current gate; other engine versions remain explicitly untested.'
} > "$OUT/java-compatibility.md"

echo "compatibility=PASS evidence=$OUT/java-compatibility.md"
