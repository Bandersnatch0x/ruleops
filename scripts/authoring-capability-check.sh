#!/usr/bin/env bash
# Reproducible V0.1 Authoring Orchestrator and retrieval baseline evidence.
# Usage: scripts/authoring-capability-check.sh [OUT_DIR]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$REPO_ROOT/.scratch/authoring-capability-$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$OUT"
cd "$REPO_ROOT"

mvn -pl control-plane/authoring -am test > >(tee "$OUT/authoring-tests.log") 2>&1
test -s control-plane/authoring/target/surefire-reports/TEST-io.ruleops.authoring.AuthoringOrchestratorTest.xml
test -s control-plane/authoring/target/surefire-reports/TEST-io.ruleops.authoring.AuthoringRetrievalTest.xml

cat > "$OUT/authoring-capability.md" <<EOF
# Authoring capability evidence

- Commit: \`$(git rev-parse HEAD)\`
- Orchestrator: \`authoring-orchestrator/0.1.0\`
- Retrieval baseline: \`hybrid-retrieval/0.1.0\` (lexical BM25; no vector dependency)
- Trust: non-authoritative authoring perimeter

| Check | Evidence | Result |
| --- | --- | --- |
| deterministic grouping and frozen input digest | \`AuthoringOrchestratorTest\` | PASS |
| fixed task coverage and mutually exclusive terminal states | \`AuthoringOrchestratorTest\` | PASS |
| group failure/empty fallback without authoritative mutation | \`AuthoringOrchestratorTest\` | PASS |
| source snapshot/config identity and ACL rejection | \`AuthoringOrchestratorTest\` | PASS |
| exact lookup and metadata filters | \`AuthoringRetrievalTest\` | PASS |
| deterministic BM25 ranking and digest binding | \`AuthoringRetrievalTest\` | PASS |
| lifecycle/ACL exclusion and no-result behavior | \`AuthoringRetrievalTest\` | PASS |

The output type is \`AuthoringSuggestion\` with all trust flags false. It is
not a Finding, EvidenceBundle, RuleRelease or GateDecision and cannot change
rule maturity or execution state. The index accepts governed metadata and
approved summaries only; repository source is not an input to this baseline.
EOF

echo "authoring-capability=PASS evidence=$OUT/authoring-capability.md"
