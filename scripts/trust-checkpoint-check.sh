#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/trust-checkpoint-check}"
mkdir -p "$OUT_DIR"
cd "$REPO_ROOT"

for file in \
  runtime/trust-state/src/lib.rs \
  runtime/trusted-context/src/lib.rs \
  runtime/trusted-ci/src/lib.rs \
  docs/adr/0144-persisted-trust-checkpoint-handoff.md \
  docs/adr/0151-trust-state-directory-sync-fail-closed.md; do
  test -s "$file"
done

rg -F -q 'initialize_signed_checkpoint' runtime/trust-state/src/lib.rs
rg -F -q 'advance_signed_checkpoint' runtime/trust-state/src/lib.rs
rg -F -q 'load_for_trusted_execution' runtime/trust-state/src/lib.rs
rg -F -q 'sync_directory' runtime/trust-state/src/lib.rs
rg -F -q 'initialize_directory_sync_failure_is_not_reported_as_persisted' runtime/trust-state/src/lib.rs
rg -F -q 'advance_directory_sync_failure_preserves_previous_state' runtime/trust-state/src/lib.rs
rg -F -q 'sync_directory' docs/adr/0151-trust-state-directory-sync-fail-closed.md
rg -F -q 'verify_with_checkpoint' runtime/trusted-context/src/lib.rs runtime/trusted-ci/src/lib.rs
rg -F -q 'persisted_trust_state: &' runtime/trusted-ci/src/lib.rs

cargo test -p erf-trust-state -p erf-trusted-context -p erf-trusted-ci \
  >"$OUT_DIR/tests.log" 2>&1

for fixture in \
  'initialize_directory_sync_failure_is_not_reported_as_persisted' \
  'advance_directory_sync_failure_preserves_previous_state'; do
  rg -F -q -- "$fixture ... ok" "$OUT_DIR/tests.log"
done
if rg -q 'test result:.*[1-9][0-9]* ignored' "$OUT_DIR/tests.log"; then
  echo "trust checkpoint tests reported ignored cases; evidence is incomplete" >&2
  exit 1
fi

cat >"$OUT_DIR/trust-checkpoint.md" <<EOF
# Persisted TrustCheckpoint Evidence

- persistence: explicit initialize/advance with bounded atomic replacement
- directorySync: rename-before/after directory fsync errors are explicit and fail closed
- trusted read: missing state is rejected by \`load_for_trusted_execution\`
- handoff: TrustedContext consumes an opaque handle loaded from the persisted TrustState
- coverage: signature, replay, rollback, tamper, expiry, state-loss and directory-sync failure boundaries
- tests: $OUT_DIR/tests.log
- decisions: docs/adr/0144-persisted-trust-checkpoint-handoff.md, docs/adr/0151-trust-state-directory-sync-fail-closed.md

The trusted CI path consumes the loaded state and cannot silently bootstrap or
advance the trust floor while evaluating a scan.
EOF
echo "trust-checkpoint=PASS evidence=$OUT_DIR/trust-checkpoint.md"
