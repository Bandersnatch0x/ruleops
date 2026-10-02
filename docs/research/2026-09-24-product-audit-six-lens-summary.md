# 产品六镜审计汇总报告（2026-09-24）

> 方法：以外部产品审计 skill 套件（product-positioning-audit、product-scope-audit、
> target-user-and-jtbd、goals-and-success-metrics、feature-scope-triage、idea-stress-test）
> 对本仓库做只读审计，六项串行执行、后项复用前项定位输出。本文件为持久汇总。
> 所有结论均系于文件/命令级证据；审计本身不改产品，产品决策见文末「待裁批次」。

## 0. 一句话总体结论

工程上近乎完成、纪律极强，但产品价值从未经真实用户验证。六项审计罕见收敛到同一动作：
**停止建造代码，转去验证需求与能力。** 「机器实现前沿已出清」，剩余不确定性全在代码之外
（真实用户、真实数据、部署决策），再写模块对当前决策零信息增益。项目已正确停在
`READY_FOR_SHADOW_WINDOW`，`2026-10-01 → 2026-10-31` 有已登记 LIVE_SHADOW 窗口。

## 1. 推荐定位（audit #1，后续四项据此对齐）

> **面向平台工程与安全/合规工程团队的「可信规则执行层」——用冻结 ScanPlan 的
> Completeness、PASS/FAIL/INCONCLUSIVE 三态 Gate 与签名执行证明，取代「0 告警即通过」。**
> （命中约 60–70% 现有代码，几乎全部 runtime + backend-host。）

收窄说明（audit #2）：定位里的「签名可信执行 / 受保护分支强制 / SCM 回写 / IDE 体验」
目前是**已实现但未分发**或**待验证假设**——shipped `./rf` 把 `release`/`gate` 显式禁用
（`rf:272-274`），扫描结论恒为 `LOCAL_UNATTESTED`，不在已交付边界内。

三处定位漂移：
- 名字与白皮书卖 authoring（规则工厂），代码最深处却是 runtime 的信任/执行/完整性；
- 「AI authoring」实为确定性 BM25 + 模板检索、无 LLM（`docs/pilot/authoring-capability-baseline.md`）；
- 「多语言 / Backend-neutral」仅 Java+PMD+SpotBugs 是真实后端，其余为 fake host（ADR-0086）。

## 2. 目标用户与 JTBD（audit #3）

- **主用户**：Platform Engineering / RuleOps 治理负责人（`platform-engineering`）。强证据：
  `specs/pilot/pilot-success-contract.yaml` 合同 owner 与裁决 decisionOwner 均为它。
  核心 job：用可信证据 + 三态 Gate 做**可审计的规则晋级 GO/NO_GO 裁决**。
- **次用户**：Rule Author。强证据但价值仍是假设（`author-active-time` 全为 `HUMAN_INPUT_REQUIRED`）。
- **三类反用户**（三处漂移的镜像）：想要 IDE 实时阻断/自动修复的开发者；想买「更强扫描器」
  替换 Sonar/PMD 的采购方；想让 LLM 在 CI 判违规的 AI 拥趸。
- `security-platform`（拥有 87/100 条规范）是最强量化证据，但当前是需求来源而非工具用户，
  列「暂不服务」。

## 3. 目标与成功指标（audit #4，锚定既有 PilotSuccessContract）

- **North Star**：作者主动时间中位数 ≤ 手写基线 80%（合同已预注册；当前无真值）。
- **支撑指标**：precision ≥ 0.95 / recall ≥ 0.90；complete-rate ≥ 0.99（即「取代 0 告警即通过」
  的直接度量）；new/historical-debt-trend ≤ 0。
- **护栏**：false-PASS = 0；scan-latency p95 ≤ 2×；authority 恒 SHADOW、全 denied。
- **关键批评**：定位卖点是「执行可信」，但合同里唯一的**行为级**指标是作者成本；
  「团队依据 Gate 结果采取行动 / 采纳率」这一终极行为在 SHADOW 阶段无法观测
  （`需要埋点/日志`，须留待 ADVISORY 阶段）。
- **反目标**：功能堆叠冒充进展（#100/#101/#92 合入但指标仍 INCONCLUSIVE）；
  用合成/预演数据冒充产品结论；测量开始后软化阈值。

## 4. 特性分诊（audit #5，33 项，Keep ≤ 1/3 达标）

| 判定 | 数量 | 代表项 |
| --- | --- | --- |
| Keep | 7（21%） | `rf scan`、json/sarif 导出、三态 Gate、ScanPlan/Completeness、Trust 栈、Java Backend Host |
| Keep-as-is | 9 | doctor/init/config、status contract-check、git-scope、affected-scope、incremental-baseline |
| Defer | 10 | pilot 观测族、center check、explain、feedback/exception/submission、status deployment-check、control-plane server |
| Spin-out | 5 | Authoring/Factory 套件、Ontology、IDE clients |
| Cut（须人工审批） | 2 | `rf decision evaluate`（fast-decision）、`backend-hosts/node-fake` |

**新手 10 分钟主线**（仅 Keep/Keep-as-is）：`rf init → rf config validate → rf doctor →
rf scan --format text`（拿到三态+Completeness，体验「执行不完整即 INCONCLUSIVE」）
`→ rf scan --format sarif` 接入 CI。

## 5. 投资决策（audit #6）

- **建议 PAUSE**：实现证据强（B，局部 A），需求证据近乎为零（E 级；零客户、零访谈、零 LOI）。
- **唯一瓶颈信念**：是否存在一类企业 Java 治理负责人，真实且有预算地感到「无法证明 CI 扫全了、
  0 告警是假安慰」这个痛，强到愿为此改用/加购工具。
- **差异化确有市场缝隙**（C/D）：Sonar 质量门在分析不完整时仍可 PASS，且无扫描完整性的
  密码学 attestation；「completeness ≠ 0 findings + 签名证明」是真实缝隙。
- **崩塌条件**：访谈中无提示下说不出此痛 / 十月 pilot 作者成本或 precision-recall 不达标 /
  买家答「信 Sonar 绿灯就够了」。
- **若证伪则转向**：做嵌入 Sonar/CodeQL/CI 的「扫描完整性证明」插件（SLSA/in-toto 风格），
  而非要企业迁移到 RuleOps Center；或锁定金融/医疗/防务受监管审计细分。

## 6. 跨审计三条硬结论

1. **不要再写机器代码**（scope + idea-stress-test + 特性分诊三方独立同证）。#100/#101/#92
   均被人工/部署决策阻塞，窗口期投入等于反目标。
2. **两条并行验证跑道，需求权重高于能力**：
   - 需求跑道（零现金零新代码）：8–12 场问题发现访谈——
     见 [2026-09-24-demand-discovery-interview-script.md](2026-09-24-demand-discovery-interview-script.md)。
   - 能力跑道：执行已登记 LIVE_SHADOW 窗口，采齐作者工时与 precision/recall 真值——
     见 [../pilot/live-shadow-window-execution-checklist.md](../pilot/live-shadow-window-execution-checklist.md)。
3. **一个前置决策需用户回答**：这个项目的目标产出是什么（VC-scale 战略产品 /
   受监管细分工具 / bootstrapped 小生意）？白皮书按战略产品口吻写，实际交付是单人严谨作品；
   此错配直接决定「该投多少」。

## 7. 待裁批次（已裁定，记入 [ADR-0167](../adr/0167-product-audit-adjudication-batch.md)）

以下均为**产品/范围决策**，按 AGENTS.md「一切决策问题由用户作答」由用户裁定于 2026-09-24。
裁定结果与执行边界见 ADR-0167，具体修改见 §8。

1. **定位承诺是否收窄** —— 裁定 **A（是）**，已执行：README/CONTEXT 开篇收窄为「可信规则
   执行层」，「可执行/强制」移入待验证假设。
2. **文档首屏是否去脚手架** —— 裁定 **A（是）**，已执行：README CLI 清单重排为 主线 /
   创作·工厂 / Shadow 内部 三区。
3. **fast-decision 是否 Cut** —— 裁定 **A（降级 experimental，不删）**，已执行：仅 README
   标注 + 撤出主线，代码与 `fast-decision-check.sh` 门禁不变。
4. **node-fake host 是否 Cut** —— 裁定 **A（删）**，已执行：删除 `backend-hosts/node-fake/`，
   连带清理 `integration.rs` 两个 node 测试与 `phase1-exit-demo.sh` node 段。
5. **Authoring/Factory/Ontology/IDE clients 是否 Spin-out** —— 裁定 **B（拆分）**，执行形态
   定为 **monorepo（不拆外部仓）**：拆分意图改由 monorepo 内的强制 seam（契约 + 签名
   RuleBundle）兑现，seam 已验证为干净（无 Rust→创作侧源码依赖），边界见
   [monorepo-boundary.md](../refactor/monorepo-boundary.md)。
6. **项目目标产出定档** —— 裁定 **先跑需求访谈再定**：暂不锁定，绑定十月访谈 + LIVE_SHADOW
   窗口证据，回填后另立 ADR 定档。

## 8. 问题修改建议（本次已落地的具体改动）

| # | 问题 | 修改前 | 修改后 |
| --- | --- | --- | --- |
| 1 | 定位 overpromise「可执行/强制」但 shipped `./rf` 已禁用 | 「可验证、可发布、**可执行**、可审计、可演进」 | 「可信规则执行层：Completeness + 三态 Gate + 签名证明取代『0 告警即通过』」+ 边界注（release/gate/promote 禁用、权威恒 SHADOW，可执行为待验证假设） |
| 2 | README 首屏 ~30 条命令淹没新手主线 | 单个平铺 `text` 块列全部命令 | 三区：主线（init/doctor/config/scan）· 创作/工厂（受治理组件）· Shadow 内部/预发布 |
| 3 | `rf decision evaluate` 偏离执行完整度定位、无需求信号 | 与主线命令并列 | 归入内部区并标 `# experimental`；代码/门禁保留 |
| 4 | node-fake 与 Rust+Java 覆盖重复、增加维护面 | `backend-hosts/node-fake/` + 2 个 node 测试 + phase1 demo node 段 | 三者一并移除；跨语言一致性收敛为 Rust+Java |
| 5 | 名字卖 authoring，代码最深处是执行层，创作侧淹没主线 | 单体内创作/工厂/本体/IDE 与执行层混居 | 保留 monorepo、不拆外部仓；README 主线只列执行核心 5 条、创作/工厂分区；以契约 + 签名 RuleBundle 为强制 seam（已验证无 Rust→创作侧源码依赖），边界记入 monorepo-boundary.md |
| 6 | 目标产出未定，导致「该投多少」无法回答 | 白皮书战略口吻 vs 单人严谨交付错配 | 暂不锁定，前置于十月访谈 + 窗口真值，回填后立 ADR |

> 尚未执行、需后续动作的项：决策 5 的物理拆分（S2，须用户主动驱动、G0 门后）；决策 6 的
> 目标产出定档（须访谈/窗口证据回填）。二者均非代理可在当前自主会话完成。

> 注（历史）：本报告审计时的原始「待裁项 + 推荐项」措辞已由上方裁定结果取代；裁定权威见 ADR-0167。
