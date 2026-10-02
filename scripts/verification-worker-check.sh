#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
if [[ $# -ge 1 ]]; then
  OUT="$1"
else
  OUT="$REPO_ROOT/.scratch/verification-worker-check"
fi
mkdir -p "$OUT"

cd "$REPO_ROOT"
cargo build --quiet -p erf-verification-worker --features fixture-signer -p erf-fake-host -p erf-fake-resolver
ERF_ALLOW_FIXTURE_SIGNER=1 ERF_ALLOW_FIXTURE_SANDBOX=1 target/debug/erf-verification-worker \
  --host target/debug/erf-fake-host \
  --resolver target/debug/erf-fake-resolver \
  --cancel-window-ms 150 \
  --suite-release-digest "$(printf '%064d' 5)" \
  --verification-worker-release-digest "$(printf '%064d' 6)" \
  --runner-release-digest "$(printf '%064d' 1)" \
  --backend-pack-digest "$(printf '%064d' 2)" \
  --resolver-pack-digest "$(printf '%064d' 3)" \
  --sandbox-provider-id rootless-oci-docker-v0 \
  --sandbox-policy-digest "$(printf '%064d' 4)" \
  --resolver-sandbox-provider-id rootless-oci-docker-v0 \
  --resolver-sandbox-policy-digest "$(printf '%064d' 7)" \
  --platform linux/amd64 \
  --fixture-sandbox-proof \
  --key-id verification-worker-fixture \
  --fixture-signing-key-hex "$(printf '01%.0s' {1..32})" \
  --output "$OUT/conformance-evidence.txt" \
  > >(tee "$OUT/verification-worker.log") 2>&1

rg -q '^verification=PASS ' "$OUT/verification-worker.log"
python3 - "$OUT/conformance-evidence.txt" <<'PY'
import base64
import json
import pathlib
import sys

envelope = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert set(envelope) == {"payloadType", "payload", "signatures"}
assert envelope["payloadType"] == "application/vnd.erf.asset.v0"
assert len(envelope["signatures"]) == 1
assert set(envelope["signatures"][0]) == {"keyid", "sig"}
assert base64.b64decode(envelope["payload"], validate=True)
assert base64.b64decode(envelope["signatures"][0]["sig"], validate=True)
PY
echo "verification-worker=PASS evidence=$OUT/conformance-evidence.txt"
