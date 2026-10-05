# 02 — 运行模型与生命周期规格核验

## 域范围

- 标的：`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md`，§3（L108–155，含 §3.1 L143–154）与 §4（L158–234）；§5 从 L238 开始，不在本域。
- 树：WSL `kali-linux2`，`/home/kali/文档/ruleops`；审计时 HEAD：`7e7d2820de5f2948bd936b64f5907ae211f00645`，2026-10-05。开始时 `git status --short` 为空。
- 精确读取命令：`nl -ba docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md | sed -n '108,242p'`；额外四行仅用于确认 §5 边界。
- 重要纠偏：§3.1 的八项是**人员角色**（Standard Owner 等）；Factory Worker、Scan Runner、Gate、Registry 不是该表里的角色。以下先核验八项原文人员职责，再单列组件职责映射，不把题面组件名称伪造为规格原文。
- 只读静态审计：不改源码、文档或脚本；仅生成本报告。每项保留 1–2 处关键证据。协调者要求立即收敛后停止源码深读，未完成调用链的项明确降级。
- 判定口径：“一致”仅表示已读实现与该条具体声明相符；“部分一致”表示部分阶段可定位但连接或完整职责未证实；“未实现”在本报告仅表示已检索范围内未找到实现证据；“无法验证”不等于缺失。组织角色描述不自动解释成规格要求某种 RBAC 技术实现。

- 结果：共 **34** 条；**一致 8**，**部分一致 21**，**不一致 1**，**未实现 1**，**无法验证 3**。

## 逐声明核验表

所有代码路径均相对上述 WSL 仓库根；规格行号均指同一份标的文件。代码证据为当前源码，不将脚本内固定 PASS 文案当作本次测试结果。

| 编号 / 规格声明 | 代码证据（file:line） | 判定 | 理由 |
| --- | --- | --- | --- |
| **OM01** — §3 L112–140：Central RuleOps Governance + Distributed Enforcement | control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:223-250；runtime/rf-runner/src/main.rs:1032-1089 | **部分一致** | 存在中心资产查询及 Runner 扫描入口；未验证一个中心对多个真实仓库的生产部署及控制面回流。 |
| **OM02** — §3 L125–133：Policy Center 经 Signed Rule Bundle 向各 Repo 分发 | runtime/rf-runner/src/main.rs:1061-1089；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:91-93 | **部分一致** | 交付扫描要求 signed Execution identity，存在只读交付接口；本次未闭合 Policy Center→签名 Bundle→交付目录的发布链，不能用签名 ExecutionLock 代替 Bundle 签名证明。 |
| **OM03** — §3 L135–140：Findings / Coverage → Feedback / Exception → RuleOps Center | control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageAnalyzer.java:48-59；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:356-362 | **部分一致** | Coverage Ledger 和反馈/例外请求验证入口存在，但未证明这些结果自动关联回中心并改变规范或规则版本。 |
| **R01** — §3.1 L147：Standard Owner 定义规范、适用范围、业务目标 | control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageAnalyzer.java:13-29 | **部分一致** | 代码消费带身份和 Scenario 的 Standard；未核实业务目标/适用范围的全字段模型及 Standard Owner 身份授权。人员职责不等同于要求存在同名类。 |
| **R02** — §3.1 L148：Rule Author 将规范转为 Scenario、Intent、Examples | control-plane/authoring/src/main/java/io/ruleops/authoring/orchestration/AuthoringOrchestrator.java:92-118；control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:51-69 | **部分一致** | 有 Scenario/样例类 authoring task 和 Intent 校验编译；未验证 Standard→人工确认的完整 Author 工作流。 |
| **R03** — §3.1 L149：Rule Expert 处理语义、Backend、边界和 Native Rule | control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:63-76,88-100 | **部分一致** | 语义模板按 target kind 分流并产出 PMD 资产；读到的入口仅支持三类模板，未找到本域可引用的 Native Rule 专家处理路径。不能据此断言仓库完全没有 Native Rule。 |
| **R04** — §3.1 L150：Independent Reviewer 独立审核 Evidence 与规则晋级 | runtime/release-registry/src/lib.rs:122-140；control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:691-701 | **无法验证** | 晋级接收 actor/reason，但它们是文本审计上下文，不证明 reviewer 与 author 独立；外围人工流程和身份系统未核实。关联 B01、L08。 |
| **R05** — §3.1 L151：Platform Operator 管理 Registry、Runner、签名、兼容矩阵、CI | runtime/release-registry/src/lib.rs:91-119；scripts/phase2-java-local-demo.sh:57-68 | **部分一致** | Registry 验签准入及兼容矩阵/执行验证脚本有技术承载；操作者授权、密钥运维与真实 CI 配置不在本次已验证证据内。 |
| **R06** — §3.1 L152：Repo Owner / Tech Lead 项目接入、债务和 Exception 治理 | runtime/rf-runner/src/main.rs:1032-1058；runtime/gate/src/lib.rs:577-586 | **部分一致** | 有项目 scope 校验、baseline 历史债务识别及精确指纹 exception 使用；未证明谁有权接入项目、批准例外或维护 baseline。 |
| **R07** — §3.1 L153：Developer 消费 Finding、修复、反馈误报 | runtime/gate/src/lib.rs:568-592；control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:161-168 | **部分一致** | 本地 GatePreview 返回 findings，但中心 GET findings 在已读分支明确返回空 items；开发者可以消费本地结果，不能将此中心接口视为已完成结果汇聚。修复执行本身是人的职责。 |
| **R08** — §3.1 L154：Security/Compliance 管理高风险规则、例外、审计策略 | 未找到本域可确认的 Security/Compliance 身份到高风险审批权限的代码映射；已查 control-plane/server、runtime/release-registry、runtime/governance-registry 的相关锚点 | **无法验证** | 存在签名角色或审计字段不等于实现组织职责与职责分离；不把缺少同名角色枚举单独定为安全漏洞。 |
| **C01** — §3 L119–122、§4 L175–189：Rule Factory / Factory Worker 承担候选构建 | control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:51-115；scripts/phase2-java-local-demo.sh:30-42 | **部分一致** | 可从 Intent 构建候选并交给验证脚本；没有核实服务端 FactoryJob 到独立 Worker 的排队、领取、执行、回写路径。Factory Worker 是补充组件映射，并非 §3.1 人员角色。 |
| **C02** — §3 L121、§4 L192–195：Verification Lab 承担验证并生成 Evidence | scripts/phase2-java-local-demo.sh:42-49；runtime/verification-worker/src/lib.rs:253-284 | **部分一致** | 脚本验证四类样例并检查 EvidenceBundle，另有 verify_and_sign 的 conformance evidence 入口；未证明二者构成同一候选作业的中心化 Verification Lab。 |
| **C03** — §3 L129–135、§4 L216：Scan Runner 承担仓库侧执行 | runtime/rf-runner/src/main.rs:1032-1089 | **部分一致** | 独立扫描入口校验 project scope、增量计划和交付执行身份；本次未读完后端调用链，故不宣称完整扫描执行已动态验收，也未证实 Runner 越权做候选编译。 |
| **C04** — §4 L219–230：Gate 消费扫描证据做决策，而非重新编译规则 | runtime/gate/src/lib.rs:548-592；runtime/authority/src/lib.rs:61-78 | **一致** | 已读 Gate 函数只消费计划、状态、观测、baseline、exception 和 policy；权威评估另检查 Trusted 与平台。这里的结论限于这两个入口，不等同于已证明所有信任字段不可伪造。 |
| **C05** — §3 L119、§4 L207–210：Registry 承担已签名发布的登记与状态管理 | runtime/release-registry/src/lib.rs:91-140 | **一致** | admit 调用 verifier 验证签名发布和字节后以 manifest digest 登记；promote 单独变更状态，不覆写发布字节。此结论不包含人工审核前置条件，见 B01。 |
| **L01** — §4 L161–164：Enterprise Standard → Standards Catalog | control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:234-250 | **部分一致** | 存在 registry/standards 查询，并明确登记仅证明存在、authority 为 UNKNOWN；未核实规范导入、Owner 签审和完整目录维护路径。 |
| **L02** — §4 L164–170：Standards Catalog → Scenario Decomposition → Coverage Analysis | control-plane/authoring/src/main/java/io/ruleops/authoring/orchestration/AuthoringOrchestrator.java:92-118；control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageAnalyzer.java:18-29 | **部分一致** | 有 Scenario authoring task 和拒绝空 Scenario 的分析入口；Catalog 与经人确认的分解结果之间未闭合。不把人工输入误判为必须自动化。 |
| **L03** — §4 L170–174：Coverage Analysis 区分 Existing / New Rule / Not Detectable→Manual Control | control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageAnalyzer.java:33-59,62-72 | **部分一致** | 逐 Scenario 分类、完整分母与 rule binding 得到检查；未深读分类枚举，也未找到本次可证明的人工控制执行/确认回流。 |
| **L04** — §4 L172–174：Existing → Configure | control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:63-68,78-85 | **一致** | existing-rule 路由至 ExistingRuleTemplate，并为 EXISTING_RULE 接受参数键值形式；存在配置既有规则的具体编译入口。未对模板内部全部语义做验收。 |
| **L05** — §4 L175–181：Rule Authoring → RuleIntent → Rule IR | control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:51-76 | **一致** | 入口校验 RuleIntent，然后编译出 RuleIr；无效 Intent 和不支持 target 有显式错误路径。 |
| **L06** — §4 L184–189：Capability Routing → Backend Compilation → Candidate | control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:63-69,88-115 | **部分一致** | 模板路由、PMD emitter 与 Candidate 组装存在；此入口固定 PMD，不能证明完整 Backend-neutral capability routing。没有把首期 PMD 优先本身判为不一致。 |
| **L07** — §4 L189–195：Candidate → Verification Lab → Evidence Bundle | scripts/phase2-java-local-demo.sh:30-49 | **一致** | 脚本将编译出的候选目录传入 candidate-verify.sh，再读取 evidence-bundle.json，要求 bad/good/holdout/near-miss 且全部 pass；证明存在可调用路径，不代表本次已执行通过。 |
| **L08** — §4 L195–201：Evidence Bundle → Human Review → SHADOW | 未找到实现证据：针对 reviewer/approver/independent/SHADOW/VERIFIED/RELEASED/HumanReview 的定向查询未定位人工审核决定的消费入口；runtime/release-registry/src/lib.rs:122-140 仅校验状态和审计文本 | **未实现** | 限本次已检索发布/服务入口，没有把审核意见与候选或 Evidence digest 绑定并作为 SHADOW 入口的实现证据；不是对线下审核活动不存在的断言。 |
| **L09** — §4 L201–204：SHADOW → Real Repo Evidence | scripts/phase2-java-local-demo.sh:67-68,74-91；scripts/phase4-shadow-pilot.sh:66,129-151 | **部分一致** | 存在 representative Java shadow 调用；另一 pilot 明确 SYNTHETIC_SHADOW，且声明不能进入 LIVE_SHADOW 产品聚合。未运行真实仓库观测，也未核实晋级时消费真实观测的路径。 |
| **L10** — §4 L204–210：Real Repo Evidence → Signed Release → Rule Registry | runtime/release-registry/src/lib.rs:91-119；scripts/phase4-release-registry.sh:15-32 | **部分一致** | Registry 有签名发布准入，脚本只运行 registry 单元测试，并明确内存 core、生产签名密钥管理不在其范围；未核实真实仓库 Evidence 到生产签名服务的上游链。 |
| **L11** — §4 L210–216：Rule Registry → Policy Bundle → Distributed Scan | 未找到本次可确认的 Registry active release 到 Policy Bundle 编制、签名再交付 Runner 的连通证据；已定位 runtime/release-registry、runtime/governance-registry、runtime/rf-runner | **无法验证** | 相关 crate 和接口存在不足以证明连接成立；停止深读后不以名称推定端到端完成。 |
| **L12** — §4 L216–219：Distributed Scan → Unified Findings | runtime/rf-runner/src/main.rs:1032-1089；runtime/gate/src/lib.rs:568-574 | **部分一致** | Runner 有扫描入口，Gate 将 BackendObservation 归一化、排序去重为 findings；尚未闭合 Runner 返回观测到这个 Gate 入口的调用链，也未证明多仓库中心汇聚。 |
| **L13** — §4 L219–222：Unified Findings → Completeness | runtime/gate/src/lib.rs:558-576 | **一致** | 以计划单元为分母，记录 accounted/failed/missing 与 observation_errors；不完整时后续判定 INCONCLUSIVE，未把零 Finding 当作充分通过证据。 |
| **L14** — §4 L222–225：Completeness → Baseline Comparison | runtime/gate/src/lib.rs:575-586 | **一致** | 计算完整性后逐 Finding 按精确 fingerprint 区分 Historical/New，并独立评估 exception；完整性和债务状态没有合并成同一字段。 |
| **L15** — §4 L225–230：Baseline Comparison → Policy Gate → PASS / FAIL / INCONCLUSIVE | runtime/gate/src/lib.rs:583-592 | **一致** | 不完整优先 INCONCLUSIVE；完整且有未豁免的新 blocking finding 为 FAIL；其余 PASS。这里只验证该决策函数的代码条件。 |
| **L16** — §4 L230–233：Fix / Exception / Feedback Loop | control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:356-362；runtime/gate/src/lib.rs:577-586 | **部分一致** | 请求入口与 gate exception 消费存在；未核实 Request→安全审批→签名 Grant→下一次 Scan 的闭环，也未证明反馈驱动规则修订。 |
| **L17** — §4 L161–233：整体 Authoring→Verify→Release→Scan→Gate 顺序可实现 | scripts/phase2-java-local-demo.sh:30-68；scripts/phase4-release-registry.sh:15-32 | **部分一致** | 已确认 Authoring→Candidate→Verify→Evidence 的脚本路径和独立 Registry/Gate 核心；不是同一份候选身份穿过人工审核、真实 SHADOW、签名发布、Policy、扫描再 Gate 的端到端证据。 |
| **B01** — §3.1 L150 + §4 L195–210：规则晋级应保留独立审核、SHADOW、真实仓库 Evidence 这些前序关口 | runtime/release-registry/src/lib.rs:102-118,122-140；control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:318-340,652-665 | **不一致** | 已读 Registry 允许验签准入后的 Verified 直接 Active，promote 只检查 actor/reason、操作序号、当前状态及 active 冲突；中心 Store 同样允许 CANDIDATE→ACTIVE。这些入口自身不消费审核决定或 SHADOW Evidence；因此状态机层并未表达图示关口。是否被生产外围强制补齐、是否形成实际越权，仍无法验证。 |

## 问题清单（按严重度）

### 高 — F02-01：发布晋级的前序证据没有进入已读状态机（B01、R04、L08–L10）

- 规格链：Evidence → Human Review → SHADOW → Real Repo Evidence → Signed Release → Registry；Independent Reviewer 负责审核与晋级。
- 正向代码事实：Rust Registry 验证 SignedRuleRelease 后登记为 `Verified`，其 `promote` 可直接变为 `Active`；Java Store 允许 `CANDIDATE → ACTIVE`，`requireContext` 只要求非空且格式合法的 actor/reason。
- 差异：已读晋级函数未接收或校验绑定 Candidate/Evidence 的人工审核决定与真实 SHADOW 证据。这里的 `Verified` 是此 Registry 的准入状态，不能直接等同于规则成熟度已通过人审。
- 后果（条件性）：若这些入口就是生产发布关口，则仅满足签名及本地状态条件的资产能够晋级，规格要求的治理顺序可能被绕过。**未证明生产签名服务/外围调用者不存在额外强制校验，因此不宣称已确认远程越权或发布漏洞。**
- 关联重复项只计一个根因，不把 R04、L08 与 B01 计算成三个独立漏洞。

### 中 — F02-02：可执行片段存在，但不能证成端到端生命周期（L11、L12、L17）

- `phase2-java-local-demo.sh` 连通 compile→verify→Evidence 与本地 SHADOW 调用；`phase4-release-registry.sh` 主要是独立 Registry 单元测试。
- 已读证据不能证明同一 Candidate/Release digest 穿过真实审核、签名、Registry、Policy Bundle、Scan、Gate；尤其 Registry→Policy Bundle 尚未核实。
- 风险是把“多个模块分别有测试”写成“生产端到端已闭环”，而非已确认某一个模块不能工作。

### 中 — F02-03：中心 Findings 回流与治理闭环未完成证明（OM03、R07、L16）

- 中心 HTTP `GET /v1/findings` 已读分支直接构造空 `items`，本地 Gate 则有 Finding 集合；故不能用该接口证明真实结果已汇聚。
- 反馈/例外请求入口与 Gate 消费 exception 是两端存在，不足以证明请求审批、签名授权及下一轮执行之间的连接。
- 限制：未审计所有客户端，本问题不否认本地文件、IDE 或其他通道可以消费 Finding。

### 中 — F02-04：人员职责的独立性无法通过已读身份事实验证（R01–R08）

- 找到了技术资产与审计 actor 字段，未找到可确认的 Standard Owner、Rule Author、Independent Reviewer、Security/Compliance 的身份绑定及独立性证据。
- 这是一项未证实的治理保证，不是仅凭“没有角色枚举”认定的安全漏洞；也不要求八个人员角色各自对应独立服务。

### 低 — F02-05：SHADOW 演示与真实仓库观察不能混计（L09）

- `phase4-shadow-pilot.sh` 明确为 `SYNTHETIC_SHADOW`，并声明不得进入 `LIVE_SHADOW` 产品聚合；这是正确边界，不是该脚本造假。
- 本域没有动态复跑真实仓库观察，不能把演示成功或脚本文案升级为真实发布成熟度证据。

## 整改建议

1. **先核实唯一发布关口，再补约束，而不是到处加审批判断。**确认生产调用者是否已经强制绑定候选摘要、审核人身份与独立性、审核结论、真实 SHADOW Evidence；若存在，将该权威边界与其输入合同写清并补拒绝用例。若不存在，在唯一晋级/签名入口实施这些约束；Registry 局部状态不能冒充完整规则成熟度。
2. **补一条最小垂直验收。**让同一 candidate/release digest 经人工决定、真实 SHADOW 证据、Signed Release、Registry、Policy Bundle、Scan 进入 Gate，保留各步身份关联；负例至少包括缺审核、审核针对旧候选、缺真实观察与扫描不完整。不要将数个独立脚本 PASS 拼接成这个验收。
3. **明确中心回流的交付边界。**若 Findings 接口仍是壳，文档/调用方应明确未实现真实查询，或接入已有结果存储；反馈与例外的接收不应被展示成已经批准或生效。
4. **给人员职责附可验证证据。**优先复用已有身份系统及审计机制，记录“谁提交、谁审核、谁签名、谁批准例外”的边界；未查明现有控制前，不新造八套角色服务或通用权限框架。
5. **保持本域发现与后续审计分工。**密钥、签名合同、Factory Worker 作业编排、CI 权威及完整 Backend 路由由对应域继续核验；本报告不推断它们全部缺失。

## 证据边界

### 已检索范围与缺失结论的依据

- 规格：仅 §3–§4；没有将 §5 以后新增要求冒充本域原文。
- 定向路径：`control-plane/authoring`、`control-plane/factory`、`control-plane/coverage`、`control-plane/server`、`runtime/authority`、`runtime/gate`、`runtime/rf-runner`、`runtime/release`、`runtime/release-registry`、`runtime/governance-registry`、`runtime/verification-worker`，以及报告列出的 Phase 2 / Phase 4 脚本。
- 精确锚点包括：`FactoryWorker|Factory Worker|ScanRunner|Scan Runner|enum Role|role_id|reviewer|approver|permission|authorize`；`reviewer|approver|independent|SHADOW|VERIFIED|RELEASED|HumanReview`；发布/作业/候选相关 `release|publish|candidate|verification|transition`。
- 结果：得到签名角色、actor/reason 文本、局部晋级状态机、SHADOW 演示和扫描入口；没有得到可引用的独立审核决定消费链。这里的未找到是**限定路径和搜索锚点的结论**，不是全仓、外部服务或线下流程不存在的证明。
- 当前宿主没有可调用的 zvec/fast-context/context-mode 检索工具，按指令退回有界 `rg` 和 `sed -n`。未创建索引。环境中未提供技能所述 Workflow/slash-command 自动审计工具，本次采用人工有界核验，没有独立反证代理，因此发现保留上述置信边界。
- 第一次按默认发行版 `kali-linux` 查目标目录失败；随后确认目标实际位于 `kali-linux2`，所有有效证据和报告均来自后者。早期不存在的 `crates`、`control-plane/lifecycle` 猜测路径没有被用作缺失证明。

### 未执行与未声称

- 没有运行 `bash scripts/check.sh`、Maven/Cargo 测试、HTTP 请求、生产签名或真实仓库扫描；原因是只读审计、全量构建会写入产物且协调者要求立即收敛。本报告不是全绿代码交付，脚本可调用与实际通过严格区分。
- 没有证明实际部署鉴权、反向代理授权、密钥托管、外部人工审批、角色独立性或 live SHADOW 观测事实。
- 没有完整追踪每条调用链及每个 caller；特别是 B01 的外围保护和 L11 的 Registry→Policy Bundle 连接仍待对应域复核。
- 只生成 `.scratch/spec-audit/02-operating-model-lifecycle.md`；未改源码/docs/scripts，未发布 GitHub/Slack 消息。报告完成验收仅包含结构、34 条声明统计、引用文件与行号范围和工作树变化检查。
