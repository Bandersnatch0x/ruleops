---
title: Goals and Success Metrics Calibration
date: 2026-09-30
repository: ruleops
head: ae61fc494057a4328dfc51369971ded84f50d2a0
branch: codex/phase1-contract-factory
status: plan-only
positioning: local-shadow-execution-evidence
scope_decision: pause to validate
primary_persona: P1
---

# 目标与成功指标校准

[verified fact] 输入沿用：01 的“可信规则执行层”定位、02 的 **pause to validate** 建议、03 的 P1 主用户——负责某个 Java 仓库 CI 检查链、需要核对一次提交是否完整执行的平台工程师。[E1] [inference] 本报告只把该方向转成可测计划，不重选用户、不将规则作者效率升为主目标、不授权产品扩张或遥测实施。

[verified fact] 当前范围仍是 Java 本地 / Shadow，不能授予生产 Gate 或 SCM 回写权威。[E2][E4] [hypothesis; evidence E] 需求、持续使用价值与付费意愿未获直接证据；本报告新增指标和目标均为待批准计划，不是既有数值或市场结论。作者演练/源码为间接工程证据 D，不能替代真实用户行为。[E1][E8]

## 1. North Star + 三个支撑指标 + guardrails

### 1.1 唯一 North Star

**[proposal] NS：每周在真实 Java CI 任务中独立完成一次可复核的扫描完整性判断，并据此选择正确下一步的合格主用户数（按人去重）。**

| field | definition |
| --- | --- |
| user | [proposal] 仅计03的 P1：本人实际负责目标 Java 仓库检查链或结果复核；不计旁观者、作者自测、代理、虚构账号和仅被演示者。[E1] |
| qualifying behavior | [proposal] 用户本人说明该次任务的范围/身份、完整性、违规结论与权限限制，并选择与证据一致的下一步；复核者核对既有输出，不由维护者代答。 |
| observable proof | [proposal] 有 `taskId`、用户匿名编号、实际任务来源、结果/失败证据摘要引用、用户判读、下一步及复核结论；不需上传源码，不另算一套 Gate。 |
| numerator / window | [proposal] 每个自然周（Asia/Shanghai，周一至周日）满足上述条件的不同 `userId` 数；同一人多跑、多仓库、多次重试本周最多计1。 |
| valid next step | [proposal] 完整无违规可说明本地完整；完整且有违规交正确负责人处理；不完整则拒绝当通过并指明补输入/修环境/复跑 owner。`INCONCLUSIVE` 被正确识别可算**判断任务**完成，不得称扫描成功或正式 GO。[E2] |
| exclusion | [proposal] 不计只执行命令/看告警数、只复述讲解、无证据的完成声明、过程中由维护者改产品源码或代完成判断的任务；记录这些干预，不能将其静默丢弃或伪装独立成功。 |
| current / target | [unknown] 当前值：未发现证据，不填0。[proposal] 验证阶段先建立可信基线；未获需求与实施批准前不以“刷高 NS”推动招募、埋点或新功能。 |

[inference] NS 衡量主用户 J1/J2/J4 的实质判断与交接，不是活跃人数、扫描次数、发现数或“完整 PASS 数”。[hypothesis; E] 这些判断值得用户反复选择仍需验证；即使 NS 上升，也不自动证明愿付费。[E1]

### 1.2 恰好三个支撑指标

| ID | metric | formula / counting boundary | desired direction / present evidence |
| --- | --- | --- | --- |
| S1 | 有可核对真实行为承诺的合格受访者数 | [proposal] 在既有一批访谈中，满足 Java/CI/审计场景并给出**真实仓库 Shadow 排期、具体意向或已发生预算方引荐**的不同受访者数；同人多种承诺只计1，附可核对记录。单纯预约访谈、称赞或说“有兴趣”不计；同时展示该批有效样本数以防只挑正例。 | [proposal] 先取得真实承诺，不强设增长率；既有方案仍要求8–12场、至少半数无提示 top-3 痛点且至少2次行为承诺，S1 不能替代前一个条件。[verified fact] 脚本存在；[unknown] 实际承诺未发现证据。[E5] |
| S2 | 首次真实任务可复核完成率 | [proposal] 按首次任务启动 cohort：7日内达到 NS 完成标准的人数 ÷ 已经过完整7日观察期的合格首次任务启动人数。启动定义为获得前置条件清单后开始实际接入/尝试；安装/输入阻塞、退出及未完成留在分母，不只收成功扫描者。重试仍属同一首次任务；尚未满7日者单列 pending，不计成功也不混入成熟分母。 | [proposal] 改善从接入到首次可复核判断的比例，不通过隐藏失败提升它。[unknown] 用户开始/完成/干预记录未发现证据；先采基线，暂不臆定目标百分比。 |
| S3 | 再次遇到同类任务时的自主复用率 | [proposal] 滚动28日内，曾完成首次任务、且再次出现真实同类触发的主用户中，自行选择本产品并再次达到 NS 标准的人数 ÷ 所有已观察到该触发的合格用户数；同一人窗口内最多计1。选择其它方案/放弃/失败者保留分母；没有复发任务的人不计流失，未被观察到触发不冒充“没有”。 | [proposal] 提高有真实选择机会时的复用，而非制造无意义扫描。[unknown] 触发、选择与再次完成的数据未发现证据；无触发或分母为0标 `NOT_OBSERVED`，不报100%。 |

[inference] 当前 pause-to-validate 的领先指标是 S1；S2/NS 的真实任务观察只在用户授权后开展；S3 在确有重复任务后才有定义。不能因为后两项没有数据就先造插件、Server 或大规模遥测。[E1]

### 1.3 明确不能变坏的 guardrail 指标

[verified fact] 以下 G1、G3–G5 引用已批准 Pilot 合同；适用范围是登记样本、冻结规则包与正式窗口，不能偷换成任意用户 cohort 或用本计划覆盖合同。[E3] [proposal] G2/G6 的事件统计用于检验既有权限/证据纪律，不授予新权限。

| ID | guardrail metrics | limit / comparison | data / fail behavior |
| --- | --- | --- | --- |
| G1 | `false-pass-count` | [verified fact] 合同阈值 **0**；不完整/故障注入不能产出 PASS。[E3] | [verified fact] baseline 定义从 run 的 trust-and-decision/独立注入证据统计；[proposal] 有一次错误通过即停止该样本成功认定，保留证据交已有 owner，不能靠 NS 增长抵消。[E6] |
| G2 | 源码上传事件数；未经授权 SCM/受保护分支写入事件数；本地结果虚假升级生产权威事件数 | [proposal] 各为 **0**，依据当前 sourceUpload/repositoryWrites/scmStatusPublication/protectedBranchAuthority 均 denied 的边界。[E4] | [unknown] 实际部署侧事件数据未发现证据；**需埋点/日志**，最小先以授权的运行观察/既有调用日志和人工边界复核记事件，不加入上传型遥测。缺检查覆盖不算0。 |
| G3 | `precision`、`recall`、`false-positive-regression` | [verified fact] **precision ≥0.95；recall ≥0.90；false-positive-regression ≤5%**，按同commit的 baseline/Shadow 配对人工复核。[E3] | [verified fact] feedback journal 可承载 regression outcome，但 precision/recall 不能由该 journal 自动推出；须采真实人工标注，未采则 INCONCLUSIVE。[E6] |
| G4 | `complete-rate`、`inconclusive-rate`、p95 latency、peak memory | [verified fact] **complete-rate ≥0.99；inconclusive-rate ≤0.01；p95 ≤基线2倍；peak memory ≤基线1.5倍**。[E3] | [verified fact] 现有 run 字段有 complete/decision/latency_millis/peak_memory_bytes；没有获准基线和相同范围不得判达标，缺测仍 INCONCLUSIVE。[E6][E7] |
| G5 | `new-violation-trend`、`historical-debt-trend` | [verified fact] 登记Pilot口径均要求 **≤0**；不是鼓励隐藏 Finding 或删规则。[E3] | [verified fact] run 的 new_finding_count / historical_finding_count 可用于配对趋势；[proposal] 若样本、范围、规则身份变化导致不可比，应标不确定，不通过缩分母/忽略规则改善趋势。[E6] |
| G6 | 客户行为证据污染事件数 | [proposal] **0**：将 LOCAL_SIMULATION/SYNTHETIC_SHADOW/作者代做、未知数据、重复人次或访谈预约记成客户完成/产品承诺，均算污染事件。 | [unknown] 用户行为记录及审查来源未发现证据；**需埋点/日志**，直接复用 §5 的手工研究账，不新建遥测服务。出现污染撤销受影响计数、保留更正轨迹，不补造0。[E1][E6] |

[verified fact] 原合同的 `author-active-time` 中位数不高于基线80%仍是正式Pilot条件；本报告没有废除它。[inference] 因主用户仍是执行者，不把“作者生成更快”改造成第二个 North Star 或第四个支撑指标；机器耗时也不能替代主动时间。[E3][E8]

## 2. 三阶段里程碑：完成时能看到什么

[proposal] **时间口径须显式确认：** 今天是2026-09-30，当前自然月与Q3今日结束，不能假装后续实验已在9月完成。为给出可执行的递进计划，本表“本周”按当前自然周截至10月4日；“本月”指本轮前瞻月计划窗截至10月31日；“本季”指后续完整Q4规划窗截至12月31日。后两项不是对当前自然9月/Q3的追认，均待用户批准；若只能按自然月/季结算，则本日只保留缺数快照，不填成达标。

| stage | one deliverable milestone | what you can see when done | owner / condition |
| --- | --- | --- | --- |
| 本周：截至2026-10-04 | [proposal] 一个可执行或明确受阻的首轮用户验证包 | [proposal] 可看到主用户资格、03的五问、实际被授权的负责人、两位合格候选的参与确认/可执行时段；未获授权或无法取得对象则看到明确停止/未就绪记录。不是新增访谈系统，也不把预约访谈计入 S1/NS。 | [proposal] 用户指定研究负责人；只筹备前序03提出的下周切面，未批准不外联；不再并列启动第二个用户群。[E1] |
| 本月：本轮月计划窗截至2026-10-31 | [proposal] 一份既有8–12场访谈的证据裁决包 | [proposal] 能看到有效分母、无提示痛点、S1逐人去重与可核对承诺、反证和未完成项；用户据既有阈值作继续受限验证/收窄/停止/未知判断。不能只交“访谈脚本写好了”。 | [proposal] 研究负责人交用户裁决；正式10月Pilot继续按原条件独立治理，访谈不替代窗口人工质量结果。[E3][E5] |
| 本季：后续Q4截至2026-12-31 | [proposal] 一份基于真实任务的季度继续/退出裁决包 | [proposal] 若前关获得支持并批准实际试用，可看到 NS/S2 的逐任务独立复核，以及有复发机会时 S3 的真实分子/分母、干预和 guardrail 状态；若需求被否定、未获批准或未出现复发，看到有依据的 stop/narrow/unknown 结论，而不是为了交卷造试用或扩大范围。 | [proposal] 产品负责人汇总、既有质量/治理负责人复核，最终用户决策；数据不足不强设正向结果，不从季度完成直接跳生产强制执行。 |

[inference] 三阶段各只有一个交付物；负向/未知结论可完成**学习里程碑**，但不算产品已赢。当前仍只推荐既有验证路径，不因此要求实现 OP3/OP4 或新特性。[E1]

## 3. Anti-goals（五条）

1. [proposal] **star、下载、文档数或功能数上涨，但没有真实 P1 完成一次可复核判断，不算赢。** 不用工程产出代替 NS。[E1]
2. [proposal] **扫描/测试次数上涨、固定仓库演练全绿或正式工程包齐全，但只有维护者/代理操作，不算用户采用。** 工程证据可支撑 guardrail，不能灌入 NS/S2/S3。[E7][E8]
3. [proposal] **收集很多“有兴趣”或访谈预约，却没有可核对的真实产品行为承诺，不算需求成立。** S1 与既有无提示痛点条件必须同时被正确解释，不称付费验证。[E5]
4. [proposal] **通过改样本、删规则、宽泛忽略、丢弃失败或把 UNKNOWN 当0让指标变好，不算赢。** 不能用污染证据通过 G1/G3–G6。[E3][E6]
5. [proposal] **靠维护者代做、扩大源码上传/SCM权限、提前解禁 gate，或临时拼出未披露的开发环境换来完成率，不算独立产品成功。** 同样，模板编译更快但主用户没有走完任务，也不算主线获胜。[E1][E4]

## 4. 特性 ↔ 指标映射（含 defer / cut 候选）

[verified fact] 功能分母沿用01的 F1–F12，机会项沿用02的 OP1–OP5；本次未访问外部 issue/backlog，**这不是声称已读取当前 GitHub 任务状态**。[E1] [inference] 下表“贡献”是因果假设/安全依赖，不代表已有指标改善数据；不能为了让每项有归宿而硬编直接贡献。

| item | metric contribution | evidence / limits | recommendation |
| --- | --- | --- | --- |
| F1 init/config/doctor | [inference] S2/NS 前置；G1/G4 | [verified fact] 已有配置/范围冻结、显式诊断；[unknown] 独立用户接入效果未发现证据。[E2] | [proposal] keep 现有主线；按真实接入阻塞修最小问题，不重建配置平台。 |
| F2 scan / JSON / SARIF / progress | [inference] NS、S2、S3 的核心输出；G1/G4 | [verified fact] 可表达身份、Finding、Completeness、三态；[hypothesis; E] 用户是否愿持续使用待测。[E2][E7] | [proposal] keep，先量任务完成而非扫描次数。 |
| F3 三模板生成/验证 | [inference] 间接保障 S2 输入和 G3；非 S1 需求证明 | [verified fact] 规则供应已存在，作者效率另有合同；不把它变第二条主线。[E1][E3] | [proposal] keep 现有供应与验证；defer 无主任务证据的新模板扩张。 |
| F4 固定覆盖账本检查 | [inference] G3/G5 的供应侧辅助，暂无 NS 的直接贡献证据 | [verified fact] 限固定100/20/20资产范围；[unknown] 通用企业覆盖产品需求未发现证据。[E1] | [proposal] keep 内部一致性检查；defer 通用覆盖平台。 |
| F5 explain / draft / exception request | [hypothesis; E] 若P1需要交接，可改善 S2/S3；G2 | [verified fact] 有实现；分发路径局限已在02披露，不能把现有源码视作用户完成证明。[E1] | [proposal] keep 已有行为；新增分发工作待 OP3 的真实需要。 |
| F6 Server / submission status / audit | [hypothesis; E] 中断恢复可能支持 S3；G2/G6 | [verified fact] 状态/回执有实现；[unknown] 部署与目标用户处理闭环未发现证据。[E1] | [proposal] keep 契约与恢复；defer 独立企业治理服务扩张。 |
| F7 signed bundle / runtime / cache | [inference] S2 的运行前提；G1/G2 | [verified fact] 装配/校验有路径，但正式发行与外部信任不能由脚本存在推出。[E1] | [proposal] keep 安全交付边界；仅在受批准真实任务需要时补最短交接。 |
| F8 status contract / deployment check | [inference] G1/G2；不直接计 NS 或 S1 | [verified fact] 是 adapter 契约检查，不是实际 SCM 写入。[E2] | [proposal] keep 安全检查；不为指标解禁生产发布。 |
| F9 Pilot observer / evidence | [inference] G1/G3/G4/G5 的测量路径；不直接证明 NS | [verified fact] run/feedback/governance journal 有既定字段；用户行为支持项不在其合同中。[E6][E7] | [proposal] keep 已登记内部取证；不要把研究字段加回普通用户流程。 |
| F10 IDE 薄客户端 | [hypothesis; E] 可能改善 S2/S3，但未有实际指标贡献证据 | [verified fact] 核与绑定存在；真实IDE采用/安装未建立证据。[E1] | [proposal] defer 扩展/上架承诺，直到观察到 CLI 对该任务的实际阻碍；不重做已存在内核。 |
| F11 experimental fast-decision | [inference] **对 NS、S1–S3 及本主任务护栏未建立贡献** | [verified fact] 另一个确定性实验入口，已被降为 experimental；没有 P1 的用户任务证据。[E1][E2] | [proposal] **defer；cut 对外主线承诺**，保留既有代码/门禁，不推翻 ADR。 |
| F12 center check | [inference] G1–G5 的组合工程检查；不能把 PASS 当 NS | [verified fact] 内部工程闭环不是运营控制台，正式结果仍待窗口。[E1][E4] | [proposal] keep 内部验收；defer 新控制台，不能用页面数量论成功。 |
| OP1 既有需求访谈 | [inference] S1，决定是否进入 S2/NS 观察 | [verified fact] 脚本和否定条件已存在；真实结果未发现证据。[E5] | [proposal] **now，唯一下一步，待用户授权**。 |
| OP2 减法约束 | [inference] G6；避免用新范围掩盖 S1 空缺 | [verified fact] 01/02已收敛执行主线与内部区。[E1] | [proposal] now，作为 OP1 约束；cut 额外公开承诺，不删已决组件。 |
| OP3 接通已存在的客户端交付 | [hypothesis; E] S2/S3，前提是结果接收人真的需要该下一跳 | [verified fact] 已有 rf-client 可复用；[unknown] 需要新增交付的真实用户阻塞未验证。[E1] | [proposal] later；无 S2 阻塞证据就 defer，不新造 Server/解释器。 |
| OP4 最小结果消费示例 | [hypothesis; E] S2/G1，前提是确有退出码/权威误读 | [verified fact] 当前合同已明确；[unknown] 真实误读频率未发现证据。[E2] | [proposal] later；先观察，不改退出码协议。 |
| OP5 通用控制台 / 新语言 / 任意AI生成等扩张 | [inference] **对当前指标未建立贡献**；任何市场收益仍为 hypothesis/E | [unknown] 当前主用户中的具体缺口、采用或投入证据未发现。[E1] | [proposal] **defer；cut 本轮计划与对外主线承诺**，不是删除产品代码或取消既有窗口。 |

## 5. 测量路径与证据缺口

### 5.1 支撑指标逐项的 source / cadence / owner

[verified fact] 现有 PilotMetricsBaseline 的支持记录是 run、feedback、governance，主要记录摘要、运行/资源、反馈与治理；没有本计划所需的主用户访谈承诺、首次任务 cohort 或真实复发选择记录。[E6] [proposal] 所以“需埋点”优先解释为**授权后的手工观测日志**，不是立即写代码、接分析 SDK 或加外发服务。

| metric | current data source / gap | minimal measurement plan | cadence | reviewer |
| --- | --- | --- | --- | --- |
| S1 | [verified fact] 有访谈问题和判读规则；[unknown] 实际纪要/承诺明细未发现证据。**需埋点/日志**。 | [proposal] 使用既有访谈纪要格式，每位匿名候选记录资格、具体过去事件、无提示痛点、承诺种类、可核对行动引用与时间；同人去重，不记源码/凭据。无需改应用。 | [proposal] 每次访谈后登记，每周复核，整批到期封存；不拿未完成批次当成功。 | [proposal] 用户指定研究负责人登记、产品负责人复核；真实人选未发现证据。 |
| S2 | [verified fact] ScanResult/ScanFailure 可给技术证据；[unknown] 起始人群、阻塞/退出/人工代做和用户判读未发现记录。**需埋点/日志**。 | [proposal] 复用同一研究账，记录 `userId/taskId/firstStartedAt/prerequisiteStatus/resultDigest/completedAt/intervention/reviewOutcome`；setup失败也建记录；7日后结算成熟cohort，结果引用而不复制完整Finding。 | [proposal] 每次首次任务状态变化时登记；每周看成熟cohort与pending数；只有授权试用后才启动。 | [proposal] 观察负责人登记，独立质量复核人核对输出与解释，产品负责人看阻塞类型。 |
| S3 | [unknown] 有用户实际再次遇到任务、选择替代方案、主动复用/放弃的记录未发现证据。**需埋点/日志**。 | [proposal] 对同意随访的已完成用户，用同一匿名编号记录 `triggerObservedAt/triggerEvidence/chosenAlternative/taskId/reviewOutcome`；把不选本产品者保留分母，不为追踪装常驻探针。 | [proposal] 真实复发时登记，按周复核滚动28日；未获随访证据标 unknown，不能自动当流失或复用。 | [proposal] 研究负责人随访、质量复核人确认再次完成、产品负责人判断是否值得继续；均待用户指定/授权。 |

### 5.2 North Star 与 guardrail 的测量补充

- [proposal] **NS：需埋点/日志**。直接由 S2/S3 的同一任务研究账聚合，复用 `userId/taskId/resultDigest/reviewOutcome`，每周由产品负责人和质量复核人核对；不增加第二份 Gate/审批真相源。缺复核记录不计入成功，但须保留未完成/未知项。
- [verified fact] G1/G4/G5 可以引用既有 run 与模式限定窗口聚合；G3 的 regression 可来自 feedback，precision/recall 必须有配对人工质量材料。现有定义/实现≠当前已有真实完整数据。[E3][E6][E7]
- [proposal] **G2：需埋点/日志**，最小人工列明检查了哪些授权、执行/传输日志和输出，记录违规事件及覆盖范围；未检查不能填0，不另收源码/凭据。
- [proposal] **G6：需埋点/日志**，与 S1–S3 同账审阅 `evidenceKind/operator/intervention/dedupKey/reviewOutcome/correctionReason`；发现误计后留更正记录，不能以删失败来修饰增长。
- [proposal] guardrail 由既有质量/治理负责人在每次窗口报告和任何越界/污染事件发生时审阅；用户行为计数由研究负责人整理，不让研究者代替治理负责人批准 release 或 Pilot GO。[E3][E4]

[inference] **需埋点/日志清单：NS、S1、S2、S3、G2、G6。** 最小实施均是获准后的本地研究/审查记录；G1/G3/G4/G5 优先采齐已有来源，不新建应用埋点。[verified fact] 本次没有创建这些日志、身份表或数据采集机制，没有联网招募/随访，也没有改合同或产品。[E1]

### 5.3 缺数、比较与隐私纪律

1. [proposal] 所有新用户指标当前均记“未发现证据/未测量”，不把缺数当0；比例缺分母记 `NOT_OBSERVED`，正式 Pilot 按既有规则记 `INCONCLUSIVE`，不互相代替。[E6]
2. [proposal] 仅用匿名研究编号、任务/结果摘要、时间和必要的行为观察；个人联系方式与源码不进入共享指标表，不为测量擅自采集。具体同意/保存/删除规则须用户批准后落实，本报告不建立新制度或外部数据源。
3. [proposal] 保留全部启动者、负向回答、拒绝、失败和干预记录；区分真实客户、作者演练、simulation/synthetic/live模式，不能合并以达到门槛。[E5–E8]
4. [verified fact] 正式Pilot阈值/样本/窗口以批准的 PilotSuccessContract 为准；baseline 文件中的 `DEFINITION_ONLY/HUMAN_INPUT_REQUIRED` 表示测量定义和人工边界，不能覆盖已批准合同，也不能据批准状态捏造已采数据。[E3][E6]
5. [proposal] 本计划不设虚构的转化率、营收、留存目标；S1沿用既有判读门槛，NS/S2/S3先有真实基线再由用户定改进阈值。不得为改进指标降低 G1–G6 或已有作者效率/质量要求。

### 5.4 证据来源与本次验证

| source | actually inspected |
| --- | --- |
| E1 | [verified fact] `docs/research/product-audit-2026-09-30/01-product-positioning-audit.md` 的定位/F1–F12；`02-product-scope-audit.md` 的 pause-to-validate、OP1–OP5；`03-target-user-and-jtbd.md` 的 P1/JTBD/下周五问；本项仅读取，前序结论未改 |
| E2 | [verified fact] 同一 Run 已读 `README.md:3–9,115–162`、`docs/adr/0126-rf-scan-exit-code-contract.md:19–39`、`docs/adr/0167-product-audit-adjudication-batch.md` 与 `docs/adr/0170-separate-user-workflow-from-pilot-measurement.md`；HEAD未变化 |
| E3 | [verified fact] 本项完整读取 `specs/pilot/pilot-success-contract.yaml`：APPROVED、SHADOW、效率/质量/安全/运行/趋势阈值、三仓库样本及10月窗口 |
| E4 | [verified fact] `specs/pilot/pilot-release-manifest.yaml:27–40`：正式结果需要人工输入；源码上传、仓库写入、状态发布及受保护分支权力禁用 |
| E5 | [verified fact] `docs/research/2026-09-24-demand-discovery-interview-script.md:43–49` 本项重读，8–12场与无提示痛点/行为承诺门槛在同一Run已全文核查；未执行访谈 |
| E6 | [verified fact] 本项完整读取 `specs/pilot/pilot-metrics-baseline.yaml` 与 `docs/pilot/pilot-metrics-baseline.md`；记录类型、字段、聚合、缺数与人工边界 |
| E7 | [verified fact] `runtime/pilot-observer/src/main.rs` 运行/聚合符号定向检索：complete、decision、latency_millis、peak_memory_bytes、模式隔离与 Product outcome 边界；不是本项运行观测器 |
| E8 | [verified fact] 同一Run已读 `docs/research/2026-09-26-real-github-e2e.md` 的作者演练/自审披露及 `docs/pilot/author-cost-baseline.md:65–75` 的 HUMAN_INPUT_REQUIRED；它们不是本项新客户数据 |
| E9 | [verified fact] 本项完整读取指定 `D:/code_space/.orca-workspaces/agent-skills/skills/product/goals-and-success-metrics/SKILL.md`；仅按五步产出计划，不实施采集 |

[verified fact] 本项读取 HEAD、git状态和前三报告 SHA-256；Python 断言已通过：五个Output章节、唯一NS、三个支撑指标及其source/cadence/reviewer、六组护栏、三个里程碑、五条anti-goal、F1–F12/OP1–OP5映射、需埋点清单、引用路径和HEAD。前三报告hash不变，git无已跟踪产品差异；没有访问外部backlog/issue、发送访谈、创建埋点代码或修改产品/合同/裁决。

[verified fact] 本次还尝试 `rtk proxy bash scripts/check.sh`，exit1，错误为 PATH 中找不到 bash，未执行到门禁脚本；未归因/修复或另启写入型构建。[unknown] 当前全量产品门禁状态本次未建立证据；报告验收不替代产品门禁。
