#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:?usage: phase4-exit-boundary.sh <output-dir>}"
mkdir -p "$out_dir"
cd "$repo_root"

classpath_file="$(mktemp)"
work="$(mktemp -d)"
trap 'rm -f "$classpath_file"; rm -rf "$work"' EXIT

mvn -q -pl control-plane/ontology -am package -DskipTests dependency:build-classpath -Dmdep.outputFile="$classpath_file"
classpath="$repo_root/control-plane/ontology/target/classes:$(<"$classpath_file")"

java -cp "$classpath" io.ruleops.ontology.OntologyPackCompilerCli \
  specs/ontology/core.ttl specs/ontology/java-extension.ttl phase4-fixed-revision "$out_dir/ontology-first" \
  > "$out_dir/ontology-first.log"
java -cp "$classpath" io.ruleops.ontology.OntologyPackCompilerCli \
  specs/ontology/core.ttl specs/ontology/java-extension.ttl phase4-fixed-revision "$out_dir/ontology-second" \
  > "$out_dir/ontology-second.log"
cmp "$out_dir/ontology-first/ontology-pack.nt" "$out_dir/ontology-second/ontology-pack.nt"
test "$(jq -r .digest "$out_dir/ontology-first/manifest.json")" = "$(jq -r .digest "$out_dir/ontology-second/manifest.json")"
test "$(jq -r .identity "$out_dir/ontology-first/manifest.json")" = "$(jq -r .identity "$out_dir/ontology-second/manifest.json")"

printf '%s\n' \
  '@prefix bad: <https://ruleops.io/ontology/bad#> .' \
  'bad:Unknown a bad:Missing .' > "$work/invalid.ttl"
if java -cp "$classpath" io.ruleops.ontology.OntologyPackCompilerCli \
  specs/ontology/core.ttl "$work/invalid.ttl" invalid-revision "$out_dir/ontology-invalid" \
  > "$out_dir/ontology-invalid.log" 2>&1; then
  echo "invalid ontology unexpectedly compiled" >&2
  exit 1
fi
rg -q 'CLOSED_WORLD|IRI is not declared' "$out_dir/ontology-invalid.log"

cargo test -q -p rf-gate gate_attestation_signs_aggregate_without_finding_content > "$out_dir/privacy-gate.log"
cargo test -q -p erf-unified-result > "$out_dir/privacy-unified-result.log"
cargo test -q -p erf-trusted-context > "$out_dir/trusted-context.log"
rg -q 'pub const TRUSTED_PLATFORM: &str = "linux/amd64"' runtime/conformance/src/lib.rs
rg -q 'RULE_RELEASE_PLATFORM: &str = "linux/amd64"' runtime/release/src/lib.rs
rg -q 'LOCAL_UNATTESTED' runtime/gate/src/lib.rs

cat > "$out_dir/phase4-exit-boundary.md" <<EOF
# Phase 4 Exit Boundary Evidence

- Commit: $(git rev-parse HEAD)
- Ontology revision: phase4-fixed-revision
- Trusted platform: linux/amd64
- Pilot authority: SHADOW; protected-branch enforcement remains disabled

| Check | Evidence | Result |
| --- | --- | --- |
| Fixed Core + Java ontology rebuild has equal canonical bytes | ontology-first/ontology-pack.nt, ontology-second/ontology-pack.nt | PASS |
| Ontology manifests have equal identity and digest | both manifest.json files | PASS |
| Closed-world unknown IRI fails compilation | ontology-invalid.log | PASS |
| GateAttestation default disclosure excludes source/Finding content | privacy-gate.log | PASS |
| UnifiedResult remains digest/summary based | privacy-unified-result.log | PASS |
| Trusted-context platform tests pass | trusted-context.log | PASS |
| Trusted platform declarations are linux/amd64 only | conformance and RuleRelease constants | PASS |

The result is evidence for the current Shadow-only pilot boundary. No source
or full Finding upload path, SCM credential, network, or protected-branch
mutation is introduced.
EOF

echo "exit-boundary=PASS evidence=$out_dir/phase4-exit-boundary.md"
