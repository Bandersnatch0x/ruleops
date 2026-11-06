#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/review-capability-doc-check}"
DOC="$REPO_ROOT/docs/review-capability-roadmap.md"
mkdir -p "$OUT_DIR"

test -s "$DOC"

# Keep the roadmap tied to the contract set. A missing link or boundary marker
# should fail this small documentation-only check before a full build starts.
contracts=(
  review-run-v1
  review-ledger-v1
  review-telemetry-v1
  review-dashboard-v1
  review-provider-adapter-v1
  review-rules-config-binding-v1
  review-impact-score-v1
  review-preparation-manifest-v1
)
for name in "${contracts[@]}"; do
  contract="$REPO_ROOT/contracts/http/$name.yaml"
  test -s "$contract"
  rg -F -q -- "../contracts/http/$name.yaml" "$DOC"
  rg -F -q -- 'status: preparation-only' "$contract"
done

for marker in \
  'P0' \
  'P1' \
  'P2' \
  'PREPARATION_ONLY' \
  'LOCAL_UNATTESTED' \
  'SCM writeback' \
  'Gate mutation' \
  'review-capability-doc-check.sh' \
  'Full-check prerequisites'; do
  rg -F -q -- "$marker" "$DOC"
done

contract_count="${#contracts[@]}"
cat > "$OUT_DIR/review-capability-doc-check.md" <<EOF2
# Review Capability Documentation Check

- Roadmap: docs/review-capability-roadmap.md
- Contracts checked: $contract_count
- Required priority and authority-boundary markers: present
- Result: PASS (documentation/link consistency only)
EOF2

echo "review-capability-doc=PASS evidence=$OUT_DIR/review-capability-doc-check.md"
