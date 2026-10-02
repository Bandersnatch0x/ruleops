# LIVE_SHADOW 观察窗口执行清单（窗口：2026-10-02 → 2026-10-08）

> **内部流程提示**：本清单只面向 Pilot 观察员、质量裁决人和治理负责人，不是规则作者或
> 普通扫描使用说明。日常使用请先看 `docs/pilot/operator-quickstart.md`；不要为了运行普通
> 扫描而编辑 `AuthorCostRecord`。作者成本表只在正式 Pilot 研究需要验证该假设时启用。

> 目的：在已登记窗口内，用真实 LIVE_SHADOW 采齐**只能由人产生**的证据，产出一份
> 非 INCONCLUSIVE 的 `PilotAdjudication`（GO / NO_GO）。这是 product-scope-audit 的
> 「唯一下一步行动」，也是 goals-and-success-metrics 的本月里程碑。
>
> 纪律铁律：`LOCAL_SIMULATION` / `SYNTHETIC_SHADOW` 结果 **不得**充当产品结论；
> 无已知 FAIL 且缺必需证据时报告才是 INCONCLUSIVE；已知 FAIL 优先 NO_GO；测量开始后**不得**软化阈值或重写基线；
> 代理**严禁**伪造作者工时、质量裁决或运行证据（见 memory `v12-machine-frontier-exhausted`）。

## 0. 角色

| 角色 | 承担 |
| --- | --- |
| 治理负责人（`platform-engineering`） | 窗口开启授权、GO/NO_GO 裁决、阈值/样本冻结把关 |
| Rule Author（≥1 真实作者） | 手写 vs 模板配对，记录 author-active-time |
| 质量裁决人 | precision/recall/false-positive-regression 的人工配对复核 |
| 观察窗口责任人 | 每次运行的 journal 采集、窗口报告聚合 |

## 1. 窗口前跑道（2026-10-01，须早于 2026-10-02T00:00:00Z）

- [ ] 治理批准、实现验收、人员/基线就绪全部须早于 `2026-10-02T00:00:00Z`；任一未满足则新版不得激活，不得追溯或自动顺延，见 ADR-0176。
- [ ] 交付门禁全绿并留证：`bash scripts/check.sh`（最新阶段输出和完整日志入 `.scratch/gate/`；人工取证另存唯一目录）。
- [ ] Shadow 预检全绿：`./rf pilot delivery-check .scratch/pilot-delivery-check`。
- [ ] 三个登记仓库冻结、不换，锚定 pinned commit（`specs/pilot/live-java-repositories.yaml`）；
      再跑一次只读演练确认 checkout 无漂移：`scripts/pilot-live-observation.sh .scratch/pilot-live-shadow-preflight`。
- [ ] 冻结阈值确认（`specs/pilot/pilot-success-contract.yaml`）：作者时间中位数 ≤ 基线 80%、
      precision ≥ 0.95、recall ≥ 0.90、false-PASS = 0、complete-rate ≥ 0.99、latency p95 ≤ 2×、
      peak-memory ≤ 1.5×。
- [ ] **人工投入排定并落空记录**：从 `specs/pilot/author-cost-record-template.yaml` 复制空记录到
      `.scratch/`，填 run id / author id / UTC 起止 / path order 元数据（否则恒 INCONCLUSIVE）。
- [ ] 定义 precision/recall 标注样本集（Quality Benchmark holdout / near-miss）与配对复核规程。
- [ ] 采集 **latency / resource 候选基线**送治理负责人批准（阈值为「相对基线」，无基线则护栏恒 INCONCLUSIVE）。
- [ ] **顺序对冲**：同一作者先后写 manual/template 有学习偏差，按 path order 做 counterbalance，裁决时报告。

判据：`.scratch/` 证据链齐全，且没有任何「仅因缺埋点/缺基线」而 INCONCLUSIVE 的项。

## 2. 窗口内采集（2026-10-02 → 10-08，UTC inclusive，模式恒 LIVE_SHADOW）

- [ ] 每次运行以 LIVE_SHADOW 模式产 digest-only 观测：`./rf pilot observe ...`（不写 SCM、不上传源码）。
- [ ] 作者主动时间：对三类模板路径（SymbolRule / StructuralRule / ExistingRule）各配对采集
      手写 vs 模板生成的 active-time，只记主动分钟数，机器等待时间另记、不得替代：
      `scripts/pilot-author-cost-baseline-check.sh .scratch/author-cost-<run> <record>`。
- [ ] 质量：对同一提交做 baseline vs shadow 结果配对，人工复核 precision/recall 与
      false-positive-regression（`./rf pilot feedback ...`）。
- [ ] false-PASS 注入测试：注入不完整/超时执行，确认计数 = 0（绝不为 PASS）。
- [ ] 新增 vs 历史债务趋势：从 journal `run.new_finding_count` / `historical_finding_count` 取值。
- [ ] latency / resource：从 journal `run.latency_millis` / `peak_memory_bytes` 取 p95/max。
- [ ] 治理事件：rollback / active-exception / oldest-exception-age（`./rf pilot governance ...`）。

## 3. 窗口聚合与封存

- [ ] 单模式聚合窗口报告：`./rf pilot window-report ...`（仅 LIVE_SHADOW，勿混模式）。
- [ ] 封存证据包：`./rf pilot evidence package INPUT OUTPUT --contract-digest HEX --sample-digest HEX`，
      要求 requiredMetrics 全部 `completeness: COMPLETE`，窗口/样本 digest 与合同匹配。
- [ ] 聚合优先级：`FAIL > INCONCLUSIVE > PASS`（任一必需指标 FAIL 即整体 NO_GO）。

## 4. 裁决与毕业（2026-10-08T23:59:59Z 结束后）

- [ ] 人工裁决：`./rf pilot evidence adjudicate PACKAGE OUTPUT --contract-digest HEX --sample-digest HEX`
      → 产出 `PilotAdjudication`：全 PASS 且包完整 = `GO`；已知失败 = `NO_GO`；缺失/冲突/无效 = `INCONCLUSIVE`。
- [ ] 写一条 ADR（`docs/adr/`），记录三条 MVP 假设（作者成本 / 验证语义质量 / CI 债务边界）的 Go/No-Go 结论。
- [ ] 对「CI 是否升级到 ADVISORY」给出显式决定；**绝不**自动跳 `BLOCKING`；GO + 治理签字前
      SHADOW 边界不变（禁 SCM 写、禁源码上传、禁 protected-branch）。

## 5. 停止条件

- 若窗口内真实作者/质量裁决人无法到位 → 无已知 FAIL 且缺必需证据时报告为 INCONCLUSIVE，已知 FAIL 则为 NO_GO；**停止**，回到 product-scope-audit
  的「narrow」分支（收缩为可离线复跑的 Completeness/三态 Gate 演示资产，对外只宣称工程闭环）。
- 不得用预演/合成数据顶替真值以凑 GO。

## 6. 人工裁决输入清单与记录模板

本节是**待人工提供**的证据清单，不表示任何项已完成、窗口已激活或已作出裁决。
2026-10-02 的追加观测不等于完整七天窗口结束；工程 qualification `PASS` 不代替
人工输入或产品结论，见 [观测实现边界](observation-implementation.md)。

### 6.0 通用记录与纳入规则

- 每次人工取证使用新的 `.scratch/pilot-human-adjudication-<run>/`，不得覆盖历史原件或
  放入会被总门禁覆盖的 `.scratch/gate/`。下文路径均为**建议的新记录位置**，不是已有证据。
- 每项关联合同 id/version/digest、登记样本 digest、UTC 窗口、repository/commit、
  RuleBundle、原件路径和 SHA-256；按
  [证据包规范](../../specs/pilot/pilot-evidence-package.yaml) 的
  `metricId / provenance / window / completeness / artifactDigest / status` 映射指标记录。
  激活批准是前提附件，不新增或冒充 `requiredMetrics`。
- 人工签认、真实计时和质量真值**仅人工可提供**；脚本可采集机器结果、计算摘要和聚合，
  但不得代填签字、把占位符改为 PASS，或从普通 journal 推导 precision/recall。
  人工边界见 [指标口径](pilot-metrics-baseline.md) 与
  [机器可读指标定义](../../specs/pilot/pilot-metrics-baseline.yaml)。
- 先核对版本及激活前提，再判断记录能否纳入当前窗口。10-02 历史说明
  `.scratch/pilot-live-window-execution-20261002.md` 仍引用旧整月窗口；
  `.scratch/pilot-live-shadow-20261002-1/live-shadow-report.md` 是该次运行区间的报告，
  不是当前完整批准窗口的证据包。保留原字节，只在新记录中登记差异与纳入判断；
  不改旧窗口、不重 seal 旧包、不把捕获时 HEAD 写成生成时 HEAD，遵循
  [ADR-0176](../adr/0176-approved-pilot-window-reanchor.md)。

### A1. 作者主动时间（author-active-time）— 仅人工可提供真实工时与签认

- **需提供**：三条固定 RuleIntent 的 manual/template 配对主动分钟数、各自机器耗时及
  证据引用；run id、author id、UTC 起止、path order、测量备注。作者确认输入与验证边界
  等价，并披露顺序/学习偏差；机器等待与无关中断不得计入 active time。
- **模板/出处**：[AuthorCostRecord 空记录](../../specs/pilot/author-cost-record-template.yaml)、
  [配对口径与样本](../../specs/pilot/author-cost-baseline.yaml)、
  [记录与复跑说明](author-cost-baseline.md)。不另造作者成本格式。
- **记录位置**：新取证目录下 `author-cost-record.yaml`；按上述说明将检查报告/TSV
  输出到该目录的 `author-cost-check/`，在证据索引中关联 `author-active-time`。
- **验收判据**：冻结三样本齐全、run 元数据及两路径真实记录可复核，无占位符；以既有
  三样本中位数公式对照合同效率阈值，不修改基线。三类模板样本的结果不外推为全部规则
  的成本结论；不完整数据仍为 `INCONCLUSIVE`，已测不达标不得改成缺测。

### A2. 质量裁决 — 仅人工可提供标注真值、配对复核与签认

- **需提供**：质量裁决人、标注样本/holdout/near-miss 范围、同仓库同提交与冻结规则下
  baseline/Shadow 的配对引用、真实 TP/FP/FN 及计算分母、precision/recall 结果、
  false-positive-regression 分类与比较依据、分歧及处理记录。无真值/分母不得填零或 PASS。
- **模板/出处**：本页第 1、2 节；[合同质量条款](../../specs/pilot/pilot-success-contract.yaml)
  `measures.quality`；[指标定义](../../specs/pilot/pilot-metrics-baseline.yaml)
  `feedback.regression_outcome`；[观测实现](observation-implementation.md)。这些文件未提供
  专用人工质量 YAML 空表，使用本节通用记录模板，不能将 fixture 结果当人工裁决模板实值。
- **记录位置**：新取证目录下 `quality-review.md`；反馈通过既有 `./rf pilot feedback`
  写入本次独立 journal，索引分别关联 `precision`、`recall`、`false-positive-regression`。
  共享证据仅含 digest/计数/裁决引用，不上传源码、路径或完整 Finding。
- **验收判据**：三项均有可复核的配对、真值及分母，按冻结合同容差逐项判定；单有
  false-positive feedback、扫描 PASS 或仓库覆盖率不足以证明 precision/recall。
  未解决分歧记为 `CONFLICTING`，不得标 `COMPLETE/PASS`。

### A3. 经批准的 latency 与资源基线 — 仅人工可提供批准与适用范围签认

- **需提供**：候选基线原始测量及摘要、p95 latency（毫秒）、峰值内存（字节），
  对应环境、仓库/提交/RuleBundle、采样与汇总口径；治理负责人的批准来源、适用范围和
  可核验完成时刻。机器可测候选值，但测得值不等于已批准基线。
- **模板/出处**：[合同运维条款](../../specs/pilot/pilot-success-contract.yaml)
  `measures.operations`；[指标来源](../../specs/pilot/pilot-metrics-baseline.yaml)
  `run.latency_millis / run.peak_memory_bytes`；[观测实现](observation-implementation.md)
  与 [ADR-0176 激活条件](../adr/0176-approved-pilot-window-reanchor.md)。
- **记录位置**：新取证目录下 `operations-baseline-approval.md`，引用候选基线原件和批准
  附件；本次 journal/window report 保留实际值与基线对照，分别索引 `scan-latency`、
  `resource-consumption`。这是人工批准记录，不新增机器 schema。
- **验收判据**：基线数值、单位、适用范围及事前批准均可复核，按合同既有相对阈值判断；
  当前运行的 p95/max、预检耗时或脚本默认值不能自行变成基线。测量开始后不得重写基线
  以求 PASS；若事前批准无法证明，按 A5 处理，不补造日期或倒签。

### A4. false-PASS 注入与不完整/超时验证 — 仅人工可提供覆盖确认与证据接纳

- **需提供**：观察员登记独立失败注入方案，逐例列出注入条件、预期非 PASS、实际
  completeness/decision、运行身份、原始输出及摘要，至少覆盖不完整执行与超时；
  裁决人签认覆盖与结果。执行、采集和计数可自动化，人工不得代造运行日志。
- **模板/出处**：[合同安全条款](../../specs/pilot/pilot-success-contract.yaml)
  `measures.security`；[指标定义](../../specs/pilot/pilot-metrics-baseline.yaml)
  `false-pass-count`（独立注入统计）；本页第 2 节及
  [可信 Pilot 失败语义](../runbooks/phase4-trusted-pilot.md)。工程验证可引用
  `scripts/phase3-incomplete-evidence.sh`、`scripts/phase3-timeout-evidence.sh` 的既有入口，
  每次显式指定新输出目录，不覆盖历史取证。
- **记录位置**：新取证目录下 `failure-injection-review.md` 与各例独立输出目录；索引
  `false-pass-count`，区分工程资格证据与获准纳入的 Pilot 证据。
- **验收判据**：独立注入及不完整/超时例真实执行，全部不得产生 PASS；false-PASS 按合同
  为零才达标，出现误 PASS 为已知失败。普通成功扫描统计中的零值不能证明注入覆盖，
  工程 fixture PASS 不能冒充 LIVE_SHADOW 产品结论；未执行/缺证据保持未完成。

### A5. 窗口激活前提 — 仅人工可提供治理批准、验收与人员/基线就绪签认

- **需提供**：治理批准、实现验收、人员/基线就绪三类独立来源；逐项列负责人、完成时刻
  （带时区，可换算 UTC）、原件摘要与适用合同版本。人员包括作者、质量裁决人和观察员；
  基线就绪须列明合同所要求的作者成本基线及 A3 的批准证据，不能用 A1 空表替代。
  脚本全绿可作实现证据，不能代签验收。
- **模板/出处**：[合同批准引用](../../specs/pilot/pilot-success-contract.yaml)
  `adjudication.approvalReference`；[登记窗口/样本](../../specs/pilot/pilot-observation-plan.yaml)；
  [样本说明](live-java-repositories.md)；[ADR-0176 Activation](../adr/0176-approved-pilot-window-reanchor.md)。
- **记录位置**：新取证目录下 `activation-prerequisites.md`，三项分别登记来源及完成时刻，
  引用完整门禁日志、人员安排、基线批准原件；不得在本清单预先勾选完成。
- **验收判据**：三项必须**全部早于 `2026-10-02T00:00:00Z`**，等于起点也不满足。
  合同 `APPROVED`、计划 `REGISTERED` 或一次观测成功均不能证明已激活；ADR 已说明
  批准来源仅有日期且未证明人员/基线就绪，不得补造精确审批时刻。现在提交旧证据必须
  证明前提确在起点前成立；缺证或实际逾期不得追溯激活、自动顺延或缩短窗口，应由治理
  负责人重新申请明确的未来窗口。本清单不批准或修改任何窗口。

### 6.1 通用人工记录模板（非机器输入格式）

除 A1 复用已有 YAML 外，A2–A5 可逐项复制以下空白 Markdown 到上述新记录位置；
填写前保持 `HUMAN_INPUT_REQUIRED`，不得将这份空表直接交给封包命令冒充有效证据。

```text
item_id / metric_ids: HUMAN_INPUT_REQUIRED
responsible_person / reviewer: HUMAN_INPUT_REQUIRED
contract_id / version / digest: HUMAN_INPUT_REQUIRED
sample_digest / repository_commit / rule_bundle_digest: HUMAN_INPUT_REQUIRED
window_start_utc / window_end_utc: HUMAN_INPUT_REQUIRED
actual_data_and_method / coverage_and_denominators: HUMAN_INPUT_REQUIRED
source_paths_and_sha256 / provenance: HUMAN_INPUT_REQUIRED
approval_or_review_source / completed_at_with_timezone: HUMAN_INPUT_REQUIRED
acceptance_against_frozen_contract / unresolved_gaps: HUMAN_INPUT_REQUIRED
completeness / metric_status / reviewer_confirmation: HUMAN_INPUT_REQUIRED
```

### 6.2 提交裁决前复核

- [ ] A1–A5 都有真实负责人、原件引用、摘要与验收说明；激活前提单独成立，缺项显式列出。
- [ ] 对照 [证据包 requiredMetrics](../../specs/pilot/pilot-evidence-package.yaml) 核对**全部**
  指标；本节五项只是人工缺口，不替代 complete/inconclusive rates、违规/债务趋势等机器证据。
- [ ] 合同/样本 digest、包与 record 的窗口匹配，完整性如实填写；不混模式、版本或新旧窗口。
- [ ] `platform-engineering` 治理负责人在完整窗口结束后复核并作出第 4 节裁决；命令执行
  不等于人工批准。按规范 `FAIL > INCONCLUSIVE > PASS`：有已知必需指标 FAIL 则 NO_GO；
  无已知 FAIL 但缺失/冲突/无效则 INCONCLUSIVE；仅完整包且所有必需指标 PASS 才能 GO。
  无论何种结论，SHADOW、禁止 SCM 写入/源码上传及 protected-branch 权限边界不变。

## 相关文件

- 阈值权威：`specs/pilot/pilot-success-contract.yaml`
- 窗口/样本：`specs/pilot/pilot-observation-plan.yaml`、`specs/pilot/live-java-repositories.yaml`
- 指标字段/证据包 schema：`specs/pilot/pilot-metrics-baseline.yaml`、`specs/pilot/pilot-evidence-package.yaml`
- 口径文档：`docs/pilot/author-cost-baseline.md`、`release-candidate.md`、`observation-implementation.md`
