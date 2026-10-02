# ERF MCP server

零 npm 依赖的后端 MCP 适配器，仅调用本安装目录的 `../../rf`，不实现另一套 Runtime、Factory 或判定逻辑。
**本 server 不改变任何 ERF 判定。** `DoctorReport` 的 PASS 也不是 Gate PASS；工具调用成功、MCP 握手成功均不代表批准或 SCM 权威。

## 环境与启动

需要 Node.js 22+、可执行的 `rf` 及其现有运行环境。本次验证环境是 WSL `kali-linux2`、Node 24；不在 `/mnt/d` 上运行。
开发检出中的 `rf` 会调用 Cargo；扫描还需要原项目要求的 Java/Backend 等组件。零依赖指不增加 npm 包，并非无需 ERF 运行环境。

```bash
cd /home/kali/文档/ruleops; export PATH="$HOME/.local/node/bin:$HOME/.cargo/bin:$PATH"; node clients/mcp/erf-mcp-server.js
```

默认 stdio：每行一个 UTF-8 JSON-RPC 2.0 消息；stdout 只输出协议消息，日志只到 stderr。

### Agent 配置（stdio）

以下是 Windows 上支持 `mcpServers` 配置的 agent 的示例，server 实际运行在指定 WSL 工作树。
其他配置格式的 agent 使用相同 command/args；这是配置示例，不会自动修改任何 agent 设置。

```json
{
  "mcpServers": {
    "erf": {
      "command": "wsl.exe",
      "args": [
        "-d", "kali-linux2", "--cd", "/home/kali/文档/ruleops", "--",
        "/usr/bin/env",
        "PATH=/home/kali/.local/node/bin:/home/kali/.cargo/bin:/usr/local/bin:/usr/bin:/bin",
        "/home/kali/.local/node/bin/node",
        "/home/kali/文档/ruleops/clients/mcp/erf-mcp-server.js"
      ]
    }
  }
}
```

在 WSL 内运行的 agent 可直接用 Node 的绝对路径作为 command、脚本绝对路径作为 args，并继承已配置的 PATH。
上例 PATH 需按实际运行环境补充 Java/Maven 等目录。建议 tool 调用总是传明确的 `repo_root`；省略时相对于 **server 启动工作目录**，不是脚本目录。

### Streamable HTTP（JSON 响应）

```bash
cd /home/kali/文档/ruleops; export PATH="$HOME/.local/node/bin:$HOME/.cargo/bin:$PATH"; node clients/mcp/erf-mcp-server.js --transport http --port 3000
```

默认监听 `127.0.0.1:3000`，endpoint 为 `POST http://127.0.0.1:3000/mcp`。支持显式 `--host IP`、`--port 0..65535`；端口 0 用于临时端口。
主机参数只接受 IP；IPv6 回环可用 `--host ::1`。非回环监听必须显式指定（例如 `--host 0.0.0.0`），启动时会向 stderr 告警。

- 请求头：`Content-Type: application/json`、`Accept: application/json, text/event-stream`。
- 握手后发送 `MCP-Protocol-Version: 2025-06-18`；未知显式版本返回 HTTP 400。
- 服务支持 MCP `2025-06-18`；客户端请求不同版本时 initialize 返回服务支持的版本，由客户端决定是否继续。
- `initialize`、`tools/list`、`tools/call`（以及 `ping`）返回 JSON-RPC 2.0 JSON 对象。
- 通知（例如 `notifications/initialized`）返回 HTTP 202 空体，不执行工具，不生成 RPC 回复。
- 无状态：不创建 `Mcp-Session-Id`；不提供 SSE、服务端主动请求、恢复/重放或批量请求。`GET`、`DELETE` 返回 405，这是无需服务端事件流时的 Streamable HTTP JSON 模式。
- 校验 Origin；回环监听还校验 Host，拒绝跨站调用与常见 DNS rebinding。没有 CORS 放行。

握手请求：

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"local-agent","version":"1.0.0"}}}
```

随后发送 `notifications/initialized`、`tools/list`，再调用：

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rf_doctor","arguments":{"repo_root":"."}}}
```

工具结果将 CLI JSON 同时放入 `content[0].text`（可 JSON.parse）和 `structuredContent`，原 CLI 退出码放在 `_meta["erf/exitCode"]`。
**有效 JSON 的非零退出也原样返回**，并设置 `isError: true`；例如缺少配置时保留 `CONFIG_MISSING` / rc=2，不伪造成功。
若 CLI 启动失败、超时、非 JSON 输出或隐私边界拒绝，则返回 `isError: true` 和明确错误码；原 stdout/stderr 仅提供 SHA-256 摘要，不回传原始日志。

## 当前工具（只读且原 CLI 支持 JSON）

所有 `inputSchema` 均为闭合 JSON Schema（`additionalProperties: false`）；`repo_root` 可选，默认 `.`。
`format` 由 server 固定为 `json`，不是可由 agent 改写的参数。

| Tool | 额外参数 | 固定 CLI 路由 |
| --- | --- | --- |
| `rf_doctor` | 无 | `rf doctor --repo-root PATH --format json` |
| `rf_config_show` | 无 | `rf config show --repo-root PATH --format json` |
| `rf_scan` | `rule_id` / `candidate` 必须且仅能选一项 | `rf scan --repo-root PATH (--rule-id ID / --candidate DIR) --format json --progress none` |
| `rf_explain` | 必须 `finding`，可选 `rule_meta` | `rf explain FINDING.json [--rule-meta FILE] --format json` |

`candidate`、`finding`、`rule_meta` 相对于 `repo_root` 解析为绝对路径。参数不允许控制字符、空串、以 `-` 开头的选项伪装；不接受任意 argv、output、host、resolver、scan_probe、server URL 或改变权威的参数。

### 暂不支持（不在 tools/list 中，也不能通过 tools/call 绕过）

| 原规划名称 | 当前 CLI 实情 |
| --- | --- |
| `rf_rule_generate` | `rf rule generate INTENT.yaml OUTPUT_DIR` 写 Candidate，不支持 `--format json` |
| `rf_rule_test` | `rf rule test CANDIDATE_DIR OUTPUT_DIR` 写 Evidence，不支持 `--format json` |
| `rf_coverage_analyze` | `rf coverage analyze OUTPUT_DIR` 写报告/测试日志，不支持 `--format json` |
| `rf_pilot_window_report` | `rf pilot window-report` 要求 `--output`，写 Markdown，非 JSON stdout |

以上经任务裁决不暴露、不假执行、不以临时写操作冒充只读。后续先在原 CLI 提供真实 `--format json` 契约及只读语义，再考虑接入；仅加 JSON 格式不能消除已有写入副作用。
`feedback submit`、`exception submit`、`pilot evidence package`、发布/批准等写操作也始终不暴露。本版没有启用写工具的环境开关；将来如要新增写能力，必须另行设计显式 opt-in 环境开关并记录权限边界，不能悄悄放进默认清单。

## 安全与权限边界

1. **只用 argv 数组**：`execFile(安装目录/rf, argv, {shell: false})`。不拼接 shell 字符串，不从请求选择可执行程序，也不执行被扫描仓库自己的 `./rf`。
2. **digest-only 投影**：使用 CLI 的 Doctor/配置/ScanResult/ScanFailure/RichExplanation 闭合字段清单，递归拒绝源码、完整 Finding 和任何未知附加字段；不读取/补充源码，不返回原始引擎报告或日志。FindingView/FindingRecord 只保留 CLI 既有摘要、消息和位置字段。拒绝整份输出而不是删字段后冒充原摘要仍有效；CLI 未来加字段时需显式审核更新清单。
3. **不创造权威**：原 `gateDecision`、`authority`、`authoritative`、拒绝原因及摘要不改写，不从进程 rc 推导 Gate PASS；`isError` 只表示 CLI/工具调用状态。
4. **不是文件系统沙箱**：允许访问 agent 显式选择的仓库/文件路径，运行权限等于 server 进程。只读指无治理/提交/产物生成工具；开发版 Cargo 构建、扫描依赖和已有 Runtime 缓存仍可能写自己的工作目录。安装的 rf、PATH 与依赖必须可信。
5. **HTTP 默认仅本机**：不内置鉴权或 TLS。不要直接暴露到不可信网络；显式非回环时应配受信网络和带身份认证的 TLS 代理。Origin/Host 校验不能替代身份认证。
6. **资源边界**：单消息/HTTP body 1 MiB，CLI stdout/stderr 各 1 MiB，CLI 子进程 50 秒超时，HTTP 请求 60 秒、headers 10 秒。超时不重试、不降级为成功；Node 超时终止直接启动的进程，不承诺隔离/回收外部 CLI 自己派生的全部进程树。

此交付是后端 MCP，不是浏览器内的 WebMCP；没有浏览器 UI，也不引入页面工具注册或 discovery 服务。

## 检查

```bash
cd /home/kali/文档/ruleops; export PATH="$HOME/.local/node/bin:$HOME/.cargo/bin:$PATH"; node --test clients/mcp/test/
```

测试会启动真实 stdio 与 HTTP server，调用真实 `rf doctor`，并与直接 CLI 输出比较；覆盖 JSON-RPC 形状、通知、只读清单、argv、隐私拒绝、HTTP Origin/Host/方法/大小边界。`test/index.js` 仅用于 Node 显式目录参数的入口解析。
本任务不运行全量 `scripts/check.sh`，不修改既有门禁证据；这些定向测试不代表全仓门禁已通过。
