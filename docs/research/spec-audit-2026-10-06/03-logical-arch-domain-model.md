# 域 03：逻辑架构与领域模型——规格／代码符合度审计

## 域范围

- 标的：`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md`，§5（238–328 行）、§6（330–490 行）；§7 从 492 行起，不纳入。
- 证据树：WSL `kali-linux2:/home/kali/文档/ruleops`；HEAD `7e7d2820de5f2948bd936b64f5907ae211f00645`。没有使用 D 盘工作树作为代码证据，没有修改 D 树或源码、docs、scripts。
- 日期：2026-10-05（Asia/Shanghai）。这是受 30 分钟上限约束的静态、限界审计，不是全仓安全认证或上线验收。
- 规格精读命令：`sed -n '238,353p' docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md` 与 `sed -n '354,493p' docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md`；引用使用原文件行号。
- **范围纠正**：§6.1 的 Identity Separation 指 Standard、Scenario、Rule Semantic、Implementation、Release、Execution 六类资产身份，不是 author／verifier／gate 三种人员或工作负载身份。后者按任务关注点补充核验，但不能冒充 §6.1 的明文要求。
- 判定口径：**一致**＝所引代码直接支持该声明，范围限于所检查路径；**部分一致**＝部分机制存在但未闭合；**不一致**＝现有实现与明确约定相反；**未实现**＝在指定实现位置与限界搜索中未找到实现证据，不声称穷尽整个仓库；**无法验证**＝依赖部署、审批、阶段选择等本次不可见事实。
- 统计：共 **80** 条规格声明；一致 **15**、部分一致 **32**、不一致 **4**、未实现 **23**、无法验证 **6**。补充角色核验不计入上述规格声明总数。数值“不一致”不把“未实现／部分一致”混算，也不等于独立问题数量。

**结论**：四平面已有逻辑模块及部分可信执行链路，但本体仍是明确标注 v0 的小型 Turtle 编译原型，不能据此声称已满足 §6 的 V1 本体供应链契约。最严重问题为缺失 RDFC-1.0、完整本体签名 envelope 和签名 deterministic Protobuf OntologyContractPack；其次是 Semantic Projection 发布阻断与跨资产本体身份闭合不足。

## 逐声明核验表

所有规格引用均指上述唯一标的；代码路径均相对于 WSL 证据树。每条最多列出两个关键代码位置。

### §5：逻辑架构与四平面

| 声明（编号、规格原文／行号） | 代码实证 | 判定 | 理由 |
|---|---|---|---|
| A01 · §5:240–264、§5.1–5.4：Control／Factory／Scan／Gate 逻辑分工 | `pom.xml:16-21`；`runtime/trusted-ci/src/lib.rs:114-174` | 部分一致 | Java 有 authoring／coverage／factory／verification／ontology／server 模块；Rust 的 Runner 与 Gate 有独立调用边界。未证明 Factory 独立隔离执行部署；规格是逻辑平面，不要求四个微服务。 |
| A02 · §5.1:272–282：Standards、Authoring、Candidate、Verification、Review、Release、Registry、Policy、Exception、Feedback、Audit | `pom.xml:16-21`；`control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:21-42` | 部分一致 | 大部分职责有模块或资源类型对应；资源枚举及 API seam 不能证明 Review、Registry 和审批执行闭环。 |
| A03 · §5.1:284–285：PoC 优先 Git＋CLI＋CI＋Artifact Repository，不要求先做微服务 | `scripts/candidate-verify.sh:14-30`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:12-27` | 部分一致 | 已有可直接运行的 CLI／脚本，不依赖微服务；Artifact Repository 和 CI 实际部署未验证。 |
| A04 · §5.1:287–290：Pilot Git 为权威源；CLI/API 生成可审查 Git 变更，不与 Server DB 双写 | `control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:327-353`；`control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:217-268` | 不一致 | API 允许 Standard／Policy／Release 等资源直接 CREATE，Store 直接追加 journal 并写 resources，不产生 Git 变更。代码自称 V0.2 API seam，若非 Pilot 权威入口可缩小影响，但它本身不实现指定的 Git 写入语义；未声称观察到生产双写。 |
| A05 · §5.1:288–289：二进制／大型 Evidence 在 Artifact Repository，Git 仅存 digest/reference | 未找到可证明真实部署存储布局的证据；已检查本地 Candidate／Ontology CLI 输出边界 | 无法验证 | 本地输出不等于受治理 Artifact Repository；本次不访问部署或远端仓库。 |
| A06 · §5.2:294–296：Candidate plugin/rule 构建不可信、无生产凭证 | `control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:34-47`；`scripts/candidate-verify.sh:10-30` | 无法验证 | 当前工厂是同进程模板编译，本地验证继承宿主环境；未找到受隔离 Candidate 构建入口，也未检查实际凭证，不能断言泄露。 |
| A07 · §5.2:297：Factory 默认无网络 | `scripts/candidate-verify.sh:24-30`；`runtime/sandbox/src/lib.rs:73-77` | 部分一致 | 通用扫描 sandbox 生成 network=none；Candidate 验证脚本直接 Maven／Java，未接入此 sandbox。不能把 Scan 隔离当作 Factory 隔离证据。 |
| A08 · §5.2:298：Factory CPU／Memory／Time 限制 | `runtime/sandbox/src/lib.rs:60-77`；`scripts/candidate-verify.sh:24-30` | 部分一致 | Sandbox 存在 CPU／内存参数；所查 Candidate 执行路径没有对应限额和 wall-time 门禁。未验证外层 CI 限时。 |
| A09 · §5.2:299：Factory 隔离文件系统 | `control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:34-47`；`runtime/sandbox/src/lib.rs:73-77` | 部分一致 | 通用 sandbox 只读 root／mount 可用；CandidateCompiler 直接在宿主 outputRoot 写文件，没有 Factory 专用隔离边界证据。 |
| A10 · §5.2:300：固定工具链 | `pom.xml:29-34`；`scripts/candidate-verify.sh:10-14` | 部分一致 | PMD、Jena、编译插件版本已固定；脚本仅检测 PATH 上存在 java，并可加载本地环境脚本，不等于固定完整构建工具链身份。 |
| A11 · §5.2:301：Dependency allowlist | 未找到实现证据；限界检查 `control-plane/factory/pom.xml`、`pom.xml`、`scripts/candidate-verify.sh` 的 allowlist 与依赖执行边界 | 未实现 | 声明依赖／固定版本不等于对不可信 Candidate 依赖实施允许集。 |
| A12 · §5.2:302：输出 Artifact＋Build Evidence | `control-plane/factory/src/main/java/io/ruleops/factory/candidate/Candidate.java:15-33`；`control-plane/verification/src/main/java/io/ruleops/verification/EvidenceBundle.java:14-24` | 部分一致 | 有 Artifact、manifest、fixture 和本地验证 Evidence；没有该工厂构建隔离／工具链证明与输出绑定的证据。 |
| A13 · §5.3:306–308：企业代码授权域内运行，默认不上传源代码 | `runtime/sandbox/src/lib.rs:73-77` | 部分一致 | Docker 路径本地只读挂载仓库且无网络；授权域、宿主权限及非 sandbox 路径没有部署证据，不能把这一分支推广为全局保证。 |
| A14 · §5.3:309：只消费签名 Rule Bundle | `runtime/rule-bundle/src/lib.rs:188-204`；`runtime/rf-runner/src/main.rs:1082-1117` | 部分一致 | delivery 分支有签名 bundle 验证；Runner 仍允许未走 delivery 的本地 bundle 路径。后者可属于 LOCAL_UNATTESTED 预览，不等于已证明权威 Gate 绕过，但“只消费”并非通用 scan API 的约束。 |
| A15 · §5.3:310：固定 Engine／Rule／Policy identity | `runtime/rf-runner/src/main.rs:769-785`；`runtime/trusted-context/src/lib.rs:183-228` | 一致 | ExecutionLock 固定后端及运行制品；可信 Context 对 policy、backend/version、bundle/lock 逐项比较。限于 trusted 路径。 |
| A16 · §5.3:311：输出 Findings＋Completeness | `runtime/trusted-ci/src/lib.rs:165-174`；`runtime/gate/src/lib.rs:558-592` | 一致 | 传入已提交 findings 和各执行单元状态，生成标准 Findings 与 Completeness。 |
| A17 · §5.3:312：不允许模型参与最终判断 | `runtime/gate/src/lib.rs:577-592` | 一致 | 已读最终判定为 completeness、baseline、exception、policy 的确定性运算，没有模型调用。仅对该最终判定链作结论。 |
| A18 · §5.4:316：Gate 与 Scanner 解耦 | `runtime/trusted-ci/src/lib.rs:138-174` | 一致 | scan 先返回 ScanResult，再 handoff 给 rf_gate；Gate 逻辑不是 Backend Host 的最终裁决。 |
| A19 · §5.4:318：Gate 验证 source identity | `runtime/trusted-context/src/lib.rs:183-190`；`runtime/trusted-context/src/lib.rs:230-243` | 部分一致 | 校验 repository/commit 与 workspace attestation digest／工作负载上下文；本次未逐层复算所有源码身份。 |
| A20 · §5.4:319：Gate 验证 head/base identity | `runtime/rf-runner/src/main.rs:149-159`；`runtime/trusted-context/src/lib.rs:183-190` | 部分一致 | 增量请求有 base/head，Gate Context 明确校验 commit；该边界没有独立 base 字段校验。base 的下游证明未追完，不宣称全系统缺失。 |
| A21 · §5.4:320：rule bundle digest | `runtime/trusted-context/src/lib.rs:222-228` | 一致 | GateAttestation 的 bundle digest 必须等于 execution lock 的值。 |
| A22 · §5.4:321：execution lock | `runtime/trusted-ci/src/lib.rs:126-135`；`runtime/trusted-context/src/lib.rs:199-228` | 一致 | 校验 scan/context 使用同一签名 lock，并比较实际 runner／backend／resolver measurements。 |
| A23 · §5.4:322：policy digest | `runtime/trusted-context/src/lib.rs:189-190` | 一致 | Gate policy digest 必须精确等于 verifier 的 expected policy digest。此条不等于完整 Policy 语义校验。 |
| A24 · §5.4:323：baseline identity | `runtime/trusted-ci/src/lib.rs:197-200`；`runtime/trusted-context/src/lib.rs:230-235` | 一致 | 基于 baseline canonical payload 求 digest，Gate 与输入 digest 比较；未把 baseline 历史审批真实性计入此条。 |
| A25 · §5.4:324：completeness | `runtime/gate/src/lib.rs:558-576`；`runtime/gate/src/lib.rs:588-590` | 一致 | missing／failed／observation errors 参与完整性，非完整结果为 Inconclusive。 |
| A26 · §5.4:325：exceptions | `runtime/gate/src/lib.rs:120-153`；`runtime/trusted-ci/src/lib.rs:165-174` | 部分一致 | ExceptionGrant 有签名、scope、approvals、时效校验；handoff 接收已归约的 fingerprint 集，本次未证明所有生产调用均先验证该集合来自合法 Grant。 |
| A27 · §5.4:326：finding delta | `runtime/gate/src/lib.rs:577-592` | 一致 | 按 baseline 区分 Historical／New，再结合 exception／blocking policy 决策。 |

### §6.0–6.4：模型、身份、本体权威与语义投影

| 声明 | 代码实证 | 判定 | 理由 |
|---|---|---|---|
| D01 · §6:334–351：Standard→Scenario→Intent→Implementation→Evaluation/Release→Scan→Finding/Completeness/GateDecision | `control-plane/authoring/src/main/java/io/ruleops/authoring/model/RuleIntent.java:21-33`；`runtime/release/src/lib.rs:18-30` | 部分一致 | Standard/Scenario 关系与 Intent、artifact、Evidence 的 release digest 关系存在；没有逐项证明全链基数与完整参照完整性。 |
| D02 · §6.1:358–359：区分 Standard 与 Scenario identity | `control-plane/authoring/src/main/java/io/ruleops/authoring/model/Standard.java:18-27`；`control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageAnalyzer.java:22-45` | 一致 | Standard metadata ID 与 Scenario ID 分开；Coverage 拒绝重复/未知 Scenario。 |
| D03 · §6.1:360–361：Rule Semantic 与 Implementation identity 分离 | `control-plane/factory/src/main/java/io/ruleops/factory/candidate/Candidate.java:15-23`；`runtime/release/src/lib.rs:18-29` | 一致 | intent identity/digest 与 Candidate/Artifact digest、backend identity 分开表达。 |
| D04 · §6.1:362–363：Release 与 Execution identity 分离 | `runtime/release/src/lib.rs:18-30`；`runtime/rf-runner/src/main.rs:769-785` | 一致 | Release 定义冻结制品输入，ExecutionLock 则定义 runner/backend/resolver/bundle 环境；不是复用单一对象身份。 |
| D05 · §6.1:365：同一 Intent 可有多个 Backend 实现 | `runtime/release/src/lib.rs:18-24`；`control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:88-99` | 部分一致 | Release 模型可组合 intent digest 与不同 backend；当前工厂实际发射为 PMD，未验证同一 Intent 的多 Backend 发布共存。 |
| D06 · §6.2:369–372：TBox 定义 Standard/Scenario/Intent/Capability/Backend/Evidence/Policy/Finding 等概念关系 | `specs/ontology/core.ttl:1-8`；`specs/ontology/java-extension.ttl:1-6` | 部分一致 | core 只含 Standard、RuleIntent、Finding 和两条关系；Java extension 加两类，尚非规格所列完整概念模型。 |
| D07 · §6.2:373–374：ABox 稳定 Asset IRI，每次语义变更为不可变 Ontology Revision | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPack.java:8-13`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:18-26` | 未实现 | 仅调用方传入 revision 字符串及内容 digest；未找到 ABox 资产 IRI 分配或不可变 version IRI 注册机制。 |
| D08 · §6.2:376–377：本体查询/受控推理可辅助 Control；推理结果不是 Release/Evidence | `control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:21-29`；`control-plane/verification/src/main/java/io/ruleops/verification/EvidenceBundle.java:7-12` | 部分一致 | Candidate 与本地 Evidence 的非发布/非权威定位明确；没有本体推理输入路径可验证其与 Release 的隔离。查询/辅助推理的“可使用”不是强制已有功能。 |
| D09 · §6.2:379–381：Execution 无开放世界推理；固定 Revision 编译封闭约束/Manifest，验证、digest、签名后入 Bundle | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:28-42`；`runtime/rule-bundle/src/lib.rs:18-28` | 部分一致 | 有封闭 IRI 检查与 pack digest 占位引用；没有所要求的约束编译、独立 pack 签名和运行解析闭环。 |
| D10 · §6.2:381–382：Release、Policy、EvidenceBundle、ScanExecution 记录 ontology revision/digest | `runtime/release/src/lib.rs:18-26`；`runtime/governance-registry/src/lib.rs:24-44` | 部分一致 | Release 有 ontology_digest；PolicyProfile 模型及其签名载荷没有 ontology 字段。其它模型未被其 bundle 顶层 digest 自动补足为各自的签名绑定。 |
| D11 · §6.3:386–392：Core／Language／Backend／Enterprise 模块化 | `specs/ontology/core.ttl:1-8`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:54-59` | 部分一致 | 有 core 和 java-extension，并拒绝 extension 重定义 core namespace；未找到 Backend／Enterprise extension。 |
| D12 · §6.3:389、394–395：中央 Steward／领域所有者及跨边界联合审批 | 未找到实现证据；检查 `specs/ontology`、ontology compiler 与仓库 `.github`（不存在） | 无法验证 | namespace 保护不等于审批；组织权限、PR 保护和实际所有者审批无法由本次本地树证实。 |
| D13 · §6.3:394–396：外部标准稳定 IRI 映射，变更附兼容分类/影响/资产/迁移计划，禁止原地改语义 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:54-73`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPack.java:8-13` | 未实现 | 有 IRI 引用检查，无治理变更记录、兼容字段、迁移或已发布概念不可变校验。 |
| D14 · §6.4:400–402：每个可执行 Scenario 有经批准 Semantic Projection，投影权威但保留 Standard 原文 | `control-plane/authoring/src/main/java/io/ruleops/authoring/model/Standard.java:20-27`；`control-plane/authoring/src/main/java/io/ruleops/authoring/model/RuleIntent.java:21-33` | 未实现 | 原文 description 保留，但 Scenario／Intent 模型没有投影引用、批准状态与 ontology binding；所查路径没有独立 Projection 模型。 |
| D15 · §6.4:404–405：冲突／未解决映射为 STANDARD_AMBIGUOUS，并阻止 RuleRelease | `control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageCategory.java:15`；`control-plane/coverage/src/main/java/io/ruleops/coverage/CoverageAnalyzer.java:54-59` | 部分一致 | 枚举对应 BLOCKED，但分析器从外部 classification 生成 ledger，并不比较原文/Projection/Intent；未找到该状态到 Release 签发的强制拦截。 |
| D16 · §6.4:405–406：仅 Standard Owner 批准可解除阻塞；AI/推理映射永远先为 Candidate | 未找到实现证据；限界搜索 authoring/ontology 主源码中的 SemanticProjection、projection、approval 与 Coverage 状态边界 | 未实现 | 缺失 Projection 治理模型和批准/解除状态机，因此无法实施该专属批准权；不能用一般 Candidate 注释替代。 |

### §6.5：技术栈与限制

| 声明 | 代码实证 | 判定 | 理由 |
|---|---|---|---|
| T01 · §6.5:412：RDF 1.1 图模型与 IRI 身份 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:12-17`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:48-73` | 部分一致 | 使用 Jena Graph、URI、Turtle triple；完整 RDF dataset／命名图边界尚未实现，不能提升为完整权威 dataset 契约。 |
| T02 · §6.5:413：仅 Control Plane 的 OWL 2 RL restricted subset | 未找到实现证据；ontology 主源码无 reasoner、白名单规则执行器或 materialization | 未实现 | EXTERNAL 中列出 OWL namespace 不等于实现或限制 OWL 2 RL 推理。 |
| T03 · §6.5:414：SHACL Core 对冻结 data＋shapes 做发布前闭合验证 | `control-plane/ontology/pom.xml:12-15`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:28-42` | 未实现 | 只有 jena-arq 和 IRI 检查，没有 shapes 输入或 SHACL 验证调用；不能把 CLOSED_WORLD 错误码当成 SHACL。 |
| T04 · §6.5:415：JSON-LD 1.1 用于 API/authoring 交换，不直接签名 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:45-49` | 未实现 | 本体入口强制 RDFLanguages.TURTLE；没有 JSON-LD 交换入口证据。Jena 潜在支持不代表应用已接入。 |
| T05 · §6.5:416：SKOS 按需标签/词表映射 | 未找到 SKOS 实现证据；已读两个 ontology Turtle 文件 | 无法验证 | “按需”是可选需求，本次无法判定有无业务需求，不列为强制缺陷。 |
| T06 · §6.5:420：禁止 SHACL-SPARQL/JS/AF 等可执行扩展 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:61-73` | 未实现 | 未接入 SHACL，也没有对这些特性作拒绝检测。不能因不执行推理就声称权威构建已验证禁用集。 |
| T07 · §6.5:421：禁止递归 Shape | 未找到实现证据；ontology compiler 无 Shape 输入／递归检测 | 未实现 | 没有受检 shape graph，因而没有递归约束实现。 |
| T08 · §6.5:422：禁止权威构建解析未 digest 固定的远程 context/import | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:28-34`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:45-49` | 部分一致 | 只显式接收本地 Path，不实现 JSON-LD context 展开或 import 解析；也没有 digest pin 输入/核验。未发现或声称运行中发生了远程解析。 |
| T09 · §6.5:423：禁止 OWL DL/Full 和未列入允许集的 RL 特性 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:22-26`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:61-73` | 未实现 | OWL namespace 被广泛视作外部已知 namespace；没有 OWL profile/feature 白名单校验。 |
| T10 · §6.5:424：影响执行的推理结果先 materialize 再编译签名 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:28-42` | 未实现 | 当前只读取 asserted triples 并排序，未发现受控推理、物化及其编译签名路径。 |
| T11 · §6.5:425：Runner 与 Gate 不携带 RDF/OWL Reasoner | `runtime/gate/Cargo.toml:6-11`；`runtime/rf-runner/src/main.rs:491-519` | 一致 | Gate 所列直接依赖没有 RDF reasoner，Runner 所查本体处理是 digest 对比。未检查所有传递依赖/最终发行包，本判定限于直接实现。 |

### §6.6：规范化和签名

| 声明 | 代码实证 | 判定 | 理由 |
|---|---|---|---|
| C01 · §6.6:429：所有 context/import 固定到 digest | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:28-34`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:18-26` | 未实现 | 输入/manifest 只有文件路径、revision 和最终 digest，没有各 context/import 的固定清单。 |
| C02 · §6.6:430：完整 asserted RDF dataset 以 RDFC-1.0 生成 canonical N-Quads 后 digest/sign | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:40-42`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:64-73` | 不一致 | 实际对各 Graph 的 N-Triples 字符串自然排序，并拒绝所有 blank node；无命名图／dataset canonicalization、RDFC-1.0 算法或签名。对简单单图可能有等价输出，不能当作 RDFC 合规。 |
| C03 · §6.6:431：禁止直接对 Turtle/JSON-LD 源文本计算语义身份 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:48-73`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:40-42` | 一致 | 确实先解析为三元组再序列化排序、计算 digest，不是原文本 hash；这不补足 C02 的算法错误。 |
| C04 · §6.6:435：envelope 含 canonicalization/hash/signature algorithms | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:20-26` | 未实现 | manifest 仅 identity/revision/digest，没有算法声明或签名 envelope。 |
| C05 · §6.6:436：envelope 含 ontology IRI 与 version IRI | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPack.java:8-13`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:21-26` | 未实现 | 不受约束 revision 字符串不能代替两种 IRI 字段。 |
| C06 · §6.6:437：envelope 含 dataset 边界、graph names | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:45-48`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:21-26` | 未实现 | 实现读取 Graph 而不是带命名图的 Dataset，manifest 没有边界字段。 |
| C07 · §6.6:438：envelope 含 shapes revision 及每个 import/context digest | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:21-26` | 未实现 | 三字段 manifest 无 shapes 或依赖清单。 |
| C08 · §6.6:439：envelope 含 validator/reasoner/compiler versions | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:21-26` | 未实现 | 构建 pom 中版本不等于被签名 envelope 内绑定的工具版本。 |
| C09 · §6.6:440–441：分别绑定 canonical asserted dataset digest 与 compiled runtime artifact digest | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPack.java:8-15`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:20-26` | 部分一致 | 只有 canonicalTriples 的单一 hash；没有区分 asserted dataset 与 compiled runtime artifact，也未签名。 |
| C10 · §6.6:443：命名 IRI 优先，限制 blank node 与图复杂度 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:64-65` | 部分一致 | 直接拒绝全部 blank node，比“限制数量”更严格；无三元组规模/复杂度限额，不是完整资源策略。 |
| C11 · §6.6:443–444：规范化超时/超配额/失败均 fail closed | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:32-38`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:45-49` | 部分一致 | parse、未知 IRI、blank node 会抛结构化错误；没有可观察超时/配额机制，因此不能覆盖资源失败。 |
| C12 · §6.6:444–445：不签全量 closure；执行相关物化结果单独可重现编译/签名 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:18-27` | 未实现 | 没有 reasoner closure 签名的错误路径，但也没有要求的独立物化制品签名；“未做签名”不能作为满足声明的证据。 |

### §6.7：版本和兼容性

| 声明 | 代码实证 | 判定 | 理由 |
|---|---|---|---|
| V01 · §6.7:449–450：稳定 ontology IRI、不可变 version IRI | `specs/ontology/core.ttl:1-8`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPack.java:8-13` | 部分一致 | 有稳定概念 namespace，但没有 ontology 系列对象与 immutable version IRI；revision 字符串未校验不可变性。 |
| V02 · §6.7:451–452：RDFC digest 为供应链身份，SemVer 不作内容身份 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPack.java:13-15`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:40-42` | 部分一致 | identity 使用 SHA-256 而不是版本号，方向正确；被哈希内容不是规定的 RDFC dataset。 |
| V03 · §6.7:454：ADDITIVE/CONSTRAINT_TIGHTENING/BEHAVIORAL/BREAKING 分类 | 未找到实现证据；限界搜索 ontology 主源码、runtime/release、runtime/rule-bundle、contracts 的分类名 | 未实现 | pack、manifest 和发布模型未见该分类契约。 |
| V04 · §6.7:455–456：非 ADDITIVE 必须影响分析、重验资产、新编译制品 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:28-42` | 未实现 | compile 不接收前一 revision、影响资产或重验记录，未发现变更类别驱动的重验控制。 |
| V05 · §6.7:456：旧 version IRI/dataset/signature/compiled artifact 可审计访问 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:16-26` | 无法验证 | CLI 对固定输出文件名写入，无保留历史逻辑；外部 Artifact Repository 的留存策略未验证，不能据此断言历史已丢失。 |
| V06 · §6.7:458–459：Release、Resolved Policy Manifest、Bundle 的 OntologyContractPack 精确 digest 匹配 | `runtime/rule-bundle/src/lib.rs:175-185`；`runtime/governance-registry/src/lib.rs:24-44` | 部分一致 | Bundle 对输入 ontology digest 有精确比较；PolicyProfile 本身没有 ontology 绑定，未找到 Resolved Policy Manifest 与 Release/Bundle 同包身份的完整校验。 |
| V07 · §6.7:459–462：V0.2 可用兼容证据但仅 ADDITIVE；其余必须新 Release | 未找到 OntologyCompatibilityEvidence 实现证据；限界搜索 ontology/release/rule-bundle/contracts | 无法验证 | 兼容证据是明确可选将来机制；当前未发现它，也未发现据此绕过重验的路径，不列作既有绕过漏洞。非 ADDITIVE 重验缺口见 V04。 |

### §6.8：实现与运行契约

| 声明 | 代码实证 | 判定 | 理由 |
|---|---|---|---|
| I01 · §6.8:466–468：以 Jena 为主，ERF RL 白名单有界 materialization＋固定用例 | `control-plane/ontology/pom.xml:12-15`；`control-plane/ontology/src/test/java/io/ruleops/ontology/OntologyContractPackCompilerTest.java:17-45` | 部分一致 | 已用 Jena，现有三个测试针对重复构建/未声明 IRI/core 重定义；没有 RL 白名单、有界物化或对应推理用例。测试只阅读，未运行。 |
| I02 · §6.8:470–472：Git revision 为权威，Jena/TDB 仅可重建 read model，索引记录 source revision/dataset digest | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:18-26` | 未实现 | 文件输入及 revision 字符串存在；没有 Jena Dataset/TDB read model、索引身份或 Git revision 验证。不存在 read model 不是违规写入证据，而是该契约未落地。 |
| I03 · §6.8:474–476：独立 OntologyCanonicalizer；只接受冻结 dataset/config、输出 bytes/digest/结构化失败；领域接口不泄露库类型 | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPackCompiler.java:28-42`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyCompileException.java:1-5` | 不一致 | 实际规范化嵌在 Compiler，入口为可变文件 Path 与 revision，不是冻结 dataset/config 深模块。公共接口不泄漏 Jena 类型、部分错误结构化属于已满足子项，但不能替代边界要求。 |
| I04 · §6.8:475–476：canonicalizer 必须通过 RDFC-1.0 官方测试向量 | `control-plane/ontology/src/test/java/io/ruleops/ontology/OntologyContractPackCompilerTest.java:17-45` | 未实现 | 所查模块只含三个自有测试，未找到官方向量或 conformance harness；重复执行同一输入只证明局部确定性。 |
| I05 · §6.8:478–479：批准/验证后编译 OntologyContractPack，权威格式 deterministic Protobuf | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:18-26`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPack.java:8-13` | 不一致 | 实际输出 ontology-pack.nt 和 manifest.json；没有 protobuf pack、批准输入、验证证明。contracts/proto 仅发现 backend_host 与 workspace_resolution 两个协议文件。 |
| I06 · §6.8:481–485：pack 包含 IRI/digest、稳定概念关系枚举 ID、Capability/闭合约束、Projection 绑定、schema/compiler/compatibility/pack digest | `control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPack.java:8-15`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:21-26` | 部分一致 | 有 revision/digest/canonicalTriples；其余指定字段与编译约束表缺失，没有完整运行语义契约。 |
| I07 · §6.8:487–488：JSON 仅审查；Rust 只读签名 pack，不解析 RDF/OWL/SHACL/JSON-LD、不推理 | `runtime/rf-runner/src/main.rs:505-518`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyPackCompilerCli.java:20-26` | 部分一致 | Rust 所查路径消费 ontology digest，没有 RDF 推理；但也没有实际签名 Protobuf pack 的读取/校验。不能把“不读任何本体内容”当成“读取指定运行契约”。 |

### 补充核验：author／verifier／gate 身份不可混同

此项是任务指定的补充关注点，**不是 §6.1 的原文**，不加入 80 条规格声明统计。

| 关注点 | 证据 | 判定 | 理由 |
|---|---|---|---|
| verifier 与 gate 有不同授权角色 | `runtime/trusted-context/src/lib.rs:160-180` | 一致 | Conformance Evidence 使用 VERIFICATION_SIGNING_ROLE，GateAttestation 使用 GATE_SIGNING_ROLE 验签。角色名分离已有代码。 |
| 不同角色必须不同 key／主体，且 author 不得同时充当 verifier/gate | `runtime/trust/src/lib.rs:821-834`；`runtime/verification-worker/src/lib.rs:255-260` | 无法验证 | TrustStore 可将同一 key 授权给多个角色，没有互斥校验；Worker 接受调用方 signer，没有 author identity 参数用于分权比较。没有发现进程/身份层的互斥证据，亦未检查部署 IAM/KMS。不能凭角色名断言人员/工作负载已隔离。 |

## 问题清单（按严重度）

严重度按本域契约缺失后果划分；以下均非已复现生产攻击。

### P1：影响本体权威与可信运行契约

1. **F01：RDFC-1.0 与签名运行契约未落地**（C01–C12、I03–I07）。现有 v0 Compiler 是排序 N-Triples，不支持完整 dataset/命名图、RDFC 官方向量；输出无本体签名 envelope、无 deterministic Protobuf pack、无独立 asserted/compiled 身份。它能产出稳定 hash，但不能作为规格承诺的本体供应链身份。**这是最严重问题。**
2. **F02：Semantic Projection 权威及发布阻断断链**（D14–D16）。STANDARD_AMBIGUOUS 只是外部分类可选枚举，未发现原文/投影/Intent 冲突检测、Standard Owner 批准解除与 Release 的强制绑定；不得把该状态名当作完整治理控制。
3. **F03：跨资产 ontology 身份未闭合**（D10、V06、I07）。Release、Bundle 有相关 digest，不代表 Policy/Evidence/Execution 的每份签名资产都绑定同一契约；当前 PolicyProfile 没有该字段，运行只比较 digest 而非读取已验证 pack。
4. **F04：Git 权威写入与 API journal 写入契约冲突**（A04）。API 在本地 Store 中直接创建治理资源；未发现“生成 Git 变更”的转换。需澄清其是否仅为非权威演示 seam；本次不推定已部署双写或擅自判定 V0.2 ADR 的优先级。

### P2：控制边界及验证覆盖不足

5. **F05：Factory sandbox 未沿 Candidate 入口闭合**（A06–A12）。Scan sandbox 的 no-network、只读、资源参数不能替代 Factory 构建约束；本地脚本直接运行宿主 Java/Maven。生产凭证是否实际存在、是否有外层隔离仍无法验证。
6. **F06：本体验证与变更治理仍缺主体实现**（D06–D13、T02–T10、V01/V03–V05、I01/I02）。SHACL、RL 白名单、import pin、影响分析/重验、version IRI/历史制品治理尚未形成可执行链。不能将 prototype 的 CLOSED_WORLD IRI 检查等同完整本体验证。
7. **F07：Gate 入口的 head/base 与 Exception provenance 需补充边界证据**（A20、A26）。所查 Context 比较 commit，handoff 接受裸 fingerprint 集；尚未证明从可信 base/Grant 到这些输入的全链验证。属于证据缺口，不是已确认可绕过。
8. **F08：角色授权不等于身份互斥**（补充核验）。TrustStore 本身允许一把 key 授权多角色；若部署政策要求 author/verifier/gate 不同主体，应在部署/信任配置证明或拒绝互斥冲突，不能引用 §6.1 代替这项政策。

### P3：可选项或边界待确认

9. **F09：未验证组织/阶段与外部证据**（A05、D12、T05、V05、V07）。PR 保护、Steward 审批、Artifact Repository 留存、SKOS 是否必要、V0.2 compatibility 是否启用均不可从本次局部源码确认；不能把这些自动列成已发生违规。

## 整改建议

1. **先确认权威承诺边界**：明确 v0 原型允许承担哪些非权威用途，以及当前是否宣称满足 §6 V1。未满足前，不把排序三元组 hash 或顶层 bundle ontology digest 对外描述成已完成 RDFC＋签名 OntologyContractPack。
2. **最小闭环先修 F01＋F03**：由冻结 asserted dataset 与固定依赖清单进入独立 canonicalizer，跑官方向量；随后生成含必备字段的 deterministic Protobuf pack 与签名 envelope，并使 Release／Policy／Evidence／Execution 在消费边界验证同一 pack identity。不要只增加几个 JSON 字段后声称符合。
3. **再闭合 Projection→Release**：批准投影绑定 Scenario/Intent；冲突生成 STANDARD_AMBIGUOUS；只有可验证 Standard Owner 批准与新验证证据才能解除。负向验收至少覆盖缺 projection、映射冲突和未授权批准都不能签发 Release。
4. **明确写入源与隔离层**：Pilot 权威写入回到 Git PR，或提交明确的规格/ADR 变更；Candidate 构建复用既有隔离能力但必须从 Factory 入口实际路由，验证无网络、无凭证、资源上限及依赖允许集，而非复制扫描配置作证明。
5. **补最少边界检查**：新增跨资产 ontology digest 不同、旧 revision 变更、非 ADDITIVE 未重验、同 key 多互斥角色、未验证 exception 与 base/head 不符等负向验收。角色互斥的具体所有权/部署政策需协调者确认，本报告不代作产品裁决。

## 证据边界

- 检索工具：当前没有语义检索/context-mode 工具，使用 `rg`、精确 `sed -n`、`nl -ba`、限定目录文件列表；没有建立/更新任何索引，也没有调子代理。
- 本体重点搜索范围：`control-plane/ontology/src`、`control-plane/ontology/pom.xml`、`specs/ontology`；关联契约只扩展到 `control-plane/authoring/src/main`、`control-plane/coverage/src/main`、`runtime/release/src`、`runtime/rule-bundle/src`、`runtime/governance-registry/src`、`contracts`。搜索词包括 `OntologyCanonicalizer|RDFC|SHACL|SemanticProjection|OntologyCompatibilityEvidence|CONSTRAINT_TIGHTENING`，未找到这些实现；另查 `materializ|JSON.LD|TDB|@context|imports` 未见 ontology 主源码匹配。枚举 `STANDARD_AMBIGUOUS` 有一处匹配，不能据其存在推导审批闭环。
- 四平面核对限于 Java 模块声明、Candidate/验证入口、Rust Runner/Trusted CI/Gate/Trusted Context/Sandbox/Trust 的上述关键位置。没有穷举所有调用者、平台、客户端、部署清单、后端或历史 ADR。
- 已主动区分：LOCAL_UNATTESTED 本地预览与 Trusted 路径；可选 SKOS／V0.2 compatibility 与强制 V1 契约；摘要存在与语义正确；namespace 保护与审批；角色名与不同主体；静态实现与运行事实。
- 验证方式：静态阅读；报告引用路径/行区间和声明统计进行了机械校验。没有运行 Maven/Cargo 测试或 `bash scripts/check.sh`：本任务明确只读、不改源码/docs/scripts，完整门禁会生成构建及其他输出，且不适合 30 分钟分域审计。测试文件作为设计覆盖证据，不冒充测试通过。
- 留存：只创建本报告；审计前与写报告前 `git status --short` 为空。所有结论固定于上述 HEAD，本报告不声称后续代码或生产状态相同。
