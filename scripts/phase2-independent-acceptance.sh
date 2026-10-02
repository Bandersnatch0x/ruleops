#!/usr/bin/env bash
# Reproducible Phase 2 independent acceptance evidence.
# Usage: scripts/phase2-independent-acceptance.sh [OUT_DIR]
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$REPO_ROOT/.scratch/phase2-acceptance-$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$OUT"

cd "$REPO_ROOT"
cargo build --quiet -p erf-conformance -p erf-fake-host -p erf-fake-resolver
target/debug/erf-conformance \
  --host target/debug/erf-fake-host \
  --resolver target/debug/erf-fake-resolver \
  --cancel-window-ms 150 \
  > >(tee "$OUT/fake-host-acceptance.log") 2>&1

command -v node >/dev/null 2>&1 || {
  echo "error: node is required for the Phase 1/2 Fake Node contract proof" >&2
  exit 2
}
target/debug/erf-conformance \
  --host node \
  --host-arg "$REPO_ROOT/backend-hosts/node-fake/host.js" \
  --resolver node \
  --resolver-arg "$REPO_ROOT/backend-hosts/node-fake/resolver.js" \
  --cancel-window-ms 150 \
  > >(tee "$OUT/node-fake-acceptance.log") 2>&1

expected_checks=(
  golden_messages_bidirectional
  negotiation_and_describe
  required_capability_unmet_fails
  major_mismatch_refused
  malformed_frame_rejected
  oversized_frame_rejected
  execute_round_trip_completeness
  deadline_is_failure
  cooperative_cancel_accounts_units
  crash_is_incomplete_failure
  resolver_golden_messages_bidirectional
  resolver_minor_compatibility
  resolver_negotiation_and_resolution
  resolver_required_capability_unmet_fails
  resolver_major_mismatch_refused
  resolver_resolution_deterministic
  resolver_malformed_frame_rejected
  resolver_unknown_optional_field_ignored
  resolver_oversized_frame_rejected
  resolver_deadline_is_failure
  resolver_cooperative_cancel_accounts_request
  resolver_crash_is_incomplete_failure
  resolver_retry_after_failure
  resolver_safe_static_symlink_rejected
  resolver_backpressure_is_bounded
  local_canonical_finding_set
  local_ten_repeat_semantic_digest
  local_message_independence
  local_baseline_exception_orthogonality
  local_incomplete_never_passes
  local_malformed_typed_observation_never_passes
)
for acceptance_log in "$OUT/fake-host-acceptance.log" "$OUT/node-fake-acceptance.log"; do
  rg -q '^31/31 checks passed$' "$acceptance_log"
  for check in "${expected_checks[@]}"; do
    rg -q "^PASS ${check}[[:space:]]" "$acceptance_log" || {
      echo "missing passing check: ${check} in ${acceptance_log}" >&2
      exit 1
    }
  done
  [[ "$(rg -c '^PASS ' "$acceptance_log")" == "${#expected_checks[@]}" ]]
done

cat > "$OUT/phase2-acceptance.md" <<EOF
# Phase 2 独立验收

- Commit: \`$(git rev-parse HEAD)\`
- Suite: \`$(sed -n 's/^suite version //p' "$OUT/fake-host-acceptance.log" | tail -n 1)\`
- Host：reference Rust Fake Host + dependency-free Node.js Fake Host（每个协议检查使用新进程）
- Trust：\`LOCAL_UNATTESTED\`
- Authority：仅 \`SHADOW\`

## Result

Rust Fake 与 Node.js Fake 均通过同一版本化的 31 项检查，包含 Backend Host 与 Workspace Resolver 的协议故障注入，
以及 typed location 规范化、canonical Finding 排序与去重、与消息无关的
SemanticResultDigest、Baseline/Exception 正交决策、畸形 observation 和不完整
执行的独立 local Gate 检查。Resolver 检查还覆盖 golden compatibility、
minor-version compatibility、畸形/未知/超限 frame、确定性 descriptor、
deadline、cooperative cancellation、crash 分类、失败重试、SAFE_STATIC
symbolic-link 输入拒绝和有界 backpressure。真实 sandbox provider 隔离及其
身份绑定仍由 Trusted CI 独立验收。

Java PMD 与 SpotBugs representative path 仍由 Rust client integration test
单独覆盖；local Gate oracle 只消费二者共享的 typed BackendObservation
contract，不导入任一 engine。

完整控制台证据：\`fake-host-acceptance.log\`、\`node-fake-acceptance.log\`。
EOF

echo "acceptance=PASS checks=31 evidence=$OUT/phase2-acceptance.md"
