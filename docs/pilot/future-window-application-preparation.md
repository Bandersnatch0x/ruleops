---
title: 未来完整七天窗口申请准备
date: 2026-10-03
status: preparation-only
authority: SHADOW
---

# 未来窗口申请准备

本文件落实已采纳 D1，**仅准备申请，不批准日期、版本或激活，不声称已验证收益**。[ADR-0177 D1](../adr/0177-pilot-window-and-adjudication-boundaries.md) 已将本周目标按未激活处置，不计正式短窗；约束来源是 [ADR-0176 Activation](../adr/0176-approved-pilot-window-reanchor.md)。当前三份 YAML 的 `1.1.0 / 2026-10-02..10-08` 仍是既有配置，不是未来申请，也不是激活证明。本次不修改它们。

## 1. 申请应明确的窗口与准备余量

| 待填项 | 要求与责任 | 仓库依据 |
| --- | --- | --- |
| `window_start_utc` / `window_end_utc` | 治理负责人确认未来明确起止，UTC、inclusive，覆盖完整七天（604800 秒）；目前均为 `HUMAN_INPUT_REQUIRED`，不默认沿用本周或自动加七天。 | ADR-0176 Decision、Activation；[观察计划](../../specs/pilot/pilot-observation-plan.yaml) `spec.window`。 |
| 准备时间与检查点 | 申请人（由负责人指定）汇总审批、实现验收、人员排期和基线准备所需时间，预留在起点前完成全部三项并核对原件的余量；治理负责人确认是否足够。不得为了比赛期限先开窗后补材料；本文件不编造固定提前天数。 | ADR-0176 Activation；[执行清单](live-shadow-window-execution-checklist.md) A5。 |
| 合同及样本绑定 | 申请列合同 id/拟议版本、批准来源、样本与冻结 RuleBundle 的身份；获批后由实现负责人统一实施合同、观察计划、证据包及实际工具绑定并验收，再登记准确 digest。准备稿不得冒充已批准合同。 | [成功合同](../../specs/pilot/pilot-success-contract.yaml) `sample/adjudication`；[证据包](../../specs/pilot/pilot-evidence-package.yaml) `contract/sample/observationWindow`。 |
| 未就绪的处置 | 任一条件未能在起点前完成，不得激活、不追溯补造、不自动顺延或缩短；重新申请明确的未来窗口。到场时间、空表、APPROVED/REGISTERED 状态或一次运行成功均不代替就绪证据。 | ADR-0176 Activation；执行清单 A5。 |

## 2. 事前激活三条件：全部严格早于起点

三项须分别保留真实完成时刻（含时区，可换算 UTC）、负责人/签认人、原件引用及摘要、适用合同版本；**完成时刻等于起点也不满足**。若只知道审批日期，保留原始粒度并明确缺口，不能用记录创建时间推算审批发生时间。（出处：ADR-0176 Approval time、Activation；执行清单 A5。）

| 条件 | 需准备的材料 | 责任人角色及确认边界 |
| --- | --- | --- |
| 治理批准 | 明确起止、合同版本、SHADOW 范围、冻结样本、批准引用与真实批准来源；不以申请稿或票据创建代批件。 | `platform-engineering` 治理负责人批准，申请人整理；来源：[成功合同](../../specs/pilot/pilot-success-contract.yaml) `adjudication`、ADR-0176。 |
| 实现验收 | 获批版本/窗口的一致实现、定向检查、完整门禁原始日志与验收签认，保存适用提交及执行时刻；旧窗口验收报告不可替代未来变更验收。 | 实现负责人产出、验收负责人签认；脚本全绿不能代签。来源：ADR-0176 Consequences and acceptance、执行清单 A5。 |
| 人员与基线就绪 | 作者、独立质量裁决人、观察员的真实安排与确认；作者成本基线，以及经批准的延迟/资源基线，连同适用环境、样本、规则及原件。 | 各执行角色确认可执行，治理负责人批准基线并复核全项就绪；姓名、时间不由代理代填。来源：执行清单 A1–A3、A5；[作者成本基线](../../specs/pilot/author-cost-baseline.yaml)。 |

以上是责任**角色**而非已指定人员；具体承接人、复核时间及签认仍须负责人确认。本文件不预勾任何完成状态。

## 3. 材料包与后续采集责任

复用执行清单 A1–A5/§6.1 的记录约定，不新增 schema；为本次获批窗口选择独立、唯一的 `.scratch/` 取证目录，不使用会被全量门禁覆盖的 `.scratch/gate/`。（出处：[开发工作流](../agents/dev-workflow.md) 全量测试纪律、[ADR-0175](../adr/0175-isolate-gate-output-from-manual-evidence.md)。）

| 材料及用途 | 准备/产出角色 | 原有格式与出处 |
| --- | --- | --- |
| `activation-prerequisites.md`：三条件原件索引，逐项完成时刻、版本与签认 | 申请人整理，治理/实现验收/各执行角色分别确认 | 执行清单 A5、§6.1；不得将本清单视作激活件。 |
| `author-cost-record.yaml` 与作者基线：冻结三条 RuleIntent，manual/template 配对主动分钟、机器等待另计、run/author/UTC/order 元数据 | 作者真实记录，观察员保管，治理负责人复核 | [空记录模板](../../specs/pilot/author-cost-record-template.yaml)、[作者成本口径](../../specs/pilot/author-cost-baseline.yaml)、执行清单 A1；不代填真实工时。 |
| `quality-review.md`：同仓同提交 baseline/Shadow 配对、独立真值/holdout/near-miss 范围、TP/FP/FN 与分母、分歧处理 | 独立质量裁决人标注及签认，观察员关联指标原件 | 成功合同 `measures.quality`、执行清单 A2；分别支持 precision、recall、false-positive-regression，不以 fixture/反馈 PASS 替代。 |
| `operations-baseline-approval.md`：p95 latency 与峰值内存原始测量、单位、环境、适用范围及事前批准 | 观察员记录候选测量，治理负责人批准 | 成功合同 `measures.operations`、[指标规范](../../specs/pilot/pilot-metrics-baseline.yaml)、执行清单 A3；测得候选值不等于已批准。 |
| `failure-injection-review.md`：独立注入方案、不完整/超时覆盖及预期、实际非 PASS 和原始输出 | 观察员执行/取证，裁决人确认覆盖与接纳 | 成功合同 `measures.security`、执行清单 A4；正常扫描零 false-pass 不证明注入覆盖。 |
| 样本、冻结规则与独立 journal：按默认分支变化/每日 catch-up 采集并记录实际提交与去重 | 观察员准备，治理负责人确认登记范围 | [三仓清单](../../specs/pilot/live-java-repositories.yaml)、观察计划 `sample/cadence/modes`；每合同版本/模式/窗口隔离，见 ADR-0177 D9。 |
| 原件及摘要索引：运行身份、输入/输出 digest、出处、缺项和访问边界 | 原件保管人（由负责人明确）保管；对外披露另获授权 | 证据包 `evidenceRecord/boundaries`；[竞赛证据索引](competition-evidence-index.md) §5。 |

窗口前先准备方法、记录模板、人员、基线与保管安排；窗口内应产生的真实测量不得事前填成结果。未采集写缺项，不填零、估计值或 PASS。（出处：执行清单 A1–A5；作者成本基线 `missingData`；证据包 `aggregation`。）

## 4. 起点前复核与窗口后出口

- 起点前：由治理负责人核对三项完成证据严格早于起点，及合同/样本/窗口与工具的一致性；获批方案不等于自动激活。未来变更的完整门禁与验收属于后续实施，本次文档任务未执行。（出处：ADR-0176 Activation、Consequences and acceptance。）
- 窗口内：只有符合登记范围与获批条件的 LIVE_SHADOW 证据进入正式包，preflight、LOCAL_SIMULATION、SYNTHETIC_SHADOW 分离；重复同一 pin 不当新增独立样本。（出处：观察计划 `sample/cadence/modes`。）
- 完整窗口结束后：核对证据包 **11 个 requiredMetrics**：author-active-time、precision、recall、false-positive-regression、complete-rate、inconclusive-rate、scan-latency、resource-consumption、new-violation-trend、historical-debt-trend、false-pass-count。包与 record 窗口一致、digest/完整性合规后，由治理负责人复核；observer 报告只是中间观测。（出处：证据包 `requiredMetrics/observationWindow`；执行清单 §6.2；ADR-0177 D9。）
- 裁决保持 `FAIL > INCONCLUSIVE > PASS`，不为补缺项降低阈值；未来 GO 也不授予 SCM/生产权威。旧证据保留原日期、版本、digest，不重 seal，不回填成未来或过去窗口成果。（出处：ADR-0176 Evidence retention and historical scope；ADR-0177 D1、D9、D10。）
