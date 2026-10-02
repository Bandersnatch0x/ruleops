---
title: Target User and JTBD Audit
date: 2026-09-30
repository: ruleops
head: ae61fc494057a4328dfc51369971ded84f50d2a0
branch: codex/phase1-contract-factory
status: audit-only
primary_persona: P1
secondary_persona: P2
persona_evidence: weak
demand_evidence: E
---

# 目标用户与 JTBD 审计

[inference] 本报告先按技能 Steps 1→3 生成并过滤候选，再选择用户、整理任务与反证；呈现顺序遵循技能 Output：主/次用户 → JTBD → anti-user → 验证切面 → 暂不服务，候选证据账本附于 §6。它延续前两项审计的范围约束，不代表新的产品裁决。[E1]

## 1. 主用户与次用户

### Primary user — P1（恰好一名主用户）

**[hypothesis; evidence E] 主用户是：负责某个 Java 仓库 CI 检查链的平台工程师，在某次提交的静态检查无告警、超时或执行范围变动后，需要核对必需检查是否完整执行，并把可复核的本地 / Shadow 结论交给项目负责人。** [inference] 选择依据不是职位名称，而是他能取得该次 commit、项目配置和既有 Candidate/交付资产，能够操作终端并读结构化结果，且不要求本产品直接授予生产阻断权力。[verified fact] README 的主线、quickstart 的 init→validate→scan 和 9 月 26 日演练支撑这一执行场景；后者由作者操作并披露人工干预，不能当成独立目标客户的使用。[E2][E3][E4] **[inference] 受众证据标签为 weak：有 D 级工程场景支撑，但真实需求、使用频率、切换与付费意愿仍为 hypothesis / E，未发现直接证据。** 他是 operator，不默认是 buyer；维护 Runtime/信任材料或批准生产决策的人可能另有其人。[E5][E7]

### Secondary user — P2（仅一名次用户）

**[hypothesis; evidence E] 次用户是：同一 Java 项目的规则作者，在收到主用户交付的一条争议 Finding 后，需要核对规范意图、适用范围与可复现性，决定修正规则/测试还是交由业务开发者整改。** [verified fact] quickstart 给出 RuleIntent 校验、Candidate 生成与测试路径，历史演练有两条规则编写/验证以及 Finding 复核；因此该角色有工程依据。[E3][E4] [inference] 标签同为 **weak（工程 D，需求/付费 E）**：未发现陌生规则作者独立完成这一交接、节省主动时间或愿付费的直接证据，作者成本文档仍是 HUMAN_INPUT_REQUIRED。[E6] [inference] 只将他作为主用户结果的技术复核接收人，不同时把“通用规则创作平台”设为第二条产品主线；`explain/feedback` 的实现也不等于扫描发行包已经包含这些命令的完整运行依赖。[E1][E7]

[inference] 选择 P1 而不是 P2 为主用户，是因为当前已裁定的对外主线是执行完整性，创作簇是受治理供应环节；不是因为已证明 P1 的市场更大。[E2][E8] [unknown] 真实预算方、维护容量和采购周期：未发现证据，不把它们补写进 persona。

## 2. 主用户 JTBD（四条，不包含次用户的规则创作任务）

[hypothesis; evidence E] 下表是主用户的**待验证任务**，不是客户访谈原话；“当前替代品”与“失败代价”也是假设，不虚构发生频率、事故、时间或金额。成功判据是建议的可观察结果，所引实现只证明机制可表达这些结果，不证明用户确实需要它。[E2–E5]

| ID | job | trigger scenario | current alternative | success criterion | cost of failure | evidence |
| --- | --- | --- | --- | --- | --- | --- |
| J1 | 核对本次扫描应覆盖什么，而不是只看有没有 Finding | [hypothesis; E] 模块/配置变动，或接手一个已有 Java CI 仓库，需要确认必需检查的范围 | [hypothesis; E] 查构建配置、引擎命令与日志，手工比对模块清单 | [inference] 同一 commit 的配置、冻结计划与实际执行范围可对应；未知构建事实被明确报告，不静默当作覆盖 | [hypothesis; E] 复核者可能遗漏范围缺口或反复找人解释；实际频率与成本未发现证据 | [verified fact] README:115–126 的配置/范围绑定；ADR-0170 日常路径。[E2][E8] |
| J2 | 将“完整无违规”“完整有违规”“不完整”分开，决定下一步处理 | [hypothesis; E] 无告警结果、超时或异常退出，需要判断是否应补跑/修配置/处理违规 | [hypothesis; E] 依赖进程退出码、告警数或人工拼读引擎日志 | [inference] 能从结构化结果指出 GateDecision、Completeness 和 LOCAL_UNATTESTED/SHADOW 边界；完整 FAIL 不被 exit0 混成 PASS，不完整不当通过 | [hypothesis; E] 可能接受不充分的检查结果，或对完整扫描无效重跑；不主张已有事故发生 | [verified fact] ADR-0126:19–39：exit0 仅表示完整，Gate 可以 FAIL；不完整 exit3。[E5] |
| J3 | 在既有违规中区分本次需要处理的问题，并保留历史证据 | [hypothesis; E] 首次接入存量 Java 仓库，或历史基线后又出现 Finding | [hypothesis; E] 手工比对两份报告、私有基线脚本、广泛忽略；具体采用哪种未发现客户证据 | [inference] 同一范围下可区分历史/新增，既有 Finding 不被删除；整改后复扫能够解释变化 | [hypothesis; E] 可能把旧债务当新回归反复分派，或用过宽忽略掩盖新问题；真实损失未知 | [verified fact] 9 月 26 日记录中 13 条 Finding 保留，精确指纹基线后 PASS，之后整改复扫。[E4:23–35] |
| J4 | 把一次扫描结果或失败交给正确的人，使其不依赖口头转述就能复核 | [hypothesis; E] 需要由项目负责人处理违规，或由 P2 核对规则争议 | [hypothesis; E] 截图/复制告警文本/发送日志，再口头解释运行范围 | [inference] 交接内容含运行/规则身份、相对位置、结论、完整性与权威限制；失败带结构化诊断；接收者能明确下一 owner，且不把申请回执当批准 | [hypothesis; E] 可能反复索取上下文、把同一问题转错人或误读批准状态；具体往返次数未发现证据 | [verified fact] README:147–157 的结果与回执边界；[inference] 先用可读的 text/JSON/SARIF，不强制依赖尚未完整分发的额外客户端。[E2][E7] |

[inference] J1–J4 组成一个有限闭环：确认范围 → 判读完成度/违规 → 分清债务 → 交接并复核；不把生成规则、维护 Server、批准例外或填 Pilot 表单塞入主用户的日常责任。[E3][E8] [hypothesis; E] 若访谈证明 J3/J4 不参与实际任务，可继续收窄为 J1/J2，不为保留功能强行扩充主用户。

## 3. Anti-user（恰好三类）

[inference] Anti-user 是当前产品边界直接不适配的使用预期，不等于这些人永远没有需求；与 §5“合理但本轮暂不作为产品受众”的角色不同。

| ID | looks like a user | why it fails here | evidence |
| --- | --- | --- | --- |
| A1 | 希望接入后无需另行部署/治理授权，就直接阻断生产受保护分支、自动批准例外的 CI 管理员 | [verified fact] `rf release/gate` 被拒绝，本地结果不授予生产权威，申请接收不等于批准；[inference] 其核心成功条件与当前权限边界冲突 | README:7–9、153–162；`rf:272–274`；同一 Run 的 gate 拒绝实测已在前报告披露。[E2][E7] |
| A2 | 不准备 RuleIntent、测试或工程复核，只想输入一句自然语言/两个示例，就得到任意语言生产规则的业务人员 | [verified fact] 当前生成路径是受支持的三类模板，未支持类型显式拒绝；[inference] 其输入、技能与质量保证预期不适配，不能靠把解释器包装成 AI 入口来满足 | quickstart 作者路径；CandidateCompiler 模板路由与前两报告已核对实现；白皮书明确不是此类 AI Demo。[E3][E7][E9] |
| A3 | 完全没有 Java 检查场景，只希望即装即用地扫描其 JavaScript/Python 项目的应用开发者 | [verified fact] 首期 Java、PMD 主 Backend、SpotBugs 补充；Node 协议探针不等于 JavaScript 分析产品；[inference] Backend-neutral 不能替代缺失的语言交付证据 | 架构计划:25–26、653–655；ADR-0167 的 Node 探针补记。[E8][E9] |

## 4. 下周最小验证切面（五个访谈问题，尚未执行）

[inference] **2026-10-05 至 2026-10-11，先从已有访谈方案中取两位符合 P1 场景的候选，逐一访谈。** 两人是本报告建议的最小画像检查，不是改写既有 8–12 场方案或成功阈值；不能据两次积极回答宣布市场成立。[E10] [inference] 入选条件是本人实际负责过某 Java 仓库的 CI 检查/结果复核，能叙述一次真实操作，而不只是有“平台/安全负责人”头衔；不要求候选预先赞同完整性是痛点。

[inference] 由用户指定并授权研究负责人后才实施；本次只拟定问题，不招募、不联系、不发邮件、不写 issue、不收集遥测。前四问先问过往事实，最后一问才展示当前 Java、本地/Shadow、无生产权威的边界说明；不演示未分发能力、不索要源码或秘密。[E10][E2]

| ID | interview question | what to record / what could disconfirm |
| --- | --- | --- |
| Q1 | 最近一次你确认某次 Java CI 检查已经完成，是怎么做的？请按实际操作顺序说，并指出你看的输出。 | [inference] 记录真正执行/复核的人、入口和完成标准；若受访者不做此任务，说明画像招募无效，而不是为其编造需求。 |
| Q2 | 过去一次引擎异常或项目模块变化时，你是否遇到过检查范围拿不准的情况？没有也请直接说；有的话，请讲当时怎样处理、谁决定下一步。 | [inference] 记录真实事件、现有发现/恢复路径；“没有发生/现有工具立即解决且无额外成本”是反证，不诱导其认可风险。 |
| Q3 | 最近一次在已有违规的仓库引入或更新检查规则时，你们如何区分旧问题与这次要处理的问题，最终是谁做决定？ | [inference] 检验 J3 是否属于 P1 的实际任务；若完全由另一角色处理且不影响本人的判断，J3 从主用户优先任务中移出。 |
| Q4 | 最近一次扫描结果需要交给别人复核时，你交了什么，对方还追问了什么，这次事情实际占用了哪些人力或预算？如果没有发生，也请说明。 | [inference] 检验 J4/P2 是否真是同一交接；记录过往投入而非未来愿望，并区分 operator 与预算方，缺数据就保留空白。 |
| Q5 | 对照刚才那次真实任务，这种仅提供 Java 本地/Shadow 完整性证据、不会生产阻断的方式能替代哪一步，或完全不能替代？如果值得试，谁能决定为哪个仓库安排一次具体的只读验证；不值得的原因是什么？ | [inference] 先记否定理由，再记具体 owner/仓库/排期是否真正落实；口头“有兴趣”不算行动，更不算付费。 |

[inference] **输出与停止条件：** 形成两份去标识记录，逐项区分实际事件、替代品、成本、角色、否定理由及有无可核对行动；若任务不归 P1、现有替代足够，或只接受生产阻断，回到画像/定位裁决，不先新增功能。若下周无法找到合格受访者，记录“未发现可验证对象”，停止该小切面，不将招募失败伪装成任务不存在。支持信号只能将具体陈述升为 C、可核对行为升为 B；独立需求和付费判断仍需更完整证据。[E10]

[verified fact] 既有方案的扩大决策信号仍是 8–12 场合格访谈中至少半数无提示把完整性列为 top-3 痛点，且至少两次真实行为承诺；本报告不改这个门槛，也不改 PilotSuccessContract。[E10]

## 5. 暂不服务清单

[inference] 此处仅指**不作为本轮对外主/次用户或新增产品承诺**；不删除现有组件，不撤销已登记 Pilot 角色，不取消窗口，也不修改 ADR。[E7][E8]

| persona | why not now | what could change it |
| --- | --- | --- |
| P3：正式 Pilot 窗口观察负责人 | [verified fact] 其 journal/窗口聚合责任已经存在；[inference] 属于内部研究运行路径，不应成为普通用户完成扫描的前置责任。[E6][E8] | [hypothesis; E] 若独立观察者对现有工具完成真实任务的直接证据表明这是独立产品需求，再由用户讨论；当前未发现证据。 |
| P4：只接收合规证据包、期望直接用于外部审计的审计负责人 | [unknown] 已有正式审计接受、合法效力或独立审计者消费证据：未发现证据；[inference] 不把 Shadow 工程包当成完成审计义务的产品。[E6][E7] | [hypothesis; E] 合格接收方在真实流程中接受受限材料并说明具体用途；不等于生产权威或认证。 |
| P5：只负责预算/采购、希望先看到 ROI 证明的安全平台预算负责人 | [unknown] 实际预算方、采购行为、价格接受度未发现证据；[inference] 是应识别的潜在决策相关者，不据此设成打开产品执行任务的用户。[E10] | [hypothesis; E] 明确使用者、实际投入与可核对采购行为后，单独建 buyer 证据；不能把 P1/P2 的操作完成当成购买意愿。 |

## 6. 候选 persona、证据过滤与验收

### 6.1 候选与筛选（Steps 1–3 的依据）

[inference] 标签只衡量“这个角色确实有该任务”的用户证据：`strong` 需要可核对的目标用户实际任务/行为；`weak` 是角色与具体入口/场景有文档、代码或作者演练支撑，但缺直接用户证据；`pure guess` 是独立用户场景只有推测，未发现正向证据。**本次没有 strong 候选**，不能用 README 自述或单测充当客户证词。[E2][E4][E10]

[inference] A–E 沿用前项报告的审计分级约定：A=实际付费且持续使用；B=可核对的资源投入/排期/预算方行动；C=具体过去事件的一手陈述；D=作者演练/源码测试等间接工程证据；E=未验证假设/未发现直接证据。它不是修改项目术语；工程 D 不自动升级需求等级。[E7]

| ID | role + scenario | why open this product (one line) | evidence label | evidence / missing evidence | selection |
| --- | --- | --- | --- | --- | --- |
| P1 | [hypothesis; E] Java 仓库 CI 检查链的平台工程师，在一次提交结果不确定时复核完整性 | [hypothesis; E] 为知道这次到底哪些必需检查跑成了，而不是只看告警数打开产品 | **weak** | [verified fact; D] README 主线、quickstart、9/26 演练可支持操作情境；[unknown] 独立用户需求/付费未发现证据。[E2–E4] | [inference] **primary**：与已裁定主线最一致，但仍是待验证选择。[E8] |
| P2 | [hypothesis; E] 同项目 Java 规则作者，收到一条争议 Finding 后技术复核 | [hypothesis; E] 为判断该改规则/测试还是由业务开发者修代码而打开产品 | **weak** | [verified fact; D] 作者 CLI、Candidate/测试与演练存在；[unknown] 真实交接完成和主动时间收益未发现证据。[E3][E4][E6] | [inference] **secondary**：仅接主用户结果，不另开通用作者主线。 |
| P3 | [hypothesis; E] 登记 Pilot 的观察负责人，需要聚合多仓库 Shadow 窗口 | [hypothesis; E] 为收齐 journal 并交付窗口材料而打开观察工具 | **weak** | [verified fact; D] 清单明确观察员角色与窗口报告；[unknown] 真实负责人完成正式窗口未发现证据。[E6] | [inference] **not serving yet**：保留内部角色，不推广成当前主受众。 |
| P4 | [hypothesis; E] 外部审计/合规接收人，需要直接采纳一份扫描证据完成审计 | [hypothesis; E] 为避免自己整理材料而打开证据包 | **pure guess** | [unknown] 未发现独立审计者采纳、审计完成或验收标准证据；白皮书的审计愿景不证明该 persona 成立。[E7][E9] | [inference] **not serving yet**：仅作为待查假设。 |
| P5 | [hypothesis; E] 安全平台预算负责人，为采购比较 ROI 与替代成本 | [hypothesis; E] 为确认投入是否值得而打开产品/材料 | **pure guess** | [unknown] 未发现真实预算、采购或付费证据；现有访谈脚本恰把该信念列为待验证。[E10] | [inference] **not serving yet**：不把潜在 buyer 等同产品 operator。 |

[inference] P1/P2 的 weak 不代表实现无效，而是拒绝把“代码服务某角色的设想”提前升级为“该角色已经需要并使用产品”；P4/P5 有概念想象但没有独立角色的行为证据，所以保留 pure guess 而不凑齐强用户。[E4][E7][E10]

### 6.2 实际读取的证据

[verified fact] 本项以 `ae61fc4` 当前 checkout 为基准；已确认 HEAD 与前两报告一致，未读取/写入远端 issue，不联系用户。前两报告仅是定位与范围输入，不能反向充当新客户证据。[E1][E7]

| ID | source / inspected scope |
| --- | --- |
| E1 | [verified fact] 指定 `D:/code_space/.orca-workspaces/agent-skills/skills/product/target-user-and-jtbd/SKILL.md` 全文；本次任务说明；前两报告的定位/角色/JTBD/范围判定 |
| E2 | [verified fact] `README.md:3–9,115–162`，当前主用户描述、结果/权限边界；后半为同一 Run 已读并由当前 HEAD 不变确认 |
| E3 | [verified fact] `docs/pilot/operator-quickstart.md:1–39`，本项重读日常扫描与规则作者路径 |
| E4 | [verified fact] `docs/research/2026-09-26-real-github-e2e.md:18–35,46–51`，历史作者演练、规则生成、Findings、基线与自审披露；不是客户研究 |
| E5 | [verified fact] `docs/adr/0126-rf-scan-exit-code-contract.md:19–39`，本项重读执行完整性与违规结论的区别 |
| E6 | [verified fact] `docs/pilot/live-shadow-window-execution-checklist.md:15–22` 角色；`docs/pilot/author-cost-baseline.md:65–75` 缺真实作者工时 |
| E7 | [verified fact] `docs/research/product-audit-2026-09-30/02-product-scope-audit.md` 的角色、五问能力表、P4→P5 分发断点、证据等级与检查记录；同一 Run 已检查 `rf:226–265`、`scripts/package-rf.sh`、thin-client 的相关实现，不以它证明实际用户交接 |
| E8 | [verified fact] `docs/adr/0170-separate-user-workflow-from-pilot-measurement.md:14–32` 本项重读；同一 Run 已完整读 `docs/adr/0167-product-audit-adjudication-batch.md` 的执行主线、创作边界、实验项与 Node 补记 |
| E9 | [verified fact] 同一 Run 已读取 `docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md:118–129,690–760`、`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md:25–26,653–655` 及 CandidateCompiler 三模板路由；本项未重新运行生成/编译 |
| E10 | [verified fact] `docs/research/2026-09-24-demand-discovery-interview-script.md:9–16,43–59` 本项重读，完整问题在前项已读；脚本不是已发生访谈 |

### 6.3 验收与明确未做之事

[verified fact] 本次已运行 `git rev-parse HEAD`、`git status --short`、有界文件读取/精确检索及前两报告 SHA-256 取样；没有运行用户实验，没有把历史演练说成本次实测。[unknown] 本次没有新的市场/付费数据，均为“未发现证据”。

[verified fact] 报告验收已通过：Python 断言核对六个章节、五个候选及合法标签、恰好一主一次、四个完整 JTBD、三个 anti-user、五问验证切面、三项暂不服务、证据路径与 HEAD；前两报告 SHA-256 不变，git 无已跟踪产品差异。[verified fact] 本次仅新增本文件，不改产品、合同、ADR、CONTEXT 或既有报告，不 commit/push、不外联。

[verified fact] 本次还尝试 `rtk proxy bash scripts/check.sh`，exit1：`Binary bash not found on PATH` / `program not found`，未执行到仓库门禁脚本；未归因或修复，也未改用其它路径运行会写入产品构建目录的全量检查。[unknown] 当前产品全量门禁状态本次未建立证据；报告验收不等于产品门禁通过。
