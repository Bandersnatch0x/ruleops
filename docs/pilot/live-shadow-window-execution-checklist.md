# LIVE_SHADOW 观察窗口执行清单（窗口：2026-10-01 → 2026-10-31）

> 目的：在已登记窗口内，用真实 LIVE_SHADOW 采齐**只能由人产生**的证据，产出一份
> 非 INCONCLUSIVE 的 `PilotAdjudication`（GO / NO_GO）。这是 product-scope-audit 的
> 「唯一下一步行动」，也是 goals-and-success-metrics 的本月里程碑。
>
> 纪律铁律：`LOCAL_SIMULATION` / `SYNTHETIC_SHADOW` 结果 **不得**充当产品结论；
> 缺任一人工输入时报告**必须**保持 INCONCLUSIVE；测量开始后**不得**软化阈值或重写基线；
> 代理**严禁**伪造作者工时、质量裁决或运行证据（见 memory `v12-machine-frontier-exhausted`）。

## 0. 角色

| 角色 | 承担 |
| --- | --- |
| 治理负责人（`platform-engineering`） | 窗口开启授权、GO/NO_GO 裁决、阈值/样本冻结把关 |
| Rule Author（≥1 真实作者） | 手写 vs 模板配对，记录 author-active-time |
| 质量裁决人 | precision/recall/false-positive-regression 的人工配对复核 |
| 观察窗口责任人 | 每次运行的 journal 采集、窗口报告聚合 |

## 1. 窗口前跑道（2026-09-24 → 09-30）

- [ ] 交付门禁全绿并留证：`bash scripts/check.sh`（证据入 `.scratch/`）。
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

## 2. 窗口内采集（2026-10-01 → 10-31，模式恒 LIVE_SHADOW）

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

## 4. 裁决与毕业（窗口结束后）

- [ ] 人工裁决：`./rf pilot evidence adjudicate PACKAGE OUTPUT --contract-digest HEX --sample-digest HEX`
      → 产出 `PilotAdjudication`：全 PASS 且包完整 = `GO`；已知失败 = `NO_GO`；缺失/冲突/无效 = `INCONCLUSIVE`。
- [ ] 写一条 ADR（`docs/adr/`），记录三条 MVP 假设（作者成本 / 验证语义质量 / CI 债务边界）的 Go/No-Go 结论。
- [ ] 对「CI 是否升级到 ADVISORY」给出显式决定；**绝不**自动跳 `BLOCKING`；GO + 治理签字前
      SHADOW 边界不变（禁 SCM 写、禁源码上传、禁 protected-branch）。

## 5. 停止条件

- 若窗口内真实作者/质量裁决人无法到位 → 报告保持 INCONCLUSIVE，**停止**，回到 product-scope-audit
  的「narrow」分支（收缩为可离线复跑的 Completeness/三态 Gate 演示资产，对外只宣称工程闭环）。
- 不得用预演/合成数据顶替真值以凑 GO。

## 相关文件

- 阈值权威：`specs/pilot/pilot-success-contract.yaml`
- 窗口/样本：`specs/pilot/pilot-observation-plan.yaml`、`specs/pilot/live-java-repositories.yaml`
- 指标字段/证据包 schema：`specs/pilot/pilot-metrics-baseline.yaml`、`specs/pilot/pilot-evidence-package.yaml`
- 口径文档：`docs/pilot/author-cost-baseline.md`、`release-candidate.md`、`observation-implementation.md`
