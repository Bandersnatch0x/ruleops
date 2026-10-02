# ADR-0137: 固化 V0.2 RuleOps Server 的版本化 API seam

- Status: Accepted
- Date: 2026-09-19
- Scope: RuleOps Server domain/API boundary for V0.2
- Decision record: GitHub issue #93

## Context

架构计划 §28、§40.1 与 §50 要求 RuleOps Server 承担 Control Plane 的标准、覆盖、
Factory、Verification、Registry、Policy、Feedback、Exception 和 Audit 能力，但不把
服务端变成源码扫描器，也不提前授予 SCM 或 protected-branch 权限。现有能力分布在
Java Control Plane 与 Rust Runtime 中，需要先形成可替换、可测试的稳定 API seam。

## Decision

### 1. 版本化资源边界

V0.2 提供 `rulefactory.io/v1` 的 HTTP/JSON domain API，资源边界对应
Standards、Coverage、Rules、Factory/Verification Jobs、Releases、Policies、Scans、
Feedback、Exceptions、Findings digest view 和 Audit。每次写入使用封闭的
`id + version + payload` envelope；资源身份和内容摘要分离记录，版本或内容变化产生
新的不可变身份，不覆盖已存在资源。

### 2. 写入与错误语义

Mutating request 必须携带 `Idempotency-Key`、`X-Actor` 和 `X-Reason`。相同 key 与相同
请求摘要只重放原结果；相同 key 的不同请求、已存在身份的不同内容、非法状态转换和
多版本的无版本读取均拒绝。每次实际状态变化追加带 sequence、previous digest、actor、
reason 和 operation id 的审计事件。

API 错误保持结构化 `apiVersion/code/message/requestId`；不完整、冲突、过期或隐私违规
输入不得被映射为成功。默认 Privacy Profile 拒绝源码、源码片段和完整 Finding 上传，
Finding 查询只返回 digest-only 视图。

### 3. 适配器与权限边界

本票提供可替换的内存 adapter 和 contract tests，用来冻结资源、幂等、审计、生命周期
和隐私语义；后续持久化 adapter 必须保持同一 port。API 不执行扫描、不持有 SCM 写凭据、
不发布 SCM 状态、不改变 `CIAuthorityStage`，也不把 API 验收伪装成生产 Registry。

## Consequences

- V0.2 的 Web/API 可以与现有 Java modular monolith 对齐，同时保留 Rust Runtime 的
  确定性执行职责。
- Server persistence、Bundle distribution/cache、认证授权和真实任务执行可以在不改动
  API 资源语义的情况下逐步接入。
- API adapter 的内存状态只证明 contract seam；生产 durability、恢复和多写入一致性
  必须由后续票单独验收。

## Verification

```text
mvn -q -pl control-plane/server -am test
bash scripts/server-contract-check.sh .scratch/server-contract-check
bash scripts/check.sh
```
