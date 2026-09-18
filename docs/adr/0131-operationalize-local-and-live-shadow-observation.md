---
status: accepted
---

# 固化本地模拟与真实只读 Shadow 观测的双模式

当前 Pilot 需要在不向外部 GitHub 仓库写入、不持有 SCM 凭据的前提下，先验证观测链路，再执行真实默认分支的 Shadow 采集。决定将两者明确建模为 `LOCAL_SIMULATION` 与 `LIVE_SHADOW` 两种 `ObservationMode`：本地追加提交只用于验证趋势、回放、反馈和窗口汇总；只有登记清单中的外部仓库只读提交可以进入 live pilot 结果。两种模式共享同一个 Observer 和 digest-only journal，但 mode 会进入运行记录身份，避免把模拟结果伪装成产品结果。

## Consequences

- 本地提交可安全地覆盖新增违规、修复、重放和失败处理场景。
- 真实 Shadow 只需要公共仓库的只读获取能力，仍不发布 SCM 状态。
- Observation Window 的缺失数据、执行不完整或未测指标继续按 `INCONCLUSIVE` 处理。
- protected-branch authority、源码上传和完整 Finding 传输仍不属于本阶段。
