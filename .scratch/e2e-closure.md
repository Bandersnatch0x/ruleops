# 任务9：Windows / kali-linux2 完整闭环验证

日期：2026-10-02（Asia/Shanghai）。基线 `codex/repo-governance-20261001` tip `125a75a`，分支 `codex/e2e-closure-20261001`。唯一工作树 `/home/kali/文档/ruleops`；`/mnt/d/...` 仅作只读外部输入，不作工作树。本任务为验证：无产品代码、阈值、既有裁决、ADR、contracts、runtime 改动；不 push、不写票。

> 前一轮 dispatch 在本树留下了证据根 `.scratch/e2e-20261001/` 与其报告（已另存为 `.scratch/e2e-20261001/REPORT-prior-attempt.md`）。本轮**独立重跑**，证据根为 `.scratch/e2e-20261002/`；关键结果与前轮一致（见「异常与复证」），但一切结论以本轮实跑为准。

## 两端结论

- **kali-linux2：核心全量门禁 PASS（rc=0，61 阶段），但参考 PMD 补充门禁 FAIL（226 条真实违规）。** 八项串行任务定向入口全部找到并实跑；主线 init/scan 与 registry list/show、agent commit hook 三态均按预期。**不能称 kali 端全绿**：`plugin-ruleset-check.py` 对 83 个 tracked Java 扫出 226 条违规（engine rc=4），未调规则、未过滤、未改业务代码来变绿。
- **Windows：本任务指定范围 PASS。** Chrome 154.0.8037.59 两次 WebMCP 徽标断言与退出码通过；`POST /api/rf {"tool":"doctor"}` 返回真实 JSON（DoctorReport/CONFIG_MISSING）；VS Code 1.127.0 安装→确认→卸载→确认全部 rc=0。这不等于 Windows 原生 namespace hook、IDE UI 激活或生产可信 Gate 证明。

下表证据统一相对前缀 **`.scratch/e2e-20261002/`**。全部 rc 由 WSL 内脚本/`subprocess` 记录（Windows `wsl.exe … bash -lc '…$?…'` 会把 `$?` 吞成 0，已弃用）；Windows PowerShell 仅转交原生退出码。`manifest.sha256` 保存本轮所有原件摘要。

## A. kali-linux2 逐项 rc 与证据

| 项目 | 实际 rc | 结论 | 证据 |
|---|---:|---|---|
| `bash scripts/check.sh` 全量门禁 | 0，61 阶段 | **PASS** | `gate.log`、`gate.rc`、`gate.stages`；各阶段产物 `.scratch/gate/` |
| `clients/mcp: node --test` | 0（tests 17 / pass 17 / fail 0） | PASS | `mcp.log` |
| `clients/mcp/webmcp: node --test` | 0（tests 5 / pass 5 / fail 0） | PASS | `webmcp.log` |
| `scripts/registry-client-check.py` | 0（38 checks passed） | PASS | `registry/rc.txt`、`registry/*.stdout/.stderr`、`registry.log` |
| `scripts/plugin-ruleset-check.py` | 1（engineExitCode=4） | **FAIL：226 violations** | `plugin.json`、`plugin.log` |
| `scripts/plugin-ruleset-check-test.py` | 0 | PASS（仅工具正负边界） | `plugin-tests.log`、`plugin-tests/` |
| `scripts/standard-rule-extract-test.py` | 0 | PASS | `standard.log`、`standard/` |
| `scripts/agent-commit-hook-test.py`（真实 PMD） | 0（`real_pmd=true`） | PASS | `hook.log`、`hook/results.json` |
| `scripts/server-contract-check.sh` | 0（EXECUTABLE） | PASS | `server-contract.log`、`server-contract/server-contract.md` |
| `scripts/server-persistence-check.sh` | 0（EXECUTABLE，首跑即过） | PASS | `server-persistence.log`、`server-persistence/server-persistence.md` |
| `./rf init --non-interactive`（隔离项目） | 0 | PASS | `init.log`、`mainline/.ruleops/config.yaml` |
| `./rf scan --format json` + JSON 解析 | scan=0，解析=0 | PASS：执行/契约闭环；**gateDecision=FAIL** | `scan.log`、`scan-summary.json`、`smoke.py` |

**无「未发现入口」项**——A2 列出的八项（MCP server、WebMCP 查看面、多端取规则、参考 PMD 补充门禁、规范→规则抽取、提交时校验、服务端注册表 contract/persistence）全部有可复跑入口并已实跑。命令与环境加载见 `run.sh`。

### 端到端主线实际输出摘要

1. 隔离目录 `mainline/` 写入 Maven/Java 样例，执行 `./rf init --repo-root … --non-interactive --language java --build-system maven`（rc=0），写入 config/workspace descriptor/scan plan 三个 digest（见 `init.log`）。
2. `./rf scan --repo-root … --rule-id corp.java.cli-smoke --resolver target/debug/erf-fake-resolver --host target/debug/erf-fake-host --format json`（rc=0）。**主线明确使用 fake host/resolver**，只证明执行链与 JSON 契约，不是生产规则有效性证明。解析（`smoke.py` 写入 `scan-summary.json`）：`kind=ScanResult`、`gateDecision=FAIL`、`authority=LOCAL_UNATTESTED`、`status=FAIL`、`trust=LOCAL_UNATTESTED`、`completeness={planned:1,accounted:1,failed:0,missing:0,observationErrors:0,complete:true}`。fake host 产出 canned finding，故 rc=0 与 Gate FAIL 不矛盾，**不得称合规 PASS**。
3. `rf registry list|show`：由 `registry-client-check.py` 启动**真实本地 Java Server**并写入 fixture，实际调用 rules-list/rules-show/standards-list/standards-show，全部 rc=0；正 pin 生效，负向分支（digest-mismatch、missing-version、malformed-digest、wrong-id/type 等）各按预期返回 rc=3。逐次 rc 见 `registry/rc.txt`（39 行），响应见对应 `.stdout`；服务由脚本回收。
4. **agent commit hook 两态**（真实 PMD，`hook/results.json` 的 `realPmd`）：暂存违规 `System.exit` 阻断（`action=BLOCK, code=FAIL`，工作区已改好仍阻断），真实 `git commit` rc≠0；合规暂存放行（`ALLOW_LOCAL`），真实 `git commit` rc=0。`boundaryChecks=passed` 与替身边界结果严格区分，不冒充 PMD。

## B. Windows 逐项 rc 与证据

| 项目 | 实际 rc | 结论 | 证据 |
|---|---:|---|---|
| Chrome 154 首次启动（WebMCP） | 0 | PASS | `chrome-1.html` |
| Chrome 154 第二次启动（WebMCP） | 0 | PASS | `chrome-2.html`、`chrome-results.json`（两次 `badgeEnabled=true`） |
| PowerShell `POST /api/rf {"tool":"doctor"}` | 0 | PASS：传输/JSON 成功 | `doctor.json`、`doctor-result.json`、`doctor.stderr` |
| VS Code `--version` | 0（1.127.0 x64） | PASS | `vscode-version.log`、`vscode-version-real.txt` |
| `--install-extension <vsix> --force` | 0 | PASS | `vscode-install.log` |
| `--list-extensions`（含 `ruleops.erf-thin-client`） | 0 | PASS | `vscode-installed.log` |
| `--uninstall-extension ruleops.erf-thin-client` | 0 | PASS | `vscode-uninstall.log` |
| `--list-extensions`（不含该扩展） | 0 | PASS | `vscode-uninstalled.log`、`vscode-results.json` |
| hook 暂存违规 Java（按规格在 WSL） | 1 | PASS：**BLOCK**（code=FAIL） | `smoke-violation.json`、`smoke-results.json` |
| hook 暂存合规 Java（按规格在 WSL） | 0 | PASS：**ALLOW_LOCAL** | `smoke-compliant.json` |
| hook 暂存非 Java（按规格在 WSL） | 0 | PASS：**NOT_APPLICABLE** | `smoke-non-java.json` |

WebMCP 服务在 WSL `127.0.0.1:33231`，Windows Chrome 用 `--headless=new --enable-features=WebMCP --virtual-time-budget=15000 --dump-dom http://localhost:33231/`，两次独立临时 profile。DOM 精确命中 **`WebMCP: 已启用（Chrome 146+）`**（`grep -o` 见 `chrome-1.html`）。Chrome 版本取自文件 VersionInfo：**154.0.8037.59**（`chrome.exe --version` 在本机输出乱码，不可用）。

`POST /api/rf` 返回 HTTP 200 且为 JSON object：`structuredKind=DoctorReport`、`isError=true`、`_meta["erf/exitCode"]=2`（`doctor-result.json`）。DoctorReport `overall=CONFIG_MISSING`，因仓库根无 `.ruleops/config.yaml`，依赖检查 NOT_RUN——**传输/JSON 通过不等于环境健康**，未把 CONFIG_MISSING 当成功。

## 未通过 / 未执行项及原因

- **未通过（真实缺陷，非环境噪声）**：`scripts/plugin-ruleset-check.py` → `status=violations`，`violationCount=226`，`inputFileCount=83`，`engineVersion=7.23.0`，`configurationErrorCount=0`、`processingErrorCount=0`（`plugin.json`）。参考 PMD 补充门禁对当前 tracked Java 判 FAIL，属规则/代码层面的真实发现，待另行定位与裁决；本轮只记录，不放水。
- **未执行（超出本任务指定验收，不宣称通过）**：浏览器内由真实模型发起的手动 WebMCP 工具调用、VS Code 命令 UI 激活、Windows 原生 namespace（非 WSL）hook 拦截、生产可信（TRUSTED）Gate。
- **仅传输层证据**：`POST /api/rf` 与 Chrome 徽标只证明桥接与激活，不证明 Gate/SCM 权威。

## 闭环结论（逐项）

| # | 项目 | 结论 |
|---|---|---|
| A1 | check.sh 全量门禁 | **PASS**（rc=0/61 阶段） |
| A2 | 八项定向入口 | MCP **PASS**／WebMCP **PASS**／多端取规则 **PASS**／参考 PMD 补充门禁 **FAIL**／规范→规则抽取 **PASS**／提交时校验 **PASS**／服务端 contract **PASS**／服务端 persistence **PASS** |
| A3 | 端到端主线（init→scan→解析） | **PASS**（执行/契约）；Gate 判定为 FAIL，**非 PASS** |
| A3 | registry list/show（本地服务端） | **PASS** |
| A3 | agent commit hook 违规/合规两态 | **PASS**（BLOCK／ALLOW_LOCAL） |
| B4 | Chrome 154 WebMCP 真机 + POST | **PASS** |
| B5 | VS Code 安装/确认/卸载/确认 | **PASS** |
| B6 | hook 三态冒烟 | **PASS**（BLOCK／ALLOW_LOCAL／NOT_APPLICABLE） |
| — | 生产可信 Gate | **INCONCLUSIVE**（未验证） |

## 异常、失败与复证（不可省略）

- **Windows 侧 `$?` 吞值**：首轮 `wsl.exe … bash -lc '…; echo $? > rc'` 被 Windows 侧把 `$?` 展开成 `0`，rc 采集失效（同 .scratch 旧坑）。改为**落盘 WSL 内脚本** `gate.sh` 采集，最终 `gate.rc=0`、`gate.stages=61` 才是证据；不猜补退出码。
- **MSYS 路径转换**：把裸 `/home/kali/…` 绝对路径作为独立参数传给 `wsl.exe` 时被 MSYS 改写成 `E:/Git/Git/home/…`，rc=127。改用 `MSYS_NO_PATHCONV=1` 或 `$HOME` 相对引用后正常。
- **VS Code 首跑路径错误**：初次把 WSL 路径 `/mnt/c/Users/…/erf-e2e-closure2.vsix` 传给 Windows `code.cmd`，被解释为 `C:\mnt\c\Users\…` → ENOENT，install/uninstall rc=1（该次日志已被随后成功轮覆盖）。改传 Windows 路径 `C:\Users\amsterdam\AppData\Local\Temp\erf-e2e-closure2.vsix` 后，install/list/uninstall/list 全部 rc=0。
- **并发自伤复现对照**：前轮 `server-persistence-check.sh` 首跑 rc=1（与全量 clean-build 并发共享 `target/`，Surefire fork 失败），空载复证 rc=0。本轮**先等全量门禁结束再跑定向**，`server-persistence` 首跑即 rc=0，印证前轮诊断为并发共享 target 的自伤。
- **前轮证据对照**：前轮 `plugin.json` 的 `analysisDigest/inputDigest/rulesetDigest/toolDigest` 与本轮**逐字相同**，`violationCount=226` 一致（仅 `rawReportDigest` 因路径/时间戳不同）；前轮 `gate-confirmed.rc=0/stages=61` 与本轮一致。结论可复现，非一次性偶然。
- **Chrome 版本取数**：`chrome.exe --version` 在 Windows PowerShell 下输出乱码（GBK/UTF-8 混），弃用；改用 `(Get-Item …chrome.exe).VersionInfo.ProductVersion` = `154.0.8037.59`。
- **前轮残留进程**：WSL 内存在 **前一会话**（启动于 2026-10-01 18:29:56）遗留的 `node clients/mcp/webmcp/server.js`（pid 579322，当前未监听端口）。非本轮产物，按「先查证再处置」保留原状并在此列明；本轮自建的 WebMCP 服务 `127.0.0.1:33231` 已由 `finally` 终止并 wait（`web-server-cleanup.txt`）。

## 清理、提交与边界

- **VS Code 扩展已卸载**并最终 `--list-extensions` 确认不含 `ruleops.erf-thin-client`；本轮自建的 Windows 临时副本 `%TEMP%\erf-e2e-closure2.vsix` 与两个 Chrome profile `%TEMP%\erf-e2e-closure2-{1,2}` **已删除**（`cleanup` 复核无残留）。
- 本轮产生的 `scripts/__pycache__` 已移除；`git status` 干净。未安装新浏览器/运行时，未改系统策略。
- 证据根 `.scratch/e2e-20261002/`（本轮）与 `.scratch/gate/`（门禁阶段产物）为列明保留的本地证据，未覆盖旧人工证据根；`.scratch/e2e-20261001/`（前轮）保留并另存其报告。
- **提交到分支但不 push**：报告 `.scratch/e2e-closure.md`（`.scratch/` 被 `.gitignore` 忽略，按任务要求 `-f` 强制纳入）。
- 标准轴：只验证/记录，无产品实现或裁决变更。规格轴：A/B 所列入口均实跑；rc 缺失、工具乱码、负向结果均明确区分。**最终不能称「两端全绿」——Windows 指定闭环 PASS；kali 因参考 PMD 真实违规 FAIL；生产可信 Gate 未验证/INCONCLUSIVE。**
