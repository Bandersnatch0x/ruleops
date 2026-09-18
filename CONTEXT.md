# Enterprise Rule Factory

Enterprise Rule Factory 把企业代码规范转化为可验证、可发布、可执行、可审计且可演进的机器检查资产。本语境使用统一语言描述从规范到治理决策的 RuleOps 生命周期。

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
