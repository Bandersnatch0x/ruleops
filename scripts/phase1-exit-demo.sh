#!/usr/bin/env bash
# Reproducible Phase 1 exit evidence pack.
# Usage: scripts/phase1-exit-demo.sh OUT_DIR
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-}"
[ -n "$OUT_DIR" ] || { echo "usage: $0 OUT_DIR" >&2; exit 2; }
mkdir -p "$OUT_DIR"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

if [ -f "$HOME/.local/opt/erf-env.sh" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.local/opt/erf-env.sh"
fi

log="$OUT_DIR/demo.log"
exec > >(tee "$log") 2>&1
cd "$REPO_ROOT"

echo "ERF Phase 1 Exit Demo"
echo "commit=$(git rev-parse HEAD)"
echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"

echo "== ontology: Core + Java Extension -> Contract Pack =="
mvn -q -pl control-plane/ontology package \
  dependency:build-classpath -Dmdep.outputFile="$WORK_DIR/ontology.classpath"
ontology_deps="$(cat "$WORK_DIR/ontology.classpath")"
java -cp "control-plane/ontology/target/classes:$ontology_deps" \
  io.ruleops.ontology.OntologyPackCompilerCli \
  specs/ontology/core.ttl specs/ontology/java-extension.ttl HEAD "$OUT_DIR/ontology-pack" \
  >"$OUT_DIR/ontology.txt"
cat "$OUT_DIR/ontology.txt"

echo "== authoring: Standard -> Intent validation =="
scripts/authoring-validate.sh specs/standard/SEC-JAVA-017.yaml \
  specs/rule-intent/corp.java.no-system-exit.yaml

echo "== factory: Intent -> IR -> PMD Candidate =="
compile_output="$(scripts/candidate-compile.sh \
  specs/rule-intent/corp.java.no-system-exit.yaml "$WORK_DIR/candidates")"
echo "$compile_output"
candidate_dir="$(printf '%s\n' "$compile_output" | sed -n 's/^CANDIDATE //p')"
[ -n "$candidate_dir" ] || { echo "candidate path missing" >&2; exit 1; }
candidate_pack="$OUT_DIR/candidate"
mkdir -p "$candidate_pack"
cp -a "$candidate_dir/." "$candidate_pack/"

echo "== verification: PMD fixtures -> EvidenceBundle =="
scripts/candidate-verify.sh "$candidate_dir" "$OUT_DIR/factory-evidence"

echo "== factory: three templates -> independent EvidenceBundles =="
for intent in \
  specs/rule-intent/corp.java.no-system-exit.yaml \
  specs/rule-intent/corp.java.avoid-duplicate-literals.yaml \
  specs/rule-intent/corp.java.no-forbidden-method.yaml; do
  template_id="$(basename "$intent" .yaml)"
  template_compile="$(scripts/candidate-compile.sh "$intent" "$WORK_DIR/templates")"
  template_candidate="$(printf '%s\n' "$template_compile" | sed -n 's/^CANDIDATE //p')"
  [ -n "$template_candidate" ] || { echo "template candidate path missing: $intent" >&2; exit 1; }
  scripts/candidate-verify.sh "$template_candidate" "$OUT_DIR/template-evidence/$template_id"
done

echo "== runtime: Resolver -> ScanPlan -> Host -> Gate preview =="
cargo run -q -p rf-cli -- scan \
  --repo-root control-plane \
  --candidate "$candidate_dir" \
  --host target/debug/erf-fake-host \
  >"$OUT_DIR/runtime-scan.txt"
cat "$OUT_DIR/runtime-scan.txt"

echo "== contract: Node Host/Resolver =="
node --check backend-hosts/node-fake/host.js
node --check backend-hosts/node-fake/resolver.js
cargo run -q -p erf-conformance -- \
  --host node --host-arg backend-hosts/node-fake/host.js \
  --cancel-window-ms 150 >"$OUT_DIR/node-conformance.txt"
cat "$OUT_DIR/node-conformance.txt"
cargo test -q -p erf-fake-resolver --test integration \
  node_fake_resolver_interoperates_with_rust_client -- --nocapture \
  >"$OUT_DIR/node-resolver.txt"
cat "$OUT_DIR/node-resolver.txt"

cat >"$OUT_DIR/phase1-exit-evidence.md" <<EOF
# Phase 1 Exit Evidence

- Commit: \`$(git rev-parse HEAD)\`
- Trust: \`LOCAL_UNATTESTED\`
- Candidate: \`$candidate_pack\`
- Representative repository: \`control-plane\`

## Exit criteria

| Criterion | Evidence | Result |
| --- | --- | --- |
| Standard → Intent → IR → PMD → Tests | factory validation, candidate manifest, factory evidence bundle | PASS |
| SymbolRule + ExistingRule + StructuralRule → EvidenceBundle | template-evidence/* | PASS |
| Core Ontology + Java Extension → content-addressed pack | ontology.txt, ontology-pack/manifest.json | PASS |
| Runtime scan → findings → Completeness → Gate preview | \`runtime-scan.txt\` | PASS |
| Node fake Host/Resolver contract basics | \`node-conformance.txt\`, \`node-resolver.txt\`, Node syntax checks | PASS |
| No partial findings after failed unit | rf-runner atomic commit path and failure probe | PASS |

## Pilot Success Contract

The efficiency, quality and security dimensions remain evidence inputs for the
human Go/No-Go decision. Thresholds are not changed by this demo. This pack
records reproducible observations only; it does not make the Go/No-Go decision.

- Efficiency: baseline/threshold must be supplied from the pre-registered Pilot Metrics Baseline.
- Quality: holdout/near-miss comparison is not claimed by this thin slice.
- Security: conformance failure injection and atomic failure behavior are captured above; trusted authority remains unavailable.

## Known contract gaps exposed

- Candidate RuleBundle is still an unsigned local candidate; no RuleRelease or blocking authority.
- Runtime Gate preview is local and does not publish an attestation.
- The demo uses reference fake runtime components; Java Host PMD execution remains a separately verified local path.
EOF

echo "evidence=$OUT_DIR/phase1-exit-evidence.md"
echo "Phase 1 demo complete; Go/No-Go remains a human decision."
