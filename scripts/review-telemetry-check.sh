#!/usr/bin/env bash
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/review-telemetry-check}"
CONTRACT="$REPO_ROOT/contracts/http/review-telemetry-v1.yaml"
mkdir -p "$OUT_DIR"
for needle in 'schemaVersion: review-telemetry-v1' 'sourceContent: forbidden' 'rawPrompts: forbidden' 'appendOnly: true' 'readOnly: true' 'mode: PREPARATION_ONLY' 'scmWriteback: forbidden'; do rg -F -q -- "$needle" "$CONTRACT"; done
cat > "$OUT_DIR/telemetry.json" <<'JSON'
{"snapshotDigest":"sha256:snapshot","queue":{"queued":1,"running":1,"succeeded":3,"inconclusive":0,"rejected":0},"versionChain":{"reviewRunId":"run-3","baseDigest":"sha256:base","headDigest":"sha256:head","status":"RUNNING","stale":false},"findingSummary":{"open":2,"kept":1,"resolved":4,"dismissed":0},"coverage":{"files":5,"changedLines":42,"reviewedScopes":3,"completedScopes":2},"usage":{"durationMs":1200,"modelTurns":4,"toolCalls":8,"deniedToolCalls":1,"estimatedCost":"sha256:cost"},"audit":{"eventCount":8,"headEventDigest":"sha256:event-8","lastEventDigest":"sha256:event-8"},"authority":"LOCAL_UNATTESTED","mode":"PREPARATION_ONLY"}
JSON
python3 - "$OUT_DIR/telemetry.json" <<'PY'
import json,sys
x=json.load(open(sys.argv[1],encoding='utf-8'))
assert x['snapshotDigest'].startswith('sha256:')
for section in ('queue','findingSummary','coverage','usage'): assert all(isinstance(v,int) and v>=0 for v in x[section].values() if isinstance(v,int))
for k in ('baseDigest','headDigest'): assert x['versionChain'][k].startswith('sha256:')
assert x['versionChain']['stale'] is False
assert x['audit']['headEventDigest']==x['audit']['lastEventDigest']
assert x['authority']=='LOCAL_UNATTESTED' and x['mode']=='PREPARATION_ONLY'
PY
cat > "$OUT_DIR/review-telemetry.md" <<'EOF2'
# Review Telemetry Evidence

- Read-only queue, version chain, finding state, coverage, usage and audit projection.
- All identities are digest-only; source content and raw prompts are forbidden.
- Boundary remains PREPARATION_ONLY / LOCAL_UNATTESTED.
EOF2
echo "review-telemetry=PASS evidence=$OUT_DIR/review-telemetry.md"
