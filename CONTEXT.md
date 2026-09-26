# Enterprise Rule Factory

Enterprise Rule Factory 把企业代码规范转化为可验证、可审计且可演进的机器检查资产，其对外承诺收窄为「可信规则执行层」——以 Completeness、三态 Gate 与签名执行证明取代「0 告警即通过」。本语境使用统一语言描述从规范到治理决策的 RuleOps 生命周期。「可执行 / 受保护分支强制」是待验证假设与后续阶段目标（ADR-0167），当前 checkout 不授予该权力。

## Repository Boundary（ADR-0167 决策 5）

本仓库为单一 **monorepo**，对外主线是**可信执行核心**；其余为 monorepo 内受治理组件，经强制
seam（`contracts/*` + 签名 RuleBundle，`runtime/bundle-delivery`）向核心交付，不与核心源码级耦合。

- **执行核心**（对外主线）：`runtime/*`（Rust 执行/信任/完整性）、`backend-hosts/{fake,java}`、
  `workspace-resolvers/*`、公共 `contracts/*`。
- **创作簇**（受治理组件）：`control-plane/authoring|factory|coverage|ontology`（Java 创作/工厂/本体）、
  `control-plane/server|verification`、`clients/{vscode,intellij}`（IDE 薄客户端，ADR-0166）。
- **唯一 seam**：创作簇产出签名 RuleBundle 经 delivery 契约交给执行核心；核心只消费 bundle digest；
  IDE 只以 `rf` 为接口。执行核心不得新增对 `control-plane/*` 源码的直接依赖。

## Intent and Coverage

**ERF Ontology**:
对 ERF 领域概念、关系与语义约束的显式、可版本化权威语义模型。
_Avoid_: Glossary, Database Schema, Embedding Index

**Ontology Revision**:
一个不可变、内容寻址的 ERF Ontology 快照，是治理资产与编译运行契约的语义依据。
_Avoid_: Latest Ontology, Mutable Vocabulary

**OntologyContractPack**:
由已批准 Ontology Revision 编译出的签名、不可变、封闭世界运行语义契约。
_Avoid_: Ontology Revision, Runtime Reasoner, JSON View

**OntologyCompatibilityEvidence**:
证明一个增量 Ontology Revision 不改变指定既有资产执行语义的签名兼容性证据。
_Avoid_: Version Range, Compatibility Claim, Re-verification Waiver

**Asset IRI**:
在 Ontology Revision 之间稳定识别某个领域资产或概念的全局标识。
_Avoid_: Display Name, Database Row ID

**Core Ontology**:
由中央治理的 ERF 稳定概念、关系与不变式集合，是所有扩展本体的共同基础。
_Avoid_: Enterprise Domain Extension, Monolithic Enterprise Ontology

**Extension Ontology**:
在 Core Ontology 边界上扩展语言、Backend 或企业领域语义的受治理本体模块。
_Avoid_: Unmanaged Vocabulary, Core Ontology

**Semantic Projection**:
将 Standard 或 Scenario 的规范意图映射为经批准本体概念、关系和范围断言的机器权威表达。
_Avoid_: Embedding, Keyword Tag

**Standard**:
组织对代码的正式要求，具有独立身份、版本、所有者和适用范围。
_Avoid_: Rule, Check

**Scenario**:
Standard 下可独立判定机器覆盖状态的最小业务语义单元。
_Avoid_: Case, Sub-rule

**Coverage Ledger**:
按 Scenario 记录既有覆盖、新规则需求、环境缺口、Backend 能力缺口与不可机器检测状态的持续治理资产。
_Avoid_: Coverage Report

**RuleIntent**:
某个 Scenario 应被检查的业务语义、范围和所需分析能力，不包含特定 Backend 实现。
_Avoid_: Rule Code, Prompt Rule

## Rule Assets

**RuleImplementation**:
一个 RuleIntent 在特定 Backend 上的可执行实现；同一 RuleIntent 可有多个实现。
_Avoid_: RuleIntent, RuleRelease

**Candidate**:
尚未通过独立 Verification 与发布治理的规则建议、实现或测试资产。
_Avoid_: RuleRelease, Verified Rule

**EvidenceBundle**:
支撑某个 RuleImplementation 质量结论的独立、可复核验证证据集合。
_Avoid_: Test Report

**RuleRelease**:
冻结 RuleIntent、RuleImplementation、Artifact、Backend/Engine 身份、Evidence 与供应链信息的不可变发布资产。
_Avoid_: Candidate, Mutable Rule

**RunnerRelease**:
冻结执行核心身份、兼容性和供应链证据的签名、不可变 Runtime 发布资产。
_Avoid_: BackendRuntimePack, Local Build, ExecutionLock

**RunnerMeasurement**:
由独立可信启动环节产生、证明实际启动 RunnerRelease 内容身份的不可变测量记录。
_Avoid_: Runner Self-report, RunnerRelease, Runtime Log

**RunnerBootstrap**:
在执行核心启动前验证其信任链与实际内容身份、但不参与扫描或 Gate 计算的最小可信职责。
_Avoid_: Runner, Auto-updater, Gate Verifier

**RuleBundle**:
面向执行环境发布的、经签名且内容寻址的 RuleRelease、Policy、ExecutionLock 和兼容性元数据集合。
_Avoid_: RuleRelease, Ruleset File

**RuleOpsServerApi**:
RuleOps Control Plane 面向客户端与治理工具的版本化领域 API，承载标准、覆盖、规则生产、发布、策略、反馈、例外与审计资源，但不授予扫描或 SCM 执行权力。
_Avoid_: Server Database, SCM Authority, Runner Gate

**GovernanceMutation**:
带不可变资源身份、内容摘要、幂等身份和审计上下文的治理写入；相同请求可重放，身份冲突、非法转换或隐私越界必须失败。
_Avoid_: Mutable Update, HTTP POST, Silent Retry

**GovernanceJournal**:
按顺序追加、可回放、以摘要链保护的治理变更事实记录；承载资源身份、幂等请求、生命周期和审计上下文，但不承载源码或完整 Finding。
_Avoid_: Mutable Log, Database Table, Eventual Audit

**JournalRecovery**:
在 RuleOps Server 启动时逐条验证并重建治理状态的 fail-closed 回放过程；序号、摘要链、资源摘要、请求摘要或生命周期不一致时拒绝启动。
_Avoid_: Best-effort Replay, Tail Dropping, Silent Repair

**BundleDeliveryDescriptor**:
绑定 RuleBundle 内容摘要、组成资产摘要、兼容身份、平台、签名/撤回快照摘要和有效期的版本化 Server-to-Runner 交付描述；tag 与 latest 不构成身份。
_Avoid_: Bundle Tag, Latest, Download Metadata

**BundleCache**:
按 BundleDeliveryDescriptor 的精确内容摘要读写、不可覆盖且可重启恢复的本地/CI 缓存边界；命中不授予签名或 Gate 权威。
_Avoid_: Mutable Cache, Registry Authority, Cache Hit as PASS

**OfflineAvailability**:
在离线或 Server 暂不可用时，对签名、TrustMetadata、撤回快照、依赖 Pack、有效期和 TrustedContext 的联合可用性判断；任一关键条件未知时结果为 INCONCLUSIVE。
_Avoid_: Best-effort Offline, Stale Bundle, Unknown Revocation

**ThinClientAction**:
由 CLI/IDE 发起的调用 Runner、展示 Finding、定位本地相对位置、解释或准备反馈/例外申请的 UX 动作；不复制 Runtime、不授予 Server/SCM/Gate 权力。
_Avoid_: IDE Authority, Client Runtime, Local Gate

**FindingView**:
供 ThinClient 展示和导航的闭合、digest-only Finding 视图；只含规则/fingerprint/message 与 allowlisted repository-relative location，默认拒绝源码和完整 Finding。
_Avoid_: Full Finding, Source Snippet, Engine Report

**ScanResultProjection**:
由 `erf-scan-output` 从真实 Runner/Gate 结果生成的 `scan-output-v1` 文本、JSON 或 SARIF
结果；绑定稳定 Finding identity/digest、Completeness、Gate 三态、authority、结构化诊断和
阶段进度，但不改变 Gate、不上传源码或获得 SCM 权力。
_Avoid_: Raw Engine Report, Gate Authority, Source Upload

**StructuredScanDiagnostic**:
带稳定 code、stage、retry、nextStep 和可选 detail digest 的扫描诊断；用于区分 resolver、
Host、sandbox、trust、timeout、plan 和 incomplete execution，而不是把自由文本日志当作
机器契约。
_Avoid_: Unstructured Error Text, Retry Guess, Hidden Failure

**FeatureEvidenceReport**:
按 `feature-evidence-v1` 记录 feature check 的输入身份、可复跑命令、stdout/stderr 日志和
证据等级；`PASS` 只表示行为命令成功，`STATIC_ONLY` 只表示结构校验，`BLOCKED` 表示所需
行为证据失败或无法运行。
_Avoid_: Keyword PASS, Missing Provenance, Silent Fallback

**GovernanceRequestDraft**:
由 ThinClient 生成、带 finding digest、actor、reason、request id 和自身摘要的 PROPOSED/REQUESTED 申请；需要 Server 或人工复核，不等于反馈入账或 ExceptionGrant。
_Avoid_: Submitted Feedback, Approved Exception, Gate Override

**ThinClientSubmission**:
由 Thin Client 在显式 Server endpoint 上发送、经闭合 digest-only 校验的 FeedbackDraft/ExceptionRequest；使用版本化 envelope、幂等键和审计上下文，只有 HTTP 400-499 可归为确定性拒绝，其他未知结果不生成成功回执、不自动重试，但始终不授予批准、Gate 或 SCM 权限。
_Avoid_: Client Approval, Direct ExceptionGrant, Gate Mutation

**SubmissionReceipt**:
Server 对 ThinClientSubmission 返回的已接收或幂等重放证明；`RECEIVED_ONLY` 只表示申请进入 Server 治理边界，不表示 Feedback 采纳、Exception 批准或 Gate PASS。
_Avoid_: Approval Receipt, GateDecision, SCM Status

**SubmissionStatusRecovery**:
在提交连接中断或响应未知后，按 request ID 或 artifact digest 对 Feedback/Exception 进行只读
状态查询的闭合结果；`PENDING`、`RECEIVED_ONLY`、`REPLAYED`、`UNKNOWN` 保持可区分，查询不
重放写请求，`UNKNOWN`/`PENDING` 不构成批准、Gate 或 SCM 权力。
_Avoid_: Automatic Retry, Approval Lookup, Gate Recovery

**StatusPublication**:
由已验证 GateAttestation 生成的、绑定 repository/commit/policy/attestation subject、运行序号、有效期与自身摘要的版本化三态状态发布事实；`PASS`/`FAIL`/`INCONCLUSIVE` 分别映射为 `success`/`failure`/`action-required`，不表示已经写入 SCM。
_Avoid_: SCM Write, Neutral Success, Mutable Status

**StatusPublicationAdapter**:
接收并按身份、单调序号、时效、幂等与 digest 校验 StatusPublication 的外部 transport seam；凭据、网络和 SCM 写入属于部署边界，Runtime 只提供无凭据 artifact。
_Avoid_: Embedded SCM Credential, Runner Push, Adapter Approval

**StatusAdapterDeployment**:
描述外部 Status Adapter 所需的 provider、HTTPS endpoint、opaque credential reference、精确 `status:write` scope、publication digest 与请求幂等身份的不可变部署输入；不携带 secret、源码或完整 Finding。
_Avoid_: Token Payload, Repository Admin Scope, Runtime Connector

**StatusAdapterReceipt**:
外部 adapter 对已校验 publication 的 `ACCEPTED`/`IDEMPOTENT` 接收证明；`REJECTED` 与 `UNKNOWN` 只表示拒绝或传输未知，均不得转成 SCM 写入、Gate PASS 或审批。
_Avoid_: SCM Published, Approval, Silent Retry

**ExecutionLock**:
冻结一次可信执行所需 Backend、Resolver、Runtime、协议与沙箱资产精确身份的签名解析结果。
_Avoid_: EngineLock, Version Range, Machine Discovery

**AuthoringSuggestion**:
由 AI 或检索能力生成的非权威作者建议，只能经人工接受后转化为新 Candidate。
_Avoid_: Finding, Evidence

**Authoring Orchestrator**:
以确定性范围、分组、覆盖状态和来源追溯约束 AI 作者任务的非权威能力。
_Avoid_: OpenCodeReview Adapter, Gate

**Authoring Retrieval**:
从已授权的 Standard、Scenario、RuleIntent、Evidence 和 Feedback 中返回带来源的相关作者上下文的非权威能力。
_Avoid_: zvec Integration, Coverage Proof

## Execution and Governance

**Finding**:
特定 RuleRelease 对特定源码位置产生的规范违反记录，带有执行与 Backend 身份证据。
_Avoid_: AuthoringSuggestion, Review Comment

**CanonicalFindingSet**:
对已验证 Finding 的语义字段进行版本化规范化与稳定排序后形成的确定性结果集合。
_Avoid_: Finding Stream, Runtime Metrics, Attempt Transcript

**DiagnosticBundle**:
经显式授权生成的加密、短期、受审计扫描诊断资产，只包含排障所需最小信息。
_Avoid_: Attempt Staging, Default Upload, ExecutionEvidence

**Semantic Anchor**:
在源码布局或规则实现变化时仍尽量稳定标识同一违规语义位置的规范化代码身份。
_Avoid_: Line Number, Message, RuleRelease Version

**BackendObservation**:
Backend Host 对引擎观测到的位置、符号、规则和证据进行结构化后产生的 Policy-neutral 事实。
_Avoid_: Finding, Raw Engine Report

**ObservationLocation**:
BackendObservation 中绑定受检内容身份的封闭位置类型，可表达源码范围、符号、字节码成员或文件。
_Avoid_: Display Line, Engine-native Coordinate, Unverified Path

**BackendRuntimePack**:
包含 Backend Host、分析引擎、运行环境依赖引用与供应链证据的签名、不可变、内容寻址执行资产。
_Avoid_: RuleBundle, RuleRelease, Machine-installed Engine

**RuntimeDependencyPack**:
由 BackendRuntimePack 按内容身份引用的、可复用且独立签名的语言或虚拟机运行环境资产。
_Avoid_: System Runtime, BackendRuntimePack

**Completeness**:
每个必需规则与分析单元是否被成功、完整执行的证明，独立于 Finding 数量。
_Avoid_: No Findings, Scan Success

**ScanPlan**:
执行前冻结的必需 RuleRelease 与分析单元集合，是 Completeness 的权威分母。
_Avoid_: Backend Report, Finding List

**BackendExecutionPlan**:
Backend Host 对获分配 ScanPlan 单元的确定性执行细化，不得改变 Completeness 分母。
_Avoid_: ScanPlan, Dynamic Work Discovery

**ScanExecution**:
对一个冻结 RepositorySnapshot、RuleBundle、ExecutionLock、Policy 与 TrustedContext 的单次执行实例。
_Avoid_: CI Job, Backend Process, ScanPlan

**ScanAttempt**:
ScanExecution 内一次具有独立身份、其结果只能整体提交或整体丢弃的 Backend Host 执行尝试。
_Avoid_: ScanExecution, Partial Finding Stream, Retry Count

**AttemptTranscriptDigest**:
绑定一个 ScanAttempt 实际接收观测序列的完整性摘要，不代表规范化语义结果身份。
_Avoid_: SemanticResultDigest, Finding Fingerprint

**SemanticResultDigest**:
对经验证、规范化和稳定排序的扫描语义结果计算的确定性摘要。
_Avoid_: AttemptTranscriptDigest, RuleBundle Digest

**Workspace Resolver**:
从 RepositorySnapshot 和可信构建元数据确定项目结构事实、但不执行检查规则的领域能力。
_Avoid_: Backend Host, Build System, Scan Planner

**WorkspaceResolverPack**:
承载特定构建生态项目结构解析能力及供应链证据的签名、不可变、内容寻址资产。
_Avoid_: BackendRuntimePack, WorkspaceDescriptor, Build Tool

**WorkspaceDescriptor**:
Workspace Resolver 生成的不可变、内容寻址项目结构事实，包括模块、源码根、依赖与构建产物身份。
_Avoid_: Build Script, BackendExecutionPlan, RepositorySnapshot

**WorkspaceAttestation**:
由可信构建身份签署、证明 WorkspaceDescriptor 与特定源码、工具链和构建产物对应关系的证据。
_Avoid_: WorkspaceDescriptor, Local Cache Metadata, Self-assertion

**WorkspaceResolutionMode**:
声明 WorkspaceDescriptor 来自无脚本静态解析或可信构建导出的封闭取值。
_Avoid_: Build Tool, Resolver Implementation, Implicit Fallback

**TrustMetadata**:
由离线信任根委派签名职责并描述密钥、资产范围、时效、轮换与撤回状态的版本化治理资产。
_Avoid_: Application Config, Public-key List, Revocation Log

**TrustCheckpoint**:
离线镜像发布的签名时点证明，固定各信任角色最高版本、快照摘要与有效期。
_Avoid_: TrustMetadata Root, Local Clock, Mutable Mirror State

**PersistedTrustState**:
由 TrustStateFile 有界、原子保存的最高已见 TrustCheckpoint 地板；首次安装必须显式初始化，
可信执行只能读取已存在状态，缺失、损坏或回退均不得静默恢复。
_Avoid_: Trust Cache, Unsigned Checkpoint, Auto Bootstrap

**ChangedSet**:
由显式 Git base/head、唯一 merge-base 和 NUL-framed diff 解析产生的、稳定排序且带摘要的
仓库内变更集合；路径不确定、浅克隆、冲突状态或提交身份不可复核时拒绝生成。
_Avoid_: HEAD~1 Guess, Full-Repository Fallback, SCM Mutation

**AffectedScope**:
从 ChangedSet 和受限静态 Maven/Gradle 图产生的、按 backend 区分的受影响闭包；PMD AST
可保留 FILE_CANDIDATE，类型解析/SpotBugs 至少为 MODULE，图或生成源不确定时升级 FULL。
_Avoid_: Git Diff as ScanPlan, Dependency Guess, Narrow Scope after Unknown

**IncrementalScanPlan**:
消费 AffectedScope 后在执行前冻结的版本化 FULL/MODULE/INCREMENTAL_PREVIEW 计划；绑定
仓库范围、RuleBundle、Policy、Resolver/Backend/Runner 与协议身份，明确 planned、excluded
和 unevaluated 单元，并决定是否具备权威 Gate 条件。
_Avoid_: Dynamic Work Discovery, Preview as PASS, Scope Digest Alone

**ScanResultCache**:
以 ScanPlan 与 SemanticResultDigest、Baseline、Exception、UnifiedResult/TrustedContext
身份组成内容寻址键的摘要缓存；可跨进程重启复核，但命中不改变 Completeness 或授予 Gate/SCM 权限。
_Avoid_: Latest Result, Source Cache, Cache Hit as PASS

**IncrementalFallbackEvidence**:
消费 ScanPlan 的计划/覆盖摘要与缓存观察，记录单调时钟阶段耗时，并在 build、规则、Policy、Resolver、
依赖图、生成源、缓存、merge-base 或影响范围预算无法证明安全时显式放宽到 FULL；增量与全量摘要不一致、
预算不满足或故障注入只能产生 INCONCLUSIVE/diagnostic，不得成为权威 PASS。
_Avoid_: Cache Hit as Authority, Wall-clock Claim, Synthetic Pilot Outcome

**Resolved Policy Manifest**:
将 Policy Profile、ruleset、版本、参数和冲突决议确定性展开后的不可变结果。
_Avoid_: Dynamic Policy, Policy Profile

**Debt Baseline**:
经批准且不可变的历史违规集合，用于把存量债务与新增违规分离。
_Avoid_: Comparison Baseline, Quality Benchmark

**Comparison Baseline**:
从可信 SCM 上下文确定的比较起点，用于判断代码变更所引入的差异。
_Avoid_: Debt Baseline, Quality Benchmark

**Quality Benchmark**:
用于评估规则 Precision、Recall 等质量的已标注样本集。
_Avoid_: Debt Baseline, Comparison Baseline

**GateDecision**:
基于 Policy、Finding、Completeness、Baseline、Exception 与可信上下文生成的 `PASS`、`FAIL` 或 `INCONCLUSIVE` 决策。
_Avoid_: Scan Result, PASS_WITH_DEGRADED_COVERAGE

**CIAuthorityStage**:
表示 RuleOps 结果对 CI/SCM 可产生的权限等级：`SHADOW` 仅观测，`ADVISORY` 可发布不阻断状态，`BLOCKING` 才允许受治理的合并阻断；阶段转换必须有独立证据和授权。
_Avoid_: GateDecision, Status Publisher, Protected-Branch Authority

**ObservationMode**:
标识一次 Pilot Measurement 来自本地提交序列的 `LOCAL_SIMULATION`、内部受控
仓库的 `SYNTHETIC_SHADOW`，还是已登记外部仓库的只读 `LIVE_SHADOW`；三种模式
必须分开聚合，前两种结果不可充当产品结论。
_Avoid_: Authority, RepositorySnapshot, Test Fixture

**PilotMeasurement**:
从已验证 UnifiedResult 派生的 digest-only 运行、反馈或治理记录，用于在预登记 Observation Window 内计算 Pilot Metrics。
_Avoid_: Full Finding Report, CI Log, Product Outcome

**PilotEvidencePackage**:
引用不可变 PilotSuccessContract、观察窗口、固定样本身份、各指标证据记录及其摘要的独立、digest-only 证据包；它承载 Pilot 结果的可复核输入，不改变合同阈值，也不授予执行权力。
_Avoid_: PilotSuccessContract, Full Finding Upload, Product Authorization

**PilotAdjudication**:
由 `platform-engineering` 治理负责人基于完整 PilotEvidencePackage 作出的人工 Pilot 裁决；只有全部必需指标为 `PASS` 且证据包完整时才能为 `GO`，已知失败为 `NO_GO`，缺失、冲突或无效证据为 `INCONCLUSIVE`。
_Avoid_: GateDecision, Automated Pilot Approval, Protected-Branch Authority

**FastDecisionEvidence**:
绑定输入摘要、版本化 DecisionSpec、provider/model 身份、typed answer、概率/置信度、
margin、阈值、过期时间和 fallback 的 digest-only 影子分流证据；`UNKNOWN` 必须回退到
确定性路径或人工复核，不能被当作 Gate 结果。
_Avoid_: GateDecision, Model Approval, Natural-language Decision

**FastDecisionAdapter**:
实现 `fast-decision-v1` 类型化 Choice/Score/Boolean 问题的 provider-neutral 适配 seam；
确定性 baseline 是默认实现，外部 provider 可选且不获得 RuleOps 权威。
_Avoid_: Gate Adapter, Trust Provider, Automatic Retry

**ProjectConfig**:
版本化、可审阅、可人工修改的仓库扫描配置；冻结语言、构建系统、模块、source set、
include/exclude、执行模式、Bundle cache 与 TrustState 路径，配置缺失、冲突或无法校验时
不得启动扫描。
_Avoid_: CLI Last-write-wins, Resolver Guess, Hidden Scope

**WorkspaceConfigIdentity**:
由有效 ProjectConfig 确定性派生的 `effectiveConfigDigest`、WorkspaceDescriptor digest
与 config ScanPlan digest 三元身份；必须随 Thin Client 扫描结果输出，供本地与 CI 复核范围。
_Avoid_: Wall-clock Identity, Latest Config, Source Upload

**BootstrapDoctor**:
在扫描前分别检查配置、依赖、Bundle cache 完整性、TrustState 和可选 scan probe 的
fail-closed 预检；cache 命中仍需由 delivery verifier 重查签名、freshness、撤回快照和
execution identity，预检本身不授予 Gate 或 Trust 权力。
_Avoid_: Silent Fallback, Cache Authority, Trusted Bootstrap

**PilotRuntimeBoundary**:
真实 Shadow Pilot 可恢复、可复核的最小治理状态边界，包含固定资产身份、摘要、生命周期审计、窗口/运行证据、反馈与可信 checkpoint，但不包含 SCM 写权限或生产强制执行。
_Avoid_: Production Registry, SCM Authority, One-shot Report

**GovernanceRecordRetention**:
由治理负责人登记的保留约束，保证 Pilot 证据、治理快照与审计链从观察窗口开始到最终裁决及规定保留期内可复核；撤销或回滚不删除原记录。
_Avoid_: Report Cleanup, Infinite Raw Retention

**FactoryRepresentativenessBoundary**:
定义一次 Pilot 的候选组成与已测 RuleIntent 最低分层，从而限定 Factory 成本结论可以外推到的范围；当前 20 条只代表三类模板路径的受限边界，不代表一般规则总体。
_Avoid_: Candidate Count, Coverage Ledger, Population Claim

**GateAttestation**:
由可信执行身份签署、绑定 Gate 全部输入、语义结果、完整性与三态决策的可验证证明。
_Avoid_: CLI Exit Code, GateDecision, CI Log

**ExceptionGrant**:
对特定规则语义、代码范围和期限授予的签名、不可变、可撤回风险接受资产。
_Avoid_: Policy Toggle, Suppression Comment, Permanent Waiver

**ExceptionDecision**:
在保留 Finding 与 BaselineState 的前提下，判定某个 ExceptionGrant 是否对本次 Gate 适用的结果。
_Avoid_: BaselineState, Finding Deletion, ExceptionGrant

**Status Publisher**:
验证 GateAttestation 后使用最小权限凭证把原决策发布到 SCM 的非决策职责。
_Avoid_: Gate Engine, Runner, Backend Host

**ArtifactStatement**:
随发布资产携带、绑定其内容身份、资产类型、签名职责和供应链证据的可移植签名声明。
_Avoid_: TrustMetadata, Mutable Tag, Download Receipt

**ConformanceEvidence**:
证明某个执行 Pack 已通过固定版本 ERF 合规套件的不可变验证证据。
_Avoid_: Unit Test Report, Self-certification, Compatibility Claim
