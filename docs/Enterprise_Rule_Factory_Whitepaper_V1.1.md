# Enterprise Rule Factory 白皮书

## 从企业代码规范到可信、可执行、可治理的 RuleOps

**版本：1.1**\
**日期：2026-09-16**

> 阅读范围（ADR-0174 决策 7）：本白皮书保留设计背景，不作为当前功能交付清单。
> 其中标记为 Defer 的 Pilot 观测/裁决、治理服务面和 IDE 客户端不列入当前公开能力承诺。
> 既有运行工具、内部 runbook、契约、兼容说明和安全检查保留；不取消已登记窗口，不重开既有商定范围。

------------------------------------------------------------------------

## 摘要

企业拥有大量研发规范，但"写在文档里的规范"和"真正持续执行的工程控制"之间存在巨大鸿沟。

传统方案通常落入两个极端：一端依赖人工 Code
Review，规则难以规模化；另一端直接采购或接入多个静态分析工具，却缺少统一的规范身份、覆盖关系、质量证据、发布治理、历史债务和例外机制。

Enterprise Rule Factory（ERF）提出一种不同的路径：

> 不重新发明静态分析器，而是建立企业代码规则的生产、验证、发布、执行和治理体系。

ERF 将规范转化为一种新的企业工程资产：**Rule Asset**。

Rule Asset 不只是一个 PMD XML、一段 Semgrep YAML 或一个 SpotBugs
JAR。它同时包含业务语义、适用范围、实现、验证证据、版本、兼容性、发布身份、签名、运行结果和治理状态。

最终目标是让企业能够回答五个过去很难回答的问题：

1.  我们有哪些规范真正被机器覆盖？
2.  某条规则为什么存在，它准确表达了什么？
3.  为什么我们相信这条规则足够可靠？
4.  某次 CI 到底完成了哪些检查？
5.  当规则误报、升级或失效时，如何安全演进？

------------------------------------------------------------------------

# 1. 为什么需要 RuleOps

代码规范治理表面上是"静态分析"问题，实际上是一个资产生命周期问题。

企业通常已经拥有 PMD、SpotBugs、Checkstyle、Sonar、Semgrep、CodeQL
或内部工具。真正缺失的往往不是另一个 Scanner，而是连接以下环节的治理层：

``` text
规范
  ↓
可检查场景
  ↓
规则实现
  ↓
规则质量
  ↓
发布
  ↓
项目采用
  ↓
CI 执行
  ↓
违规治理
  ↓
反馈与演进
```

如果缺少这条链，会出现：

-   规范很多，但没人知道覆盖率；
-   同一句规范在不同团队有不同实现；
-   规则升级没有影响评估；
-   "0 告警"无法证明扫描成功；
-   新规则让数万条历史债务瞬间阻塞所有项目；
-   误报只能通过关闭规则解决；
-   AI 生成了更多规则，却没有增加企业信任。

RuleOps 的核心不是"更多 Rule"，而是"可信 Rule"。

------------------------------------------------------------------------

# 2. 产品愿景

ERF 的愿景是：

> **把组织研发意图转化为可验证、可发布、可执行、可审计的工程控制。**

它建立一种从 Standard 到 Enforcement 的供应链：

``` text
Organizational Intent
        |
        v
     Standard
        |
        v
     Scenario
        |
        v
     RuleIntent
        |
        v
 RuleImplementation
        |
        v
   Verification
        |
        v
     Evidence
        |
        v
      Release
        |
        v
    Enforcement
        |
        v
     Feedback
```

------------------------------------------------------------------------

# 3. Rule Factory 不是什么

ERF 不是：

-   一个新的静态分析引擎；
-   一个"输入两个例子自动写规则"的 AI Demo；
-   PMD 的图形化配置器；
-   Sonar 的简单替代品；
-   一个只负责在 CI 中返回红绿灯的脚本；
-   一个要求企业一次性迁移所有分析器的平台。

ERF 是现有分析能力之上的 **Rule Engineering & Governance Layer**。

------------------------------------------------------------------------

# 4. Rule Asset：新的企业工程资产

传统规则通常只保存实现：

``` text
rule.xml
rule.yaml
CustomRule.java
Detector.class
```

ERF 保存的是：

``` text
Rule Asset
|
+-- Standard / Scenario
+-- Rule Intent
+-- Scope
+-- Semantic Identity
+-- Implementation
+-- Backend / Engine Lock
+-- Tests
+-- Verification Evidence
+-- Compatibility
+-- Remediation
+-- Provenance
+-- SBOM
+-- Signature
+-- Release State
`-- Audit History
```

因此规则从"配置文件"升级为"受治理的软件供应链资产"。

------------------------------------------------------------------------

# 5. 中央治理，分布式执行

ERF 推荐的企业运营模式是：

> **Central Governance + Distributed Enforcement**

中央 RuleOps Center 负责：

-   规范；
-   规则生产；
-   Verification；
-   Registry；
-   Policy；
-   发布；
-   审计。

业务仓库负责：

-   在自己的 CI 授权域执行；
-   消费统一 Rule Bundle；
-   修复新增问题；
-   管理历史债务；
-   提交反馈和例外。

这种模式既避免每个团队重复造规则，又避免把所有企业源代码上传到中央平台。

------------------------------------------------------------------------

# 6. 先做 Coverage，而不是先生成规则

当出现一条企业规范时，第一个问题不应该是：

> "用什么模型生成 PMD Rule？"

而应该是：

> "我们为什么需要一条新规则？"

ERF 首先进行 Coverage Analysis：

``` text
Standard Scenario
      |
      +-- Existing Rule
      |
      +-- Existing but Disabled
      |
      +-- Configuration Gap
      |
      +-- Environment Gap
      |
      +-- New Rule Required
      |
      +-- Backend Capability Gap
      |
      +-- Standard Ambiguous
      |
      `-- Not Machine Detectable
```

这一步防止 Rule Factory 变成"规则数量工厂"。

------------------------------------------------------------------------

# 7. 从规范到 Scenario

自然语言规范往往不是一个检查。

例如：

> 业务代码禁止直接终止 JVM。

它可能包含：

-   `System.exit()`；
-   FQN 调用；
-   static import；
-   同名自定义类型；
-   `Runtime.halt()`；
-   反射调用；
-   test scope；
-   bootstrap exception。

因此 ERF 把 **Scenario** 定义为最小业务覆盖单位。

企业最终能够看到：

``` text
SEC-JAVA-017

direct-system-exit      COVERED
runtime-halt            PARTIAL
reflective-invocation   UNSUPPORTED
```

这比一个模糊的"该规范已支持"真实得多。

------------------------------------------------------------------------

# 8. RuleIntent 与实现分离

ERF 最核心的架构决策之一是：

> **规则的业务语义，不等于规则的技术实现。**

同一个 RuleIntent 可以存在：

``` text
RuleIntent
   |
   +-- PMD implementation
   |
   +-- SpotBugs implementation
   |
   `-- Semgrep implementation
```

因此企业不会被某一个工具的数据结构绑死。

替换 Backend 不意味着重新定义企业规范。

------------------------------------------------------------------------

# 9. 有限 Rule IR，而不是万能 DSL

为了降低不同 Backend 之间的耦合，ERF 引入 Rule IR。

但 IR 不试图表达所有静态分析。

首期只提供有限类型：

``` text
ExistingRule
StructuralRule
SymbolRule
TypeRule
BytecodeRule
ArchitectureRule
NativeRule
```

复杂 Dataflow 等能力可以由专门 Backend 表达。

这种设计允许系统保持统一治理，同时承认不同分析引擎之间真实存在能力差异。

------------------------------------------------------------------------

# 10. Backend 是能力提供者

PMD、SpotBugs、ast-grep、Semgrep 等在 ERF 中都是 Backend。

每个 Backend 必须声明 Capability：

``` text
AST
Symbol Resolution
Type Resolution
Bytecode
Dataflow
Cross-file
Architecture
Supported Languages
Supported Versions
```

Factory 根据 RuleIntent 所需能力进行 Routing。

如果没有 Backend 可以可靠实现，正确答案可以是：

> UNSUPPORTED

而不是偷偷退化成 regex。

------------------------------------------------------------------------

# 11. AI 的正确位置

生成式 AI 对 Rule Factory 很有价值，但必须放在正确的位置。

适合 AI 的工作：

-   规范解释；
-   歧义识别；
-   Scenario 建议；
-   反例和边界案例生成；
-   RuleIntent 草稿；
-   Rule IR 草稿；
-   Native implementation scaffold；
-   测试草稿；
-   Finding explanation。

不适合 AI 的权力：

-   自动批准；
-   自动签名；
-   自动晋级 blocking；
-   修改生产 Policy；
-   在 CI 中临时决定代码是否违规。

因此：

> AI 生产 Candidate，确定性系统执行 Release。

------------------------------------------------------------------------

# 12. Verification 是产品核心

Rule Factory 与"代码生成工具"的根本区别，是生成后还有独立 Verification。

``` text
Candidate
   |
   v
Contract
   |
Positive / Negative
   |
Near Miss / Boundary
   |
Holdout
   |
Representative Repositories
   |
Compatibility
   |
Performance
   |
Security
   |
Repeatability
   |
   v
Evidence Bundle
```

规则是否可以进入生产，不由"作者觉得正确"决定，而由 Evidence 支撑。

------------------------------------------------------------------------

# 13. 从 Candidate 到 Blocking

规则有成熟度，而不是简单的 enabled/disabled。

``` text
DRAFT
  ↓
CANDIDATE
  ↓
VERIFIED
  ↓
SHADOW
  ↓
ADVISORY
  ↓
BLOCKING
  ↓
DEPRECATED
  ↓
RETIRED
```

Shadow 是关键运营阶段。

一条规则可以先扫描真实仓库而不阻塞任何人，通过真实数据了解：

-   Finding 数量；
-   FP；
-   受影响团队；
-   历史债务；
-   性能；
-   兼容性。

只有证据充分时才晋级。

------------------------------------------------------------------------

# 14. 为什么"零告警"不够

传统 CI 常把：

``` text
0 findings
```

解释为：

``` text
everything is fine
```

但真实情况可能是：

-   SpotBugs 没跑；
-   classpath 缺失；
-   worker timeout；
-   report 为空；
-   module 被排除；
-   bytecode 过期。

因此 ERF 将 Findings 和 Completeness 分离。

最终 Gate 是三态：

``` text
PASS
FAIL
INCONCLUSIVE
```

`INCONCLUSIVE` 表示：

> 当前没有足够证据得出 PASS 或 FAIL。

这是可信治理的重要基础。

------------------------------------------------------------------------

# 15. 历史债务不是新违规

企业上线新规则时，最危险的做法是让存量问题一次性阻塞所有开发。

ERF 建立 Debt Baseline：

``` text
Historical Debt       New Violation
      |                     |
  track / burn-down         |
                            v
                           Gate
```

例如：

``` text
existing: 3842
resolved:    3
new:         1
```

Gate 可以只因为那一个新 blocking finding 失败，而不要求开发者在当前 PR
修复 3839 个历史问题。

这让严格治理可以渐进落地。

------------------------------------------------------------------------

# 16. Exception 不是永久 Suppress

合理例外一定存在。

ERF 把 Exception 当成治理对象：

``` text
Rule
Scope
Reason
Owner
Approval
Expiry
Audit
```

Exception 必须足够窄，并自动过期或重新审核。

这比无限期的 `SuppressWarnings` 更适合企业治理。

------------------------------------------------------------------------

# 17. 误报如何变成系统资产

ERF 不把误报看成单纯客服问题。

``` text
False Positive
      |
      v
   Triage
      |
      v
Regression Case
      |
      v
Rule Revision
      |
      v
Verification
```

真实误报会成为未来版本的回归测试。

随着运营时间增长，企业积累的不只是更多规则，而是越来越强的 **Rule
Evidence Corpus**。

------------------------------------------------------------------------

# 18. 规则升级与影响分析

规则语义改变必须产生新版本。

新版本上线前，对同一 source snapshot 进行 old/new double scan：

``` text
Rule v1 --------+
                |
                +--> Impact Analysis
                |
Rule v2 --------+
```

运营者可以看到：

-   Finding delta；
-   新受影响仓库；
-   预计历史债务；
-   FP 变化；
-   性能变化。

升级因此成为可评估的变更，而不是一次不可控的全公司开关。

------------------------------------------------------------------------

# 19. 规则供应链

随着规则拥有阻断 CI 的权力，Rule Artifact 本身就是供应链资产。

因此 ERF 引入：

-   immutable release；
-   engine lock；
-   dependency lock；
-   SBOM；
-   provenance；
-   signature；
-   revocation；
-   trusted CI context。

被篡改的 Rule Bundle 不应该被执行。

被撤回的版本不应该因为本地缓存继续生效。

------------------------------------------------------------------------

# 20. 开源生态告诉我们的事

ERF 不需要重新实现成熟生态已经解决的问题。

**PMD** 已经提供可扩展的多语言静态分析能力；**PMD Rule Designer** 提供
AST inspection、XPath rule design、metadata 和 rule test 编辑。这说明
ERF 更应该解决规范和规则资产的治理，而不是重造 AST IDE。

**ast-grep** 展示了基于 tree-sitter 的结构化搜索、lint、rewrite 和 YAML
rule，非常适合未来作为轻量多语言 Structural Backend。

**Semgrep** 展示了成熟的 rule-as-code、rule validation、tests 和 CI
工作流，证明"规则也需要软件工程化测试"。

**Error Prone** 对默认 ERROR check 强调易理解、易修复、极低误报和
correctness value。这说明"能检测"与"应该阻断开发者"是两个不同问题。

**openHAB static-code-analysis** 同时集成
PMD、SpotBugs、Checkstyle、自定义规则、单测和汇总报告，说明多 Backend
原生实现 + 统一治理是一条现实可行路径。

ERF 的差异化不在 Scanner，而在 **Rule Lifecycle + Evidence +
Governance + Enforcement Integrity**。

------------------------------------------------------------------------

# 21. 企业运营驾驶舱

RuleOps Center 应围绕结果而不是产量运营。

### Standard Coverage

``` text
规范总数
可机器检查
完全覆盖
部分覆盖
未覆盖
不可机器化
```

### Rule Health

``` text
Shadow
Advisory
Blocking
High-FP
Deprecated
Revoked
```

### Enforcement Health

``` text
Complete Rate
INCONCLUSIVE Rate
New Violations
Historical Debt
Exceptions
Scan Latency
```

### Factory Efficiency

``` text
Time-to-Rule
Author Active Time
Verification Cycle Time
Promotion Rate
Rollback Rate
```

核心 KPI 不应该是"本季度生成 1000 条规则"。

------------------------------------------------------------------------

# 22. 商业与组织价值

ERF 带来的价值不是简单替换某个静态分析产品。

它可以降低：

### 规则生产成本

通过 Intent、模板、AI authoring 和统一 Verification 缩短从规范到 Rule
Asset 的周期。

### 重复建设

企业不再让每个团队分别实现同一规范。

### 误报成本

通过 Shadow、Evidence 和 Feedback Loop，在 blocking 前发现低质量规则。

### 治理风险

通过 Completeness 和三态 Gate 避免扫描失败造成假绿。

### 迁移成本

通过 Intent/Implementation 分离，企业规范不被某个 Backend 锁死。

### 审计成本

规则为什么存在、谁批准、哪个版本、在哪些仓库执行、是否完整，都有可追踪证据。

------------------------------------------------------------------------

# 23. 首期落地

首期不追求平台"大而全"。

12 周 Pilot 聚焦：

``` text
Java
+
100 Standards Inventory
+
20 Candidate Scenarios
+
PMD Primary Backend
+
1 SpotBugs Representative Rule
+
3 Typed Templates
+
RuleIntent / Rule IR
+
Verification
+
Evidence Bundle
+
Signed Release
+
3 Representative Repositories
+
Baseline
+
PASS / FAIL / INCONCLUSIVE
```

第 4 周设置明确 Go/No-Go：

> 如果 Standard → Intent → IR → PMD → Tests
> 不能明显降低规则生产成本，同时保持质量，则先修正 Factory
> Kernel，不继续扩建平台。

------------------------------------------------------------------------

# 24. 长期演进

Pilot 之后，ERF 可以沿三个方向扩展。

## Backend Expansion

``` text
PMD
SpotBugs
ast-grep
Semgrep
Checkstyle
Error Prone
ArchUnit
CodeQL
Infer
Internal Engines
```

## Language Expansion

从 Java 逐步进入：

-   JavaScript / TypeScript；
-   Python；
-   Go；
-   Kotlin；
-   C/C++；
-   企业内部 DSL。

扩展顺序由企业 Coverage Gap 决定。

## Governance Expansion

-   Web authoring；
-   SSO/RBAC；
-   multi-tenant；
-   policy inheritance；
-   IDE feedback；
-   analytics；
-   architecture governance；
-   security rule packs；
-   compliance evidence。

------------------------------------------------------------------------

# 25. 成功的最终形态

当 ERF 成熟以后，一个企业规范可以拥有完整数字生命周期：

``` text
“我们要求什么”
        |
        v
“哪些场景可以机器检查”
        |
        v
“由什么 Backend 实现”
        |
        v
“我们为什么相信它”
        |
        v
“哪个版本正在生产”
        |
        v
“哪些项目完整执行了它”
        |
        v
“它阻止了哪些新增问题”
        |
        v
“还有多少历史债务”
        |
        v
“哪些例外正在生效”
        |
        v
“哪些真实反馈改善了下一版本”
```

这才是 Enterprise Rule Factory 的最终意义：

> **把企业代码规范从静态文档，升级为持续运行、持续验证、持续演进的工程控制系统。**

------------------------------------------------------------------------

# 26. 结语

静态分析工具解决的是"如何发现问题"。

Enterprise Rule Factory 解决的是另一个层次的问题：

> **企业如何可靠地决定检查什么、如何生产检查、为什么相信检查、如何安全执行检查，以及如何让检查随组织长期演进。**

因此 ERF 的核心不是 AI，也不是 PMD，更不是某一种 DSL。

它的核心是：

> **Standard + Intent + Evidence + Release + Enforcement + Feedback**

Scanner 会变化，模型会变化，语言会变化。

只要这条 RuleOps
生命周期保持稳定，企业就拥有了一层可以长期积累的研发治理基础设施。

------------------------------------------------------------------------

## 公开参考

-   PMD --- https://github.com/pmd/pmd
-   PMD Rule Designer --- https://github.com/pmd/pmd-designer
-   ast-grep --- https://github.com/ast-grep/ast-grep
-   Semgrep --- https://github.com/semgrep/semgrep
-   Semgrep Rules --- https://github.com/semgrep/semgrep-rules
-   Error Prone, Criteria for new checks ---
    https://github.com/google/error-prone/wiki/Criteria-for-new-checks
-   openHAB static-code-analysis ---
    https://github.com/openhab/static-code-analysis

------------------------------------------------------------------------

## 独立性声明

Enterprise Rule Factory 是独立项目。

它不依赖 Amber Protocol、Amber runtime、Amber CLI 或 Amber
的发布和存储体系。对 Amber
的参考仅限于此前架构讨论中具有普适价值的工程治理思想；ERF 拥有自己的
Domain Model、Protocol、Runtime、Registry、Lifecycle 和产品路线。

------------------------------------------------------------------------

# 27. 产品运行形态：服务端治理，本地执行

ERF 最终不是一个单纯的"Web 平台"，也不是一个安装在 IDE 里的大型插件。

正式产品形态是：

> **中央 RuleOps 服务端 + 分布式本地/CI Runner + 轻量 IDE/CLI 客户端。**

``` text
RuleOps Server
     |
Rule / Policy / Signed Bundle
     |
     +----------+-----------+
     |          |           |
Developer PC   CI Agent   Air-gap
     |          |           |
     +----------+-----------+
                |
            rf-runner
                |
         PMD / SpotBugs
                |
            Local Code
```

服务端负责"企业应该检查什么、哪些规则可信、哪些规则正在发布"。

Runner 负责"在代码所在的授权域真正执行检查"。

因此默认的数据原则是：

> **规则去找代码，而不是代码去找规则服务。**

------------------------------------------------------------------------

# 28. 为什么不把扫描全部放在服务端

如果每次扫描都上传源代码，会带来：

-   源码安全边界扩大；
-   网络和带宽成本；
-   大型 monorepo 延迟；
-   企业内网和 air-gap 难以接入；
-   RuleOps Server 故障直接影响全公司 CI。

因此 ERF 把执行能力下沉为统一 `rf-runner`。

同一个 Runner 可以运行在开发者电脑、CI Agent、企业构建集群和离线环境。

------------------------------------------------------------------------

# 29. IDE 插件只是客户端

IntelliJ 或 VS Code 插件的职责是：

-   展示 Finding；
-   跳转源码；
-   Explain；
-   Report False Positive；
-   Request Exception。

插件不应该分别实现 PMD、SpotBugs 和 Baseline/Gate。

它们统一调用 `rf-runner`。

这样本地和 CI 可以使用相同 Rule Bundle、Engine Lock 和
Normalizer，降低"IDE 不报、CI 却报"的语义漂移。

------------------------------------------------------------------------

# 30. 服务端不是 CI 的实时单点

RuleOps Server 发布签名 Rule Bundle。

Runner 可以缓存：

``` text
Policy
Rules
Artifacts
Engine Locks
Compatibility
Signatures
Revocation Snapshot
```

只要 Bundle 和撤回信息仍满足企业 Policy，即使 Server 暂时不可用，CI
仍可继续扫描。

如果无法确认 Bundle 是否仍可信，则返回 `INCONCLUSIVE`，而不是假装 PASS。

------------------------------------------------------------------------

# 31. 支持内网和离线企业

ERF 从架构上支持：

1.  Connected Enterprise；
2.  Private / Air-gapped；
3.  Standalone Developer。

企业可以把 `.rfbundle` 镜像到内网 Artifact Repository。

开发者也可以：

``` text
rf scan --bundle company-java-policy-2026.09.rfbundle
```

进行完全离线扫描。

这使产品能够进入金融、政企、安全研发和隔离网络等场景，而不要求源码访问公共云。

------------------------------------------------------------------------

# 32. 本地结果和生产结果不是同一种信任

本地扫描的目标是快速反馈。

CI 扫描的目标是形成可信工程证据。

因此 ERF 区分：

``` text
LOCAL_UNATTESTED
CI_TRUSTED
CONTROLLED_RUNNER
```

受保护分支只接受可信 CI 注入并验证过的 repository、commit、policy、rule
bundle、engine 和 baseline identity。

开发者不能通过修改本地配置或伪造 result.json 绕过生产 Gate。

------------------------------------------------------------------------

# 33. Rule Factory Worker 与 Runner 分离

规则生成和代码扫描也不能混在一起。

Factory Worker 处理 AI 或用户生成的 Candidate
Rule，因此必须被视为不可信构建环境。

`rf-runner` 只运行已经 Verification、签名并发布的 Rule Release。

因此系统形成两个明确安全域：

``` text
Untrusted Rule Production
        |
Factory Sandbox
        |
Verified + Signed Release
        |
======== Trust Boundary ========
        |
Deterministic Execution
        |
     rf-runner
```

这使 AI 可以提高规则生产效率，同时不进入生产期裁决链。

------------------------------------------------------------------------

# 34. 最终产品组件

成熟后的 ERF 可以被理解成四个产品组件：

``` text
1. RuleOps Server
   企业规范、规则生产、Verification、Registry、Policy、治理

2. rf-runner
   本地/CI 扫描、Backend、Finding、Completeness、Baseline、Gate

3. Factory Worker
   隔离 Candidate 构建和测试

4. IDE / CLI
   Developer Experience
```

其中真正不可替代的核心是：

> **RuleOps Server + rf-runner**

IDE Plugin 是体验层，不应该成为首期架构关键路径。

------------------------------------------------------------------------

# 35. 推荐交付顺序

首期优先：

``` text
CLI
+
rf-runner
+
PMD
+
Bundle
+
CI
+
Git/Artifact based Control Plane
```

第二阶段再建设完整 RuleOps Server。

第三阶段再做 IntelliJ / VS Code 深度体验。

这样可以先证明 RuleOps 的核心价值，而不是把首个季度消耗在 Web UI 或 IDE
插件工程上。
