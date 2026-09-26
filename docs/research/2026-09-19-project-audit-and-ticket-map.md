# RuleOps 当前项目双轴审计与拆票地图

- 审计日期：2026-09-19
- 审计范围：当前工作树（分支 `codex/phase1-contract-factory`，基线 `origin/codex/phase1-contract-factory` / `ef926dc`）
- 审计方式：产品范围审计 + 实现质量审计；两路只读 agent 独立执行后合并
- 变更约束：本审计未修改既有代码、决策或 GitHub Issues
- 远端状态：已使用 `env -u GITHUB_TOKEN gh` 回读本机有效凭据；新增 9 张票已发布为 GitHub Issues #113–#121，人工执行沿用 #89，既有地图也已追加审计 Log

## 结论

产品建议为 `narrow`：保留 Java/PMD、20 条候选 RuleIntent、3 个登记仓库、SHADOW、digest-only、三态结果和失败证据；在正式 Pilot 证据闭合前暂停继续扩展 Server、IDE、SCM Adapter、增量扫描等横向产品面。

实现质量结论为 `REVISE`：当前 seam 的边界设计大体清楚；ChangedSet 冲突拒绝、Server 生命周期版本寻址、Journal 恢复、Thin Client 未知响应、TrustState 持久化确认、验收脚本可信度和 PilotEvidencePackage 机器边界已完成工程收口，真实人工 Pilot 证据仍待窗口输入。

机器侧的 `PilotEvidencePackage + PilotAdjudication` 关口已完成；下一主线是由 #89 在正式
窗口采集真实 LIVE_SHADOW、作者主动时间和人工质量证据。没有完整证据和人工裁决，不得
把工程 MVP、局部脚本 PASS 或本地合成 Shadow 升格成产品 GO、Advisory 或 protected-branch authority。

## 定位与边界

暂定定位：

> 面向负责企业 Java 规范治理的 Platform Engineering / RuleOps 治理负责人，在规范需要机器化但不能把扫描、审批和 SCM 权限集中到平台时，ERF 将 Standard/Scenario 转化为可验证、可审计、可在本地或 CI Shadow 执行的 Rule Asset，并保留完整性、信任和失败证据。

主 job 是判断一条新增或升级的企业规范是否有清晰机器覆盖、是否经过足够验证、扫描是否完整可信，以及是否可以安全进入下一授权阶段。当前最小可交付结果是绑定合同、样本、运行、质量和安全证据的 digest-only Shadow 证据包及人工 `GO` / `NO_GO` / `INCONCLUSIVE` 裁决，而不是规则数量或模块数量。

| 边界 | 当前判断 |
|---|---|
| Own | Standard/Scenario/Coverage、RuleIntent/IR、Candidate factory、EvidenceBundle、Completeness、三态 Gate、信任与审计语义、Shadow 证据 |
| Integrate | PMD、SpotBugs、Java Backend Host、Workspace Resolver、外部 CI、SCM adapter、凭据/Secret Store |
| Defer | 生产级 Server、认证/TLS/HA、远程 Registry、对象存储、真实 IDE、`SHADOW → ADVISORY → BLOCKING`、增量依赖闭包 |
| Exclude | 新建分析引擎、LLM 直接裁决 CI、默认上传源码/完整 Finding、Runner/Thin Client/Server 持有 SCM 写权限 |

## 证据摘要

| 类型 | 结论 | 证据 |
|---|---|---|
| Verified fact | 当前核心工程闭环是 Java/PMD、20 条候选、3 个登记仓库、SHADOW/digest-only；RuleOps Center 自身标为 `ENGINEERING_MVP` | `README.md:45-137`；`specs/center/ruleops-center-closure.yaml` |
| Verified fact | `PilotEvidencePackage` / `PilotAdjudication` 已由 ADR-0133 要求，现由 ADR-0157、`runtime/pilot-evidence`、versioned spec 和独立可执行 check 实现 | `docs/adr/0157-pilot-evidence-package-and-adjudication-runtime-boundary.md`；`.scratch/pilot-evidence-check-113-final/pilot-evidence.md` |
| Verified fact | 作者主动时间仍为 `HUMAN_INPUT_REQUIRED`，正式窗口质量/资源输入缺失时保持 `INCONCLUSIVE` | `docs/pilot/author-cost-baseline.md:63-73`；`specs/pilot/pilot-release-manifest.yaml:29-33` |
| Verified fact | 内部 Phase 4 Shadow 验收脚本创建本地合成仓库；外部 LIVE_SHADOW 是另一条路径 | `scripts/phase4-shadow-pilot.sh:44-62`；`scripts/pilot-live-observation.sh` |
| Verified fact | Server 是 loopback/local journal seam，Thin Client/Status 不持有 SCM 权限 | `control-plane/server/src/main/java/io/ruleops/server/RuleOpsServerMain.java`；`contracts/http/*.yaml` |
| Verified fact | 当前工作树有大量未提交的新模块、合同、ADR 和脚本；`.scratch/` 含既有运行产物 | `git status --short`；`git diff --stat` |
| Critical | ChangedSet 只对提交间 `git diff`，没有先拒绝当前 index/worktree 的真实未合并冲突 | `runtime/git-scope/src/lib.rs:251-275`；`docs/adr/0145-git-changed-set-scope.md:19-35` |
| Critical | Server `transition()` 对同一 `type + id` 使用 `findFirst()`，没有按 immutable `version` 精确寻址 | `control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:207-260`；`contracts/http/ruleops-api-v1.yaml` |
| Required | Journal 上限约 4 GiB，却用 `Files.readAllBytes()` 全量恢复，并以替换式 UTF-8 解码 | `control-plane/server/src/main/java/io/ruleops/server/FileRuleOpsJournal.java:31-63` |
| Required | Thin Client 把所有非 2xx 且 `<500` 响应归为 rejected，3xx 未按未知响应处理 | `runtime/thin-client/src/transport.rs:275-287`；`docs/adr/0141-thin-client-server-submission-transport.md` |
| Required | TrustState 原子替换后目录 `sync_all()` 错误被忽略，无法证明持久化已落盘 | `runtime/trust-state/src/lib.rs:194-215`；`docs/adr/0144-persisted-trust-checkpoint-handoff.md` |
| Required | 多个 feature check 主要依赖文件存在和关键词匹配后输出 `PASS`，不能独立证明实现级行为 | `scripts/bundle-delivery-check.sh`；`scripts/server-contract-check.sh`；`scripts/thin-client-check.sh` |

## 实现状态增补（2026-09-19）

本审计快照中的 #116 Server 生命周期寻址缺口已完成修复并关闭：

- `RuleOpsStore` 使用 `VersionQualifiedLifecycleTransition` 和完整 `type + id + version`
  key 精确查找，不再按同一 `type + id` 使用 `findFirst()`。
- HTTP lifecycle request 的路径提供 `id`，JSON body 只允许文本 `version`；缺失版本返回
  `RESOURCE_VERSION_REQUIRED`，同 ID v1/v2、跨版本 operation id 冲突和重启回放均有测试。
- ADR-0154、`ruleops-api-v1.yaml` 和 `server-contract-check.sh` 已同步冻结该边界。

因此上表 #116 的 Critical 项以 ADR-0154 的实现状态为准；当时未完成的相邻机器票为
#114、#113，人工输入边界仍由 #89、#100、#101 等票负责。

#115 的 ChangedSet 冲突边界也已完成并关闭：ADR-0155、真实 merge/rebase fixture、真实
criss-cross multiple merge-base fixture 和 `feature-evidence-v1` 执行证据均已落地；当前
相邻推荐机器票为 #114、#113（#114 后续已由 ADR-0156 收口）。

#114 的观测模式边界也已完成并关闭：ADR-0156 将 `LOCAL_SIMULATION`、内部
`SYNTHETIC_SHADOW` 与登记的外部 `LIVE_SHADOW` 固定为不可互换的聚合域；Journal、观测
记录和报告都绑定 repository、commit、run identity 与 rule-bundle digest。窗口报告要求
精确的登记样本，模式、commit 或 bundle 不匹配均 fail closed；合成资格报告可以有工程
`PASS`，但产品结果固定为 `INCONCLUSIVE`。#113 的机器证据包关口随后已完成；后续输入
转入 #89 的正式人工 Pilot window。

#122 的 Fast Decision Evidence 影子分流边界也已完成并关闭：ADR-0158、
`fast-decision-v1` contract 与 `runtime/fast-decision` 已提供 provider-neutral 的
CHOICE/SCORE/BOOLEAN 类型化证据、确定性 baseline、holdout/near-miss/低置信度/过期/冲突/
provider 不可用/非法输入回退，以及带 disagreement、abstain、latency、calibration 的
ShadowRecord。证据和 ShadowRecord 均只能通过已校验构造路径生成，字段封装、spec/expiry/
digest/status 绑定和极值边界均 fail closed；15 个单测、专项 executable check 与全量门禁
通过。该层仍是 `SHADOW_ONLY`，不改变 Gate、Trust、Exception、retry 或 SCM 权限，不能替代
#89 的真实人工 Pilot window。

## 当前旅程断点

### 产品用户

`Standard/RuleIntent YAML → ./rf → Candidate/EvidenceBundle → rf scan → Shadow observation → feedback/exception` 能在仓库内运行，但设置依赖 JDK/Maven/Cargo/checkout，FindingView 需要人工准备，提交只产生 `RECEIVED_ONLY`，没有审批、Gate 变更或 SCM 发布。该路径适合受控 Shadow operator，不是已验证的自助产品。

### Builder / Operator

`Standard → Scenario/Coverage → Candidate → EvidenceBundle` 的局部闭环较强；digest-only
证据包与机器裁决边界已经接通，但正式 LIVE_SHADOW 数据、人工质量/作者成本证据仍未
输入。80 条 Standard 仍是 inventory-only，三类模板的作者成本结论仍是假设。

## 拆票原则

1. Critical 正确性缺口先于横向扩展。
2. 机器实现票与需要治理负责人/真实作者输入的票分开。
3. 只有 `PilotEvidencePackage` 完整且人工裁决为 `GO`，才允许开启后续产品能力票。
4. 本地合成、离线 Shadow、在线 LIVE_SHADOW 必须在报告身份上不可混淆。
5. 每票必须有可观察验收条件；没有条件就标 `needs-info` 或 `ready-for-human`，不伪装成可直接落码。

## 已发布票据与地图映射

| 审计 ID | GitHub Issue | 主题 |
|---|---:|---|
| `AUDIT-00` | #86 | 复用既有 Pilot Go/No-Go 总览地图 |
| `AUDIT-01` | #113 | `PilotEvidencePackage` / `PilotAdjudication` |
| `AUDIT-02` | #89 | 沿用人工 Shadow window 执行票 |
| `AUDIT-03` | #114 | 观察模式身份隔离 |
| `AUDIT-04` | #115 | ChangedSet 真实冲突 fail closed |
| `AUDIT-05` | #116 | Server lifecycle `id + version` |
| `AUDIT-06` | #117 | Journal 流式严格恢复 |
| `AUDIT-07` | #118 | Thin Client 3xx unknown |
| `AUDIT-08` | #119 | TrustState 目录 fsync 错误传播 |
| `AUDIT-09` | #120 | feature check 可执行证据 |
| `AUDIT-10` | #121 | governance `requestId` 人工决策 |
| `AUDIT-11` | #111 | Pilot GO 后的 RuleBundle 分发主路径 |
| `AUDIT-12` | #107 | Pilot GO 后的 FindingView/诊断主路径 |
| `AUDIT-13` | #122 | Fast Decision Evidence 影子分流边界 |
| `AUDIT-14` | #108 | 项目配置、`rf init`/`rf doctor` 与扫描范围预检 |
| `AUDIT-15` | #109 | UNKNOWN 提交结果查询与恢复 |

人工执行沿用 #89；V0.4 主线沿用 #92、#107、#111，不重复开票。

## 票据正文摘要

### AUDIT-00 — Pilot evidence gate（复用 #86）

- 建议标题：`[P0] Close the PilotEvidencePackage and adjudication gate before product expansion`
- 建议标签：`needs-triage`
- 类型：地图/总览票
- 目标：把合同 digest、固定样本、观察窗口、作者成本、质量、运行、结果和安全证据组成独立 digest-only 包，并由治理负责人输出 `GO` / `NO_GO` / `INCONCLUSIVE`。
- 阻塞边：`AUDIT-01`、`AUDIT-02`、`AUDIT-03`；工程修复票 `AUDIT-04` 至 `AUDIT-09` 应在进入正式裁决前完成或明确豁免。
- 验收：`FAIL > INCONCLUSIVE > PASS` 聚合；缺失、冲突、篡改或不完整证据绝不能产生 `GO`；`GO` 前不授予 protected-branch 或 SCM 权限。

### AUDIT-01 — Implement PilotEvidencePackage and PilotAdjudication（#113）

- 建议标题：`[P0] Implement the digest-only PilotEvidencePackage and PilotAdjudication boundary`
- 建议标签：`ready-for-agent`
- 依赖：`AUDIT-00`、ADR-0133、ADR-0136
- 验收：提供 versioned schema、canonical digest、合同/样本/窗口身份；每条指标带 provenance、时间范围、完整性和 artifact digest；实现 `FAIL > INCONCLUSIVE > PASS`；缺失数据不得生成 `GO`；加入完整包、缺失、冲突、篡改、错误合同 digest 的负例测试。
- 状态：已完成；ADR-0157、`runtime/pilot-evidence`、`rf pilot evidence` 和
  `scripts/pilot-evidence-check.sh` 已落地。合同 id/version、批准窗口、合同/样本
  digest 均在裁决前绑定；解析失败和缺失 digest 输出 `INCONCLUSIVE`。

### AUDIT-02 — Collect human pilot evidence（沿用 #89）

- 建议标题：`[P0] Run the formal Shadow window and record author/quality/resource evidence`
- 建议标签：`ready-for-human`
- 依赖：`AUDIT-01`、已批准 `PilotSuccessContract`、登记仓库清单、正式窗口 `2026-10-01` 至 `2026-10-31`
- 验收：三仓库 LIVE_SHADOW 运行记录、三类模板配对作者主动时间、人工 precision/recall/false-positive 标注、延迟/内存基线、恢复/回滚记录均有身份和摘要；缺测保持 `INCONCLUSIVE`；不得用机器耗时或本地 fixture 代替作者证据。
- 人工决策：治理负责人是否批准窗口、谁作为真实 Rule Author/Reviewer、如何记录主动时间和质量标签。

### AUDIT-03 — Separate observation modes（#114）

- 建议标题：`[P0] Make local simulation, synthetic qualification, and live shadow non-interchangeable`
- 建议标签：`ready-for-agent`
- 依赖：ADR-0133、ADR-0134、现有两个 Shadow 入口
- 验收：每份报告强制写入 `ObservationMode`、repository identity、commit、rule-bundle digest；工程门禁不能把 `org/ruleops-shadow-*` 合成仓库计为外部产品结果；`LIVE_SHADOW` 窗口数据独立汇总并在模式错配时 fail closed。

### AUDIT-04 — Reject unmerged worktree before ChangedSet（#115）

- 建议标题：`[P0] Reject unmerged worktree state before generating ChangedSet`
- 建议标签：`ready-for-agent`
- 依赖：ADR-0145
- 验收：真实 merge/rebase conflict 的 index/worktree 在 ChangedSet 生成前返回结构化拒绝；干净提交范围仍绑定 base/head/merge-base 并产生稳定 digest；补充冲突、空差异和路径安全测试。

### AUDIT-05 — Qualify lifecycle transitions by version（#116）

- 建议标题：`[P0] Make RuleOps Server lifecycle transitions version-qualified`
- 建议标签：`ready-for-agent`
- 依赖：ADR-0137、`contracts/http/ruleops-api-v1.yaml`
- 验收：transition 请求携带并校验 `id + version`；同一 ID 多版本时缺失 version 返回 `RESOURCE_VERSION_REQUIRED`；指定版本只改变精确资源；幂等重放和 journal replay 保持同一资源与审计链；补充 v1/v2 同 ID 测试。

### AUDIT-06 — Harden Journal recovery（#117）

- 建议标题：`[P1] Stream and strictly decode RuleOps journal recovery`
- 建议标签：`ready-for-agent`
- 依赖：ADR-0138
- 验收：恢复逐行流式读取，不全量载入文件；总文件和单条记录上限均生效；非法 UTF-8、空行、截断尾项、超限、摘要链错误均 fail closed；既有 append-before-mutation 和重启恢复测试继续通过。

### AUDIT-07 — Classify unknown HTTP responses correctly（#118）

- 建议标题：`[P1] Treat only known 4xx responses as submission rejection`
- 建议标签：`ready-for-agent`
- 依赖：ADR-0141、`contracts/http/thin-client-v1.yaml`
- 验收：已知 4xx 为 `SUBMISSION_REJECTED`；3xx、5xx、连接/超时、畸形响应为 `SUBMISSION_UNKNOWN`；未知结果不产生成功 receipt、不自动重试；补充 301/307/308、空响应和错误 JSON 测试。

### AUDIT-08 — Propagate directory-sync failure（#119）

- 建议标题：`[P1] Propagate TrustState directory-sync failure after atomic replacement`
- 建议标签：`ready-for-agent`
- 依赖：ADR-0144
- 验收：目录 fsync 失败返回明确错误；调用方不得拿到“已持久化” handle；旧状态仍可读，临时文件清理不掩盖原始错误；补充注入失败测试。

### AUDIT-09 — Make contract checks executable evidence（#120）

- 建议标题：`[P1] Replace grep-only contract checks with executable evidence checks`
- 建议标签：`ready-for-agent`
- 依赖：`docs/agents/dev-workflow.md`、各 ADR 验收命令
- 验收：每个 feature check 至少执行对应 Cargo/Maven 测试及关键负例；静态存在性检查单独标为 `STATIC_ONLY`；输出区分 `PASS`、`STATIC_ONLY`、`BLOCKED`；脚本失败日志可定位到具体测试和输入。

### AUDIT-10 — Specify request identity formula（#121）

- 建议标题：`[P1] Specify and independently verify content-addressed governance request identity`
- 建议标签：`question`, `ready-for-human`
- 依赖：ADR-0141、Server API contract
- 人工决策：`requestId` 是由 canonical request envelope 计算，还是由调用方提供并以另一摘要绑定；需要明确跨客户端/Server 的唯一算法、字段顺序和版本化规则。
- 验收：契约写出 canonicalization 与 SHA-256 计算式；客户端和 Server 独立实现；任意格式正确但与 envelope 不匹配的 request ID 被拒绝；跨实现产生相同 digest。

### AUDIT-11 — Connect the approved-asset execution path（沿用 #111，延期条件已回写 #92）

- 建议标题：`[P1] Connect Candidate approval to RuleRelease, RuleBundle, Trusted Runner, and GateAttestation`
- 建议标签：`ready-for-agent`
- 依赖：AUDIT-00 `GO`；release registry、bundle cache、trust-state、Runner
- 状态：`defer until pilot GO`
- 验收：从批准 Candidate 生成签名 RuleRelease/RuleBundle，经精确 cache/trust closure 后执行并生成 GateAttestation；`rf scan` 不再直接绕过已批准资产消费 Candidate 目录；任何身份、撤回、有效期或信任缺失保持 fail closed。

### AUDIT-12 — Connect FindingView to scan output（沿用 #107，延期条件已回写 #92）

- 建议标题：`[P1] Emit FindingView directly from a real scan into explain and governance requests`
- 建议标签：`ready-for-agent`
- 依赖：AUDIT-00 `GO`、AUDIT-11、Thin Client contracts
- 状态：`defer until pilot GO`
- 验收：一次真实扫描可直接产生闭合 digest-only FindingView；`rf explain`、feedback/exception draft/submit 可复用同一 finding identity；receipt 继续明确区分 received、approved、Gate 和 SCM 状态。

### AUDIT-13 — Implement Fast Decision Evidence shadow boundary（#122）

- 建议标题：`[P1] Implement provider-neutral Fast Decision Evidence for shadow-only routing`
- 建议标签：`ready-for-agent`
- 依赖：既有结构化诊断、digest-only FindingView、Pilot evidence boundary；不依赖真实 provider 或人工 Pilot GO。
- 验收：versioned contract 支持 CHOICE/SCORE/BOOLEAN；adapter/baseline 产生绑定
  input/spec/provider/model/answer/probability/confidence/threshold/fallback/time/digest 的
  evidence；低置信度、margin、过期、冲突、provider 不可用和非法输入均 UNKNOWN 并回退；
  ShadowRecord 留存 disagreement/abstain/latency/calibration；执行证据不得触碰 Gate、Trust、
  Exception、retry 或 SCM。
- 状态：已完成；ADR-0158、`runtime/fast-decision`、`contracts/fast-decision-v1.yaml`、
  `rf decision evaluate`、专项 check 与全量门禁已落地并通过。

### AUDIT-14 — Bootstrap project configuration and doctor boundary（#108）

- 建议标题：`[P1] Implement rf init/rf doctor and fail-closed project scan configuration`
- 依赖：无；为 #112 提供项目配置、范围冻结和配置身份基础。
- 状态：已完成；ADR-0159、`runtime/project-config`、`rf init`/`rf doctor`、配置来源优先级、
  workspace/scan scope 绑定、digest-only 扫描身份和用户路径 evidence 已落地。
- 验收结果：配置缺失、依赖不可用、缓存/信任状态异常、symlink/越界/空范围、无交互不确定识别
  均 fail closed；`Connected`、`Offline`、`Standalone` 模式与来源展示可复核；`bash scripts/check.sh`
  全绿。
- 复核结果：Hegel/Turing 双轴复核均无 P0/P1；仅保留 CLI/env 逗号值编码的 P2 易用性项，不阻塞
  本票验收。

### AUDIT-15 — Recover UNKNOWN submission outcomes（#109）

- 建议标题：`[P1] Add read-only UNKNOWN submission status lookup and recovery`
- 依赖：#97 Thin Client submission transport；不授予审批、Gate 或 SCM 权限。
- 状态：已完成；ADR-0160、`submission-status-v1`、Server 查询入口、Rust transport parser、
  `rf submission status` 与专项 executable evidence 已落地。
- 验收结果：反馈/例外均可按 request ID 或 artifact digest 查询；`PENDING`、`RECEIVED_ONLY`、
  `REPLAYED`、`UNKNOWN` 语义分离；资源版本、artifact/request identity 和原始 audit context 绑定；
  摘要冲突返回 `SUBMISSION_IDENTITY_CONFLICT`；UNKNOWN/PENDING 不自动重试且 CLI 以 inconclusive
  exit code 返回；全量门禁通过。

## 暂不拆票

下列内容是明确的后续边界，不应在当前 Pilot 证据未闭合时制造工作量：

- 真实 SCM Status Adapter、凭据/TLS/KMS/HSM 和 protected-branch blocking；
- IntelliJ/VS Code UI；
- 远程 Registry、对象存储、HA、多进程并发和保留治理；
- 把 ChangedSet 接入完整增量依赖闭包；
- 扩大模板样本或宣称一般 Factory 成本结论；应先完成当前三条配对测量，再由结果决定。

## 验证记录

- `git diff --check`：PASS。
- `bash -n scripts/*.sh`：PASS。
- `cargo metadata --no-deps`：PASS。
- `bash scripts/check.sh`：#109 最终日志为 `.scratch/check-109-final6.log`，最终输出
  `check: all sides green`；其中作者成本基线为 `INCONCLUSIVE`（`pairs=0/3`），其余工程门禁、
  Phase 4 聚合和 RuleOps Center 闭环检查通过。
- `bash scripts/submission-status-check.sh .scratch/submission-status-check-109-v4`：PASS；覆盖
  feedback/exception、request/artifact 双查询、真实 in-flight `PENDING`、replay、unknown、
  selector/CLI 冲突、无 payload 状态投影和 CLI/transport/Server 行为证据；Rust 8 个 transport
  测试与 Java HTTP 13 个测试均为零失败、零 skipped。
- 既有局部证据只证明工程 seam 或 qualification，不证明产品指标、用户采用或生产授权。
