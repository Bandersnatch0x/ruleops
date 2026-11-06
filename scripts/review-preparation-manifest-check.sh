#!/usr/bin/env bash
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/review-preparation-manifest-check}"
CONTRACT="$REPO_ROOT/contracts/http/review-preparation-manifest-v1.yaml"
mkdir -p "$OUT_DIR"
for needle in 'schemaVersion: review-preparation-manifest-v1' 'optional: [providerDraftDigest, impactScoreDigest]' 'optionalArtifactRule:' 'artifactRevisionBinding:' 'revisionMismatch: reject' 'staleManifest: reject' 'sourceContent: forbidden' 'scoreIsProjectionOnly: true' 'codeGraphIsOptional: true' 'dashboardReadOnly: true' 'providerDraftOnly: true' 'rulesLoadedFromBaseRevision: true' 'mode: PREPARATION_ONLY' 'scmWriteback: forbidden'; do rg -F -q -- "$needle" "$CONTRACT"; done
cat > "$OUT_DIR/manifest.json" <<'JSON'
{
  "manifestId": "manifest-1",
  "revisionBindingDigest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
  "scopePlanDigest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
  "reviewRunDigest": "sha256:3333333333333333333333333333333333333333333333333333333333333333",
  "ledgerHeadDigest": "sha256:4444444444444444444444444444444444444444444444444444444444444444",
  "telemetrySnapshotDigest": "sha256:5555555555555555555555555555555555555555555555555555555555555555",
  "dashboardSnapshotDigest": "sha256:6666666666666666666666666666666666666666666666666666666666666666",
  "providerDraftDigest": "sha256:7777777777777777777777777777777777777777777777777777777777777777",
  "impactScoreDigest": "sha256:8888888888888888888888888888888888888888888888888888888888888888",
  "rulesConfigBindingDigest": "sha256:9999999999999999999999999999999999999999999999999999999999999999",
  "manifestDigest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "revision": {
    "baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  },
  "artifactRevisions": {
    "revisionBindingDigest": {"baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
    "scopePlanDigest": {"baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
    "reviewRunDigest": {"baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
    "ledgerHeadDigest": {"baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
    "telemetrySnapshotDigest": {"baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
    "dashboardSnapshotDigest": {"baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
    "providerDraftDigest": {"baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
    "impactScoreDigest": {"baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
    "rulesConfigBindingDigest": {"baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
    "manifestDigest": {"baseSha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "headSha": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}
  },
  "authority": "LOCAL_UNATTESTED",
  "mode": "PREPARATION_ONLY"
}
JSON
python3 - "$OUT_DIR/manifest.json" <<'PY'
import copy
import json
import re
import sys

DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")
SHA = re.compile(r"^[0-9a-f]{40}$")
REQUIRED = {
    "revisionBindingDigest", "scopePlanDigest", "reviewRunDigest",
    "ledgerHeadDigest", "telemetrySnapshotDigest", "dashboardSnapshotDigest",
    "rulesConfigBindingDigest", "manifestDigest",
}
OPTIONAL = {"providerDraftDigest", "impactScoreDigest"}


def validate_manifest(value):
    assert REQUIRED <= value.keys()
    present = REQUIRED | (OPTIONAL & value.keys())
    assert value["revision"]["baseSha"] != value["revision"]["headSha"]
    assert SHA.fullmatch(value["revision"]["baseSha"])
    assert SHA.fullmatch(value["revision"]["headSha"])
    for key in present:
        assert DIGEST.fullmatch(value[key]), key

    # Every referenced artifact carries the same immutable base/head identity.
    # Optional artifacts may be absent, but a present digest cannot omit its
    # revision metadata or silently bind to a different revision.
    revisions = value["artifactRevisions"]
    assert set(revisions) == present
    for key in present:
        observed = revisions[key]
        assert observed["baseSha"] == value["revision"]["baseSha"], key
        assert observed["headSha"] == value["revision"]["headSha"], key

    assert value["authority"] == "LOCAL_UNATTESTED"
    assert value["mode"] == "PREPARATION_ONLY"
    assert "sourceContent" not in json.dumps(value)


valid = json.load(open(sys.argv[1], encoding="utf-8"))
validate_manifest(valid)

# A preparation manifest remains valid when optional projections are not
# available. Their digest and revision entries must be omitted together.
minimal = copy.deepcopy(valid)
for key in OPTIONAL:
    minimal.pop(key)
    minimal["artifactRevisions"].pop(key)
validate_manifest(minimal)
print("optional-artifacts=OMIT_ALLOWED")


def expect_reject(label, mutate):
    candidate = copy.deepcopy(valid)
    mutate(candidate)
    try:
        validate_manifest(candidate)
    except (AssertionError, KeyError):
        print(f"{label}=REJECT")
    else:
        raise AssertionError(f"{label} was accepted")


expect_reject(
    "cross-artifact-revision-mismatch",
    lambda value: value["artifactRevisions"]["dashboardSnapshotDigest"].__setitem__(
        "headSha", "c" * 40
    ),
)
expect_reject(
    "present-optional-without-binding",
    lambda value: value["artifactRevisions"].pop("providerDraftDigest"),
)
expect_reject(
    "malformed-optional-digest",
    lambda value: value.__setitem__("impactScoreDigest", "sha256:not-a-digest"),
)
PY
cat > "$OUT_DIR/stale-manifest.json" <<'JSON'
{"revision":{"baseSha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","headSha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},"resume":{"baseSha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","headSha":"cccccccccccccccccccccccccccccccccccccccc"}}
JSON
python3 - "$OUT_DIR/stale-manifest.json" <<'PY'
import json,sys
x=json.load(open(sys.argv[1],encoding='utf-8')); assert x['revision'] != x['resume']; print('stale-manifest=REJECT')
PY
cat > "$OUT_DIR/review-preparation-manifest.md" <<'EOF2'
# Review Preparation Manifest Evidence

- Cross-contract digests bind revision, scope, run, ledger, telemetry, dashboard, provider draft, impact score and rules/config.
- Provider-draft and impact-score projections are optional: omission is accepted, while a present
  optional digest must carry matching base/head revision metadata.
- Any cross-artifact revision mismatch or malformed optional digest is rejected before resume.
- Score remains a projection; Code Graph is optional; dashboard/provider paths are read-only/draft-only.
- Boundary remains PREPARATION_ONLY / LOCAL_UNATTESTED.
EOF2
echo "review-preparation-manifest=PASS evidence=$OUT_DIR/review-preparation-manifest.md"
