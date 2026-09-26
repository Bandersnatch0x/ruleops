# ADR-0152: Thin Client 只把已知 4xx 归为确定性拒绝

- Status: Accepted
- Date: 2026-09-19
- Scope: `runtime/thin-client` submission response classification
- Decision record: GitHub issue #118
- Related: ADR-0141, issue #97

## Context

ADR-0141 定义了 HTTP 4xx 为已知拒绝、连接/超时/5xx/未知响应为
`SUBMISSION_UNKNOWN`。原实现用“非 2xx 且小于 500”近似这个规则，因而把 301、307、308
等重定向当成 `SUBMISSION_REJECTED`。重定向并不能证明 Server 是否收到写入，客户端不能把
它变成确定性业务拒绝。

## Decision

- 只有状态码 `400..=499` 才映射为 `SubmissionRejected`。
- 3xx、5xx、1xx 或其他非成功状态均映射为 `SubmissionUnknown`；连接失败、超时、空响应、
  畸形 HTTP/JSON、缺少、空白或含控制字符的响应 `requestId`，或身份不匹配也保持 unknown。
- 响应 `apiVersion` 与有效的 `requestId` 必须先通过校验；即使状态码为 4xx，缺少或无效响应
  身份也不得生成 `SubmissionRejected`。
- `SUBMISSION_UNKNOWN` 不生成成功 receipt，不自动重试，不映射为批准、Gate PASS 或 SCM
  写入；真实结果只能由 Server/人工复核确认。

## Consequences

- 客户端不再把可能已经写入的重定向或服务端故障伪装成确定性拒绝。
- 4xx 仍保留结构化 status/code/message，便于用户修正请求；未知结果保留响应 request id
  供人工或服务端查询。
- 该边界只约束本地 HTTP contract seam；生产 TLS、重试策略和最终治理权仍属于部署/Server
  边界。

## Verification

```text
cargo test -p erf-thin-client transport::tests
bash scripts/thin-client-check.sh .scratch/thin-client-check
bash scripts/thin-client-submit-check.sh .scratch/thin-client-submit-check
bash scripts/check.sh
```
