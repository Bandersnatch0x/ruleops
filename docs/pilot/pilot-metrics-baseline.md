# Pilot Metrics Baseline

本基线定义后续 Shadow pilot 的可观测指标、数据来源、聚合方式与缺测语义，
但不填入 live pilot 的结果。当前范围为 Java、100 条 Standard inventory、
20 条候选 RuleIntent 与 3 个 local Shadow repository identity。

指标从 digest-only observation journal 读取：

- Run：new finding、historical debt、complete/inconclusive、latency、peak
  memory 与 false-pass injection；
- Feedback：false-positive regression outcome；
- Governance：rollback、active exception count 与 oldest exception age。

每项指标都固定 record、field、unit、aggregation、missing-data behavior 与
disclosure。缺数据、非法数据或不完整执行只能进入 `INCONCLUSIVE`，不能产生
PASS。输出仍只含 digest、计数、状态和资源摘要，不上传 source、path 或
完整 Finding。

本基线明确保留人工边界：observation window、sample、acceptance thresholds
仍为 `HUMAN_INPUT_REQUIRED`；author-active-time、precision、recall 与 live
pilot outcome 不从 fixture 或测试结果推导。它与
`specs/pilot/pilot-success-contract.yaml` 配合使用，不能替代人工批准。

机器可读主记录：`specs/pilot/pilot-metrics-baseline.yaml`。
验收入口：`scripts/pilot-metrics-baseline-check.sh`。
