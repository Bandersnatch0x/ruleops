# ADR-0165: 冻结 governance requestId 的内容寻址算法

- Status: Accepted
- Date: 2026-09-24
- Scope: Thin-client↔Server governance request identity (feedback / exception)
- Decision record: GitHub issue #121

## Context

Thin-client 与 Server 的 contract 一直声明 governance request（FeedbackDraft /
ExceptionRequest）的 `requestId` 是 content-addressed，但从未冻结它的 canonical 输入、
字段顺序、版本参与方式与摘要计算式。历史实现里客户端用长度前缀的字节拼接
（`digest_parts`）生成 `requestId`，Server 端只校验其格式并要求它等于传输 envelope 的
`id` 与 `Idempotency-Key`，从不独立复算。结果是：一个格式正确但与请求内容无关的
`requestId` 能被 Server 接受，request identity 与它所声称承载的 envelope 内容脱钩，
replay/幂等与审计链的内容寻址前提无法被任一端独立验证。

治理负责人已裁决（#121，三项均取推荐）：`requestId` 由版本化、闭合的 canonical request
envelope 计算 SHA-256；`schemaVersion` 作为 envelope 字段参与摘要；canonicalization 采用
递归 key 字典序排序（沿用仓库既有的 canonical JSON + sha256 self-digest 约定）。

## Decision

冻结 `requestId` 内容寻址算法为：

```text
envelope   = requestPayload 去除派生自字段 { requestId, digest } 后的全部字段
requestId  = "sha256:" + hex( SHA-256( canonical(envelope) ) )
canonical  = 递归按 object key 字典序排序，array 保序，UTF-8 编码
```

`schemaVersion`、`kind`、`apiVersion`、`status`、`findingDigest`、`findingFingerprint`、
`ruleId`、`classification`(feedback)/`scope`(exception)、`reason`、`actor` 都进入 envelope
并参与摘要；`requestId` 与 `digest` 两个派生字段不参与自身来源。`digest` 自字段仍按既有
约定为 `sha256(canonical(payload 去除 digest))`（因此 `digest` 覆盖 `requestId`，
`requestId` 不覆盖 `digest`）。`requestId` 同时充当传输 envelope 的 `id` 与
`Idempotency-Key`，三者必须一致。

Client 与 Server 各自独立复算并交叉校验，复用同一 canonical 序列化器，因此对同一 envelope
产出 byte-identical 的 `requestId`。任意格式正确但与 envelope 复算结果不匹配的 `requestId`
一律 fail-closed 拒绝：客户端 `from_json` 返回 `RequestIdMismatch`，Server 返回
HTTP 400 `REQUEST_ID_MISMATCH`。replay 与 identity conflict 语义保持不变
（`SUBMISSION_IDENTITY_CONFLICT` fail closed）。

跨语言 byte-identical 的成立前提是当前 governance envelope 为**闭合、全字符串值、object key
全为 ASCII 固定字段名**的 schema：此前提下 Rust（`serde_json`）与 Java（Jackson）对 canonical
JSON 的序列化逐字节一致，`recursiveLexicographicKeySort` 对 ASCII key 无论按字节序还是 UTF-16
码元序都得到同一顺序。若未来 envelope 引入数值字段（两端数字格式化可能分歧）或非 ASCII 的
object key（Rust 按 Unicode 标量、Java `String.compareTo` 按 UTF-16 码元排序，BMP 外字符会分歧），
必须先重新冻结数字规范化与排序码元基准，再扩展 schema；否则跨实现 `requestId` 不再保证相等。

本决策不改变 receipt 的 `RECEIVED_ONLY` 语义，不授予反馈采纳、ExceptionGrant、Gate 或
SCM 权限，不改变 digest-only 与禁止上传源码/完整 Finding 的边界。

## Verification

```text
cargo test -q -p erf-thin-client --all-targets
mvn -pl control-plane/server -am test
bash scripts/request-identity-check.sh .scratch/request-identity-check
bash scripts/check.sh
```
