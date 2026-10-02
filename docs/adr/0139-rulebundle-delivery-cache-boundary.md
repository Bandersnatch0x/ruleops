# ADR-0139: 固化 RuleBundle 分发、缓存与离线可用性边界

- Status: Accepted
- Date: 2026-09-19
- Scope: RuleBundle Server-to-Runner delivery seam
- Decision record: GitHub issue #95

## Context

架构计划 §42–§46 与 §51 要求 RuleOps Server 发布签名 RuleBundle，Runner 在 Server
不可用时通过本地/CI cache 继续执行，但不能使用未知撤回状态、过期资产或未验证的依赖。
现有 `runtime/rule-bundle`、`runtime/packs`、`runtime/trust` 和 `runtime/trust-state`
已经分别验证签名资产、OCI Pack、TrustMetadata 与 TrustCheckpoint；缺少把这些身份
组织成 Server-to-Runner delivery descriptor 和 cache/offline 决策的共同 seam。

## Decision

### 1. Delivery descriptor

`BundleDeliveryDescriptor` 使用 `rulefactory.io/v1` 与 `rule-bundle-delivery-v1`，冻结
bundle bytes digest、bundle schema、compatibility/platform、RuleRelease、Policy、
ExecutionLock、OntologyContractPack、BackendRuntimePack、ArtifactStatement、撤回快照
摘要和有效时间。descriptor 是封闭的 canonical payload；字段增加必须升级 descriptor
版本。tag、latest 和文件名不是身份。

### 2. Cache adapter

`BundleCache` 按 bundle digest 读写；`FileBundleCache` 将对象存放在固定的 `sha256/` 内容
寻址目录，采用临时文件同步后 hard-link 的 create-only 方式提交，拒绝覆盖既有 digest。
重启可重新打开 cache；非普通文件、符号链接、超大对象、摘要不匹配或损坏对象均拒绝。
adapter 只保存 signed bundle bytes，不把 cache 命中当成签名或 Gate 权威。

### 3. Connected 与 offline

`resolve_cached` 先验证 descriptor 时间窗和调用方提供的既有 trust evidence，再读取精确
digest。Connected profile cache miss 返回 `SERVER_FETCH_REQUIRED`；Enterprise/Air-gapped
和 Standalone Developer cache miss、过期、签名/TrustMetadata/撤回快照/依赖 Pack/
TrustedContext 任一缺失均返回 `INCONCLUSIVE`。未知撤回状态不得降级为旧 bundle 继续执行。

本票不实现远程 registry、对象存储、KMS/HSM 或真实 IDE；这些 transport/authority 继续
由后续部署和薄客户端票定义。Runner 仍须在消费前调用既有 RuleBundle/Pack/Trust verifier。

## Consequences

- Server 暂时不可用时，满足完整 trust closure 的缓存 bundle 可以离线继续执行。
- 缓存命中和 Connected fetch retry 与 Gate authority 分离，避免把 availability 误报为 PASS。
- descriptor 与 cache seam 可被 HTTP、OCI mirror、Standalone export 和未来 IDE client 复用。

## Verification

```text
cargo test -p erf-bundle-cache
bash scripts/bundle-delivery-check.sh .scratch/bundle-delivery-check
bash scripts/check.sh
```
