# 交接文档 — ruleops 交付落差审计 → 作者成本基线实测

- 日期：2026-09-18
- 仓库：`/home/kali/文档/ruleops`
- 分支：`codex/phase1-contract-factory`
- 审计基线 commit：`ef926dc87dfc0f4101c5588fbdace215ddae0c67`

## 一、上一会话做了什么

只读审计，回答"当前代码对 Plan 完成多少、为何交付远不及预期"。**没有提交、没有推送、没有改票、没有改动工作区任何文件。**

结论一句话：Phase 0–3 的工程机制与证据链基本建齐，Phase 4 的产品出口未通过；Plan 真正要验证的三件事（非专家是否显著降低规则生产成本、Verification 能否把 Candidate 变成可信资产、CI 能否在不破坏历史债务的前提下阻止新增违规）一件都没实测。

## 二、当前仓库状态（勿破坏）

- `git fetch origin` 后当前分支与 `origin/codex/phase1-contract-factory` 一致，**无落后**。
- `origin/master`（`7135c00`）与 HEAD 的 tree 相同（`017b9498…`），内容等价，**无需合并**；若将来合并是 squash 等价物，会制造假历史。
- 工作区有**大量未提交成果必须保留**：`runtime/pilot-observer/`、`runtime/rule-bundle/`、`specs/center/`、`specs/pilot/{live-java-repositories,pilot-observation-plan,pilot-release-manifest}.yaml`、`docs/pilot/{release-candidate,observation-implementation,ruleops-center-closure,live-java-repositories}.md`、`docs/adr/0130`–`0132`、`scripts/pilot-*.sh`、`scripts/ruleops-center-check.sh`，以及 `rf`、`scripts/check.sh`、`README.md`、`CONTEXT.md`、`runtime/pilot-metrics/src/lib.rs` 的修改。
- `.scratch/` 为临时证据目录，不进版本库；本次审计新增 `.scratch/audit-shadow-pilot-20260918/`（可删）。

## 三、审计已确认的五处落差（下一步工作的靶子）

1. **20 条候选规则中只有 6 条走自研模板。** 14 条是 `existing-rule`（包一层 PMD 自带规则），6 条 `structural-node`，1 条 `method-call`。工厂价值最大的样本最小。
2. **Plan §31.3 的 Week 4 Go/No-Go 从未真正裁决。** 阈值经 #79 写入 `specs/pilot/pilot-success-contract.yaml`（作者活跃时间 ≤ 基线 80%、precision ≥0.95、recall ≥0.90），但**基线值与任何实测数字都是空的**。
3. **已提交版本的"三仓库 Shadow"是脚本合成的 fixture 仓库。** 见 `scripts/phase4-shadow-pilot.sh`（现场生成只含 `System.exit(i)` 的仓库）。真正扫描公共仓库的清单在工作区未提交，且只跑 1 条规则，窗口报告仍 `INCONCLUSIVE`。
4. **治理与注册中心是内存态。** `runtime/release-registry/src/lib.rs` 自述持久化与 SCM 适配器在本 crate 之外；`./rf release|gate|feedback|exception` 一律拒绝执行；仓库无 `.github` CI 配置。
5. **存在两条冲突的达标线**：已提交 Phase 4 报告说计数型 20 条，白皮书系文档说 10 条，均非真实试点结果。

附带工程问题：17:40 一次全量 `scripts/check.sh` 在 shadow 步骤中断且 `shadow-console.log` 为 0 字节（无失败原因）；单独复跑该步骤 `PASS`。属偶发，但门禁失败可观测性不足。

## 四、下一步任务（用户已认可的方向）

**做作者成本基线实测的最小切片**，产出可进 #79 的实测依据：

1. 选 3 条规则，覆盖不同模板类型（建议 1 条 `method-call`、1 条 `structural-node`、1 条 `existing-rule`，例如 `corp.java.no-system-exit`、`corp.java.no-clone-method`、`corp.java.avoid-duplicate-literals`）。
2. 建立**可复跑的作者工时记录模板**（人工手写对照 vs 模板生成两条路径），落 `specs/pilot/` 与 `docs/pilot/`。
3. 产出实测基线文档，写清测量方法、样本、绝对值与相对值，**不得事后调阈值**；缺测部分保持 `INCONCLUSIVE`。

注意事项：
- #79 已 `CLOSED`（合同已 `APPROVED`）。实测结果需**新开票**并经人工裁决，不要重开 #79。
- 该任务必然需要**人为输入**（真实作者的工时数据）。若下一个 agent 无法获得人工数据，正确做法是交付可复跑的测量工具与空模板，并显式标注 `HUMAN_INPUT_REQUIRED`，不得臆填数字。

## 五、机构性约束（务必遵守）

- 仓库级权威：`AGENTS.md`、`CONTEXT.md`、`docs/adr/`、`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md`、`docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md`。
- 工作流：`docs/agents/dev-workflow.md`（七阶段）；票据路由：`docs/agents/issue-tracker.md`（GitHub Issues，`gh` CLI）。**票只追加不改写**。
- 交付门禁：`bash scripts/check.sh` 必须全绿方可交付；完整日志落 `.scratch/` 后整体读取。
- 工作语言中文；机器面（frontmatter、字段名、代码标识）用英文。
- 用户未授权前**不要** commit / push / 合并 / 改票 / 改远端；工作区未提交成果必须保持原样。
- 外部思路进入产品内容使用自研措辞；交付前检查外部名边界。

## 六、建议下一个 agent 调用的技能

- `implement` — 按票实现作者成本基线切片。
- `tdd` — 若测量工具需要测试先行。
- `code-review` — 交付前的两轴评审（标准轴/规格轴）。
- `to-tickets` — 把基线与测量切分拆成带阻塞边的可执行票。
- `research` — 若需要外部佐证"规则开发成本"的度量口径。
- `handoff` — 再次交接时复用。

## 七、可直接复用的证据锚点

- 阶段出口对照：`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md` §31（MVP/Go-No-Go）、§32（Phase 0–4）、§34（验收 25 条）。
- Phase 1 出口：`scripts/phase1-exit-demo.sh`（自述 fake 组件、Candidate 未签名）。
- Phase 2 出口：`.scratch/phase2-java-local-demo/phase2-evidence.md`。
- Phase 3 出口：`.scratch/phase3-trusted-ci/phase3-trusted-ci.md`（17/17）、`.scratch/phase3-trusted-scan/phase3-real-scan.md`（7/7）。
- Phase 4 报告：`docs/pilot/phase4-pilot-report.md`（自述 live metrics 未测）。
- 成功契约与阈值：`specs/pilot/pilot-success-contract.yaml`、`docs/adr/0129-approve-pilot-success-contract.md`。
- 真实仓库清单（未提交）：`specs/pilot/live-java-repositories.yaml`。
- 覆盖率台账：`specs/coverage/*.yaml`（14 `EXISTING_RULE` + 6 `NEW_RULE_REQUIRED`）。
