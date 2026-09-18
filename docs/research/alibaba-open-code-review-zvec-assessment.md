# Alibaba OpenCodeReview / zvec 对 Enterprise Rule Factory 的适配性评估

> 研究日期：2026-09-17  
> 结论适用的源码快照：OpenCodeReview `e556dfad56f11afe15cce9b1a0ca7f8993a65134`；zvec `027e7d5b4434db62de4d96f3bcf1864a2cadf776`  
> 方法：只使用两个官方仓库的 README、文档、源码、发布工作流、License/NOTICE 和官方 Release；未依赖第二手文章。

## 执行结论

**产品决策是吸收能力而不直接集成：OpenCodeReview 和 zvec 均只作为一手设计样本，ERF 不调用其 CLI/MCP，不嵌入其库，不 fork 或分发其代码。**

根本原因是产品边界不同：

- ERF 要把 Standard 变成经独立 Verification、签名发布、可确定执行的 Rule Asset，并明确要求“AI 只参与 Authoring，不参与生产期裁决”、“0 findings 不等于 scan complete”。[本项目架构基线](https://github.com/Bandersnatch0x/ruleops/blob/730fefd31b71e7eca1552b709c370922110d1e18/docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md#L16-L38) 还把 LLM 在 CI 中直接判断是否违规列为 V1 非目标。[产品边界](https://github.com/Bandersnatch0x/ruleops/blob/730fefd31b71e7eca1552b709c370922110d1e18/docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md#L65-L104)
- OpenCodeReview 是一个把 Git diff/文件发给可配置 LLM，再产生行级 review comment 的 AI Code Review CLI；它的最终缺陷判断仍来自 LLM，确定性工程主要约束范围、分组、定位与输出。[OpenCodeReview README](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/README.md#L42-L46) [设计原则](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/README.md#L82-L100)
- zvec 是进程内向量数据库，提供向量、全文、标量过滤和混合检索，适合“找相关资产”，不适合“证明规则已被完整执行”。[zvec README](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/README.md#L35-L59)

因此，最合适的结合方式是：

```text
Standards / Scenarios / Verified Rules / Feedback
                     |
      ERF-owned hybrid retrieval
                     |
        Authoring context (non-authoritative)
                     |
       ERF AI authoring orchestration
                     |
        untrusted candidate suggestions
                     |
     RuleIntent -> IR -> Backend -> Verification
                     |
       signed RuleRelease -> rf-runner -> Gate

  上方两层可换、可失败；下方执行链不依赖它们。
```

## 1. OpenCodeReview 值得借鉴什么

### 1.1 确定性骨架包围不确定性 Agent

OpenCodeReview 的流程是 bootstrap → diff 生成 → 确定性过滤/规则匹配 → 语义分组 → 并行子任务 → 结果定位/过滤/输出。[官方架构文档](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/pages/src/content/docs/en/architecture.md#L12-L35) 这与 ERF “确定性 Framework 管边界，AI 只生成 candidate”的原则同向，可借鉴：

1. 在调用模型前冻结输入范围，为每个排除项记录可机器读的 reason；
2. 先按文件/语义单元分解任务，对超限输入 fail/partial 而非静默跳过；
3. 将“模型内容判断”与“路径、范围、定位、状态转换”分离；
4. 任何 LLM 分组失败都回退到一文件一组，不因优化器失败丢覆盖。OpenCodeReview 对分组设置最大 10 文件、token 上限、未分配文件单独分组，并在模型返回失败时回退。[分组与回退机制](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/pages/src/content/docs/en/architecture.md#L96-L119)

### 1.2 Coverage manifest 的状态机思路

OpenCodeReview 的 `ocr.run-manifest/v1` 把终态定义为 `complete / partial / failed / skipped`，并把 selected 作为分母，要求它等于 completed、reused、failed、waived 四个互斥集合的并集。[状态与 coverage 数据结构](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/internal/session/manifest.go#L159-L188) [分母不变式](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/internal/session/manifest.go#L228-L237) 这对 ERF 的 Completeness/Evidence 很有价值：先封存应执行集合，再允许每个单元只进入一个终态，最终状态不从 finding 数量推断。

但只能借鉴状态机，不能直接复用其完整性语义：OpenCodeReview v1 manifest 只接入 diff `review`，其 `scan` 仍无 manifest；而 ERF 需要的分母是 `rule × module/file analysis unit`，并且要表达 dependency、bytecode、timeout、worker crash、report parse 等后端完整性。[OpenCodeReview 的 v1 限制](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/internal/session/manifest.go#L20-L27) [ERF Completeness 模型](https://github.com/Bandersnatch0x/ruleops/blob/730fefd31b71e7eca1552b709c370922110d1e18/docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md#L813-L871)

### 1.3 规则分层、预览和结果定位

OpenCodeReview 支持 CLI override、project、global、system 四层 prompt rule，通过有序 path glob 为每个文件选择规则，并提供无模型消耗的 preview。[规则优先级与格式](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/pages/src/content/docs/en/review-rules.md#L7-L60) [确定性选择门](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/pages/src/content/docs/en/review-rules.md#L78-L109) ERF 可借鉴“resolve 后生成不可变 effective view”和“在执行前可解释为何选/不选”，但 ERF 的权威规则必须仍是 typed RuleIntent/IR、CapabilityDescriptor、Policy 和 signed bundle，不能降格成 path-to-prompt。

OpenCodeReview 还在 LLM 输出后做两次行号解析、可选重定位和 review-filter，以 JSON 输出。[评论处理链](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/pages/src/content/docs/en/architecture.md#L276-L301) 这可用于 ERF Authoring UI 中将 candidate 稳定锚定到样本源码；但 LLM review-filter 不能替代独立 holdout/near-miss/mutation/shadow verification。

### 1.4 可观测性与隐私默认值

OpenCodeReview 的 telemetry 只导出请求数、延迟、状态等形状数据，不导出 prompt/response；但完整会话记录保存在本地 JSONL，而显式开启 raw logging 后会把完整请求/响应 body 写入本地磁盘。[官方 telemetry 文档](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/pages/src/content/docs/en/telemetry.md#L186-L210) ERF 可借鉴 shape-only telemetry，同时必须为会话和 raw log 设置存储边界、脱敏、TTL 和加密策略。

## 2. OpenCodeReview 能力吸收边界

### 决策：重新实现能力，不集成产品

ERF 在自有 Authoring 边界中实现所需能力：

- 冻结输入范围并为排除项保存机器可读 reason；
- 确定性选择和分组，失败时回退且不丢失覆盖；
- 使用固定分母和互斥终态表达 authoring task completeness；
- 结果定位、结构化输出、shape-only telemetry 与 AuthoringSuggestion 信任标记。

一手资料证明 OpenCodeReview 具有 CLI JSON/SARIF 输出，但这些只用于研究其行为和建立 ERF 对照测试，不是产品集成面。[CLI 范围](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/README.md#L140-L166) [CI JSON 接口](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/pages/src/content/docs/en/integrations/ci.md#L12-L43) [SARIF 接口](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/pages/src/content/docs/en/integrations/ci.md#L293-L317)

### 明确禁止的复用形式

核心编排位于 `internal/agent` 和 `internal/llmloop`，仓库 module 使用 Go 1.25.5。[架构包位置](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/pages/src/content/docs/en/architecture.md#L27-L35) [`go.mod`](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/go.mod#L1-L5) ERF 不 import 其内部包，也不把 CLI JSON/SARIF 或 MCP 作为生产适配面。

局部算法也不直接 vendor；ERF 从可观察行为和公开契约归纳需求，以自有测试驱动独立实现。

## 3. OpenCodeReview 不应复用的部分与边界冲突

| OpenCodeReview 能力 | 与 ERF 的冲突 | 处理方式 |
|---|---|---|
| LLM 直接产生 bug/security finding | ERF 禁止 AI 成为 CI 最终违规裁决器 | 仅 Authoring/Advisory；不映射为 blocking Finding |
| path-to-prompt rule | 没有 Standard/Scenario/Intent/Implementation 身份分离，没有 Backend capability | 可用于上下文选择，不作权威 Rule Asset |
| review-filter/LLM reflection | 仍是模型裁决，不是独立语义验证 | 不计入 EvidenceBundle 的合格证据 |
| file-level review manifest | ERF 必须证明 `rule × analysis unit` 完整性 | 借鉴不变式，重新定义 ERF schema |
| PR inline comments / SARIF | 缺少签名 release、engine lock、baseline、trusted context | 仅作展示/导入 candidate，不作 Gate Evidence |
| 外部 LLM 端点 | 默认会把 diff/文件内容发给 provider，与“代码默认不离开本地/CI 授权域”冲突 | 限内部批准 endpoint/本地模型，或对非敏感样本做隔离 PoC |

OpenCodeReview 官方威胁模型明确它会读取 Git diff 并经 HTTPS 发送到配置的 LLM provider，仓库与 provider 都只被当作 semi-trusted。[安全保证文档](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/ASSURANCE_CASE.md#L5-L39) ERF 则规定正常扫描“rules go to code; code does not go to server”，源码上传必须由显式 Deployment Policy 开启。[ERF Source-Code Locality](https://github.com/Bandersnatch0x/ruleops/blob/730fefd31b71e7eca1552b709c370922110d1e18/docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md#L1694-L1737)

## 4. zvec 到底适合哪些场景

### 适合：非权威的 Authoring Retrieval Plane

zvec 同时提供 dense/sparse vector、FTS、structured filter 和 multi-query reranking；其 Python 实现可对多路查询用 RRF/weighted/callback reranker 合并，并将 filter 下推到查询。[zvec 功能列表](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/README.md#L50-L59) [多路查询与 reranker 实现](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/python/zvec/executor/query_executor.py#L146-L195) 这些能力适合以下明确用例：

1. **新规范入库前的相似资产检索**：以 `standard_id / domain / language / backend / maturity / version / access_scope` 作标量过滤，用 BM25/FTS 命中精确术语，dense vector 找语义改写，RRF 合并；用于回答“是否已有规则/配置可复用”，对应 ERF “先判断是否需要新规则”。
2. **Authoring context retrieval**：为 AI/人类作者取回相关 Standard、Scenario、已批准 RuleIntent、示例、适用范围和后端能力，降低上下文噪声。
3. **Verification corpus discovery**：从大型样本库找与 positive 语义接近但标签为 negative 的候选 near-miss，或对候选用例做聚类/多样性抽样。检索结果只用来建议样本；最终 suite 由 reviewer 批准并冻结。
4. **Feedback 归类和重复问题聚类**：聚合 false positive、exception reason、用户解释和漏报反馈，辅助发现需要升级、拆分或撤回的规则。
5. **Finding explanation 检索**：为开发者找签名 release 对应的权威规范、正反例和 remediation 文档；检索结果必须带回权威 ID/version/digest，不得只返回无来源文本片段。

对中英文代码规范语料，混合检索比只用 dense vector 更合理：Rule ID、Java symbol、API 名、CWE 号、PMD/SpotBugs 类名等需精确词法匹配，同时规范又有语义改写。zvec 的 `Query` 区分 vector 与 FTS 子查询，多路合并时必须指定 reranker。[`Query` 接口](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/python/zvec/model/param/query.py#L33-L103) [合并约束](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/python/zvec/executor/query_executor.py#L146-L168)

### 不适合：生产执行与信任关键链

zvec **不应进入**：

- `rf-runner` 的 Rule Resolver/Scan Planner/Backend Runtime；
- PMD/SpotBugs 是否完成扫描的 Completeness 计算；
- Baseline fingerprint 对比；
- PASS/FAIL/INCONCLUSIVE 决策；
- RuleRelease/Policy/Bundle 的权威存储和签名验证；
- 对“某条规则是否覆盖某段代码”的最终判定。

原因不是 zvec 性能不足，而是向量/ANN 检索的 top-k 是相关性排序，不是完整性证明；embedding 模型、chunking、index 参数和 reranker 变化都会改变结果。ERF 生产 Gate 要求签名 Rule Bundle、engine lock、baseline identity 和 trusted context，并在关键身份不可验时返回 INCONCLUSIVE。[ERF Runner Trust Model](https://github.com/Bandersnatch0x/ruleops/blob/730fefd31b71e7eca1552b709c370922110d1e18/docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md#L1892-L1944)

### 能力实现位置

该能力位于 **RuleOps Server 的 Authoring Search 模块**，由 ERF 定义自有的 query、ACL、provenance 和 index-generation 契约。zvec 官方当前没有 Java SDK，且为 native C++ 核心，这进一步支持不把它引入 Java `rf-runner` 或 Server 产品依赖的决定。[官方 SDK 列表](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/README.md#L61-L79)

zvec 的进程内、多读单写模型只是设计参考。[zvec 并发模型](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/README.md#L53-L59) ERF 自有能力仍必须独立定义 tenant/ACL、备份恢复、密钥管理、调用审计、配额、索引重建和灾备。

## 5. 推荐的能力原型与决策门槛

### Prototype A：ERF Authoring Orchestrator（1—2 周）

**目标**：验证 ERF 自有的确定性选择、分组回退和 AuthoringSuggestion 管道能否降低 Scenario/边界样本发现成本。

**范围**：

1. 以研究中归纳的公开行为编写 ERF 自有 contract tests，不运行 OpenCodeReview 产品。
2. 选 5 条已有人工高质量 RuleIntent/测试的 Java 规范作 retrospective blind set，再选 5 条 Phase 0 新候选规范。
3. 在 2 个 representative repo 中运行 ERF 原型，生成 candidate scenario、positive、negative、near-miss；独立 reviewer 盲评。
4. 原型只输出 `AuthoringSuggestion`，不能写 RuleRelease/Policy，不能改 Gate，不能拿签名权限。

**Go 门槛（建议预注册）**：

- reviewer 认定“可直接进入精修”的 suggestion precision ≥ 80%；
- 在不降低 retrospective set 边界覆盖的前提下，Scenario + test candidate 的人工活跃时间中位数降低 ≥ 25%；
- 已知 critical near-miss 零丢失；任何原型失败/超时/超预算只会使 authoring task 降级，绝不会生成 verified/pass；
- 100% suggestion 可追溯到 source snapshot、model/config identity；零未批准源码外发。

不应把 OpenCodeReview README 中自报的通用 code-review benchmark 直接当作 ERF 规则生产效果：该 benchmark 衡量的是 PR review precision/recall/token，不是 RuleIntent 语义合格率或 RuleRelease 可重复性。[官方 benchmark 声明](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/README.md#L52-L66)

### Prototype B：ERF Hybrid Retrieval（1 周，可与 Prototype A 并行）

**目标**：验证 hybrid retrieval 是否在 ERF 真实语料上明显优于简单 FTS/BM25，避免在只有 100 条 standards 时过早引入向量库。

**范围**：

1. 仅索引可用于 Authoring 的 Standard、Scenario、RuleIntent、已批准 examples/evidence summary 和脱敏 feedback；不索引业务仓库原始源码、密钥或未授权 finding detail。
2. 以 ERF 自有的内存/测试索引实现 FTS、dense candidate 与 metadata filter 对照，不运行或链接 zvec；每条 document 保存 authority ID、version、digest、ACL scope 和 lifecycle state。
3. 建立 40—60 条人工标注查询，覆盖精确 Rule/API ID、中文改写、跨文档同义、旧版排除、ACL 隔离和“无相关结果”。
4. 对比 FTS/BM25 baseline 与 `FTS + dense + metadata filter + RRF` 的 Recall@5、nDCG@5、MRR、p95 latency、index size 和重建时间。zvec 的公开契约只作为测试用例参考；原型必须锁定 model ID/digest、dimension 和 preprocessing 版本。[自定义 embedding protocol](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/python/zvec/extension/embedding_function.py#L22-L70)

**Go 门槛（建议预注册）**：

- 相对 FTS/BM25 baseline，Recall@5 至少提高 10 个百分点，且 ID/API 精确查询不退化；
- ACL 隔离测试零越权，只返回当前有效且授权的 asset version；
- 目标数据量 10 倍压测下 p95 ≤ 100 ms，全量可重建，故障时 Authoring 回退到 FTS/直接 ID lookup；
- embedding/chunking 更改会创建新 index generation，查询日志可记录 generation/model identity；
- 零未批准数据出域，且 Authoring Retrieval 不影响 Verification/Release/Runtime/Gate 的可用性。

若检索质量改善小于门槛，保留 ERF 自有 FTS/BM25；在资产量、反馈量或语义改写需求显著增长后再重试语义检索能力。

### 原型的最小自有契约

两个 ERF 自有能力共享一个极小输出契约：

```yaml
kind: AuthoringSuggestion
schemaVersion: erf.authoring/v0alpha1

source:
  capability: authoring-orchestrator | hybrid-retrieval
  capabilityVersion: "..."
  sourceSnapshot: "sha256:..."
  configDigest: "sha256:..."
  modelIdentity: "..."       # 未使用模型时为 null
  indexGeneration: "..."     # 未使用索引时为 null

scope:
  standardIds: ["..."]
  repositoryIdentity: "sha256:..."
  permittedDataClass: "..."

suggestions:
  - type: scenario | positive | negative | near_miss | explanation | related_asset
    content: "..."
    sourceRefs: ["asset-id@version#digest"]
    confidence: null          # 不把模型自报分数当作信任证据

trust:
  authoritative: false
  independentlyReviewed: false
  usableForGate: false
```

该契约必须满足以下不变式：

- 它不得被解析为 `Finding`、`EvidenceBundle`、`RuleRelease` 或 `GateDecision`；
- `sourceRefs` 只允许引用当前调用者有权访问的资产；
- 缺少 snapshot/config/model/index identity 时，整个输出丢弃，不做默认补全；
- 用户接受 suggestion 后仍需创建新的、可审查的 RuleIntent/测试变更，不直接改写权威对象；
- capability 失败、超时、检索空结果和模型拒答都是 authoring 降级，不改变任何现有成熟度或 Gate 状态；
- 对外展示时明确标识“AI/retrieval suggestion, not verified evidence”。

## 6. License、供应链与数据安全风险

### License

两个仓库均以 Apache-2.0 发布，因而允许商业使用、修改和再分发，但要遵守 LICENSE/归属/修改声明等条件。[OpenCodeReview LICENSE](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/LICENSE#L1-L16) [zvec LICENSE 与 NOTICE 条款](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/LICENSE#L100-L121)

zvec 自带 `NOTICE`，列出 Unicode 数据和改造自 pyglass 的组件及其单独许可文本；发行包或派生作品必须保留适用的 NOTICE。[zvec NOTICE](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/NOTICE#L1-L18) [pyglass 归属](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/NOTICE#L60-L96) 上线前仍应由法务/OSS compliance 流程审核，本研究不构成法律意见。

### 供应链

**OpenCodeReview：中等风险，可通过锁版本+验证降低。** 官方 release workflow 为二进制生成 SHA-256 清单并调用 GitHub build provenance attestation，npm installer 会校验下载的二进制。[发布校验与证明](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/.github/workflows/release.yml#L139-L160) [installer checksum 校验](https://github.com/alibaba/open-code-review/blob/e556dfad56f11afe15cce9b1a0ca7f8993a65134/scripts/install.js#L192-L243) 但 checksum 与二进制来自同一 release channel，ERF 仍应：

- 在企业 artifact mirror 中按 digest 锁定二进制，验证 provenance；
- 生成/收集 SBOM，扫描 Go/npm/GitHub Action 依赖；
- 不在生产中使用 `@main` 或 `latest`；
- 在 sandbox 中执行，不赋予 Release Signing/Policy 写权限。

**zvec：在进入企业制品前风险较高。** 它是 native C++ 组件，包含大量 Git submodule/三方库；仓库的 Python metadata 仍将开发状态标为 `Alpha`，当前稳定 tag 为 0.x。[zvec submodules](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/.gitmodules#L1-L59) [Python package maturity](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/pyproject.toml#L4-L31) [v0.7.0 Release](https://github.com/alibaba/zvec/releases/tag/v0.7.0) 官方 release workflow 检查了平台产物完整性、嵌入版本和 Jieba 数据，但在所评估的 workflow 中未见 checksum 清单、SBOM 或 provenance attestation 步骤。[官方 release workflow](https://github.com/alibaba/zvec/blob/027e7d5b4434db62de4d96f3bcf1864a2cadf776/.github/workflows/release.yml#L22-L143) 建议：

- PoC 锁定 `v0.7.0` 和确切 wheel/archive digest；正式使用前优先在受控 CI 从锁定 source/submodule commit 重建；
- 为 native artifact 生成 SBOM/provenance，做 CVE、license、恶意代码和平台兼容扫描；
- 在内部镜像重签名，通过薄 adapter 隔离 0.x API 变更；
- 预先测试 crash recovery、index rebuild、backup/restore、版本升降级和 CPU/OS 兼容矩阵。

### 数据安全

1. **OpenCodeReview 数据出域**：代码 diff/文件可被发给模型 provider；必须受 Deployment Policy 、repository classification 和 provider allowlist 约束。严禁把 OCR 默认接入所有 representative repo。
2. **本地 transcript/raw log**：本地记录可包含源码、prompt 和 response；默认关闭 raw logging，对 session 目录做最小权限、磁盘加密、定期删除和敏感信息扫描。
3. **zvec 索引是敏感派生数据**：embedding、metadata、document text 和查询日志都按原文数据等级管理；索引目录放在加密卷，通过 OS/service account 做 ACL，租户或业务域间分 collection/分进程。
4. **embedding 数据出域**：zvec 库本身可本地运行，但其可选 HTTP/OpenAI/Qwen/Jina embedding 可把文本发往外部端点；敏感语料必须使用批准的本地/内网模型，并锁定模型与预处理版本。
5. **prompt injection/污染**：只索引经 ACL 和 provenance 验证的权威资产；把 retrieved text 标记为 data 而非 instruction；保留 asset ID/version/digest；用毒化文档测试证明检索结果不能授予签名、发布或 Policy 权限。

## 7. 建议的产品决策

| 决策项 | 建议 | 进入时间 |
|---|---|---|
| OpenCodeReview 的确定性选择、分组回退、coverage manifest、行定位、shape-only telemetry 设计 | **借鉴** | 立即进入 Factory/Runtime 设计 review checklist |
| OpenCodeReview CLI/库/MCP/Fork | **不集成** | 仅保留研究快照和对照测试 |
| OpenCodeReview 结果作 Gate Finding/Evidence | **拒绝** | 长期保持边界；除非未来另定 advisory product |
| zvec 的 hybrid retrieval、filter、reranking 能力 | **吸收为 ERF 自有契约** | 需求达到门槛后实现 |
| zvec 库/进程/CLI/MCP/Fork | **不集成** | 仅作一手设计参考 |
| zvec 进入 `rf-runner`/Gate/Baseline/签名链 | **拒绝** | 架构级禁止 |

总体上，OpenCodeReview 为 ERF 提供的最大价值是**可观察、可对照的 AI review orchestration 设计样本**；zvec 提供的最大价值是**hybrid retrieval 的能力模型与测量基准**。ERF 吸收这些能力，但所有契约、代码、制品和运维责任都保持自有，并不改写信任主线：`typed intent -> deterministic backend -> independent verification -> signed release -> complete execution -> trusted gate`。
