<!-- scaffold-stamp: agent-scaffold/templates/agents/issue-tracker.md@017b06f -->

# 票据惯例

本仓票据路由：GitHub Issues，使用 `gh` CLI。票与解票地图存于远端议题；后端命令明细只住本文档。

## 话术 → 动作映射

- 「发布到 tracker」「开个票」：`gh issue create --title "..." --body "..."`。
- 「取相关票」「看看某票」：`gh issue view <number> --comments`。
- 「记到票上」「留个记录」：`gh issue comment <number> --body "..."`。
- 「关票」「这票完了」：`gh issue close <number> --comment "..."`。
- 「有哪些开着的票」：`gh issue list --state open` 并按条件过滤。

- 从 `git remote -v` 确认仓库；在仓根执行时 `gh` 自动推断仓库。

## 约定

- 票只追加不改写：评论是带日期的 Log 条目；关票前先把决议写入 Log。
- 「须人工」票先向用户提出一个决策问题，拍板后再实施。
- PR 不作为需求分诊入口；必要时用对应的 PR 查看、评论、标签与关闭命令处理。

## 来票分诊（triage）

| 分诊角色 | 含义 | 本仓标签 |
| --- | --- | --- |
| 待评估 | 新来件，尚未分诊 | `needs-triage` |
| 待补信息 | 复现或需求信息不足，等回帖 | `needs-info` |
| 代理可接 | 界定清楚，代理可直接领做 | `ready-for-agent` |
| 须人工 | 需用户拍板或人工操作 | `ready-for-human` |
| 不修 | 决议不处理，理由入 Log | `wontfix` |

## 地图式解票操作

- 地图是一张总览票，记录 Notes、Decisions-so-far 与 Fog；子票通过归属边挂在地图下，阻塞边声明先后。
- 前沿是开放、无未闭阻塞且未认领的子票，按图序取首。
- 认领是会话对票的首次写入：`gh issue edit <number> --add-assignee @me`。
- 解决时追加答案 Log、关闭票，再把上下文指针回写地图的 Decisions-so-far。
- 有原生依赖能力时，用 `gh api` 写入依赖边；否则在子票正文顶部写 `Blocked by: #<n>`。
