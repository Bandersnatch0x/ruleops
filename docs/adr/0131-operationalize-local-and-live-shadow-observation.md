---
status: accepted
---

# 固化本地模拟与真实只读 Shadow 观测的双模式

当前 Pilot 需要在不向外部 GitHub 仓库写入、不持有 SCM 凭据的前提下，先验证观测链路，再执行真实默认分支的 Shadow 采集。此 ADR 先定义了 `LOCAL_SIMULATION` 与 `LIVE_SHADOW` 的边界；后续 ADR-0156 在两者之间补充 `SYNTHETIC_SHADOW` 作为内部合成资格验证模式，并要求三种模式按登记身份独立聚合。

## Consequences

- 本地提交可安全地覆盖新增违规、修复、重放和失败处理场景。
- 真实 Shadow 只需要公共仓库的只读获取能力，仍不发布 SCM 状态。
- Observation Window 的缺失数据、执行不完整或未测指标继续按 `INCONCLUSIVE` 处理。
- protected-branch authority、源码上传和完整 Finding 传输仍不属于本阶段。
