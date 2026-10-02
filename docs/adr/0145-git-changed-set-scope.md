---
status: accepted
date: 2026-09-19
---

# ADR-0145: 冻结 Git ChangedSet 与提交身份 seam

## Context

增量扫描不能把隐式的 `HEAD~1`、不完整的浅克隆或未经复核的文件集合当作
`ScanPlan` 分母。后续受影响模块闭包和增量计划需要一个独立、确定性、与 SCM
写入无关的 Git 变更范围事实。

## Decision

`erf-git-scope` 提供一个深模块接口：调用方显式构造 `GitChangeRange(base, head)`，
再交给只读 `GitChangeResolver`。实现按以下顺序工作：

1. 验证仓库可用，并在任何 revision/diff 工作前拒绝未合并的 index/worktree；
2. 拒绝 shallow repository；
3. 将 base/head 解析为完整 commit object id；
4. 使用唯一 `merge-base` 作为比较起点；无共同祖先或多个共同祖先均失败；
5. 读取 `git diff --name-status -z`，识别 A/M/D/R/C/T，U/B 等冲突状态拒绝；
6. 归一化 repository-relative path，拒绝绝对路径、Windows drive/backslash、`..` 和非法编码；
7. 以固定排序和版本化 length-delimited canonical bytes 产生 ChangedSet digest。

ChangedSet 包含 resolved base/head/merge-base 身份、稳定排序的路径变更、rename/copy
old/new path 与 similarity。空差异是有效的空集合，但仍绑定完整提交身份。解析失败不
退化为全仓或空范围。

## Consequences

- #104 可以只消费 ChangedSet，不重复解释 Git 状态或路径安全规则；这是后续受影响闭包
  的外部 seam。
- 该模块不写 Git、不访问远程 SCM、不上传源码、不决定 Gate，也不实现依赖闭包、缓存或
  最终增量 ScanPlan；这些由后续票负责。
- 多 merge-base、浅克隆、缺失 revision、冲突状态和路径不确定都会保守失败，避免漏扫。

## Verification

```text
cargo test -p erf-git-scope
bash scripts/git-change-scope-check.sh .scratch/git-change-scope-check
bash scripts/check.sh
```
