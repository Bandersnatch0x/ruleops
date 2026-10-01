# 项目指南

证据基线：`codex/agent-commit-hook-20261001` 的 `04019de`（2026-10-01）。本文档组只新增说明，不改变代码、契约、ADR、既有裁决或阈值。
当前承诺为可信规则执行层；本地结果保持 `LOCAL_UNATTESTED`/SHADOW，不是生产强制或受保护分支授权（`CONTEXT.md:3`）。

| 文档 | 适用范围与不适用 |
| --- | --- |
| [架构说明](architecture.md) | 适用于理解分层、数据/控制流、seam 与权威边界；不适用于据此认定规划中的生产部署已落地。 |
| [运行方式](run.md) | 适用于指定 WSL 开发环境、CLI、本地 Server、MCP/WebMCP 启动及错误判读；不是生产安装、认证/TLS 或 SCM 接入手册。 |
| [完整测试说明](testing.md) | 适用于总门禁全部阶段导航、定向检查与证据/复证纪律；不是本次全量门禁通过证明，也不把独立脚本冒充已接入总门禁。 |
| [使用手册](manual.md) | 适用于规则作者与使用者的 init→config→scan→结果判读和隐私边界；不授予发布签名、规则批准、BLOCKING 或受保护分支权力。 |

推荐入口：日常操作先看使用手册，环境问题看运行方式，实现/评审看架构，验收看测试说明。

## 权威与证据

- 仓库规则：`AGENTS.md`；领域统一语言：`CONTEXT.md`；裁决：`docs/adr/`。
- 架构计划：`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md`；白皮书：`docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md`。愿景与当前交付范围分开阅读，收窄边界见 `docs/adr/0167-product-audit-adjudication-batch.md`。
- 开发/验收纪律：`docs/agents/dev-workflow.md`；总门禁实际阶段：`scripts/check.sh`；命令面：`rf`（`./rf --help`）。
- MCP、WebMCP、本地 hook 专项说明：`clients/mcp/README.md`、`clients/mcp/webmcp/README.md`、`docs/agent-commit-hook.md`。
- 本次文档验收：`.scratch/project-docs.md`。报告记录行数、20 处引用抽查 rc 及未做/未核实项；本次按任务要求未运行全量门禁。

本文中的 `文件:行` 均相对于仓库根及上述基线；源码后续变化时应重新核对。`.scratch/` 报告是本地补充取证，不替代版本化权威，也不保证新克隆里存在；缺失时不可据此虚构历史或当前结论。
