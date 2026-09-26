---
status: accepted
date: 2026-09-19
---

# ADR-0147: 冻结增量 ScanPlan、缓存身份与完整性语义

## Context

`AffectedScope` 已经把 Git 变更转换为按 Backend 分开的最小安全闭包，但它本身不是
执行分母，也不能让一个缓存命中绕过 Completeness。快速反馈还需要复用结果，因而必须
把“计划是什么”“实际评估了什么”和“结果是否可进入权威 Gate”分开保存。

## Decision

`erf-scan-plan` 提供 `ScanPlan::freeze` 深模块。它只消费 `AffectedScope`、显式的
`AnalysisUnit` 清单和固定身份，冻结三种模式：

- `FULL`：所有输入单元均为计划单元；
- `MODULE`：按 AffectedScope 的模块闭包选择单元，图闭包已证明时可进入权威 Gate；
- `INCREMENTAL_PREVIEW`：优先使用 PMD AST 的文件候选，显式记录未评估单元，永远只能
  产生非权威 preview。

计划 digest 绑定 repository/base/head/tree、scope、RuleBundle、Policy、Resolver、
Backend、Runner、协议与 CanonicalFindingSet 身份，以及排序稳定的计划单元和图摘要。
scope 不确定时计划自动升级到 `FULL`；Host 只能执行计划已列出的 unit id，不能重写
分母。

`CompletenessReport` 同时保留计划模式、覆盖状态、已评估/缺失/未评估集合、结构化原因
和 Gate `Completeness` 映射。缺失、失败、unsupported、not-applicable、观察错误或
preview 未评估范围均不能形成权威 PASS。

结果缓存使用 `scan-cache-key/v1` 内容寻址键，绑定 plan digest、SemanticResultDigest、
Baseline、Exception、UnifiedResult 和可选 TrustedContext 身份。`FileScanCache` 只保存
摘要与完整性摘要，不保存源码或完整 Finding；重启后按 key 重新读取，并再次校验全部
输入身份、有效期、计划 digest 和权威条件。损坏、过期、身份不匹配、计划不完整或
preview PASS 均 fail closed。

rf scan --base/--head 已接入该边界：Runner 在执行前做有界计划级 probe，CLI 在
Gate 归一化之后写入结果摘要。命中不会跳过 Backend 执行，也不会把摘要误当作 Finding；
过期或损坏会把本次路由升级为 FULL。

## Consequences

- Runner 接收冻结后的计划并把 planned unit ids 交给 Host；Host refinement 不能新增、
  删除或合并分母。
- cache hit 只表示可复核摘要可重用，不自动授予签名、Gate 或 SCM 权力。
- 快速 preview 和权威 Gate 的结果边界可由结构化 coverage/reason 解释，而不是依赖
  “0 findings”。
- 缓存、计划和结果绑定保持 transport-neutral；源码上传、SCM 写入和生产凭证不在本票。

## Verification

```text
cargo test -p erf-scan-plan
bash scripts/scan-plan-check.sh .scratch/scan-plan-check
bash scripts/check.sh
```
