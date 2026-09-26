---
status: accepted
date: 2026-09-19
---

# ADR-0144: 将持久化 TrustCheckpoint 接入可信 handoff

## Context

架构计划 §43、ADR-0066、ADR-0094 与 ADR-0136 要求离线 Runtime 保留最高已见的
TrustCheckpoint 版本，并在重启、撤回或状态不确定时 fail closed。仓库已有
`erf-trust` 的签名/单调性校验与 `erf-trust-state` 的有界原子文件存储，但若可信
handoff 只调用内存校验，库级能力仍可能没有进入实际 TrustedContext。

## Decision

### 1. 显式状态生命周期

`TrustStateFile` 提供三条有意不同的路径：

- `initialize_signed_checkpoint` 只用于首次安装信任地板；已有文件时拒绝覆盖；
- `advance_signed_checkpoint` 必须先加载已有状态，再验证签名、时效、仓库和角色版本单调性，
  成功后才使用 fsync + 同目录原子替换；
- `load_for_trusted_execution` 只读取已有状态，缺失、损坏或不确定都返回错误。

原始 `TrustState` 的文件写入实现不是公开操作；外部调用方只能通过上述生命周期入口产生
`PersistedTrustState`，不能绕过单调性检查重置回退地板。

状态文件不是信任来源。签名角色、DSSE payload、checkpoint 内容和版本规则仍由
`erf-trust` 校验；写入失败不得被当作已接受。

### 2. TrustedContext 入口

可信入口使用 `verify_with_checkpoint`，要求调用方同时提供签名 checkpoint 和由
`TrustStateFile` 产生的 `PersistedTrustState` opaque handle。该 handle 没有公开构造器，
因此调用方不能用 `TrustState::default()` 或内存突变伪造“已持久化”状态。验证器会重放
checkpoint 到 handle 的只读状态副本；若重放会改变状态，说明调用方尚未先持久化，直接拒绝。
`erf-trusted-ci::TrustedCiInputs` 将这些字段设为必需，handoff 不得静默退回没有 checkpoint
的可信路径；真实演练会在写入后重新从文件加载 handle 再进入 handoff。

现有 `verify` 仍作为 local/test 兼容 helper，不能单独产生 Trusted CI handoff 或 SCM
authority。该票不改变 Gate 三态、Status Publisher、SCM、KMS/HSM 或 protected branch
边界。

## Consequences

- Trust floor 可跨进程、跨重启复核，checkpoint rollback、role rollback、篡改、过期和
  状态缺失不会被转成 PASS。
- 首次部署与正常可信执行的权限边界清晰，scan 过程不能自行 bootstrap 或推进 trust。
- 真实 Shadow 窗口、外部镜像和生产密钥仍需人工/部署边界，不由本 ADR 虚构完成。

## Verification

```text
cargo test -p erf-trust-state -p erf-trusted-context -p erf-trusted-ci
bash scripts/trust-checkpoint-check.sh .scratch/trust-checkpoint-check
bash scripts/check.sh
```
