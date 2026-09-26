# ADR-0141: Thin Client 到 RuleOps Server 的申请提交 transport seam

- Status: Accepted
- Date: 2026-09-19
- Scope: V0.3 Thin Client / V0.2 RuleOps Server request submission
- Decision record: GitHub issue #97

## Context

ADR-0140 固化了 Thin Client 的闭合 `FindingView`、反馈草稿和例外申请，但刻意没有实现
Server submission。RuleOps Server 已有 `/v1/feedback`、`/v1/exceptions`、版本化 envelope、
幂等键、审计上下文和 journal-before-mutation；缺少的是把 digest-only request 安全送入这条
权威边界的可复用 transport。

## Decision

### 1. Explicit endpoint and closed input

`rf feedback submit` 与 `rf exception submit` 必须显式指定 `--server URL`，只接受 #96
生成或通过同样闭合校验的 request artifact。客户端拒绝未知字段、源码/完整 Finding、错误
版本/状态、非法摘要、非法相对 scope 和自摘要不匹配的 artifact；不会自动发现 Server 或
自动重试。

当前 transport 只提供 `http://host:port/base` 的本地 contract seam；生产 TLS 属于部署边界，
不能由这一层默认为已安全。

### 2. Server authority and identity

客户端把申请作为 `/v1/feedback` 或 `/v1/exceptions` 的 `id + version + payload` envelope
发送，并绑定 `Idempotency-Key`、`X-Request-Id`、`X-Actor`、`X-Reason`。Server 对申请
payload 执行闭合 schema、digest-only、status/kind/version、request identity、actor/reason
一致性与相对 scope 校验，然后沿用 `RuleOpsStore` 的幂等、不可变身份、journal-before-
mutation 和审计链。

### 3. Receipt is not approval

成功响应只生成 `SubmissionReceipt`：`SUBMITTED` 或 `REPLAYED`，治理结果为
`RECEIVED_ONLY`。Receipt 显式携带 `exceptionApproved=false`、`gateMutated=false`、
`scmPublished=false`；HTTP 4xx 是已知拒绝，连接/超时/5xx/未知响应是
`SUBMISSION_UNKNOWN`，客户端不把任何一种结果渲染成批准或 Gate PASS。

## Consequences

- 本地 CLI 可以把申请送入既有 Server/journal 边界，重复请求可安全重放。
- 客户端仍是 UX/transport client，不复制治理 authority，也不接触源码或完整 Finding。
- 真实 TLS、IDE UI、ExceptionGrant 审批、Gate/SCM 写入与远程 registry 仍需独立部署/产品票。
- UNKNOWN 提交后的只读恢复查询由 ADR-0160 冻结，不改变本 ADR 的 receipt/authority 语义。

## Verification

```text
cargo test -p erf-thin-client
mvn -q -pl control-plane/server -am test
bash scripts/thin-client-check.sh .scratch/thin-client-check
bash scripts/server-contract-check.sh .scratch/server-contract-check
bash scripts/check.sh
```
