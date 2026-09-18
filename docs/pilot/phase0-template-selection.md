# Phase 0 候选场景与模板选择证据

## 决策

当前 Java 候选边界固定为 20 条 RuleIntent / 20 个已选 Scenario，来源是
`SEC-JAVA-017` 至 `SEC-JAVA-036` 的 CoverageLedger。100 条 Standard inventory
中其余 80 条仍是 inventory-only，不计入本次模板选择分母。

分类分布为：14 条 `EXISTING_RULE`、6 条 `NEW_RULE_REQUIRED`；
`EXISTING_RULE_NOT_ENABLED`、`EXISTING_RULE_UNSUITABLE`、`ENVIRONMENT_GAP`、
`BACKEND_CAPABILITY_GAP`、`STANDARD_AMBIGUOUS`、`NOT_MACHINE_DETECTABLE` 在这
20 条候选中均显式为 0。相邻但未晋级的 SEC-JAVA-017 场景另有 1 条
`STANDARD_AMBIGUOUS` 与 1 条 `NOT_MACHINE_DETECTABLE`，它们不进入候选分母。

选择三类 typed template：

- `ExistingRule`：13 条路径保留既有 PMD rule identity 与参数；
- `StructuralRule`：6 条 `structural-node` 路径使用有限 AST 结构边界；
- `SymbolRule`：1 条 `method-call` 路径使用静态方法调用与符号解析边界。

模板是由 RuleIntent 的 typed target kind 决定的，CoverageLedger 的分类则
独立记录覆盖现状；两者在证据中分别校验，避免把“已有覆盖”与“采用哪类
编译模板”混成一个事实。

## 边界

此证据完成 Phase 0 的分类与模板选择记录，但不把 inventory-only 标准
伪装成候选规则，不替人工澄清标准含义，也不宣称已获得 live pilot 的模板
批准。未来新增或替换模板，必须更新机器可读证据、覆盖差距与兼容影响。

机器可读主记录：`specs/pilot/phase0-template-selection.yaml`。
验收入口：`scripts/phase0-template-selection-check.sh`。
