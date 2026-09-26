# ADR-0155: 在生成 ChangedSet 前拒绝未合并工作树

- Status: Accepted
- Date: 2026-09-19
- Scope: `erf-git-scope` ChangedSet resolution boundary
- Decision record: GitHub issue #115
- Related: ADR-0145, issue #103

## Context

ChangedSet 的输入必须是可证明的提交范围。真实 merge/rebase conflict 会把多个
stage 写入 index，并让 worktree 处于未合并状态；此时仅对两个 commit 执行
`git diff` 可能产生看似合法但无法代表当前工作状态的范围。仅在 name-status
parser 接收 `U/B` token 也不能覆盖真实冲突工作树。

## Decision

- `GitChangeResolver::resolve` 在仓库检查之后、shallow/revision/merge-base/diff
  之前调用 `reject_unmerged_worktree`。
- 先用 `git ls-files --unmerged -z --` 检查 index，再用 `git diff --name-only
  --diff-filter=U ... -z --` 检查 worktree。任一命中都返回结构化
  `GitChangeScopeError::UnmergedWorktree`，不产生 `ChangedSet` 或 digest。
- 该边界只读 Git；不写 index/worktree，不访问远程 SCM，不退化为全仓或空范围。冲突
  详情不把路径内容放进错误身份，避免把工作树信息扩散到结果 digest。
- parser 层继续拒绝输入流中的 `U/B`，作为防御性边界；它不能替代真实仓库状态检查。

## Verification

- 集成 fixture 在临时 Git 仓库中实际制造 conflicting merge，并实际制造 conflicting
  rebase；resolver 在 revision resolution/diff 前返回 `UnmergedWorktree`。
- 既有干净提交范围、空差异、浅克隆、缺失 revision、多 merge-base 和路径安全测试
  保持通过。
- `bash scripts/git-change-scope-check.sh` 记录全量测试与真实 merge/rebase fixture
  的独立命令日志；最终交付仍由 `bash scripts/check.sh` 门禁。
