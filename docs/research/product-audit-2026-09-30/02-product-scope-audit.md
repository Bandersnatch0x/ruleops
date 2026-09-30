---
title: Product Scope Audit
date: 2026-09-30
repository: ruleops
head: ae61fc494057a4328dfc51369971ded84f50d2a0
branch: codex/phase1-contract-factory
status: audit-only
recommendation: pause to validate
confidence: medium
scope: whole-product
skill: D:/code_space/.orca-workspaces/agent-skills/skills/product/product-scope-audit/SKILL.md
prior_report: docs/research/product-audit-2026-09-30/01-product-positioning-audit.md
---

# 产品范围审计

## 1. 推荐、置信度与可推翻前提

**[inference] 推荐：pause to validate——暂不扩大产品范围，保留已裁定的 Java 本地 / Shadow 执行主线，先验证“扫描完整性是否是目标团队愿意投入资源解决的任务”。** 这是供用户裁决的建议，不代表已暂停项目、取消十月窗口、冻结任何运行中的任务或修改既有 ADR。[S1][S2][S10][S11]

- [inference] 推荐置信度为**中**：工程能力与受限交付有相互印证，但不足以判断产品是否值得继续扩张；应先补直接证据而非先填功能缺口。[S3–S10]
- [hypothesis; evidence E] “存在可独立采购的完整性痛点、愿意换用本项目、可形成可持续业务”均未获直接客户证据支持。仓库测试/作者演练即使成立，也不能把这三项升级为市场事实。[S9][S11]
- [verified fact] 当前目标与阶段：README 主线是可信规则执行层；release manifest 为 `READY_FOR_SHADOW_WINDOW / SHADOW_ONLY_MVP`，质量结果 `INCONCLUSIVE_UNTIL_FORMAL_WINDOW`，生产强制执行被禁用。[S2][S10]
- [verified fact] 当前已决方向：ADR-0167 保留 monorepo、执行核心与创作簇边界，fast-decision 为 experimental，商业目标须访谈后再定；ADR-0170 已分离日常使用与 Pilot 取证。[S2]
- [unknown] 正在暂停的其它具体任务、开放 issue 状态、当前团队预算/维护容量：未发现证据；本次不访问外部 tracker，不推定取消、恢复或接管任何工作。[S12]
- [verified fact] 审计授权仅为读取并新增本报告；不包含代码/合同/既有裁决修改、招募、访谈实施、遥测、发布、外部写入或 commit/push。[S0]

| assumption that could reverse recommendation | observable reversal | decision implication |
| --- | --- | --- |
| [hypothesis; evidence E] 完整性问题尚未证明值得独立投入 | [hypothesis] 既有访谈门槛达成：至少半数合格样本无提示将其列为 top-3 痛点，且至少两次真实行为承诺。[S11] | [inference] 可由用户裁定进入限定场景的 continue / 规格阶段，不等于立即扩成完整平台。 |
| [hypothesis; evidence E] 主要需求是执行判断，而非作者效率或一次性审计材料 | [hypothesis] 访谈具体历史事件和投入一致指向另一任务，当前任务被多数目标用户否定。[S1][S11] | [inference] 转入 reposition 的用户决策，不凭仓库中哪个模块更多决定。 |
| [hypothesis; evidence E] 目标团队接受本地 / Shadow 证据，无需先获得生产阻断权威 | [hypothesis] 多数合格样本只接受生产阻断、且不愿为 Shadow 任务安排任何验证投入。[S2][S11] | [inference] 当前最小产品边界不成立；讨论 narrow/stop，而不是代理擅自开放 gate。 |

[inference] 不是直接 continue：help、单测和内部闭环不能证明需求；不是再做一次范围大收缩：已有 ADR 已收窄主线；不是立即 reposition/stop：有真实 Java 演练，但尚无足够客户反证。因此阻塞类型是“缺证据”，路由到一个无新代码的有限实验。[S2][S9][S11]

### Claim 与证据等级口径

[inference] 本报告的 `[verified fact]` 仅指本次/同一 Run 实测或所引文件确实记载的事实；`[inference]` 是由事实得出的判断；`[hypothesis]` 是未验证主张/实验提议；`[unknown]` 表示检查范围未建立证据。历史记录只证明“该记录如此记载”，不等价于独立客户测试。

[inference] 为落实 A–E 要求，采用以下**本报告分级约定**，不改项目术语或裁决：A=目标客户实际付费且持续使用的直接证据；B=可核对的真实资源投入/排期/预算方行动；C=目标用户具体过往事件的一手陈述但行动尚未核实；D=作者演练、代理样本、源码/测试等间接证据；E=未验证假设或未发现直接证据。等级与 claim 类型相互独立，D 级工程证据不能证明市场契合。

## 2. 定位、主用户/任务与自有能力边界

**[hypothesis; evidence E] 暂定定位：面向具备 Java 与 CI 操作能力的平台 / AppSec 执行者，在怀疑必需检查没有完整运行时，以冻结计划、Completeness 和三态结果替代“只看告警数/脚本绿灯”的判断，交付可复核的本地 / Shadow 执行证据；不代替分析引擎、人工批准或生产分支权威。** 机制依据已有实现，用户偏好及替代价值仍是假设；证伪条件见 §1，不宣称独有技术或市场优势。[S1][S2][S5]

### 2.1 角色与实际使用条件

| role | job / outcome | expertise, access, resources | authority / evidence |
| --- | --- | --- | --- |
| operator | [hypothesis; E] 平台/DevEx/安全工程执行者核对一次 Java 扫描是否漏跑 | [verified fact] 需目标仓库、可审阅配置、Candidate 或 delivery 资产；开发 checkout 路径需 Cargo、JDK21/Maven，打包路径仍需对应 runtime 与 Java 环境。[S2–S4] | [verified fact] 可读结果、起草/提交申请；不能批准或写 SCM。[S2][S7] |
| beneficiary | [hypothesis; E] 开发团队和质量负责人减少对不完整绿灯的误判 | [unknown] 实际误判频次、复核成本、无专家时成功率：未发现证据。[S9][S11] | [verified fact] 9 月 26 日记录是本地演练，不是该角色的独立使用研究。[S9] |
| maintainer / administrator | [inference] 提供 Runtime/规则资产、配置部署与保留 journal 的工程维护者 | [verified fact] 打包、签名、Server journal 和权限边界分别有实现；ServerMain 默认本地，显式 journal-path 才接持久 journal。[S3][S4][S7] | [unknown] 真实部署责任人姓名、值班容量和发行 SLA：未发现证据。 |
| approver | [verified fact] Pilot 质量裁决人与治理负责人确认人工质量及阶段升级 | [verified fact] 需要正式窗口、基线、质量复核，不可用机器耗时代替作者主动时间。[S10] | [verified fact] GO 或本地 PASS 不自动升级至生产阻断。[S2][S10] |
| buyer | [hypothesis; evidence E] 有 Java/CI/审计压力的预算负责人可能为可证明完整性投入资源 | [unknown] 谁付款、预算科目、价格、采购周期：未发现证据；不把 operator 当成 buyer。[S11] | [hypothesis; E] 切换意愿与商业可行性待验证。 |

[unknown] 无终端使用能力的人、辅助技术用户、离线受限环境与非标准 Java 构建的独立可用性：未发现用户研究证据；不能用“有 CLI”推出排斥，也不能用“有 IDE 核测试”推出可访问性已通过。[S4][S8]

### 2.2 能力归属（责任，不是目录分类）

| capability domain | boundary | owned outcome / handoff | owner |
| --- | --- | --- | --- |
| 执行身份、范围与完整性 | own | [verified fact] 配置/ScanPlan/结果身份与三态语义；错误不升级为 PASS。[S2][S5] | [inference] 执行核心维护者；下游按结构化结果消费，不再重算另一套完整性。 |
| 企业规则资产的受控供应 | own | [verified fact] 三模板 Candidate、验证与覆盖账本存在；创作簇经 RuleBundle seam 交接。[S2][S6] | [inference] 规则作者/创作簇维护者；不另建平行规则真相源。 |
| digest-only 申请、审计与恢复身份 | own | [verified fact] 客户端、Server 与 journal 负责请求/回执/状态一致性；接收不等于批准。[S7] | [inference] Thin Client/Server 维护者；最终批准归人和治理边界。 |
| 静态分析与可信构建输入 | integrate | [verified fact] Java 主 PMD、SpotBugs 补充；Resolver 交付工作区/构建上下文，不能猜测缺失的 ATTESTED 输入。[S5][S6] | [inference] Backend/构建系统提供者；ERF 消费约定，不复制分析引擎。 |
| IDE、CI 展示与外部状态适配 | integrate | [verified fact] IDE 调 rf；status 只交付可供 adapter 消费的契约，不持 SCM 凭据。[S8][S14] | [inference] IDE/CI/部署集成维护者；provider 权限归外部适配边界。 |
| 产品级生产权威与完整平台扩张 | defer | [verified fact] 当前 CLI 禁 release/gate、正式阶段升级有前置条件。[S2][S10] | [inference] 用户/治理负责人；本审计不提前授权，亦不取消已批准的窗口。 |
| 任意语言/任意规则 AI 生成、通用业务决策产品 | defer | [inference] 不纳入本轮主任务；保留三模板与 experimental 现状，不偷偷删代码或逆转 ADR。[S2][S6][S13] | [hypothesis; E] 只有新的直接用户证据才能支持另开产品决策。 |
| 客户端审批、源码上传、绕过信任门禁、重造分析器 | exclude | [verified fact] 当前明确拒绝 SCM/源码上传与客户端授予批准；白皮书排除重造静态分析引擎。[S2][S7][S10] | [inference] 无新增产品 owner；违反边界不是机会项。 |

### 2.3 六个核心镜头的结论

| lens | finding / evidence gap |
| --- | --- |
| positioning and goal coherence | [inference] 执行层定位与主命令一致；最大的未决矛盾是代码完备不等于独立购买任务，商业目标仍未定。[S2][S11] |
| users and capability domains | [inference] operator/作者/观察员/批准者已能分离；buyer 和维护资源未建立证据。[S2][S10][S11] |
| workspace / builder main path | [verified fact] 有七阶段工作流、Issue 路由与门禁；[unknown] 外部实际票据是否按该流程执行未检查。[S12] |
| reachability and closure | [inference] 扫描配置/打包已有路径，包装后的 explain/feedback 不能由源码存在推出可用；详见 P2/G2。[S3][S4][S7] |
| friction and support burden | [verified fact] 命令需要区分退出码和 GateDecision，部署/研究输入有前置条件；[unknown] 真实用户耗时与工单频率未发现证据。[S5][S10] |
| next action / signature outcome | [hypothesis; E] 值得验证的标志性结果（signature outcome）是“在零告警但漏跑时正确拒绝通过并指出缺口”，选择它的用户理由仍需 §6 的行为证据。[S5][S11] |

## 3. 能力证据与两张旅程图

### 3.1 五问必须分开

[verified fact] 矩阵检查范围为当前 HEAD 的文档/实现/测试和同一 Run 的限定命令；历史 E2E 演练标注日期。[S0][V0–V4] [inference] `distributed` 只陈述已查到的 package/入口，构建脚本存在不等于已有公开发行；`helped` 不从测试 PASS 推导客户价值。

| capability | promised | implemented | distributed | usable | helped |
| --- | --- | --- | --- | --- | --- |
| C1 项目接入与扫描（原 F1/F2） | [verified fact] 主线 init/config/doctor/scan，非生产权威。[S2] | [verified fact] launcher→配置→Runner，三态/JSON/SARIF；9/26 记录实际扫描。[S3][S5][S9] | [verified fact] package 脚本复制 rf、rf-cli、project-config、host/resolver；[unknown] 实际公开发行未发现证据。[S3] | [verified fact] 有 Candidate/runtime 等前置条件；[unknown] 当前全量门禁、陌生用户首用未验证。[S4][V4] | [verified fact; D] 作者演练有 183/183 单元与整改；[hypothesis; E] 目标客户受益未验证。[S9] |
| C2 规则创作与覆盖（F3/F4） | [verified fact] 受治理组件，不是执行新手主线。[S2] | [verified fact] 三模板、验证、固定 100/20/20 覆盖脚本。[S6] | [verified fact] checkout 命令调 Maven/脚本；扫描包清单未包含完整创作工具链。[S3][S6] | [inference] 适用于会提供 RuleIntent/测试与 Java 构建环境的作者，不是任意规范自动转规则。[S6] | [unknown] 真实作者工时未发现证据；[hypothesis; E] 提效未成立。[S10] |
| C3 签名交付 / Runtime pack（F7） | [verified fact] signed RuleBundle seam；本地不自授信任。[S2][S4] | [verified fact] 真 Java host 打包及签名检查存在，通用包为 reference fixture。[S3][S4] | [verified fact] 本地 pack 装配脚本存在；[unknown] 已发布生产包/信任材料及收件人未发现证据。[S4] | [inference] 需真实资产、runtime 身份和外部信任配置，不能仅执行 reference package 就宣称生产可用。[S4][S5] | [hypothesis; E] 用户是否愿承担该接入成本尚未验证；工程 fixture 属 D。 |
| C4 解释/起草（F5） | [verified fact] digest-only 的本地解释与申请，非批准。[S2][S7] | [verified fact] Rust 路由与纯核可达；本次纯核五断言通过。[S7][V3] | [verified fact] 扫描包未复制 rf-client；rf 的相关命令直接 cargo run，未走安装二进制分支。[S3] | [inference] 在无 ERF 源码的目标工作区不能仅凭该扫描包完成这些命令；未做独立安装实测。[S3] | [unknown] 非维护者完成导航→申请的证据未发现；需求为 [hypothesis; E]。[S8] |
| C5 提交、状态与审计（F6） | [verified fact] 显式 Server，RECEIVED_ONLY；不自动重试。[S7] | [verified fact] HTTP transport、状态恢复/身份校验、Server journal 和相关测试存在。[S7] | [verified fact] ServerMain 是本地入口，Thin Client 分发局限同 C4；[unknown] 真实部署未发现证据。 | [verified fact] HTTP local seam、TLS 归部署边界；UNKNOWN 用 ID 查询，不等于成功或再试指令。[S7] | [unknown] 真实治理者收件/处理及支持成本未发现证据；[hypothesis; E] 客户收益未验证。 |
| C6 外部状态 / IDE（F8/F10） | [verified fact] 薄适配，不复制权威。[S8][S14] | [verified fact] status 契约检查、两端纯核和绑定实现存在。[S8][S14] | [verified fact] README 列人工插件打包/安装；[unknown] 实际安装/上架未发现证据。[S8] | [unknown] 真 IDE 端到端、实际 provider 连接未验证；[verified fact] status 检查有意不写 SCM。[S14] | [hypothesis; E] 用户更愿经 IDE 使用或为此切换尚未验证。 |
| C7 Pilot / center（F9/F12） | [verified fact] 内部 Shadow 窗口与工程闭环，不是日常用户必填表单。[S2][S10] | [verified fact] observer、package/adjudicate、center 检查存在；历史演练有 journal→报告。[S9][S10] | [verified fact] 源码脚本和 release manifest 的观察入口；[unknown] 外部部署交接未发现证据。[S10] | [verified fact] 正式结论需窗口/基线/人工复核，缺测 INCONCLUSIVE；本地试运行不替代。[S10] | [unknown] 正式窗口、人工真值未发现完成证据；[hypothesis; E] 产品收益未成立。 |
| C8 experimental decision（F11） | [verified fact] 已降为 experimental。[S2] | [verified fact] 确定性 baseline adapter 产生 evidence。[S13] | [verified fact] checkout 子命令；扫描 package 不含该专用 binary。[S3][S13] | [inference] 能作内部机制实验，不能被当作通用智能决策服务。 | [hypothesis; E] 独立用户任务或付费理由未验证。 |

[inference] C3 的 `distributed` 结论应读为“本地装配/验证脚本可见，真实发行未知”，不把脚本生成的 RELEASE_ARTIFACT 标签当已对外分发事实。[S4]

### 3.2 Product-user journey

[inference] 下图是**源码/文档推演**，不是新客户测试；唯一历史实际扫描是 2026-09-26 作者演练，且包含作者写规则、整改和修复实现等干预。[S9]

```text
[hypothesis; E] P0 怀疑扫描漏跑 / 发现产品
 -> [inference] P1 获得正确 runtime + Candidate/delivery
 -> [verified fact] P2 init -> config validate / doctor
 -> [verified fact] P3 scan -> Finding + Completeness + GateDecision
 -> [inference] P4 理解 LOCAL_UNATTESTED 与结果
 -> [inference] P5 交给开发者 / 可选解释、反馈申请
 -> [inference] P6 整改复扫 / 按 ID 恢复查询 / 明确退出
```

| transition | actor / intent | entrypoint / prerequisites | output / next owner | failure / recovery |
| --- | --- | --- | --- | --- |
| P0→P1 | [hypothesis; E] 平台执行者寻找能解释漏跑的方案 | [verified fact] README 主线与 quickstart；[unknown] 实际发现渠道/切换动机未发现证据。[S2][S11] | [inference] 维护者交付 runtime 与规则资产，不把用户送到 research YAML。 | [inference] 若只需现有扫描器即可完成任务，退出而非强制平台迁移。[S11] |
| P1→P2 **首次接入断点** | [inference] 执行者获得可用运行环境 | [verified fact] package/production pack 不同；`rf init --repo-root` 需 Java 项目，scan 需规则输入。[S3][S4] | [verified fact] `.ruleops/config.yaml`、配置校验与 doctor 诊断；下一步交扫描者。[S2] | [inference] 缺 runtime/规则/可信构建事实，回到资产或构建提供者；不得 fake engine 冒充成功。[S4][S5] |
| P2→P3 **首个价值** | [inference] 执行者完成一次受限扫描 | [verified fact] `rf scan --candidate ... --format json/sarif`，配置预检、实际 host/resolver。[S3][S5] | [verified fact] 结果含 planned/executed、Finding/身份和 Gate；9/26 演练有真 FAIL。[S9] | [verified fact] 输入错误 exit2、执行不完整 exit3、信任失败 exit4；保留结构化诊断，不把失败变通过。[S5] |
| P3→P4 **高风险语义交接** | [inference] CI 消费者/复核者判断本次结论 | [verified fact] 必须读结构化 GateDecision；exit0 仅完整，Gate 仍可能 FAIL。[S5] | [inference] 交付“完整且有违规/完整无违规/不完整”的不同结果；当前权威仍本地。 | [inference] 若调用者只读 shell 绿灯，将丢失结果语义；成本是重新接线和复核，真实发生频率未知。不得为省事改既有退出码合同。[S5] |
| P4→P5 **分发断点** | [inference] 开发者解释或起草反馈 | [verified fact] scan 结果本身可读；可选 `rf explain/feedback/exception` 依赖 erf-thin-client 源码路径，当前扫描包没带该 binary。[S3][S7] | [verified fact] 申请按 digest 交 Server，RECEIVED_ONLY 交治理者；不授予批准。[S7] | [inference] 若无开发 checkout，该交接未建立完整分发路径；不应把装 Cargo/找源码无提示转嫁给结果接收人。[S3] |
| P5→P6 **中断与返回** | [inference] 申请人确认网络未知后的真实状态 | [verified fact] `rf submission status --request-id/--artifact-digest`；需显式 endpoint 和保留身份。[S7] | [verified fact] UNKNOWN/PENDING/RECEIVED_ONLY/REPLAYED 有区分，后续归 Server/治理负责人；本地整改后可复扫。[S7][S9] | [verified fact] UNKNOWN 不自动重试、不伪造回执；[inference] 无可用 endpoint 就保留申请并交维护者，或以本地结果结束。一次性复核可以合法结束，不强制建立 SaaS 循环。 |

### 3.3 Builder/operator workflow

[verified fact] 已声明的开发流程与产品使用不是同一张图；以下“流程存在”不等于当前外部 Issues 和每次发布都遵守它。[S12]

```text
[verified fact] B0 用户方向/ADR
 -> [verified fact] B1 规格与 GitHub Issue 路由
 -> [verified fact] B2 实现（Runtime / 受治理创作簇）
 -> [verified fact] B3 check.sh + 验收
 -> [inference] B4 package / 资产 / 部署交接
 -> [verified fact] B5 Shadow 观测与人工裁决
 -> [inference] B6 直接用户证据 -> 下一次用户决策
```

| transition | actor / intent | actual entrypoint / prerequisites | output / next owner | interruption / exit |
| --- | --- | --- | --- | --- |
| B0→B1 | [verified fact] 用户裁决方向，代理整理事实 | [verified fact] ADR-0167/0170、dev-workflow 的路由→拷问→成谱→成票。[S2][S12] | [verified fact] 决策进入 ADR、任务按 Issues 路由；[unknown] 本次未读取外部实际票据。 | [inference] 未定商业目标留在用户决策，不将审计候选偷写为权威。 |
| B1→B2 | [inference] 实现者交付约定契约 | [verified fact] rf→Runtime/Factory、contracts 与 signed bundle seam 已有代码路径。[S2][S3][S6] | [inference] 输出既有实现和测试，由验证者接手。 | [inference] 不重新提出“新建模板/状态恢复/日常快路径”——这些已经存在。[S2][S6][S7] |
| B2→B3 | [verified fact] 验证者执行全量 gate | [verified fact] `bash scripts/check.sh` 含 Maven、Cargo 与行为检查；当前调用未进入脚本：PATH 无 bash。[S15][V4] | [unknown] 本次无法确立 HEAD 全量红/绿；不发“产品验收通过”。 | [inference] 保留失败事实，交仓库维护者；本任务不修环境或代码，不拿历史 PASS 代替。 |
| B3→B4 **交付断点** | [inference] 发布/集成维护者把正确组件给操作者 | [verified fact] 通用 reference 包、真实 Java pack 检查、IDE 人工安装说明并存；[unknown] 真实收件包与责任人未发现证据。[S3][S4][S8] | [inference] 应交接可运行资产及边界，不仅“仓库测试通过”；具体修补须等用户任务确认。 | [inference] 若 gate/资产/安装证明缺失，状态保持未验收；不要静默换 fixture 或扩大 public surface。 |
| B4→B5 | [verified fact] 已登记观察员采集正式窗口 | [verified fact] `scripts/pilot-live-observation.sh`、window-report/evidence adjudicate，需窗口/基线/人工质量；源码脚本已能编译候选与绑定样本。[S10] | [verified fact] digest journal/证据包/裁决；下一 owner 为质量与治理负责人。 | [verified fact] 缺真值 INCONCLUSIVE，人工不可到位时按已有清单停止，不用合成数据替换。[S10] |
| B5→B6 **学习断点** | [hypothesis; E] 产品负责人据真实任务与投入决定是否继续 | [verified fact] 访谈脚本与既有阈值存在；[unknown] 完成的直接客户材料未发现证据。[S11] | [inference] 用证据回到 ADR-0167 决策 6，而非从模块数量推导市场。 | [inference] 没有客户行为或正式窗口真值时不再扩功能来制造“进展”；建议 §6 的单一实验。 |

## 4. 按优先级排列的 gap、成本与根因

[inference] 优先级是影响下一次产品决策的顺序，不是未经测量的严重度/工期分数；故“没有客户证据”高于“再打一个包”。安全拒绝、内部能力未公开、没有 GUI 本身不列为缺陷。[S0][S2]

| priority / ID | broken transition | observable cost / who pays | root cause vs symptom | evidence / confidence |
| --- | --- | --- | --- | --- |
| P1 / G1 | [inference] B5→B6：工程证据不能接到购买/继续决策 | [unknown] 真实预算、切换投入、重复使用未发现证据；[inference] 产品负责人只能继续猜，新增维护面无法获得收益依据。金额/频次不估算。 | [inference] 根因是决策缺直接行为证据；“模块越来越多”“故事摇摆”是症状，不能靠再做功能解决。 | [verified fact] ADR 商业目标待访谈，脚本已存在；[hypothesis; E] 痛点/预算未证实。[S2][S11] |
| P2 / G2 | [inference] P1→P2、P4→P5、B3→B4：扫描资产和结果消费者的可运行交接未闭合 | [verified fact] 两种包性质不同；解释申请命令走 cargo 而扫描包没带 rf-client；[inference] 接收者可能需要维护者补环境/源码。真实支持耗时未知。 | [inference] 根因是已实现组件与“哪个用户拿哪个包”的分发边界未统一；症状可能是离开开发 checkout 后无法继续，不是缺少另一套解释/反馈实现。 | [verified fact] package 清单、launcher、消费者测试已查；[inference] 干净安装失败为源码推断，未实测。[S3][S4][S7][S8] |
| P3 / G3 | [inference] P3→P4：完成状态、违规结论、权威三个维度容易被压成一个绿灯 | [verified fact] 完整 FAIL 仍 exit0；[inference] 下游若仅用 shell 成功判定，须重新解析结果并复核。实际误用/事故未发现证据。 | [inference] 根因是消费方需要理解明确合同；把 status-check 当 provider 发布或把 RECEIVED_ONLY 当批准是症状。不是退出码合同 bug，不移除拒绝。 | [verified fact] ADR-0126、thin-client 合同、README；误用频率为 [unknown]。[S2][S5][S7][S14] |
| P4 / G4 | [inference] B4→B5：内部工程演练不能给正式产品结论 | [verified fact] 作者主动时间、窗口基线和人工质量仍为必要输入；[inference] 成本落在真实作者/观察员/裁决人，不是普通扫描用户。工时未测量。 | [inference] 根因是外部真值未到齐，不是缺一个填表页面；将内部字段塞回日常路径是反向修补。 | [verified fact] manifest currentOutcome、作者成本状态、ADR-0170；[hypothesis; E] 作者提效尚未验证。[S10][S2] |

## 5. Opportunity ledger：先连接、减法与实验，再考虑新增

### 5.1 最高优先 gap 的替代办法

- [inference] 对 G1，“连接现有能力”可借用现成扫描证据解释机制，但不能把演练变客户证据；“减法”是不扩大用户/语言/平台承诺；“无构建实验”是已有访谈批次；“新建遥测/控制台/AI 作者”既未解决证据来源，也引入隐私和维护责任。因此选择无构建实验，减法作为它的范围约束，而非第二个项目。[S2][S9][S11]
- [hypothesis; evidence E] **signature outcome**：同一明确 Java 检查任务，在“零告警但必需单元缺失”时，不再得到模糊绿灯，而是可指出冻结分母、未完成单元与 INCONCLUSIVE；完整无违规与完整有违规保持可区别。[verified fact] 机制来源为现有结果/退出码合同，不是新算法。[S5]
- [hypothesis; E] 最小证明是目标用户能用已验收的同任务正反样本正确复核并愿意为其真实仓库安排 Shadow；复用理由应是下次同类争议仍需要该判断，而非“UI 新颖”。[unknown] 用户偏好、比当前工具更好及持续价值未发现证据；本次不运行新的样本、不开遥测、不称独有优势。[S11]

| ID | user/job | broken transition | evidence/confidence | proposed change | reused capability | owner/boundary | dependencies | smallest validation | maintenance cost | disconfirm condition | disposition |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| OP1 | [hypothesis; E] Java 平台/安全负责人 J1 | [inference] G1：B5→B6 | [verified fact] 访谈脚本已存在；[hypothesis; E] 需求未证实 / 判断中。[S11] | [inference] 完成既有一批无提示问题访谈，不写新代码、不先演示未分发能力。 | [verified fact] 现有访谈脚本、完整性合同、历史演练边界 | [inference] 用户指定研究负责人；不授予外联权限 | [inference] 批准与合格参与者；保留 Shadow 限定 | [hypothesis] 达到脚本的痛点+行为承诺门槛 | [inference] 一次性访谈/归纳成本，无新增运行面；实际工时未知 | [hypothesis] 多数不痛且无真实承诺；或到期样本不足保持 unknown | [inference] **now：唯一下一步，待用户授权** |
| OP2 | [hypothesis; E] 主用户不应承担内部研究负担 | [inference] G1/G4；D1/D2 | [verified fact] ADR-0167/0170 已分区；[inference] 无需再造一遍。[S2] | [inference] 减法：在 OP1 期间不扩公开承诺、不补新平台、不要将 Pilot 字段加回主流程。 | [verified fact] 现有主线与内部区、experimental 标签 | [inference] 产品负责人；保留已决 monorepo 和窗口 | [inference] OP1 的范围约束，不另立开发行动 | [inference] 访谈材料仅呈现已分发/明确受限能力 | [inference] 不增加代码维护；材料审阅成本未知 | [hypothesis] 直接需求证明其它任务更重要时，重新请用户裁决 | [inference] **now：并入 OP1 的减法约束** |
| OP3 | [hypothesis; E] 结果接收者需要解释/反馈 J4 | [inference] G2：P4→P5 | [verified fact] 有 rf-client 实现但已查扫描包未带；[inference] 分发判断中，未做干净安装实测 | [inference] 若任务被验证，再连接已有 client 与明确发行/安装说明；不新造解释器或 Server。 | [verified fact] rf-client、现有 request/status、已有 IDE 核 | [inference] Runtime/客户端发行维护者；Server 不新增第二份状态 | [inference] OP1 证明 J4 为必要下一跳、发行 owner 与可用基线 | [hypothesis] 干净目标工作区，仅拿已声明资产完成解释/起草/失败恢复 | [inference] 需要版本兼容/安装测试；未估工期 | [hypothesis] 用户只需要 scan JSON/SARIF，或自有管道已能处理，则不扩包 | [inference] **later：接线而非新能力** |
| OP4 | [hypothesis; E] CI 消费者正确理解 J1 | [inference] G3：P3→P4 | [verified fact] ADR-0126 和 status 契约已存在；[unknown] 实际误用未知 | [inference] 有真实误读时才补最小消费示例/交接说明，不修改退出码或自动授信。 | [verified fact] ScanResult、现有门禁/契约测试 | [inference] 消费者/文档 owner；权威仍在已有合同 | [inference] 观察到明确误读、现有输出足以正确判断 | [hypothesis] 用户在三种样本中区分完整性、违规、authority | [inference] 示例与契约同步维护，小于另建 UI；实际成本未知 | [hypothesis] 使用者已正确消费结果，无需再加 | [inference] **later：证据触发** |
| OP5 | [hypothesis; E] 面向作者/审计/全语言的其它市场 | [inference] 主定位未定，不能以功能叠加绕过 G1 | [unknown] 新 UI、更多语言、通用 AI/业务决策的直接需求未发现证据 | [inference] 不建运营控制台、不扩语言/任意规则生成、不将 fast-decision 升主线；保留现有实现。 | [verified fact] 三模板、现有 CLI 和 experimental seam 已可支撑已定范围 | [inference] 用户另行裁决；不得自动取消已存在模块 | [hypothesis; E] 需要独立直接需求与可维护 owner | [hypothesis] 先证明无法由现有任务路径完成的具体需求 | [inference] 新界面/引擎/服务均增加长期所有权，不估虚假数值 | [hypothesis] 现有 CLI 或集成满足任务即否决 | [inference] **not now：减法保留项** |

### 5.2 前一审计逐项处置（保留 ID）

[inference] 下表是本次审计处理意见，不改上一报告或任何裁决；“已存在”表示不能再开成新增能力。

| prior ID | disposition | rationale / link |
| --- | --- | --- |
| O1 | [inference] keep → G3 / OP4 | [verified fact] 权威边界已写明；[hypothesis; E] 首句误读风险未作用户测试，不升级为文档缺陷。[S1][S2] |
| O2 | [inference] needs-evidence → G1/G4 / OP1 | [unknown] ROI/主动时间未发现真值；不删除安全/质量标准。[S10] |
| O3 | [inference] keep → G2 / OP3 | [verified fact] 两类 pack 与分发差异可核对；[unknown] 真实发行未知。[S3][S4] |
| H1 | [inference] reclassify-as-existing → C1/P6 | [verified fact] 历史债务隔离在演练已有；[hypothesis; E] 是否是主购买理由待访谈，不另建 baseline。[S9] |
| H2 | [inference] reclassify-as-existing → C2 | [verified fact] 固定资产覆盖检查已存在；[inference] 不把它提升为通用分析服务。[S6] |
| D1 | [inference] merge → G1 / OP1/OP2 | [verified fact] 作者与执行者边界已裁定；[hypothesis; E] 用户更需要哪一类尚未知。[S2][S11] |
| D2 | [inference] reclassify-as-existing → OP2 | [verified fact] README 分区与 ADR-0170 已做；[hypothesis; E] 尚未证明仍造成实际操作障碍。[S2] |
| D3 | [inference] defer / reject-as-UI-request → OP5 | [verified fact] Center 是内部工程检查；[inference] 没有页面不构成缺陷，更不能由命名推导要新建控制台。[S10] |
| A 与 §6-A | [inference] keep as hypothesis → §2 / OP1 | [hypothesis; E] 保留执行主线假设及既有证伪测试，不把推荐当市场成立。[S1][S11] |
| B 与 §6-B | [inference] defer → OP5 | [unknown] 作者成本真值缺失；[inference] 不启动第二个作者产品或本次配对实验。[S10] |
| C 与 §6-C | [inference] needs-evidence / defer → OP5 | [hypothesis; E] 一次性审计材料接受度未验证，前报告的两人盲评是提案而非已执行结果。[S1] |
| F1/F2/F3/F4 | [inference] reclassify-as-existing → C1/C2 | [verified fact] 配置、扫描、三模板、固定覆盖已有，不重复提建设。[S3][S5][S6] |
| F5/F6/F7/F8/F10 | [inference] keep with five-question split → C3–C6 | [inference] 上篇 works 只指限定行为，不能推出分发与终端可用。[S3][S7][S8] |
| F9/F12、F11 | [inference] keep internal / defer expansion → C7/C8 | [verified fact] Shadow 研究、工程中心与 experimental 已有各自边界。[S2][S10][S13] |
| J1 | [inference] keep → 主任务/OP1 | [hypothesis; E] 先检验完整性判断的真实痛点。[S11] |
| J2/J5 | [inference] defer as primary job；keep internal | [verified fact] 作者和 Pilot 任务保留；[inference] 本轮不并列验证所有产品方向。[S2][S10] |
| J3/J4 | [inference] merge → J1 的返回/交接候选 | [hypothesis; E] 只有真实任务需要时才升级为最小产品必要范围。[S9][S7] |
| 上篇 §7 推荐、§8 第1–5条 | [inference] keep / refine | [inference] 定位 A 保留为假设，执行动作改为 pause to validate；五条不做项沿用，未反转既有 ADR。[S1][S2] |

## 6. One next action

**[inference] 唯一下一步：请用户授权并指定一名需求研究负责人，完成已有脚本规定的一批 8–12 场合格对象的问题发现访谈，提交一份能回到 ADR-0167 决策 6 的证据汇总。** 这是一项有边界的研究批次，不同时启动新功能、第二种产品实验、招募自动化或新遥测。[S11][S2]

| field | specification |
| --- | --- |
| owner | [inference] 用户指定的产品/需求研究负责人；[unknown] 实际人选未发现证据，不冒认已有任命。 |
| prerequisite | [inference] 用户批准本建议、允许联系对象并落实参与者；[verified fact] 合格样本条件沿用现有脚本：Java + CI 门禁 + 审计压力，前20分钟不得 pitch。[S11] |
| input | [verified fact] 现有访谈脚本、README/ADR 权限边界、前一审计 A 假设；[inference] 历史演练仅作受限机制材料，不展示未分发能力或宣称生产可信。[S1][S2][S9][S11] |
| output | [inference] 每场事件原话、现有替代、实际投入、具体角色和可核对行为承诺；一份汇总区分有效分母/无效样本/缺失数据、claim 类型、A–E 等级与支持/反证，交用户裁决。不得代写正面用户证词。 |
| exit | [verified fact] 原脚本支持信号是至少半数有效样本无提示列 top-3 痛点，且至少两次真实排期/意向/预算方引荐行为；[inference] 达标仅支持下一轮受限验证或规格讨论，不证明定价/商业规模/生产可信。[S11] |
| stop | [verified fact] 多数“现有绿灯够用/审计不需要/无预算”是既有证伪信号；[inference] 到脚本的2–3周窗口仍无足够合格样本或行为承诺，结束本批并报 unknown/反证，不无限续跑、不扩功能补进度。[S11] |
| next owner | [inference] 用户/治理负责人决定 continue、narrow、reposition 或 stop；研究者不直接改 CONTEXT/ADR，不自动重启暂停事项。[S2][S12] |

[verified fact] 上述访谈本次没有实施，也没有联系任何人或外部系统。[S0] [inference] 已登记 LIVE_SHADOW 仍按原治理条件处理；本报告既不取消该窗口，也不把执行窗口当成第二条新行动授权。[S10]

## 7. 证据缺口、检查范围与实际验证

### 7.1 未建立的证据

- [unknown] 真实客户访谈完成材料、真实排期/资源投入、付费、重复使用、流失、支持工单：未发现证据；因此需求/切换/商业可行性一律 `[hypothesis; evidence E]`，不是“市场不存在”。[S11]
- [unknown] 用户收到的真实安装包、生产 TLS/信任配置、provider 部署、IDE 安装验收、非专家首次使用：未发现证据；本次不把源码推演写成用户测试。[S3][S4][S8]
- [unknown] 正式窗口人工质量、作者主动时间、延迟/资源基线与最终 GO/NO_GO：未发现完成证据；不能把9/26演练与未来窗口混合。[S9][S10]
- [unknown] 当前全量构建门禁状态：本次命令未进入脚本；不对其它机器/历史红绿作推断，不归因或修复。[V4]
- [verified fact] scope 技能和 review-lenses 已完整阅读，六个核心镜头均有结论；额外仅采用与本任务相关的分发、权威、投入/替代、可逆性镜头，不为凑全景增列机会项。[S0]

### 7.2 检查范围与来源索引

[verified fact] 源码以 `ae61fc4` 的当前 checkout 为基准，同一 Run 已读事实只在 HEAD 未变化且无已跟踪差异的条件下复用；旧演练单独标日期。[V0] [verified fact] 检索使用 exact-anchor `rg` 和有界 Python，无语义检索工具、无网络抓取、无外部 issue 查询。前报告是定位输入，不是市场证据。[S1]

| source | actual inspected material |
| --- | --- |
| S0 | [verified fact] 本任务说明；`D:/code_space/.orca-workspaces/agent-skills/skills/product/product-scope-audit/SKILL.md`（121行）与 `references/review-lenses.md`（86行）全文；仓库/用户 AGENTS 规则 |
| S1 | [verified fact] `docs/research/product-audit-2026-09-30/01-product-positioning-audit.md`：定位/能力/全部 gap、JTBD、候选与证伪；只读且 hash 保持不变 |
| S2 | [verified fact] `README.md`、`CONTEXT.md:1–19`；`docs/adr/0167-product-audit-adjudication-batch.md`、`docs/adr/0170-separate-user-workflow-from-pilot-measurement.md`（同一 Run 全文，当前复核主线和暂停边界） |
| S3 | [verified fact] `rf:53–75,218–269`；`scripts/package-rf.sh` copy/build/manifest 清单；`Cargo.toml`、`runtime/thin-client/Cargo.toml`、`runtime/rf-cli/Cargo.toml` 名称/成员 |
| S4 | [verified fact] `scripts/ci-neutral-scan-check.sh:9–48,66–99`；`scripts/production-runtime-pack-check.sh:13–20,95–109,214–258` 及 pack/signature 检索；没有运行构建脚本 |
| S5 | [verified fact] `docs/adr/0126-rf-scan-exit-code-contract.md` 全文；README:115–162；`runtime/rf-cli/src/main.rs` exit/scan 符号；同一 Run 的 `runtime/rf-runner/src/main.rs` incomplete/authority 路径、`docs/adr/0169-attested-java-build-context-boundary.md` |
| S6 | [verified fact] 同一 Run 的 `control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java` 模板路由、对应 CandidateCompilerTest 测试入口；`scripts/authoring-validate.sh`、`scripts/phase4-coverage-ledger.sh:9–46`；只把已读片段视为实现证据 |
| S7 | [verified fact] `runtime/thin-client/src/main.rs` 路由、`runtime/thin-client/src/transport.rs` endpoint/回执/状态及 tests（特别 unknown_status_has_no_resource_and_never_becomes_a_retry_instruction）；`contracts/http/thin-client-v1.yaml:90–108`、`contracts/http/submission-status-v1.yaml`；同一 Run 的 ServerMain/HTTP 路由与本次 `control-plane/server/src/test/java/io/ruleops/server/RuleOpsJournalTest.java` 恢复/重放测试片段 |
| S8 | [verified fact] 同一 Run 的 `clients/vscode/README.md`、`clients/intellij/README.md`；本次 `scripts/ide-client-check.sh` 消费路径与 `clients/vscode/src/core.js` 五个纯核断言；未开 IDE |
| S9 | [verified fact] `docs/research/2026-09-26-real-github-e2e.md:16–51`：公开仓库副本、作者干预、整改、实现修复、13 Finding/183 单元、Shadow 边界与自审披露；历史 D 级工程记录 |
| S10 | [verified fact] `docs/pilot/release-candidate.md`、`specs/pilot/pilot-release-manifest.yaml` 全文；`specs/center/ruleops-center-closure.yaml` 状态；`scripts/pilot-live-observation.sh` 资产/候选/运行入口；同一 Run 的 `docs/pilot/operator-quickstart.md`、`docs/pilot/author-cost-baseline.md:65–75`、`docs/pilot/live-shadow-window-execution-checklist.md`、pilot-evidence CLI 与 center 检查 |
| S11 | [verified fact] `docs/research/2026-09-24-demand-discovery-interview-script.md` 本次全文；它是方案，不是已完成访谈 |
| S12 | [verified fact] `docs/agents/issue-tracker.md` 全文；`docs/agents/dev-workflow.md:7–35` 及同一 Run 的全文；不调用 gh，不对远端票据状态做结论 |
| S13 | [verified fact] 同一 Run 的 `runtime/fast-decision/src/main.rs:20–62` 与 ADR-0167 experimental / Node 协议探针补记；不再把既有探针恢复当新产品能力 |
| S14 | [verified fact] 同一 Run 的 `scripts/status-publication-check.sh:1–40` 与 README:158–162 外部 adapter 边界；未运行 provider 集成 |
| S15 | [verified fact] 同一 Run 读取 `scripts/check.sh:1–90` 及检查命令绑定；本次只尝试规定命令，未走绝对 Git Bash 路径重跑写入型全量构建 |

### 7.3 本次实际验证（非用户测试）

| ID | command / check | result / limit |
| --- | --- | --- |
| V0 | [verified fact] `git status --short`、`git rev-parse HEAD`；前报告 SHA-256 | [verified fact] HEAD 同上，已有09-29/09-30研究目录；前报告 hash `1d874ff0cd92d9acfb813d3fa4f9a85435d3d71f304d79bbc0d315e3e50cf39d` 已在验收时核对一致，未修改前报告。 |
| V1 | [verified fact] `E:/Git/Git/bin/bash.exe ./rf --help` | [verified fact] exit0；证明 launcher/命令枚举可用，不证明已打包全部子命令。 |
| V2 | [verified fact] `E:/Git/Git/bin/bash.exe ./rf gate` | [verified fact] exit2，`gate command is not enabled in this thin client`；拒绝符合边界。 |
| V3 | [verified fact] `node -e` 调已有 `clients/vscode/src/core` 的5个只读断言 | [verified fact] exit0：合法/越界路径各一条、回执不含/含虚假批准各一条、无显式 endpoint 拒绝一条；不写文件、不联网、不运行完整插件/Server。 |
| V4 | [verified fact] `rtk proxy bash scripts/check.sh` | [verified fact] exit1，`Binary 'bash' not found on PATH` / `program not found`；未执行到 gate。不使用绝对 Git Bash 绕过本任务写入边界来跑 target/.scratch 构建，不归因不修复。 |

[verified fact] 报告验收已通过：Python 断言核对七节 reading order、8项能力的五问表、两张旅程图、六镜头、12列 ledger、前审计 gap disposition、单一下一步字段、claim 标签、所引完整路径、HEAD 与前报告 hash；git 无已跟踪产品差异。[inference] 该验收只针对报告，不宣称产品交付门禁通过。
