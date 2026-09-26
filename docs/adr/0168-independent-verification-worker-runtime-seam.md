---
status: accepted
---

# ADR-0168: 独立 Verification Worker 运行时边界

## 决策

Contract Conformance Suite 的聚合与 verification-signing 签名由
runtime/verification-worker 持有。它提供独立可执行文件
erf-verification-worker：Worker 运行完整固定 suite，拒绝部分通过结果，
绑定 Runner、Backend Pack、Workspace Resolver Pack、Sandbox Provider/Policy 和
platform identity，然后要求受信 Sandbox Provider 对两侧命令、策略、测量摘要及
rootless/read-only/network/no-new-privileges/capability/resource controls 签发
`SandboxProof`。Worker 只使用部署配置的 provider public key 验证该 proof；proof
缺失、签名无效、命令或策略漂移、隔离控制不完整时拒绝签名。通过后以标准 DSSE
JSON envelope 输出签名的 ConformanceEvidence。

部署信任根通过受控部署环境变量 `ERF_SANDBOX_TRUST_PUBLIC_KEY_HEX` 注入到 Worker
进程，不属于请求 JSON、普通 CLI 参数或候选资产可选择的输入；Worker API 只接受
部署拥有的 `SandboxTrustAnchor`。

Pack Builder 只提交候选 executable 和不可变身份摘要；不会调用 Builder 内的签名
helper，也不持有 Worker signing key。scripts/verification-worker-check.sh 以独立
进程驱动 reference Host/Resolver，作为该边界的可复现验收。

## 边界

当前仓库中的 fixture signer 仅编译在 `fixture-signer` feature 中，使用固定
Ed25519 key，且只能在显式设置 `ERF_ALLOW_FIXTURE_SIGNER=1` 的本地验收模式中通过，
并使用明确的 `--fixture-signing-key-hex` 参数；默认 Worker 二进制不包含该 signer。
生产部署必须提供受信 provider proof，并替换为隔离 Worker 的 workload identity
或 HSM signer；同时把 suite release、Worker release 和 Sandbox Profile 的发布摘要
接入 Deployment Policy。Runtime 仍只验证
verification-signing role 的签名资产，不从旧 test result 推断新组合兼容。
