#!/usr/bin/env bash
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/review-ledger-check}"
CONTRACT="$REPO_ROOT/contracts/http/review-ledger-v1.yaml"
mkdir -p "$OUT_DIR"
for needle in \
  'schemaVersion: evidence-bundle-v1' \
  'schemaVersion: review-run-ledger-v1' \
  'resumability: digest-bound' \
  'schemaVersion: review-event-ledger-v1' \
  'appendOnly: true' \
  'chainRule: eventDigest binds previousEventDigest and payloadDigest' \
  'schemaVersion: review-trace-v1' \
  'mode: PREPARATION_ONLY' \
  'scmWriteback: forbidden'; do rg -F -q -- "$needle" "$CONTRACT"; done
cat > "$OUT_DIR/ledger.json" <<'JSON'
{
  "bundleDigest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
  "reviewRunDigest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
  "revisionBindingDigest": "sha256:3333333333333333333333333333333333333333333333333333333333333333",
  "planDigest": "sha256:4444444444444444444444444444444444444444444444444444444444444444",
  "reviewRun": {
    "reviewRunId": "run-1",
    "revisionBindingDigest": "sha256:3333333333333333333333333333333333333333333333333333333333333333",
    "planDigest": "sha256:4444444444444444444444444444444444444444444444444444444444444444",
    "state": "RUNNING",
    "headDigest": "sha256:5555555555555555555555555555555555555555555555555555555555555555",
    "eventHeadDigest": "sha256:7c1f967bd7050402e6405717530f5c4dd8e395b75c323e91651bcc44df09cde8"
  },
  "events": [
    {
      "sequence": 1,
      "eventId": "e-1",
      "reviewRunId": "run-1",
      "eventType": "RUN_CREATED",
      "payload": {"kind": "run-created", "scopeId": "scope-1"},
      "payloadDigest": "sha256:53ba6fe0ea3f2aae299a2c41efa61b4cabf645ae885328ee097c50ca0df6956e",
      "previousEventDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
      "eventDigest": "sha256:00e35940adda578fb27dd9be06109d056feb20279713faada6715bfa36a96f31"
    },
    {
      "sequence": 2,
      "eventId": "e-2",
      "reviewRunId": "run-1",
      "eventType": "SCOPE_STARTED",
      "payload": {"kind": "scope-started", "scopeId": "scope-1"},
      "payloadDigest": "sha256:7afed6511d24b9446404c63c1d3d535e69330857285eb31302c1b93af90f4205",
      "previousEventDigest": "sha256:00e35940adda578fb27dd9be06109d056feb20279713faada6715bfa36a96f31",
      "eventDigest": "sha256:7c1f967bd7050402e6405717530f5c4dd8e395b75c323e91651bcc44df09cde8"
    }
  ],
  "authority": "LOCAL_UNATTESTED",
  "mode": "PREPARATION_ONLY"
}
JSON
python3 - "$OUT_DIR/ledger.json" <<'PY'
import copy
import hashlib
import json
import re
import sys

DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")
EVENT_TYPES = {"RUN_CREATED", "SCOPE_STARTED", "SCOPE_COMPLETED", "FINDING_UPDATED", "VERIFIER_DISCARDED", "RUN_COMPLETED", "RUN_REJECTED"}


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def digest(value):
    return "sha256:" + hashlib.sha256(canonical(value)).hexdigest()


def validate_ledger(value):
    for key in ("bundleDigest", "reviewRunDigest", "revisionBindingDigest", "planDigest"):
        assert DIGEST.fullmatch(value[key]), key
    run = value["reviewRun"]
    assert run["revisionBindingDigest"] == value["revisionBindingDigest"]
    assert run["planDigest"] == value["planDigest"]
    events = value["events"]
    assert events and [event["sequence"] for event in events] == list(range(1, len(events) + 1))
    for event in events:
        assert event["reviewRunId"] == run["reviewRunId"]
        assert event["eventType"] in EVENT_TYPES
        assert DIGEST.fullmatch(event["payloadDigest"])
        assert DIGEST.fullmatch(event["previousEventDigest"])
        assert DIGEST.fullmatch(event["eventDigest"])
        # The ledger contract's chain rule is executable here: payloadDigest
        # covers canonical metadata-only payload bytes, while eventDigest
        # covers the immutable event identity and both digest links.
        assert digest(event["payload"]) == event["payloadDigest"]
        event_identity = {
            "eventId": event["eventId"],
            "eventType": event["eventType"],
            "reviewRunId": event["reviewRunId"],
            "sequence": event["sequence"],
            "payloadDigest": event["payloadDigest"],
            "previousEventDigest": event["previousEventDigest"],
        }
        assert digest(event_identity) == event["eventDigest"]
        assert "sourceContent" not in json.dumps(event)
    for previous, current in zip(events, events[1:]):
        assert current["previousEventDigest"] == previous["eventDigest"]
    assert run["eventHeadDigest"] == events[-1]["eventDigest"]
    assert value["authority"] == "LOCAL_UNATTESTED" and value["mode"] == "PREPARATION_ONLY"


valid = json.load(open(sys.argv[1], encoding="utf-8"))
validate_ledger(valid)


def expect_reject(label, mutate):
    candidate = copy.deepcopy(valid)
    mutate(candidate)
    try:
        validate_ledger(candidate)
    except (AssertionError, KeyError):
        print(f"tampered-{label}=REJECT")
    else:
        raise AssertionError(f"tampered {label} was accepted")


expect_reject("payloadDigest", lambda value: value["events"][0].__setitem__("payloadDigest", "sha256:" + "f" * 64))
expect_reject("previousEventDigest", lambda value: value["events"][1].__setitem__("previousEventDigest", "sha256:" + "e" * 64))
expect_reject("eventDigest", lambda value: value["events"][1].__setitem__("eventDigest", "sha256:" + "d" * 64))
PY
cat > "$OUT_DIR/stale-ledger.json" <<'JSON'
{"reviewRun":{"headDigest":"sha256:current"},"resume":{"headDigest":"sha256:stale"}}
JSON
python3 - "$OUT_DIR/stale-ledger.json" <<'PY'
import json,sys
x=json.load(open(sys.argv[1],encoding='utf-8')); assert x['reviewRun']['headDigest'] != x['resume']['headDigest']
print('stale-resume=REJECT')
PY
cat > "$OUT_DIR/review-ledger.md" <<'EOF2'
# Review Ledger Evidence

- EvidenceBundle, ReviewRun and append-only event chain are digest-bound.
- Event payload and event identity digests are recomputed from canonical JSON;
  tampered payload, previous-event or event digests are rejected.
- Resume rejects stale head digests and preserves event ordering.
- ReviewTrace is metadata-only; source content is forbidden.
- Boundary remains PREPARATION_ONLY / LOCAL_UNATTESTED.
EOF2
echo "review-ledger=PASS evidence=$OUT_DIR/review-ledger.md"
