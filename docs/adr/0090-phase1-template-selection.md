---
status: accepted
---

# Phase 1 template selection

Phase 0 的 20 个候选 Scenario / Coverage Gap 证据现已固化在
`specs/pilot/phase0-template-selection.yaml`，并由
`scripts/phase0-template-selection-check.sh` 逐项校验。证据支持三类模板：
`SymbolRule`、`ExistingRule`、`StructuralRule`。选择理由是覆盖一条新语义
编译路径、一个既有 PMD 身份绑定路径和一个受边界约束的 AST 结构路径，能够
验证“新增模板不等于新增任意 DSL”。

`ExistingRule` 必须保留 PMD rule identity 与参数，不能改写为另一个规则；
`StructuralRule` v0 仅接受有限 AST 节点集合和最多一个字面属性约束，超出边界
返回结构化编译错误。新增模板或替换选择必须重新记录覆盖差距与兼容影响，
并通过 Phase 0 选择证据检查器。
