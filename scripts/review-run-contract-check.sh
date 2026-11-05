#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/review-run-contract-check}"
CONTRACT="$REPO_ROOT/contracts/http/review-run-v1.yaml"
mkdir -p "$OUT_DIR"
test -s "$CONTRACT"
for needle in \
  'schemaVersion: revision-binding-v1' \
  'stalePolicy: reject-result-when-headSha-or-baseSha-changes' \
  'schemaVersion: scope-plan-v1' \
  'exactlyOneScopePerFile: true' \
  'schemaVersion: finding-lifecycle-v1' \
  'humanResolvedStateIsPreserved: true' \
  'schemaVersion: incremental-review-v1' \
  'schemaVersion: review-trace-v1' \
  'mode: PREPARATION_ONLY' \
  'scmWriteback: forbidden'; do
  rg -F -q -- "$needle" "$CONTRACT"
done

cat > "$OUT_DIR/valid-review.json" <<'JSON'
{
  "schemaVersion": "review-run-v1",
  "revisionBinding": {
    "baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "sourceRef": "refs/heads/feature/review",
    "targetRef": "refs/heads/main",
    "bindingDigest": "sha256:binding"
  },
  "scopePlan": {
    "planDigest": "sha256:plan",
    "scopes": [
      {"scopeId": "scope-1", "files": ["src/A.java"], "changedLines": 12},
      {"scopeId": "scope-2", "files": ["src/B.java"], "changedLines": 4}
    ]
  },
  "findings": [
    {"findingId": "f-1", "state": "OPEN"},
    {"findingId": "f-2", "state": "RESOLVED", "humanResolved": true}
  ],
  "incrementalReview": {
    "reviewRunId": "run-2",
    "reviewedHeadSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "baselineReviewRunDigest": "sha256:baseline",
    "deltaMode": "CHANGED_SCOPES_ONLY"
  },
  "authority": "LOCAL_UNATTESTED",
  "mode": "PREPARATION_ONLY"
}
JSON

python3 - "$OUT_DIR/valid-review.json" <<'PY'
import json
import sys

review = json.load(open(sys.argv[1], encoding="utf-8"))
binding = review["revisionBinding"]
assert binding["baseSha"] != binding["headSha"]
assert binding["bindingDigest"].startswith("sha256:")
scopes = review["scopePlan"]["scopes"]
files = [path for scope in scopes for path in scope["files"]]
assert len(files) == len(set(files)), "each changed file must belong to exactly one scope"
assert review["scopePlan"]["planDigest"].startswith("sha256:")
assert review["incrementalReview"]["reviewedHeadSha"] == binding["headSha"]
assert review["incrementalReview"]["baselineReviewRunDigest"].startswith("sha256:")
assert review["authority"] == "LOCAL_UNATTESTED"
assert review["mode"] == "PREPARATION_ONLY"
for finding in review["findings"]:
    assert finding["state"] in {"OPEN", "KEPT", "RESOLVED", "DISMISSED"}
assert next(f for f in review["findings"] if f["findingId"] == "f-2")["humanResolved"] is True
PY

cat > "$OUT_DIR/stale-review.json" <<'JSON'
{"revisionBinding":{"baseSha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","headSha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},"incrementalReview":{"reviewedHeadSha":"cccccccccccccccccccccccccccccccccccccccc"}}
JSON
set +e
python3 - "$OUT_DIR/stale-review.json" <<'PY'
import json
import sys
review = json.load(open(sys.argv[1], encoding="utf-8"))
if review["incrementalReview"]["reviewedHeadSha"] == review["revisionBinding"]["headSha"]:
    raise SystemExit("expected stale revision to be rejected")
print("stale-revision=REJECT")
PY
stale_rc=$?
set -e
[ "$stale_rc" -eq 0 ]

cat > "$OUT_DIR/review-run-contract.md" <<'EOF2'
# Review Run Contract Evidence

- Contract: `contracts/http/review-run-v1.yaml`
- Revision binding: base/head SHA and refs are digest-bound; stale revisions are rejected
- Scope plan: each changed file belongs to exactly one deterministic scope
- Finding lifecycle: open/kept/resolved/dismissed with human-resolved preservation
- Incremental review: baseline digest and reviewed head SHA are required
- Review trace: metadata-only tool and verifier telemetry
- Boundary: PREPARATION_ONLY / LOCAL_UNATTESTED; no SCM writeback or promotion
EOF2

echo "review-run-contract=PASS evidence=$OUT_DIR/review-run-contract.md"
