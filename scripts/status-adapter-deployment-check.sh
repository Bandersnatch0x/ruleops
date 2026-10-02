#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/status-adapter-deployment-check}"
CONTRACT="$REPO_ROOT/contracts/http/status-adapter-deployment-v1.yaml"
STATUS="$REPO_ROOT/runtime/status/src/deployment.rs"
STATUS_LIB="$REPO_ROOT/runtime/status/src/lib.rs"
ADR="$REPO_ROOT/docs/adr/0143-status-adapter-deployment-contract.md"
mkdir -p "$OUT_DIR"

for file in "$CONTRACT" "$STATUS" "$ADR"; do test -s "$file"; done
for needle in \
  'schemaVersion: status-adapter-deployment-v1' \
  'scheme: https' \
  'secretBytes: forbidden' \
  'exact: [status:write]' \
  'transportOrAvailabilityUnknown: UNKNOWN' \
  'pub struct StatusAdapterDeployment' \
  'pub struct StatusAdapterReceipt' \
  'UnknownTransport' \
  'deployment_descriptor_is_minimal_https_scoped_and_replayable'; do
  rg -F -q -- "$needle" "$CONTRACT" "$STATUS" "$STATUS_LIB" "$ADR"
done

(cd "$REPO_ROOT" && cargo test -q -p erf-status)

cat > "$OUT_DIR/status-adapter-deployment.md" <<EOF
# Status Adapter Deployment Evidence

- contract: contracts/http/status-adapter-deployment-v1.yaml
- descriptor: HTTPS-only, opaque secret reference, exact status:write scope
- binding: StatusPublication digest plus current-time freshness and identity checks
- receipt: ACCEPTED/IDEMPOTENT, RECEIVED_ONLY, self-digested, scmWritten=false
- failures: validation/authority rejection is fail-closed; availability remains UNKNOWN
- adapter: local fake only; no network call, SCM connector, token, or private key
- tests: replay, secret/endpoint/scope rejection, freshness, digest and outcome classification
EOF
echo "status-adapter-deployment=PASS evidence=$OUT_DIR/status-adapter-deployment.md"
