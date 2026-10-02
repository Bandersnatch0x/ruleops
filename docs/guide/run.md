# 运行方式

## 范围

面向 `04019de` 开发检出和已准备本地 Runtime/Candidate 的操作者；不是生产部署手册。
当前 `./rf` 只提供本地/SHADOW 能力，不授予受保护分支或 SCM 权力（`CONTEXT.md:3`、`rf:41`）。命令面以 `./rf --help` 与 `rf:6` 为准。
以下 Linux 命令均在仓库根执行；本次核对环境只使用 `wsl.exe -d kali-linux2` 和 `/home/kali/文档/ruleops`，不用 `/mnt/d` 作工作树。

## 环境前置

| 项 | 要求与证据 |
| --- | --- |
| JDK | ≥21，配置 `JAVA_HOME`/PATH；门禁显式验证，缺失/版本不符 rc=2（`scripts/check.sh:19`） |
| Maven / Cargo | Java 模块与 Rust workspace 构建；Rust 固定版本见 `rust-toolchain.toml`（本基线为 1.98.1），依赖见 `Cargo.lock`、`pom.xml` |
| Python 3 / jq / rg / openssl | 总门禁显式前置工具（`scripts/check.sh:23`）；不是可选替代品 |
| Node.js | MCP/WebMCP 要求 22+；架构协议探针也使用 Node（`clients/mcp/README.md:8`、`clients/mcp/webmcp/README.md:7`） |
| Bash / Git / timeout | 启动器、Git scope、限时验证使用；hook 另要求允许用户/网络命名空间的 `unshare`（`docs/agent-commit-hook.md:37`） |
| 编译/依赖环境 | 按实际平台提供 linker、Maven/Cargo 依赖；可选环境文件导出 `JAVA_HOME`、PATH、`RUSTFLAGS`（`scripts/check.sh:5`） |

Windows 进入指定环境：

```powershell
wsl.exe -d kali-linux2 --cd /home/kali/文档/ruleops
```

WSL 中准备 PATH（存在本地环境文件时加载；不存在应自行正确配置工具链，而非忽略缺依赖）：

```bash
cd /home/kali/文档/ruleops; if [ -f "$HOME/.local/opt/erf-env.sh" ]; then . "$HOME/.local/opt/erf-env.sh"; fi; export PATH="$HOME/.local/node/bin:$HOME/.cargo/bin:$PATH"; java -version; mvn -version; cargo --version; node --version; command -v python3 jq rg openssl git timeout
```

`check.sh` 会自动加载该文件，不能据此假定所有独立脚本或 `rf` 都会自动加载（`scripts/check.sh:14`、`rf:1`）。退出码在 WSL 内立即保存，不用 Windows shell 的退出状态猜测内部步骤。

## 构建与本地资产

```bash
mvn -q verify; rc=$?; printf 'JAVA_RC=%s\n' "$rc"
cargo build --workspace; rc=$?; printf 'RUST_BUILD_RC=%s\n' "$rc"
```

Java verify 含测试；构建失败先查原日志，不继续声称环境就绪。仓库标准全量验收另见 [testing.md](testing.md)，本文没有运行该门禁。
开发 `rf` 的 init/doctor/config/scan 路由优先使用 `bin/` 中 binary，否则开发检出由 Cargo 启动（`rf:55`、`rf:64`、`rf:72`）。
标准 scan 需要兼容的本地 runtime 和 Candidate 或经验证的交付输入；仅 Cargo build 不保证 Java Host、Resolver、签名/信任资产已经齐备（`README.md:119`）。
可在**提交检查之外**用既有 `scripts/production-runtime-pack-check.sh NEW_OUTPUT` 准备并验证本地 Java Runtime Pack，产物在该输出下 `production-rf/`；该脚本会构建和运行验收，不是纯安装命令，名称不等于生产授权（`scripts/production-runtime-pack-check.sh:5`；`docs/agent-commit-hook.md:162`）。
缺发布资产、缓存或可信身份时应按结构化失败排查，不使用 `--host`/`--resolver` 绕过正式部署边界；这些参数仅留给本地 harness（`README.md:119`）。

## 常用 CLI

以下 `$PROJECT`、`$CANDIDATE`、`$RUNTIME_HOME`、`$INTENT`、`$NEW_CANDIDATE`、`$NEW_EVIDENCE` 是操作者提供的实际路径，输出不得覆盖历史证据。

```bash
./rf --help
./rf init --repo-root "$PROJECT" --non-interactive
./rf config validate --repo-root "$PROJECT"
./rf config show --repo-root "$PROJECT" --format json
./rf doctor --repo-root "$PROJECT" --format json
./rf scan --repo-root "$PROJECT" --candidate "$CANDIDATE" --runtime-home "$RUNTIME_HOME" --format json --progress none; rc=$?; printf 'SCAN_RC=%s\n' "$rc"
./rf rule validate "$INTENT"
./rf rule generate "$INTENT" "$NEW_CANDIDATE"
./rf rule test "$NEW_CANDIDATE" "$NEW_EVIDENCE"
```

- init 只生成可审阅的 `.ruleops/config.yaml`；已有/冲突配置、不确定识别、路径越界不覆盖、不猜测（`README.md:115`）。
- config 优先级是 CLI > environment > project > defaults；scan 消费冻结 scope（`README.md:117`）。
- scan 在 `--rule-id`、`--candidate`、`--delivery-descriptor` 中按契约选一种输入；支持 text/json/sarif、progress 和 output，不能把 registry 元数据当 Candidate（`rf:39`；`contracts/http/rf-cli-v1.yaml`）。
- `--format json` 适用于 doctor/config show/scan/explain；不能随意加给 rule generate/test、coverage 或所有子命令（`rf:9`；`clients/mcp/README.md:91`）。
- explain 消费已有 FindingView：`./rf explain "$FINDING" --format json`；不是读取完整源码再推断新权威（`rf:31`）。

## 本地 RuleOps Server

入口接收 `[port] [journal-path]`，默认 8080；不传 journal 则内存模式。监听 loopback，启动时输出 `ruleops-server=...`；文件 journal 需保留并由同一部署管理，损坏时启动拒绝而非丢弃记录（`control-plane/server/src/main/java/io/ruleops/server/RuleOpsServerMain.java:7`）。
下面用唯一目录准备 classpath，再前台启动，Ctrl+C 停止；重启恢复时复用同一 journal，而不是新建空库：

```bash
OUT="$(mktemp -d "$PWD/.scratch/server-run.XXXXXX")"; mvn -q -pl control-plane/server -am package -DskipTests dependency:build-classpath "-Dmdep.outputFile=$OUT/classpath.txt"; rc=$?; printf 'SERVER_BUILD_RC=%s\n' "$rc"; if [ "$rc" -eq 0 ]; then java -cp "control-plane/server/target/classes:$(cat "$OUT/classpath.txt")" io.ruleops.server.RuleOpsServerMain 8080 "$OUT/governance.jsonl"; fi
```

classpath 启动方式依据 `scripts/registry-client-check.py:99`；journal 参数依据 Main。这个开发启动示例跳过测试，不是发布验收。
服务没有已证明的生产认证/授权，`X-Actor` 不能当认证身份，不对公网开放（`.scratch/server-rule-management.md:17`）。
`/v1/registry/rules` 与 `/v1/registry/standards` 只查询 Store 已注册资源，新服务不自动导入本地 specs（`.scratch/server-rule-management.md:26`）。

```bash
./rf registry list rules --server http://127.0.0.1:8080/v1 --limit 10
./rf registry show rules "$RULE_ID" --version "$VERSION" --expected-digest "$PIN" --server http://127.0.0.1:8080/v1
```

registry 原生输出 JSON，不另加 `--format json`；list 仅一页，续页传 `--cursor-json` 的 JSON 字符串，不能把含 NUL 的原游标直接送入 argv。
show 需要独立可信来源的 pin；list 的 FORMAT_ONLY 与 show 的 PIN_MATCH 均不是签名/有效性认证，authority 仍 UNKNOWN（`.scratch/multi-end-rule-fetch.md:42`、`.scratch/multi-end-rule-fetch.md:61`）。
当前 registry transport 仅字面量回环 HTTP，拒绝远端/重定向/凭据；生产 HTTPS 与身份边界尚非本地客户端能力（`.scratch/multi-end-rule-fetch.md:24`）。

## MCP / WebMCP

预先配置同一 WSL 工具链，在仓库根启动；以下服务互为独立进程，按需求选择，不能在同一前台终端顺序执行而期待同时启动。

```bash
node clients/mcp/erf-mcp-server.js
node clients/mcp/erf-mcp-server.js --transport http --port 3000
node clients/mcp/webmcp/server.js
```

- MCP 默认 stdio，一行一条 UTF-8 JSON-RPC；stdout 只用于协议，stderr 为日志；HTTP 默认为 `POST http://127.0.0.1:3000/mcp`，不是 SSE 服务（`clients/mcp/README.md:15`、`clients/mcp/README.md:48`）。
- Agent 的 Windows `mcpServers` 配置示例见 `clients/mcp/README.md:19`，通过 `wsl.exe` 指向本安装目录；不执行被扫项目自己的不可信 `rf`。
- MCP 仅四工具：rf_doctor、rf_config_show、rf_scan、rf_explain；WebMCP 对应 doctor/config_show/scan/explain，未接入 registry、生成、测试、提交或批准（`clients/mcp/README.md:80`；`clients/mcp/webmcp/README.md:33`）。
- Web 页面默认 `http://127.0.0.1:3210`；Windows 使用 `http://localhost:3210`，依赖 WSL2 localhost 转发；可用 `PORT=3211 node clients/mcp/webmcp/server.js` 换端口（`clients/mcp/webmcp/README.md:13`）。
- `repo_root` 默认相对服务启动 cwd；应显式填写实际项目。scan 恰选 rule_id/candidate；explain 必须 finding（`clients/mcp/webmcp/README.md:15`）。
- Windows Chrome 需 146+ 且实际启用实验能力；启动参数 `--enable-features=WebMCP`，宜使用独立测试 profile，避免复用旧进程导致参数未生效。以 `document.modelContext?.registerTool` 和四项注册成功为准，不以版本号或能打开页面为准（`clients/mcp/webmcp/README.md:19`；`.scratch/webmcp-polish.md:45`）。
- 历史 Windows headless 激活证据见 `.scratch/webmcp-registration-fix.md:23`；本次未重跑浏览器，未发现本次浏览器代理调用成功的证据。不支持实验 API 时仍可手动查看，但不能称为 WebMCP 已激活。
- 两者没有生产鉴权/TLS；WebMCP 固定本机，不能加公网代理冒充多租户服务。只读接口不等于文件系统沙箱或无构建/缓存写入（`clients/mcp/README.md:106`、`clients/mcp/webmcp/README.md:37`）。

## 常见失败与退出码

| 现象 / 范围 | 含义和下一步 | 依据 |
| --- | --- | --- |
| scan rc=0 | 执行完整，Gate **可能 PASS 或 FAIL**；继续读 gateDecision、Completeness、authority | `README.md:128` |
| scan rc=2 / 3 / 4 | 输入或契约错误 / INCONCLUSIVE 或执行不完整 / trust failure；保留结构化诊断，不改成成功 | `README.md:128` |
| usage、缺 runtime | 参数错误或缺安装资产，通常 rc=2；核对 help/包路径 | `rf:49`、`rf:59` |
| doctor CONFIG_MISSING | 实际项目未配置；核对 repo_root，再审阅 init/config，不把它当服务故障 | `clients/mcp/README.md:71` |
| registry rc=3、UNKNOWN | endpoint、连接、响应、版本或 pin 失败；不回退规则、不把空集当通过 | `.scratch/multi-end-rule-fetch.md:25`、`.scratch/multi-end-rule-fetch.md:36` |
| API 冲突/错误 | 相同幂等 key 不同内容、不可变身份冲突、多版本无 version 或隐私越界，修正请求而非覆盖资源 | `docs/adr/0137-versioned-ruleops-server-api-seam.md:27` |
| Maven/Cargo/Node 缺失 | 修正服务进程 PATH；原日志不是 JSON 时 MCP 明确报错，只返回日志摘要 | `clients/mcp/README.md:73`、`.scratch/webmcp-polish.md:61` |
| MCP/WebMCP HTTP 200 + isError=true | HTTP 成功不代表 CLI 成功；读 `_meta["erf/exitCode"]` 与原 JSON | `clients/mcp/webmcp/README.md:35` |
| 本地 hook rc=1 | 本地阻断；PASS 才可能 ALLOW_LOCAL，SKIPPED/NOT_APPLICABLE 均不是 Gate PASS | `docs/agent-commit-hook.md:19` |
| 人工 timeout 返回 124 | GNU timeout 到期，不是业务扫描结论；保留超时和已有日志，先归因再重跑 | 本节使用的 GNU timeout 包装；项目纪律见 `docs/agents/dev-workflow.md:31` |

不要跨工具复用退出码解释：独立 PMD 检查 rc=1 表示违规、rc=2 表示工具错误；标准抽取 rc=0 只是产出草稿（`scripts/plugin-ruleset-check.py:116`；`scripts/standard-rule-extract.py:201`）。
