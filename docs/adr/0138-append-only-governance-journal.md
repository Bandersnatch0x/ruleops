# ADR-0138: 使用 append-only journal 固化 RuleOps 治理状态的恢复边界

- Status: Accepted
- Date: 2026-09-19
- Scope: RuleOps Server V0.2 persistence adapter
- Decision record: GitHub issue #94

## Context

ADR-0137 只冻结了 RuleOps Server 的 domain/API seam；内存 adapter 在进程重启后会丢失
资源、生命周期与审计状态，不能作为 V0.2 的可恢复边界。架构计划 §40.1、§42 与 §43
要求治理资源具有可验证身份、可追溯变更和明确的失败语义，但当前阶段尚未引入数据库、
集群共识或远程存储。

## Decision

### 1. Port 与本地 adapter

RuleOpsStore 通过 `RuleOpsJournal` port 接入 append-only 记录；`FileRuleOpsJournal` 以
UTF-8 JSON Lines 作为本地 Shadow/Pilot adapter。每条记录包含资源类型、不可变
`id + version`、资源摘要、请求摘要、幂等 operation id、actor、reason、状态转换、
序号、前链摘要、事件摘要和记录时间。

持久化顺序为：校验请求 → 生成审计事件 → 强制追加并 flush journal → 提交内存状态。
追加失败时不提交内存 mutation；现有 HTTP 资源和错误语义保持不变。内存构造器继续
保留，便于 contract test 和无持久化的本地开发。

### 2. 回放与 fail-closed

带 journal 启动时逐条回放。回放必须验证序号连续、前链摘要、资源/请求摘要、事件摘要、
幂等 operation id 唯一、创建初态和允许的生命周期转换；任一截断尾项、空项、非法 JSON、
摘要不一致或隐私越界均以启动错误结束，不得静默丢弃尾部。

本地适配器限制单条记录和总文件大小；它不承诺跨进程并发写入、数据库事务、集群恢复、
远程备份或保留期清理。这些能力必须在后续 adapter/发行治理票中单独定义。

### 3. 运行入口与权限边界

`RuleOpsServerMain` 的第二个参数可指定 journal 路径，重启即可恢复同一治理状态。该
入口仍只提供 V0.2 domain/API seam，不执行扫描、不接收源码或完整 Finding、不持有 SCM
写权限，也不把本地 journal 伪装成生产 Registry 或签名信任根。

## Consequences

- V0.2 server 在单机 Shadow/Pilot 生命周期内具备可验证的恢复和审计连续性。
- journal 是事实记录而不是可随意编辑的缓存；损坏会阻断启动，运维必须修复或替换完整
  受信副本。
- 后续数据库、OCI bundle、缓存和多节点部署可以复用相同的资源/审计 port，但必须保留
  fail-closed 与隐私默认值。

## Verification

```text
mvn -q -pl control-plane/server -am test
bash scripts/server-contract-check.sh .scratch/server-contract-check
bash scripts/check.sh
```
