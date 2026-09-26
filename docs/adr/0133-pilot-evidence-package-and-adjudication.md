# ADR-0133: 固化 Pilot Evidence Package 与人工裁决边界

- Status: Accepted
- Date: 2026-09-18
- Scope: Phase 4 Shadow-only pilot evidence and Go/No-Go adjudication
- Decision record: GitHub issue #90

## Context

`specs/pilot/pilot-success-contract.yaml` 已经由治理流程批准为
`pilot-success-contract-v0`。它冻结了 100 条 Standard、20 条候选
RuleIntent、3 个 Java Shadow 仓库、观察窗口和全部验收阈值。它是预注册的
比较分母，不是 live pilot 结果，也不应被测量后的结果反向调整。

当前 author-active-time、Pilot Metrics 和观察窗口报告分别有自己的记录边界。
它们还需要一个不改变已批准合同、且不把本地预演或缺失输入伪装成产品结论的
统一证据边界。

## Decision

### 1. 独立证据包

引入 `PilotEvidencePackage` 作为独立的、digest-only 的证据容器。它必须引用：

- 不可变 `PilotSuccessContract` 的身份和 digest；
- 预登记观察窗口、固定仓库/RuleIntent 样本身份及其范围；
- author-active-time、quality、operations、outcomes 和 security 各指标的证据记录；
- 每条证据记录的来源身份、时间范围、完整性状态和 artifact digest；
- 按指标的 `PASS`、`FAIL` 或 `INCONCLUSIVE` 状态及聚合结果。

证据包不得上传 source、完整 Finding 或 SCM 状态，不得修改合同阈值，不得授予
Advisory 或 protected-branch 权限。

### 2. 指标状态与人工裁决

指标状态固定为 `PASS`、`FAIL`、`INCONCLUSIVE`，聚合优先级为
`FAIL > INCONCLUSIVE > PASS`。已知失败不能被缺失数据遮蔽；缺失、非法、冲突、
篡改或不完整的证据不能产生 `PASS`。

最终结果使用独立于 `GateDecision` 的 `PilotAdjudication`：

- `GO`：证据包完整，且全部必需指标为 `PASS`；
- `NO_GO`：至少一个必需指标为 `FAIL`；
- `INCONCLUSIVE`：没有已知失败，但仍有必需证据缺失、非法、冲突或不完整。

最终裁决负责人固定为 `platform-engineering` 治理负责人。作者、运行人员和
人工标注者可以提供带身份的证据与注释，但不能替代该负责人作出最终裁决。

记录模板中尚未开始测量的占位值继续使用 `HUMAN_INPUT_REQUIRED`；进入观察窗口
后，缺失或无效输入必须投影为 `INCONCLUSIVE`。两者都不能被当作成功结果。

### 3. 结果毕业门槛

只有完整证据包经人工裁决为 `GO`，才可以创建下一阶段面向产品能力的规格票和
实现票。`NO_GO` 只产生整改/止损票；`INCONCLUSIVE` 只产生补测或升级票。
任何一种结果都不自动开启 protected-branch blocking，也不自动授予 SCM 凭证。

这条门槛不阻止为完成本次 Shadow 观测而维护 digest-only 记录和验证工具；它阻止
的是在 Pilot 证据闭环前把结果升级为产品执行权力。

## Consequences

- 已批准的 PilotSuccessContract 保持不可变，阈值不会被结果驱动地重调。
- author cost、quality、operations 和 outcome 证据可以在一个可复核包中关联，
  但仍保留各自的来源和人工边界。
- 报告可以明确区分合同已批准、证据尚未完成和最终治理裁决，避免把工程预演
  当成 live pilot outcome。
- 真实 Shadow window、质量人工标注和运行基线仍是 `PilotEvidencePackage` 的
  必填输入；当前仓库不能宣称 `GO`。

## Verification

现有合同、作者成本和 Pilot Metrics 校验器继续验证各自的局部边界；后续证据包
校验与窗口报告任务必须验证合同 digest、固定样本、指标完整性、状态聚合和
`INCONCLUSIVE` fail-closed 语义。

```text
bash scripts/pilot-success-contract-check.sh .scratch/pilot-success-contract
bash scripts/pilot-author-cost-baseline-check.sh .scratch/pilot-author-cost-baseline-check specs/pilot/author-cost-record-template.yaml
bash scripts/pilot-metrics-baseline-check.sh .scratch/pilot-metrics-baseline-check
bash scripts/check.sh
```
