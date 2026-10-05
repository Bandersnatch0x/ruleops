# 01 — 定位与边界：规格符合度审计

## 域范围

- 树：WSL `kali-linux2:/home/kali/文档/ruleops`；HEAD `7e7d2820de5f2948bd936b64f5907ae211f00645`，以读取时工作树内容为准。
- 规格：`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md`，§0（13–40）、§1（44–61）、§2（65–104）。首先执行 `sed -n '29,115p'`，再以 `nl -ba ... | sed -n '8,108p'` 补读摘要与精确行号；29 行是 §0.1，而非 §0 开头。
- 55 条原文声明：摘要 4、核心原则 10、问题/目标 10、In Scope 22、Non-goals 9；另有 3 条用户指定补充边界，共 58 条。
- 判定统计：一致 30、部分一致 14、不一致 3、未实现 1、无法验证 10；不一致涉及 3 个独立根因。
- 方法：逐声明静态追踪源代码、调用入口、契约与 ADR；没有执行产品测试。下文“一致”只指已检查路径的静态一致性，不是生产验收通过。
- 只写本报告；未改 D: 树、源码、docs、scripts，也未调用 GitHub。

## 证据路径简称

所有路径相对上述 WSL 根；以下简称用于表内 `简称:行号`，不是仅凭文件名推定实现。

|简称|完整文件路径|
|---|---|
|STD|control-plane/authoring/src/main/java/io/ruleops/authoring/model/Standard.java|
|INT|control-plane/authoring/src/main/java/io/ruleops/authoring/model/RuleIntent.java|
|IR|control-plane/authoring/src/main/java/io/ruleops/authoring/model/RuleIrKind.java|
|ORCH|control-plane/authoring/src/main/java/io/ruleops/authoring/orchestration/AuthoringOrchestrator.java|
|COV|control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageAnalyzer.java|
|LEDGER|control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageLedger.java|
|CMD|control-plane/factory/src/main/java/io/ruleops/factory/CandidateCmd.java|
|COMP|control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java|
|LOCAL|control-plane/verification/src/main/java/io/ruleops/verification/LocalVerification.java|
|EVID|control-plane/verification/src/main/java/io/ruleops/verification/EvidenceBundle.java|
|HTTP|control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java|
|STORE|control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java|
|WORKER|runtime/verification-worker/src/lib.rs|
|RELEASE|runtime/release/src/lib.rs|
|REG|runtime/release-registry/src/lib.rs|
|GOV|runtime/governance-registry/src/lib.rs|
|BUNDLE|runtime/rule-bundle/src/lib.rs|
|GATE|runtime/gate/src/lib.rs|
|PLAN|runtime/scan-plan/src/lib.rs|
|CLI|runtime/rf-cli/src/main.rs|
|AUTH|runtime/authority/src/lib.rs|
|STATUS|runtime/status/src/lib.rs|
|FEEDBACK|runtime/feedback/src/lib.rs|
|METRICS|runtime/pilot-metrics/src/lib.rs|
|HOST|backend-hosts/java/src/main/java/io/ruleops/javahost/JavaBackendHost.java|
|HOSTCLIENT|runtime/host-client/src/lib.rs|
|VSC|clients/vscode/src/core.js|
|INVOKER|clients/vscode/src/rf-invoker.js|
|MCP|clients/mcp/erf-mcp-server.js|
|THIN|contracts/http/thin-client-v1.yaml|

## 逐声明核验表

### §0 Executive Summary

|ID / 规格声明（原文、章与行）|代码实证|判定|理由|
|---|---|---|---|
|S01 §0 L15–16：“可验证、可发布、可执行、可审计、可演进”|LOCAL:39–100；RELEASE:148–180；REG:91–140,168–192；GATE:548–592；STORE:327–335|部分一致|关键部件存在，但完整晋级治理与人工评审未闭合，见 F01/F04。|
|S02 §0 L18：“系统不重新实现静态分析器，也不承诺从任意两组正反例可靠合成任意静态分析规则”|LOCAL:60–79 调用 PMD；COMP:63–76 有限模板、未知 kind 拒绝|一致|是现有引擎集成与约束编译，不是任意样例合成。|
|S03 §0 L20–23：“Standard → Scenario → Rule Intent → Rule Implementation → Verification → Release → Enforcement → Feedback”的“完整 RuleOps 生命周期”|STD:18–27；INT:21–33；COMP:51–103；EVID:14–26；REG:91–140；FEEDBACK:188–229|部分一致|资产和阶段实现存在；Coverage→生产、Verification→Server 晋级缺少强制连接，生命周期不能仅由组件存在证明。|
|S04 §0 L25–27：“首期聚焦 Java，PMD 为主 Backend；SpotBugs 用于字节码语义类规则……Backend-neutral，可后续接入……”|HOST:47–55,150–156,189–190；`backend-hosts/java/src/main/java/io/ruleops/javahost/PmdBackendAdapter.java`、`SpotBugsBackendAdapter.java`；HOSTCLIENT:137–185|部分一致|中立握手与适配器边界可见，两个 Java 适配器存在；本域未深查每个适配器的语义正确性，不把可后续接入当作已支持列出的全部引擎。|

### §0.1 核心原则

|ID / 规格声明|代码实证|判定|理由|
|---|---|---|---|
|P01 §0.1 L31：“规范优先，不是规则代码优先。”|STD:18–27；INT:21–33；CMD:36–52|部分一致|Intent 带版本化 StandardRef，但独立构建入口只校验 Intent，没有读取规范目录/覆盖决策来保证流程优先级。|
|P02 §0.1 L32：“RuleIntent 与 RuleImplementation 分离。”|INT:13–33；COMP:51–103；RELEASE:159–169|一致|声明、IR/规则制品与签名 release 分离，release 绑定 intent digest。|
|P03 §0.1 L33：“AI 只参与 Authoring，不参与生产期裁决。”|ORCH:19–26,112–118；GATE:558–590；MCP:127–144|一致|Authoring 产建议；Gate 是确定性计算；MCP 调 CLI 而不是 LLM 裁决。运行域 LLM/API 关键词搜索未见直接裁决调用（搜索边界见后）。|
|P04 §0.1 L34：“执行必须确定性、可复现、可审计。”|COMP:100–103 内容摘要；GATE:573–574 排序去重；RELEASE:162–175 绑定锁与来源；STORE:329–335 审计 journal|部分一致|身份与审计机制存在；没有实际重复执行/跨环境复现证据，不能证明所有引擎执行都确定。|
|P05 §0.1 L35：“没有发现问题，不等于完成了检查。”|GATE:558–588；PLAN:304–352|一致|missing、failed、observation_errors 与 planned/accounted 独立于 findings；缺失和 unsupported 均不允许完整成功。空计划语义未在本域独立验证。|
|P06 §0.1 L36：“Gate 使用 PASS / FAIL / INCONCLUSIVE 三态。”|GATE:13,588–590；CLI:1574–1584；STATUS:35–41|一致|三态明确，INCONCLUSIVE 导致 CLI 3 与 SCM ActionRequired，不是 Success。|
|P07 §0.1 L37：“历史债务与新增违规分离。”|GATE:16,577–586；METRICS:176–177|一致|基于 baseline fingerprints 区分 Historical/New，只有未豁免的新违规参与阻断判断。|
|P08 §0.1 L38：“规则必须经过独立 Verification 才能晋级。”|HTTP:340–343,564–572；STORE:318–335,633–655；反证 REG:103–115,129–130；WORKER:261–280|不一致|Server release 路径允许 CANDIDATE→ACTIVE，只校验状态/身份/幂等，不要求验证证据。Rust registry 的签名校验和 Worker 套件不能约束此 HTTP 路径；生产运行时是否接受该 Server ACTIVE 未证明。见 F01。|
|P09 §0.1 L39：“中央治理、分布式执行。”|HTTP:327–353；STORE:327–335；HOST:47–55,189–190；INVOKER:21–35；AUTH:61–77|一致|中央资源服务、外部 backend host、本地 CLI 与单独权威计算边界存在；不等价于已验证多机部署。|
|P10 §0.1 L40：“先判断是否需要新规则，再进入规则生产。”|CMD:26–52；COMP:51–69；COV:13–59；HTTP:329–337|不一致|Coverage 是独立能力，CandidateCmd/Compiler 不要求其结果；HTTP coverage/analyze 与 generate 也只是分别创建资源。未找到 factory/server/CLI 调用 CoverageAnalyzer 或消费 CoverageLedger 的前置门禁。见 F02。|

### §1 Problem Definition

这些是外部现状与改善目标，不应把存在某个类当成企业现状或指标改善的证据。

|ID / 规格声明|代码实证|判定|理由|
|---|---|---|---|
|D01 §1 L49：“规范存在，但没有机器检查”|COV:13–59；STD:18–27|无法验证|实现可表达覆盖账本；没有企业全量规范/扫描库存，不能确认外部断层规模。|
|D02 §1 L50：“检查能力存在，但没有启用或配置不正确”|HOSTCLIENT:137–185；PLAN:317–352|无法验证|能力/执行完整性检测存在；没有真实企业配置证据。|
|D03 §1 L51：“规则存在，但误报、漏报、版本和适用范围不可治理”|INT:19,35–39；FEEDBACK:72–96,188–225；METRICS:179–182|无法验证|治理机制存在；不证明企业当前均存在此问题或已经解决。|
|D04 §1 L52：“CI 给出\"0 告警\"，但无法证明实际完成了预期扫描。”|GATE:575–590；CLI:1574–1584|无法验证|代码防范这一问题；没有现场 CI 历史数据来验证问题事实。|
|D05 §1 L54–56：“提高……Standard Coverage”|COV:48–59；LEDGER:15–46|无法验证|固定分母账本可支持计量；没有前后基线和实测改善值。|
|D06 §1 L54,57：“提高……Qualified Rule Coverage”|COV:62–72；REG:103–115；METRICS:168–189|无法验证|覆盖分类与 release 身份验证分离；未见本域已读统计中把真实资格证据合并成该改善指标，更无前后对照。|
|D07 §1 L54,58：“提高……Rule Quality”|LOCAL:83–100；METRICS:179–182|无法验证|fixture 与误报/回归计量存在，不证明真实规则质量已提升。|
|D08 §1 L54,59：“提高……Enforcement Reliability”|METRICS:169–175；GATE:575–590|无法验证|能计量完整扫描与不确定运行；没有历史对照。|
|D09 §1 L54,60：“提高……Rule Authoring Efficiency”|ORCH:19–26,68–88；METRICS:168–189|无法验证|编排可支持效率，但所读 metrics 聚焦运行/反馈，缺少规则生产耗时基线。|
|D10 §1 L54,61：“提高……Governance Auditability”|STORE:329–335；METRICS:413–446|无法验证|持久审计链可见，尚无运营场景下可审计性改善对照。|

### §2.1 In Scope

|ID / 规格声明|代码实证|判定|理由|
|---|---|---|---|
|I01 §2.1 L69：“企业代码规范目录与版本”|STD:12–27；HTTP:237–265；STORE:301–302|一致|Standard 模型有 ID/version/owner，服务支持 standards 目录和版本身份。|
|I02 §2.1 L70：“Scenario 分解”|STD:27；ORCH:112–118|部分一致|Scenario 模型与建议任务类型存在；未核实自动或人工分解到规范版本的完整工作流。|
|I03 §2.1 L71：“Coverage Analysis”|COV:13–59,62–72；LEDGER:15–46；HTTP:329–330|部分一致|本地分析及账本已实现；名为 analyze 的 HTTP 入口只创建资源，不调用分析器。|
|I04 §2.1 L72：“Rule Intent 建模”|INT:13–67；CMD:44–49|一致|独立模型及输入校验；不与 implementation 混用。|
|I05 §2.1 L73：“Typed Rule IR”|IR:12–19；COMP:61–94|一致|封闭七类 IR，编译器只接受已支持子集，不静默转换未知类型。|
|I06 §2.1 L74：“Template / Native Rule Authoring”|COMP:63–76,89–99；IR:19|部分一致|三种模板可构建；NativeRule 仅找到类型声明，所读 Candidate 入口明确不支持它。见 F05。|
|I07 §2.1 L75：“Backend Capability Routing”|HOSTCLIENT:137–185；HOST:150–156；COMP:63–76|部分一致|执行能力协商/拒绝缺失能力存在；Candidate 生产仍固定 PMD 模板，未证明端到端多 Backend 自动路由。|
|I08 §2.1 L76：“Candidate Rule 构建”|CMD:44–56；COMP:35–47,51–103|一致|校验、模板编译、摘要与制品写出是真实实现，不只是接口。|
|I09 §2.1 L77：“Verification Lab”|LOCAL:39–100；WORKER:255–280|部分一致|本地 fixture 和独立 Worker 套件存在；Worker 证据绑定 backend/resolver/runner，不能直接证明每条 Rule 的独立晋级验证与人工评审完成。|
|I10 §2.1 L78：“Evidence Bundle”|EVID:14–26,50–65；WORKER:267–280|一致|本地 evidence 明确 LOCAL_UNATTESTED；Worker 另行生成签名证据，无把本地包直接称为 release 的实现。|
|I11 §2.1 L79：“Human Review”|THIN:115；HTTP:564–572；STORE:318–335；REG:122–140|未实现|在规则/release 晋级路径未找到独立人工评审记录、审批结论或 reviewer 资格校验；actor/reason 不是评审机制。其他反馈 reviewer 字串/人工 fallback 不算规则评审实现。见 F04 与搜索边界。|
|I12 §2.1 L80：“Rule Registry”|HTTP:237–265；REG:91–140|部分一致|资源注册与签名 registry 都有，但状态语义分裂；资源 registry 明确 authority UNKNOWN，不能当成 Verified 注册中心。|
|I13 §2.1 L81：“Signed Rule Release”|RELEASE:148–180；REG:103–115|一致|制品与意图、锁、evidence 等摘要绑定，admit 调用签名/度量校验；本项不代表独立 rule verification 已闭环。|
|I14 §2.1 L82：“Policy Bundle”|GOV:24–30,48–75,82–107；BUNDLE:33–62,131–156|部分一致|签名 PolicyProfile 与签名 bundle 组合存在；显式空 blocking 集合在 Gate 的语义有问题（F03）。|
|I15 §2.1 L83：“CI/CLI Scan Runtime”|CLI:1370–1381,1570–1584；HOST:189–190|一致|CLI 消费真实执行身份、报告与 Gate，失败/不完整有非零出口；部署级 CI 权威未实测。|
|I16 §2.1 L84：“Finding Normalization”|GATE:570–574|一致|normalize 失败计入 observation_errors，成功 finding 排序去重，不隐藏为成功。|
|I17 §2.1 L85：“Completeness”|GATE:558–576；PLAN:304–352|一致|计划账户、缺失、失败、unsupported 与 observation_errors 有显式计算。|
|I18 §2.1 L86：“Baseline”|GATE:30,60–78,577–580|一致|有分类/签名验证及 fingerprint 债务归属。|
|I19 §2.1 L87：“Exception”|GATE:100–155,577–585；GOV:24–30|部分一致|签名 grant、范围/有效期/审批字段及门禁豁免存在；Server REQUESTED→ACTIVE 仍仅是资源状态，无独立审批执法。|
|I20 §2.1 L88：“PASS / FAIL / INCONCLUSIVE Gate”|GATE:13,588–590；STATUS:35–41|一致|真实三态计算与下游状态映射。|
|I21 §2.1 L89：“Feedback、升级、撤回、回滚和退役”|FEEDBACK:188–229；REG:122–192|部分一致|反馈、激活、撤回、回滚存在；rollback 将原活动版本设为 Revoked，未见独立退役语义，不能把注释 retires 当独立退役实现。|
|I22 §2.1 L90：“运营指标与审计”|METRICS:168–189,413–446；STORE:329–335|一致|运行/反馈/治理 metrics 和 journal 存在；与 D05–D10 所要求的实测提升是不同证据。|

### §2.2 Explicit Non-goals（共同限定：“V1 不承诺”，L94）

|ID / 规格声明|代码实证|判定|理由|
|---|---|---|---|
|N01 §2.2 L96：“任意自然语言规范自动生成可靠规则”|ORCH:19–26；COMP:63–76|一致|只产建议与有限模板；不把非目标当成应补齐功能。|
|N02 §2.2 L97：“任意语言、任意分析引擎的一键转换”|IR:7–19；COMP:63–76；HOST:150–156|一致|显式能力范围和 unsupported 拒绝，不承诺任意转换。|
|N03 §2.2 L98：“自研完整 AST / CFG / Dataflow Engine”|LOCAL:60–79；COMP:99；HOST:189–190|一致|被查执行链复用 PMD/适配器；自有 IR/规范化不等于完整分析器。|
|N04 §2.2 L99：“用 LLM 在 CI 中直接判断代码是否违规”|GATE:558–590；STATUS:35–41|一致|被查 CI/Gate 链使用计算结果与签名状态；未找到运行路径 LLM 裁决调用。|
|N05 §2.2 L100：“自动修复后直接合并代码”|MCP:127–144；STATUS:1–6,35–41；运行域 auto-merge/API 搜索无命中|一致|所查客户端只调用 CLI，状态发布模型不是合并动作；未见自动修复直合并链。|
|N06 §2.2 L101：“首期建设大型微服务平台”|HTTP:34,51–55,96–104；HOST:47–55|一致|看到模块化服务入口与独立执行 host，没有需要大型微服务平台的强制执行链；未审计外部部署。|
|N07 §2.2 L102：“首期覆盖公司所有规范”|COV:13–59；LEDGER:15–22|一致|按单个 Standard/version 与固定分母记账，不将公司全部规范作为启动前提。|
|N08 §2.2 L103：“把所有规则直接升级为 blocking”|GOV:82–107；GATE:583–586；CLI:1426|不一致|显式空 blocking 集合被 Gate 当成全部 blocking，而非不阻断；CLI 未指定 blocking rule 同样走默认全阻断预览。没有证明生产调用 GOV.gate_policy，但公共 Gate 行为已与选择性 blocking 边界冲突。见 F03；不声称规则成熟度本身被修改。|
|N09 §2.2 L104：“证明业务逻辑正确性”|LOCAL:83–100；EVID:14–24；GATE:575–592|一致|结果限定 fixture、finding 与完整性，没有业务正确性证明输出。|

### 用户指定补充边界（不冒称 §0–2 的原文）

|ID / 规格声明|代码实证|判定|理由|
|---|---|---|---|
|B01 “不持有权威 / 不复制 Runtime/Server/Registry/SCM/Gate 权威”；来源 THIN:8,32,151–156，ADR-0141/0149|INVOKER:21–35；MCP:127–144；HTTP:247–250；AUTH:61–77|一致|所查 VS Code/MCP 调 CLI；registry presence=UNKNOWN；authority 在 Runtime 验证。IntelliJ 全部 action 未逐一追踪，结论只覆盖已读路径。|
|B02 “INCONCLUSIVE 不得当成功”；来源 §0.1 L35–36 + 三态消费要求|GATE:588–590；CLI:1574–1584；STATUS:35–41；VSC:555–570；MCP:133–144|一致|CLI 3、SCM ActionRequired、UI 单独显示不确定；MCP 按 CLI 非零标 error。CLI FAIL 可返回 0 是 scan 完成语义，不可把它当 PASS，VS Code 显式防止这一误读。|
|B03 “证据等级不得虚报”；来源 ADR-0150:16–32|EVID:8–26；`scripts/feature-evidence-check.sh`:82–108,123；`scripts/thin-client-check.sh`:145–181|一致|结构检查报 STATIC_ONLY；行为测试失败保持 BLOCKED；本地证据 LOCAL_UNATTESTED。仅核查这两个脚本及本地 EvidenceBundle，不声称全 scripts 都无虚报。|

## 本域问题清单（按严重度）

### F01 — 高：Server release 晋级不要求独立 Verification（P08；关联 S01/S03/I09/I12）

- 入口：`POST releases/{id}/promote` 经 HTTP:340–343、564–572 调 STORE.transition。
- STORE:633–655 定义 release 初始 CANDIDATE，允许直接 ACTIVE；318–335 除身份、上下文、状态与幂等外无 evidence/review 检查，即写 journal 与状态。
- 相反，REG:103–115 验证 signed release 后才存 Verified，129–130 要求 Verified 才 Active。两个路径并非同一守卫。
- 反证与限界：HTTP:249–250 明确 registry presence 不代表 verification/approval/publication；REG 仍校验签名，因此**没有证据证明可通过 Server ACTIVE 绕过生产 Runtime 信任**。已确认的是 Server 晋级端点不兑现“必须独立 Verification”的统一状态含义。若这是纯目录状态，应显式改名/标记，不能与权威晋级混称。

### F03 — 高：显式空 blocking 集合被解释为“所有规则阻断”（N08；关联 I14）

- GOV:89–103 允许非空 rule_ids 配空 blocking_rule_ids，106–107 原样生成 GatePolicy。
- GATE:586 使用 `p.blocking_rule_ids.is_empty() || contains(rule_id)`；因此完整扫描中任何新增、未豁免 finding 都令 GATE:589 为 Fail。
- CLI:1426 只在 set 非空时建立 policy，未传选择器也会触发 Gate 的无 policy 默认 true。
- 限界：`rg gate_policy\(` 在 runtime 只找到定义，未找到生产调用；这不是已复现线上误阻断，也没有修改规则成熟度。风险位于共享 Gate API 的实际语义，未来接入签名 policy 时仍会出现。

### F02 — 中：Coverage 先行原则没有进入生产入口（P10；关联 P01/I03）

- CandidateCmd 仅接收 Intent 与输出目录；COMP.build 仅验证 Intent 后选模板，调用方可以不提交任何 Coverage Analysis。
- factory/server/CLI 源码搜索 `CoverageAnalyzer|CoverageLedger` 无命中；HTTP:329–337 是两个独立 create 入口，不是 coverage 决策完成后才能 generate。
- 现有分析器不是缺失；缺失的是可约束生产的前置结果/决策连接。单纯再加一个 coverage API 不能修复。

### F04 — 中：规则 Human Review 尚未形成可执法实现（I11；关联 P08）

- 查询 `HumanReview|human.review|reviewer|reviewDecision|review|approval|approver`：命中局部注释、feedback 测试身份、fast-decision 人工 fallback 与 exception approvals 字段，不是规则发布审批。
- HTTP.transition、STORE.transition、REG.promote 只收 actor/reason/序列及资产身份，不消费人工审核记录。
- 在本域晋级链标“未实现”，不推断外部组织没有线下审批；若允许线下审批，仍需绑定可验证记录才能支持声明。

### F05 — 中：Native Rule Authoring 的范围尚未接到构建入口（I06）

- IR:19 声明 NativeRule，但 COMP:63–76 仅接受 method-call/existing-rule/structural-node，89–94 仅处理三类 IR。
- 在 factory 主源码与 CLI 搜索 NativeRule/native 未找到相应生产路径。Backend 有 native 能力不等于 ERF 已实现其 authoring/review/build 生命周期。

### F06 — 低：生命周期与运营目标的证据仍不足（I21、D05–D10）

- REG.rollback 把旧 Active 设置为 Revoked（184–185），没有独立 retirement 操作；不宜用“retires”注释证明“退役”全语义。
- METRICS 有运行可靠性、误报、回归、治理统计；本域没有 Coverage/Qualified Coverage/Authoring Efficiency 的前后改善证据。目标尚不能标“已达成”，但这不等于相关产品能力全部缺失。

## 整改建议

1. 优先澄清并统一 release 晋级状态语义：权威晋级只消费已验证、绑定候选与独立审核证据的记录；若 Server 只是目录登记，明确非权威状态，拒绝把其 ACTIVE 映射为已验证发布。新增未提供 evidence/审核记录不得晋级的负例。
2. 将 Gate 中“未指定 policy”与“显式空 blocking 集合”分开；后者不能隐式扩大阻断范围。增加空集、非空子集、无 policy 三个边界测试，并在实际 policy 接入时重复验证。
3. 在现有 Candidate 入口绑定 Standard/version/Scenario 对应的 Coverage 决策，区分复用、配置修复与必须新建；不要复制分析逻辑。
4. Human Review 和 Native authoring 若不属于当前交付范围，在实现状态中明确未完成；若纳入，沿现有资产身份与 journal 补齐，不另造平行治理系统。
5. 独立标注撤回/回滚/退役语义及运营目标证据等级；没有实测前后对照时不报告“指标提升”。

## 证据边界与可复核性

- 这是只读源代码审计，证据等级 **STATIC_ONLY**。没有运行 `bash scripts/check.sh`、Cargo/Maven、外部 CI 或真实 SCM；这些会生成构建/验收产物，不属于本次只读委派。报告不存在“全量门禁通过”的结论。
- 无语义索引工具可用，采用 bounded `rg`/`sed`。搜索在 control-plane/runtime/clients/backend-hosts/contracts/scripts/docs/adr，通常排除 target/build/node_modules。`runtime/rf-cli/src/main.rs` 含 NUL，发现普通 rg 判 binary 后已用 `rg -a` 重查关键路径。
- 负向搜索仅支持“在注明范围内未找到”；特别是 Human Review、Native authoring、外部 LLM/自动合并，不是对整个生态或外部服务的不存在证明。
- 核心可复查查询：`rg -an 'CoverageAnalyzer|CoverageLedger' control-plane/factory control-plane/server runtime/rf-cli --glob '**/src/**' --glob '!**/target/**'`；`rg -an 'gate_policy\(' runtime --glob '*.rs' --glob '!**/target/**'`；`rg -an 'HumanReview|human.review|reviewer|reviewDecision|NATIVE_RULE|NativeRule' control-plane runtime contracts --glob '!**/target/**' --glob '!**/build/**' --glob '!**/node_modules/**'`。
- 运行域 LLM/自动合并查询：`rg -n 'LLM|openai|anthropic|chat.completions|auto.?merge|git merge|pulls/.*/merge' clients runtime backend-hosts --glob '**/src/**' --glob '*.js' --glob '!**/target/**' --glob '!**/node_modules/**'`，本次无命中；不把关键词缺席独立当成证明，另结合所读 Gate/CLI/Status 调用链。
- 已主动反证：没有把 HTTP registry 注册当权威；没有把 LocalVerification 当独立生产验证；没有把 CLI scan 的 exit=0 当 Gate PASS；没有把新增 Backend 的未来目标当当前缺陷；没有将企业问题陈述当可由代码证明的事实。
- 三条“不一致”是 P08/P10/N08；同一根因的关联“部分一致”不重复计为独立缺陷。其余差距按部分一致、未实现、无法验证分别列出。
