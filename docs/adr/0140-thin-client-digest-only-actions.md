# ADR-0140: 以 digest-only 动作固化 V0.3 Thin Client 边界

- Status: Accepted
- Date: 2026-09-19
- Scope: CLI/IDE Thin Client UX contract for V0.3
- Decision record: GitHub issue #96

## Context

架构计划 §40.3、§48、§49 与 §51 将 CLI/IDE 定义为 UX Client：调用 `rf-runner`、展示
Finding、导航本地源码位置、解释、报告误报和请求例外；Client 不维护独立 PMD/SpotBugs
Runtime，也不拥有 RuleOps Server、SCM、Registry 或 Gate authority。当前 `./rf` 已有
authoring、pilot 和 scan 入口，但缺少可复用的 Finding View 与反馈/例外 request seam。

## Decision

### 1. FindingView 与隐私

`FindingView` 使用 `rulefactory.io/v1`，闭合字段包括 Finding identity、rule id、
fingerprint、message 和 repository-relative allowlisted location。输入拒绝未知字段、
绝对路径、跨目录 `..` 和 source/sourceSnippet/fullFinding 等隐私字段；FindingView
自身计算稳定 digest，Client 不接收完整引擎报告。

### 2. Thin Client actions

CLI 暴露 `rf explain`、`rf feedback draft` 与 `rf exception request`。前者只输出摘要、
相对位置和可用动作；后两者生成确定性的 `GovernanceRequestDraft`：Feedback 状态为
`PROPOSED`，Exception 状态为 `REQUESTED`，均带 request id、Finding digest/fingerprint、
actor、reason 和自身 digest。它们不提交 Server、不批准 ExceptionGrant、不改变 Gate。

真实 IDE 可复用同一 contract 实现 invoke runner、show/navigate、explain、feedback 和
exception action；本票不伪造 IntelliJ/VS Code UI，也不复制 Runtime。

### 3. Authority separation

`release`/`gate` 以及 feedback submission、exception approval 继续在 Client 外部处理。
Thin Client 的 draft/request 是可审计、可重放的输入，不是治理结果；Server 或人工流程
必须重新校验请求和权限。

## Consequences

- CLI 与未来 IDE 有统一的隐私、定位和请求身份语义。
- 本地开发者可立即得到 explain/申请体验，同时不会把 UX 入口误当成治理 authority。
- 下一步可在不改变 Runtime/Server contract 的情况下接入真实 IDE adapter 和 Server request
  submission；人工 Shadow 窗口仍是独立外部边界。

## Verification

```text
cargo test -p erf-thin-client
bash scripts/cli-surface-check.sh .scratch/thin-client-cli-surface-check
bash scripts/thin-client-check.sh .scratch/thin-client-check
bash scripts/check.sh
```
