<!-- scaffold-stamp: agent-scaffold/templates/agents/AGENTS.md@017b06f -->

# 代理工作规则

## 开工前指针

- 仓库级权威：本文件、`CONTEXT.md`、`docs/adr/`、`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md` 与 `docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md`。
- 票据路由：GitHub Issues，使用 `gh` CLI；一切开票、读票、记录、关票与分诊按 `docs/agents/issue-tracker.md` 办。
- 决策落点：`docs/adr/`；术语表：`CONTEXT.md`。
- 进入开发任务先读 `docs/agents/dev-workflow.md`；正文使用阶段名与机制名，实际命令只见该文档的绑定表。

## 项目上下文

Enterprise Rule Factory(ERF) 把企业代码规范转化为可验证、可发布、可执行、可审计、可演进的机器检查资产。首期聚焦 Java，主 Backend 为 PMD，SpotBugs 补充字节码语义类规则，架构保持 Backend-neutral。

现有领域文档：`CONTEXT.md`、`docs/adr/`、`docs/agents/domain.md` 与 `docs/agents/triage-labels.md`。

## 工作规则

- 裁决批次：待裁项编号并标注推荐项；结果记入决策落点。
- 拷问阶段及一切决策问题由用户作答；事实性疑问由代理查证后呈上。
- 外部思路进入产品内容时使用自研措辞；交付前检查外部名边界。
- 工作语言为中文；机器面(frontmatter、字段名、代码标识)用英文。
- 产物两分：临时产物落 `.scratch/`，持久产物落 `docs/`；仓库根级不新增其他杂项目录。

## 门禁

- 交付门禁命令（全绿方可交付）：`bash scripts/check.sh`。

## 验收

- 每次交付必须验收，范围按交付粒度确定；流水线、全量测试纪律与验收规程见 `docs/agents/dev-workflow.md`。
