# 域 08：计划与验收只读审计

## 域范围

- 标的：`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md` §30–39，1538–2325 行（Appendix A 不纳入）。下文规格行号均指该文件。
- 实证树：WSL `kali-linux2` 的 `/home/kali/文档/ruleops`，不是 Windows 工作树；读取时 HEAD 为 `7e7d2820de5f2948bd936b64f5907ae211f00645`。
- 方法：规格声明 → 1–2 个仓库证据 → 判定；ADR 核查的是独立文件存在及主题对应，不代表其全部运行语义已验收。
- 按协调者强制落盘指令停止继续核验。尚未完成的逐声明实证明确标为「无法验证」，没有把有限检索无结果推断为全仓未实现。

## 逐声明核验表

### §30 外部研究、§31 MVP

| 声明（规格位置） | 证据 | 判定 | 理由 |
|---|---|---|---|
| §30 PMD，1542–1554：复用分析引擎，不建 AST IDE | `runtime/host-client/tests/java_host.rs:71`；`backend-hosts/java/src/test/java/io/ruleops/javahost/PmdBackendAdapterTest.java:66` | 部分一致 | 有 PMD 接入测试；未穷举产品界面以证明完全没有 AST IDE。 |
| §30 ast-grep，1556–1566：未来 Backend，不自建多语言 parser | 本轮未追查对应依赖及实现 | 无法验证 | 未来方向不是当前必交付；不判缺陷。 |
| §30 Semgrep，1568–1580：能力必须显式建模 | `scripts/phase2-independent-acceptance.sh:30-45` | 部分一致 | 套件明确包含 required capability 不满足及 major 不兼容拒绝；未核验所有 Backend 能力语义。 |
| §30 Error Prone，1582–1586：可运行与可 blocking 分离 | `docs/pilot/phase4-pilot-report.md:21-30` | 部分一致 | 当前资格证据与生产授权明确分离；未追成熟度所有状态迁移。 |
| §30 openHAB，1588–1594：统一治理多个原生 Backend | `runtime/host-client/tests/java_host.rs:71,140` | 部分一致 | PMD、SpotBugs 都有宿主集成测试入口。 |
| §30 OpenCodeReview，1596–1608：自有契约、失败不丢覆盖、authoring-only | `control-plane/authoring/src/test/java/io/ruleops/authoring/AuthoringOrchestratorTest.java:79,96`；`docs/adr/0017-absorb-capabilities-without-product-dependencies.md:5` | 部分一致 | 自有编排有失败覆盖测试；未执行依赖闭包检查证明无外部嵌入。 |
| §30 zvec，1610–1618：自有 BM25、ACL、非权威建议 | `control-plane/authoring/src/test/java/io/ruleops/authoring/AuthoringRetrievalTest.java:70,91` | 部分一致 | 检索测试显式覆盖确定性 BM25、不可信建议及权限过滤；未穷举运行链依赖。 |
| §31.1，1624–1630：非专家降本、可信资产、可靠新增违规阻断 | `specs/pilot/author-cost-baseline.yaml:9-19`；`.scratch/gate/phase4-pilot-report/phase4-pilot-report.md:58-72` | 部分一致 | 工程资格已建立；AuthorCostBaseline 仍为 INCONCLUSIVE，产品级效果和生产授权未完成。 |
| §31.2，1632–1657：Java、100 inventory、20 candidates、3 repos、typed templates 等 MVP | `docs/pilot/phase0-template-selection.md:5-19`；`.scratch/gate/phase4-pilot-report/phase4-pilot-report.md:26-45` | 部分一致 | 已有 100/20/3 计数与三模板证据；三仓库计数资格不等于 live outcome，未逐项验收所有 MVP 子项。 |
| §31.3，1659–1677：W1 预注册混合契约且 W4 Go/No-Go 先于扩平台 | `specs/pilot/pilot-success-contract.yaml:8-44`；`specs/pilot/author-cost-baseline.yaml:9,49-52` | 部分一致 | 已批准效率/质量/安全阈值及 2026-10-02～08 窗口；未找到本轮可确认的 W1 事前锁定与 W4 Go 裁决证据，效率实测仍 INCONCLUSIVE。不能据此断言已违背历史顺序。 |

### §32 阶段计划、§33 团队

| 声明（规格位置） | 证据 | 判定 | 理由 |
|---|---|---|---|
| Phase 0，1683–1704：100 inventory、配置盘点、20 场景、Ledger、3 repos、基线、成功契约、模板、威胁模型 | `scripts/check.sh:70-89`；`docs/pilot/phase0-template-selection.md:5-29` | 部分一致 | 门禁显式覆盖盘点、契约、候选分类、仓库与基线；本轮未逐条打开威胁模型及全部数据。 |
| Phase 0 Exit：候选分类、gap 区分、预锁成功契约 | `docs/pilot/phase0-template-selection.md:9-13`；`specs/pilot/pilot-success-contract.yaml:38-44` | 部分一致 | 候选分类明确，部分 gap 为 0；无法核实最初 W1 锁定时间及未经事后调整。 |
| Phase 1，1706–1736：本体、协议、fake、Intent/IR、模板、PMD、compiler、verification、CLI 与最薄 runtime | `control-plane/`、`runtime/`、`backend-hosts/` 目录；`scripts/check.sh:35-45` | 部分一致 | 对应模块及构建/宿主/协议验收入口存在；目录存在不证明每项实现完成，剩余细项未验证。 |
| Phase 1 Exit：Standard→Intent→IR→PMD→Tests 并首次 Go/No-Go | `control-plane/factory/src/test/java/io/ruleops/factory/CandidateCompilerTest.java:82`；`specs/pilot/author-cost-baseline.yaml:9` | 部分一致 | 确定性编译测试存在；首次产品 Go 决策未证实。 |
| Phase 2，1738–1761：Java Local、SAFE_STATIC、typed observations、独立套件、holdout/near-miss 等 | `scripts/phase2-independent-acceptance.sh:30-96`；`control-plane/verification/src/test/java/io/ruleops/verification/LocalVerificationTest.java:108-113` | 部分一致 | 31 项协议/local gate 套件及 holdout 失败断言存在；实际 sandbox 验收明确留给 Trusted CI。 |
| Phase 2 Exit：≥5 VERIFIED、独立可复核、仅 LOCAL_UNATTESTED/SHADOW | `scripts/check.sh:44-49`；`scripts/phase2-independent-acceptance.sh:77-96` | 部分一致 | 五规则出口脚本接入总门禁，独立报告明确 Shadow；未成功读取五规则具体历史报告，不宣称已证明五条 VERIFIED。 |
| Phase 3，1763–1780：OCI、Bootstrap/Release/Lock/Trust、签名资产、staging、trusted CI/status | `.scratch/gate/phase3-trusted-ci/phase3-trusted-ci.md:23-35`；`.scratch/gate/phase3-trusted-scan/phase3-real-scan.md:15-25` | 部分一致 | 信任闭包及真实 Runner handoff 有历史 PASS；17 项 harness 明示不拉 registry、不启动 OCI、不调用 SCM、不配凭证，不能替代部署证明。 |
| Phase 3 Exit：十次同 digest、拒篡改、失败不绿、独立 verifier | `.scratch/gate/phase3-trusted-scan/phase3-real-scan.md:7-23`；`.scratch/gate/phase3-trusted-ci/phase3-trusted-ci.md:23-31` | 部分一致 | 实际 Java 路径十次摘要相同及失败 ActionRequired 有证据；篡改清单未逐项覆盖原文全部对象。 |
| Phase 4，1782–1797：Registry、Policy/Exception、audit、feedback、report、runbook | `scripts/phase4-pilot-report.sh:11-19`；`docs/runbooks/phase4-trusted-pilot.md:3-38` | 部分一致 | 有聚合门禁与运维手册，但报告仍 Shadow-only。 |
| Phase 4 Exit：3 Shadow、≥10 qualified、无假 PASS、债务分离、共享 conformance、确定性本体、隐私、可信分支 | `.scratch/gate/phase4-pilot-report/phase4-pilot-report.md:26-56`；`scripts/phase2-independent-acceptance.sh:85-96` | 部分一致 | 工程资格报告为 20 规则，fake 共享套件明确；生产 enforcement disabled，未对受保护分支做 live 验收。 |
| §33，1800–1810：4 核心人员+0.5 共享、48 person-weeks | 未检查真实人员及工时记录 | 无法验证 | 属建议，不把未有人员档案判作代码缺陷。 |

### §34 Acceptance Criteria（25 项）

| 声明（行号） | 证据 | 判定 | 理由 |
|---|---|---|---|
| 1 非专家模板完成（1818） | `specs/pilot/author-cost-baseline.yaml:9-19` | 无法验证 | 人员实测未完成；编译成功不能替代非专家完成率。 |
| 2 歧义必须 clarify（1819） | `control-plane/coverage/src/test/java/io/ruleops/coverage/CoverageAnalyzerTest.java:62,70` | 部分一致 | 歧义分类 blocked 和 needs-clarification 有测试片段，完整交互未核验。 |
| 3 FQN/static import/同名类型语义（1820） | 对 Java Test 的 `staticImport/fullyQualified/sameName/FQN` 有界检索未获充分证据 | 无法验证 | 未找到实现证据，不断言未实现。 |
| 4 PMD/SpotBugs 加载与 location（1821） | `runtime/host-client/tests/java_host.rs:71,140`；`backend-hosts/java/src/test/java/io/ruleops/javahost/SpotBugsBackendAdapterTest.java:22` | 部分一致 | 两后端及 typed bytecode observation 有测试；未详查全部 location 断言。 |
| 5 缺依赖/stale bytecode→INCONCLUSIVE（1822） | `backend-hosts/java/src/test/java/io/ruleops/javahost/SpotBugsBackendAdapterTest.java:56` | 部分一致 | 缺失/非 class 输入 fail-closed 有测试；缺依赖与 stale bytecode 完整链未验证。 |
| 6 依赖变化未改行问题（1823） | `runtime/affected-scope/src/lib.rs:1237,1307` | 部分一致 | 有反向依赖闭包与配置变化扩大范围测试；没有完成真实新 Finding 端到端确认。 |
| 7 move/copy fingerprint（1824） | 有界搜索未找到充分 fingerprint 断言 | 无法验证 | rename 扫描范围测试不等于 fingerprint move/copy 语义。 |
| 8 rule upgrade 与债务分离（1825） | `runtime/gate/src/lib.rs:687` | 部分一致 | 精确 baseline 历史 Finding 测试存在；upgrade 专项未验证。 |
| 9 PR gate config 不绕 trusted policy（1826） | `runtime/status/src/lib.rs:603` | 部分一致 | 不同 Policy 证明拒绝测试存在；没有执行恶意 PR 改配置的集成验收。 |
| 10 timeout/crash/empty 不 green（1827） | `scripts/phase2-independent-acceptance.sh:38-40,60-61`；`.scratch/gate/phase3-trusted-scan/phase3-real-scan.md:18-19` | 部分一致 | crash/不完整已测；空报告具体路径未核验。 |
| 11 篡改 artifact 拒绝（1828） | `runtime/trust/src/lib.rs:1102`；`.scratch/gate/phase3-trusted-ci/phase3-trusted-ci.md:23-27` | 部分一致 | 有 artifact 篡改测试及若干信任对象拒绝记录；未穷举资产类型。 |
| 12 FP→regression+新版本（1829） | `runtime/feedback/src/lib.rs:275-309` | 部分一致 | 断言 FP 创建 MustBeClean regression 并记入 ledger；到新版本发布未核验。 |
| 13 revoke/emergency override 可审计（1830） | `.scratch/gate/phase4-pilot-report/phase4-pilot-report.md:51-52` | 部分一致 | rollback 与 audit 篡改测试报告存在；emergency override 未验证。 |
| 14 固定 snapshot 稳定（1831） | `.scratch/gate/phase3-trusted-scan/phase3-real-scan.md:7-17` | 一致 | 已存真实 Runner 报告十次相同 digest；仅历史测试证据，不代表本轮重跑。 |
| 15 forged local/stale head 拒绝（1832） | `runtime/status/src/lib.rs:540,566`；`.scratch/gate/phase3-trusted-ci/phase3-trusted-ci.md:28` | 部分一致 | stale/cross-commit/tampering 检查存在，真实 SCM head 联动未验证。 |
| 16 删除规则/exclude 不作债务减少（1833） | 有界检索未获对应验收断言 | 无法验证 | 未找到实现证据。 |
| 17 同输入十次 SemanticResultDigest（1834） | `.scratch/gate/phase3-trusted-scan/phase3-real-scan.md:7-17` | 一致 | 真实 Java 扫描历史验收明确 ten_repeatable PASS，不仅依赖 fake-host 脚本。 |
| 18 crash/timeout/漏单元/stale metadata 无 PASS（1835） | `scripts/phase2-independent-acceptance.sh:38-40,60-61`；`.scratch/gate/phase3-trusted-ci/phase3-trusted-ci.md:29-31` | 部分一致 | 失败/不完整及过期 metadata 有检查；未逐个追查全链。 |
| 19 Bundle/Pack/Lock/本体/Runner 篡改拒绝（1836） | `.scratch/gate/phase3-trusted-ci/phase3-trusted-ci.md:23-27` | 部分一致 | Lock、Runner、Backend Pack 有拒绝记录；Bundle/本体尚未在此证据中逐项确认。 |
| 20 过期/回滚 checkpoint 拒绝（1837） | `runtime/trust/src/lib.rs:1273`；`runtime/trust-state/src/lib.rs:547` | 部分一致 | 对应专项测试明确存在，本轮未读完整断言/运行结果。 |
| 21 PMD→Finding/Baseline/Exception/GateAttestation（1838） | `.scratch/gate/phase3-trusted-scan/phase3-real-scan.md:15-25`；`scripts/phase2-independent-acceptance.sh:59` | 部分一致 | 真实 Finding trusted handoff 与 baseline/exception 正交检查都有，但未确认单次同路径串全四者。 |
| 22 独立 Status Publisher 三态（1839） | `.scratch/gate/phase3-trusted-ci/phase3-trusted-ci.md:17-20,35` | 部分一致 | 独立验证/三态模型有记录；harness 明示不调用 SCM API，不能宣称真实发布已验收。 |
| 23 Fake Node Host/Resolver 共享套件（1840） | `scripts/phase2-independent-acceptance.sh:22-28,63-96` | 一致 | 同一 expected_checks 对 Rust/Node 日志强制 31/31，Host/Resolver 均覆盖。 |
| 24 本体契约可确定性重建（1841） | `control-plane/ontology/src/test/java/io/ruleops/ontology/OntologyContractPackCompilerTest.java:18-24` | 部分一致 | 同 revision 重编检查 digest/canonicalTriples 一致；不是完整发布 pack 的 clean-build 两次证明。 |
| 25 默认无源码/完整 Finding 上传（1842） | `.scratch/gate/phase4-pilot-report/phase4-pilot-report.md:55-56`；`runtime/feedback/src/lib.rs:308-309` | 部分一致 | 报告声明 summaries/digests，feedback 有隐私断言；未做全部上传出口流量核验。 |

### §35 Metrics

| 声明 | 证据 | 判定 | 理由 |
|---|---|---|---|
| Product 六指标，1848–1855 | `specs/pilot/author-cost-baseline.yaml:9-19`；`docs/pilot/phase4-pilot-report.md:35-46` | 部分一致 | Author Active Time 有定义但 INCONCLUSIVE；覆盖计数存在，Time-to-Rule、两种 conversion 实测未验证。 |
| Quality 六指标，1857–1864 | `specs/pilot/pilot-success-contract.yaml:20-23`；`specs/pilot/pilot-metrics-baseline.yaml:35-49` | 部分一致 | precision/recall 阈值及 complete/FP 指标有定义；FN sampled rate 与全套实测未验证。 |
| Operations 六指标，1866–1873 | `specs/pilot/pilot-metrics-baseline.yaml:42-84`；`.scratch/gate/phase4-pilot-report/phase4-pilot-report.md:58-69` | 部分一致 | 定义 inconclusive/latency/resource/rollback/exception；revoked release count 未核验，live 指标仍未宣称完成。 |
| Business 五指标，1875–1881 | `specs/pilot/pilot-metrics-baseline.yaml:21-28`；`.scratch/gate/phase4-pilot-report/phase4-pilot-report.md:62-69` | 部分一致 | 新违规/债务 trend 定义存在；修复工时、review workload、cost per qualified scenario 未验证。 |
| 禁止生成规则数作为主成功指标，1883 | `specs/pilot/pilot-success-contract.yaml:15-33`；`.scratch/gate/phase4-pilot-report/phase4-pilot-report.md:31-43` | 一致 | 成功契约为效率/质量/安全/运营/outcome；计数 qualification 不冒充产品结果。 |

### §36 Key ADRs：89 项编号与主题逐项比对

共列 ADR-001～089。独立文件匹配 79 项（011～089），缺少 10 项（001～010）；四位文件前缀与三位正文编号是表示差异，不是缺失。以下“一致”仅指独立决策记录的编号/主题对应；ADR-011 已正确标记被 ADR-027 取代。后续 0090+ ADR 存在，不属于本节列出的 89 项，未将其计入缺失或替代覆盖。

| 声明（§36 原文行号） | 证据 | 判定 | 理由 |
|---|---|---|---|
| ADR-001 Reuse analysis engines（1889） | `docs/adr/` 全文件名清单：无 `0001-*` | 不一致 | 仅规格内摘要，未找到对应独立 ADR 文件；不等于业务功能未实现。 |
| ADR-002 Intent / Implementation separation（1893） | `docs/adr/` 全文件名清单：无 `0002-*` | 不一致 | 仅规格内摘要，未找到对应独立 ADR 文件；不等于业务功能未实现。 |
| ADR-003 Typed finite IR（1897） | `docs/adr/` 全文件名清单：无 `0003-*` | 不一致 | 仅规格内摘要，未找到对应独立 ADR 文件；不等于业务功能未实现。 |
| ADR-004 AI is authoring-only（1901） | `docs/adr/` 全文件名清单：无 `0004-*` | 不一致 | 仅规格内摘要，未找到对应独立 ADR 文件；不等于业务功能未实现。 |
| ADR-005 Evidence before promotion（1905） | `docs/adr/` 全文件名清单：无 `0005-*` | 不一致 | 仅规格内摘要，未找到对应独立 ADR 文件；不等于业务功能未实现。 |
| ADR-006 Tri-state gate（1909） | `docs/adr/` 全文件名清单：无 `0006-*` | 不一致 | 仅规格内摘要，未找到对应独立 ADR 文件；不等于业务功能未实现。 |
| ADR-007 Baseline separation（1913） | `docs/adr/` 全文件名清单：无 `0007-*` | 不一致 | 仅规格内摘要，未找到对应独立 ADR 文件；不等于业务功能未实现。 |
| ADR-008 Immutable releases（1917） | `docs/adr/` 全文件名清单：无 `0008-*` | 不一致 | 仅规格内摘要，未找到对应独立 ADR 文件；不等于业务功能未实现。 |
| ADR-009 Central governance, distributed execution（1921） | `docs/adr/` 全文件名清单：无 `0009-*` | 不一致 | 仅规格内摘要，未找到对应独立 ADR 文件；不等于业务功能未实现。 |
| ADR-010 CLI/Git first（1925） | `docs/adr/` 全文件名清单：无 `0010-*` | 不一致 | 仅规格内摘要，未找到对应独立 ADR 文件；不等于业务功能未实现。 |
| ADR-011 Java modular monolith for V0.x（1929） | `docs/adr/0011-java-modular-monolith-for-v0x.md:5` — Use a Java modular monolith for V0.x | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-012 Git is authoritative during the pilot（1933） | `docs/adr/0012-git-is-authoritative-during-the-pilot.md:5` — Git is authoritative during the pilot | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-013 Organization-managed signing identities（1937） | `docs/adr/0013-use-organization-managed-signing-identities.md:5` — Use organization-managed signing identities | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-014 Signed immutable Debt Baselines（1941） | `docs/adr/0014-debt-baselines-are-signed-immutable-assets.md:5` — Debt Baselines are signed immutable assets | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-015 Strict three-state Gate（1945） | `docs/adr/0015-keep-gate-decisions-strictly-three-state.md:5` — Keep Gate decisions strictly three-state | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-016 Minimal Runner upload by default（1949） | `docs/adr/0016-minimize-runner-data-upload-by-default.md:5` — Minimize Runner data upload by default | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-017 Absorb capabilities without product dependencies（1953） | `docs/adr/0017-absorb-capabilities-without-product-dependencies.md:5` — Absorb external capabilities without product dependencies | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-018 Lifecycle-aligned modules（1957） | `docs/adr/0018-align-modules-to-ruleops-capabilities.md:5` — Align modules to RuleOps capabilities | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-019 Frozen ScanPlan denominator（1961） | `docs/adr/0019-freeze-the-scan-plan-before-execution.md:5` — Freeze the ScanPlan before execution | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-020 Gate computation and authority are separate（1965） | `docs/adr/0020-separate-gate-computation-from-authority.md:5` — Separate Gate computation from authority | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-021 Evidence-governed maturity transitions（1969） | `docs/adr/0021-govern-maturity-transitions-with-evidence.md:5` — Govern maturity transitions with evidence | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-022 Resolve Policy before distribution（1973） | `docs/adr/0022-resolve-policy-before-distribution.md:5` — Resolve Policy before distribution | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-023 Authoritative ontology compiled for runtime（1977） | `docs/adr/0023-authoritative-ontology-compiled-for-runtime.md:5` — Make the ontology authoritative and compile it for runtime | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-024 Core Ontology and governed extensions（1981） | `docs/adr/0024-govern-a-core-ontology-and-extensions.md:5` — Govern a Core Ontology and extensions | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-025 Approved Semantic Projections（1985） | `docs/adr/0025-require-approved-semantic-projections.md:5` — Require approved Semantic Projections | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-026 RDF, OWL 2 RL and SHACL Core（1989） | `docs/adr/0026-use-rdf-owl-rl-and-shacl-for-the-ontology.md:5` — Use RDF, OWL 2 RL and SHACL Core for the ontology | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-027 Rust Runtime with process-isolated Backend Hosts（1993） | `docs/adr/0027-use-a-rust-runtime-with-process-isolated-backend-hosts.md:5` — Use a Rust Runtime with process-isolated Backend Hosts | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-028 Canonicalize RDF before signing（1997） | `docs/adr/0028-canonicalize-rdf-before-signing.md:5` — Canonicalize RDF before signing | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-029 Version ontology identity and compatibility（2001） | `docs/adr/0029-version-ontologies-with-iris-and-content-digests.md:5` — Version ontologies with IRIs and content digests | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-030 Framed Protobuf over stdio（2005） | `docs/adr/0030-use-framed-protobuf-over-stdio-for-backend-hosts.md:5` — Use framed Protobuf over stdio for Backend Hosts | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-031 Signed Backend Runtime Packs（2009） | `docs/adr/0031-distribute-signed-backend-runtime-packs.md:5` — Distribute signed Backend Runtime Packs | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-032 Split parsing from governance normalization（2013） | `docs/adr/0032-split-backend-parsing-from-governance-normalization.md:5` — Split backend parsing from governance normalization | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-033 Jena-centered ontology toolchain（2017） | `docs/adr/0033-use-a-jena-centered-ontology-toolchain.md:5` — Use a Jena-centered ontology toolchain | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-034 Compile an Ontology Contract Pack（2021） | `docs/adr/0034-compile-an-ontology-contract-pack.md:5` — Compile an Ontology Contract Pack | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-035 Scan-scoped Backend Host processes（2025） | `docs/adr/0035-use-scan-scoped-backend-host-processes.md:5` — Use scan-scoped Backend Host processes | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-036 Global ScanPlan with Host refinement（2029） | `docs/adr/0036-refine-the-global-scan-plan-inside-hosts.md:5` — Refine the global ScanPlan inside hosts | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-037 Hermetic trusted runtime closure（2033） | `docs/adr/0037-require-a-hermetic-trusted-runtime-closure.md:5` — Require a hermetic trusted runtime closure | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-038 Versioned semantic Finding fingerprints（2037） | `docs/adr/0038-use-versioned-semantic-finding-fingerprints.md:5` — Use versioned semantic Finding fingerprints | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-039 Default-deny Backend Host sandbox（2041） | `docs/adr/0039-sandbox-backend-hosts-by-default.md:5` — Sandbox Backend Hosts by default | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-040 Major-versioned additive Host protocol（2045） | `docs/adr/0040-evolve-the-host-protocol-additively-within-a-major-version.md:5` — Evolve the Host protocol additively within a major version | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-041 Typed observation locations（2049） | `docs/adr/0041-use-typed-observation-locations.md:5` — Use typed observation locations | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-042 Git-authoritative ontology with rebuildable indexes（2053） | `docs/adr/0042-keep-ontology-authority-in-git.md:5` — Keep ontology authority in Git | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-043 Reference and embed Ontology Contract Packs（2057） | `docs/adr/0043-reference-and-embed-ontology-contract-packs.md:5` — Reference and embed Ontology Contract Packs | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-044 Resolve an immutable Workspace Descriptor（2061） | `docs/adr/0044-resolve-an-immutable-workspace-descriptor.md:5` — Resolve an immutable Workspace Descriptor | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-045 Static or attested workspace resolution（2065） | `docs/adr/0045-use-static-or-attested-workspace-resolution.md:5` — Use static or attested workspace resolution | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-046 Linux-only OS family for trusted execution in V0.1（2069） | `docs/adr/0046-certify-only-linux-trusted-execution-in-v0-1.md:5` — Certify only the Linux OS family for trusted execution in V0.1 | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-047 Atomically commit successful Host attempts（2073） | `docs/adr/0047-atomically-commit-successful-host-attempts.md:5` — Atomically commit successful Host attempts | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-048 Exact ontology digest in V0.1（2077） | `docs/adr/0048-require-exact-ontology-digests-in-v0-1.md:5` — Require exact ontology digests in V0.1 | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-049 Separate ontology and runtime-pack signing roles（2081） | `docs/adr/0049-separate-ontology-and-runtime-pack-signing-roles.md:5` — Separate ontology and runtime-pack signing roles | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-050 Process-isolated Workspace Resolver Packs（2085） | `docs/adr/0050-run-workspace-resolvers-as-process-isolated-packs.md:5` — Run Workspace Resolvers as process-isolated packs | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-051 Bind and attest Workspace Descriptors（2089） | `docs/adr/0051-bind-and-attest-workspace-descriptors.md:5` — Bind and attest Workspace Descriptors | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-052 Separate attempt transcript and semantic result digests（2093） | `docs/adr/0052-separate-attempt-transcript-and-semantic-result-digests.md:5` — Separate attempt transcript and semantic result digests | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-053 Offline-root delegated trust metadata（2097） | `docs/adr/0053-use-offline-root-delegated-trust-metadata.md:5` — Use offline-root delegated trust metadata | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-054 Rootless OCI as the first Sandbox Provider（2101） | `docs/adr/0054-use-rootless-oci-as-the-first-sandbox-provider.md:5` — Use rootless OCI as the first Sandbox Provider | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-055 Resolve one top-level Execution Lock（2105） | `docs/adr/0055-resolve-one-top-level-execution-lock.md:5` — Resolve one top-level Execution Lock | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-056 DSSE in-toto Workspace attestations（2109） | `docs/adr/0056-use-dsse-in-toto-workspace-attestations.md:5` — Use DSSE in-toto Workspace attestations | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-057 Threshold offline Root with operational role keys（2113） | `docs/adr/0057-use-a-threshold-offline-root.md:5` — Use a threshold offline Root | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-058 Canonical Finding Set v1（2117） | `docs/adr/0058-define-canonical-finding-set-v1.md:5` — Define Canonical Finding Set v1 | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-059 OCI Image Layout for executable packs（2121） | `docs/adr/0059-package-executable-packs-as-oci-image-layouts.md:5` — Package executable packs as OCI Image Layouts | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-060 Bounded append-only attempt staging（2125） | `docs/adr/0060-use-bounded-append-only-attempt-staging.md:5` — Use bounded append-only attempt staging | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-061 Control Plane-signed Execution Locks（2129） | `docs/adr/0061-sign-execution-locks-in-the-control-plane.md:5` — Sign Execution Locks in the Control Plane | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-062 Bind Workspace attestations to CI workload context（2133） | `docs/adr/0062-bind-workspace-attestations-to-ci-workload-context.md:5` — Bind Workspace attestations to CI workload context | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-063 Certify linux/amd64 first（2137） | `docs/adr/0063-certify-linux-amd64-first.md:5` — Certify linux/amd64 first | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-064 Major-version Canonical Finding Sets（2141） | `docs/adr/0064-major-version-canonical-finding-sets.md:5` — Major-version Canonical Finding Sets | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-065 Zero-retention attempt staging（2145） | `docs/adr/0065-delete-attempt-staging-by-default.md:5` — Delete attempt staging by default | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-066 Persist signed offline trust checkpoints（2149） | `docs/adr/0066-persist-signed-offline-trust-checkpoints.md:5` — Persist signed offline trust checkpoints | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-067 DSSE in-toto Gate attestations（2153） | `docs/adr/0067-issue-dsse-in-toto-gate-attestations.md:5` — Issue DSSE in-toto Gate attestations | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-068 Signed immutable Runner Releases（2157） | `docs/adr/0068-publish-signed-immutable-runner-releases.md:5` — Publish signed immutable Runner Releases | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-069 One authoritative implementation per scope（2161） | `docs/adr/0069-select-one-authoritative-implementation-per-scope.md:5` — Select one authoritative implementation per scope | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-070 ERF-owned contract conformance suite（2165） | `docs/adr/0070-require-erf-contract-conformance-evidence.md:5` — Require ERF Contract Conformance Evidence | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-071 Trust metadata plus portable artifact statements（2169） | `docs/adr/0071-combine-trust-metadata-with-portable-artifact-statements.md:5` — Combine TrustMetadata with portable ArtifactStatements | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-072 Independently bootstrap and measure Runner（2173） | `docs/adr/0072-bootstrap-and-measure-runner-independently.md:5` — Bootstrap and measure Runner independently | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-073 Signed immutable Exception Grants（2177） | `docs/adr/0073-govern-signed-immutable-exception-grants.md:5` — Govern signed immutable Exception Grants | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-074 No runtime promotion of Shadow implementations（2181） | `docs/adr/0074-do-not-promote-shadow-implementations-at-runtime.md:5` — Do not promote Shadow implementations at runtime | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-075 Credential-isolated Status Publisher（2185） | `docs/adr/0075-publish-scm-status-through-a-credential-isolated-adapter.md:5` — Publish SCM status through a credential-isolated adapter | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-076 Independently signed Conformance Evidence（2189） | `docs/adr/0076-sign-conformance-evidence-independently.md:5` — Sign Conformance Evidence independently | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-077 Independently managed Runner Bootstrap（2193） | `docs/adr/0077-manage-runner-bootstrap-independently.md:5` — Manage Runner Bootstrap independently | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-078 Closed Exception selector grammar（2197） | `docs/adr/0078-use-a-closed-exception-selector-grammar.md:5` — Use a closed Exception selector grammar | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-079 Strict tri-state SCM status mapping（2201） | `docs/adr/0079-map-gate-states-strictly-to-scm-status.md:5` — Map Gate states strictly to SCM status | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-080 Minimal-disclosure Gate attestations（2205） | `docs/adr/0080-minimize-gate-attestation-disclosure.md:5` — Minimize Gate attestation disclosure | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-081 Revalidate changed conformance combinations（2209） | `docs/adr/0081-revalidate-changed-conformance-combinations.md:5` — Revalidate changed conformance combinations | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-082 Orthogonal Baseline and Exception decisions（2213） | `docs/adr/0082-keep-baseline-and-exception-decisions-orthogonal.md:5` — Keep Baseline and Exception decisions orthogonal | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-083 Ephemeral scoped Gate signing（2217） | `docs/adr/0083-use-ephemeral-scoped-gate-signing.md:5` — Use ephemeral scoped Gate signing | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-084 Four vertical delivery slices（2221） | `docs/adr/0084-deliver-through-four-vertical-slices.md:5` — Deliver through four vertical slices | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-085 No trusted Gate before the trusted slice（2225） | `docs/adr/0085-withhold-trusted-gate-until-trusted-slice.md:5` — Withhold trusted Gate until the trusted slice | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-086 Fake Node contract proof before real JavaScript（2229） | `docs/adr/0086-prove-language-neutrality-with-a-fake-node-host.md:5` — Prove language neutrality with a Fake Node Host | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-087 Minimal ontology for the pilot（2233） | `docs/adr/0087-limit-pilot-ontology-scope.md:5` — Limit pilot ontology scope | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-088 Freeze protocol 1.0 at the trusted pilot（2237） | `docs/adr/0088-freeze-protocol-1-0-at-the-trusted-pilot.md:5` — Freeze protocol 1.0 at the trusted pilot | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
| ADR-089 Deterministic trusted-pilot acceptance（2241） | `docs/adr/0089-use-deterministic-trusted-pilot-acceptance.md:5` — Use deterministic trusted-pilot acceptance | 一致 | 编号及主题对应；仅核验决策文件，未逐项追实现。 |
### §37–39

| 声明 | 证据 | 判定 | 理由 |
|---|---|---|---|
| §37 风险 11 项与缓解，2247–2261 | `scripts/check.sh:74-89`；`scripts/phase2-independent-acceptance.sh:30-61` | 部分一致 | 成功契约、基线、能力/失败套件有落实入口；未逐项核验 SBOM、原生逃生口、成熟度等全部风险缓解。 |
| §38 Quarter +1，2269–2276 | 未逐项检查 Web/ast-grep/Semgrep/dashboard/tenancy/inheritance | 无法验证 | 这是 Pilot 后评估清单，不应因未实现直接计作 MVP 缺陷。 |
| §38 Quarter +2，2278–2285 | 未逐项检查 Error Prone/ArchUnit/语言/IDE/SSO/API/analytics | 无法验证 | 未核验交付与 Pilot 前置条件的历史关系。 |
| §38 真实需求后才考虑四类扩展，2287–2293 | 本轮未核查 CodeQL/Infer、dataflow IR、跨仓规则、自动修复需求证据 | 无法验证 | 不将未来项当当前义务。 |
| §39 RuleOps 生命周期、14 节点核心链，2296–2321 | `control-plane/`、`runtime/` 模块目录；`.scratch/gate/phase4-pilot-report/phase4-pilot-report.md:9-21` | 部分一致 | authoring/verification/runtime/governance/feedback 等有实现及分段报告；未证明同一身份贯穿全部节点的单条全生命周期端到端验收。 |

## 问题清单（按严重度）

### P1：产品成功与工程资格不可混同（验收证据缺口，不是已证实运行漏洞）

1. §31.1/31.3 的降本假设和 W4 Go 未获本轮可确认的完成证据：AuthorCostBaseline 明确 INCONCLUSIVE，已有平台功能不能代替效率/质量对照。无法确认原始 W1→W4 时间顺序，不断言违规扩建。
2. §32 Phase3/4 与 §34.22 的信任/SCM验收需分层：17项本地 harness 明示不拉 registry、不启动 OCI、不调用 SCM、不配置凭证；当前聚合报告关闭生产 enforcement。工程 PASS 不构成 protected-branch/live 发布证明。
3. §34 的 FQN/static import/同名类型、move/copy fingerprint、删规则/exclude 不算债务下降未找到充分专项实证；这些是未验证项，应优先确认，不可直接宣称缺失功能。

### P2：规格追溯缺口

4. §36 ADR-001～010 没有独立 `docs/adr/0001-*`～`0010-*` 文件（10/89）。原文保留其摘要，因此是独立 ADR 追溯缺失，不是决策内容彻底消失；ADR-011～089 均对应，011 supersession 正确。
5. §35 实际指标定义和结果不齐：机器基线为 DEFINITION_ONLY（`specs/pilot/pilot-metrics-baseline.yaml:9`），Product 转化率、FN sampled rate、撤回数量、修复工时/review workload/单场景成本尚未在本轮证实。必须区分未定义、已定义未采集与采集未达标。
6. 人类文档与批准合同有同步风险：`docs/pilot/pilot-metrics-baseline.md:19-22` 仍说 window/sample/threshold 为 HUMAN_INPUT_REQUIRED，而 `specs/pilot/pilot-success-contract.yaml:8,38-44` 已 APPROVED 且指定窗口；可能是刻意保持基线定义边界，但应明确权威解析关系以免被误读为未批准。

## 整改建议

1. 对缺失十项 ADR 选择补独立记录或在 §36 明确“内联历史决定”，避免虚构决策日期；保留与后续 superseding ADR 的链接。
2. 给 §34 的 25 项建立固定 `requirement → test/assertion → evidence run + commit → environment/authority` 索引；优先补本轮无法验证的 3/7/16 及真实 SCM 发布证据，不用泛化 harness 名代替断言。
3. 将 W4 Go、live Pilot outcome、工程 qualification、protected-branch authorization 分开记录；缺实测继续 INCONCLUSIVE，不用规则计数或门禁全绿替代人工效率/质量验证。
4. §32 按“实现痕迹/自动测试/真实环境/业务裁决”标交付状态；§35 给每个指标标定义及采集状态。后续研究路线只在需求与 Pilot 前置条件满足后评估。

## 证据边界

- 本轮只读源码/docs/scripts，仅写本报告；没有运行全量 `bash scripts/check.sh`，不宣称当前 HEAD 全绿。理由：受限审计任务及协调者强制停止指令；运行总门禁还会覆盖 `.scratch/gate` 历史取证。
- `.scratch/gate/*` 是已存历史报告，可能由并行工作覆盖；未锁定全部原件或证明各报告均属于读取时 HEAD。报告中的 PASS 只说明该证据声称的检查结果。
- ADR 文件存在性与标题核验已覆盖本节 89 项；未逐条追踪其实现状态。其余章节对部分复合声明做了合并核验，未检验的子项不算已验收。
- 未查人员工时、真实企业受保护分支配置、生产凭证、完整外部仓库观察期与人工判定记录；没有用目录/测试函数名推断完整功能正确。
- 有界检索无结果写作“未找到实现证据/无法验证”，不是不存在证明。未来计划不判当前漏实现。没有改源码、规格或脚本，没有向 GitHub/Slack 发消息。