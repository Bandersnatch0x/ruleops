---
status: accepted
---

# ADR-0156：固化三种 Pilot 观察模式的身份与聚合边界

`LOCAL_SIMULATION`、内部合成资格验证与登记外部仓库的 `LIVE_SHADOW` 都可以
调用同一个 Observer，但它们不是同一种产品证据。决定将内部合成路径命名为
`SYNTHETIC_SHADOW`，并把 `ObservationMode`、repository identity、commit
identity、rule-bundle digest 与 run identity 固定写入每条运行测量、journal 和
window report。

window report 必须显式选择一个模式；`LIVE_SHADOW` 必须携带登记样本并要求
观察到的 repository/commit 集合与登记集合、冻结的 rule-bundle digest 完全
相等。登记缺失、commit 漂移、bundle 漂移、模式错配或空模式结果均 fail
closed。报告只聚合所选模式，内部两种模式永远
不能满足产品结果；它们最多产生工程门禁 `PASS`，产品结果仍为
`INCONCLUSIVE`。

## Consequences

- 合成仓库的 qualification evidence 与外部产品 Pilot 结果不能通过同一个
  无模式聚合入口互相冒充。
- journal 仍为 digest-only；报告可以展示固定的身份摘要而不上传源码或完整
  Finding。
- `LIVE_SHADOW` 获取登记 commit 后必须再次核对实际 checkout，commit 漂移
  不会被当作新的产品样本。
- `LOCAL_SIMULATION` 与 `SYNTHETIC_SHADOW` 的正例和错配负例可以在工程门禁
  中验证；人工质量裁决与治理决定仍属于后续 Pilot 票据。
