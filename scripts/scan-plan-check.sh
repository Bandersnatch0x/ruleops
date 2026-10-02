#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-$REPO_ROOT/.scratch/scan-plan-check}"
mkdir -p "$OUT_DIR"
cd "$REPO_ROOT"

for file in \
  runtime/scan-plan/Cargo.toml \
  runtime/scan-plan/src/lib.rs \
  docs/adr/0147-frozen-incremental-scan-plan-and-cache.md; do
  test -s "$file"
done

rg -F -q 'pub struct ScanPlan' runtime/scan-plan/src/lib.rs
rg -F -q 'pub enum PlanMode' runtime/scan-plan/src/lib.rs
rg -F -q 'pub struct FileScanCache' runtime/scan-plan/src/lib.rs
rg -F -q 'pub struct CompletenessReport' runtime/scan-plan/src/lib.rs
rg -F -q 'frozen_plan' runtime/rf-runner/src/main.rs
rg -F -q 'ScanPlanNotAuthoritative' runtime/trusted-ci/src/lib.rs

cargo test --quiet -p erf-scan-plan >"$OUT_DIR/tests.log"
cargo test --quiet -p rf-runner >"$OUT_DIR/runner-tests.log"

cat >"$OUT_DIR/scan-plan.md" <<EOF
# Frozen ScanPlan Evidence

- modes: FULL, MODULE and INCREMENTAL_PREVIEW
- identity: repository/base/head/tree, AffectedScope, RuleBundle, Policy, Resolver, Backend, Runner, protocol and canonical-set versions
- completeness: selected, missing, failed, unsupported, not-applicable and unevaluated units remain explicit
- authority: preview cannot become authoritative PASS; complete module/full closures may be Gate-eligible
- cache: content-addressed, immutable, restart-readable records recheck plan/result identities and expiry
- privacy: cache records contain digests and coverage metadata only; no source or full Finding
- tests: $OUT_DIR/tests.log and $OUT_DIR/runner-tests.log
- decision: docs/adr/0147-frozen-incremental-scan-plan-and-cache.md
EOF
echo "scan-plan=PASS evidence=$OUT_DIR/scan-plan.md"
