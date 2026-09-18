# ADR-0123: 固化 Pilot Metrics Baseline 的定义边界

- Status: Accepted
- Date: 2026-09-18
- Scope: Phase 0/Phase 4 pilot measurement definition

## Context

Phase 0 要求 Pilot Metrics Baseline。仓库已有 digest-only 的
`erf-pilot-metrics` ledger 与可复放 journal，但只有实现测试和 Phase 4
报告，没有一份独立的指标定义记录。缺少定义会让未来 observation window
中的“缺数据”“不完整执行”和“没有发生事件”混淆，也可能把 fixture 结果
误报成产品结果。

## Decision

以 `specs/pilot/pilot-metrics-baseline.yaml` 固定 11 个机器可观测指标，分别
绑定 run、feedback 或 governance journal record 的字段、单位、聚合和
缺测行为。所有指标采用 digest-only disclosure；缺数据或非法数据为
`INCONCLUSIVE`，不得转为 PASS。

该基线只定义测量，不填写 observation window、sample、acceptance threshold
或 live outcome。author-active-time、precision、recall 与 live pilot outcome
保留为未定义/人工输入，不从本地 fixture 推导。

## Consequences

- 未来适配器可以按固定字段收集同口径数据，且重启后可从本地 journal 复放。
- 指标定义与 PilotSuccessContract 的人工批准边界分离，避免倒推阈值。
- 当前证据仍是 seam/definition evidence，不是生产效果、代表性或保护分支
  授权。

## Verification

运行 `scripts/pilot-metrics-baseline-check.sh .scratch/pilot-metrics-baseline`
并以 `bash scripts/check.sh` 作为全量门禁。
