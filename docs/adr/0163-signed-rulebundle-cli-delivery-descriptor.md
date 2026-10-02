# ADR-0163: 标准 rf scan 的签名 RuleBundle Delivery Descriptor

- Status: Accepted
- Date: 2026-09-20
- Scope: V0.4/Runtime、rf CLI、RuleBundle delivery/cache
- Decision record: GitHub issue #123
- Related: #112、#121、contracts/http/rule-bundle-delivery-v1.yaml

## Context

#112 已经冻结安装包入口、项目配置身份、结构化失败和 CI-neutral
reference package，但标准 rf scan 仍没有一个可以从文件或 CI 环境传入
BundleDeliveryConfig 的闭合入口。若让调用方继续拼接内部 Host/Resolver
参数，或者在缺少 verified RuleBundle 时回退到本地规则，就无法证明
Connected、Offline、Standalone 三种模式消费的是同一个签名资产。

## Decision

rf scan 接受 --delivery-descriptor <file>，也接受
RULEOPS_DELIVERY_DESCRIPTOR 环境变量。命令行优先于环境变量。descriptor
必须是仓库内的 rf-delivery-v1 JSON 对象；未知字段、越界路径、错误
profile、错误 digest 编码和 descriptor/verification 不一致都在 Resolver 或
Backend 启动前拒绝。

CLI 只负责把 descriptor 中的 endpoint、cache policy、deployment profile、
RuleBundle/Runtime Pack verification context、ExecutionLock binding 装配为
BundleDeliveryConfig。实际签名、manifest bytes、RuleBundle inputs、TrustStore/
TrustMetadata、Runtime Pack statement 和 cache provenance 仍由
rf-runner 的已有 delivery/cache seam 验证。服务端返回的布尔 evidence
不能替代本地验证。

当 descriptor 走标准安装包路径时，`rf-cli` 还必须在启动 Resolver 或 Backend
之前验证 `runtime-manifest.json` 的 Ed25519 DSSE。manifest 自带的公钥不是信任
根；其公钥必须与部署注入的 `RULEOPS_RUNTIME_RELEASE_TRUST_ROOT_PEM` 完全一致，
缺少或不匹配时以 trust failure 拒绝执行。该根由 RunnerBootstrap/企业安装流程
提供，delivery descriptor 不能声明或提升自己的 Runtime Pack 信任根。

同一边界也适用于 delivery 内的 RuleBundle、Runtime Pack 和 ExecutionLock signer：
`verification.trust.delegation` 是由离线 Root 签名的 DSSE 委派，覆盖角色 key、
有效期和撤回集合；`RULEOPS_DELIVERY_TRUST_ROOT_PUBLIC_KEY_HEX` 是外部注入的
Root 公钥。descriptor 自带的 `verification.trust.keys` 只能作为被委派 key 的载体，
不能自行建立信任。

撤回状态也不是一个可由 descriptor 自报的布尔值：交付必须携带由委派的
`revocation-snapshot-signing` key 签名、由 `revocationSnapshotDigest` 绑定、并按本机时钟
验证有效窗口的 Snapshot；Snapshot 中命中的 RuleBundle、Runtime Pack、ExecutionLock、
Policy、Ontology 或 Artifact Statement digest 在扫描前拒绝。

Runtime Pack 的交付表示是内联的、内容寻址的 OCI Image Layout：每个 pack 必须同时提供
`oci-layout`、`index.json`、config blob 与 asset layer blob；index 的 manifest digest/size、
manifest 的 config/layer digest/size、config identity 和 layer 到已安装资产的绑定都在
Runner 本地重算。只验证一段签名 manifest 文本，不再算作完整 Runtime Pack 输入。

descriptor 与 --candidate 互斥；--rule-id 若同时给出必须与 descriptor
中的 ruleId 相同。descriptor 的 executionLockPath 可以被 CLI
--execution-lock 覆盖，但二者存在时必须指向同一文件。三种执行模式映射为
connected、enterpriseAirGapped、standaloneDeveloper，并沿用既有
SERVER_FETCH_REQUIRED、INCONCLUSIVE、trust failure 和 input failure
退出分类。

结构化 JSON/SARIF 只输出闭合诊断和 detailDigest，不泄露 endpoint、文件系统
路径或进程细节；文本模式保留本地调试信息。生产 Runtime Pack 另行由生产包
检查确认真实 Java Host/Resolver、内容 digest、兼容性 identity、平台和签名
元数据，REFERENCE_FIXTURE 或 algorithm: none 永远不能通过该检查。

descriptor 不接收调用方提供的当前时间；rf-cli 使用本机 Unix clock 判断
issued/expiry。签名 RuleBundle manifest 必须包含并签署 bundleDigest，且与
cache 返回的实际 payload digest 相同。ExecutionLock 的 backend/resolver
digest 使用对应 Runtime Pack manifest digest，并由 Runner 交叉绑定。标准 delivery
路径只接受由被 Root 委派的 `execution-lock-signing` key 签署的 ExecutionLock，且
`runner_release_digest` 必须等于当前安装包 `rf-cli` 的实际 digest。

Runtime Manifest 中用于交付启动的 `cli`、`backendHost`、`backendHostJar` 和
`workspaceResolver` 资产必须是包内 regular file，digest、platform、兼容性 identity 和
`RELEASE_ARTIFACT` 状态均通过签名 manifest 验证；delivery 运行时不得把
`REFERENCE_FIXTURE` 当成生产资产。

## Verification

- cargo test -q -p rf-cli --all-targets
- descriptor parser/unit and three-profile structured failure checks
- production Java Runtime Pack check
- bash scripts/check.sh
