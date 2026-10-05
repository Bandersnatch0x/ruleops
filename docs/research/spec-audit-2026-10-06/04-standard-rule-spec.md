# 域 4/9：标准与规则规格审计

## 域范围

- 标的：`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md` §7–§13，行 492–844；§14（846 行）不纳入。
- 唯一代码证据根：`/home/kali/文档/ruleops`，WSL `kali-linux2`；HEAD `7e7d2820de5f2948bd936b64f5907ae211f00645`。Windows Orca 工作区不是本报告证据源。
- 本报告是只读、限时规格审计：未改源码、docs 或 scripts，只写 `.scratch/spec-audit/04-*`。按协调者收敛要求停止继续深读，未闭环项明确标注。
- 共 **52 条声明**：一致 18；部分一致 21；不一致 6；未实现 1；无法验证 6；**不一致 + 未实现 = 7**。部分一致不合并成已实现，也不合并成完全未实现。
- 最重要结论：RuleIntent/IR、Java Backend、Conformance 与 Authoring 基础设施不是空白；最高风险是 **Runtime 在生成 Finding 前没有重验 blob/字节边界，以及 capability=false 仍可能通过 required-capability 握手**。
- 契约位置结论：`contracts/` 有 Backend/Workspace Protobuf 和 HTTP/YAML 交付契约；限定文件盘点未找到独立 Standard/RuleIntent/RuleIR schema。对应封闭资产契约实际由 `control-plane/authoring` 的 Java records、严格 Parser 与 Validator 承担。§9/§10 本身没有规定必须放在 `contracts/`，故不把文件路径布局单独算作硬性违约。

## 逐声明核验表

说明：原文位置指上面的单一规格文档；每条最多两个关键代码证据。`一致` 仅指表中明确核验的范围，不表示生产部署验收通过；`未实现` 指限定检索未找到所需接线，`无法验证` 指证据不足或端到端未闭环。

| ID | 规格原文位置与声明 | 代码实证 | 判定 | 理由 |
| --- | --- | --- | --- | --- |
| S01 | §7:494-516：Standard 包含版本、身份、owner、语言及 Scenario expected。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/model/Standard.java:12-27`<br>`control-plane/authoring/src/main/java/io/ruleops/authoring/AuthoringValidator.java:127-157` | **一致** | 模型覆盖示例字段；校验版本、owner、Scenario 非空/唯一及 expected 枚举。 |
| S02 | §7:519-520：Scenario 是 Coverage 最小业务单元，不能只记整条规范覆盖。 | `control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageAnalyzer.java:18-57`<br>`control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageLedger.java:33-38` | **一致** | 以 Scenario ID 建立分母，拒绝缺失、重复或未知分类；输出逐 Scenario 条目。 |
| C01 | §8:526-537：覆盖分析使用列出的八种分类。 | `control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageCategory.java:8-16` | **一致** | 枚举逐项对应 EXISTING_RULE 至 NOT_MACHINE_DETECTABLE，并映射 coverage state。 |
| C02 | §8:526：创建新规则前必须先分类。 | `control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:50-69`<br>`control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageAnalyzer.java:18-57` | **不一致** | 分析器存在，但 CandidateCompiler.build 只验证 intent 随即选模板，不要求/读取 Coverage Ledger 或分类结果；已核验的候选创建入口可绕过此前置要求。 |
| C03 | §8:539-555：Coverage Ledger 是可持续运营资产而非临时报表。 | `control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageLedger.java:15-46` | **部分一致** | 有 Standard 版本、确定性 YAML 和 SHA-256 digest；持久化仓储、运营更新生命周期未在本次边界内闭环核验，不能仅凭 asYaml 认定完成。 |
| I01 | §9:561-610：RuleIntent 具有 Standard/Scenario、语言、scope、target、capabilities、unresolved、examples、severity 契约。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/model/RuleIntent.java:13-67`<br>`control-plane/authoring/src/main/java/io/ruleops/authoring/AuthoringParser.java:21-23` | **一致** | 对应 Java record 与封闭 YAML 绑定，含所有示例字段；额外 holdout/nearMiss 不与示例冲突。contracts/ 未找到独立 RuleIntent schema，当前模型契约位于 authoring 模块，不应误报为完全无实现。 |
| I02 | §9:565-567：RuleIntent 的 metadata.id/version 是规则身份。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/AuthoringValidator.java:162-170`<br>`control-plane/authoring/src/main/java/io/ruleops/authoring/model/RuleIntent.java:13-19` | **部分一致** | 字段已建模，但 validateIntent 仅在 metadata 非 null 时验证身份；缺失 metadata 没有对应 required 错误，后续 compile 使用 metadata 时可能暴露空值。 |
| I03 | §9:569-577：RuleIntent 精确关联 Standard 版本和 Scenario。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/AuthoringValidator.java:94-123` | **一致** | 资产集合校验路径验证引用存在、版本一致、Scenario 存在及语言一致；此结论不表示所有单资产 compile 路径都会执行跨资产校验。 |
| I04 | §9:612：RuleIntent 禁止嵌入任意可执行脚本。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/AuthoringParser.java:21-23`<br>`control-plane/authoring/src/main/java/io/ruleops/authoring/model/RuleIntent.java:21-67` | **一致** | 字段集无脚本入口，未知字段被拒绝；示例片段作为数据而非执行接口。 |
| R01 | §10:618-629：IR 是有限、类型化、可验证的表达，枚举七种类型而非万能 DSL。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/model/RuleIrKind.java:12-19`<br>`control-plane/authoring/src/main/java/io/ruleops/authoring/AuthoringValidator.java:261-285` | **一致** | 七种 kind 封闭枚举且 IR 有字段级校验；枚举并不意味着七类模板都已经实现。contracts/ 同样未找到独立 Rule IR schema。 |
| R02 | §10.1:633-648：给定 SymbolRule YAML 示例可表达 owner/name/parameters、scope 和 onUnresolved。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/model/RuleIr.java:9-19`<br>`control-plane/authoring/src/main/java/io/ruleops/authoring/AuthoringValidator.java:261-264` | **不一致** | 字段语义对应，但实际校验强制 apiVersion，而原文示例没有该字段；代码注释明确承认这一偏差，属示例/契约文档漂移。 |
| R03 | §10.1:650-651：复杂跨过程 dataflow 不强塞通用 IR，由专用 Backend 或 NativeRule 表达。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/model/RuleIr.java:13-26`<br>`control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:61-69` | **部分一致** | 当前有限 IR/模板没有通用跨过程 DSL；专用 dataflow 或 NativeRule 创作链路未在本次证据中确认。原文是边界要求，不等于要求当前实现 dataflow 引擎。 |
| R04 | §10.1:653-656：按 20 个候选和 Coverage Gap 选最多三类模板，默认 Existing/Structural/Symbol；SpotBugs 不推动 IR 扩张。 | `scripts/phase0-template-selection-check.sh:10-28`<br>`control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:61-94` | **一致** | 检查脚本约束二十候选与三类模板；编译器只实现这三类并明确拒绝其他 target/kind。此行只核验模板收敛，不把 SpotBugs Native 完整闭环算作已验证。 |
| B01 | §11:662-670：内部 SPI 提供 describe/validate/compile/verify/plan/execute/observe 七操作。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/BackendAdapter.java:10-20`<br>`backend-hosts/java/src/main/java/io/ruleops/javahost/JavaBackendHost.java:142-190` | **部分一致** | 实际 Adapter 接口只有身份、capabilityFacts、ruleKinds、observe；describe/plan/execute 部分由 Host 承担，validate/compile/verify 不在该 SPI。功能分散实现不等同规格列出的统一接口。 |
| B02 | §11:672-674：SPI 为 Host 内部接口，Runtime 消费自有跨进程消息。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/BackendAdapter.java:10-20`<br>`runtime/host-client/src/lib.rs:171-207` | **一致** | 包内 Java 接口与 Rust protobuf client 分离；本结论限定于已读 Host/Client 主路径，不推断所有部署均无其他集成。 |
| B03 | §11:676-691：describe 能表达 Java 版本与 AST/symbol/type 等能力事实。 | `contracts/proto/backend_host.proto:54-66`<br>`backend-hosts/java/src/main/java/io/ruleops/javahost/PmdBackendAdapter.java:65-72` | **部分一致** | 已返回 backend 身份、language、facts、ruleKinds；schema 只有语言 ID，未表达 8/11/17/21 source-level 范围；PMD 只宣告 AST、symbol 和 SymbolRule，未宣告 type 能力。缺失能力按契约为不存在，不能把引擎理论能力当已集成能力。 |
| B04 | §11:694：Router 可以返回 UNSUPPORTED，不得静默降级。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/JavaBackendHost.java:160-173`<br>`runtime/scan-plan/src/lib.rs:317-352` | **部分一致** | Runtime accounting 支持 unsupported 并保留 incomplete 原因，但 Java Host.plan 无条件把所有输入列为 planned，没有按 language/capability 拒绝的分支；完整 Router 选择链未继续核验。 |
| H01 | §11.1:698-703：stdin/stdout 使用 length-delimited Protobuf 请求/响应帧。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/Framing.java:22-50`<br>`contracts/proto/backend_host.proto:83-99` | **一致** | Java 采用长度前缀读写字节帧；HostEnvelope 使用 typed payload 和 correlation。 |
| H02 | §11.1:703：stderr 为结构化运维日志。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/JavaBackendHost.java:109-113`<br>`backend-hosts/java/src/main/java/io/ruleops/javahost/JavaBackendHost.java:298-302` | **不一致** | 存在 JSON error 日志，但 cancel-outside-window 输出普通 [host] 文本；stderr 格式未统一。该差异不等于 stdout 被污染。 |
| H03 | §11.1:706-707：非协议、未知必需、超限或损坏帧显式失败并计入 Completeness，禁止猜测解析。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/Framing.java:29-40`<br>`runtime/host-client/src/lib.rs:119-123` | **部分一致** | 畸形 protobuf/超限长度有显式错误；Java 读长度遇 EOF 直接返回 null，未区分干净 EOF 与部分头。全部错误到 Completeness 的端到端映射未完成核验。 |
| H04 | §11.1:709-714：契约涵盖 correlation、describe/plan/execute/cancel、observation、structured error、completion。 | `contracts/proto/backend_host.proto:83-99`<br>`backend-hosts/java/src/main/java/io/ruleops/javahost/JavaBackendHost.java:176-223` | **一致** | 消息/Host 执行路径具有 correlation、逐 unit outcome、observation 和 execute-complete；资源/时限约束另见 H07-H08。 |
| H05 | §11.1:716-717：Host 不监听网络，不以 JNI/FFI/N-API 嵌入 Rust。 | 未找到足够实现证据／未闭环（见理由） | **无法验证** | 已看到独立 Host 与 IPC 结构，但本次未逐一核验启动器、所有监听调用和部署入口；不以架构注释替代全路径否定性证明。 |
| H06 | §11.1:719-722：major 不同拒绝；同 major additive evolution；握手交换版本范围。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/JavaBackendHost.java:123-139`<br>`contracts/proto/backend_host.proto:31-51` | **部分一致** | major mismatch 显式拒绝，schema 注释约束 additive evolution；实际仅 major/minor 单版本，没有版本范围字段。历史 field-number 不复用未跨历史版本验证。 |
| H07 | §11.1:721-722：无法满足 required capability 必须显式失败，optional 可忽略。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/JavaBackendHost.java:123-139`<br>`runtime/host-client/src/lib.rs:184-191` | **不一致** | Java hello 没有检查 required_capabilities 即 ack；Runtime 补验只看 facts.contains_key，不检查布尔值 true。声明 capability=false 的 Host 可通过此握手检查，不能把键存在等同有能力。 |
| H08 | §11.1:713,721：握手交换并遵守最大帧/最大事件/资源限制。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/JavaBackendHost.java:123-139`<br>`backend-hosts/java/src/main/java/io/ruleops/javahost/Framing.java:20-25` | **不一致** | Host 把客户端 limits 原样回显为 accepted_limits，但 Framing 上限是构造时 final 值，hello 不更新；已读 execute 也未按协商事件上限计数。Runtime 有自身事件/时限保护，故不能据此断言可突破整个 Runtime 资源边界，但 Host 接受承诺不真实。 |
| H09 | §11.1:712：支持 deadline 与 cooperative cancellation。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/JavaBackendHost.java:188-215`<br>`runtime/host-client/src/lib.rs:308-319` | **部分一致** | Runtime 有 recv timeout，Host 在 unit 之间检查取消；单个耗时 unit 执行中无已证实的协作取消点，Host 内未证实执行协商 deadline。 |
| H10 | §11.1:724-726：治理消息为共享 typed schema，不允许 Any/opaque JSON；新旧版本双向 golden tests。 | `contracts/proto/backend_host.proto:18-19`<br>`runtime/conformance/src/lib.rs:608-608` | **部分一致** | 共享 proto 及 golden 测试函数有对应实现证据；本次未执行 Rust/Java/JavaScript 新旧版本完整双向矩阵，不能由四项 evidence 单元测试代替。禁用 Any 的声明不是单独的机械门禁证据。 |
| H11 | §11.1:728-731：Workspace Resolver 使用独立 schema 与独立 Runtime Client。 | `contracts/proto/workspace_resolution.proto:1-1`<br>`runtime/resolver-client/src/lib.rs:456-459` | **一致** | 存在独立 workspace_resolution.proto、resolver-client；不是复用 Backend describe/plan 伪装解析。与 Host 等价的全部超时/资源行为本行不作运行保证。 |
| L01 | §11.2:735-739：Observation location 为 SOURCE_SPAN/SYMBOL/BYTECODE_MEMBER/FILE 封闭 union。 | `contracts/proto/backend_host.proto:231-263`<br>`runtime/gate/src/lib.rs:624-649` | **一致** | protobuf oneof 与 Runtime 四种分支对应，缺失/未知 location 不能正常归一化。 |
| L02 | §11.2:741-742：Host 将引擎坐标转为仓库相对路径、blob digest、UTF-8 byte range。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/PmdBackendAdapter.java:128-145`<br>`backend-hosts/java/src/main/java/io/ruleops/javahost/SourceSpans.java:26-41` | **部分一致** | PMD 路径限制和 SourceSpans digest/字节行区间存在；精度是整行而非完整 violation span。越界行会降为 EOF 空区间，代码依赖 Runtime 再校验，而实际存在 L04 缺口。 |
| L03 | §11.2:743-744：SYMBOL/BYTECODE_MEMBER 使用规范 owner/name/signature，FILE 仍可验证身份。 | `runtime/gate/src/lib.rs:632-647`<br>`contracts/proto/backend_host.proto:249-263` | **部分一致** | typed identity 存在；Runtime 仅做非空、无控制/空白和 digest 长度检查，未证明完整 canonical signature 语法或 FILE 对应实际 blob 身份。 |
| L04 | §11.2:746-747：Runtime 重验 blob 与范围、推导行列和 Semantic Anchor，异常不能生成 Finding。 | `runtime/gate/src/lib.rs:621-653`<br>`runtime/gate/src/lib.rs:570-571` | **不一致** | normalize 仅检查 digest 为 32 字节、end>=start、Host line/column 非零；不读仓库 blob、不校验范围上界/UTF-8 边界，直接采用 Host 行列并构造 Finding。错误 digest、越界 end、伪造行列只要通过这些表面条件即可流入 Findings；相应错误计数不会触发。 |
| K01 | §11.3:751-754：存在版本化、Backend-neutral、含 fake/golden/failure injection 的可执行 Conformance Suite。 | `runtime/conformance/src/lib.rs:26-64`<br>`runtime/conformance/src/lib.rs:468-468` | **一致** | 有固定 suite version/检查清单和 run_acceptance；fake Host/Resolver 文件已盘点。本次从当前源码运行 cargo test -p erf-conformance --lib --offline，4/4 通过；这只证明证据单元测试可执行，不表示完整 acceptance suite 已通过。 |
| K02 | §11.3:752-754：Suite 至少验证全部列出的协议、资源与 sandbox escape resistance。 | `runtime/conformance/src/lib.rs:32-64` | **部分一致** | 固定清单覆盖 negotiation、frames、cancel、deadline、crash、resolver retry/backpressure、local determinism/completeness；没有直接体现 Host retry/backpressure 和完整 sandbox escape/resource adversarial checks。另有 SandboxProof 不等同执行 escape 测试，完整覆盖无法由现有清单证明。 |
| K03 | §11.3:756-758：Pack 仅凭精确 suite digest 与 ConformanceEvidence 晋级；suite major 变化重认证。 | 未找到足够实现证据／未闭环（见理由） | **无法验证** | 证据类型与 worker 存在，但未闭环追踪 BackendRuntimePack/WorkspaceResolverPack 的所有晋级入口及 major 变更拒绝路径；本次限定搜索未找到足够执行证据，不能称已实现或确定缺失。 |
| K04 | §11.3:758：ExecutionLock 固定 suite identity。 | `runtime/rf-runner/src/main.rs:778-778`<br>`runtime/rf-runner/src/main.rs:984-995` | **部分一致** | 锁结构和解析具有 conformance_suite_digest；未继续核验它与当前已验证 evidence 的所有组合一致性比较。 |
| K05 | §11.3:760-763：Suite release 签名不可变；独立 Worker 在隔离环境运行完整测试并用 verification-signing 签证；Builder 无晋级权。 | `runtime/verification-worker/src/lib.rs:253-280`<br>`runtime/verification-worker/src/lib.rs:320-321` | **部分一致** | worker 验 sandbox proof、调用完整 acceptance、拒绝失败并产出签名证据，有 signing role 标识；suite-release 签名发布入口、真实隔离部署及 Builder 权限分离未运行核验。 |
| K06 | §11.3:765-768：ConformanceEvidence 绑定 Pack/suite/Runner/platform/Sandbox identities，变化必须新证据。 | `runtime/conformance/src/lib.rs:199-211`<br>`runtime/verification-worker/src/lib.rs:267-279` | **部分一致** | 证据中有这些身份字段，当前源码证据测试通过；受影响测试重跑与新聚合签发的全流程未逐入口验证，不能仅凭字段存在判全部一致。 |
| T01 | §12:776-783：Tier1 PMD 承担 Java AST、structural/XPath、native、symbol/type 规则。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/PmdBackendAdapter.java:65-72`<br>`backend-hosts/java/src/main/java/io/ruleops/javahost/PmdBackendAdapter.java:92-124` | **部分一致** | 真实 PMD ruleset/PmdAnalysis 调用已落地，非 fake；对外能力只宣告 AST/symbol 与 SymbolRule，native/type 全链条未在本次范围证明。 |
| T02 | §12:785-791：Tier1 SpotBugs 承担字节码、编译后语义及 PMD 不适合的 detector。 | `backend-hosts/java/src/main/java/io/ruleops/javahost/SpotBugsBackendAdapter.java:37-43`<br>`backend-hosts/java/src/main/java/io/ruleops/javahost/SpotBugsBackendAdapter.java:67-89` | **部分一致** | SpotBugs 4.9.8 Adapter、bytecode capability 与 observe Java/class 输入边界存在，也有独立测试文件；未重跑 detector 实际结果及 Native 验证/发布闭环，因此不认定完整落地。 |
| T03 | §12:793-799：Tier2 列出 ast-grep/Semgrep/Checkstyle/Error Prone/ArchUnit。 | 未找到足够实现证据／未闭环（见理由） | **无法验证** | 限定 backend-hosts 文件盘点仅看到 fake/Java/Node-fake，未找到对应生产 Adapter 证据；这是接入层级规划，不能把缺少每一种工具都列为当前阻断缺陷。 |
| T04 | §12:801-805：Tier3 列出 CodeQL/Infer/企业内部分析器。 | 未找到足够实现证据／未闭环（见理由） | **无法验证** | 本次限定盘点未找到生产集成证据；同属规划，不推断必须在当前版本实现。 |
| T05 | §12:807：工具接入顺序由真实 Coverage Gap 决定，不以工具数量为 KPI。 | `scripts/phase0-template-selection-check.sh:12-28` | **无法验证** | Phase0 有候选和分类检查，但没有核验之后所有接入决策的业务依据，也无法从源码证明组织 KPI。 |
| A01 | §13:813-823：AI 可解释规范/歧义，建议 Scenario/边界/测试及 Finding explanation。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/orchestration/AuthoringOrchestrator.java:112-118`<br>`control-plane/authoring/src/main/java/io/ruleops/authoring/suggestion/AuthoringSuggestion.java:194-200` | **部分一致** | 已有 scenario、positive/negative、near-miss、explanation、related-asset 类型与执行器接缝；这些不等于真实模型提供者和各项生成任务已经接通，歧义识别未找到专门实现证据。允许能力列表也非逐项强制交付清单。 |
| A02 | §13:819-821：AI 可生成 RuleIntent/Rule IR candidate 与 Native Rule scaffold。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/orchestration/AuthoringOrchestrator.java:112-118` | **无法验证** | 已读 task type 没有这三种专门类型；通用 explanation/content 不能证明受治理的生成/解析链路。限定 authoring/server/factory 搜索未得到可核验生成入口，不能把模型类或确定性模板误称 AI 生成。 |
| A03 | §13:825-831：AI 不得自主批准/发布、取得签名权限、改生产 Policy 或作 CI 最终裁决。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/suggestion/AuthoringSuggestion.java:56-58`<br>`control-plane/authoring/src/main/java/io/ruleops/authoring/orchestration/AuthoringOrchestrator.java:20-26` | **部分一致** | 输出 trust flags 强制 false，编排宣告非权威；真实部署角色/签名密钥和生产权限隔离未审计，注释不能替代授权证据。 |
| A04 | §13:833-836：模型与检索输出统一非权威 AuthoringSuggestion，不得直接成为 Finding/EvidenceBundle/RuleRelease/GateDecision。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/suggestion/AuthoringSuggestion.java:38-58`<br>`control-plane/authoring/src/main/java/io/ruleops/authoring/retrieval/AuthoringRetrieval.java:271-285` | **一致** | Suggestion 构造器强制 kind/schema/trust，检索生成同一非权威对象；本行针对已读 authoring/retrieval 输出边界，不覆盖未核验外部集成。 |
| A05 | §13:836-837：人工接受建议必须创建新 Candidate，再走 Verification 与 Review。 | `control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:33-59` | **未实现** | 未找到实现证据：对 control-plane/server/src/main 与 factory/src/main 检索 AuthoringSuggestion、acceptSuggestion、accept.*uggestion 无命中。已有独立 RuleIntent→Candidate 编译器，但未发现 Suggestion 人工接受→新 Candidate 的接线；结论仅限已检索路径。 |
| A06 | §13:839-840：内建 Orchestrator 冻结输入范围与来源集合。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/orchestration/AuthoringOrchestrator.java:67-88` | **一致** | Request 对 tasks/permittedSourceRefs 防御复制，范围不能为空；输入范围可冻结，不直接赋权。 |
| A07 | §13:839-840：Orchestrator 确定性任务分组与失败回退。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/orchestration/AuthoringOrchestrator.java:222-266` | **一致** | 实现按 taskId 排序、primary/group 与 fallback 分离，并记录 fallbackCount；有对应 AuthoringOrchestratorTest 文件，本次未重跑 Java 测试。 |
| A08 | §13:839-840：Orchestrator 互斥终态及来源追溯。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/orchestration/AuthoringOrchestrator.java:43-58`<br>`control-plane/authoring/src/main/java/io/ruleops/authoring/orchestration/AuthoringOrchestrator.java:396-408` | **部分一致** | 封闭 terminal states 和 permittedSourceRefs 检查有证据；本次未重跑失败/超时组合来证明所有路径恰好一个终态。 |
| A09 | §13:840-842：检索先提供 ID lookup、metadata filter、FTS/BM25，不提前引入向量。 | `control-plane/authoring/src/main/java/io/ruleops/authoring/retrieval/AuthoringRetrieval.java:240-267`<br>`control-plane/authoring/src/main/java/io/ruleops/authoring/retrieval/AuthoringRetrieval.java:311-313` | **一致** | 当前实现是 exactAuthorityId→可见性过滤、词项 BM25、确定性排序；该类未走语义向量检索。检索语料与离线质量收益评估不在此次执行验证中。 |

## 问题清单（按严重度）

### P1：直接影响可信执行结论

1. **L04 — 不可信 Observation 可绕过仓库内容/坐标真实性校验。** `normalize` 没有仓库 blob 输入，只检查 digest 字节数和区间相对顺序，沿用 Host 行列便构造 Finding。非零但伪造的行列、任意 32 字节 digest、超出文件长度的 end 均没有对应核验步骤；也会影响 anchor/fingerprint 可信度。`SourceSpans` 对越界行的 EOF 回退明确依赖一个当前不存在的 Runtime 复验，放大这一缺口。此为静态数据流结论，未声称运行了恶意 Host 利用测试。
2. **H07 — required capability 把键存在当作能力满足。** Java Host 不拒绝 required 缺口，Rust Client 用 `contains_key` 接受 false。即使后续某些 Router 另有保护，握手这一独立契约已不成立；本报告不将未读后续 Router 假定为补救保证。

### P2：流程/契约完整性缺口

3. **H08/H09 — Host 协商承诺与执行不一致。** limits 原样回显，却没有配置到固定 framing 上限；单 unit 内无已确认 cooperative cancellation。Runtime 自身的 deadline/event ceiling 是保护层，不等于 Host 已遵守 accepted_limits。
4. **C02 — 创建候选规则可以不带 Coverage 分类。** 八分类和 Ledger 已实现，编译入口却没有前置依赖，无法保证“先查重/判 Gap 再创建”。
5. **K02/K03/K05/K06 — 不应将证据单测或 SandboxProof 当完整 Pack 认证闭环。** 测试清单缺少明确的完整 sandbox escape/resource 检验说明；Pack 晋级、重认证、独立部署签名权限尚未在本次闭环确认，属于验收阻断证据缺口，不声称这些功能必然不存在。
6. **A05 — 人工接受 Suggestion→新 Candidate→Verification/Review 未找到接线。** 当前具备 untrusted suggestion 与独立 candidate compiler，但未找到二者之间的人工接受入口。
7. **I02 — RuleIntent metadata 缺失校验不完整。** 有身份字段但 null metadata 跳过校验，降低 schema 边界可信性。

### P3：兼容/能力声明与文档漂移

8. **R02 — §10.1 示例缺少实际必需的 apiVersion。** 示例按文复制不能通过当前 IR validator；代码已有 deviation 注释，应修正权威示例或明示示例为片段。
9. **B01/B03/T01 — SPI 形状与可宣告能力窄于规格。** 七操作分散而非统一 SPI；source-level range/type 能力无完整对外表达。不要因 PMD 本身支持某项能力就宣称 ERF 集成层已完成。
10. **H02 — stderr 混用结构化与普通文本日志。** 影响统一日志消费，但未观察到 stdout 污染证据。

Tier2/Tier3 是接入规划，不能因本次没找到 Adapter 就按当前强制功能缺失处罚；AI “可以”列表同样区别于 V0.1 明文“必须”的编排最小能力。

## 整改建议

1. **优先修 Runtime 的唯一 Observation 边界。** 让归一化接收冻结仓库 blob/解析结果，校验实际 digest、UTF-8 范围及字段语义，从 byte range 推导展示坐标；错误产生结构化 observation error，禁止构造 Finding。补一组错误 digest、过界范围、无效 UTF-8 边界、伪造行列的可执行回归测试；同时去掉依赖未实现复验的静默 EOF 降级。
2. **把 required-capability 判定集中为布尔真值检查。** Host 遇缺失/false 必须返回 UNSUPPORTED_CAPABILITY，Client 同样拒绝 false；Conformance 同时测试“键缺失”和“键存在但 false”。将 accepted limits 与实际 framing/events/deadline 控制绑定，而不是只回显。
3. **补缺失流程的最短接线，不另建平行体系。** 复用已有 CoverageLedger/分类给 candidate 创建提供可验证前置输入；人工接受 Suggestion 时复用 CandidateCompiler 创建新候选，并保留来源与后续 Verification/Review 身份。
4. **认证优先补行为证据。** 在现有固定 Suite 中补全声明要求的 host/resource/sandbox tests，证明 Pack 晋级入口确实验证精确 evidence 与 ExecutionLock identities；部署签名/Builder 权限另行验收，不以单元测试绿灯替代。
5. **低风险文档/契约收敛。** 修 IR 示例 apiVersion、RuleIntent metadata required、stderr 格式；对尚未支持的能力/source levels 明确声明未支持。是否抽出 `contracts/` 独立 authoring schema 应按真实跨语言消费者需求决定，不为目录形式新增重复契约。

## 证据边界

### 已执行的验证

- 规格按要求使用 `sed -n '492,846p'` 精确读取，并对关键子段增加行号；没有以旧摘要代替当前文档。
- 先限定盘点 `contracts/`、`backend-hosts/`，再按准确名称定位 `control-plane/authoring|coverage|factory` 与 `runtime/host-client|resolver-client|gate|conformance|verification-worker|rf-runner`；无语义索引工具可用，因此使用有界 `rg`/精确读取。缺失结论仅覆盖已记录的这些路径。
- `timeout 60s /home/kali/.cargo/bin/cargo test -p erf-conformance --lib --offline`：从当前源码编译并执行，**4 passed / 0 failed**；日志 `.scratch/spec-audit/04-conformance-test.log`。四项是 suite digest 与 Evidence 身份/失败/撤销相关单元测试，不是整个 `run_acceptance`。
- 初次用 PATH 中 `cargo` 执行失败（未加入 PATH），随后明确定位 `/home/kali/.cargo/bin/cargo` 后重跑成功；最终日志记录的是成功重跑，不伪称初次成功。
- Java PMD/SpotBugs/Authoring 存在测试源码及本地历史 surefire 报告，但本次没有重跑，未把历史报告当当前绿灯。

### 未验证／不作保证

- 没有运行完整 `bash scripts/check.sh`、完整 Host/Resolver Conformance acceptance、Java 全量测试、跨语言新旧版本 golden matrix、生产 sandbox escape 或签名服务部署验证。此次交付为只读审计报告而非“实现可发布”的门禁结论。
- 没有审查所有 Pack 晋级调用者、所有授权入口、模型提供者运行环境、历史 schema field-number 演进与所有 Resolver deadline/resource 分支；相应行标部分一致或无法验证。
- 未进行独立反证代理复核；协调者要求收敛后停止继续深读。P1/P2 结论基于已读实际函数，未跑专门 exploit/故障注入复现。
- 多 worker 并行期间使用当前 checkout 的行号；报告标注 HEAD，但不保证其他审计域或外部进程在全程绝无变化。未修改已有跟踪文件。
- `未实现` 的 A05 使用的有限搜索：`AuthoringSuggestion|AuthoringOrchestrator|acceptSuggestion|accept.*uggestion|RULE_INTENT|NATIVE_RULE`，范围 `control-plane/server/src/main` 与 `control-plane/factory/src/main`；无匹配不证明仓库其他未读目录或外部系统不存在实现。
