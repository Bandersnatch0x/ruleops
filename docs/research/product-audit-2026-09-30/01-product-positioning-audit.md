---
title: Product Positioning Audit
date: 2026-09-30
repository: ruleops
head: ae61fc494057a4328dfc51369971ded84f50d2a0
branch: codex/phase1-contract-factory
status: audit-only
scope: repository-evidence
skill: D:/code_space/.orca-workspaces/agent-skills/skills/product/product-positioning-audit/SKILL.md
---

# 产品定位审计

结论：保留“执行层”主线，但当前有证据支持的交付是 **Java 本地 / Shadow 扫描完整性与结果解释**，不是生产可信强制执行。创作工厂和治理取证已有实现，不能据此推出用户愿意采购完整 RuleOps 平台；真实作者收益和正式窗口结果仍缺证据。[E1][E2][E3][E12]

本报告按指定技能的六步顺序执行，只审计、不改产品与既有裁决。候选定位、取舍及实验均为待验证建议，不是新裁决。编号证据见 §9；结论不以旧审计代替当前文件核查。

## 1. 当前定位原文

README 开篇逐字引用，保留原始换行与 Markdown 强调（`README.md:3–5`）：

```text
Enterprise Rule Factory (ERF) — 面向平台工程与安全/合规工程团队的**可信规则执行层**：用冻结
ScanPlan 的 Completeness、PASS/FAIL/INCONCLUSIVE 三态 Gate 与签名执行证明，取代「0 告警即通过」，
把企业代码规范转化为可验证、可审计、可演进的机器检查资产。
```

根包描述逐字引用（`pom.xml:13`）：

```text
Java 21 modular monolith hosting Factory-side RuleOps capabilities (ADR-0018; process-isolated engines per ADR-0027).
```

必须一起读的限定：README:7–9 已明确禁用 release/gate/promote 与 SCM 回写，权威为 `LOCAL_UNATTESTED` / `SHADOW`；ADR-0167 已收窄承诺，不能再把旧白皮书远景当作当前交付。[E1][E2]

## 2. 现实中的用户可见表面

`works`：所列**限定行为**有实现及测试/演练证据，不等于本次全量构建通过或生产可用；`half-built`：局部实现存在，但用户交付链仍缺通用化、安装、部署或真实使用闭环；`skeleton`：仅入口/占位而无实质行为。本次没有将有行为的本地契约误标为 skeleton。来源、测试源码、本次实测与历史演练分开记录。

按用户任务合并为 **12 个等权功能组**，作为候选比例的唯一分母，不按 crate、命令别名或代码量放大功能数。

| ID | 真实命令 / 接口 / 页面 | Status | 已存在的行为与限定；证据 |
| --- | --- | --- | --- |
| F1 | `rf init`、`config validate/show`、`doctor` | works | 初始化并冻结 Java 项目配置，报告依赖/缓存/信任问题；公开仓库演练使用过 init、validate、doctor。未知解析不自动变可信，本次未重新初始化。[E1][E3][E4] |
| F2 | `rf scan`：text / JSON / SARIF、进度、Candidate / delivery 输入 | works | 真实 Runner、Finding、Completeness；9 月 26 日演练有 183/183 单元、13 条 Finding 的 FAIL 及整改复扫。仅认定本地 / Shadow，不认定生产 Gate 或所有 Java 构建。[E3][E5] |
| F3 | `rf standard validate`、`rule validate/generate/test` | works | 三类模板编译 Candidate 与验证；支持 method-call / existing-rule / structural-node，不支持的类型显式拒绝，不是任意自然语言自动生成规则。演练有两条规则生成和测试记录。[E3][E6] |
| F4 | `rf coverage analyze OUTPUT_DIR` | half-built | 实际运行固定资产 CoverageLedger 检查，要求 100 Standards、20 Intents、20 Ledgers；不是任意企业规范输入后的通用覆盖服务。[E7] |
| F5 | `rf explain`、`feedback draft`、`exception request` | works | 本地 Finding 解释、digest-only 申请；起草不等于批准。纯核测试可验证动作参数/隐私边界，本次没有重跑 Rust CLI。[E1][E8][C3] |
| F6 | `feedback submit`、`exception submit`、`submission status`；`RuleOpsServerMain` 的 `/v1` API | half-built | 有资源查询、版本生命周期、审计、状态和 journal；transport 只接受显式 HTTP endpoint，ServerMain 为 loopback。回执仅 RECEIVED_ONLY，不是已部署企业治理服务。[E8][E9] |
| F7 | `rf scan --delivery-descriptor`；`GET /v1/rule-bundles/{digest}/delivery`；Runtime 打包脚本 | half-built | 有签名交付与缓存消费，也有真实 Java Runtime Pack 检查，不能说“只有 fake”；但通用 package 脚本装 reference host/resolver，生产 pack 检查本地生成签名材料。正式发行及外部信任运营：未发现证据。[E10] |
| F8 | `rf status contract-check`、`status deployment-check` | works | 交付的是 StatusPublication / adapter 部署契约检查，不是 SCM provider 写入客户端。README 明示不联网、不读凭据、不回写；本次未复跑检查。[E1][E11] |
| F9 | `rf pilot report/delivery-check/baseline/observe/feedback/governance/window-report/evidence package/evidence adjudicate` | works | Shadow journal、基线、窗口报告和证据裁决存在；演练跑过 observe→feedback→governance→report。缺人工数据 INCONCLUSIVE 是正常语义，不是默认成功；正式 LIVE_SHADOW 窗口在十月。[E3][E12] |
| F10 | VS Code / IntelliJ Finding 导航、解释、申请 UI | half-built | 两端有薄绑定与纯核，不是空壳；VS Code 核本次 18/18 通过。README 将完整打包、安装及真实 IDE 验收留作人工步骤，实际安装验收未发现证据。[E13][C3] |
| F11 | `rf decision evaluate` | works | 调用 DeterministicBaselineAdapter 生成证据，是真实确定性实验程序，不是智能业务决策平台；已被降为 experimental。[E2][E14] |
| F12 | `rf center check` | half-built | 工程组合验收脚本，不是运营中心页面；ENGINEERING_MVP / SHADOW，productionEnforcement disabled，liveProductOutcome PENDING_FORMAL_WINDOW。[E15] |

`rf --help` 实测可用；`rf gate` 实测退出 2 并拒绝，这是刻意权限边界，不是待修 bug。[C1][C2] 产品 Web 控制台页面：定向检索 `*.html/*.tsx/*.vue` 未找到，未发现证据；不能凭 Center 名称虚构页面。[C5][E15]

## 3. 三类 gap

### 3.1 overpromise：承诺强度超过现实证据

| ID | Gap | 文件 / 命令证据 |
| --- | --- | --- |
| O1 | 首句“可信规则执行层 / 签名执行证明”可能被读成生产可信服务，当前本地权威不是它。**这是首句与限定之间的理解风险，不是指控 README 没有免责声明。** | README:3–9、153–162；`rf:272–274`；`rf gate` exit 2；Runner:764–766 明示不提供部署权威。[E1][E5][C2] |
| O2 | 白皮书称可降低规则生产、误报与审计成本，现实只支持机制和演练，不能支持已兑现 ROI。 | 白皮书:690–718；作者成本文档:65–75 仍 HUMAN_INPUT_REQUIRED / INCONCLUSIVE；正式窗口未结束。[E12][E16] |
| O3 | “已安装 release 的标准 scan”有实现路线，但不等于已交付可运营生产发行；真实 Java pack 已有检查，发布与信任运营仍缺证据。 | README:119–125；package-rf:10–20、51–65 为 REFERENCE_FIXTURE；production-runtime-pack-check:13–20、95–109 有 Java host 和本地签名；外部分发与使用证明未发现证据。[E1][E10] |

### 3.2 hidden value：已有但未进入主要定位的价值

此处“未提及”的比较面是 **§1 定位句及 README 用户价值表达**，不声称整个仓库无人记录。若严格要求“在全部文档从未提过”，未发现足够证据；下列是有证据的叙事隐值，不是新功能建议。

| ID | Gap | 文件 / 命令证据 |
| --- | --- | --- |
| H1 | 可以保留真实 Finding 同时隔离历史债务，接入不必以“先清空所有存量问题”为条件；此收益未在定位句或 README baseline 命令旁解释。 | 9 月 26 日演练:23–35：13 条 Finding 保留，精确指纹历史基线后 PASS，再整改复扫；README 仅列 pilot baseline。[E3][E1] |
| H2 | 维护者可对照规范场景与规则引用检查覆盖账本一致性，比“又一个扫描器”更具体，但定位只讲执行可信。 | phase4-coverage-ledger:26–43 比较场景与 rule 引用；必须同时披露固定 20 条范围，不能包装为通用覆盖平台。[E7] |

### 3.3 positioning drift：同一能力服务对象发生偏移

| ID | Gap | 文件 / 命令证据 |
| --- | --- | --- |
| D1 | 对外面向平台 / 安全合规负责人，Factory 包与白皮书成败又落在规则作者效率；两个购买任务不能混用未验证 ROI。 | README:3–5、78–86；pom description；白皮书:695–698、756–760；ADR-0167 已将创作降为受治理组件。[E1][E2][E16] |
| D2 | Completeness 对扫描者是“本次是否漏跑”，对 Pilot 人员却是窗口和研究裁决。CLI 已分区，但大量 pilot/center 入口仍可能使读者误认需要先做研究取证。 | README:88–112；operator-quickstart 三步；ADR-0170 分离三条路径。本项是已部分治理的残余漂移，不声称现仍强制日常作者填 YAML。[E1][E4][E12] |
| D3 | Center 易被理解为合规运营台，实际可见面主要服务本地组合验收的工程人员。 | center check → ruleops-center-check.sh；closure 为 PENDING_FORMAL_WINDOW；ServerMain:5、22 为本地开发；页面检索无证据。[E9][E15][C5] |

## 4. Jobs-to-be-done

替代品来自白皮书/访谈脚本的问题假设，不是已访谈用户的事实；“胜出”指机制潜力。真实使用频次、预算方与替代成本未发现已完成验证的证据。[E16][E17]

| ID | 用户与触发场景 | 当前替代品（待验证） | 为何本项目可能胜出；证据 |
| --- | --- | --- | --- |
| J1 | Java CI / 平台负责人看到无告警，需要判断必需检查是否全部执行 | 引擎退出码、告警数量、自建 CI 脚本 | 冻结计划和 Completeness 将不完整区别于 PASS，避免静默失败变绿灯；F1/F2/F8，不替换分析引擎。[E1][E5][E16] |
| J2 | 规则作者要把受支持规范场景变成可复验规则 | 手写 PMD 规则/配置、重复编写测试 | 三种模板、内容寻址 Candidate 和验证链有实现；主动时间节约未发现证据。F3/F4。[E6][E7][E12] |
| J3 | 接入已有 Java 仓库，存量违规使团队难以区分新增问题 | 一次性清仓、全局忽略、私有基线脚本 | 演练证明精确指纹历史基线不删除 Finding，可继续整改复扫；F2/F9，真实团队行为收益待验证。[E3] |
| J4 | 治理负责人追问某规则版本的结果、反馈与例外申请 | 分散报告、工单、手工对账 | digest-only 请求、按身份查询状态、不可把回执当批准，减少结果与治理状态混淆；F5/F6/F7，不是生产审批 SaaS。[E8][E9][E10] |
| J5 | 观察负责人决定候选规则能否进入更强 CI 阶段 | 人工汇总日志与零散试运行结论 | 同模式窗口、证据包、缺测 INCONCLUSIVE、显式裁决；F9/F12，不因演练 PASS 自动放宽门槛。[E3][E12][E15] |

## 5. 互斥候选定位与取舍

互斥的是**首要用户、替代对象、购买理由和验收结果**，不是底层代码必须互不重用；只能选一个对外主线。比例按 §2 十二组等权计算，包括 half-built，仅衡量叙事关联，**不是完成率、需求命中率或市场份额**。

| ID | 一句话定位（谁 / 场景 / 替代 / 优势） | 命中现有功能 | 必须砍掉或降级的特性 / 承诺 | 最快验证信号 |
| --- | --- | --- | --- | --- |
| A | 给 Java 平台与 AppSec 团队，在核对 CI 是否漏跑时，替代“只看告警数与脚本绿灯”的判断，用冻结计划、Completeness 和三态结果提供可复核的本地 / Shadow 执行证据。 | **7/12 ≈ 58%**：F1/F2/F5/F6/F7/F8/F9；F6/F7 仅为交接配套。[E1][E5][E8–E12] | F3/F4 降为规则供应，F10 不作主卖点，F11 继续 experimental，F12 降为内部验收；撤除当前版本“已生产强制执行”的理解，不删代码或改 ADR。[E2] | 已有 8–12 场访谈中，至少两位合格负责人真实排期 Shadow / 引荐预算方，而非口头称赞架构。[E17] |
| B | 给 Java 企业规则作者，在落地受支持规范时，替代重复手写 PMD 规则与验证脚本，用三种有类型模板和 Candidate 验证缩短规则交付时间。 | **6/12 = 50%**：F3/F4/F5/F6/F7/F10。[E6–E10][E13] | F1/F2 退为测试执行器，F8/F9/F12 退为质量研究，F11 退出叙事；不再以可信执行层为主要购买理由，不承诺任意规则生成。[E2][E6][E16] | 真实作者三类模板的手写/模板配对达到已有成本和质量阈值；机器编译变快不算作者收益。[E12] |
| C | 给 Java 合规 / 质量观察负责人，在评审规则试运行时，替代散落日志与手工拼接报告，用身份绑定、窗口聚合和缺测不通过的证据包交付一次可复核的 Shadow 审计材料。 | **7/12 ≈ 58%**：F2/F4/F6/F7/F8/F9/F12。[E5][E7][E9–E12][E15] | F1 退为采集准备，F3 退为材料来源，F5/F10 不扩为开发者工作台，F11 退出叙事；不卖持续强制执行或通用作者平台，只交付限定材料。[E2][E12] | 独立复核人能用现有包复核版本、范围、缺测，并愿意将其纳入现行 Shadow 评审；不得将包当生产认证。[E12][E15] |

## 6. 每个候选的一条可证伪测试

以下**未执行、未修改合同或阈值**。A 沿用访谈脚本，B 沿用现有 Pilot 标准；C 的人数/时间为本审计实验提议，未获裁定。

| Candidate | 测试 | 可观察的反证 |
| --- | --- | --- |
| A | 完成 8–12 场符合“Java + CI + 审计压力”的无提示访谈，记录 top-3 痛点、既有投入及真实承诺。[E17] | 多数认为现有绿灯够用或审计不要求完整性，且拿不到两次真实行为承诺，则“独立执行层是必须采购的任务”不成立；工程正确不能替代需求。[E17] |
| B | 真实作者三类模板做顺序对冲的手写/模板配对，记录主动时间与人工质量复核。[E6][E12] | 完整数据下，作者主动时间中位数未达到基线 80% 阈值，或 precision/recall 未达 0.95/0.90，则“质量不退化且提效”失败；缺测只能 INCONCLUSIVE，不算成功或反证。[E12] |
| C | 两位未参与构建的质量/合规人员，各用现有样本包做一次 30 分钟盲复核，指出 commit、规则身份、范围和缺测原因，再判断是否纳入现行 Shadow 评审。[E12][E15] | 若两人均无法独立复核，或虽能复核却明确不接受这种无生产权威的材料进入真实流程，则“独立审计材料产品”失败；客户接受度未发现证据。 |

## 7. 一条推荐

**推荐 A：维持既有执行层主线，当前只按 Java 本地 / Shadow 完整性证据交付来评价。** README/CONTEXT 与 ADR-0167 已收拢至此，真实仓库演练也支持扫描、基线与复核，而非生产可信部署；B 的作者成本仍缺真值，C 的正式窗口及接受者尚无结论。此为审计推荐，不覆盖用户裁决。[E1–E3][E12]

**最大风险是“绿灯可信度”没有独立预算与强痛点。** 技术机制有证据，但已完成需求访谈、付费或真实排期承诺未发现证据；脚本要求无提示痛点与行为承诺。若 §6-A 被证伪，应推翻定位假设，而不是叠回更多工厂、IDE 和中心功能掩盖需求缺失。[E17][E2 决策 6]

## 8. 本项目不做……

1. **本项目不做新的静态分析引擎或要求一次迁移全部分析器。** PMD 主 Backend、SpotBugs 补充；Backend-neutral 是契约设计，不等于已覆盖所有语言。[E16][E18]
2. **本项目当前不做生产 Gate、受保护分支强制执行或 SCM 回写。** 本地为 LOCAL_UNATTESTED / SHADOW，`rf gate` 明确拒绝。[E1][C2]
3. **本项目不做“输入任意自然语言/两个例子即可自动交付可靠规则”的 AI 产品承诺。** 生成限三类模板，未支持类型明确拒绝。[E6][E16:118–129]
4. **本项目不要求普通扫描用户维护 Pilot 研究 YAML，也不因缺作者成本数据阻止日常本地使用。** 作者成本采集是独立研究路径。[E4][E12/ADR-0170]
5. **本项目不把已接收申请、演练 PASS、工程闭环 PASS 当作批准、真实产品收益或正式窗口 GO。** 权限、证据来源及缺测保持区别。[E3][E8][E12][E15]

## 9. 证据来源与验收记录

### 文件索引

路径相对仓库根；如 `[E8–E12]` 指该区间的来源。源码/测试检索不是本次执行证明；9 月 26 日演练是历史记录，不升级为当前 HEAD 全绿。

| ID | 实际读取 / 定向检索来源 |
| --- | --- |
| E1 | `README.md`；`CONTEXT.md:1–19`；`pom.xml:13`；`rf` usage、路由、安装和拒绝分支 |
| E2 | `docs/adr/0167-product-audit-adjudication-batch.md` 全文，含 9 月 26 日恢复 Node 协议探针的补记，未沿用“永久移除”旧描述 |
| E3 | `docs/research/2026-09-26-real-github-e2e.md` 全文：固定 commit、183 单元、13 Findings、基线、权限边界 |
| E4 | `docs/pilot/operator-quickstart.md` 全文；`runtime/project-config/src/{main,lib}.rs` 文件存在检索，行为主要依据 E1/E3，未冒充本次重跑 |
| E5 | `runtime/rf-cli/src/main.rs` CLI/delivery/ScanResult 符号（`rg -a`）；`runtime/rf-runner/src/main.rs:764–766,1223–1343,1657–1672`；`runtime/gate/src/lib.rs` Completeness 结构；`docs/adr/0169-attested-java-build-context-boundary.md:9–28` |
| E6 | `scripts/authoring-validate.sh`；`control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java:23–94` 模板路由；`control-plane/factory/src/test/java/io/ruleops/factory/CandidateCompilerTest.java:34–106` 测试入口检索 |
| E7 | `scripts/phase4-coverage-ledger.sh:9–46`；三个 specs 目录 YAML 数量实际统计，不代表逐条验证全部资产 |
| E8 | `runtime/thin-client/src/main.rs:31–167` 动作分派检索；`runtime/thin-client/src/transport.rs:236–260`、状态/回执符号；`clients/vscode/test/core.test.js:11–34,98–140`；README:153–171 |
| E9 | `control-plane/server/src/main/java/io/ruleops/server/RuleOpsServerMain.java`；同目录 `RuleOpsHttpServer.java:113–193,220–265`；`control-plane/server/src/test/java/io/ruleops/server/RuleOpsHttpServerTest.java` lifecycle/幂等/审计测试名 |
| E10 | `scripts/package-rf.sh:1–65`；`scripts/production-runtime-pack-check.sh` pack/Java host/signature 检索，尤其 13–20、95–109；`runtime/rf-cli/src/delivery.rs`；Server delivery 路由 |
| E11 | README:158–162；`rf` status 路由；`scripts/status-publication-check.sh:1–40`；`scripts/check.sh` status-publication/status-adapter-deployment 绑定；本次未复跑 |
| E12 | `docs/adr/0170-separate-user-workflow-from-pilot-measurement.md`；`docs/pilot/live-shadow-window-execution-checklist.md`；`docs/pilot/author-cost-baseline.md:65–75`；`runtime/pilot-observer/src/main.rs:124–142,291` 等入口检索；`runtime/pilot-evidence/src/main.rs:1–65` package/adjudicate 路径；lib.rs 仅存在检索 |
| E13 | `clients/vscode/README.md`、`clients/intellij/README.md`、`clients/vscode/package.json`、`clients/vscode/test/core.test.js` 及本次输出 |
| E14 | `runtime/fast-decision/src/main.rs:20–62`；E2 决策 3 |
| E15 | `scripts/ruleops-center-check.sh:1–50`；`specs/center/ruleops-center-closure.yaml:9–12,71`；README:211–219 |
| E16 | `docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md:10–40,118–129,690–760` |
| E17 | `docs/research/2026-09-24-demand-discovery-interview-script.md:9–16,43–59`；访谈文件名检索只定位脚本，不能证明仓库外没有访谈 |
| E18 | `docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md:25–26,653–655,776–791,1089–1156` 的 Java/Backend/模板边界检索 |

### 本次命令结果

未访问 GitHub Issues 或其他外部系统，不以 issue 状态为证据。语义检索工具不可用，采用 `rg` 和有界 Python 读取；命令由 `rtk proxy` 包裹，行为命令有 60 秒超时。

| ID | 实际命令 / 检查 | 观察结果 |
| --- | --- | --- |
| C0 | `git rev-parse HEAD`；`git branch --show-current`；`git status --short` | HEAD/分支如 frontmatter；开工已有未跟踪 `docs/research/product-audit-2026-09-29/`，未读其审计结论、未修改 |
| C1 | `E:/Git/Git/bin/bash.exe ./rf --help` | exit 0；只证明 launcher 和命令枚举可用 |
| C2 | `E:/Git/Git/bin/bash.exe ./rf gate` | exit 2；`error: gate command is not enabled in this thin client` |
| C3 | `node --test --test-name-pattern "^(?!resolveSafeWorkspaceFile)" clients/vscode/test/core.test.js` | exit 0；实际 **18 tests / 18 pass / 0 fail / 0 skipped**，包含路径测试。既有测试使用并清理系统临时目录；无持久产品文件改动，不等于完整 IDE 验收 |
| C4 | Python：三个目录分别 `len(list(Path(p).glob('*.yaml')))` | Standards 100、RuleIntents 20、CoverageLedgers 20；计数不等于质量通过 |
| C5 | `rg --files -g '*.html' -g '*.tsx' -g '*.vue' -g '*Interview*' -g '*interview*' -g '*feedback*.md'` | 定位访谈脚本与反馈 ADR，未找到查询类型的产品 Web 页面；不推断其它界面形式或仓库外事实不存在 |
| C6 | `rtk proxy bash scripts/check.sh` | **未执行到门禁脚本**：`Binary 'bash' not found on PATH` / `program not found`。之后找到 Git Bash，仅用于 help/拒绝检查；未用其重跑会写 Maven/Cargo target 与 `.scratch/` 的全量门禁，以免越过仅报告目录写入范围。当前 HEAD 门禁红/绿本次未建立证据，不沿用历史全绿、不归因、不修复 |

> 注（2026-09-30 搬运补记）：C0 行所述 `docs/research/product-audit-2026-09-29/` 为审计开工时被审检出（D: 树）的既存未跟踪产物，不属本轮交付；本轮交付范围仅 `product-audit-2026-09-30/`，该目录未随产物搬运。

交付验收已通过：Python 断言核对 §1–§8、README/包描述逐字一致、HEAD/分支、12 个功能组、O3/H2/D3 三类 gap、5 个 JTBD、3 个候选及 3 个证伪测试、5 条不做项、所引完整路径存在及无尾随空格；另确认目录仅有本报告、git 无已跟踪产品差异。只新增本文件，无 commit/push、无外部状态修改。不声称完成仓库全量产品验收。
