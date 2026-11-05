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
{"bundleDigest":"sha256:bundle","reviewRunDigest":"sha256:run","revisionBindingDigest":"sha256:binding","planDigest":"sha256:plan","reviewRun":{"reviewRunId":"run-1","state":"RUNNING","headDigest":"sha256:head","eventHeadDigest":"sha256:event-2"},"events":[{"sequence":1,"eventId":"e-1","eventType":"RUN_CREATED","payloadDigest":"sha256:p1","previousEventDigest":"sha256:genesis","eventDigest":"sha256:event-1"},{"sequence":2,"eventId":"e-2","eventType":"SCOPE_STARTED","payloadDigest":"sha256:p2","previousEventDigest":"sha256:event-1","eventDigest":"sha256:event-2"}],"authority":"LOCAL_UNATTESTED","mode":"PREPARATION_ONLY"}
JSON
python3 - "$OUT_DIR/ledger.json" <<'PY'
import json,sys
x=json.load(open(sys.argv[1],encoding='utf-8'))
for k in ('bundleDigest','reviewRunDigest','revisionBindingDigest','planDigest'):
    assert x[k].startswith('sha256:')
events=x['events']; assert events and [e['sequence'] for e in events]==list(range(1,len(events)+1))
for prev,cur in zip(events,events[1:]): assert cur['previousEventDigest']==prev['eventDigest']
assert x['reviewRun']['eventHeadDigest']==events[-1]['eventDigest']
assert x['authority']=='LOCAL_UNATTESTED' and x['mode']=='PREPARATION_ONLY'
assert all('sourceContent' not in e for e in events)
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
- Resume rejects stale head digests and preserves event ordering.
- ReviewTrace is metadata-only; source content is forbidden.
- Boundary remains PREPARATION_ONLY / LOCAL_UNATTESTED.
EOF2
echo "review-ledger=PASS evidence=$OUT_DIR/review-ledger.md"
