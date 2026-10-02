# ADR-0143: SCM Status Adapter 的部署与权限契约

- Status: Accepted
- Date: 2026-09-19
- Scope: external SCM Status Adapter deployment seam
- Decision record: GitHub issue #99

## Context

ADR-0142 只冻结了无凭据的 `StatusPublication` artifact 和接收 seam。真实 SCM
connector 仍需要 endpoint、凭据引用、权限 scope、请求幂等和 transport failure 语义；
如果这些输入留给各 provider 自行解释，容易把 token、私钥或过宽 repository 权限带入
artifact，或者把网络未知误报成成功。

## Decision

### 1. Deployment descriptor

`StatusAdapterDeployment` 使用 `rulefactory.io/v1` / `status-adapter-deployment-v1`，以
`publicationDigest` 绑定 #98 的 StatusPublication，并用自身 `deploymentDigest` 保护
provider、HTTPS endpoint、opaque `secretref://`、精确 `status:write` scope、request id 和
idempotency key。descriptor 不携带 StatusPublication 全文、源码、Finding、token 或私钥。

### 2. External adapter seam

外部 connector 只能在部署边界解析 credential reference，并接收经过 freshness、identity、
digest、单调和幂等校验的 publication。本地 fake 复用 `StatusPublicationAdapter`，返回
`ACCEPTED` 或 `IDEMPOTENT` 的 `RECEIVED_ONLY` receipt；校验/权限错误为 `REJECTED`，
网络或可用性无法判定为 `UNKNOWN`，两者都不得变成 SCM 写入或 Gate PASS。

### 3. Security and deployment boundary

只允许 HTTPS endpoint；URL 不得含 credentials、query 或 fragment；scope 固定为
`status:write`。真实 token store、TLS 证书、KMS/HSM、provider API、protected branch 和
重试策略由外部部署/治理边界负责。本票不连接任何 SCM，也不产生网络副作用。

## Consequences

- 后续 provider connector 有稳定且最小披露的输入/receipt contract。
- credential、TLS、权限和未知网络状态不再由 Runtime/Thin Client/Server 隐式决定。
- `status-adapter-deployment=PASS` 只证明 deployment-ready seam，不证明 SCM 已接通。

## Verification

```text
cargo test -p erf-status
bash scripts/status-adapter-deployment-check.sh .scratch/status-adapter-deployment-check
bash scripts/check.sh
```
