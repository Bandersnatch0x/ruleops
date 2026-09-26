#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/affected-scope-check}"
mkdir -p "$OUT_DIR"
cd "$REPO_ROOT"

for file in \
  runtime/affected-scope/Cargo.toml \
  runtime/affected-scope/src/lib.rs \
  docs/adr/0146-java-affected-scope-closure.md; do
  test -s "$file"
done

rg -F -q 'pub struct AffectedScopeResolver' runtime/affected-scope/src/lib.rs
rg -F -q 'pub struct AffectedScope' runtime/affected-scope/src/lib.rs
rg -F -q 'PmdTypeResolution' runtime/affected-scope/src/lib.rs
rg -F -q 'SpotBugsBytecode' runtime/affected-scope/src/lib.rs
rg -F -q 'GeneratedSourcesUncertain' runtime/affected-scope/src/lib.rs

cargo test --quiet -p erf-affected-scope >"$OUT_DIR/tests.log"

cat >"$OUT_DIR/affected-scope.md" <<EOF
# Java AffectedScope Evidence

- input: explicit #103 ChangedSet; no second Git parser
- graph: bounded static Maven/Gradle metadata and reverse dependency closure
- backend closures: PMD AST FILE_CANDIDATE, PMD type resolution MODULE, SpotBugs bytecode MODULE
- fail-closed widening: shared build/config, lockfile, dynamic graph, generated source uncertainty and unmapped paths -> FULL
- digest: versioned canonical scope bytes bind ChangedSet digest, workspace graph digest, backend scopes and reasons
- tests: $OUT_DIR/tests.log
- decision: docs/adr/0146-java-affected-scope-closure.md
EOF
echo "affected-scope=PASS evidence=$OUT_DIR/affected-scope.md"
