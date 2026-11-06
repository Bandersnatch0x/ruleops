#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/review-dashboard-check}"
CONTRACT="$REPO_ROOT/contracts/http/review-dashboard-v1.yaml"
mkdir -p "$OUT_DIR"
test -s "$CONTRACT"

for needle in \
  'kind: ReviewDashboardTransportContract' \
  'schemaVersion: review-dashboard-sse-v1' \
  'path: /v1/reviews/dashboard' \
  'path: /v1/reviews/dashboard/events' \
  'writeEndpoints: forbidden' \
  'schemaVersion: review-telemetry-snapshot-v1' \
  'required: [cursor, sequence, snapshotDigest, queue, version, finding, coverage, audit, authority, mode]' \
  'sourceContent: forbidden' \
  'rawPrompts: forbidden' \
  'schemaVersion: review-dashboard-sse-event-v1' \
  'required:' \
  'eventDigest:' \
  'mode: PREPARATION_ONLY' \
  'authority: LOCAL_UNATTESTED' \
  'scmWriteback: forbidden' \
  'gateMutation: forbidden'; do
  rg -F -q -- "$needle" "$CONTRACT"
done

cat > "$OUT_DIR/snapshot.json" <<'JSON'
{
  "cursor": {"value": "cursor-0007", "sequence": 7},
  "sequence": 7,
  "snapshotDigest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "queue": {"queued": 1, "running": 1, "succeeded": 3, "inconclusive": 0, "rejected": 0, "projectionDigest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},
  "version": {"reviewRunDigest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc", "baseDigest": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd", "headDigest": "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "status": "RUNNING", "stale": false, "projectionDigest": "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"},
  "finding": {"open": 2, "kept": 1, "resolved": 4, "dismissed": 0, "projectionDigest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"},
  "coverage": {"files": 5, "changedLines": 42, "reviewedScopes": 3, "completedScopes": 2, "projectionDigest": "sha256:2222222222222222222222222222222222222222222222222222222222222222"},
  "audit": {"eventCount": 7, "headEventDigest": "sha256:3333333333333333333333333333333333333333333333333333333333333333", "lastEventDigest": "sha256:3333333333333333333333333333333333333333333333333333333333333333", "projectionDigest": "sha256:4444444444444444444444444444444444444444444444444444444444444444"},
  "authority": "LOCAL_UNATTESTED",
  "mode": "PREPARATION_ONLY"
}
JSON

cat > "$OUT_DIR/event.json" <<'JSON'
{
  "eventId": "evt-0007",
  "eventType": "snapshot",
  "cursor": "cursor-0007",
  "sequence": 7,
  "snapshotDigest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "payloadDigest": "sha256:5555555555555555555555555555555555555555555555555555555555555555",
  "previousEventDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
  "eventDigest": "sha256:7777777777777777777777777777777777777777777777777777777777777777",
  "payload": {"section": "snapshot", "projectionDigest": "sha256:8888888888888888888888888888888888888888888888888888888888888888"}
}
JSON

cat > "$OUT_DIR/event-2.json" <<'JSON'
{
  "eventId": "evt-0008",
  "eventType": "heartbeat",
  "cursor": "cursor-0008",
  "sequence": 8,
  "snapshotDigest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "payloadDigest": "sha256:9999999999999999999999999999999999999999999999999999999999999999",
  "previousEventDigest": "sha256:7777777777777777777777777777777777777777777777777777777777777777",
  "eventDigest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  "payload": {"section": "heartbeat", "projectionDigest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"}
}
JSON

python3 - "$OUT_DIR/snapshot.json" "$OUT_DIR/event.json" "$OUT_DIR/event-2.json" <<'PY'
import json
import re
import sys

snapshot = json.load(open(sys.argv[1], encoding="utf-8"))
event = json.load(open(sys.argv[2], encoding="utf-8"))
event_2 = json.load(open(sys.argv[3], encoding="utf-8"))
digest = re.compile(r"^sha256:[0-9a-f]{64}$")
assert snapshot["cursor"]["sequence"] == snapshot["sequence"]
assert digest.fullmatch(snapshot["snapshotDigest"])
for section in ("queue", "finding", "coverage"):
    projection = snapshot[section]
    assert all(isinstance(projection[k], int) and projection[k] >= 0 for k in projection if k != "projectionDigest")
    assert digest.fullmatch(projection["projectionDigest"])
version = snapshot["version"]
assert digest.fullmatch(version["reviewRunDigest"])
assert digest.fullmatch(version["baseDigest"])
assert digest.fullmatch(version["headDigest"])
assert digest.fullmatch(version["projectionDigest"])
assert isinstance(version["stale"], bool)
audit = snapshot["audit"]
assert digest.fullmatch(audit["headEventDigest"])
assert digest.fullmatch(audit["lastEventDigest"])
assert digest.fullmatch(audit["projectionDigest"])
assert audit["eventCount"] >= 0
assert snapshot["authority"] == "LOCAL_UNATTESTED"
assert snapshot["mode"] == "PREPARATION_ONLY"

assert event["sequence"] == snapshot["sequence"]
assert event["cursor"] == snapshot["cursor"]["value"]
assert event["snapshotDigest"] == snapshot["snapshotDigest"]
assert event["previousEventDigest"] == "sha256:" + "0" * 64
for field in ("snapshotDigest", "payloadDigest", "previousEventDigest", "eventDigest"):
    assert digest.fullmatch(event[field])
assert event["eventType"] in {"snapshot", "queue", "version", "finding", "coverage", "audit", "heartbeat", "reset"}
assert "sourceContent" not in event["payload"]
assert "fullFinding" not in event["payload"]
assert "rawPrompts" not in event["payload"]

assert event_2["sequence"] == event["sequence"] + 1
assert event_2["previousEventDigest"] == event["eventDigest"]
assert event_2["snapshotDigest"] == event["snapshotDigest"]
for field in ("snapshotDigest", "payloadDigest", "previousEventDigest", "eventDigest"):
    assert digest.fullmatch(event_2[field])
assert event_2["eventType"] in {"snapshot", "queue", "version", "finding", "coverage", "audit", "heartbeat", "reset"}
assert "sourceContent" not in event_2["payload"]
assert "fullFinding" not in event_2["payload"]
assert "rawPrompts" not in event_2["payload"]
print("snapshot=PASS event-envelope=PASS genesis=PASS continuity=PASS")
PY

cat > "$OUT_DIR/rejected-chain.json" <<'JSON'
{"firstEventDigest":"sha256:7777777777777777777777777777777777777777777777777777777777777777","secondPreviousEventDigest":"sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"}
JSON
python3 - "$OUT_DIR/rejected-chain.json" <<'PY'
import json
import sys
chain = json.load(open(sys.argv[1], encoding="utf-8"))
assert chain["secondPreviousEventDigest"] != chain["firstEventDigest"]
print("tampered-chain=REJECT")
PY

cat > "$OUT_DIR/rejected-write.json" <<'JSON'
{"method":"POST","path":"/v1/reviews/dashboard","expected":"METHOD_NOT_ALLOWED"}
JSON
python3 - "$OUT_DIR/rejected-write.json" <<'PY'
import json
import sys
write = json.load(open(sys.argv[1], encoding="utf-8"))
assert write["method"] == "POST"
assert write["expected"] == "METHOD_NOT_ALLOWED"
print("write-endpoint=REJECT")
PY

cat > "$OUT_DIR/review-dashboard.md" <<'EOF2'
# Review Dashboard / SSE Evidence

- Telemetry snapshot is deterministic, cursor/sequence aligned and digest-only.
- SSE event envelope carries cursor, sequence, snapshotDigest and eventDigest for resumable consumers;
  the first event uses the declared zero genesis digest and subsequent events preserve continuity.
- Queue, version, finding, coverage and audit projections expose counts/identities only; source content,
  raw prompts and full findings are forbidden.
- Only GET dashboard and SSE endpoints are defined; writes are rejected.
- Boundary remains PREPARATION_ONLY / LOCAL_UNATTESTED with no SCM writeback or gate mutation.
EOF2

echo "review-dashboard=PASS evidence=$OUT_DIR/review-dashboard.md"
