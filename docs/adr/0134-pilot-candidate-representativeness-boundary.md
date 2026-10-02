# ADR-0134: 固化 Pilot 候选包的 Factory 代表性边界

- Status: Accepted
- Date: 2026-09-19
- Scope: Java MVP candidate composition and Factory cost claim
- Decision record: GitHub issue #87

## Context

Plan §31.2 固定 Java MVP 使用 20 条候选规则和最多三类 typed template；已批准
的 Pilot Success Contract 也把候选分母冻结为 20。当前 Phase 0 证据中的 20 条
候选包含 14 条 `EXISTING_RULE` 与 6 条 `NEW_RULE_REQUIRED`；按 RuleIntent
target kind 分为 13 条 `existing-rule`、6 条 `structural-node` 和 1 条
`method-call`。其中 80 条 Standard 仍是 inventory-only。

作者成本预注册样本是每类模板各 1 条，用于验证三条 Factory 路径是否可测，不能
单独支持对一般规则总体的统计性结论。

## Decision

1. 冻结当前 20 条作为本次 Shadow/Verification Pilot 的预注册分母，不在测量前
   为了追求比例而改写已批准合同。当前 20 条可以进入测量，但证据中的代表性
   声明必须限定为三类模板路径的受限边界；`method-call n=1`、80 条
   inventory-only 和复杂度分布限制必须显式记录。广义 Factory 成本假设在没有
   后续分层证据时保持 `INCONCLUSIVE`。
2. 若未来要声称 Factory 成本结论可外推到一般规则，至少要有每个 target kind
   `existing-rule`、`structural-node`、`method-call` 各 3 条已测 RuleIntent，且
   每个实际纳入的分层同时覆盖低复杂度和中复杂度。当前 `method-call` 只满足
   路径存在性，不满足该泛化门槛。
3. 候选包的 `NEW_RULE_REQUIRED` 下限固定为同时满足：至少 6 条，且占候选总数
   至少 25%。每条必须保留 CoverageLedger 的 `NEW_RULE_REQUIRED` 分类、
   `rule: null` 和独立 Verification 证据。当前 6/20 = 30%，满足该边界。

上述边界只限定 Pilot 结论的可外推范围，不改变 `PilotSuccessContract v1.0`
的 20 条分母、Shadow 权限或 protected-branch 安全边界。未来扩充候选包时，
必须重新生成代表性证据并在新的治理记录中说明对照关系。

## Consequences

- 当前 Pilot 可以继续登记和测量，不因候选构成不平衡而事后修改预注册分母。
- `PilotEvidencePackage` 必须携带候选分类、target kind、复杂度分层、样本数量
  与代表性限制；不能把 20 条计数写成一般 Factory 成功证明。
- 后续若要把作者成本结论升级为一般 Factory 结论，至少需要补足 method-call
  样本和各分层的低/中复杂度证据。
- Verification/Shadow 的当前资格与广义 Factory 的外推资格分离，避免把已有
  PMD rule 包装路径掩盖为新规则生产能力。

## Verification

```text
scripts/phase0-template-selection-check.sh .scratch/phase0-template-selection
scripts/phase4-coverage-ledger.sh .scratch/phase4-coverage-ledger
bash scripts/check.sh
```
