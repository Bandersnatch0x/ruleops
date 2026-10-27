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

## 8. H07 小改动（已完成，验证边界保留）

基于圆桌审计，当前工作树已加入 required-capability 真值校验：Host/Resolver 客户端和 fake/Java Host 只有在 capability fact 明确为 `true` 时才接受握手；缺失或 `false` 均视为未满足，并由 Java Host 返回 `UNSUPPORTED_CAPABILITY`。该改动不扩大产品权限，只收紧既有协议语义。

验证边界：本环境没有可用 Maven/Cargo 依赖下载网络，暂不能运行行为门禁；已完成 `git diff --check` 和源码级语义核对。提交/推送状态以 Git 状态和远端复核为准，不把静态检查写成编译通过。

## 9. Threat-model boundary slice

为把审计发现留在可验证边界，新增 5 个 `BOUNDARY_ONLY` threat rows：AttemptStaging 权限/清理、caller-supplied BootstrapMeasurement、ExecuteComplete 对账、Root threshold/delegation、ExceptionGrant 消费闭合。`scripts/threat-model-check.sh` 现在固定计数并校验这些行的 `BOUNDARY_ONLY` 状态；本次只产生静态边界证据，不宣称 Runtime 修复、Trusted、SCM 或生产安全 attestation。


## 10. 重点风险裁决选项与当前建议

本节把尚未具备实现授权或完整运行证据的事项记录为可选方案，避免把“建议推进”误读为已经完成的生产能力。

### 10.1 ExecuteComplete 完成对账（#146）

- 选项 A（推荐，需先冻结协议）：新增 `observation_count`、`attempt_digest` 及 attempt/nonce 绑定字段；Host、Rust/Java/Node fake、conformance 和 Runner 同步拒绝重复、乱序、缺失或对账不符的完成消息。
- 选项 B（当前保守边界）：维持现有 `units_reported` 对账，只补 ADR、schema 检查和 `BOUNDARY_ONLY` 证据，不宣称完成协议已闭合。
- 当前决定：用户已批准进入 A 的协议冻结与实现阶段；本提交先落 ADR-0179，随后按其顺序更新 proto、三种 Host、Runtime 和 conformance。完整行为门禁恢复前仍不得宣称 Trusted 通过。

### 10.2 ExceptionGrant 消费闭合

- 选项 A（推荐，需治理注册表接入）：Gate 只消费经过签名、有效期、撤回状态和 repository/rule/path/fingerprint 绑定校验的 active grant，并由注册表生成 `exception_grant_digests` 与实际豁免集合。
- 选项 B（当前保守边界）：继续允许调用方传入 fingerprint 集合，但将其保持为 `LOCAL_UNATTESTED` / `BOUNDARY_ONLY`，不把 caller-supplied 集合宣称为生产授权。
- 当前决定：用户已批准进入 A 的治理注册表接入阶段；在注册表、身份、撤回和 fingerprint 绑定实现完成前，现有 caller-supplied 集合仍保持 `LOCAL_UNATTESTED`，不得提升为生产授权。

### 10.3 Root threshold/delegation 与 HSM（#152）

- 选项 A：实现独立 Root 身份去重、阈值验签、委派链、namespace、轮换和离线/HSM 运维验收。
- 选项 B（推荐当前）：维持 `LOCAL_UNATTESTED` / `SHADOW`，只补边界证据和人工审批记录，不新增无人调用的 threshold API。
- 当前决定：用户已批准进入 A 的设计/实现准备阶段；先补 threshold/delegation/HSM ADR 与验收契约，再实施，未完成前继续保持 `LOCAL_UNATTESTED` / `SHADOW`。

### 10.4 RDFC/本体签名与制品准入（#147/#218）

- 选项 A：先出实现 ADR、确定制品格式与签名身份，再实施跨语言 deterministic pack 和 admission。
- 选项 B（推荐当前）：继续保持 `RDFC_NOT_ADMITTED`，不引入新 SHACL/RDFC 依赖，不把研究性制品接入生产路径。
- 当前决定：用户已批准进入 A 的实现准备阶段；先冻结 RDFC 制品、签名身份、迁移和回滚方案，再接 admission，未完成前仍保持 `RDFC_NOT_ADMITTED`。

### 10.5 已落地的可独立验证项

- H07 required capability 已收紧为明确 `true`，并补齐 fake Host/Resolver 的 false-capability 回归覆盖。
- Runner 已在 staging 前把 Backend SourceSpan/File 摘要绑定到 WorkspaceDescriptor，摘要缺失、冲突或不匹配会 fail-closed。
- 上述两项已提交并推送；静态门禁通过，但 Cargo/Maven 行为验证仍受环境依赖阻塞。


## 11. Approved implementation plans

- ADR-0179 freezes the additive ExecuteComplete accounting plan: observation_count, attempt_digest, canonical digest rules, trusted/legacy compatibility and cross-language conformance.
- ADR-0180 freezes the Root threshold/delegation/HSM preparation plan: independent identities, two-of-three threshold, namespace/expiry/revocation, rollback protection and custody/recovery gates.
- ADR-0181 freezes the RDFC admission preparation plan: deterministic signed artifact pack, dedicated signing role, dependency/resource/revocation checks and migration/rollback evidence.

## 12. Latest implementation evidence (2026-10-25)

- #146 ExecuteComplete accounting 已落地：proto 新增 `observation_count` 与 `attempt_digest`；Rust Host Client 校验单位数、观察数、32-byte digest；Rust/Node/Java Host 输出同一 canonical attempt digest；conformance 对错误 observation count 与错误 digest 均 fail-closed。
- 独立 Phase 2 acceptance 已通过 Rust Fake 33/33、Node Fake 33/33；两者 attempt digest 一致：`81f9d50ff553f4be25ecee40c40da242ec4c47e0fb3fbd09ce1ce6baca563f69`。`cargo test -p erf-conformance --lib` 4/4 通过（使用环境内 vtkprotoc 兼容工具）。
- Root threshold 仅落 preparation/test seam：`contracts/trust-root-v1.yaml`、`runtime/trust` pure metadata/2-of-3/去重/撤回/namespace/expiry/rotation 校验与 fixture；`cargo test -p erf-trust --all-targets` 22/22 通过（使用仓库自带 toolchain 环境）。未接入 TrustStore、Runner、CI 或 authority，仍为 `PREPARATION_ONLY` / `LOCAL_UNATTESTED_SHADOW`。
- Root metadata chain 仅新增 pure seam：`RootTrustMetadata::validate_chain` 校验候选 generation 的 previous digest、namespace、version 与 `issued_at_epoch_secs` 单调关系；显式允许过期上一代作为历史链结，不写入 `TrustStore`/`TrustState`，不授予 delegated/execution 权限。当前环境无 Cargo，新增回归未执行编译；仍为 `PREPARATION_ONLY` / `LOCAL_UNATTESTED_SHADOW`。
- RDFC admission 仅落 deterministic pack envelope/parser/validator、canonical/wire vectors 与边界回归；`RdfcAdmission` 固定返回 `RDFC_NOT_ADMITTED`，未引入 SHACL/RDFC 新依赖，未接入生产准入。
- 已知阻塞：完整 workspace 门禁仍受真实构建依赖和环境工具链（包括 Maven/Javac 缺失、部分 workspace 需要 protoc）影响；HSM/离线 custody、双人审批、recovery、Root chain/registry/CI 集成仍未实现。

## 13. Root chain fixture evidence

- Remote `b7b8ace` extends the Root preparation fixture with a portable generation-chain case and six `validate_chain` negative cases: missing previous metadata/digest, wrong digest, namespace mismatch, version rollback, and issued-at rollback.
- The fixture also freezes the allowed historical-link behavior: an expired previous generation may remain a valid chain anchor when the digest and monotonic metadata checks hold.
- This remains evidence-only: no TrustStore, Runner, CI, HSM custody, dual-person approval, recovery, or production authority integration was added.

## 14. Blockers and maintained TODO (2026-10-26)

### A. 可立即执行

- 保持当前分支与远端同步，提交前运行 `git diff --check`、Rust trust/conformance 局部门禁和 threat-model check。
- 每个新增 Issue 先核对完整正文、关联 ADR、目标文件与可复跑命令；不对当前分支不存在的历史文件制造修复。
- 继续维护本节，记录新的阻塞、裁决和证据，不把局部通过升级为生产 Gate 结论。

### B. 环境恢复后执行

- 补齐 JDK 21 完整工具链（必须包含 `javac`），并通过 `source ../toolchain/ruleops-env.sh` 注入 Maven/Cargo。
- 恢复 `repo.maven.apache.org` 的 DNS/代理，或预置可审计的 Maven 依赖缓存；当前缓存仅有 `*.lastUpdated`，不能离线构建。
- 重新运行 `scripts/check.sh` 全量门禁；若失败，按首个真实失败阶段记录日志，不以脚本启动成功代替通过。
- 全量门禁通过后，再核对跨语言、Java Host、真实 Runner、CI publication 和重复性证据是否完整。

### C. 需要外部基础设施或明确裁决

- Root authority：离线/HSM custody、双人审批、recovery、TrustStore/Runner/CI/registry 接入和运维演练。
- RDFC：制品格式、签名身份、迁移/回滚策略和正式 admission；在此之前保持 `RDFC_NOT_ADMITTED`。
- Trusted CI：受信 workload identity、Bootstrap 测量、真实签名/attestation、registry 与 protected-branch 状态发布。
- 任何会扩大生产权限、引入新制品依赖或改变信任模型的动作，先记录“明确选项/推荐项/阻塞条件”，获得裁决后再实施。

### D. 当前已确认事实

- `cargo test -p erf-trust --all-targets`：22 passed / 0 failed（使用 bundled toolchain）。
- `scripts/check.sh` 当前首个阻塞为 `mvn not found`（默认 PATH）；注入环境后 Maven 进一步因 DNS 无法解析 `repo.maven.apache.org`，且 JDK 仅有 runtime、缺少 `javac`。
- 工作树 clean，最新远端提交为 `fe5476d`；上述问题均为环境/基础设施阻塞，不是已观察到的仓库代码编译错误。


## 15. Single backlog status and decision log (2026-10-26)

- **唯一待办入口**：本维护文档；新增需求、阻塞、证据和裁决只追加到本文件，不另建并行待办清单。
- **当前状态**：远端分支 `codex/use-case-issues-audit-20261024` 保持 clean；Root chain fixture、6 个 `validate_chain` 负例、历史 previous 过期链结语义均已推送；`erf-trust` 使用 bundled toolchain 实测 22/22 通过。
- **环境阻塞**：默认 PATH 缺少 Maven/Cargo；注入 toolchain 后，JDK 只有 runtime、缺少 `javac`；Maven 依赖解析受 `repo.maven.apache.org` DNS/代理不可用阻塞，缓存仅有 `.lastUpdated` 标记。
- **生产边界**：Root authority/HSM custody、双人审批、recovery、TrustStore/Runner/CI/registry 接入、Trusted CI attestation 以及 RDFC admission 均未接入；继续 fail-closed，不把 preparation/shadow 证据升级为生产权威。
- **下一次代码提交准入条件**：绑定具体 Issue 与 ADR，提供目标文件/行号、可复跑命令、预期失败条件和唯一证据日志；环境未恢复或缺少信任裁决时，只更新本节状态，不提交生产路径实现。
- **待用户裁决项**：Root/HSM 运维与恢复模型、Trusted CI workload identity/registry 方案、RDFC 制品与签名/迁移/回滚策略。每项继续以“明确选项 / 推荐项 / 阻塞条件”记录后再执行。


## 16. Issue slice + brainstorm / roundtable decision (2026-10-26)

### 已编码切片

- 绑定 #204 的可验证缺口：Runner 在接收 `UnitStatus` 时新增 fail-closed 身份守卫；空白 `unit_id` 立即生成 `UNASSIGNED_EVENT_UNIT`，不进入 statuses、不提交观察结果。
- 新增回归测试 `unit_status_requires_an_assigned_event_unit`，覆盖空白 ID 拒绝与正常 `module:rule` 接受。
- 使用 `PROTOC=/usr/bin/vtkprotoc-pv5.13` 的 bundled toolchain 实测 `cargo test -p rf-runner --lib`：20 passed / 0 failed；`git diff --check` 通过。
- 远端代码提交：`499a5d7`。该切片只收紧运行时完整性，不授予 Trusted/production authority。

### 头脑风暴候选功能

1. **Attempt integrity dashboard**：展示 planned/status/completion/observation/attempt digest 的对账差异；只读、digest-first。
2. **Issue evidence manifest**：每个整改项绑定目标文件、测试命令、预期失败条件和证据摘要；自动生成审计索引。
3. **Roundtable replay pack**：把安全、平台、治理三席的裁决输入固化为可重放 fixture，避免口头决策漂移。
4. **Environment readiness doctor**：仅诊断 JDK/javac、Maven DNS/cache、Cargo/protoc，不改变生产信任状态。
5. **Boundary-aware feature flags**：将 PREPARATION_ONLY、LOCAL_UNATTESTED_SHADOW、RDFC_NOT_ADMITTED 作为显式能力状态，阻止 UI/CLI 误报生产能力。

### 圆桌裁决

- Runtime/安全席：优先 1 和 5；任何新功能必须 fail-closed，不能把本地证据升级为 authority。
- 平台席：优先 2 和 4；环境 doctor 可提高执行效率，但不能掩盖依赖缺失。
- 治理/审计席：优先 2 和 3；所有决策必须可追溯到 Issue/ADR/证据，不接受计数型“完成”。
- **综合建议**：下一切片优先实现 Issue evidence manifest（文档/fixture 层），其次做 Attempt integrity dashboard；Environment doctor 作为环境恢复后的辅助项，Roundtable replay pack 先保持设计，不接生产权限。

### 当前不做

- 不接 Root/HSM、Trusted CI、registry、RDFC admission 或 protected-branch authority。
- 不引入新 SHACL/RDFC 依赖，不改变协议冻结字段，不删除现有 fail-closed 守卫。


## 17. Issue evidence manifest slice and roundtable update (2026-10-26)

### 已编码并推送

- 按用户最新优先级，先忽略 JDK 21/javac 与 Maven DNS/缓存阻塞，落地 digest-only 的 PilotEvidenceManifest 小切片，不进入 Root、Trusted CI、registry 或 RDFC admission。
- runtime/pilot-evidence 新增 manifest v1：绑定 sealed package、contract digest、sample digest；每条记录只序列化 metric id、provenance、字节数和 SHA-256，绝不序列化本地路径、源码、Finding、SCM 状态或凭据。
- CLI 新增 rf pilot evidence manifest PACKAGE OUTPUT --contract-digest HEX --sample-digest HEX --artifact METRIC=PATH；缺失、重复、替换、篡改和路径型 provenance 均 fail-closed。
- 新增规格 specs/pilot/pilot-evidence-manifest.yaml、ADR-0157 边界说明和 scripts/pilot-evidence-check.sh 的 manifest/tamper 验收场景。
- 远端按文件顺序推送：ffbbc33、f446e56、01356a5、e7bf733、a17d4d8、08eecf5、458a245。其中 08eecf5 补上路径型 provenance 拒绝，458a245 新建 manifest 规格。

### 验收证据

- source ../toolchain/ruleops-env.sh && cargo test -p erf-pilot-evidence --lib --bins：12 passed / 0 failed。
- source ../toolchain/ruleops-env.sh && bash scripts/pilot-evidence-check.sh .scratch/pilot-evidence-check-20261026：PASS，产出 executable evidence；git diff --check 和 bash -n scripts/pilot-evidence-check.sh rf 通过。
- cargo fmt --check 未运行成功，原因是当前 Rust toolchain 未安装 cargo-fmt；不将其写成通过。

### #204 复核结论

- 当前 checkout、refs 与不可达对象均没有历史 .scratch/a-201/report.md、coord-201 或 probe-replay-r1，因此 L-2 的 76/64、47/48、349/524 数字无法安全修改；当前可见 scripts/check.sh 与 docs/guide/testing.md 均为 51 个 stage。
- 该历史报告缺失已记录为待恢复证据后的人工复核项；本轮不制造报告、不伪造跨进程 fixture。

### 圆桌增补裁决

- 推荐顺序：Issue evidence manifest 已完成；下一步是只读 Attempt integrity dashboard，再做 Environment readiness doctor；Roundtable replay pack 先保持设计稿。
- 明确不做：Root/HSM、Trusted CI、registry、RDFC admission、protected-branch authority 和任何会扩大生产权限的协议/身份变更。
- 继续保持 LOCAL_UNATTESTED / SHADOW / RDFC_NOT_ADMITTED，所有新增功能必须保留 fail-closed 与可复跑证据边界。

### 维护入口

- 本文仍是唯一待办入口；后续新增需求、状态、决策、阻塞与证据只追加新节，不另建并行清单。


### 17.1 Manifest follow-up hardening

- CLI artifact SHA-256 的返回类型已显式固定为 32-byte digest，避免无 cargo 环境下的类型推断歧义；远端提交 bd1dd5d。
- 该修复不改变协议、权限或信任边界；沿用 §17 的 12/12 Rust 单测与 pilot-evidence-check 证据。


## 18. Attempt integrity dashboard slice (2026-10-26)

### 已编码并推送

- 新增 runtime/host-client 的只读 AttemptIntegrityDashboard，不扩展 protobuf/ExecuteComplete 字段，不接 Runner Gate、SCM、Root 或 Trusted authority。
- 汇总 planned/status/completion/observation/attempt digest 既有事实，检查重复/空 ID、planned↔status 集合、UnitOutcome、units_reported、observation_count 与 Host/consumer digest；缺终止帧或本地 digest 时为 INCONCLUSIVE，矛盾时为 FAIL_CLOSED，并输出稳定 reason code 与 digest-only render_line。
- ExecuteStream 暴露既有 semantic digest 的只读计算与计数 accessor，仍沿用原有 fail-closed 校验。
- 新增 scripts/attempt-integrity-check.sh 作为本地可复跑验收，修正 heredoc 命令替换风险。
- 远端按文件顺序推送：92788ab、c5a138b、d527b23；本地聚合提交为 138edea。

### 验收证据

- source ../toolchain/ruleops-env.sh && cargo test -p erf-host-client --lib attempt_integrity：4 passed / 0 failed。
- source ../toolchain/ruleops-env.sh && bash scripts/attempt-integrity-check.sh .scratch/attempt-integrity-20261026：PASS。
- git diff --check、bash -n scripts/attempt-integrity-check.sh 通过。
- 当前仍未把 dashboard 字段并入 ScanReport/Runner 输出，避免无 ADR 的 schema/output churn；后续若要对外暴露需单独裁决。

### 当前决策

- 推荐继续保持 standalone/read-only dashboard，先积累本地 evidence；不把 VERIFIED 误写成生产 Gate PASS。
- 下一候选为环境 readiness doctor（只读、输出 BLOCKED JSON），但继续忽略 JDK/Maven/DNS 依赖恢复，直到用户改变优先级。


### 18.1 Dashboard follow-up hardening

- 补充 `missing computed digest -> INCONCLUSIVE` 回归，并修正注释以明确“缺终止帧但存在更强矛盾时仍 FAIL_CLOSED”。
- 验收脚本现在把相对 OUT_DIR 规范化为绝对路径，避免切换工作目录后证据写入错误位置。
- 远端提交：8e5bb6c、5be29c6；本地 follow-up 为 7f60e98。
- 复验：`source ../toolchain/ruleops-env.sh && cargo test -p erf-host-client --lib attempt_integrity`：5 passed / 0 failed；相对路径脚本验收 PASS；`git diff --check`、`bash -n` 通过。


## 19. Environment readiness doctor slice (2026-10-26)

### 已编码并推送

- 新增 scripts/environment-readiness-doctor.sh：只读检查 java/javac/cargo/mvn/protoc、JDK 21+、Maven DNS 与本地缓存，输出 environment-readiness-v1 JSON、命令日志及 stdout/stderr 原件。
- 默认模式是诊断-only：即使结果 BLOCKED 也 exit 0；--strict 才 exit 3。脚本不执行依赖下载、不写缓存、不修改 DNS/代理/系统设置、不加载信任材料，报告明确 gate=NOT_RUN、trust=NOT_EVALUATED、diagnosticOnly=true。
- 新增 scripts/environment-readiness-doctor-test.sh，使用 fake toolchain 验证 READY、缓存带 .lastUpdated 时 BLOCKED 以及 strict exit 3。
- 远端提交：b305f43、dcb383c；本地聚合提交 cdf6600。

### 验收证据

- 真实环境默认诊断：overall=BLOCKED、exit 0；--strict：overall=BLOCKED、exit 3。当前观察到 java READY，其余工具或 Maven DNS/cache 仍按事实报告 BLOCKED。
- shell 回归：scripts/environment-readiness-doctor-test.sh 输出 PASS；bash -n 和 JSON 解析通过。
- 该工具只是环境可观测性，不改变用户已确认的“暂缓 JDK/Maven/DNS、优先编码”策略，也不把 BLOCKED 误报为全量门禁结果。

### 圆桌更新

- 证据 manifest、Attempt integrity dashboard、Environment readiness doctor 三项均已落到可复跑边界内；下一步只做低风险证据/消费者体验增强。
- Root/HSM、Trusted CI、registry、RDFC admission、protected-branch authority、Server 合法晋级及协议扩展继续保持明确选项/推荐项/阻塞条件，不自动实施。


### 19.1 README 入口说明

- README 已补充 readiness doctor 的用法、BLOCKED/strict 语义和不改变 trust/gate 的边界；远端提交 dfbbe13。


### 19.2 Readiness doctor hardening

- DNS 探测失败时现在继续尝试下一个 resolver，并保留最终诊断事实；多行 Maven settings.xml 的 localRepository 解析也已补齐。
- 远端提交 32b7cab（对应本地 56baa55）；回归测试仍通过。


### 19.3 Probe-log completeness

- readiness doctor 现在把 Maven cache 的只读 find 探针写入 commands.log，保证 BLOCKED/READY 结论可复跑；远端提交 bc3c41f（对应本地 0a54bbf）。


### 19.4 Renderer path hardening

- readiness doctor 在调用者缩窄 PATH 时回退到 /usr/bin/python3 渲染 JSON，避免把报告生成器路径缺失误报为环境事实；远端提交 2521146（对应本地 158d639）。


## 20. Remaining low-risk backlog (2026-10-26)

- Evidence claim linter：已形成带 PASS/WARN/FAIL fixture 的可交付提交，并通过 scripts/evidence-claim-lint-test.sh；不把静态 lint 结果升级为行为 Gate 结论。
- 下一次可执行顺序：先记录跨语言 ExecuteComplete 负例，再补 schema-only roundtable replay pack；两者都复用现有协议字段，不新增 nonce/信任字段。
- 若需要对外扩展 ScanReport/Runner 输出，先新增 ADR 与兼容性证据；当前 AttemptIntegrityDashboard 保持 standalone/read-only。


## 21. Evidence claim linter slice (2026-10-27)

### 已编码并推送

- 新增 scripts/evidence-claim-lint.py 与 wrapper：对 evidence-claims-v1 manifest 或 Markdown marker 做 provenance 检查，要求每条 claim 具备 source file:line、replay command、evidenceLevel、commitSha、blocker responsibility、consumer responsibility。
- grep/rg/wc/count-only 命令只能产生 WARN，永不产生 PASS；退出码固定为 PASS=0、WARN=2、FAIL=1，避免计数型静态证据被误读为行为通过。
- 新增 PASS/WARN/FAIL fixtures、shell regression、以及 docs/research/evidence-claim-linter-v1.md。仅改 docs/scripts，未改生产 trust、协议或 authority。
- 远端按文件顺序推送：e9084c8、2aba4e6、41a9264、1ea7d42、a1e6a6d、f80264e、72369a1；本地聚合提交 fbddc25。

### 验收证据

- scripts/evidence-claim-lint-test.sh：PASS，覆盖完整 claim、grep/count warning-only、缺字段/非法 SHA 失败。
- py_compile、bash -n、git diff --check 通过；全量 scripts/check.sh 继续按用户指示暂缓，受 JDK/Maven/DNS 环境阻塞。

### 下一步裁决

- 推荐继续补跨语言 ExecuteComplete 负例，但只复用现有字段；若出现协议/nonce/身份扩展需求，先回到 ADR 与用户裁决。


## 22. Cross-language ExecuteComplete negative conformance (2026-10-27)

### 已编码并推送

- Rust Fake/Node Fake 新增仅测试用的既有字段扰动：FAKE_BAD_UNITS_REPORTED，以及 FAKE_STATUS_ID_MODE=duplicate|unknown|blank；不新增 protobuf 字段、不改 Java、不接 trust/authority。
- 新增 scripts/execute-complete-negative-check.sh：先跑现有 Rust/Node accounting tamper 证据，再验证两种 Fake 对重复、未知、空白 UnitStatus.unit_id 均 fail-closed；Node blank 场景同时验证 canonical wire digest 不匹配也会被拒绝。
- 远端按文件顺序推送：930d52d、d433d36、a504a49、41a993d、5ebf598、13d1d17；本地聚合提交 13f7ce7。

### 验收证据

- cargo test -p erf-conformance --lib：4/4（使用 PROTOC=/usr/bin/vtkprotoc-pv5.13）。
- cargo build -p erf-conformance -p erf-fake-host：通过；node --check backend-hosts/node-fake/host.js、bash -n、git diff --check 通过。
- 新脚本完整运行：Rust/Node duplicate、unknown、blank 六个负例均 PASS；现有 units_reported/observation_count/attempt_digest tamper probe 保持通过。
- cargo check/full gate 未宣称通过；JDK/Maven/DNS/cache 阻塞仍按前节记录。

### 裁决

- 仅保留 Rust/Node Fake 负例，避免 Java Fake 缺少注入能力导致共享 conformance 误报；不新增 nonce、身份或协议字段。

## 23. Roundtable replay pack v1 (2026-10-27)

### 已编码并推送

- 新增 docs/research/roundtable-replay-v1.md、三席决策 fixture、静态 checker 与负例测试；仅 docs/fixture/script，不改 protobuf、Runner、Trust、SCM 或生产权限。
- fixture 记录 runtime-security、platform、governance-audit 三席输入，明确 option、recommendation、blockingConditions、evidence 与 boundary；当前 reviewStatus=PROPOSED，不冻结生产决策。
- 远端按文件顺序推送：fb364db、810f799、78bd428、439326d；本地聚合提交为 dc8663a（首版 294cfc4，随后补 review-aware 字段）。

### 验收证据

- bash -n scripts/roundtable-replay-check.sh scripts/roundtable-replay-check-test.sh：通过。
- ./scripts/roundtable-replay-check-test.sh：roundtable-replay-test=PASS；默认 checker PASS。
- git diff --check：通过；不依赖 Cargo/Maven/网络。
- 该切片是 schema-only/static replay evidence，boundary=LOCAL_UNATTESTED；checker 不宣称已执行 fixture 中的业务命令，也不产生生产 Gate/Trusted 结论。

### 后续增强（非阻塞）

- 可继续增强 source file:line、commit SHA 格式和命令可执行性校验；当前不阻塞合并，不扩大信任边界。


### 23.1 Replay checker hardening

- checker now validates `source` as file:line and `commitSha` as a hexadecimal 7-40 character reference; negative fixture covers invalid source location.
- 远端更新：8e1904b（checker）、63120d6（negative test）；本地聚合提交 7c4df78。
- 验收：roundtable-replay-test=PASS、默认 checker PASS、bash -n、git diff --check；仍为 schema-only/static evidence，不执行业务命令，不扩大 authority。


### 23.2 README entrypoint

- README.md now documents the roundtable checker and its `PROPOSED` / `LOCAL_UNATTESTED` boundary; it explicitly states the checker does not execute business commands or produce production Gate/Trusted conclusions.
- 远端提交：56e04a3；本地提交 cfaeae3。
- 验收：roundtable checker PASS、bash -n、git diff --check；无 Cargo/Maven 依赖。


## 24. Production authority scope decision (2026-10-27)

- 用户提出“授权生产权限”，但范围尚未明确；该请求不能自动解释为合并主分支、Trusted CI、registry、Root/HSM 或 ExceptionGrant 全部授权。
- 明确选项：
  1. 仅允许当前分支合并主分支（不改变 runtime trust/authority）。
  2. 允许接入生产 Gate、Trusted CI、registry 与状态发布（需要身份、attestation、回滚与审计契约）。
  3. 允许 Root/HSM、ExceptionGrant 签发/撤回等生产授权能力（需要双人审批、custody、recovery、revocation 与运维验收）。
- 推荐项：先选择 1；若需要 2 或 3，先分别冻结 ADR、威胁模型、验收证据和回滚方案，再按最小范围实施。
- 当前阻塞：等待用户明确编号组合；在此之前继续保持 `LOCAL_UNATTESTED` / `SHADOW` / `RDFC_NOT_ADMITTED`，不授予生产 authority。


### 24.1 Scope approved by user (2026-10-27)

- 用户已明确批准范围 1/2/3：主分支合并；Trusted CI、registry、生产 Gate 与状态发布；Root/HSM、ExceptionGrant 签发/撤回。
- 执行顺序：先完成 ADR、威胁模型、回滚/恢复和验收契约；再按单一动作拆分实际权限变更。
- 当前仍不等同于已启用生产 authority：每个具体启用动作需在目标、权限变化、影响面和回滚点明确后进行 action-time confirmation；在确认前保持 fail-closed。
- 推荐顺序：主分支合并准备 → Trusted CI/registry 准入准备 → Root/HSM/ExceptionGrant 运维准备；任何一步失败都回滚到 `LOCAL_UNATTESTED` / `SHADOW` / `RDFC_NOT_ADMITTED`。


### 24.2 Rollout plan prepared

- 新增 `docs/research/production-authority-rollout-plan-20261027.md`，将 1/2/3 拆成 branch promotion、Trusted CI/registry、Root/HSM/ExceptionGrant 三阶段，分别定义输入、证据、回滚和 fail-closed 不变量。
- 远端提交：7bf1583；本地提交 aeefaba。
- 该文档只记录准备方案，不启用任何 production authority；每个实际启用动作仍需 action-time confirmation。


## 25. PR #248 mergeability investigation (2026-10-27)

- PR #248 targets `master`; GitHub reports `open`, `merged=false`, `mergeable=false`.
- Compare evidence: branch is ahead 93 and behind 0; local `git merge-tree` found no content conflict.
- GitHub status/workflow runs are empty and no PR reviews exist. Current diagnosis is repository policy/review or pending GitHub mergeability calculation, not a demonstrated file conflict.
- Action: do not force merge or bypass review. Keep PR open until review/branch policy state is observable; production authority remains disabled.


### 25.1 Manual merge verified (2026-10-27)

- 用户已手动合并 PR #248；远端 master 最新合并提交为 `2a64fa5`，提交信息包含 `(#248)`。
- 已复核 master README 与维护文档包含本轮内容；本次合并只落地代码/文档，未启用 Trusted CI、registry、Root/HSM、ExceptionGrant 或生产 Gate authority。
- 后续仍按 rollout plan 分阶段执行；任何生产 authority enablement 需 action-time confirmation 与独立证据/回滚点。
