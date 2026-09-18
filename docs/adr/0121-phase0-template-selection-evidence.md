# ADR-0121: 固化 Phase 0 候选场景与三类模板选择证据

- Status: Accepted
- Date: 2026-09-18
- Scope: Java MVP Phase 0 candidate and template boundary

## Context

Phase 1 的历史 ADR 记录了 `SymbolRule`、`ExistingRule` 与 `StructuralRule`
三类默认选择，但当时明确注明 20 个候选 Scenario / Coverage Gap 证据尚未
入库。没有这层证据，模板数量与候选分布之间的关系不可复核，也容易把
100 条 Standard inventory 误报成 100 条可执行候选。

## Decision

将 `specs/pilot/phase0-template-selection.yaml` 作为当前 Phase 0 决策证据。
它固定 20 条候选 RuleIntent / Scenario，逐项绑定 Standard、CoverageLedger
分类、规则身份和 typed target 对应的模板，并显式记录所有未观察到的 gap
类别为 0。当前选择最多三类模板：13 条 `ExistingRule`、6 条 `StructuralRule`
和 1 条 `SymbolRule`。

`CoverageLedger` 分类与模板类型是两个独立维度：前者描述覆盖现状，后者
由 RuleIntent 的 typed target kind 决定。`runtime-halt` 与
`reflective-invocation` 作为 SEC-JAVA-017 的相邻 gap 保留在证据中，但因
分别需要澄清和超出当前 source backend 能力，不晋级为候选。

## Consequences

- Phase 0 的候选分母、分类分布和模板选择可由脚本复核；缺项、重复项、
  悬空绑定或模板漂移会 fail closed。
- 100 条 Standard inventory 与 20 条候选规则保持明确分层。
- 新增模板不能只修改编译器；必须同步更新候选证据、覆盖差距和兼容影响。
- 该证据不等同于 live pilot 的人工批准，也不替代对 ambiguous standard 的
  澄清。

## Verification

运行 `scripts/phase0-template-selection-check.sh .scratch/phase0-template-selection`
并检查生成的证据；交付仍以 `bash scripts/check.sh` 全绿为准。
