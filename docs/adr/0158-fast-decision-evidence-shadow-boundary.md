# ADR-0158: Fast Decision Evidence 影子分流边界

- Status: Accepted
- Date: 2026-09-20
- Scope: Provider-neutral typed fast-decision evidence for diagnostic routing
- Decision record: GitHub issue #122

## Context

RuleOps 已有确定性的 `ScanPlan`、结构化诊断、digest-only `FindingView` 和独立
Gate authority，但高频诊断分流仍缺少可比较的快速决策观察 seam。快速决策结果
可能来自不同 provider，概率或置信度不能直接被当作正确性或 Gate 结果。

## Decision

新增 `runtime/fast-decision` 与 `fast-decision-v1` contract，支持 `CHOICE`、
`SCORE`、`BOOLEAN` 三种类型化问题。`FastDecisionAdapter` 只接收 digest/metadata
输入并返回 `FastDecisionEvidence`：decision spec/version/digest、provider/model
身份、typed answer、probability/confidence/margin、threshold policy、fallback、
created/expiry 和 canonical evidence digest 均必须绑定。

内置 `DeterministicBaselineAdapter` 用于可复跑的 baseline/holdout/near-miss；外部
provider 是可选 adapter，不是权威依赖。置信度不足、margin 不足、输入过期、答案
冲突或 provider 不可用统一返回 `UNKNOWN`，并回退既有确定性路径或人工复核。影子
比较记录 disagreement、abstain、latency 和 calibration，但不改变现有决策。

该层永远不能生成 Gate PASS/FAIL、批准 Exception、改变 TrustState、自动重试或写
入 SCM。`rf decision evaluate` 仅输出 digest-only 影子 evidence。

## Verification

```text
cargo test -q -p erf-fast-decision --all-targets
bash scripts/fast-decision-check.sh .scratch/fast-decision-check
bash scripts/check.sh
```
