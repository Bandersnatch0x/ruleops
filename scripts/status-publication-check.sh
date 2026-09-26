#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/status-publication-check}"
CONTRACT="$REPO_ROOT/contracts/http/status-publication-v1.yaml"
STATUS="$REPO_ROOT/runtime/status/src/lib.rs"
TOML="$REPO_ROOT/runtime/status/Cargo.toml"
ADR="$REPO_ROOT/docs/adr/0142-status-publication-artifact.md"
mkdir -p "$OUT_DIR"

for file in "$CONTRACT" "$STATUS" "$TOML" "$ADR"; do test -s "$file"; done
for needle in \
  'schemaVersion: status-publication-v1' \
  'status: READY_FOR_ADAPTER' \
  'neutral: forbidden' \
  'externalAdapterRequired: true' \
  'scmCredentialsInArtifact: false' \
  'pub struct StatusPublication' \
  'pub trait StatusPublicationAdapter' \
  'pub struct FakeStatusAdapter' \
  'publication_artifact_binds_mapping_freshness_and_external_authority' \
  'publication_adapter_maps_fail_and_inconclusive_without_neutral_success'; do
  rg -F -q -- "$needle" "$CONTRACT" "$STATUS" "$ADR"
done

(cd "$REPO_ROOT" && cargo test -q -p erf-status)

cat > "$OUT_DIR/status-publication.md" <<EOF
# Status Publication Evidence

- contract: contracts/http/status-publication-v1.yaml
- artifact: StatusPublication v1, self-digest and freshness-bound
- verification: signed GateAttestation, repository/commit/policy/subject identity
- mapping: PASS=success, FAIL=failure, INCONCLUSIVE=action-required
- adapter: transport-neutral FakeStatusAdapter; no SCM client or credential
- boundary: artifact is ready for an external adapter, not an SCM publication
- tests: mapping, replay, tamper, freshness, stale/identity and no-neutral-success cases
EOF
echo "status-publication=PASS evidence=$OUT_DIR/status-publication.md"
