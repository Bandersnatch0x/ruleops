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
-   engine lock；
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

------------------------------------------------------------------------

# 11. Backend Adapter SPI

``` text
describe()   -> CapabilityDescriptor
validate()   -> ValidationResult
compile()    -> RuleArtifact
verify()     -> BackendVerification
plan()       -> ScanPlan
execute()    -> RawResult
normalize()  -> Finding[]
```

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

Framework 定义必须有哪些 Evidence；企业 Policy 定义晋级阈值。

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

------------------------------------------------------------------------

# 16. Rule Release

RuleRelease 是不可变资产：

``` text
RuleRelease
  |- RuleIntent
  |- RuleImplementation
  |- RuleArtifact
  |- Backend Identity
  |- Engine Lock
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

Profile resolve 后形成不可变 Signed Rule Bundle。

------------------------------------------------------------------------

# 19. Scan Contract

输入：

``` text
RepositorySnapshot
RuleBundle
EngineLock
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

Advisory backend 可由 Policy 允许
`PASS_WITH_DEGRADED_COVERAGE`，但不得伪装为完整 PASS。

------------------------------------------------------------------------

# 24. Exceptions

Exception：

``` yaml
rule: SEC-JAVA-017

scope:
  repository: payment-service
  path: src/main/java/Bootstrap.java

reason: JVM bootstrap requires explicit termination
owner: platform-team
expiresAt: 2026-12-31
approval: ...
```

必须具备：

-   exact scope；
-   owner；
-   reason；
-   expiry；
-   approval；
-   audit。

代码风险 Exception 不得豁免"扫描未执行"本身。

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
│   ├── standard/
│   ├── rule-intent/
│   ├── rule-ir/
│   ├── evidence/
│   ├── result/
│   └── policy/
├── cli/
├── core/
│   ├── domain/
│   ├── coverage/
│   ├── factory/
│   ├── verification/
│   ├── registry/
│   └── gate/
├── adapters/
│   ├── pmd-java/
│   └── spotbugs/
├── templates/
├── runner/
├── sandbox/
├── ci/
├── examples/
└── docs/
```

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
-   Shadow；
-   trusted baseline；
-   tri-state Gate；
-   feedback；
-   rollback / revoke。

## 31.3 Go / No-Go at Week 4

必须在大平台建设前回答：

> Intent → IR → PMD Artifact → Tests
> 是否显著降低规则开发成本并保持语义质量？

若答案是否定，暂停扩建 Registry/Gate 平台，先调整 Factory Kernel。

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
-   threat model。

Exit：

-   20 个候选场景完成分类；
-   能明确哪些是 configuration gap、environment gap、new rule。

## Phase 1 --- Factory Kernel / W3-W4

交付：

-   RuleIntent schema；
-   Rule IR v1；
-   3 类 template；
-   PMD adapter；
-   Candidate compiler；
-   local verification；
-   CLI authoring path。

Exit：

``` text
Standard -> Intent -> IR -> PMD -> Tests
```

完整跑通。

执行第一次 Go/No-Go。

## Phase 2 --- Verification / W5-W6

交付：

-   independent acceptance suite；
-   holdout；
-   near-miss；
-   Evidence Bundle；
-   compatibility matrix；
-   representative repo shadow；
-   sandbox；
-   repeatability test。

Exit：

-   至少 5 条规则达到 VERIFIED；
-   Evidence 可以由非作者 reviewer 独立复核。

## Phase 3 --- Governance / W7-W9

交付：

-   RuleRelease；
-   Registry；
-   signatures；
-   SBOM/provenance；
-   maturity lifecycle；
-   promotion/revoke/rollback；
-   Policy Profile；
-   exception model。

Exit：

-   VERIFIED → SHADOW → ADVISORY 完整闭环；
-   tampered artifact 被拒绝。

## Phase 4 --- Runtime Integration / W10-W12

交付：

-   trusted CI integration；
-   unified result；
-   completeness；
-   debt baseline；
-   comparison baseline；
-   tri-state gate；
-   feedback；
-   dashboards/report；
-   operational runbook。

Exit：

-   3 个 repo Shadow；

-   =10 条规则达到 pilot qualified；

-   failure injection 不产生假 PASS；

-   新违规可以稳定区分于历史债务。

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
                    | Planner     |
                    | Backend SPI |
                    | Normalizer  |
                    | Complete    |
                    | Baseline    |
                    | Gate        |
                    +------+------+
                           |
              +------------+------------+
              v            v            v
             PMD        SpotBugs      Future
              |            |
              +------+-----+
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
+-- Scan Planner
+-- Backend Runtime
|    +-- PMD
|    +-- SpotBugs
|    `-- Future Adapters
+-- Result Normalizer
+-- Completeness Engine
+-- Baseline Engine
`-- Gate Engine
```

IDE、CLI 和 CI 不各自实现一套扫描逻辑，而是复用同一 Runner。

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
Engine Lock
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
+-- rules/
+-- artifacts/
+-- engine-lock/
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

Runner 不依赖服务端实时拼装规则后再扫描。

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
Engine available
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
-   engine lock；
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
engine digest
baseline identity
runner identity
```

Gate 必须验证这些身份。

因此开发者即使修改本地 result、rule config 或 baseline，也不能伪造生产
Gate PASS。

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
rf-runner
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
```

CI 与本地使用同一个 Runner、同一 Rule Bundle、同一 Engine Lock 和同一
Normalizer，避免环境语义漂移。

------------------------------------------------------------------------

# 50. Runtime Delivery Sequence

运行形态的产品交付顺序调整为：

## V0.1 --- Execution Foundation

-   rf CLI；
-   rf-runner；
-   PMD Adapter；
-   Bundle format；
-   local cache；
-   CI integration；
-   Git/Artifact Repository based Control Plane。

## V0.2 --- RuleOps Server

-   Web/API；
-   Standards/Coverage；
-   Factory Jobs；
-   Verification；
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
-   `rf-runner` 是本地/CI 的确定性执行核心；
-   `IDE Plugin / CLI` 是交互客户端；
-   `Factory Worker` 是隔离的不可信 Candidate 构建环境；
-   `Signed Rule Bundle` 是控制面与执行面之间的主要发布契约；
-   服务端故障不应默认导致全公司 CI 停摆；
-   企业源码默认留在本地或 CI 授权域。

这一决策属于 V1 架构基线，不作为后续实现细节留白。
