# ADR-0157: PilotEvidencePackage 与 PilotAdjudication 运行时边界

- Status: Accepted
- Date: 2026-09-20
- Scope: Phase 4 Shadow-only pilot evidence aggregation and adjudication
- Decision record: GitHub issue #113

## Context

ADR-0133 固定了 digest-only `PilotEvidencePackage` 和人工
`PilotAdjudication` 的产品边界，但此前只有分散的局部指标记录。局部门禁通过
不能证明跨 author-active-time、quality、operations、outcomes 和 security 的
证据包完整，也不能安全地产生 `GO`。

## Decision

新增 `runtime/pilot-evidence`，以不可变的 canonical payload 生成证据包摘要，并
提供 `rf pilot evidence package` 与 `rf pilot evidence adjudicate` 两个入口。
证据包必须绑定批准合同 digest、登记样本 digest、观察窗口和固定的必需指标集合。
每条记录只保留来源身份、窗口、完整性、状态和 artifact digest；不接收源码、完整
Finding、SCM 状态或凭据。

当前批准合同 `pilot-success-contract-v0@1.0.0` 的登记窗口固定为
`2026-10-01T00:00:00Z` 至 `2026-10-31T23:59:59Z`（UTC，含首尾）；合同
id/version、窗口和两个外部 digest 都必须在裁决前匹配。

聚合严格使用 `FAIL > INCONCLUSIVE > PASS`。缺失、非法、冲突、篡改或不完整输入
只能得到 `INCONCLUSIVE`；已知 `FAIL` 优先于缺失数据。只有已封存且合同/样本 digest
均匹配、全部必需指标完整且为 `PASS` 的包，才可由固定的
`platform-engineering` 治理负责人生成 `GO`。`NO_GO` 和 `INCONCLUSIVE` 不产生
受保护分支权限，也不改变已批准合同。

## Verification

```text
cargo test -q -p erf-pilot-evidence --all-targets
bash scripts/pilot-evidence-check.sh .scratch/pilot-evidence-check
bash scripts/check.sh
```
