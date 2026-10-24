# ERF 使用情形、功能审计与 Issue 裁决（2026-10-24）

## 1. 审计范围与证据边界

- 审计 checkout：`master`，HEAD `d9da333646f87de79cee183a13b775d7cbd24080`。
- 参考资料：`AGENTS.md`、`CONTEXT.md`、README、`docs/agents/dev-workflow.md`、运行核心目录、控制面目录、GitHub open issues，以及用户提供的本地移交文档。
- 本 checkout 的 `./rf --help` 已验证；完整 `bash scripts/check.sh` 尚未通过，因为当前云环境缺少 Rust/Cargo/Maven 依赖下载所需的可用网络代理/DNS。
- 用户移交文档描述了另一棵本地 WSL 工作树及其后台门禁；该状态不能直接当作本 checkout 的提交或远端 `master` 状态。两者必须分开记录。

## 2. 产品定位

ERF（Enterprise Rule Factory）不是单纯的规则集合或 IDE 插件，而是可信规则执行层：把 Standard/Scenario 编译为可验证、可发布、可执行、可审计、可演进的机器检查资产。核心承诺是冻结 ScanPlan、Completeness、PASS/FAIL/INCONCLUSIVE 三态 Gate 和签名执行证明，而不是把“0 告警”直接当作通过。

当前边界由 ADR-0167 收窄：`./rf` 不授予 release/promote/revoke、Gate authority、Exception approval 或 SCM 回写权；权威保持 `LOCAL_UNATTESTED` / `SHADOW`。生产强制、受保护分支和 Trusted 执行仍是待验证目标。

## 3. 角色与真实使用情形

| 角色 | 目标 | 主要流程 | 当前能力映射 | 关键缺口 |
|---|---|---|---|---|
| 规则作者 | 把业务要求变成可验证规则 | Standard/Scenario → RuleIntent → Candidate → Verification | `control-plane/authoring`、`factory`、`coverage`、`./rf rule *` | 真实生产规则包签名、PMD/SpotBugs 生产资产与作者体验仍需验证 |
| 平台工程师 | 在仓库/CI 中稳定运行扫描 | init/doctor → 配置冻结 → ScanPlan → Runner → Gate | `runtime/project-config`、`scan-plan`、`rf scan`、`scripts/check.sh` | CI 依赖下载、真实缓存/撤回/信任根与长时运行证据 |
| 安全/合规负责人 | 证明结果可审计、可复核 | 规则发布、证据包、状态发布、异常申请 | `runtime/status`、`pilot-evidence`、`thin-client`、`contracts/http` | Server auth/TLS/多租户、持久 journal、SCM adapter 与人工审批链 |
| IDE/客户端用户 | 快速查看 Finding、解释和反馈 | `rf`/IDE/MCP → FindingView → digest-only request | `clients/*`、`runtime/thin-client`、`explain`、feedback/exception draft | 薄客户端不能成为权威；跨端一致性和真实 IDE 验收仍需环境支持 |
| 运行时/后端维护者 | 承载规则执行和工作区解析 | Resolver/Host 握手 → 计划执行 → Observation/UnitStatus | `backend-hosts/*`、`workspace-resolvers/*`、proto seam | 真实构建 attestation、sandbox、PMD 编译错误显式化、生产故障恢复 |
| 治理/审计人员 | 复核整改、门禁与证据 | Issue → ADR → 证据 → 双轴验收 | `docs/adr`、`.scratch` 证据、`scripts/check.sh` | 报告结构可证伪性、落点行号、fixture 与生产权威边界需持续收紧 |

## 4. 核心运行链

1. `rf-cli` 预检 repo/config、身份摘要、路径和 scope。
2. 交付描述符校验 RuleBundle、签名、撤回快照、缓存 freshness、ExecutionLock 和 runtime 身份。
3. Workspace Resolver 通过长度前缀 protobuf stdio 协议握手并返回 WorkspaceDescriptor。
4. Git ChangedSet 与 affected closure 生成冻结 FULL/MODULE/INCREMENTAL_PREVIEW ScanPlan；不安全增量回退 FULL。
5. Backend Host 按冻结分母执行，输出 Observation、UnitStatus 和 ExecuteComplete；缺失、超时、错误以 INCONCLUSIVE 结束，不提交半成品 findings。
6. Gate 根据 finding canonicalization、baseline、exception、policy 与 unresolved facts 生成三态结果。
7. 输出 text/JSON/SARIF、progress、digest-only FindingView 和可选 StatusPublication；缓存只保存摘要，不获得权威。

## 5. 圆桌观点

### Runtime/安全席

- 最重要的风险是“契约全绿”被误读为“生产可信”。`REFERENCE_FIXTURE`、`algorithm:none`、`LOCAL_UNATTESTED` 和 `SHADOW` 必须在所有报告中显式保留。
- 缓存命中、离线运行、增量结果和状态发布都必须重新验证签名、freshness、撤回、execution identity 与 completeness。
- 不能用 Node fake、IDE 画面或静态 grep 代替真实 PMD/SpotBugs、attestation、sandbox 和信任根。

### 平台/用户体验席

- 平台工程师需要一条可解释的失败链：配置错误、resolver 不确定、host 编译错误、超时、缓存失效和信任失败应各有稳定 code、stage、nextStep。
- 规则作者需要从 Scenario 到 EvidenceBundle 的闭环，避免只看到“规则文件生成成功”而不知道是否真的执行。
- 安全负责人需要可重放、可核验的 digest-only 申请和状态，不应把 receipt、PENDING 或 UNKNOWN 解读成批准或 Gate PASS。

### 治理/审计席

- Issue 整改必须给文件:行号和可复跑命令；`grep -c`、关键词存在或静态计数不能单独证明结构正确。
- 报告必须区分“现状事实、已验证行为、消费者责任、生产缺口和人工决策”，否则会形成新的审计债务。
- 任何涉及新制品（如 SHACL 模块）、Root 阈值或生产强制权力的决定必须回到用户裁决，不能由代理默推。

## 6. Issue 裁决与优先级

### P0：阻断/不可自动关闭

- #246/#247/#214：ADR-0186 结构、R-1/R-2/R-7/R-8 落点和证据可证伪性连续整改；当前必须保留为阻断记录，不能用计数或报告措辞掩盖。
- #237/#238/#239：RDFC 生产路径与行为守卫仍存在廉价绕过风险；需真实行为证据，不适合在缺少完整门禁时宣称完成。

### P1：需要生产化设计或用户裁决

- #152/#158/#145：Root 阈值/委派/namespace、离线/HSM、生产认证和隐私边界。
- #140/#141/#146/#147/#148/#149/#153/#154：可信执行链、Bootstrap 测量、Finding 身份、RDFC 本体签名和合法晋级链。
- #218/#147 的制品准入：是否引入新 SHACL 依赖属于用户/架构裁决，不自动决定。

### P2：可持续整改/证据精度

- #143/#144/#172：安全 Controls、验收证据和审计地图。
- #189/#204/#205/#210/#211/#212/#216/#219/#230/#231：报告、夹具、跨语言消费和门禁可见性。

### 当前可自动推进项

1. 先补本审计文档，固化角色、功能、边界和 Issue 映射。
2. 在网络/工具链恢复后，按 `docs/agents/dev-workflow.md` 运行完整门禁，不把局部静态检查升级为 Gate 结论。
3. 选择一个明确的文档/证据精度票做小范围改动，提交前生成可证伪的 diff 和唯一门禁日志。
4. 对需要新制品、生产信任根或真实设备的事项暂停并请求用户裁决。

## 7. 当前交付结论

- 本轮已完成仓库理解、角色/使用情形映射、圆桌观点和 Issue 优先级裁决。
- 本文是研究/审计产物，不宣称完成任何生产可信、生产 Gate、SCM 写入或完整门禁。
- 下一次代码改动应绑定一个具体 Issue，先读其完整正文与关联 ADR，再以最小范围提交并复验。

## 8. H07 小改动（进行中）

基于圆桌审计，当前工作树已加入 required-capability 真值校验：Host/Resolver 客户端和 fake/Java Host 只有在 capability fact 明确为 `true` 时才接受握手；缺失或 `false` 均视为未满足，并由 Java Host 返回 `UNSUPPORTED_CAPABILITY`。该改动不扩大产品权限，只收紧既有协议语义。

验证边界：本环境没有可用 Maven/Cargo 依赖下载网络，暂不能运行行为门禁；已完成 `git diff --check` 和源码级语义核对。提交/推送状态以 Git 状态和远端复核为准，不把静态检查写成编译通过。

## 9. Threat-model boundary slice

为把审计发现留在可验证边界，新增 5 个 `BOUNDARY_ONLY` threat rows：AttemptStaging 权限/清理、caller-supplied BootstrapMeasurement、ExecuteComplete 对账、Root threshold/delegation、ExceptionGrant 消费闭合。`scripts/threat-model-check.sh` 现在固定计数并校验这些行的 `BOUNDARY_ONLY` 状态；本次只产生静态边界证据，不宣称 Runtime 修复、Trusted、SCM 或生产安全 attestation。
