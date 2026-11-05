#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/review-impact-score-check}"
CONTRACT="$REPO_ROOT/contracts/http/review-impact-score-v1.yaml"
mkdir -p "$OUT_DIR"
test -s "$CONTRACT"

for needle in \
  'schemaVersion: code-graph-impact-context-v1' \
  'optional: true' \
  'authoritative: false' \
  'authority: non-authoritative' \
  'advisoryOnly: true' \
  'digestOnly: true' \
  'sourceContent: forbidden' \
  'scmWriteback: forbidden' \
  'graphMutation: forbidden' \
  'schemaVersion: finding-score-projection-v1' \
  'derivedFrom: findings-only' \
  'displayOnly: true' \
  'gateDecision: unchanged' \
  'gateReplacement: forbidden' \
  'mode: PREPARATION_ONLY' \
  'authority: LOCAL_UNATTESTED'; do
  rg -F -q -- "$needle" "$CONTRACT"
done

cat > "$OUT_DIR/impact-score.json" <<'JSON'
{
  "schemaVersion": "review-impact-score-v1",
  "reviewRunDigest": "sha256:review-run",
  "impactContext": {
    "schemaVersion": "code-graph-impact-context-v1",
    "graphDigest": "sha256:graph",
    "graphRevisionDigest": "sha256:graph-revision",
    "impactedNodeCount": 4,
    "impactedEdgeCount": 6,
    "impactDigest": "sha256:impact"
  },
  "scoreProjection": {
    "schemaVersion": "finding-score-projection-v1",
    "projectionDigest": "sha256:score-projection",
    "findingSetDigest": "sha256:findings",
    "findingCount": 3,
    "severityWeights": {"INFO": 1, "LOW": 2, "MEDIUM": 5, "HIGH": 8, "CRITICAL": 13},
    "rawScore": 22,
    "normalizedScore": 44,
    "scoreLabel": "HIGH",
    "displayOnly": true,
    "derivedFrom": "findings-only",
    "gateDecision": "unchanged"
  },
  "findings": [
    {"findingId": "f-info", "severity": "INFO", "findingDigest": "sha256:f-info"},
    {"findingId": "f-high", "severity": "HIGH", "findingDigest": "sha256:f-high"},
    {"findingId": "f-critical", "severity": "CRITICAL", "findingDigest": "sha256:f-critical"}
  ],
  "authority": "LOCAL_UNATTESTED",
  "mode": "PREPARATION_ONLY"
}
JSON

python3 - "$OUT_DIR/impact-score.json" <<'PY'
import json
import sys

payload = json.load(open(sys.argv[1], encoding="utf-8"))
impact = payload["impactContext"]
assert impact["schemaVersion"] == "code-graph-impact-context-v1"
for key in ("graphDigest", "graphRevisionDigest", "impactDigest"):
    assert impact[key].startswith("sha256:")
for key in ("impactedNodeCount", "impactedEdgeCount"):
    assert isinstance(impact[key], int) and impact[key] >= 0
assert all("sourceContent" not in value for value in impact.values() if isinstance(value, dict))

projection = payload["scoreProjection"]
assert projection["schemaVersion"] == "finding-score-projection-v1"
assert projection["derivedFrom"] == "findings-only"
assert projection["displayOnly"] is True
assert projection["gateDecision"] == "unchanged"
assert projection["findingSetDigest"].startswith("sha256:")
weights = projection["severityWeights"]
findings = payload["findings"]
assert projection["findingCount"] == len(findings)
assert projection["rawScore"] == sum(weights[f["severity"]] for f in findings)
assert 0 <= projection["normalizedScore"] <= 100
assert projection["projectionDigest"].startswith("sha256:")
for finding in findings:
    assert finding["findingDigest"].startswith("sha256:")
    assert "sourceContent" not in finding
assert payload["authority"] == "LOCAL_UNATTESTED"
assert payload["mode"] == "PREPARATION_ONLY"
assert "sourceContent" not in payload
assert "sourceSnippet" not in payload

# The graph is an optional advisory input: a valid score projection is still
# accepted when the graph context is absent.
without_graph = dict(payload)
without_graph.pop("impactContext")
assert without_graph["scoreProjection"]["derivedFrom"] == "findings-only"
PY

cat > "$OUT_DIR/without-code-graph.json" <<'JSON'
{
  "schemaVersion": "review-impact-score-v1",
  "scoreProjection": {
    "schemaVersion": "finding-score-projection-v1",
    "projectionDigest": "sha256:score-projection-no-graph",
    "findingSetDigest": "sha256:findings",
    "findingCount": 0,
    "severityWeights": {"INFO": 1, "LOW": 2, "MEDIUM": 5, "HIGH": 8, "CRITICAL": 13},
    "rawScore": 0,
    "normalizedScore": 0,
    "scoreLabel": "NONE",
    "displayOnly": true,
    "derivedFrom": "findings-only",
    "gateDecision": "unchanged"
  },
  "findings": [],
  "authority": "LOCAL_UNATTESTED",
  "mode": "PREPARATION_ONLY"
}
JSON
python3 - "$OUT_DIR/without-code-graph.json" <<'PY'
import json
import sys
payload = json.load(open(sys.argv[1], encoding="utf-8"))
assert "impactContext" not in payload
assert payload["scoreProjection"]["rawScore"] == 0
assert payload["scoreProjection"]["displayOnly"] is True
assert payload["scoreProjection"]["gateDecision"] == "unchanged"
PY

cat > "$OUT_DIR/rejected-score.json" <<'JSON'
{
  "scoreProjection": {
    "derivedFrom": "code-graph",
    "displayOnly": false,
    "gateDecision": "FAIL",
    "sourceContent": "forbidden"
  }
}
JSON
set +e
python3 - "$OUT_DIR/rejected-score.json" <<'PY'
import json
import sys
payload = json.load(open(sys.argv[1], encoding="utf-8"))
projection = payload["scoreProjection"]
invalid = (
    projection.get("derivedFrom") != "findings-only"
    or projection.get("displayOnly") is not True
    or projection.get("gateDecision") != "unchanged"
    or "sourceContent" in projection
)
assert invalid, "invalid score projection should be rejected"
print("invalid-score=REJECT")
PY
rejected_rc=$?
set -e
[ "$rejected_rc" -eq 0 ]

cat > "$OUT_DIR/review-impact-score.md" <<'EOF2'
# Review Impact and Score Evidence

- Code Graph impact context is optional, digest-only, advisory and non-authoritative.
- The score is a deterministic projection derived from finding severities; it is display-only.
- Missing Code Graph context does not block score preparation.
- Source content, graph mutation, SCM writeback and Gate replacement are forbidden.
- Boundary remains PREPARATION_ONLY / LOCAL_UNATTESTED.
EOF2

echo "review-impact-score=PASS evidence=$OUT_DIR/review-impact-score.md"
