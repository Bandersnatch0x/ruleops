# ruleops

Enterprise Rule Factory(ERF)——企业级 RuleOps / 代码规范工程与治理平台。把企业代码规范转化为可验证、可发布、可执行、可审计、可演进的机器检查资产。首期聚焦 Java,主 Backend 为 PMD(SpotBugs 补字节码语义类规则),架构保持 Backend-neutral。

- Spec:`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md`
- 白皮书:`docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md`

## Agent skills

### Issue tracker

Issues 在 GitHub Issues 跟踪，用 `gh` CLI 操作。See `docs/agents/issue-tracker.md`.

### Triage labels

使用默认 triage 标签：`needs-triage`、`needs-info`、`ready-for-agent`、`ready-for-human`、`wontfix`。See `docs/agents/triage-labels.md`.

### Domain docs

单上下文：根 `CONTEXT.md` + `docs/adr/`。See `docs/agents/domain.md`.
