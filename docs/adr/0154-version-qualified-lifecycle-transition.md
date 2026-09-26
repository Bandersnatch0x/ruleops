# ADR-0154: 生命周期转换按版本限定资源身份

- Status: Accepted
- Date: 2026-09-19
- Scope: RuleOps Server lifecycle mutation boundary
- Decision record: GitHub issue #116
- Related: ADR-0137, ADR-0138, issue #93

## Context

RuleOps 资源以不可变的 `type + id + version` 识别。同一 `id` 可以合法拥有多个
immutable revisions，但旧的 lifecycle transition 只按 `type + id` 找到第一条记录，
可能把 promote/revoke 作用到错误版本。HTTP 路由中的 `{id}` 不能替代 mutation 对
目标 `version` 的明确声明。

## Decision

- 生命周期请求使用 `POST /v1/{resource}/{id}/{promote|revoke}`，请求 JSON 只允许
  `version`；`id` 由路径提供，`type` 由资源路径提供。
- Store 将请求建模为 `VersionQualifiedLifecycleTransition`，先校验 type/id/version，
  再以完整 `Key(type,id,version)` 精确查找；不再使用按 id 的 `findFirst()`。
- 缺失或空白 version 返回 `RESOURCE_VERSION_REQUIRED`；非文本、未知字段或非法身份
  返回结构化 400；不存在的精确版本返回 `NOT_FOUND`。
- operation id 的幂等身份包含精确资源 key 与目标状态。同一 operation id 指向另一版本
  或另一目标状态时返回 `IDEMPOTENCY_CONFLICT`；相同请求在重启后重放原资源，不追加
  第二条审计事件。
- journal 仍保持 append-before-mutation；TRANSITION 记录的 request digest、来源状态、
  目标状态和完整资源身份共同参与回放校验。

## Verification

- 同一 release id 的 v1/v2 缺 version 被拒绝，指定 v2 只改变 v2。
- operation id 跨版本复用被拒绝。
- FileRuleOpsJournal 重启回放后 v1/v2 状态与审计链保持一致，原 operation 可幂等重放。
- `RuleOpsHttpServerTest` 与 `RuleOpsJournalTest` 均执行且无 skipped；`bash scripts/check.sh`
  是交付门禁。
