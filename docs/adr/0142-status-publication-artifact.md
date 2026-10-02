# ADR-0142: 独立 Status Publisher 的 publication artifact 边界

- Status: Accepted
- Date: 2026-09-19
- Scope: Trusted CI / Status Publisher / external SCM adapter seam
- Decision record: GitHub issue #98

## Context

架构计划 §41、ADR-075 与 ADR-079 要求 Runner 只产生 GateAttestation，由独立 Status
Publisher 重新验证证明后，以最小权限把三态结果交给 SCM adapter。仓库已有
`runtime/status` 的签名验证、repository/commit/policy 绑定、单调 run identity 和三态
映射，但尚无可被外部 adapter 消费的版本化 publication artifact。

## Decision

### 1. Publication artifact

`StatusPublication` 使用 `rulefactory.io/v1` / `status-publication-v1`，绑定 repository、
commit、policy digest、GateAttestation subject、run sequence、issued/expiry 和严格的
`publicationDigest`。内容摘要按 canonical JSON（排除自身摘要）计算；篡改、错误状态或
freshness 立即拒绝。adapter port 显式接收当前时间，按 `[issuedAt, expiresAt)` 判断
artifact 是否仍可接收，避免延迟到达的旧结果重新进入 SCM 边界。

### 2. Strict mapping and monotonic identity

`PASS → success`、`FAIL → failure`、`INCONCLUSIVE → action-required`；neutral、success
降级和未知状态禁止。外部 adapter 以 repository + commit + policy + attestation subject
和 run sequence 做幂等/单调检查；旧 sequence、跨 commit、同序冲突或旧 issued-at 不得覆盖
当前 publication。

### 3. Credential and authority boundary

artifact 不含 SCM 凭证，`externalAdapterRequired=true` 且 `scmWritten=false`。本票只提供
transport-neutral `StatusPublicationAdapter` 与内存 `FakeStatusAdapter`，不调用 GitHub/
GitLab/Bitbucket，不让 Runner、Thin Client 或 RuleOps Server直接写 SCM。生产 connector、
凭证隔离、KMS/HSM 与 protected-branch 部署另行治理。

## Consequences

- 外部 SCM connector 有稳定、最小披露且可验证的输入。
- 三态语义与幂等/防旧结果覆盖规则可以在没有外部凭证的环境中验收。
- `status-publication=PASS` 只证明 artifact/adapter seam，通过不代表 SCM 已发布。

## Verification

```text
cargo test -p erf-status
bash scripts/status-publication-check.sh .scratch/status-publication-check
bash scripts/check.sh
```
