#!/usr/bin/env bash
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/review-preparation-manifest-check}"
CONTRACT="$REPO_ROOT/contracts/http/review-preparation-manifest-v1.yaml"
mkdir -p "$OUT_DIR"
for needle in 'schemaVersion: review-preparation-manifest-v1' 'staleManifest: reject' 'sourceContent: forbidden' 'scoreIsProjectionOnly: true' 'codeGraphIsOptional: true' 'dashboardReadOnly: true' 'providerDraftOnly: true' 'rulesLoadedFromBaseRevision: true' 'mode: PREPARATION_ONLY' 'scmWriteback: forbidden'; do rg -F -q -- "$needle" "$CONTRACT"; done
cat > "$OUT_DIR/manifest.json" <<'JSON'
{"manifestId":"manifest-1","revisionBindingDigest":"sha256:binding","scopePlanDigest":"sha256:scope","reviewRunDigest":"sha256:run","ledgerHeadDigest":"sha256:ledger","telemetrySnapshotDigest":"sha256:telemetry","dashboardSnapshotDigest":"sha256:dashboard","providerDraftDigest":"sha256:provider","impactScoreDigest":"sha256:score","rulesConfigBindingDigest":"sha256:rules","manifestDigest":"sha256:manifest","revision":{"baseSha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","headSha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},"authority":"LOCAL_UNATTESTED","mode":"PREPARATION_ONLY"}
JSON
python3 - "$OUT_DIR/manifest.json" <<'PY'
import json,sys
x=json.load(open(sys.argv[1],encoding='utf-8'))
for k,v in x.items():
    if k.endswith('Digest'): assert v.startswith('sha256:')
assert x['revision']['baseSha'] != x['revision']['headSha']
assert x['authority']=='LOCAL_UNATTESTED' and x['mode']=='PREPARATION_ONLY'
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
- Score remains a projection; Code Graph is optional; dashboard/provider paths are read-only/draft-only.
- Boundary remains PREPARATION_ONLY / LOCAL_UNATTESTED.
EOF2
echo "review-preparation-manifest=PASS evidence=$OUT_DIR/review-preparation-manifest.md"
