# Enterprise Rule Factory

## 最终架构设计、Technical Spec 与实施计划

**版本：V1.2 Final**\
**日期：2026-09-16**\
**状态：可启动 / PoC Implementation Baseline**\
**定位：独立于 Amber 的企业级 RuleOps / Code Rule Engineering &
Governance Platform**

------------------------------------------------------------------------

## 0. Executive Summary

Enterprise Rule
Factory（ERF）用于把企业代码规范转化为**可验证、可发布、可执行、可审计、可演进**的机器检查资产。

系统不重新实现静态分析器，也不承诺从任意两组正反例可靠合成任意静态分析规则。系统的核心价值是建立从：

> Standard → Scenario → Rule Intent → Rule Implementation → Verification
> → Release → Enforcement → Feedback

的完整 RuleOps 生命周期。

首期聚焦 Java，PMD 为主 Backend；SpotBugs
用于字节码语义类规则。架构从第一天保持 Backend-neutral，可后续接入
ast-grep、Semgrep、Checkstyle、Error Prone、ArchUnit、CodeQL 等。

### 0.1 核心原则

1.  **规范优先，不是规则代码优先。**
2.  **RuleIntent 与 RuleImplementation 分离。**
3.  **AI 只参与 Authoring，不参与生产期裁决。**
4.  **执行必须确定性、可复现、可审计。**
5.  **没有发现问题，不等于完成了检查。**
6.  **Gate 使用 PASS / FAIL / INCONCLUSIVE 三态。**
7.  **历史债务与新增违规分离。**
8.  **规则必须经过独立 Verification 才能晋级。**
9.  **中央治理、分布式执行。**
10. **先判断是否需要新规则，再进入规则生产。**

------------------------------------------------------------------------

# 1. Problem Definition

企业代码规范通常散落在 Wiki、文档、培训材料、Code Review
经验和静态分析器配置中，存在四类断层：

-   规范存在，但没有机器检查；
-   检查能力存在，但没有启用或配置不正确；
-   规则存在，但误报、漏报、版本和适用范围不可治理；
-   CI 给出"0 告警"，但无法证明实际完成了预期扫描。

ERF 的目标不是增加规则数量，而是提高：

-   Standard Coverage；
-   Qualified Rule Coverage；
-   Rule Quality；
-   Enforcement Reliability；
-   Rule Authoring Efficiency；
-   Governance Auditability。

------------------------------------------------------------------------

# 2. Product Boundary

## 2.1 In Scope

-   企业代码规范目录与版本；
-   Scenario 分解；
-   Coverage Analysis；
-   Rule Intent 建模；
-   Typed Rule IR；
-   Template / Native Rule Authoring；
-   Backend Capability Routing；
-   Candidate Rule 构建；
-   Verification Lab；
-   Evidence Bundle；
-   Human Review；
-   Rule Registry；
-   Signed Rule Release；
-   Policy Bundle；
-   CI/CLI Scan Runtime；
-   Finding Normalization；
-   Completeness；
-   Baseline；
-   Exception；
-   PASS / FAIL / INCONCLUSIVE Gate；
-   Feedback、升级、撤回、回滚和退役；
-   运营指标与审计。

## 2.2 Explicit Non-goals

V1 不承诺：

-   任意自然语言规范自动生成可靠规则；
-   任意语言、任意分析引擎的一键转换；
-   自研完整 AST / CFG / Dataflow Engine；
-   用 LLM 在 CI 中直接判断代码是否违规；
-   自动修复后直接合并代码；
-   首期建设大型微服务平台；
-   首期覆盖公司所有规范；
-   把所有规则直接升级为 blocking；
-   证明业务逻辑正确性。

------------------------------------------------------------------------

# 3. Operating Model

采用：

> **Central RuleOps Governance + Distributed Enforcement**

``` text
                     RuleOps Center
                           |
       +-------------------+-------------------+
       |                   |                   |
 Standards Center      Rule Factory       Rule Registry
       |                   |                   |
       |             Verification Lab          |
       |                   |                   |
       +-------------------+-------------------+
                           |
                      Policy Center
                           |
                    Signed Rule Bundle
                           |
             +-------------+-------------+
             |             |             |
          Repo A        Repo B        Repo C
             |             |             |
             +-------------+-------------+
                           |
                   Findings / Coverage
                           |
                Feedback / Exception
                           |
                           v
                     RuleOps Center
```

### 3.1 Roles

  Role                     Responsibility
  ------------------------ -------------------------------------------
  Standard Owner           定义规范、适用范围、业务目标
  Rule Author              将规范转为 Scenario、Intent、Examples
  Rule Expert              处理语义、Backend、边界条件和 Native Rule
  Independent Reviewer     独立审核 Evidence 与规则晋级
  Platform Operator        Registry、Runner、签名、兼容矩阵、CI 集成
  Repo Owner / Tech Lead   项目接入、债务和 Exception 治理
  Developer                消费 Finding、修复、反馈误报
  Security/Compliance      高风险规则、例外、审计策略

------------------------------------------------------------------------

# 4. End-to-End Lifecycle

``` text
Enterprise Standard
        |
        v
Standards Catalog
        |
        v
Scenario Decomposition
        |
        v
Coverage Analysis
   /        |          \
Existing  New Rule   Not Detectable
   |         |            |
Configure    v       Manual Control
        Rule Authoring
             |
             v
          RuleIntent
             |
             v
          Rule IR
             |
             v
     Capability Routing
             |
     Backend Compilation
             |
             v
         Candidate
             |
             v
     Verification Lab
             |
             v
       Evidence Bundle
             |
             v
        Human Review
             |
             v
           SHADOW
             |
             v
      Real Repo Evidence
             |
             v
       Signed Release
             |
             v
        Rule Registry
             |
             v
        Policy Bundle
             |
             v
       Distributed Scan
             |
             v
     Unified Findings
             |
             v
       Completeness
             |
             v
     Baseline Comparison
             |
             v
         Policy Gate
       /      |       \
    PASS     FAIL   INCONCLUSIVE
             |
      Fix / Exception /
        Feedback Loop
```

------------------------------------------------------------------------

# 5. Logical Architecture

``` text
+-------------------------------------------------------------------+
|                         CONTROL PLANE                             |
|                                                                   |
|  Standards   Authoring   Factory Kernel   Verification   Registry |
|      |           |             |               |            |     |
|      +------> RuleIntent --> Rule IR --> Candidate --> Evidence   |
|                                      |                --> Release |
+--------------------------------------+----------------------------+
                                       |
                              Signed Rule Bundle
                                       |
============================ TRUST BOUNDARY =========================
                                       |
+--------------------------------------v----------------------------+
|                        EXECUTION PLANE                             |
|                                                                   |
| Snapshot -> Scan Planner -> Backend Runners -> Normalizer          |
|                                      |                            |
|                                      v                            |
|                                Completeness                       |
|                                      |                            |
|                                Baseline Engine                     |
|                                      |                            |
|                                 Policy Gate                        |
+-------------------------------------------------------------------+
```

## 5.1 Control Plane

负责：

-   Standard / Scenario / RuleIntent；
-   Authoring；
-   Candidate 生成；
-   Verification；
-   Review；
-   Release；
-   Registry；
-   Policy；
-   Exception；
-   Feedback；
-   Audit。

PoC 阶段优先使用 Git + CLI + CI + Artifact
Repository，不要求先建设微服务。

Pilot 期间的权威写入源为 Git：Standard、Scenario、RuleIntent、Policy、Ontology、
Shape、Semantic Projection 和 Release Manifest 通过 PR 审批；二进制 Artifact 与大型
Evidence 保存在 Artifact Repository，Git 只保存 digest 和 reference。CLI/API 生成
可审查的 Git 变更，不与 Server DB 双写。

## 5.2 Factory Execution Plane

Candidate plugin/rule 构建属于不可信工作负载：

-   无生产凭证；
-   默认无网络；
-   CPU / Memory / Time 限制；
-   隔离文件系统；
-   固定工具链；
-   Dependency allowlist；
-   输出 Artifact + Build Evidence。

## 5.3 Scan Execution Plane

运行在企业代码授权域内：

-   默认不上传源代码；
-   只消费签名 Rule Bundle；
-   固定 Engine / Rule / Policy identity；
-   输出 Findings + Completeness；
-   不允许模型参与最终判断。

## 5.4 Trusted Gate Plane

Gate 与 Scanner 解耦，验证：

-   source identity；
-   head/base identity；
-   rule bundle digest；
-   execution lock；
-   policy digest；
-   baseline identity；
-   completeness；
-   exceptions；
-   finding delta。

------------------------------------------------------------------------

# 6. Domain Model

核心模型：

``` text
Standard
   |
   +-- Scenario
          |
          +-- RuleIntent
                 |
                 +-- RuleImplementation [1..n]
                        |
                        +-- Evaluation [0..n]
                        |
                        +-- RuleRelease [0..n]
                               |
                               +-- ScanExecution
                                      |
                                      +-- Finding
                                      +-- Completeness
                                      +-- GateDecision
```

## 6.1 Identity Separation

必须区分：

-   **Standard Identity**：规范身份；
-   **Scenario Identity**：规范下的可检查场景；
-   **Rule Semantic Identity**：规则要表达的语义；
-   **Implementation Identity**：PMD/SpotBugs 等具体实现；
-   **Release Identity**：冻结后的发布资产；
-   **Execution Identity**：某次扫描的环境与输入身份。

这允许同一个 Intent 同时存在多个 Backend 实现。

## 6.2 Ontology Authority

ERF 引入权威 ERF Ontology，分为：

-   **Concept Layer / TBox**：定义 Standard、Scenario、RuleIntent、Capability、Backend、
    Evidence、Policy、Finding 等概念、关系和语义约束；
-   **Instance Layer / ABox**：为受治理资产分配稳定 Asset IRI，并把每次语义变更保存为
    不可变 Ontology Revision。

Control Plane 可使用本体查询和受控推理辅助 Coverage、Authoring、冲突发现和
Capability Routing。推理结果本身不是 RuleRelease、Evidence 或 Gate Evidence。

Execution Plane 不执行开放世界推理。固定 Ontology Revision 必须先编译为封闭、
确定性的 Schema/约束和运行 Manifest，经验证、计算 digest 并签名后才能进入
RuleBundle。RuleRelease、Policy、EvidenceBundle 与 ScanExecution 必须记录其依据的
ontology revision/digest。

## 6.3 Ontology Modules and Governance

ERF Ontology 不建设单一巨型企业本体，也不允许各团队使用无治理字符串词表：

-   **Core Ontology**：Standard、Scenario、RuleIntent、Capability、Evidence、Release、
    Policy、Execution 等稳定概念，由中央 Ontology Steward 维护；
-   **Language Extension**：Java 及未来其他语言的语义概念；
-   **Backend Extension**：PMD、SpotBugs 等 Backend 能力和制品概念；
-   **Enterprise Domain Extension**：安全、可靠性、架构与业务域语义。

外部标准优先通过稳定 IRI 映射，不复制整套外部本体。Core 变更由中央所有者
审批；Extension 由对应领域所有者审批；跨边界变更必须联合审查。每次变更必须
附带兼容性分类、影响分析、受影响资产和迁移计划；禁止原地改变已发布概念语义。

## 6.4 Standard Semantic Projection

Standard 原文保存组织规范意图；每个可机器执行的 Scenario 必须具有经批准的
Semantic Projection，映射到具体本体概念、关系与适用范围。该投影是机器执行的
权威语义表达，但不删除或替代 Standard 原文。

当原文、Semantic Projection 或 RuleIntent 之间存在冲突或未解决映射时，Coverage 状态
必须为 `STANDARD_AMBIGUOUS`，并阻止 RuleRelease。只有 Standard Owner 审批后的
Semantic Projection 可解除该阻塞；AI 或本体推理产生的映射始终是 Candidate。

## 6.5 Ontology Technology Baseline

V1 本体技术栈固定为：

-   **RDF 1.1**：权威图数据模型和 IRI 身份层；
-   **OWL 2 RL restricted subset**：仅在 Control Plane 用于分类、关系推导、冲突和缺口发现；
-   **SHACL Core**：对固定 data graph + shapes graph 执行发布前封闭约束验证；
-   **JSON-LD 1.1**：API 和 authoring 交换序列化，不是独立语义层或直接签名输入；
-   **SKOS**：按需表达标签、同义词、broader/narrower/related 与词表映射。

V1 约束：

-   禁止 SHACL-SPARQL、SHACL-JS、SHACL-AF 及其他可执行扩展；
-   禁止递归 Shape；
-   禁止在权威构建中从网络解析未按 digest 固定的 `@context` 或 ontology import；
-   禁止 OWL 2 DL/Full 或未列入允许集的 RL 特性；
-   影响执行的推理结果必须在 Control Plane materialize，再编译和签名；
-   Runner 和 Gate 不携带 RDF/OWL Reasoner。

## 6.6 Ontology Canonicalization and Signature

权威构建必须将所有 `@context` 和 ontology import 固定到 digest，对完整 asserted RDF
dataset 使用 RDFC-1.0 生成 canonical N-Quads，再计算 digest 和签名。禁止直接
对 Turtle 或 JSON-LD 源文本计算语义身份。

签名 envelope 至少包含：

-   canonicalization/hash/signature algorithm；
-   ontology IRI 与 version IRI；
-   dataset 边界和 graph names；
-   shapes revision 与每个 import/context digest；
-   validator、reasoner 和 compiler 版本；
-   canonical asserted dataset digest；
-   compiled runtime artifact digest。

优先使用命名 IRI，限制 blank node 数量和图复杂度。Canonicalization 超时、超出资源
配额或失败时必须 fail closed。不签名某个 Reasoner 的全量推理 closure；影响执行的
推理结果 materialize 到可重现的 compiled artifact 并单独签名。

## 6.7 Ontology Versioning and Compatibility

-   稳定 **ontology IRI** 识别逻辑本体系列；
-   不可变 **version IRI** 识别具体 Ontology Revision；
-   RDFC-1.0 规范化后的内容 digest 是供应链身份；
-   SemVer 可用于人类沟通，不作为内容身份。

变更分为 `ADDITIVE`、`CONSTRAINT_TIGHTENING`、`BEHAVIORAL` 和 `BREAKING`。
所有非 `ADDITIVE` 变更都必须运行影响分析、重新验证受影响资产并生成新编译
制品。旧 version IRI、canonical dataset、签名和编译制品必须保持可审计访问。

V0.1 中 RuleRelease、Resolved Policy Manifest 和 RuleBundle 引用的
OntologyContractPack 必须精确 digest 匹配。V0.2 可引入签名的
`OntologyCompatibilityEvidence`，但只允许证明 `ADDITIVE` revision 对既有资产兼容；
`CONSTRAINT_TIGHTENING`、`BEHAVIORAL` 或 `BREAKING` 变化必须重新验证受影响资产并产生
新 RuleRelease，禁止用兼容声明绕过。

## 6.8 Ontology Implementation and Runtime Contract

Control Plane 以 Apache Jena 作为 RDF dataset、JSON-LD、SHACL Core 和基础图处理的
主工具链，避免同时引入多套重叠对象模型。OWL 2 RL 不开放任意 Reasoner 能力，而是
以 ERF 白名单规则集进行有界 materialization，并用固定推理用例验证。

Git 中按 revision 固定的 RDF、Shape、Semantic Projection 和 manifest 是权威写入源。
Jena Dataset/TDB 只作为可从该 revision 重建的查询 read model，不接受绕过 PR 和
发布流程的权威写入；索引身份必须记录 source revision 与 canonical dataset digest。

RDFC-1.0 位于独立 `OntologyCanonicalizer` 深模块之后。它的 Interface 只接受已冻结的
dataset 与算法配置并返回 canonical bytes、digest 或结构化失败；具体实现必须通过
RDFC-1.0 官方测试向量。Jena 或 canonicalizer 的库类型不得进入领域 Interface。

Control Plane 将已批准、已验证的 Ontology Revision 编译为
`OntologyContractPack`。权威运行格式为 deterministic Protobuf，至少包含：

-   ontology/version IRI 与 canonical digest；
-   稳定概念、关系和枚举 ID；
-   Capability 关系与封闭约束表；
-   Semantic Projection 到 Scenario/RuleIntent 的绑定；
-   schema/compiler version、compatibility classification 与 pack digest。

同时可以生成便于人工审查的 JSON view，但它不是权威执行输入。Rust Runtime 只读取
签名后的 OntologyContractPack，不解析 RDF、OWL、SHACL 或 JSON-LD，也不执行推理。

------------------------------------------------------------------------

# 7. Standard & Scenario Spec

``` yaml
apiVersion: rulefactory.io/v1
kind: Standard

metadata:
  id: SEC-JAVA-017
  version: 1.0.0
  owner: security-platform

spec:
  title: 禁止业务代码直接终止 JVM
  description: 业务代码不得直接调用 JVM 终止 API
  languages: [java]

  scenarios:
    - id: direct-system-exit
      expected: detectable

    - id: runtime-halt
      expected: needs-clarification

    - id: reflective-invocation
      expected: unsupported
```

Scenario 是 Coverage
的最小业务单元，不允许只用"整条规范是否覆盖"表达复杂覆盖关系。

------------------------------------------------------------------------

# 8. Coverage Analysis

在创建新规则前必须分类：

``` text
EXISTING_RULE
EXISTING_RULE_NOT_ENABLED
EXISTING_RULE_UNSUITABLE
NEW_RULE_REQUIRED
ENVIRONMENT_GAP
BACKEND_CAPABILITY_GAP
STANDARD_AMBIGUOUS
NOT_MACHINE_DETECTABLE
```

输出 Coverage Ledger：

``` yaml
standard: SEC-JAVA-017
coverage:
  direct-system-exit:
    status: covered
    rule: corp.java.no-system-exit

  runtime-halt:
    status: partial

  reflective-invocation:
    status: unsupported
```

Coverage Ledger 是运营资产，而不是临时报表。

------------------------------------------------------------------------

# 9. RuleIntent Spec

``` yaml
apiVersion: rulefactory.io/v1
kind: RuleIntent

metadata:
  id: corp.java.no-system-exit
  version: 1.0.0

spec:
  standard:
    id: SEC-JAVA-017
    version: 1.0.0
    scenario: direct-system-exit

  language:
    id: java
    sourceLevels: ["11", "17", "21"]

  intent:
    statement: 禁止业务代码直接调用 java.lang.System.exit(int)

  scope:
    includeSourceSets: [main]
    excludeSourceSets: [test]

  semantics:
    target:
      kind: method-call
      declaringType: java.lang.System
      method: exit
      invocation: static
      parameters: [int]

  requiredCapabilities:
    - java.ast
    - java.symbol-resolution

  unresolved:
    behavior: unknown

  examples:
    bad:
      - "System.exit(1);"
      - "java.lang.System.exit(0);"
    good:
      - "shutdownManager.shutdown();"

  severity:
    default: advisory
```

RuleIntent 禁止嵌入任意可执行脚本。

------------------------------------------------------------------------

# 10. Rule IR

V1 不设计万能 DSL。IR 是有限、类型化、可验证的中间表达。

``` text
RuleIR
  |- ExistingRule
  |- StructuralRule
  |- SymbolRule
  |- TypeRule
  |- BytecodeRule
  |- ArchitectureRule
  `- NativeRule
```

## 10.1 Example

``` yaml
kind: SymbolRule
language: java

match:
  node: method-call
  symbol:
    owner: java.lang.System
    name: exit
    parameters: [int]

scope:
  sourceSets: [main]

onUnresolved: unknown
```

复杂跨过程 Dataflow 不强行塞入 V1 通用 IR；由专用 Backend 或 NativeRule
表达。

MVP 不因为 IR 枚举了七种类型就实现七类 Template。Phase 0 必须根据
20 个候选 Scenario 和 Coverage Gap 的实际分布选择最多三类；默认候选为
`ExistingRule`、`StructuralRule` 和 `SymbolRule`。SpotBugs Bytecode 以一个
Native 闭环验证，不用于推动 IR 为了形式完整而扩张。

------------------------------------------------------------------------

# 11. Backend Adapter SPI

``` text
describe()   -> CapabilityDescriptor
validate()   -> ValidationResult
compile()    -> RuleArtifact
verify()     -> BackendVerification
plan()       -> BackendExecutionPlan
execute()    -> RawResult
observe()    -> BackendObservation[]
```

该 SPI 是 Backend Host 内部 Interface，不是 Rust Runtime 直接加载的语言 ABI。
Host Adapter 在进程内将它映射为跨进程 Backend Host Contract；Runtime 只消费
ERF 自有的 capability、plan、observation、error 和 completeness fact。

最重要的能力是 `describe()`。

``` yaml
backend: pmd-java
version: "7.x"

capabilities:
  languages:
    java: ["8", "11", "17", "21"]

  facts:
    ast: true
    symbolResolution: true
    typeResolution: true
    bytecode: false
    interproceduralDataflow: false
```

Router 必须允许返回 `UNSUPPORTED`，禁止静默降级为低可信实现。

## 11.1 Backend Host Contract

Rust Runtime 与 Backend Host 使用 ERF 自有、版本化的跨进程契约：

``` text
stdin  -> length-delimited Protobuf request frames
stdout <- length-delimited Protobuf response/event frames
stderr <- structured operational logs
```

`stdout` 只能承载协议帧；任何非协议输出、未知必需字段、超限帧或损坏帧都使本次
Backend 执行失败并进入 Completeness，禁止尝试猜测或容错解析。契约至少覆盖：

-   protocol/capability negotiation；
-   request correlation；
-   describe、plan、execute 和 cancel；
-   deadline 与 cooperative cancellation；
-   最大帧、最大事件数和资源限制；
-   BackendObservation、Completeness fact、structured error 和 completion。

Host 不开启本地网络监听器，也不通过 JNI、FFI 或 N-API 嵌入 Rust 进程。新增语言只需
实现该契约，不得把特定 VM、SDK 或引擎对象泄漏进 Runtime。

协议版本使用 `major.minor`：major 不一致时拒绝执行；同一 major 内只允许
Protobuf additive evolution，已发布 field number 不得复用或改变语义。握手必须交换
版本范围、required/optional capability、消息与资源上限。未知 optional field 可忽略，
无法满足 required capability 必须显式失败。

治理消息禁止使用 Protobuf `Any`、不透明 JSON 或 Backend 私有 payload 绕过契约。
Rust、Java 和未来 JavaScript 实现必须共享 schema，并运行新旧版本双向 golden
compatibility tests。

Workspace Resolver Adapter 同样使用 length-delimited Protobuf framing、版本协商、
deadline、取消和资源上限，但采用独立的 Workspace Resolution schema。它不得复用
Backend Host 的 describe/plan/observation 消息伪装项目解析；Rust Runtime 分别拥有
Backend Host Client 与 Workspace Resolver Client。

## 11.2 Backend Observation Location

`BackendObservation` 使用封闭 typed location union：

``` text
SOURCE_SPAN | SYMBOL | BYTECODE_MEMBER | FILE
```

`SOURCE_SPAN` 必须绑定 repository-relative path、repository blob digest 与 UTF-8 byte
range；Host 负责把引擎私有坐标转换为该规范坐标，并可保留原始 line/column 作为诊断。
`SYMBOL` 与 `BYTECODE_MEMBER` 使用规范化 owner/name/signature，可选绑定对应 source
blob。`FILE` 只用于无法提供更精确、但仍可验证文件身份的观测。

Rust Runtime 验证 blob 与范围、推导展示行列并计算 Semantic Anchor。坐标越界、blob
不匹配或 location kind 所需字段缺失属于结构化 observation error，不能生成 Finding。

## 11.3 Contract Conformance Suite

ERF 维护版本化、与具体 Backend 无关的 Contract Conformance Suite，包含 reference fake
Host/Resolver、golden messages 和 failure injection。至少验证 negotiation、跨版本兼容、
畸形/未知/超限 frame、backpressure、取消、deadline、crash、retry、位置坐标、planned-unit
accounting、determinism、Completeness、资源限制和 sandbox escape resistance。

BackendRuntimePack 与 WorkspaceResolverPack 只有在声明并通过精确 suite digest、保存
ConformanceEvidence 后才能晋级。实现方单元测试不能替代该套件；suite major 变化必须
重新认证受影响 Pack，ExecutionLock 固定所依据的 suite identity。

Suite release 本身是签名、不可变资产。独立 Verification Worker 在隔离环境中对精确
Pack digest、suite digest、platform、RunnerRelease 和完整测试结果生成由
`verification-signing` role 签名的 ConformanceEvidence；Pack Builder 只能提交候选 Pack，
不得选择性删减测试、签发证据或自行晋级。

ConformanceEvidence 同时绑定 Sandbox Profile/Provider digest。Pack、suite、RunnerRelease、
Sandbox Profile/Provider 或 platform 任一身份变化都会使原聚合证据不再适用于新组合；
Verification Worker 可以引用仍适用的不可变 test result，但必须运行受影响测试并签发新的
聚合 ConformanceEvidence，禁止由 Runtime 推断“应该仍兼容”。

------------------------------------------------------------------------

# 12. Backend Strategy

## Tier 1 / Pilot

### PMD

用途：

-   Java AST；
-   XPath/structural；
-   Java native rule；
-   symbol/type 相关规则。

### SpotBugs

用途：

-   bytecode；
-   编译后语义；
-   PMD 不适合的 detector。

## Tier 2

-   ast-grep：轻量 structural、多语言、YAML rule；
-   Semgrep：pattern/taint/multi-language；
-   Checkstyle：Java style；
-   Error Prone：compile-time correctness；
-   ArchUnit：architecture rules。

## Tier 3

-   CodeQL；
-   Infer；
-   企业内部分析器。

接入顺序由真实 Coverage Gap 决定，不以"支持工具数量"为 KPI。

------------------------------------------------------------------------

# 13. AI Authoring

AI 可以：

-   解释规范；
-   识别歧义；
-   建议 Scenario；
-   生成边界案例；
-   生成 RuleIntent candidate；
-   生成 Rule IR candidate；
-   生成 Native Rule scaffold；
-   生成测试 candidate；
-   生成 Finding explanation。

AI 不可以：

-   自主批准；
-   自主发布；
-   获取签名权限；
-   修改生产 Policy；
-   在 CI 中作为最终违规裁决器。

模型输出始终视为 untrusted candidate。

AI 与检索外围能力统一输出非权威 `AuthoringSuggestion`，不得直接解析为
Finding、EvidenceBundle、RuleRelease 或 GateDecision。被人工接受的建议仍必须
创建新 Candidate，再进入正常 Verification 与 Review。

V0.1 必须内建 ERF 自有 Authoring Orchestrator 的最小能力：冻结输入范围、
确定性任务分组、失败回退、互斥终态和来源追溯。Authoring Retrieval
先仅提供 ID lookup、metadata filter 和 FTS/BM25 baseline；在语料规模和离线
检索质量证明收益前，不引入语义向量检索。

------------------------------------------------------------------------

# 14. Verification Lab

Candidate 必须通过独立 Verification。

## 14.1 Verification Layers

1.  Contract Validation
2.  Build / Load Validation
3.  Positive Tests
4.  Negative Tests
5.  Near-miss Tests
6.  Boundary Tests
7.  Holdout Tests
8.  Mutation Tests（适用时）
9.  Representative Repository Shadow Scan
10. Compatibility
11. Performance
12. Security / Isolation
13. Repeatability

## 14.2 Evidence Bundle

``` yaml
kind: EvidenceBundle

rule: corp.java.no-system-exit
implementation: pmd-java/v1

evidence:
  contract: ...
  semantic: ...
  holdout: ...
  repository: ...
  compatibility: ...
  performance: ...
  security: ...

conclusion:
  state: verified
```

Framework 定义 `VERIFIED` 的最低 Evidence：Contract、Positive、Negative、
Near-miss、Holdout、Compatibility 和 Repeatability。Policy 可根据风险等级对后续
晋级追加 Shadow 样本量、Precision、Performance、Security 和人工审批阈值，
但不得削减 Framework 最低证据。

------------------------------------------------------------------------

# 15. Rule Maturity Lifecycle

``` text
DRAFT
  |
CANDIDATE
  |
VERIFIED
  |
SHADOW
  |
ADVISORY
  |
BLOCKING
  |
DEPRECATED
  |
RETIRED
```

旁路状态：

``` text
REJECTED
REVOKED
```

`BLOCKING` 不是规则生成成功后的默认状态，而是治理晋级结果。

每次状态转换都是显式、可审计的命令，必须验证源状态、目标状态、所需
Evidence、角色和审批记录。Rule Author 不得独立完成 `CANDIDATE → VERIFIED`；
只有发布签名身份可产生 RuleRelease；只有 Policy Owner 可将 RuleRelease 加入
Blocking Profile。

------------------------------------------------------------------------

# 16. Rule Release

RuleRelease 是不可变资产：

``` text
RuleRelease
  |- RuleIntent
  |- RuleImplementation
  |- RuleArtifact
  |- Backend Identity
  |- Backend Engine Lock
  |- Compatibility
  |- Evidence Bundle
  |- Remediation Metadata
  |- SBOM
  |- Provenance
  |- Signature
  `- Release Manifest
```

任何语义、Artifact、Backend lock 或 Evidence 变化产生新 Release。

------------------------------------------------------------------------

# 17. Registry

Registry 负责：

-   immutable releases；
-   metadata index；
-   signatures；
-   compatibility；
-   revocation；
-   policy bundle resolution；
-   provenance；
-   Backend Runtime Pack metadata、compatibility 与 revocation；
-   audit。

Registry 不负责扫描源代码。

------------------------------------------------------------------------

# 18. Policy Profile

项目不直接复制规则文件，而消费 Profile：

``` yaml
apiVersion: rulefactory.io/v1
kind: PolicyProfile

metadata:
  id: company-java-backend
  version: 2026.09

spec:
  rulesets:
    - java-baseline
    - security
    - reliability
    - company-practices

  gate:
    blocking:
      maturity: BLOCKING
    incompleteRequiredScan: INCONCLUSIVE
```

Profile 在发布时确定性 resolve。所有 ruleset、版本、参数、Exception 和冲突
决议展开为不可变 Resolved Policy Manifest，再与 RuleRelease 组成 Signed Rule
Bundle。重复 RuleIntent、不同 RuleRelease 或不同 severity 的冲突必须显式解决，
禁止隐式 last-wins。Runner 不在扫描时动态获取“最新”ruleset。

对于每个 `Scenario × applicability scope`，Resolved Policy 必须且只能选择一个
authoritative RuleImplementation 参与 Finding、Baseline 和 Gate。其他已验证实现可以作为
`SHADOW` implementation 同时执行并产生对比 Evidence，但其 Observation 不进入权威
CanonicalFindingSet，也不能阻断 Gate。V1 禁止多个 authoritative implementation 投票、
合并或以“多数一致”掩盖分歧；分歧进入 Feedback/Verification。

权威实现出现 `UNSUPPORTED`、失败或不完整时，不得在 Runtime 中自动切换为 SHADOW
实现，结果按 Completeness 进入 `INCONCLUSIVE`。改变权威实现必须先完成语义等价验证，
再发布新的 Resolved Policy Manifest、RuleBundle 和 ExecutionLock；Runtime 不具有即时
晋升 implementation 的权限。

------------------------------------------------------------------------

# 19. Scan Contract

输入：

``` text
RepositorySnapshot
WorkspaceDescriptor
RuleBundle
OntologyContractPack
ExecutionLock
Policy
TrustedContext
Baseline
```

输出：

``` text
Findings[]
CompletenessReport
ExecutionEvidence
```

Scan 命令本身不决定 PASS/FAIL。

------------------------------------------------------------------------

# 20. Finding Schema

``` yaml
ruleId: corp.java.no-system-exit
ruleRelease: 1.2.0
scenario: direct-system-exit

location:
  path: src/main/java/A.java
  startLine: 42
  startColumn: 9

symbol:
  owner: java.lang.System
  name: exit

message: 禁止业务代码直接终止 JVM

severity: blocking

fingerprint: "sha256:..."

baselineState: NEW

evidence:
  backend: pmd-java
  engineVersion: "..."
```

Baseline state：

``` text
NEW
EXISTING
RESOLVED
SUPPRESSED
AMBIGUOUS
```

Finding 的形成分为两层：

1.  Backend Host 把原始引擎报告、位置、符号和错误映射为 policy-neutral
    `BackendObservation`，并报告实际执行单元与完整性事实；
2.  Rust Runtime 绑定 RuleRelease/Scenario，规范化仓库相对路径，计算版本化
    fingerprint 与 Baseline state，并聚合为 Finding 和 CompletenessReport。

Backend Host 不得决定 blocking/advisory、Gate 或 Baseline state；Rust Runtime
不得解析 PMD、SpotBugs 或未来 JavaScript 引擎的私有报告格式。

## 20.1 Versioned Finding Fingerprint

Finding fingerprint 必须记录算法版本。主身份由以下稳定信息构成：

``` text
RuleIntent identity
Scenario identity
normalized repository-relative path
semantic anchor
```

`severity`、message、RuleRelease 版本和 Backend 实现版本不得进入主身份，使已验证的
实现升级或文案调整不会制造虚假 NEW。Semantic Anchor 优先使用规范化 symbol、AST
结构或 bytecode identity；无法获得时才使用显式标记的 location fallback。

跨路径 move/copy 只能通过可解释的重映射证据关联。出现一对多、多对一或低置信映射时
Baseline state 必须为 `AMBIGUOUS`，不得自动归入历史债务。fingerprint 算法版本变化
必须提供迁移映射并对同一 commit 执行 double scan。

------------------------------------------------------------------------

# 21. Completeness Model

Completeness 至少按：

> rule × module/file analysis unit

表达。

状态：

``` text
COMPLETE
PARTIAL
FAILED
NOT_APPLICABLE
```

不能把：

``` text
0 findings
```

解释为：

``` text
scan complete
```

缺 dependency、stale bytecode、timeout、worker crash、report parse
failure 等都必须进入 Completeness。

执行前由 Planner 生成并冻结 ScanPlan，列出每个必需
`RuleRelease × module/file analysis unit`，并计算 plan digest。Backend 输出不得改写
该分母；每个 planned unit 必须且只能进入一个终态。任何排除、跳过或降级
必须在 ScanPlan 或 Completeness 中保留结构化 reason。

ScanPlan 之前，`Workspace Resolver` 以 RepositorySnapshot 为输入，通过 Maven、Gradle
及未来 npm/pnpm/tsconfig Adapter 生成 immutable、content-addressed
`WorkspaceDescriptor`。它描述 module、source root、language level、dependency/classpath
reference、build output 和配置文件身份，并记录 resolver/version/input digest。

Resolver 默认只做静态解析或消费可信构建导出的 metadata，不执行任意项目构建脚本。
缺失或无法确定的项目事实必须结构化为 unresolved fact，并进入 planning/completeness；
Backend Host 只能消费 WorkspaceDescriptor，不得各自猜测或重新定义项目结构。SpotBugs
所需 bytecode 必须绑定可信构建产物 digest。

Workspace resolution 只有两个显式模式：

-   `SAFE_STATIC`：Resolver 不执行项目脚本；静态事实不足以满足必需 Capability 时，
    对应单元必须进入不完整状态；
-   `ATTESTED_BUILD_EXPORT`：已有可信构建阶段在 rf-runner 和 Backend Host 之外执行
    Maven、Gradle、npm 等工具，导出 WorkspaceDescriptor、构建产物 digest、工具链身份
    和 provenance，由 Runtime 验证后消费。

模式必须进入 WorkspaceDescriptor 与 ExecutionEvidence。Backend Host 永远不启动项目
构建，也不得把缺失项目事实替换为本机猜测值。

Rust Runtime 拥有 resolution 编排与 WorkspaceDescriptor 验证；Maven、Gradle 和未来
JavaScript Adapter 以签名、内容寻址的 `WorkspaceResolverPack` 独立进程运行。可信
Policy 按精确 digest 选择 Pack，Resolver 进程使用与 Backend Host 相同的沙箱原则，
但只实现独立 Workspace Resolution Contract。

WorkspaceDescriptor envelope 必须绑定 repository identity、commit/tree digest、所有已
消费 config/lockfile blob digest、WorkspaceResolutionMode、Resolver Pack digest、工具链
身份、module graph、build output digest 和 descriptor digest。`SAFE_STATIC` 由可信 Runner
现场计算并记录 provenance，不生成构建证明；`ATTESTED_BUILD_EXPORT` 必须附带由可信 CI
workload identity 签名的独立 `WorkspaceAttestation`，Runtime 验证后才能把构建事实用于
可信 ScanPlan。

WorkspaceAttestation 使用 DSSE envelope 包装 in-toto Statement，定义版本化 ERF
Workspace predicate。Statement subject 是 WorkspaceDescriptor digest；materials/results
覆盖 repository、config/lockfile、Resolver、toolchain 和 build output digest。Verifier
必须先验证 DSSE payload type/signature，再按 Deployment Policy 验证 predicate、subject、
CI workload identity、repository/ref/environment 约束与 freshness，禁止接受自签或任意
私有 JSON 证明。

Connected CI 的 workload identity 必须绑定允许的 issuer、audience、repository、ref、
workflow、run identity、commit/tree、ExecutionLock digest、issued-at 与 expiry；证明不得在
其他仓库、提交、执行锁或环境重放。Air-gapped CI 使用组织管理、仅限 workspace
attestation scope 的专用 key，仍必须满足相同 subject/material/freshness 约束。

规划采用两级模型：Rust Planner 拥有全局 ScanPlan 与 Completeness 分母；每个 Host
在执行前返回 deterministic `BackendExecutionPlan`，把分配给它的 planned unit 映射为
引擎批次、module、classpath 和调用参数。BackendExecutionPlan 可以细化执行方式，
但不能新增、删除或合并到无法追踪原 planned unit 的状态。

不能执行的单元必须逐项返回结构化 `UNSUPPORTED`、`NOT_APPLICABLE` 或 error reason，
由 Rust Runtime 映射到 Completeness。Host 报告成功但未覆盖全部分配单元时，本次
Backend 执行不完整。

------------------------------------------------------------------------

# 22. Baseline Model

必须区分三类 Baseline：

1.  **Debt Baseline**：历史违规；
2.  **Comparison Baseline**：PR / merge-base 比较；
3.  **Quality Benchmark**：规则 Precision/Recall 等质量样本。

它们不可混用。

规则升级时应对同一 commit 做 double scan：

``` text
old release + same source
new release + same source
          |
          v
      Impact Delta
```

Debt Baseline 是中央批准、签名且不可变的治理资产，绑定 repository
snapshot、Policy 和 RuleRelease 身份。Comparison Baseline 由可信 SCM
merge-base 派生。RuleRelease 升级后只能在同源 double scan 的 Impact Delta
被显式接受后发布新 Debt Baseline。

------------------------------------------------------------------------

# 23. Gate

``` text
GateDecision =
    Policy
  + Findings
  + Completeness
  + Baseline
  + Exception
  + TrustedContext
```

Gate 先对每个 Finding 计算 BaselineState，再独立计算 `ExceptionDecision`。ExceptionGrant
不删除 Finding、不改变 fingerprint，也不覆盖 BaselineState；它只决定该 Finding 在本次
Policy/Gate 中是否豁免。报告和 GateAttestation 必须同时保留 BaselineState、
ExceptionDecision 与匹配的 grant digest。

## 23.1 Decision

### PASS

-   required scan complete；
-   identity trusted；
-   没有未豁免的新 blocking finding。

### FAIL

-   存在确定的新、未豁免 blocking finding。

### INCONCLUSIVE

-   没有确定 blocking finding；
-   但 required scan 不完整、不可比较或身份不可信。

GateDecision 严格只有 `PASS / FAIL / INCONCLUSIVE` 三态。只有被 Policy 显式
声明为 non-required/advisory 的覆盖缺失才可以 PASS，并附带
`coverage.degraded=true` 及结构化原因；任何 required analysis unit 不完整都是
INCONCLUSIVE。Config/contract/trust failure 是错误分类和 CLI exit code，不是新的
GateDecision 状态。

本地、CI 和 Controlled Runner 使用同一个确定性 Gate Engine。区别不在计算
算法，而在 TrustedContext：本地只产生 `LOCAL_UNATTESTED` preview；只有已验证
repository、commit、policy、bundle、baseline 和 runner identity 的 GateDecision 才能
作为 protected branch 的权威证明。

## 23.2 Gate Attestation

可信 Gate 必须输出 DSSE in-toto `GateAttestation`，而不只依赖进程 exit code。其 subject
是本次 GateDecision/ExecutionEvidence digest，predicate 至少绑定 repository/commit、
RuleBundle、ExecutionLock、RunnerRelease、WorkspaceDescriptor/Attestation、Baseline、
CanonicalFindingSet/semanticResultDigest、Completeness、Exception 集合和三态 GateDecision。

GateAttestation 由 Policy 允许的 Runner workload identity 签名，并包含 issued-at、expiry、
run identity 和环境身份。服务端、SCM status publisher 或 protected branch integration
只有在验证 DSSE、workload identity、全部 subject/material digest 与 freshness 后才接受；
CLI exit code 只用于进程控制，不是可移植权威证明。

GateAttestation 默认不内嵌 Finding、源码路径、symbol、message 或源码片段；只携带
semanticResultDigest、Completeness 摘要、GateDecision、关键输入 digest 和 Privacy Profile
允许的聚合计数。完整 Finding report 是独立受控资产，是否保存或上传继续由 Deployment
Policy/Privacy Profile 决定，且其 digest 必须与证明一致。

Connected CI 通过 workload identity broker 或签名服务获得仅对当前 GateAttestation
digest 有效的短期签名能力；禁止把长期 private key 注入 Runner 环境。Air-gapped Runner
只能调用受限 signer socket/HSM，签名请求必须绑定 run identity、predicate type 和 digest，
Runner 无法导出密钥或签署任意 payload。

------------------------------------------------------------------------

# 24. Exceptions

Exception 以签名、不可变、可撤回的 `ExceptionGrant` 表达：

``` yaml
grantId: exception/payment-system-exit/2026-01
ruleIntent: SEC-JAVA-017
scenario: direct-system-exit

scope:
  repository: payment-service
  path: src/main/java/Bootstrap.java
  symbol: com.example.Bootstrap#shutdown
  fingerprint: sha256:optional-exact-finding

reason: JVM bootstrap requires explicit termination
owner: platform-team
ticket: RISK-1234
effectiveAt: 2026-09-01
expiresAt: 2026-12-31
approvals: [...]
signature: ...
```

必须具备：

-   exact scope；
-   owner；
-   reason；
-   expiry；
-   approval；
-   content digest 与 `exception-signing` role signature；
-   revocation state；
-   audit。

Gate 只消费 RuleBundle/Resolved Policy 精确引用的 grant digest。过期、未生效、被撤回、
scope 不匹配或签名无效时视为不存在；禁止永久、无范围的通配 grant。代码风险
ExceptionGrant 不得豁免“扫描未执行”、身份不可信或其他 Completeness failure。

Selector grammar 是封闭的：repository、RuleIntent 和 Scenario 必填；path 只能使用从仓库根
锚定、拒绝 `..` 的受限 glob；symbol 使用规范化身份；fingerprint 如出现必须精确匹配。
所有已填写 selector 采用 AND 语义。禁止任意 regex、脚本、动态查询和隐式全仓库通配。

------------------------------------------------------------------------

# 25. Feedback Loop

``` text
Finding
  |
Feedback
  |
Triage
  +-- Not FP
  +-- Documentation
  +-- Scope Issue
  `-- True FP
          |
      Regression Case
          |
      New Revision
          |
      Verification
          |
        Shadow
          |
        Release
```

真实 FP 默认沉淀为 regression asset。

------------------------------------------------------------------------

# 26. Security Model

## 26.1 Threats

-   prompt injection；
-   malicious generated Java；
-   malicious build script；
-   dependency poisoning；
-   artifact tampering；
-   forged scan result；
-   policy tampering；
-   baseline manipulation；
-   path traversal / symlink；
-   zip bomb；
-   report injection；
-   secret exfiltration；
-   cache poisoning。

## 26.2 Controls

-   Candidate worker sandbox；
-   model 无 publish/signing 权限；
-   fixtures 默认 compile/analyze，不执行；
-   dependency allowlist / lock；
-   SBOM；
-   signed artifacts；
-   provenance；
-   revocation；
-   trusted CI injects context；
-   project PR 不可修改 trusted gate；
-   source identity validation；
-   resource quotas；
-   network deny by default；
-   immutable release。

------------------------------------------------------------------------

# 27. CLI

``` text
rf standard validate
rf coverage analyze

rf rule init
rf rule validate
rf rule generate
rf rule test
rf rule submit

rf release verify
rf release promote
rf release revoke

rf scan
rf gate
rf explain

rf feedback submit
rf exception request
```

Exit code 建议：

### `rf scan`

-   0：执行完整；
-   2：输入/contract 错误；
-   3：执行不完整；
-   4：trust failure。

是否有 Finding 不改变 scan 的"执行成功"语义。

### `rf gate`

-   0：PASS；
-   1：FAIL；
-   2：config/contract error；
-   3：INCONCLUSIVE；
-   4：trust failure。

------------------------------------------------------------------------

# 28. API Boundary

PoC 不要求服务化，但稳定 Domain API：

``` text
POST /standards
POST /coverage/analyze

POST /rules
POST /rules/{id}/generate
POST /rules/{id}/evaluations

POST /releases
POST /releases/{id}/promote
POST /releases/{id}/revoke

POST /scans
GET  /scans/{id}
GET  /findings

POST /feedback
POST /exceptions
```

------------------------------------------------------------------------

# 29. Repository Layout

``` text
rule-factory/
├── specs/
│   ├── ontology/
│   ├── standard/
│   ├── rule-intent/
│   ├── rule-ir/
│   ├── evidence/
│   ├── result/
│   └── policy/
├── contracts/
├── control-plane/
│   ├── coverage/
│   ├── authoring/
│   ├── factory/
│   ├── verification/
│   └── release/
├── runtime/
│   ├── rf-cli/
│   ├── rf-runner/
│   ├── workspace-resolver/
│   └── gate/
├── workspace-resolvers/
│   ├── maven/
│   ├── gradle/
│   └── javascript/
├── backend-hosts/
│   └── java/
├── adapters/
│   ├── pmd-java/
│   └── spotbugs/
├── templates/
├── sandbox/
├── ci/
├── examples/
└── docs/
```

## 29.1 V0.x Implementation Seams

V0.x 采用分层多语言形态：

-   Control Plane 和 Factory 使用 Java 21 模块化单体，按 RuleOps 生命周期能力划分；
-   `rf` 与 `rf-runner` 使用 Rust，共享同一 Runtime 核心；
-   Rust Runtime 拥有 Bundle/Signature、Cache、Planner、进程编排、Normalizer、
    Completeness、Baseline 和 Gate Engine；
-   分析引擎通过小型、版本化 Backend Host Interface 跨进程接入，不嵌入 Rust 进程；
-   首个 Java Backend Host 托管 PMD/SpotBugs；未来 JavaScript 或其他分析器使用自己的 Host Adapter；
-   Rust 只理解 ERF 自有 Contract，不暴露 PMD AST、SpotBugs Detector、Node API 或 VM 类型；
-   Factory Worker 作为独立进程/沙箱运行，该边界不因模块化单体而合并；
-   V0.1 必须从第一条 PMD 规则开始跑通 Rust Runtime → Java Backend Host 纵切。

------------------------------------------------------------------------

# 30. External Project Research & Design Lessons

本设计参考公开项目的成熟机制，但不复制其产品边界。

## PMD / PMD Rule Designer

可借鉴：

-   extensible static analysis；
-   AST inspection；
-   XPath rule design；
-   rule metadata；
-   rule test workflow。

结论：ERF 不重复建设 AST IDE；重点建设"企业规范 → governed rule
asset"的上层生命周期。

## ast-grep

可借鉴：

-   tree-sitter structural matching；
-   YAML lint rules；
-   multi-language lightweight analysis；
-   structural rewrite。

结论：适合作为未来 StructuralRule Backend，而不是 ERF 自己实现多语言
parser。

## Semgrep

可借鉴：

-   rule-as-code；
-   pattern composition；
-   validate/test workflow；
-   CI rule testing；
-   pattern / taint 等不同能力层级。

结论：Backend capability 必须显式建模，不能假设所有规则引擎语义等价。

## Error Prone

其新 check 准入强调可理解、可修复、低误报和 correctness value。

结论：规则"可运行"与"可 blocking"必须是两个不同成熟度阶段。

## openHAB static-code-analysis

公开实践同时整合
PMD、SpotBugs、Checkstyle，自定义规则、单元测试和汇总报告。

结论：统一治理多个原生 Backend 是现实路径，不需要发明万能分析引擎。

## OpenCodeReview

可借鉴：

-   确定性的文件选择、规则路由和任务分组；
-   分组失败时不丢覆盖的回退机制；
-   Coverage Manifest 的固定分母与互斥终态不变式；
-   行级定位、结果结构化和 shape-only telemetry。

结论：吸收上述能力并在 ERF 内定义自有契约与实现，不调用 OpenCodeReview
CLI/MCP，不嵌入、fork 或分发其代码。ERF 自有 AI authoring 的输出仍统一为
AuthoringSuggestion，不得进入生产 Gate 裁决或充当 Verification Evidence。

## zvec

可借鉴：

-   Standard/Scenario/RuleIntent 的混合检索与重复资产发现；
-   Verification near-miss 候选样本检索；
-   Feedback/False Positive 聚类与解释资产检索。

结论：ERF 自行定义 Authoring Retrieval 的混合检索、权限过滤、来源追溯与
index generation 契约，不集成 zvec 库、进程、CLI 或 MCP。该能力始终在
Authoring 外围，不得进入 `rf-runner`、Completeness、Baseline、签名或 Gate 链。
当前语料规模先以 ERF 自有 FTS/BM25 baseline 验证需求，未证明语义检索价值前不
扩建为 MVP 能力。详细评估见
[`docs/research/alibaba-open-code-review-zvec-assessment.md`](research/alibaba-open-code-review-zvec-assessment.md)。

------------------------------------------------------------------------

# 31. MVP

## 31.1 MVP Goal

验证三件事：

1.  非专家是否能显著降低低/中复杂度规则生产成本；
2.  Verification 是否能把 Candidate 变成可信 Rule Asset；
3.  CI 是否能在不破坏历史债务治理的前提下可靠阻止新增违规。

## 31.2 MVP Scope

-   Java；
-   100 条规范 inventory；
-   20 条候选规则；
-   PMD primary；
-   SpotBugs 仅选择 1 个代表性 native plugin 闭环；
-   3 类 typed template；
-   RuleIntent；
-   V1 Rule IR；
-   Verification Lab；
-   Evidence Bundle；
-   signed RuleRelease；
-   CLI；
-   3 个 representative repos；
-   Core Ontology v0、Java/Backend 最小 Extension 和首批 Scenario Projection；
-   Fake Node.js Host/Resolver（仅协议验证，不承担真实 JS 规则）；
-   Shadow；
-   trusted baseline；
-   tri-state Gate；
-   feedback；
-   rollback / revoke。

三类 Template 不预先以架构完整性决定，而是由 Phase 0 的 20 个候选 Scenario
和 Coverage Gap 实证选择，最多三类。OpenCodeReview 和 zvec 只是设计研究
参考；MVP 不集成、嵌入、fork 或分发二者。

## 31.3 Go / No-Go at Week 4

必须在大平台建设前回答：

> Intent → IR → PMD Artifact → Tests
> 是否显著降低规则开发成本并保持语义质量？

若答案是否定，暂停扩建 Registry/Gate 平台，先调整 Factory Kernel。

Go / No-Go 不由规则数量或 reviewer 即兴判断。W1 在编写实现前预注册混合
成功契约：

-   效率：低/中复杂度规则的 Author Active Time 相对 Pilot Metrics Baseline
    达到预注册改善阈值；
-   质量：独立 holdout/near-miss 上的语义质量不劣于人工手写对照；
-   安全：failure injection、伪造结果和执行不完整情况下为零假 PASS。

阈值、样本分层、对照方法和判定人必须在 W1 锁定，不得根据实验结果事后
调整。

------------------------------------------------------------------------

# 32. 12-Week Implementation Plan

## Phase 0 --- Discovery / W1-W2

交付：

-   100 条 mandatory standards inventory；
-   当前 PMD/SpotBugs/CI 配置盘点；
-   Coverage Ledger；
-   3 个 representative repos；
-   20 个候选 Scenario；
-   Pilot Metrics Baseline；
-   预注册 Pilot Success Contract；
-   基于候选 Scenario 的最多三类 Template 选择证据；
-   threat model。

Exit：

-   20 个候选场景完成分类；
-   能明确哪些是 configuration gap、environment gap、new rule。
-   Pilot Success Contract 已锁定，未使用实验结果反推阈值。

## Phase 1 --- Contract + Factory Kernel / W3-W4

交付：

-   Core Ontology v0、Java/Backend 最小 Extension、OntologyContractPack compiler；
-   Backend/Workspace Protobuf contracts；
-   reference fake Host、fake Workspace Resolver 与 Contract Conformance Suite skeleton；
-   RuleIntent schema；
-   Rule IR v1；
-   3 类 template；
-   Java Backend Host + PMD adapter；
-   Candidate compiler；
-   local verification；
-   CLI authoring path。
-   一条规则、一个代表仓库的最薄端到端骨架，包括最小 Bundle、Rust Runner、
    Completeness 与 Gate preview；
-   Fake Node.js Host/Resolver 通过基础 framing、schema 和 capability negotiation；

Exit：

``` text
Standard -> Intent -> IR -> PMD -> Tests
```

完整跑通，并在同一条规则/仓库上连通最小 Runtime 骨架。该 Runtime 只用于
暴露契约断层，Week 4 Go/No-Go 仍只裁决 Factory 假设，不允许藉此提前扩建
通用 Registry/Gate 平台。

执行第一次 Go/No-Go。

## Phase 2 --- Java Local Slice / W5-W6

交付：

-   Java Backend Host + PMD/SpotBugs representative path；
-   SAFE_STATIC Workspace Resolver + Maven/Gradle static Adapter；
-   typed BackendObservation、Finding、Baseline、Exception 与 local Gate preview；
-   independent acceptance suite；
-   holdout；
-   near-miss；
-   Evidence Bundle；
-   compatibility matrix；
-   representative repo shadow；
-   sandbox contract tests 与 failure injection harness；
-   repeatability test。

Exit：

-   至少 5 条规则达到 VERIFIED；
-   Evidence 可以由非作者 reviewer 独立复核。
-   所有结果仍为 LOCAL_UNATTESTED/SHADOW，不得阻断生产分支。

## Phase 3 --- Trusted CI Slice / W7-W9

交付：

-   OCI Runtime/Resolver Packs + linux/amd64 Sandbox Provider；
-   RunnerBootstrap、RunnerRelease、ExecutionLock 与 TrustMetadata；
-   signed RuleRelease/Policy/Baseline/ExceptionGrant；
-   GateAttestation、ArtifactStatement、ConformanceEvidence；
-   debt/comparison baseline 与 bounded staging；
-   trusted CI integration 与 independent Status Publisher；

Exit：

-   同一 commit 重复 10 次 SemanticResultDigest 一致；
-   篡改 Bundle、Pack、ExecutionLock、本体或 trust metadata 被拒绝；
-   Host crash、timeout、漏报单元都产生 INCONCLUSIVE；
-   GateAttestation 可被独立 verifier 验证。

## Phase 4 --- Governance + Pilot / W10-W12

交付：

-   RuleRelease Registry 与 immutable lifecycle；
-   Policy Profile、ExceptionGrant、promotion/revoke/rollback；
-   unified result、tri-state gate 与完整 audit；
-   feedback；
-   dashboards/report；
-   operational runbook。

Exit：

-   3 个 repo Shadow；

-   ≥10 条规则达到 pilot qualified；

-   failure injection 不产生假 PASS；

-   新违规可以稳定区分于历史债务；
-   Fake Node.js Host/Resolver 和 Java path 通过同一 Conformance Suite；
-   OntologyContractPack 可从固定 revision 确定性重建；
-   默认不上传源码或完整 Finding；
-   只有 linux/amd64 Trusted CI 结果可以进入 protected branch。

------------------------------------------------------------------------

# 33. Team

建议：

-   1 Tech Lead / Architect；
-   2 Platform / Static Analysis Engineers；
-   1 Product/Full-stack or Developer Productivity Engineer；
-   0.5 DevSecOps/Security shared；
-   Standard Owner / Reviewer 由业务团队兼职参与。

12 周约 48 core person-weeks + shared security/DevOps capacity。

------------------------------------------------------------------------

# 34. Acceptance Criteria

必须覆盖：

1.  非专家通过模板完成支持场景；
2.  歧义规范被要求 clarify，而不是猜测；
3.  FQN/static import/同名类型等语义正确；
4.  PMD/SpotBugs artifact 可加载、Finding location 正确；
5.  缺 dependency/stale bytecode → INCONCLUSIVE；
6.  dependency change 导致未改行出现新问题仍可发现；
7.  move/copy fingerprint 行为可解释；
8.  rule upgrade 与历史债务分离；
9.  PR 修改 gate config 无法绕过 trusted policy；
10. timeout/crash/empty report 不产生 green；
11. tampered artifact 被拒绝；
12. FP 可转 regression + 新版本；
13. revoke/emergency override 可审计；
14. 固定 snapshot 重复扫描结果稳定；
15. forged local result / stale head 被拒绝；
16. 删除规则、exclude module 不被误判为 debt reduction。
17. 同一输入重复 10 次产生相同 SemanticResultDigest；
18. Host crash、timeout、漏报单元和 stale metadata 均不能产生 PASS；
19. tampered Bundle、Pack、ExecutionLock、本体和 Runner 被拒绝；
20. 过期或回滚 TrustCheckpoint 被拒绝；
21. PMD 规则完整跑通 Finding、Baseline、Exception、GateAttestation；
22. Status Publisher 可独立验证并发布三态结果；
23. Fake Node.js Host/Resolver 通过共享协议合规套件；
24. OntologyContractPack 可确定性重建；
25. 默认不上传源码或完整 Finding。

------------------------------------------------------------------------

# 35. Metrics

## Product

-   Standard Scenario Coverage；
-   Qualified Coverage；
-   Time-to-Rule；
-   Author Active Time；
-   Shadow-to-Advisory conversion；
-   Advisory-to-Blocking conversion。

## Quality

-   Precision / Recall（仅对有标签数据的适用范围）；
-   FP rate；
-   FN sampled rate；
-   Complete Rate；
-   Scenario Coverage；
-   repeatability。

## Operations

-   INCONCLUSIVE rate；
-   scan latency；
-   resource consumption；
-   rollback frequency；
-   exception count / age；
-   revoked release count。

## Business

-   new violation trend；
-   historical debt trend；
-   developer remediation time；
-   review workload；
-   cost per qualified scenario。

禁止以"生成规则数量"作为主要成功指标。

------------------------------------------------------------------------

# 36. Key ADRs

### ADR-001 --- Reuse analysis engines

不自研静态分析引擎。

### ADR-002 --- Intent / Implementation separation

语义身份与 Backend 实现身份解耦。

### ADR-003 --- Typed finite IR

不设计万能 DSL。

### ADR-004 --- AI is authoring-only

LLM 不参与生产 Gate 判断。

### ADR-005 --- Evidence before promotion

Candidate 不能直接 Blocking。

### ADR-006 --- Tri-state gate

失败或不完整不得转成"0 告警 PASS"。

### ADR-007 --- Baseline separation

Debt、Comparison、Quality benchmark 三类基线独立。

### ADR-008 --- Immutable releases

发布资产不可变，变化即新版本。

### ADR-009 --- Central governance, distributed execution

源代码默认留在企业授权执行域。

### ADR-010 --- CLI/Git first

PoC 不以微服务化为成功标准。

### ADR-011 --- Java modular monolith for V0.x

已被 ADR-027 取代。

### ADR-012 --- Git is authoritative during the pilot

Pilot 治理元数据以 Git PR 为权威写入流程，不与 Server DB 双写。

### ADR-013 --- Organization-managed signing identities

Release、Policy 和 Baseline 使用隔离的组织签名身份，Author/Worker 无签名权。

### ADR-014 --- Signed immutable Debt Baselines

Debt Baseline 是中央批准、签名且不可变的治理资产。

### ADR-015 --- Strict three-state Gate

Degraded coverage 是 PASS 元数据，不是第四种 GateDecision。

### ADR-016 --- Minimal Runner upload by default

Runner 默认仅上传身份、digest、Completeness、Gate Evidence 与聚合指标。

### ADR-017 --- Absorb capabilities without product dependencies

OpenCodeReview 和 zvec 仅作为设计参考，相应能力由 ERF 自有契约和实现承载。

### ADR-018 --- Lifecycle-aligned modules

模块按 RuleOps 能力划分，跨模块通过显式契约交互。

### ADR-019 --- Frozen ScanPlan denominator

Completeness 以执行前冻结并计算 digest 的 ScanPlan 为权威分母。

### ADR-020 --- Gate computation and authority are separate

各环境共用同一 Gate Engine，但只有可信 TrustedContext 能产生权威决策。

### ADR-021 --- Evidence-governed maturity transitions

Framework 固定最低证据，Policy 可追加风险门槛，晋级使用显式审计命令。

### ADR-022 --- Resolve Policy before distribution

Policy 冲突必须显式解决，发布前冻结为已签名 Resolved Policy Manifest。

### ADR-023 --- Authoritative ontology compiled for runtime

ERF Ontology 是 Control Plane 的权威语义模型，Execution Plane 只消费由固定修订编译并签名的封闭契约。

### ADR-024 --- Core Ontology and governed extensions

中央治理 Core Ontology，语言、Backend 和企业领域以分层 Extension Ontology 扩展。

### ADR-025 --- Approved Semantic Projections

Standard 保存规范意图，经批准 Semantic Projection 作为机器执行的权威语义表达；冲突会阻止发布。

### ADR-026 --- RDF, OWL 2 RL and SHACL Core

RDF 承载权威图与 IRI，受限 OWL 2 RL 只用于 Control Plane 推理，SHACL Core 用于发布前封闭验证。

### ADR-027 --- Rust Runtime with process-isolated Backend Hosts

`rf/rf-runner` 使用 Rust，分析引擎通过跨进程 Backend Host Interface 接入；首个 Java Host 托管 PMD/SpotBugs。

### ADR-028 --- Canonicalize RDF before signing

Ontology Revision 固定 context/import 后使用 RDFC-1.0 规范化为 canonical N-Quads，并对摘要签名；失败时禁止发布。

### ADR-029 --- Version ontology identity and compatibility

Ontology 同时具有稳定 ontology IRI、不可变 version IRI 与内容摘要，并把变化分类为 additive、constraint-tightening、behavioral 或 breaking。

### ADR-030 --- Framed Protobuf over stdio

Rust Runtime 与 Backend Host 使用 length-delimited Protobuf over stdin/stdout，stderr 只承载结构化日志。

### ADR-031 --- Signed Backend Runtime Packs

Host、分析引擎和必要运行配置以独立的签名、不可变、内容寻址 BackendRuntimePack 发布，由 EngineLock 引用。

### ADR-032 --- Split parsing from governance normalization

Host 产出 policy-neutral BackendObservation；Rust Runtime 统一完成治理归一化、fingerprint、Baseline 和 Completeness。

### ADR-033 --- Jena-centered ontology toolchain

Control Plane 使用 Apache Jena 主工具链和受限 OWL 2 RL 规则集；RDFC-1.0 canonicalization 隐藏在独立、通过标准测试向量的深模块之后。

### ADR-034 --- Compile an Ontology Contract Pack

已批准 Ontology Revision 编译为签名的 deterministic Protobuf OntologyContractPack；Rust Runtime 不加载语义网工具链。

### ADR-035 --- Scan-scoped Backend Host processes

每个 ScanExecution 与 BackendRuntimePack 组合启动一个 Host，扫描内复用、扫描后销毁，V1 不跨扫描池化。

### ADR-036 --- Global ScanPlan with Host refinement

Rust 冻结全局 ScanPlan，Host 仅用 BackendExecutionPlan 细化执行，不能改变 Completeness 分母。

### ADR-037 --- Hermetic trusted runtime closure

可信执行使用全部由 digest 锁定和签名的 Host、engine 与 Runtime Dependency Pack 闭包；系统运行时只能产生本地非证明结果。

### ADR-038 --- Versioned semantic Finding fingerprints

指纹基于 RuleIntent、Scenario、规范化路径和 Semantic Anchor，排除发布版本、严重性和消息；不确定重映射产生 AMBIGUOUS。

### ADR-039 --- Default-deny Backend Host sandbox

Host 默认无网络、凭证和工作区写权限，只获得只读输入、隔离临时目录及显式资源配额；可信环境无法建立沙箱时返回 INCONCLUSIVE。

### ADR-040 --- Major-versioned additive Host protocol

major 不兼容即拒绝，同一 major 只允许 additive Protobuf evolution，并通过跨语言双向兼容测试保护。

### ADR-041 --- Typed observation locations

BackendObservation 使用绑定 blob digest 的 typed location union，源码规范坐标为 UTF-8 byte range，展示行列由 Runtime 推导。

### ADR-042 --- Git-authoritative ontology with rebuildable indexes

Git revision 是 Ontology 权威源；Jena Dataset/TDB 只作为绑定源摘要且可完整重建的查询 read model。

### ADR-043 --- Reference and embed Ontology Contract Packs

RuleBundle 引用独立签名 OntologyContractPack 的精确摘要，并在标准及离线 Bundle 中物理内嵌以支持原子离线验证。

### ADR-044 --- Resolve an immutable Workspace Descriptor

独立 Workspace Resolver 通过生态 Adapter 生成不可变 WorkspaceDescriptor；Backend 不自行推断项目结构，默认不执行任意构建脚本。

### ADR-045 --- Static or attested workspace resolution

Workspace resolution 仅允许 SAFE_STATIC 或 ATTESTED_BUILD_EXPORT；构建脚本始终在 Scanner 与 Backend Host 之外执行。

### ADR-046 --- Linux-only OS family for trusted execution in V0.1

V0.1 只在 Linux OS family 认证 CI/Controlled Runner，具体首个架构由 ADR-063 限定为 linux/amd64；其他平台认证前仅产生 LOCAL_UNATTESTED 结果。

### ADR-047 --- Atomically commit successful Host attempts

Runtime 暂存 attempt 输出，仅在校验成功 Completion 后原子提交；瞬时启动/传输失败最多以新进程重试一次。

### ADR-048 --- Exact ontology digest in V0.1

V0.1 要求资产与 OntologyContractPack 精确 digest 匹配；V0.2 仅允许签名证据证明 ADDITIVE revision 兼容。

### ADR-049 --- Separate ontology and runtime-pack signing roles

ontology-signing 与 runtime-pack-signing 使用独立 key identity 和授权策略，并与 RuleRelease、Policy/Bundle、Baseline 签名职责隔离。

### ADR-050 --- Process-isolated Workspace Resolver Packs

Rust 编排和验证 resolution，生态 Adapter 作为签名 WorkspaceResolverPack 独立进程运行，并使用独立 Protobuf schema。

### ADR-051 --- Bind and attest Workspace Descriptors

WorkspaceDescriptor 绑定源码、配置、Resolver、工具链和产物摘要；ATTESTED_BUILD_EXPORT 另需可信 CI workload identity 签名证明。

### ADR-052 --- Separate attempt transcript and semantic result digests

ScanAttempt 以原始 Observation frame transcript 摘要保证传输完整性，Runtime 对排序后的规范结果另算语义摘要。

### ADR-053 --- Offline-root delegated trust metadata

ERF 用离线 Root 委派各签名角色，版本化且有时效的 trust metadata 支持 key/asset 撤回、重叠轮换与离线快照。

### ADR-054 --- Rootless OCI as the first Sandbox Provider

V0.1 首个认证 Linux Sandbox Provider 使用 rootless OCI；Provider Interface 允许未来增加原生实现但不允许裸进程回退。

### ADR-055 --- Resolve one top-level Execution Lock

ExecutionLock 冻结全部 Backend、Runtime Dependency、Workspace Resolver、协议和沙箱资产；EngineLock 只作为 Backend 子记录。

### ADR-056 --- DSSE in-toto Workspace attestations

WorkspaceAttestation 使用 DSSE in-toto Statement 与 ERF Workspace predicate，并由 Policy 允许的 CI workload identity 签名。

### ADR-057 --- Threshold offline Root with operational role keys

生产 Root 使用 2-of-3 离线阈值；日常 delegated role 使用 HSM 中 1-of-N 签名并保留双人发布审批。

### ADR-058 --- Canonical Finding Set v1

semanticResultDigest 对稳定排序、确定性编码且排除非语义字段的 CanonicalFindingSet v1 计算 SHA-256。

### ADR-059 --- OCI Image Layout for executable packs

BackendRuntimePack、WorkspaceResolverPack 和 RuntimeDependencyPack 使用 OCI Image Layout，OCI manifest digest 是 Pack 内容身份。

### ADR-060 --- Bounded append-only attempt staging

Runtime 将 Observation 流写入有界 append-only staging，超限即取消并判定不完整，禁止截断后继续 Gate。

### ADR-061 --- Control Plane-signed Execution Locks

Control Plane 随发布确定性解析并签名 ExecutionLock；可信 Runtime 只验证执行，本地临时解析只能产生 LOCAL_UNATTESTED 结果。

### ADR-062 --- Bind Workspace attestations to CI workload context

WorkspaceAttestation 绑定 workload issuer/audience、仓库、ref、workflow/run、commit/tree、ExecutionLock 与有效期；Air-gap 使用受限专用 key。

### ADR-063 --- Certify linux/amd64 first

Pack 从第一天使用 OCI Image Index 支持多平台描述，但 V0.1 只认证 linux/amd64；其他平台需独立制品和一致性证据。

### ADR-064 --- Major-version Canonical Finding Sets

CanonicalFindingSet 的 schema、排序、编码或字段语义变化升级 major/domain separator，并以双摘要和映射完成迁移。

### ADR-065 --- Zero-retention attempt staging

Attempt staging 默认成功失败均删除且不上传；显式排障只能生成加密、短 TTL、受审计的 DiagnosticBundle。

### ADR-066 --- Persist signed offline trust checkpoints

Runner 持久化最高 trust role 版本并验证签名 checkpoint 的版本与时效；回滚、时钟异常或状态丢失使可信执行 INCONCLUSIVE。

### ADR-067 --- DSSE in-toto Gate attestations

可信 Gate 输出绑定全部输入、CanonicalFindingSet、Completeness 与决策的 DSSE in-toto GateAttestation，由 Runner workload identity 签名。

### ADR-068 --- Signed immutable Runner Releases

rf-runner 作为可信计算基的一部分以 RunnerRelease 发布，ExecutionLock 固定其精确 digest；自编译 Runner 仅产生 LOCAL_UNATTESTED。

### ADR-069 --- One authoritative implementation per scope

Resolved Policy 对每个 Scenario 和适用范围只选择一个权威实现；其他实现只可 SHADOW，不参与 Gate。

### ADR-070 --- ERF-owned contract conformance suite

Backend/Resolver Pack 晋级必须通过固定 suite digest 的跨语言协议、确定性、失败和沙箱合规测试。

### ADR-071 --- Trust metadata plus portable artifact statements

TrustMetadata 保护分发与 freshness；每个发布资产另附 DSSE ArtifactStatement，使脱离原仓库后仍可独立验证。

### ADR-072 --- Independently bootstrap and measure Runner

RunnerBootstrap 在执行前验证 trust chain、RunnerRelease statement 与实际 digest，Gate verifier 交叉检查 RunnerMeasurement；Runner 不可信自证。

### ADR-073 --- Signed immutable Exception Grants

ExceptionGrant 精确绑定规则、场景、仓库和代码范围、理由、审批、工单与有效期，并以独立角色签名和撤回。

### ADR-074 --- No runtime promotion of Shadow implementations

权威实现失败导致 INCONCLUSIVE；SHADOW 不得自动接管，切换必须经过等价验证并发布新 Policy/Bundle/ExecutionLock。

### ADR-075 --- Credential-isolated Status Publisher

Runner 只产出 GateAttestation；独立 Status Publisher 验证后以最小权限 SCM 凭证发布状态，执行进程不持有写凭证。

### ADR-076 --- Independently signed Conformance Evidence

独立 Verification Worker 针对固定 Pack/suite/platform/Runner digest 运行完整套件并签发 ConformanceEvidence，Builder 不得自证。

### ADR-077 --- Independently managed Runner Bootstrap

RunnerBootstrap 由企业 CI/OS/终端管理独立签名分发，只验证顺序 Root 轮换和 RunnerRelease；Runner 无权更新它或 trust state。

### ADR-078 --- Closed Exception selector grammar

ExceptionGrant 使用必填 repository/RuleIntent/Scenario 与受限 path glob、规范化 symbol、可选精确 fingerprint 的 AND selector，禁止脚本和任意 regex。

### ADR-079 --- Strict tri-state SCM status mapping

Status Publisher 将 PASS 映射 success、FAIL 映射 failure、INCONCLUSIVE 映射 error/action-required，并以证明 subject 幂等且防止旧结果覆盖。

### ADR-080 --- Minimal-disclosure Gate attestations

GateAttestation 默认只携带摘要、身份、完整性、决策和允许的聚合计数；完整 Finding 是独立隐私受控资产。

### ADR-081 --- Revalidate changed conformance combinations

Pack、suite、Runner、Sandbox 或 platform digest 变化都需 Verification Worker 签发新聚合 ConformanceEvidence。

### ADR-082 --- Orthogonal Baseline and Exception decisions

先计算 BaselineState，再独立计算 ExceptionDecision；Exception 只影响 Gate，不删除或改写 Finding 与 Baseline。

### ADR-083 --- Ephemeral scoped Gate signing

Connected CI 使用 workload identity 短期签名能力，Air-gap 使用受限 signer/HSM；Runner 永不持有可导出的长期私钥。

### ADR-084 --- Four vertical delivery slices

12 周按 Contract/Factory、Java Local、Trusted CI、Governance/Pilot 四个纵向切片交付，不先横向建设完整平台。

### ADR-085 --- No trusted Gate before the trusted slice

Contract 和 Java Local Slice 的结果只能是 LOCAL_UNATTESTED/SHADOW；Trusted CI Slice 通过验收后才允许 protected branch 阻断。

### ADR-086 --- Fake Node contract proof before real JavaScript

V0.1 用 Fake Node.js Host/Resolver 验证语言中立契约，真实 JavaScript Backend 延后到 Java pilot 稳定后。

### ADR-087 --- Minimal ontology for the pilot

Pilot 只建设 Core Ontology v0、Java/Backend Extension 与首批 Scenario Projection，禁止预建通用企业知识图谱。

### ADR-088 --- Freeze protocol 1.0 at the trusted pilot

开发阶段协议使用 0.x 并允许显式迁移；第一个 Trusted CI pilot 通过 Conformance Suite 后冻结 1.0。

### ADR-089 --- Deterministic trusted-pilot acceptance

Trusted pilot 以重复语义摘要、篡改拒绝、失败不假绿、跨语言 Fake Host、OntologyContractPack 重建和隐私默认值作为预注册验收。

------------------------------------------------------------------------

# 37. Risks

  Risk                      Mitigation
  ------------------------- --------------------------------------
  规范本身模糊              Scenario + Clarification
  Examples 被误当完整语义   Intent/Scope/Capabilities 必填
  IR 过度抽象               有限 typed IR + Native escape hatch
  Backend 语义差异          CapabilityDescriptor
  AI 幻觉                   Candidate + independent verification
  FP 导致开发者失去信任     Shadow / Advisory / maturity
  历史债务使 CI 全红        Trusted Debt Baseline
  扫描失败假绿              Completeness + INCONCLUSIVE
  Rule supply-chain         signature/SBOM/provenance/revoke
  PoC 过度平台化            Week-4 Go/No-Go
  KPI 驱动无效规则生产      Coverage/Quality/Outcome metrics

------------------------------------------------------------------------

# 38. Post-Pilot Evolution

通过 Pilot 后再评估：

### Quarter +1

-   Web authoring/review；
-   ast-grep adapter；
-   Semgrep adapter；
-   richer dashboard；
-   multi-team tenancy；
-   policy inheritance。

### Quarter +2

-   Error Prone / ArchUnit；
-   more languages；
-   IDE integration；
-   enterprise SSO/RBAC；
-   service API；
-   advanced analytics。

只有真实需求证明必要时，才考虑：

-   CodeQL/Infer；
-   complex dataflow IR；
-   cross-repo architecture rules；
-   auto-remediation workflow。

------------------------------------------------------------------------

# 39. Final Architecture Decision

项目应以 **RuleOps 生命周期平台** 启动，而不是以"AI Rule
Generator"启动。

最小核心链路是：

``` text
Standard
  -> Scenario
  -> Coverage
  -> RuleIntent
  -> Rule IR
  -> Backend
  -> Verification
  -> Evidence
  -> Release
  -> Scan
  -> Completeness
  -> Baseline
  -> Gate
  -> Feedback
```

只要这条链的身份、证据和状态机稳定，后续增加 AI、Web、更多语言和更多
Backend 都属于增量扩展，而不需要重新设计核心。

------------------------------------------------------------------------

# Appendix A --- Research References

-   PMD: https://github.com/pmd/pmd
-   PMD Rule Designer: https://github.com/pmd/pmd-designer
-   ast-grep: https://github.com/ast-grep/ast-grep
-   Semgrep: https://github.com/semgrep/semgrep
-   Semgrep Rules: https://github.com/semgrep/semgrep-rules
-   Error Prone --- Criteria for new checks:
    https://github.com/google/error-prone/wiki/Criteria-for-new-checks
-   openHAB static-code-analysis:
    https://github.com/openhab/static-code-analysis
-   in-toto Attestation Envelope / DSSE:
    https://github.com/in-toto/attestation/blob/main/spec/v1/envelope.md
-   OCI Image Layout:
    https://github.com/opencontainers/image-spec/blob/main/image-layout.md
-   The Update Framework Specification:
    https://theupdateframework.github.io/specification/draft/

------------------------------------------------------------------------

# Appendix B --- Relationship to Amber

本项目**独立于 Amber**。

Amber 仅作为架构思考参考之一。本项目：

-   不依赖 Amber Protocol；
-   不复用 Amber runtime；
-   不共享 CLI；
-   不共享 storage；
-   不共享 release lifecycle；
-   不要求部署 Amber；
-   可独立开发、部署、升级和商业化。

本文档中的 RuleIntent、RuleRelease、EvidenceBundle、GateDecision
等均属于 Enterprise Rule Factory 自身协议。

------------------------------------------------------------------------

# 40. Deployment & Runtime Architecture

ERF 的正式运行形态定义为：

> **Server-side RuleOps Control Plane + Distributed Local/CI Execution
> Runtime + Thin IDE/CLI Clients**

它不是"所有能力都在服务端"的 SaaS，也不是"把 Rule Factory
全部安装到开发者电脑"的本地工具。

``` text
                       +--------------------------+
                       |      RuleOps Server      |
                       |                          |
                       | Standards / Coverage     |
                       | Rule Factory             |
                       | Verification             |
                       | Registry / Policy        |
                       | Feedback / Exception     |
                       | Audit / Analytics        |
                       +------------+-------------+
                                    |
                         HTTPS / Bundle Protocol
                                    |
                         Signed Immutable Assets
                                    |
            +-----------------------+-----------------------+
            |                       |                       |
            v                       v                       v
    Developer Workstation        CI Agent             Air-gap Mirror
            |                       |                       |
       IDE Plugin                   |                       |
            |                       |                       |
          rf CLI                  rf CI                   Cache
            |                       |                       |
            +--------------+--------+-----------------------+
                           |
                           v
                    +-------------+
                    |  rf-runner  |
                    |             |
                    | Resolver    |
                    | Workspace   |
                    | Planner     |
                    | Host Client |
                    | Normalizer  |
                    | Complete    |
                    | Baseline    |
                    | Gate        |
                    +------+------+
                           |
                           |
                 Backend Host Interface
              +------------+------------+
              v                         v
       Java Backend Host          Future Hosts
       +-- PMD                     +-- JavaScript
       `-- SpotBugs                `-- Other
              \                         /
               +-----------+-----------+
                           v
                      Local Source
```

## 40.1 RuleOps Server

服务端属于 Control Plane，负责：

-   Standards Catalog；
-   Scenario / Coverage；
-   Rule Authoring；
-   AI Authoring；
-   Factory Jobs；
-   Verification；
-   Human Review；
-   Registry；
-   Policy；
-   Release Signing；
-   Revocation；
-   Feedback；
-   Exception；
-   Audit；
-   Analytics。

服务端默认不承担业务仓库日常源码扫描。

## 40.2 rf-runner

`rf-runner` 是统一的 Execution Runtime，可运行于：

-   Developer Workstation；
-   CI Agent；
-   企业构建机；
-   内网 Runner；
-   Air-gapped Environment。

内部包含：

``` text
rf-runner
|
+-- Bundle Resolver
+-- Policy Resolver
+-- Workspace Resolver
+-- Scan Planner
+-- Backend Host Client
+-- Backend Process Supervisor
+-- Result Normalizer
+-- Completeness Engine
+-- Baseline Engine
`-- Gate Engine
```

`rf` 和 `rf-runner` 使用 Rust 实现。IDE、CLI 和 CI 不各自实现一套扫描逻辑，而是
复用同一 Runtime 核心。

可信启动由与 RunnerRelease 分离、功能最小的 `RunnerBootstrap` 完成。它持有初始组织
Root trust anchor，验证 TrustMetadata chain、RunnerRelease ArtifactStatement 与实际
binary/OCI manifest digest，随后以只读 `RunnerMeasurement` 启动 Runtime。Bootstrap 不解析
规则、不计算 Finding/Gate；Runner 自报 digest 不能替代该测量。未经过 Bootstrap 或测量
不匹配的执行只能产生 `LOCAL_UNATTESTED`。

RunnerBootstrap 不由 rf/rf-runner 自更新。它通过企业 CI base image、OS package manager
或 endpoint management 独立签名分发，内置初始 Root trust anchor，并只接受按版本连续、
满足阈值的 Root rotation。Runner 进程无权写 Bootstrap binary、anchor 或 highest-seen trust
state；Bootstrap 升级失败不能通过启动未测量 Runner 绕过。

可信 Runtime 本身以签名、不可变 `RunnerRelease` 发布，包含 rf/rf-runner binary digest、
SBOM、provenance、协议/schema、CanonicalFindingSet、Sandbox Provider 与平台兼容性。
ExecutionLock 固定精确 RunnerRelease digest；本地源码构建或未签名二进制可以用于开发，
但只能产生 `LOCAL_UNATTESTED` 结果。

Backend Host 是一个深模块：其对外 Interface 只暴露 capability 描述、执行计划、
执行请求、结构化观测与完整性事实，引擎加载、classloader/VM、报告解析和
引擎特有错误都封装在 Host 内。Rust Runtime 通过 Backend Process Supervisor 启动
经摘要验证的 BackendRuntimePack，并使用 11.1 的 framed Protobuf 契约调用它。

V1 对每个 `ScanExecution × BackendRuntimePack digest` 启动一个 Host 进程。Host 可在
本次扫描内复用引擎初始化、classpath 和只读缓存，但不得把可观察状态带到下一次
ScanExecution；扫描完成、取消、deadline 或协议失败时由 Supervisor 终止并回收进程。
V1 不建立跨扫描 Host pool。任何未来池化都必须证明状态清理、租户隔离和结果等价性。

每次 Host 启动形成独立 `ScanAttempt`。Runtime 将该 attempt 的 Observation 和
Completeness fact 暂存在隔离 staging area，只有收到并验证成功 Completion 中的
request/attempt identity、observation count 与 digest 后才原子提交。取消、超时、崩溃、
摘要不匹配或缺少 Completion 时，全部部分输出丢弃并记录失败证据。

Completion 中的 `attemptTranscriptDigest` 对实际收到、保持顺序的 length-delimited
Observation frame bytes 计算，Host 与 Runtime 必须独立得到相同值；它只证明本次协议
transcript 完整，不作为 Finding 身份。Runtime 验证 Observation 后按稳定 canonical key
排序规范化 Finding，再计算 `semanticResultDigest`。并行度或引擎输出顺序变化不得改变
semanticResultDigest。

Runtime 不在内存中无界缓存 attempt。每个 frame 经尺寸与 schema 验证后写入 sandbox 内
append-only staging file，并同步推进 transcript digest。Deployment Policy 必须限制
单帧大小、Observation 数量、staging 总字节和磁盘配额；达到上限时 Runtime 取消请求、
终止 Host、丢弃 staging 并将对应执行单元标为不完整，禁止截断 Observation 后继续 Gate。

Staging 使用 Runner-only 权限和隔离临时卷，默认无论成功或失败都在提交/终止后立即删除，
且不进入 Runner upload。常规 ExecutionEvidence 只保留 digest、结构化错误和资源指标。
仅当 Deployment Policy 显式启用诊断时，Runtime 才可生成组织密钥加密、带短 TTL、访问
审计和最小字段集的 `DiagnosticBundle`；不得把保留 staging 作为默认 debug 开关。

成功 Completion 验证后，Runtime 从 staging 读取 Observation，生成
`CanonicalFindingSet v1`：只包含 RuleIntent/Scenario、规范化位置与 Semantic Anchor、
Finding 语义、Backend/Rule identity 和 Baseline 所需字段，排除 timestamp、duration、
log order、attempt identity 与 Runtime metrics。记录按稳定 key 排序，以 deterministic
Protobuf length-delimited encoding 加版本化 domain separator，计算 SHA-256
`semanticResultDigest` 后原子提交。

CanonicalFindingSet 的 major version 与 domain separator 是摘要身份的一部分。schema、
排序规则、编码或字段语义的任何变化都必须升级 major；additive 字段也只有在明确排除于
旧摘要或升级版本后才能发布。迁移窗口可以为同一结果并行计算 old/new digest，并发布
Baseline 与 fingerprint 映射；禁止在同一版本下静默改变规范化字节。

只有进程启动或协议传输等瞬时基础设施失败可用全新 Host 进程最多重试一次；输入错误、
`UNSUPPORTED`、资源上限、确定性引擎错误和 observation validation error 不重试。
ExecutionEvidence 必须记录全部 attempt，但 Finding 只能来自最终成功提交的 attempt。

Backend Process Supervisor 必须用平台 Adapter 建立 default-deny sandbox：RepositorySnapshot、
WorkspaceDescriptor 和构建输出只读；仅隔离 temp/cache 可写；默认无网络、无环境凭证、
无宿主用户配置，并限制 CPU、内存、wall time、子进程数、打开文件和协议输出量。

子进程能力必须由 BackendRuntimePack manifest 声明并由 Deployment Policy 显式允许。
可信 CI 或 Controlled Runner 无法建立所需沙箱时不得降级裸跑，ScanExecution 必须进入
`INCONCLUSIVE`；Developer Profile 的显式降级仍只能产生 `LOCAL_UNATTESTED` 结果。

V0.1 只把 Linux Sandbox Provider 纳入可信执行认证范围。macOS 和 Windows 可以运行
相同 CLI/Runtime 获取开发反馈，但在各自 Provider 通过文件系统、网络、凭证、资源限制、
进程树清理和跨平台结果一致性测试前，只能产生 `LOCAL_UNATTESTED` 结果，不能作为
protected branch 的权威证明。

V0.1 首个认证实现为 rootless OCI Sandbox Provider：只读 root filesystem、只读输入
mount、隔离 writable temp、network namespace disabled、全部 capabilities dropped、
`no-new-privileges`、seccomp profile 和 cgroup CPU/memory/process 配额。Provider 必须
输出自身 identity、policy digest 和 enforcement evidence；OCI 不可用时可信执行失败，
不得回退为普通子进程。未来原生 Linux Provider 必须通过同一 conformance suite。

V0.1 可信平台进一步限定为 `linux/amd64`。Pack 必须从第一天使用 OCI Image Index 与
platform descriptor，允许未来加入 `linux/arm64`；但 arm64 只有在提供独立 executable/
RuntimeDependencyPack manifest、Sandbox Provider 证据和跨架构语义一致性测试后才能
进入可信矩阵。

## 40.3 Thin Clients

### CLI

用于：

-   authoring；
-   local scan；
-   explain；
-   feedback；
-   CI integration。

### IDE Plugin

IntelliJ / VS Code Plugin 只作为 UX Client：

``` text
IDE Plugin
   |
   +-- invoke rf-runner
   +-- show finding
   +-- navigate source
   +-- explain
   +-- report false positive
   `-- request exception
```

IDE Plugin 不维护独立 PMD/SpotBugs runtime。

------------------------------------------------------------------------

# 41. Source-Code Locality

ERF 默认采用：

> **Rules go to code; code does not go to the RuleOps Server.**

正常扫描数据流：

``` text
RuleOps Server
      |
 Rule / Policy /
 Signed Bundle
      |
      v
   rf-runner <----- Local Repository
      |
      v
PMD / SpotBugs
      |
      v
Findings + Completeness
      |
      +----> Local Report
      |
      `----> Summary / Evidence / Metrics -> Server
```

服务端原则上只接收治理所需的结果、Evidence 和聚合信息。

如果企业未来允许上传源码做中央分析，应作为显式 Deployment
Policy，而不是默认行为。

------------------------------------------------------------------------

# 42. Client / Server Protocol

Server 与 Runner 之间使用版本化协议。

Runner 获取：

``` text
Policy Manifest
Rule Bundle
Execution Lock
Compatibility Metadata
Revocation Metadata
Optional Baseline Metadata
```

Runner 上传：

``` text
Execution Identity
Rule Bundle Digest
Policy Digest
Completeness
Normalized Findings / permitted summary
Gate Evidence
Runtime Metrics
```

默认 Privacy Profile 只允许上传 Execution Identity、digest、Completeness、Gate
Evidence 和聚合 Runtime Metrics。`Normalized Findings` 只有在 Deployment Policy 显式
开启后才可上传；源码片段默认禁止，path/symbol 字段必须通过 allowlist 或脱敏。

协议必须具备：

-   API version；
-   schema validation；
-   backward compatibility policy；
-   content digest；
-   signature verification；
-   idempotency；
-   retry；
-   offline/cache semantics。

源码内容是否允许上传由企业 Policy 单独控制。

------------------------------------------------------------------------

# 43. Bundle Distribution

Rule Bundle 是 Server → Runner 的主要发布单元。

建议格式：

``` text
company-java-policy-2026.09.rfbundle
|
+-- manifest.yaml
+-- policy/
+-- ontology-contract/
+-- rules/
+-- artifacts/
+-- execution-lock/
+-- compatibility/
+-- evidence-refs/
+-- sbom/
+-- provenance/
+-- signatures/
`-- revocation-snapshot/
```

Bundle 必须：

-   immutable；
-   content-addressed；
-   signed；
-   versioned；
-   verifiable offline；
-   可镜像；
-   可缓存。

RuleRelease、Policy/Bundle、Baseline、OntologyContractPack 与 Runtime Pack 签名使用
相互隔离的组织身份，其中新增 `ontology-signing` 和 `runtime-pack-signing` role。
Pilot 可以使用同一 KMS/HSM 基础设施，但必须使用不同 key identity、授权策略和审计流。
manifest 从第一天起区分 signing role 与 trust root；Author、Factory Worker、Ontology
Compiler 和 Pack Builder 只能提交待签资产，不得持有发布签名权限。

所有角色由离线 Root trust metadata 委派。Root 与 delegated metadata 均有递增版本、
有效期和允许的 asset type/namespace；轮换通过新旧 key 的有界重叠完成。签名的
snapshot 同时承载 key revocation 和 asset-digest revocation，记录原因与生效时间。
Runner 只信任满足 Deployment Policy TTL 的完整 metadata chain；链缺失、过期、回滚或
无法确认撤回状态时 fail closed，并使 Gate 为 `INCONCLUSIVE`。

生产 Root metadata 至少使用三个彼此独立的离线保管身份并要求 `2-of-3` 签名。
日常 delegated role metadata 与资产签名接受 HSM 内 `1-of-N` 有效 key，但发布工作流仍
必须执行双人审批。加密签名阈值不替代治理审批，治理审批也不替代 Root 阈值。

每次离线镜像发布签名 `TrustCheckpoint`，包含 repository identity、各 trust role 最高
版本、snapshot digest、issued-at 与 expires-at。Runner 必须在受保护本地状态中持久化
每个 role 的 highest-seen version；checkpoint 降版、可信时钟偏差超出 Policy、状态意外
丢失或 checkpoint 超过 TTL 时均返回 `INCONCLUSIVE`。恢复必须导入更新 checkpoint 或
执行经审计的 trust-state recovery，禁止关闭 expiry/rollback 检查。

TrustMetadata 负责 repository delegation、freshness、rollback 和 revocation；资产本身
同时随附 DSSE `ArtifactStatement`。Statement subject 是资产内容 digest，predicate 绑定
asset type、schema version、signing role、provenance/SBOM digest 与 compatibility identity。
RuleBundle、OntologyContractPack、ExecutionLock、RunnerRelease 和全部 OCI Pack 都必须
具有该可移植证明。离线复制不得剥离 Statement；只有 Statement 与 TrustMetadata chain
均验证通过，资产才可信。

Runner 不依赖服务端实时拼装规则后再扫描。

## 43.1 Ontology Contract Pack

OntologyContractPack 保持独立 asset identity、签名、版本和撤回状态。RuleBundle manifest
必须引用其精确 digest，并证明全部 RuleRelease、Resolved Policy Manifest 与该 Pack
语义兼容。标准发布与 Air-gapped export 默认把 Pack 物理内嵌在 Bundle 中，Runner
仍按独立签名资产验证，禁止用缓存中的“最新本体”替换。

Bundle 缺少所引用 Pack、digest/signature 不匹配或 compatibility validation 失败时，
不得开始扫描。相同 digest 可在本地 cache 去重，不改变其逻辑身份。

## 43.2 Backend Runtime Pack

BackendRuntimePack 是与 RuleBundle 逻辑分离的执行资产，其 OCI payload 逻辑内容为：

``` text
backend-runtime-pack
|
+-- manifest
+-- backend-host
+-- analysis-engine
+-- required-runtime-config
+-- compatibility
+-- sbom
+-- provenance
`-- signatures
```

它必须 immutable、content-addressed、signed、可撤回并可离线验证。`EngineLock`
引用精确的 pack digest 与协议兼容范围；Runner 必须验证摘要、签名、兼容性和
撤回状态后才可启动 Host。CI 不得使用 PATH/classpath 中未锁定的本机引擎替代该资产。

可信执行要求完整依赖闭包可验证。为避免每个 Host 重复携带 JRE、Node 等大体积环境，
BackendRuntimePack 可以按 digest 引用独立签名的 RuntimeDependencyPack；EngineLock
必须冻结整个传递闭包，Supervisor 在启动前验证所有成员。任何未锁定、缺失或被撤回的
成员都使结果 `INCONCLUSIVE`。

Developer Profile 可以显式选择兼容的系统运行时以获得快速反馈，但该执行必须标记
`LOCAL_UNATTESTED`，不得上传为可信 Gate Evidence，也不得静默替代锁定资产。

RuleBundle 不吸收 BackendRuntimePack 的身份或生命周期。Air-gapped export 可以为了
运输方便把二者装入同一物理归档，但验证和审计时仍视为两个独立签名资产。

## 43.3 Workspace Resolver Pack

WorkspaceResolverPack 是由 `runtime-pack-signing` 发布的独立签名执行资产，包含 Resolver
Adapter、Workspace Resolution schema compatibility、运行依赖、SBOM 和 provenance。
可信 resolution 必须按精确 digest 获取并验证 Pack；它不属于 RuleBundle，也不得由
BackendRuntimePack 隐式携带。Air-gapped export 可物理携带所需 Pack 和 trust snapshot。

## 43.4 Execution Lock and Pack Format

顶层 `ExecutionLock` 是签名、确定性解析结果，冻结：

-   每个 Backend 的 EngineLock 与 BackendRuntimePack digest；
-   全部传递 RuntimeDependencyPack digest；
-   WorkspaceResolverPack digest；
-   Backend/Workspace protocol 与 schema version；
-   Sandbox Profile/Provider compatibility 要求。
-   RunnerRelease 与 Contract Conformance Suite digest。

EngineLock 保留为 Backend 专属子记录，不再承担 Workspace Resolver、通用 Runtime 或
沙箱身份。可信执行禁止使用版本范围、“latest”或本机自动发现值补全 ExecutionLock。

ExecutionLock 由 Control Plane 在 Policy/RuleBundle 发布时确定性解析并使用对应发布角色
签名；同一发布输入必须生成同一 lock digest。可信 Runtime 只能验证、缓存和执行，不得在
扫描开始时替换或补解析资产。Developer Profile 可生成临时本地锁用于反馈，但必须标记
`LOCAL_UNATTESTED`，且不能作为 Gate Evidence；任何 Pack、协议、schema 或 Sandbox
Profile identity 变化都会产生新的 ExecutionLock。

BackendRuntimePack、WorkspaceResolverPack 和 RuntimeDependencyPack 的物理格式统一为
OCI Image Layout。OCI manifest 使用 ERF 注册 media type 的 config blob 保存 Pack manifest、
compatibility 和 entrypoint，layers 保存可执行内容；OCI manifest digest 直接作为 Pack
内容身份与签名 subject。离线归档必须保留完整 OCI layout 和 digest，不以 tag 作为身份。
RuleBundle 继续使用 `.rfbundle`，不要求转换为 OCI artifact。

------------------------------------------------------------------------

# 44. Cache & Availability Model

RuleOps Server 不得成为所有 CI 的实时单点依赖。

``` text
RuleOps Server
      |
   publish/sync
      |
      v
 Local / CI Cache
      |
      v
   rf-runner
      |
      v
      CI
```

当 Server 暂时不可用时，只要满足：

``` text
Bundle signature valid
Bundle not expired by policy
Revocation snapshot within allowed TTL
Referenced BackendRuntimePack available and valid
Trusted context valid
```

Runner 可以继续执行。

如果缓存过期、撤回状态无法满足企业 Policy 或关键身份不可验证，则 Gate
应返回：

``` text
INCONCLUSIVE
```

而不是静默使用未知状态的旧资产。

------------------------------------------------------------------------

# 45. Offline / Air-Gapped Mode

ERF 正式支持三种 Deployment Profile。

  -------------------------------------------------------------------------------
  Profile           Control Plane     Execution         Typical Use
  ----------------- ----------------- ----------------- -------------------------
  Connected         中央 RuleOps      Local/CI Runner   默认企业部署
  Enterprise        Server                              

  Private /         企业内网部署或    内网 Local/CI     金融、政企、安全环境
  Air-gapped        Mirror            Runner            

  Standalone        无持续 Server     Runner + exported PoC、本地开发、临时离线
  Developer                           bundle            
  -------------------------------------------------------------------------------

离线使用：

``` text
rf scan --bundle company-java-policy-2026.09.rfbundle
```

离线模式仍验证：

-   signature；
-   manifest；
-   execution lock；
-   compatibility；
-   bundle integrity。

离线结果的 Trust Level 与 CI Trusted Result 必须区分。

------------------------------------------------------------------------

# 46. Runner Trust Model

同一个 `rf-runner` 在不同环境拥有不同 Trust Level。

``` text
LOCAL_UNATTESTED
CI_TRUSTED
CONTROLLED_RUNNER
```

本地扫描可以用于快速反馈，但不能直接作为 protected branch 的最终证明。

Trusted CI 必须注入：

``` text
repository identity
commit SHA
base / merge-base
policy identity
rule bundle digest
ontology contract pack digest
execution lock digest
backend engine lock digests
backend runtime pack digest
backend host/engine digests
workspace descriptor digest
workspace resolver pack digest
workspace attestation identity when required
sandbox provider/policy identity
baseline identity
runner identity
runner release digest
runner measurement digest
```

Gate 必须验证这些身份。

同一 Gate Engine 可在本地、CI 或 Controlled Runner 运行；仅当上述身份构成
可验证 TrustedContext 时，结果才具有生产权威性。本地结果必须标记
`LOCAL_UNATTESTED`，不得升格为 protected branch 证明。

因此开发者即使修改本地 result、rule config 或 baseline，也不能伪造生产
Gate PASS。

SCM 写入由独立薄 `Status Publisher` 完成。它接收 GateAttestation，重新验证签名、
repository/commit、freshness 与所有关键 digest 后，才使用限定到 commit status/check 的
最小权限凭证发布结果。rf-runner、Backend Host 和 Workspace Resolver 均不持有 SCM 写
凭证；Publisher 不能修改 GateDecision，只能拒绝或原样发布已验证证明。

状态映射固定为 `PASS → success`、`FAIL → failure`、
`INCONCLUSIVE → error/action-required`；不得把 INCONCLUSIVE 降为 neutral 或 success。
Publisher 以 `repository + commit + policy + GateAttestation subject` 作为幂等键，并维护
同一执行序列的单调 run identity；旧证明、过期证明或不同 commit 的证明不得覆盖新状态。

------------------------------------------------------------------------

# 47. Factory Worker vs Scan Runner

必须把规则生产执行环境和规则扫描执行环境严格分离。

``` text
Factory Worker                    rf-runner
--------------                    ---------
处理不可信 Candidate               只消费可信 Release
Control Plane side                Repo / CI side
允许构建 Rule Plugin               不生成 Rule Plugin
严格 Sandbox                      Deterministic Runtime
默认无生产凭证                     只持验证所需信任材料
生成 Artifact + Evidence           生成 Finding + Completeness
无 Release Signing 权限            无 Rule Authoring 权限
```

AI 生成的 Native Java Rule、build file、fixture
等均视为不可信输入，必须进入 Factory Worker。

生产 Runner 不执行临时 AI 输出。

------------------------------------------------------------------------

# 48. Developer Runtime Flow

项目只需要薄配置：

``` yaml
apiVersion: rulefactory.io/v1
kind: ProjectConfig

policy:
  profile: company-java-backend
  version: "2026.09"
```

开发者体验：

``` text
Developer
    |
    v
IDE / rf CLI
    |
    v
rf-runner
    |
    +------> RuleOps Server
    |          |
    |     resolve policy
    |     fetch signed bundle
    |          |
    <----------+
    |
    v
Local Repository
    |
    v
Backend Engines
    |
    v
Findings / Explain
```

本地结果标记为 `LOCAL_UNATTESTED`。

------------------------------------------------------------------------

# 49. CI Runtime Flow

``` text
SCM PR / Push
      |
      v
Trusted CI Job
      |
      v
RunnerBootstrap -> rf-runner
   /      \
  v        v
Bundle   Repository Snapshot
  \        /
   v      v
 Backend Engines
      |
      v
Findings + Completeness
      |
      v
Baseline Compare
      |
      v
Trusted Gate
   /    |     \
PASS  FAIL  INCONCLUSIVE
      |
      v
GateAttestation -> Status Publisher -> SCM Status
```

CI 与本地使用同一个 Runner、同一 Rule Bundle、同一 ExecutionLock 和同一
Normalizer，避免环境语义漂移。

------------------------------------------------------------------------

# 50. Runtime Delivery Sequence

运行形态的产品交付顺序调整为：

## V0.1 --- Thin End-to-End Foundation

-   rf CLI；
-   Rust rf-runner；
-   Java Backend Host + PMD Adapter；
-   framed Protobuf Backend Host Contract；
-   signed OCI Runtime Packs + ExecutionLock；
-   deterministic OntologyContractPack；
-   Workspace Resolver + Maven/Gradle Adapter；
-   typed BackendObservation location；
-   default-deny Host sandbox；
-   SAFE_STATIC Workspace resolution；
-   atomic Host attempt staging/commit；
-   linux/amd64 trusted execution profile；
-   signed WorkspaceResolverPack + independent resolution contract；
-   WorkspaceDescriptor envelope/attestation；
-   attempt transcript digest + semantic result digest；
-   delegated trust metadata and revocation snapshot；
-   DSSE in-toto WorkspaceAttestation；
-   CanonicalFindingSet v1；
-   bounded append-only attempt staging；
-   Control Plane-signed ExecutionLock；
-   TrustCheckpoint rollback protection；
-   DSSE GateAttestation + ArtifactStatement；
-   signed RunnerRelease；
-   Contract Conformance Suite；
-   RunnerBootstrap + RunnerMeasurement；
-   signed ExceptionGrant；
-   credential-isolated Status Publisher；
-   closed Exception selector + orthogonal ExceptionDecision；
-   ephemeral Gate signing path；
-   scan-scoped Host lifecycle 与两级执行计划；
-   versioned Finding fingerprint；
-   最小 RuleIntent / IR / Candidate Compiler；
-   最小 Authoring Orchestrator；
-   ID/metadata/FTS Authoring Retrieval baseline；
-   最小 Verification / Evidence；
-   Bundle format；
-   local cache；
-   CI integration；
-   Git/Artifact Repository based Control Plane。

V0.1 必须用一条规则、一个仓库连通作者侧与执行侧的最薄纵切。它不是先建完
Factory 或先建完 Runtime，也不授权在 Week 4 前构建通用 Registry/Gate 平台。

## V0.2 --- RuleOps Server

-   Web/API；
-   Standards/Coverage；
-   Factory Jobs；
-   完整 Verification；
-   Registry；
-   Policy；
-   Feedback/Exception；
-   Audit。

## V0.3 --- Developer UX

-   IntelliJ Plugin；
-   VS Code Extension；
-   richer explain；
-   feedback/exception UX。

IDE Plugin 不进入首个关键路径。

------------------------------------------------------------------------

# 51. Runtime Architecture Decision

最终运行形态正式确定为：

> **RuleOps Server + rf-runner + Thin Clients**

其中：

-   `RuleOps Server` 是治理与生产控制面；
-   Rust `rf-runner` 是本地/CI 的确定性执行核心；
-   `Backend Host` 通过跨进程契约封装特定语言/VM 的分析引擎；
-   `IDE Plugin / CLI` 是交互客户端；
-   `Factory Worker` 是隔离的不可信 Candidate 构建环境；
-   `Signed Rule Bundle` 是控制面与执行面之间的主要发布契约；
-   `Signed BackendRuntimePack` 是 Host/engine 的独立发布契约；
-   服务端故障不应默认导致全公司 CI 停摆；
-   企业源码默认留在本地或 CI 授权域。

这一决策属于 V1 架构基线，不作为后续实现细节留白。
