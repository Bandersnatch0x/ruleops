# ADR-0151: TrustState 目录同步失败必须 fail closed

- Status: Accepted
- Date: 2026-09-19
- Scope: `runtime/trust-state` atomic persistence boundary
- Decision record: GitHub issue #119
- Related: ADR-0144, issue #102

## Context

`TrustStateFile` 已对临时文件执行 `sync_all()` 并用同目录 rename 替换目标文件，但此前
忽略了目录打开或目录 `sync_all()` 的错误。这样调用方可能拿到表示“已持久化”的
`PersistedTrustState`，却没有证据证明 rename 已跨崩溃边界持久化。

## Decision

- 目录同步由可注入的 `sync_directory` seam 执行；rename 前和 rename 后的失败均返回明确
  `TrustStateStoreError::Io`，不返回 `PersistedTrustState`。
- rename 后目录同步失败时，保存逻辑尽力用已读取的旧 bytes 恢复旧目标；初始化失败则移除
  新目标。原始目录同步错误优先保留，回滚失败作为附加诊断，不得被临时文件清理覆盖。
- initialize 与 advance 都必须通过该边界；测试注入第一次/第二次目录同步失败，证明初始化
  不产生状态，推进失败仍可读取旧状态。
- 读写操作还必须持有同目录持久 lock file 上的内核 advisory `flock`；单个
  `TrustStateFile` 的进程内互斥不足以阻止不同句柄或不同进程在首次初始化时互相覆盖。
  lock file 不在 Drop 时删除，避免等待者切换到新 inode；进程退出后由内核释放锁。
- TrustedContext/Trusted CI 不改变其 opaque handle 要求；只有成功返回的持久化结果才可进入
  后续 handoff。

## Consequences

- 文件落盘成功不再等同于“目录 rename 已被确认持久化”；未知状态保持失败。
- 多一次目录同步带来小幅写入成本，但消除了静默吞错的持久化假设。
- 真实文件系统/磁盘故障仍由部署环境提供；本仓库只提供可复核、fail-closed 的本地边界。

## Verification

```text
cargo test -p erf-trust-state --all-targets
bash scripts/trust-checkpoint-check.sh .scratch/trust-checkpoint-check
bash scripts/check.sh
```
