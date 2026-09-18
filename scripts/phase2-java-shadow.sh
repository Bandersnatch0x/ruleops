#!/usr/bin/env bash
# Reproducible Java PMD representative-repository shadow scan.
# Usage: scripts/phase2-java-shadow.sh [OUT_DIR]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$REPO_ROOT/.scratch/phase2-java-shadow-$(date -u +%Y%m%dT%H%M%SZ)}"
case "$OUT" in
  /*) ;;
  *) OUT="$REPO_ROOT/$OUT" ;;
esac
WORK="$OUT/work"
REPO="$WORK/representative-maven-repo"
CANDIDATES="$WORK/candidates"
mkdir -p "$OUT" "$REPO/src/main/java"
trap 'rm -rf "$WORK"' EXIT

cat > "$REPO/pom.xml" <<'EOF'
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <groupId>io.ruleops.acceptance</groupId>
  <artifactId>representative-shadow</artifactId>
  <version>1.0.0</version>
</project>
EOF
cat > "$REPO/src/main/java/ShadowBad.java" <<'EOF'
public final class ShadowBad {
    void terminate() {
        System.exit(1);
    }
}
EOF

cd "$REPO_ROOT"
cargo build --quiet -p rf-cli -p erf-static-resolver
mvn -q -f backend-hosts/java/pom.xml package -DskipTests

compiled="$(scripts/candidate-compile.sh specs/rule-intent/corp.java.no-system-exit.yaml "$CANDIDATES")"
candidate="$(printf '%s\n' "$compiled" | sed -n 's/^CANDIDATE //p')"
[ -n "$candidate" ] || { echo "candidate path missing" >&2; exit 1; }

host_jar="$REPO_ROOT/backend-hosts/java/target/erf-java-host-0.1.0-shaded.jar"
resolver="$REPO_ROOT/target/debug/erf-static-resolver"
cli="$REPO_ROOT/target/debug/rf-cli"

for run in 1 2; do
  "$cli" scan \
    --repo-root "$REPO" \
    --candidate "$candidate" \
    --resolver "$resolver" \
    --resolver-arg --kind \
    --resolver-arg maven \
    --host java \
    --host-arg -jar \
    --host-arg "$host_jar" \
    > "$OUT/run-$run.txt" 2>&1
done

alt_repo="$WORK/representative-existing-rule-repo"
mkdir -p "$alt_repo/src/main/java"
cp "$REPO/pom.xml" "$alt_repo/pom.xml"
cat > "$alt_repo/src/main/java/ExistingRuleBad.java" <<'EOF'
public final class ExistingRuleBad {
    void run() {
        String a = "duplicate";
        String b = "duplicate";
        String c = "duplicate";
        String d = "duplicate";
    }
}
EOF
alt_compiled="$(scripts/candidate-compile.sh specs/rule-intent/corp.java.avoid-duplicate-literals.yaml "$CANDIDATES")"
alt_candidate="$(printf '%s\n' "$alt_compiled" | sed -n 's/^CANDIDATE //p')"
[ -n "$alt_candidate" ] || { echo "existing-rule candidate path missing" >&2; exit 1; }
"$cli" scan \
  --repo-root "$alt_repo" \
  --candidate "$alt_candidate" \
  --resolver "$resolver" \
  --resolver-arg --kind \
  --resolver-arg maven \
  --host java \
  --host-arg -jar \
  --host-arg "$host_jar" \
  > "$OUT/run-existing-rule.txt" 2>&1

for run in 1 2; do
  rg -q '^planned=1 executed=1 failed=0 attempt_failure=None$' "$OUT/run-$run.txt"
  rg -q 'findings=1 trust=LOCAL_UNATTESTED' "$OUT/run-$run.txt"
  rg -q 'gate=Fail completeness=Completeness \{ planned: 1, accounted: 1, failed: 0, missing: 0, observation_errors: 0, complete: true \}' "$OUT/run-$run.txt"
done

digest1="$(sed -n 's/.*semantic_result_digest=\(\[[^]]*\]\).*/\1/p' "$OUT/run-1.txt" | tail -n 1)"
digest2="$(sed -n 's/.*semantic_result_digest=\(\[[^]]*\]\).*/\1/p' "$OUT/run-2.txt" | tail -n 1)"
plan1="$(sed -n 's/^plan_digest=\(.*\)$/\1/p' "$OUT/run-1.txt" | tail -n 1)"
plan2="$(sed -n 's/^plan_digest=\(.*\)$/\1/p' "$OUT/run-2.txt" | tail -n 1)"
[ -n "$digest1" ] && [ "$digest1" = "$digest2" ]
[ -n "$plan1" ] && [ "$plan1" = "$plan2" ]
rg -q '^rule_id=corp.java.avoid-duplicate-literals$' "$OUT/run-existing-rule.txt"
rg -q '^planned=1 executed=1 failed=0 attempt_failure=None$' "$OUT/run-existing-rule.txt"
rg -q 'findings=1 trust=LOCAL_UNATTESTED' "$OUT/run-existing-rule.txt"

cat > "$OUT/phase2-java-shadow.md" <<EOF
# Phase 2 Java representative repository shadow evidence

- Commit: \`$(git rev-parse HEAD)\`
- Resolver: \`erf-maven-static-resolver\` / SAFE_STATIC
- Backend: \`pmd-java\`
- Trust: \`LOCAL_UNATTESTED\`
- Authority: \`SHADOW\` only
- Runs: 2

## Result

Both runs resolved the frozen Maven fixture, negotiated the real Java PMD
Backend Host, accounted one required unit, produced one typed SourceSpan
Finding, and returned a complete non-authoritative Gate result. Plan digest and
SemanticResultDigest were equal across both runs. A separate ExistingRule
candidate was also scanned and its manifest-derived rule identity was preserved
as \`corp.java.avoid-duplicate-literals\` in the ScanPlan and CLI output.

The fixture is intentionally small and local. This proves the resolver/Runner/
Host seam and repeatability for the representative path; it does not prove
coverage of an external production repository, trusted signing, SCM status, or
protected-branch authority.

Run logs: \`run-1.txt\`, \`run-2.txt\`.
EOF

echo "shadow=PASS digest=$digest1 evidence=$OUT/phase2-java-shadow.md"
