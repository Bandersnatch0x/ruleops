# ERF WebMCP 本地查看面

零 npm 依赖；复用 `../erf-mcp-server.js` 的清单、参数校验、固定 `rf` 可执行文件、`execFile` argv 数组和 digest-only 输出校验，不实现另一套执行逻辑。

## 启动与查看

需要 Node.js 22+ 和已有 ERF CLI 运行环境（开发检出需要 Cargo，scan 还依赖对应 Backend）。在 WSL `kali-linux2` 内：

```bash
cd /home/kali/文档/ruleops; export PATH="$HOME/.local/node/bin:$HOME/.cargo/bin:$PATH"; node clients/mcp/webmcp/server.js
```

默认 URL：`http://127.0.0.1:3210`。Windows 侧可用 `http://localhost:3210` 打开查看（依赖 WSL2 localhost 端口转发）；也可用 `PORT=3211 node clients/mcp/webmcp/server.js` 选择端口，`PORT=0` 自动分配并输出 URL。仅绑定 `127.0.0.1`，没有对外监听选项。

选择工具、填写参数、点击「调用并查看」。`repo_root` 省略时相对于服务启动工作目录；scan 必须恰选 `rule_id` 或 `candidate`；explain 必须提供 `finding`。doctor 展示 checks 表格；scan 展示原始 gateDecision、authority、findingRecords、完整性与诊断；所有调用均提供折叠原始 JSON。错误仍显示，不将 CONFIG_MISSING 当成功。

## WebMCP 与后端 MCP

WebMCP 是实验性浏览器 API，需 Chrome 146+ **且实际启用相应实验能力**；版本号本身不保证可用。页面检测 `document.modelContext?.registerTool`，逐项注册 `{name, description, inputSchema, execute, annotations: {readOnlyHint: true}}`；全部成功才显示「WebMCP: 已启用（Chrome 146+）」。不支持的浏览器显示「WebMCP: 未启用（此浏览器不支持，仍可手动查看）」，仍可通过 HTTP 手动查看。

已注册工具优先使用 `getTools()` 发现当前同源窗口工具，再使用 `executeTool(tool, args)`；早期实现仅支持注册时，手动调用同一个已注册 execute 回调。注册失败会明确显示错误与已注册数量，未注册工具仍走 `/api/rf`；执行失败不静默重试。代理调用 execute 也更新页面，人与代理共享结果。

规范 README 原文（2026-10-01 获取）：

> WebMCP is designed to complement, not replace, existing backend-focused protocols like MCP.

> It would not make sense for purely server-side task completion.

来源：<https://github.com/webmachinelearning/webmcp/blob/main/README.md>（Non-goals、Imperative Tool Registration、Discovering and running tools）。WebMCP **补充而非取代后端 MCP**，依赖打开的网页与浏览器上下文，不适用于纯服务端任务。已有后端 MCP 提供 stdio/HTTP JSON-RPC；本目录的 HTTP API 只是页面桥接，不是新的 MCP transport，也不会使 Node 服务本身成为 WebMCP。

## API 与安全边界

- `GET /api/tools`：仅 doctor/config_show/scan/explain 四项，schema 来自后端；外部名称去掉后端的 `rf_` 前缀。
- `POST /api/rf`：`{"tool":"doctor","args":{}}`，仅允许 tool/args 顶层字段；参数、必填项、互斥项、非法选项全部由同一个后端校验器拒绝。
- HTTP 200 保留后端工具结果完整 envelope：`content`、`structuredContent`、`isError`、`_meta["erf/exitCode"]`。CLI 非零退出并不等于 HTTP 失败；无效工具/参数为 HTTP 400。
- 无 shell 字符串拼接。拒绝写类命令、任意 executable、host、output 参数；复用后端的 50 秒执行超时、1 MiB 输出上限和封闭输出字段投影。
- 无 CORS 放行；Host 仅允许当前端口的 localhost/127.0.0.1；存在 Origin 时必须精确同源；POST 强制 application/json，body 上限 16 KiB，HTTP 请求超时 60 秒。无认证，仅供可信本机用户，勿加公网代理或将其作为多租户服务。
- 路径参数可访问服务账号权限内的本地项目，不构成文件系统沙箱。只读指不暴露写类 CLI 与治理操作；scan/构建可能产生 CLI 原有缓存或构建产物，不承诺底层文件系统零写入。
- digest-only 沿用 CLI 投影，不表示结果完全没有路径/诊断；DOM 用 textContent 渲染，不将结果作为 HTML 执行。
- **本页只读、仅本地、digest-only、不产生 Gate/SCM 权威。** 不将 doctor PASS、工具调用成功、WebMCP 激活解释为 Gate PASS 或 SCM 批准。

## 定向验证

```bash
cd /home/kali/文档/ruleops; timeout 60s node --test clients/mcp/webmcp/test/server.test.js; echo TEST_RC:$?
curl -sS http://127.0.0.1:3210/api/tools
curl -sS -H 'Content-Type: application/json' -d '{"tool":"doctor","args":{}}' http://127.0.0.1:3210/api/rf
```

单测捕获共享后端的实际 execFile 边界，检查白名单、参数拒绝、argv 原样传递与 shell=false、回环地址、Host/Origin 边界以及错误 envelope 保留；它不是实际 CLI 执行证据。实际 CLI、浏览器激活验证状态见本次 `.scratch/webmcp-viewer.md` 报告。本任务不运行全量 `scripts/check.sh`。
