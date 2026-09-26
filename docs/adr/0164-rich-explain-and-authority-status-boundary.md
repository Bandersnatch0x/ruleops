# ADR-0164: Rich Explain 与权威状态展示边界

- Status: Accepted
- Date: 2026-09-24
- Scope: Digest-only rich finding explanation layered on verified rule metadata
- Decision record: GitHub issue #110

## Context

Thin-client 已能对 `FindingView` 做 digest-only 的基础 explain，但只回显 finding
身份与定位，无法向作者解释「这条规则为什么触发、如何修复、来自哪个标准与版本、
当前严重级别、Baseline/Exception 影响，以及该结论是否具备权威」。这些解释性字段
来自 RuleBundle/Policy 元数据与权威扫描结果，若在客户端凭空拼装或使用过期、撤回、
身份不匹配的元数据，会把不可信内容伪装成权威解释。客户端边界仍必须保持 digest-only、
不得上传源码、不得生成 Gate 结论或改变信任状态。

## Decision

在 `runtime/thin-client` 内新增 `explain` 模块与 `rich-explain-v1` contract，`rf
explain` 扩展为 `--rule-meta`、`--result`、`--assessment`、`--format`、`--output`。

`RuleExplanationMetadata` 是封闭字段、拒绝隐私字段、且带 canonical 自摘要的
digest-only 投影：ruleId/ruleVersion/severity/status/rationale/remediation/
standardId/standardVersion/bindingDigest 均须绑定，自摘要不符即失败关闭。可选的
`AuthorityContext` 绑定一次权威扫描结果（gateDecision × authority、ruleId、
bindingDigest、findingFingerprint），可选的 `FindingAssessmentView` 携带
Baseline(NEW/HISTORICAL) 与 Exception(NOT_APPLICABLE/EXEMPTED)。

权威解释仅在「元数据存在且 status=active、metadata.ruleId==finding.ruleId，且在给出
权威结果时 bindingDigest 与 findingFingerprint 均一致」时生成。`TRUSTED_CI` 不能由结果
JSON 自声明：AuthorityContext 必须携带绑定上述 identity 的 DSSE proof，并由本地配置的
`AuthorityTrust` 与新鲜 `TrustMetadata` 验证；无法验证时拒绝。权威状态标签把决策与
权威两轴正交折叠为 `TRUSTED_CI`/`LOCAL_UNATTESTED`/`INCONCLUSIVE`（gate=Inconclusive
恒为 INCONCLUSIVE）。缺少元数据时只产出非权威 provisional 视图并给出下一步；若给出了
权威结果却缺元数据，则以 `METADATA_MISSING` 拒绝。元数据过期、撤回、摘要不匹配、proof
无效或规则身份不匹配统一 fail-closed，返回 `RichExplanationRefusal`（typed reason + nextStep）。

退出码：权威或 provisional 解释=0，拒绝=3，输入非法=2。该层永远不能生成 Gate
PASS/FAIL、批准 Exception、改变 TrustState、自动重试或写入 SCM，也不上传源码；解释内容
与 finding、结果、元数据三者的身份摘要绑定。

## Verification

```text
cargo test -q -p erf-thin-client --all-targets
bash scripts/rich-explain-check.sh .scratch/rich-explain-check
bash scripts/check.sh
```
