---
status: accepted
date: 2026-09-19
---

# ADR-0146: 冻结 Java AffectedScope 与 backend 最小安全闭包

## Context

`ChangedSet` 只是 Git 候选事实，不能直接成为 Java 增量扫描分母。Maven/Gradle
模块、反向依赖、module descriptor、共享构建配置和生成源会让未修改文件受到变更
影响；依赖图或生成入口无法证明时，窄范围会产生漏扫风险。

## Decision

`erf-affected-scope` 提供只读的 `AffectedScopeResolver`。调用方把 #103 产生的
`ChangedSet` 和 repository root 交给它；模块内部以受限静态元数据解析 Maven/Gradle
项目，生成排序稳定、带 digest 的三类 backend scope：

- `PmdAst`：纯 Java source change 可保留 `FILE_CANDIDATE`；module descriptor、资源或
  构建变化升级为 `MODULE`；
- `PmdTypeResolution`：对 seed module 计算反向依赖闭包，至少为 `MODULE`；
- `SpotBugsBytecode`：同样使用反向依赖闭包，并记录 bytecode/classpath 的 module
  边界，至少为 `MODULE`。

任何 backend scope 的共同图事实无法证明时升级为 `FULL`，包括未识别或混合构建系统、
动态 Gradle include、`projectDir` 重定向、缺失 module metadata、生成源/自定义 source
set 不确定、共享构建配置、lockfile、rule/policy/resolver 配置和无法映射的变更路径。
删除或跨模块重命名无法从当前快照复原旧模块身份时同样 `FULL`。

Scope digest 绑定 ChangedSet digest、静态构建图摘要、变更路径、受影响模块、各 backend
level/path/reason 与版本域；相同输入必须产生相同结果。

## Consequences

- #105 可以只消费 `AffectedScope` 冻结增量 `ScanPlan` 与缓存键，不重复解析 Git、模块
  边界或依赖闭包。
- Resolver 不执行 Maven/Gradle、生成器或分析 backend，不写 SCM、不上传源码、不决定
  Gate，也不实现缓存或最终增量计划。
- `FILE_CANDIDATE` 只属于 PMD AST 的窄候选面；需要类型解析或字节码的 backend 不能
  把文件候选冒充模块闭包。

## Verification

```text
cargo test -p erf-affected-scope
bash scripts/affected-scope-check.sh .scratch/affected-scope-check
bash scripts/check.sh
```
