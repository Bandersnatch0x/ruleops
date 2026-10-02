# ADR-0177: 短窗未激活处置、最终裁决入口与权限边界

- Status: Accepted
- Date: 2026-10-03
- Approval source: 本次任务明确项目负责人已全部采纳 `.scratch/adjudication-recommendations-20261002.md`；本 ADR 落地其中 D1、D9、D10，不补造审批时分秒、签名或外部审批件。
- Scope: 治理决定与使用边界；不修改 ADR-0176、既有合同、阈值、代码或历史证据。

## D1：本周不计正式短窗，另准备未来窗口申请

### 背景

[ADR-0176](0176-approved-pilot-window-reanchor.md) 的 Activation 要求治理批准、实现验收、人员/基线就绪全部早于 `2026-10-02T00:00:00Z`，等于起点也不满足；批准窗口方案不等于完成激活。
[实现报告](../../.scratch/window-reanchor-implementation.md) §5 留有起点前验收运行记录，但它不能代替人员/基线就绪证明。
[10-02 观测记录](../../.scratch/pilot-live-window-execution-20261002.md) 的“仍须人工完成”仍列作者时间、质量裁决及批准基线缺口；其使用旧整月口径，不是七天完整包。
[已采纳建议](../../.scratch/adjudication-recommendations-20261002.md) §1 区分了事前批准来源、实现验收与连续未闭合的人员/基线条件，未证实三项全部事前成立。

### 决策

本周所指 `2026-10-02T00:00:00Z` 至 `2026-10-08T23:59:59Z` 的目标窗口，因未证实全部事前激活条件，**本周不计正式短窗，按未激活处置**；本周采集不得列为正式短窗成果。另准备起止明确、留足事前准备时间的未来完整七天申请，见[未来窗口申请准备](../pilot/future-window-application-preparation.md)。本 ADR 不批准未来日期、版本或激活。

### 后果与影响

- 延后正式 Pilot 结论，不降低阈值、不缩短或自动顺延窗口，不追溯补造批准、就绪记录或生效时间。历史资料保留其生成合同/版本和真实缺项；不得重写日期、版本、digest 或重 seal 冒充新窗。（出处：ADR-0176 Activation、Evidence retention and historical scope。）
- 旧活动按原合同及原版本工具复核；ADR-0176 指向 Git `fb53e19` 及历史保全区。本次不切换旧工具、不修改旧字节；未来版本/日期须治理批准后另行一致实施并验收。
- 未激活不是“测得指标 FAIL”。不得宣布 GO；按[证据包](../../specs/pilot/pilot-evidence-package.yaml) `aggregation/adjudication`，有已知必需指标 FAIL 则 NO_GO，无已知 FAIL 而缺必需证据则 INCONCLUSIVE，不凭程序性缺口编造性能失败。

## D9：observer 仅作中间观测，完整证据包是唯一最终裁决入口

### 背景

[observer 源码](../../runtime/pilot-observer/src/main.rs) `439–458` 行从 ledger 中取非 NotApplicable 的 regression feedback，未在该处按模式/窗口过滤，并将合并检查写作 precision/recall/false-positive 与 Product outcome。
[指标规范](../../specs/pilot/pilot-metrics-baseline.yaml) `boundaries.unsupportedOutcomeClaims` 明确不支持 author-active-time、precision、recall、live-pilot-outcome 声明；[证据包规范](../../specs/pilot/pilot-evidence-package.yaml) `requiredMetrics` 要求 11 项独立完整性。报告字段名或一次 PASS 均不改变此差别。

### 决策

observer/window report 仅为**中间观测**，包括其中 Product outcome/Overall 字段；不能直接作为最终产品裁决。最终结论只走现有 **11 指标完整证据包与治理复核**，按[成功合同](../../specs/pilot/pilot-success-contract.yaml) `adjudication` 由 `platform-engineering` 治理负责人复核 digest-only 证据、合同指标及总门禁结果。
每个合同版本、模式、窗口采用隔离 journal；人工质量三项分别提供有真值、配对和分母的证据，普通 regression feedback PASS 不升级为三项独立 PASS，正常扫描零 false-pass 不替代独立失败注入证据。（出处：[执行清单](../pilot/live-shadow-window-execution-checklist.md) A2、A4、§6.2；[已采纳建议](../../.scratch/adjudication-recommendations-20261002.md) §5.2。）

### 后果与影响

- 保持证据包唯一门槛及 `FAIL > INCONCLUSIVE > PASS`；只有完整包且全部必需指标 PASS 才具备 GO 条件，命令执行仍不等于治理批准。
- 隔离 journal 是现行受限使用条件，不证明实现已正确处理任意混合历史，也不声称现有独立 journal 已被污染。本次仅静态核对，未运行污染复现。
- 若确需共享 journal 或将 observer 作为直接产品入口，须另行批准修正范围过滤/指标语义并验证；本次不改代码、不以文档将该能力宣布完成。（出处：已采纳建议 §5.2。）

## D10：保持排除 SCM/生产权威，不为比赛或全绿启用凭证或发布

### 背景

[观察计划](../../specs/pilot/pilot-observation-plan.yaml) `evidence` 禁止源码上传、仓库写入、SCM 状态发布及 protected-branch authority。
[E2E 目录](../e2e/catalog.md) 将 MAN-003 真实 GitHub 状态发布、MAN-004 生产 TRUSTED Gate 单列人工项；[历史 E2E 报告](../../.scratch/e2e/full-20261002/report.md) 仍记为 MANUAL，不能据工程路径通过宣称它们已验收。
[ADR-0167](0167-product-audit-adjudication-batch.md) 维持当前 checkout 的 release/gate/promote 与 SCM 回写禁用边界。

### 决策

维持 SCM/生产权威有意排除、相关人工项未执行的边界；不为比赛、演示或“全 E2E 全绿”申请、读取、启用凭证，不发布 SCM 状态，不升级 TRUSTED/BLOCKING 或受保护分支权威。
只有独立业务需要与治理授权后，才另行进入对应执行范围；本次及未来 SHADOW 窗口的申请准备不包含该授权。（出处：[已采纳建议](../../.scratch/adjudication-recommendations-20261002.md) §5.3；观察计划 `evidence`；[证据包边界](../../specs/pilot/pilot-evidence-package.yaml) `boundaries`。）

### 后果与影响

人工 E2E/生产接入继续如实列为排除或未执行，不是为凑全绿应自动补齐的普通漏测。可展示受限工程路径，不能声称全部人工 E2E 或生产接入已验收；即使未来 GO 也不授予 SCM 或 protected-branch 权限。申报边界见[申报审校框架](../research/competition-2026-10/03-submission-review-framework.md)，不以比赛资格要求反向扩大产品承诺。
