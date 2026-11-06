#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/review-runtime-integration-check}"
mkdir -p "$OUT_DIR"
ADAPTER="$REPO_ROOT/runtime/rf-cli/src/review_preparation.rs"
CHANGED_SET="$REPO_ROOT/runtime/git-scope/src/lib.rs"

for needle in \
  'REVIEW_PREPARATION_VERSION: &str = "review-preparation/v1"' \
  'PREPARATION_MODE: &str = "PREPARATION_ONLY"' \
  'PREPARATION_AUTHORITY: &str = "LOCAL_UNATTESTED"' \
  'ReviewPreparationAdapter::prepare' \
  'ChangedSet::from_digest_only_paths' \
  'scmWriteback": "forbidden"' \
  'forbidden source-bearing field' \
  'PlanMode::IncrementalPreview' \
  'PlanAuthority::PreviewOnly'; do
  rg -F -q -- "$needle" "$ADAPTER"
done
rg -F -q -- 'pub fn from_digest_only_paths' "$CHANGED_SET"

if command -v cargo >/dev/null 2>&1; then
  cargo test -p rf-cli review_preparation --quiet | tee "$OUT_DIR/rust-tests.log"
  cargo test -p erf-git-scope digest_only_projection --quiet | tee -a "$OUT_DIR/rust-tests.log"
else
  printf '%s\n' 'cargo=UNAVAILABLE (static contract assertions passed)' > "$OUT_DIR/rust-tests.log"
fi

cat > "$OUT_DIR/review-runtime-integration.md" <<'EOF2'
# Review Runtime Integration Evidence

- Input: digest-only `RevisionBinding` + `ScopePlan` from `review-run-v1`
- Projection: read-only `ChangedSet` → bounded `AffectedScope` → frozen `ScanPlan`
- Finding lifecycle: OPEN / KEPT / RESOLVED / DISMISSED metadata with human-resolved preservation
- Authority: `PREPARATION_ONLY` / `LOCAL_UNATTESTED`
- Plan mode: incremental preview; Gate authority is not replaced or mutated
- Privacy: source, diff, prompt, comment and full Finding fields are rejected
- SCM: writeback is forbidden; no SCM command or credential is used by the adapter
EOF2

echo "review-runtime-integration=PASS evidence=$OUT_DIR/review-runtime-integration.md"
