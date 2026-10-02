# 架构说明

## 适用范围与证据基线

本文描述 `04019de`（2026-10-01）的仓库结构、当前接口及既有架构约束；不把计划中的生产部署写成当前能力。
仓库权威入口为 `AGENTS.md:7`、`CONTEXT.md`、`docs/adr/`、架构计划与白皮书；本文不修改这些裁决。
白皮书的愿景不等于交付权限：当前承诺是可信规则执行层，受保护分支强制仍是待验证假设（`CONTEXT.md:3`；`docs/adr/0167-product-audit-adjudication-batch.md:23`）。

## 分层与源码落点

| 层 | 当前职责与入口 | 不承担的职责 / 证据 |
| --- | --- | --- |
| `runtime/` | Rust 的 CLI、配置、执行编排、bundle/cache、计划、Completeness、Gate 与信任；`runtime/rf-cli/`、`runtime/host-client/`、`runtime/gate/` | 不依赖创作簇源码；参见 `CONTEXT.md:7`、`README.md:41` |
| `control-plane/` | Java 21 模块化单体：authoring、factory、coverage、ontology、verification、server | 治理与创作不是业务仓库日常扫描；`docs/adr/0027-use-a-rust-runtime-with-process-isolated-backend-hosts.md:7`；架构计划 `docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md:2428` |
| `clients/` | IDE 薄客户端与 MCP/WebMCP 调用原 `rf`，投影/展示结果 | 不复制 Runtime，不批准规则或改写 Gate；`README.md:62`、`clients/mcp/README.md:3` |
| `backend-hosts/` | Java Host 运行 PMD/SpotBugs；fake Host 提供契约证明，进程隔离 | 不决定 Policy、Completeness 分母或 SCM 状态；`docs/adr/0027-use-a-rust-runtime-with-process-isolated-backend-hosts.md:7`、`contracts/proto/backend_host.proto` |
| `workspace-resolvers/` | 独立进程解析项目结构，输出不可变 WorkspaceDescriptor | 静态解析不执行构建脚本；不伪装成 Backend Host；`contracts/proto/workspace_resolution.proto:5` |
| `contracts/` | `proto/` 跨进程契约；`http/` 治理、交付、薄客户端、输出与状态 publication 契约 | 是边界，不是隐式授予权限；`README.md:53` |
| `scripts/` | 构建、打包、定向契约/行为验收、Pilot 取证、独立本地工具 | 脚本名带 trusted/production 不等于当前 checkout 已有生产权威；`scripts/check.sh`、`scripts/ci-neutral-scan-check.sh:27` |
| `specs/` | Standard、RuleIntent、Rule IR、Coverage、Ontology、兼容性及 Pilot/Security 资产文件 | 本地文件不自动注册到 Server，也不自动成为已签名 Release；`README.md:55`、`.scratch/server-rule-management.md:26` |

仓库保持 monorepo。执行核心与创作簇通过 `contracts/*` 和签名 RuleBundle 交付，不做源码级耦合；当前没有拆成外部仓库（`CONTEXT.md:14`；`docs/adr/0167-product-audit-adjudication-batch.md:49`）。
根 `rf` 是 Bash 启动器，优先使用安装包 binary，开发检出调用 Cargo；不能把这一包装层误读为另一套执行引擎（`rf:64`、`rf:72`）。
外部研究只吸收能力，不引入对应产品依赖；沿用既有 ERF 领域名（`docs/adr/0017-absorb-capabilities-without-product-dependencies.md:7`）。

## 主要 seam 与契约

| seam | 交换内容 / 不变量 | 证据 |
| --- | --- | --- |
| 创作 → 执行 | Candidate 经独立 Verification/发布治理形成签名 RuleBundle；核心消费精确 digest，不把 RuleIntent 或生成成功当 Release | `CONTEXT.md:69`、`CONTEXT.md:81`、`runtime/bundle-delivery/` |
| Runtime ↔ Backend Host | framed stdio protobuf；stdout 仅协议帧，stderr 为日志；major 不匹配或必需 capability 缺失则失败，不静默降级 | `contracts/proto/backend_host.proto:5`；`docs/adr/0027-use-a-rust-runtime-with-process-isolated-backend-hosts.md` |
| Runtime ↔ Workspace Resolver | 独立协议/客户端；WorkspaceDescriptor 冻结结构事实；不执行构建脚本 | `contracts/proto/workspace_resolution.proto:5` |
| 配置 → ScanPlan | 配置来源、配置投影与实际 Resolver 身份均可追溯；先冻结必需规则和分析单元，Host 不得缩小分母 | `CONTEXT.md:239`、`CONTEXT.md:243`、`contracts/http/scan-output-v1.yaml:38` |
| Server API → Store/journal | `rulefactory.io/v1`；不可变 `id + version + payload`；写入幂等、审计；日志先追加后变更，恢复 fail closed | `docs/adr/0137-versioned-ruleops-server-api-seam.md:19`、`CONTEXT.md:109`、`control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java` |
| Server → Bundle cache/Runner | descriptor 绑定内容身份、兼容性、签名/撤回快照和有效期；缓存命中不授予 Gate 权威 | `contracts/http/rule-bundle-delivery-v1.yaml`、`CONTEXT.md:117` |
| Thin Client → Server | 显式 endpoint 提交 digest-only feedback/exception request；回执仅接收，不等于批准；未知状态不自动重试 | `docs/adr/0141-thin-client-server-submission-transport.md:19`、`contracts/http/submission-status-v1.yaml` |
| 只读 registry → 客户端 | rules/standards 身份/state/digest，不返回 payload；list 为 FORMAT_ONLY，show 为独立 pin 的 PIN_MATCH；authority 始终 UNKNOWN | `runtime/thin-client/src/registry.rs`、`.scratch/multi-end-rule-fetch.md:61` |
| Runner → Status Publisher → 外部 adapter | GateAttestation 经独立重新验证后形成 StatusPublication；adapter 收取无凭据 artifact，不改变决策 | `docs/adr/0142-status-publication-artifact.md:19`、`docs/adr/0143-status-adapter-deployment-contract.md:19` |

Server 默认本地内存 Store，第二启动参数选择文件 journal；恢复异常显式抛出。该入口是本地开发服务，不证明多写者数据库或生产 Registry（`control-plane/server/src/main/java/io/ruleops/server/RuleOpsServerMain.java:5`；`.scratch/server-rule-management.md:19`）。
只读 registry 读取同一个 Store/journal，**不遍历导入 `specs/`**；Store envelope digest、YAML 字节摘要、RuleBundle 签名摘要不是同一身份（`.scratch/server-rule-management.md:20`）。

## 数据流与控制流

### 规则作者路径

```text
Standard → Scenario / Coverage Ledger → RuleIntent
  → Factory / Candidate → 独立 Verification / Evidence
  → 人工评审与发布治理 → RuleRelease / Policy / 签名 RuleBundle
  → delivery descriptor / 缓存 → Runtime
```

此链同时展示已有对象和架构发布边界，并非一个全部可由 `./rf` 自动跑通的发布命令。
AI 只能提出草稿，不能自动批准、签名、晋级 blocking 或修改生产 Policy（`docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md:345`）。
离线规范抽取目前仅有限确定性映射并留有 provider seam；未映射项是 `needs_review`，草稿缺独立 holdout/nearMiss，不能算已覆盖或获准发布（`.scratch/standard-to-rules.md:63`）。
外部 PMD ruleset 检查是独立本地脚本，不是自动导入/发布管线（`.scratch/plugin-rule-extension.md:53`；`scripts/plugin-ruleset-check.py`）。

### 执行与结果路径

```text
CLI / IDE / MCP / WebMCP → rf → project-config / rf-cli → 同一 Runner 核心
  → 验证交付与配置身份 → Workspace Resolver → 冻结 ScanPlan
  → Backend Host 执行 → 结构化观测 / 规范化结果
  → Completeness + Policy / Baseline / Exception → GateDecision
  → ScanResultProjection（text / JSON / SARIF）→ 本地展示
```

架构运行时职责见 `docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md:2460`；输出契约见 `contracts/http/scan-output-v1.yaml:6`。
MCP 使用 argv 调用安装目录的 `rf`；WebMCP 只是页面与同一后端的桥接，不是新 Runtime 或替代后端 MCP（`clients/mcp/README.md:103`；`clients/mcp/webmcp/README.md:29`）。
registry 查询仅查询身份，**不是下载可执行规则**，也不自动安装、执行或回退到本地规则（`.scratch/multi-end-rule-fetch.md:24`）。

## 权威边界：必须同时看决策、完整性、信任

- `GateDecision` 只有 `PASS / FAIL / INCONCLUSIVE`。Finding 数量与 Completeness 分离；没有足够证据时是 INCONCLUSIVE，不是“零告警通过”（`CONTEXT.md:343`；`docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md:446`）。
- `CIAuthorityStage` 的 SHADOW 与 GateDecision 不同；本 checkout 的本地结果是 `LOCAL_UNATTESTED`/SHADOW，不能升级为 BLOCKING 或受保护分支证明（`CONTEXT.md:347`；ADR-0167）。
- 架构 TrustedContext 要绑定 repository、commit、policy、bundle/pack/lock、baseline 与独立 RunnerMeasurement 等身份；本地自编译或自报摘要不能替代可信启动（`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md:2480`、`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md:2909`）。
- 发布签名由组织委派、相互隔离的角色负责；RuleRelease、Policy/Bundle、Baseline、OntologyContractPack、Runtime Pack 不共用同一签名职责。Author、Factory Worker、Ontology Compiler、Pack Builder 只能提交待签资产（`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md:2712`）。
- ExecutionLock 由 Control Plane 在 Policy/RuleBundle 发布时确定性解析并由对应发布角色签名；Runtime 只验证、缓存和执行，不临时用 latest 或本机发现补齐（`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md:2794`）。
- 可信执行身份签署 GateAttestation；独立 Status Publisher 验签、校验 commit/freshness/digest 后原样发布。Runner、Host、Resolver 不持有 SCM 写凭据（`CONTEXT.md:409`；`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md:2941`）。
- 仓库 StatusPublication seam 映射 `PASS → success`、`FAIL → failure`、`INCONCLUSIVE → action-required`；`scmWritten=false`，需要外部 adapter。token store、TLS、KMS/HSM、provider API、受保护分支均属外部部署治理（`docs/adr/0142-status-publication-artifact.md:27`；`docs/adr/0143-status-adapter-deployment-contract.md:26`）。
- 当前 `rf release`、`rf gate` 被显式拒绝，命令面不提供 promote；Server 通用治理 API 的存在也不证明部署已认证、获批或接通 SCM（`rf:277`；`.scratch/server-rule-management.md:17`）。

## 隐私及未证实能力

默认不上传源码、片段或完整 Finding；跨端只使用闭合的 digest-only 投影，允许契约中的消息/相对位置，并非“没有任何路径”（`docs/adr/0137-versioned-ruleops-server-api-seam.md:32`；`clients/mcp/webmcp/README.md:39`）。
本地 Candidate/测试 fixture 可以包含源码，`.scratch/` 只是产物位置，不是访问控制或自动脱敏器（`docs/agents/dev-workflow.md:30`）。
未发现本次交付证明生产 TLS/认证授权、多租户隔离、真实 SCM 写入、组织签名基础设施和受保护分支强制可用的证据；不得用契约测试 PASS、registry 回执或本地 hook 放行替代。

继续阅读：[运行方式](run.md) · [测试说明](testing.md) · [使用手册](manual.md)。
