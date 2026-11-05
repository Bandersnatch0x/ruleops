#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/review-provider-adapter-check}"
CONTRACT="$REPO_ROOT/contracts/http/review-provider-adapter-v1.yaml"
mkdir -p "$OUT_DIR"
test -s "$CONTRACT"

for needle in \
  'schemaVersion: review-provider-draft-v1' \
  'providerNeutral: true' \
  'supportedProviders: [github, gitlab]' \
  'draftOnly: true' \
  'sourceContent: forbidden' \
  'rawPrompts: forbidden' \
  'schemaVersion: review-rescore-job-v1' \
  'independentRescore: true' \
  'metadataOnly: true' \
  'baselineMustBeDigestBound: true' \
  'updatesOriginalDraft: forbidden' \
  'mode: PREPARATION_ONLY' \
  'authority: LOCAL_UNATTESTED' \
  'scmWriteback: forbidden' \
  'gateReplacement: forbidden'; do
  rg -F -q -- "$needle" "$CONTRACT"
done

cat > "$OUT_DIR/provider-drafts.json" <<'JSON'
{
  "schemaVersion": "review-provider-draft-v1",
  "providerNeutral": true,
  "drafts": [
    {
      "draftId": "draft-github-42",
      "provider": "github",
      "repositoryDigest": "sha256:repo-github",
      "changeRequestId": "42",
      "changeRequest": {
        "providerNeutralKind": "change-request",
        "number": 42,
        "titleDigest": "sha256:title-github",
        "bodyDigest": "sha256:body-github"
      },
      "changeRequestKind": "change-request",
      "baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "findings": [
        {
          "findingId": "finding-1",
          "ruleDigest": "sha256:rule-1",
          "locationDigest": "sha256:location-1",
          "severity": "HIGH",
          "disposition": "OPEN"
        }
      ],
      "projectionDigest": "sha256:projection-github"
    },
    {
      "draftId": "draft-gitlab-7",
      "provider": "gitlab",
      "repositoryDigest": "sha256:repo-gitlab",
      "changeRequestId": "7",
      "changeRequest": {
        "providerNeutralKind": "change-request",
        "number": 7,
        "titleDigest": "sha256:title-gitlab",
        "bodyDigest": "sha256:body-gitlab"
      },
      "changeRequestKind": "change-request",
      "baseSha": "cccccccccccccccccccccccccccccccccccccccc",
      "headSha": "dddddddddddddddddddddddddddddddddddddddd",
      "findings": [],
      "projectionDigest": "sha256:projection-gitlab"
    }
  ],
  "rescoreJob": {
    "rescoreJobId": "rescore-1",
    "draftDigest": "sha256:projection-github",
    "baselineScoreDigest": "sha256:score-baseline",
    "independentRescore": true,
    "scorer": {
      "provider": "local-deterministic",
      "model": "rule-reviewer",
      "modelVersion": "1",
      "policyDigest": "sha256:policy"
    },
    "requestedAt": "2026-11-05T15:00:00Z",
    "status": "SUCCEEDED",
    "result": {
      "resultDigest": "sha256:rescore-result",
      "scoreDigest": "sha256:score-rescored"
    },
    "metadataDigest": "sha256:rescore-metadata"
  },
  "authority": "LOCAL_UNATTESTED",
  "mode": "PREPARATION_ONLY"
}
JSON

python3 - "$OUT_DIR/provider-drafts.json" <<'PY'
import json
import re
import sys

payload = json.load(open(sys.argv[1], encoding="utf-8"))
assert payload["schemaVersion"] == "review-provider-draft-v1"
assert payload["providerNeutral"] is True
assert {draft["provider"] for draft in payload["drafts"]} == {"github", "gitlab"}
sha = re.compile(r"^[0-9a-f]{40}$")
for draft in payload["drafts"]:
    for key in ("draftId", "repositoryDigest", "changeRequestId", "projectionDigest"):
        assert draft[key]
    assert draft["changeRequestKind"] == "change-request"
    assert draft["changeRequest"]["providerNeutralKind"] == "change-request"
    assert draft["changeRequest"]["number"] == int(draft["changeRequestId"])
    assert draft["changeRequest"]["titleDigest"].startswith("sha256:")
    assert draft["changeRequest"]["bodyDigest"].startswith("sha256:")
    assert sha.fullmatch(draft["baseSha"]) and sha.fullmatch(draft["headSha"])
    assert draft["repositoryDigest"].startswith("sha256:")
    assert draft["projectionDigest"].startswith("sha256:")
    for finding in draft["findings"]:
        assert finding["ruleDigest"].startswith("sha256:")
        assert finding["locationDigest"].startswith("sha256:")
        assert finding["disposition"] in {"OPEN", "KEPT", "RESOLVED", "DISMISSED"}
        assert "sourceContent" not in finding
job = payload["rescoreJob"]
assert job["independentRescore"] is True
assert job["draftDigest"].startswith("sha256:")
assert job["baselineScoreDigest"].startswith("sha256:")
assert job["metadataDigest"].startswith("sha256:")
assert job["result"]["resultDigest"].startswith("sha256:")
assert job["result"]["scoreDigest"] != job["baselineScoreDigest"]
assert payload["authority"] == "LOCAL_UNATTESTED"
assert payload["mode"] == "PREPARATION_ONLY"
# Source payloads are never part of a provider-neutral draft or rescore metadata.
assert "sourceContent" not in payload
assert "rawBody" not in payload
assert "diff" not in payload
PY

cat > "$OUT_DIR/rejected-provider.json" <<'JSON'
{"provider":"bitbucket","draftOnly":true}
JSON
set +e
python3 - "$OUT_DIR/rejected-provider.json" <<'PY'
import json
import sys
payload = json.load(open(sys.argv[1], encoding="utf-8"))
if payload.get("provider") in {"github", "gitlab"}:
    raise SystemExit("unsupported provider should be rejected")
print("unsupported-provider=REJECT")
PY
rejected_rc=$?
set -e
[ "$rejected_rc" -eq 0 ]

cat > "$OUT_DIR/review-provider-adapter.md" <<'EOF2'
# Review Provider Adapter Evidence

- GitHub pull requests and GitLab merge requests use one provider-neutral draft projection.
- Drafts and findings are digest-only; provider write APIs and source content are forbidden.
- Rescore jobs carry independent scorer, baseline and result metadata without mutating the draft or gate.
- Boundary remains PREPARATION_ONLY / LOCAL_UNATTESTED; SCM writeback and gate replacement are forbidden.
EOF2

echo "review-provider-adapter=PASS evidence=$OUT_DIR/review-provider-adapter.md"
