---
title: 1 周 SHORT_WINDOW pilot 方案与开窗清单
date: 2026-09-30
repository: ruleops
status: historical-proposal
authority: SHADOW
superseded_by:
  - docs/adr/0176-approved-pilot-window-reanchor.md
  - docs/pilot/live-shadow-window-execution-checklist.md
---

# 1 周 SHORT_WINDOW pilot 方案

> **历史提案，非当前开窗授权（2026-10-02 注）**：本方案已由
> [ADR-0176](../../adr/0176-approved-pilot-window-reanchor.md) 与
> [现行执行清单](../../pilot/live-shadow-window-execution-checklist.md) 取代。
> 下文背景与治理动作保留 2026-09-30 原文；整月窗口、“起止待定”及“实现与回归另计”均为历史语境，不是当前实施状态。
> 当前配置目标为合同 `1.1.0`、`2026-10-02T00:00:00Z` 至 `2026-10-08T23:59:59Z`（UTC、inclusive）。
> **日期落窗不等于治理已激活**：治理批准、实现验收、人员/基线就绪必须全部早于起点，等于起点也不满足。
> 当前材料未证实全部事前条件已满足，本文件不宣称短窗已激活；治理裁决留待负责人核对真实记录，不补造激活时刻或审批证据。
> 任一项未及时完成则新版不得激活，不自动顺延、缩短或追溯生效；旧活动仍用旧合同及原版本工具，另申请明确的未来窗口。
> 第 3 节仅修正独立取证命令的输出隔离与实参，不替代上述激活条件；第 2、4 节的预期指标亦非已取得成果。

## 背景（历史原文）

正式观察窗口为 `2026-10-01 → 2026-10-31`（`specs/pilot/pilot-success-contract.yaml`
observationWindow，GitHub issue #79 批准）。实际可留出的验证窗口约 1 周，因此按治理流程
**修订窗口长度**，不用预演/合成数据顶替，也不事后软化阈值。

## 1. 治理动作（历史原文；先做，不得事后调阈值）

- 提 issue 申请：把 observationWindow 由 31 天缩为 SHORT_WINDOW（建议 7 天，起止待定），治理负责人批准。
- **只改窗口长度**；样本（3 仓库、pinned commit、不替换）与全部阈值保持不变。
- 修订记录写入 `specs/pilot/pilot-success-contract.yaml` 与 `specs/pilot/pilot-observation-plan.yaml`，
  并补一条 ADR 说明缩短原因与**结论外推力限制**。七天包封存前须经治理批准，
  重锚 `runtime/pilot-evidence/src/lib.rs` 的批准 epoch 常量，并使合同、证据包规格与运行时窗口一致；
  实现与回归另计，见计划票 #136，不能仅修订文档后按清单直接封存。
- 纪律：SHORT_WINDOW 的趋势类结论只能报「有界短窗口趋势」，不得外推为月度结论。

## 2. 7 天能拿 / 不能拿

**能拿**：complete-rate、inconclusive-rate、scan-latency p95、peak memory、false-PASS 注入=0、
findings 与债务的 7 天 delta。
**受限**：new/historical-debt trend（短窗口，只出方向）；precision/recall（需人工配对复核，做有界样本）；
promotion/rollback（治理事件多半不发生，如实记 0 或不适用）。
**独立于窗口、优先做**：author-active-time 配对测量（`corp.java.no-system-exit` /
`no-clone-method` / `avoid-duplicate-literals` 三条固定 RuleIntent，1-2 天可完成）——这是「项目价值·提效」
最有说服力的数字。

## 3. 开窗清单（复用 `live-shadow-window-execution-checklist.md`，仅列差异/精简）

独立取证每次先创建唯一 `RUN` 输出根；所有后续命令在同一 shell 中引用它。
以下仅示意路径，不会因运行命令而获得正式窗口授权；总门禁仍使用专属 `.scratch/gate/`。

```bash
RUN="$(mktemp -d .scratch/short-window-XXXXXXXX)"
RECORD_FILE="$RUN/author-cost-record.yaml"
cp specs/pilot/author-cost-record-template.yaml "$RECORD_FILE"
# 由真实作者在 RECORD_FILE 中填写 run 元数据和配对工时；空模板不代表已测。
```

- 窗口前：门禁全绿留证（`bash scripts/check.sh`）；`./rf pilot delivery-check "$RUN/delivery-check"`；
  3 仓库冻结 pinned commit 并跑只读演练确认无漂移
  （`scripts/pilot-live-observation.sh "$RUN/live-shadow-preflight"`）；
  在上述 `$RECORD_FILE` 中填写真实 run 元数据；
  latency/resource 候选基线送治理负责人批准；precision/recall 标注样本集与配对规程定义，排定 counterbalance。
- 窗口内（7 天，模式恒 LIVE_SHADOW）：每日 catch-up 跑 `./rf pilot observe ...`（不写 SCM、不上传源码）；
  作者成本配对（显式传入 `OUTPUT_DIR` 与 `RECORD_FILE`）
  `scripts/pilot-author-cost-baseline-check.sh "$RUN/author-cost" "$RECORD_FILE"`；质量配对复核 `./rf pilot feedback ...`；
  false-PASS 注入确认计数=0；治理快照 `./rf pilot governance ...`。
- 窗口尾：单模式聚合 `./rf pilot window-report ...`（勿混模式）；封存证据包
  `./rf pilot evidence package ...`（requiredMetrics 须全 `COMPLETE`，窗口/样本 digest 匹配）；
  `./rf pilot evidence adjudicate ...` 出 `GO / NO_GO / INCONCLUSIVE`（聚合优先级 FAIL > INCONCLUSIVE > PASS）。
- 停止条件：真实作者/质量裁决人无法到位 → 已知 `FAIL` 时 `NO_GO` 优先；仅无已知 `FAIL` 且缺必需证据时报告为 `INCONCLUSIVE`，收缩为「工程闭环演示」，绝不凑 GO。

## 4. 比赛叙事建议（两层报，别混）

- **能力演示层（已实测）**：preflight 3/3 complete、100 标准 / 20 场景、可复现构建 SHA-256。
- **有界效能验证层（7 天）**：complete-rate / latency / debt delta / author-active-time。
  在申报材料里明确标注 SHORT_WINDOW 及外推限制。
