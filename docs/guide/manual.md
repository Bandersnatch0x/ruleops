# 使用手册

## 谁用、用到哪里为止

本手册面向规则作者和规则使用者，基线 `04019de`。先按 [run.md](run.md) 准备本地环境、兼容 Runtime 和真实项目路径；不需要为了日常扫描填写 Pilot 证据 YAML（`docs/pilot/operator-quickstart.md:3`）。
所有路径变量由操作者填写；以下 `./rf` 指 ERF 安装/开发检出的受信入口，不是从不可信目标仓库随便执行同名脚本（`clients/mcp/README.md:103`）。
**当前终点是本地可复核反馈，不是生产强制。** LOCAL_UNATTESTED、SHADOW、UNKNOWN、received-only 等原状态必须保留（`CONTEXT.md:3`；`rf:41`）。

## 路径 A：规则作者

### 1. 从 Standard/Scenario 开始，而不是先贴一段检查代码

- Standard 是正式要求，Scenario 是其最小可判定业务单元；Coverage Ledger 记录已覆盖、新规则需求、环境/能力缺口及不可机器检测状态（`CONTEXT.md:51`）。
- RuleIntent 表达 backend-neutral 语义，RuleImplementation 才是特定 Backend 的实现；Candidate 尚未通过独立 Verification 与发布治理（`CONTEXT.md:63`、`CONTEXT.md:69`）。
- 用仓库 `specs/standard/`、`specs/rule-intent/`、`specs/coverage/` 的既有形态，不新造术语。实际例子 `specs/rule-intent/corp.java.no-system-exit.yaml:8` 绑定 Standard/Scenario，scope 区分 main/test 并声明 requiredCapabilities。
- 人工确认规范意图、scope、正反例和能力缺口；不能把不支持项删掉后宣称全覆盖。AI 草稿无批准或签名权（`docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md:345`）。

```bash
./rf standard validate "$STANDARD"
./rf rule validate "$INTENT"
```

语法与语义校验入口见 `rf:15`；校验成功不表示批准发布。版本与引用变更必须走既有治理，不能覆盖已发布资产身份（`CONTEXT.md:81`；`docs/adr/0137-versioned-ruleops-server-api-seam.md:19`）。

### 2. 可选：把本地规范文档抽成待审核草稿

```bash
python3 scripts/standard-rule-extract.py "$STANDARD_SOURCE" "$NEW_DRAFT_DIR"; rc=$?; printf 'EXTRACT_RC=%s\n' "$rc"
```

只读输入，输出目录必须不存在；生成 inventory 和 JSON 语法的 YAML 草稿，不会自动执行/发布（`scripts/standard-rule-extract.py:201`）。
当前确定性映射仅覆盖已核实的 J000086 形态，其余返回 `needs_review`；原文正反例与来源身份保留，独立 holdout/nearMiss 缺口未被自动补齐（`.scratch/standard-to-rules.md:63`、`.scratch/standard-to-rules.md:84`）。
rc=0 只表示至少产出一个草稿，rc=1 是无映射草稿，rc=2 是输入/抽取失败；不能把 0 解释为整份规范覆盖或 RuleRelease 可用（`scripts/standard-rule-extract.py:216`）。

### 3. 生成 Candidate，再验证

为每次生成和验证选择新的本地输出目录：

```bash
./rf rule generate "$INTENT" "$NEW_CANDIDATE"
./rf rule test "$NEW_CANDIDATE" "$NEW_EVIDENCE"
```

这是作者主线，入口见 `docs/pilot/operator-quickstart.md:28`。失败先查参数、契约/能力与真实验证结果；不对调正反例、删难测样例或放宽阈值来求通过。
Candidate/Evidence 仍是本地资产；要成为可交付签名 Release，需独立 Verification、人工评审及发布治理，作者/Factory Worker 没有发布签名权限（`CONTEXT.md:73`；`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md:2712`）。

### 4. init → config → scan，验证目标项目上的行为

```bash
./rf init --repo-root "$PROJECT" --non-interactive
./rf config validate --repo-root "$PROJECT"
./rf config show --repo-root "$PROJECT" --format json
./rf doctor --repo-root "$PROJECT" --format json
./rf scan --repo-root "$PROJECT" --candidate "$NEW_CANDIDATE" --runtime-home "$RUNTIME_HOME" --format json --progress none; rc=$?; printf 'SCAN_RC=%s\n' "$rc"
```

仅在项目尚未有配置时 init；遇到已有配置不覆盖，先审阅 `.ruleops/config.yaml`。检查 main/test、模块、include/exclude 是否符合规范意图，再看结果（`README.md:115`、`README.md:122`）。
作者本地 fixture 成功不替代项目级完整执行；局部/增量 scope 也不能被描述成全仓覆盖（`CONTEXT.md:239`；`contracts/http/rf-cli-v1.yaml:22`）。

### 5. 可选外部 PMD ruleset 比较，不把它当注册/发布

```bash
python3 scripts/plugin-ruleset-check.py "$PROJECT" "$PMD_HOME" "$RULESET_XML" "$NEW_REPORT_JSON"; rc=$?; printf 'PLUGIN_RC=%s\n' "$rc"
```

它检查当前 tracked Java 内容、排除 src/test，拒绝空范围/路径越界，输出计数/摘要；不是 staged-only 检查，也不把外部 XML 自动变成 ERF RuleIntent/Release（`.scratch/plugin-rule-extension.md:58`）。
rc=0/1/2 分别为 passed/violations/tool_error，与 scan rc=0 可能 Gate FAIL 的语义不同（`scripts/plugin-ruleset-check.py:116`）。
外部工具与规则文件由操作者提供，只读使用；本仓库不因此分发这些资产或获得其生产 CI 权威（`.scratch/plugin-rule-extension.md:47`）。

## 路径 B：规则使用者

### 1. 明确输入来源

准备实际 Java 项目、已审阅配置、匹配的 Runtime 和 Candidate，或已有获准交付流程提供的 descriptor/bundle 输入。
`rf registry list/show` 可查询 Server Store 的规则/规范身份，不能下载源码、完整 payload 或安装可执行规则；新服务不会自动加载本地 specs（`.scratch/server-rule-management.md:26`；`.scratch/multi-end-rule-fetch.md:61`）。

```bash
./rf registry list rules --server "$SERVER" --limit 10
./rf registry show rules "$RULE_ID" --version "$VERSION" --expected-digest "$PIN" --server "$SERVER"
```

`$SERVER` 当前必须是受支持的字面量回环 HTTP endpoint；`$PIN` 来自独立可信输入，而非复制本次响应后自比。FORMAT_ONLY/PIN_MATCH、UNKNOWN authority 不能作发布证据；找不到、离线或摘要不符是显式失败，不会回退本地规则（`.scratch/multi-end-rule-fetch.md:24`）。

### 2. 初始化、审阅和诊断

```bash
./rf init --repo-root "$PROJECT" --non-interactive
./rf config validate --repo-root "$PROJECT"
./rf config show --repo-root "$PROJECT" --format json
./rf doctor --repo-root "$PROJECT" --format json
```

已有配置时从 validate 开始。非交互识别不确定、路径越界、配置冲突会失败关闭，不能猜测；配置优先级为 CLI > environment > project > defaults（`README.md:115`）。
doctor 检查工具、缓存完整性、TrustState 等；其 PASS 不等于 Gate PASS。CONFIG_MISSING 应修正项目路径/配置，不能让 MCP/UI 抹掉错误（`README.md:124`；`clients/mcp/README.md:4`）。

### 3. 扫描与保存结果

```bash
PROJECT="/absolute/path/to/project"
./rf scan --repo-root "$PROJECT" --candidate "$CANDIDATE" --runtime-home "$RUNTIME_HOME" --format json --progress none; rc=$?; printf 'SCAN_RC=%s\n' "$rc"
./rf scan --repo-root "$PROJECT" --candidate "$CANDIDATE" --runtime-home "$RUNTIME_HOME" --format sarif --output "$PROJECT/scan.sarif"
```

将 `PROJECT` 替换为实际目标项目的绝对路径。SARIF 命令是另一种展示/保存方式，不要求连续重复扫描；`scan.sarif` 为目标仓库内操作者选择的新输出，先确认不覆盖所需旧结果（`docs/pilot/operator-quickstart.md:19`；`README.md:120`）。
`--output` 的相对路径按当前工作目录（cwd）解析，不按 `--repo-root` 解析；在 ERF 仓根扫描外部项目时，`--output scan.sarif` 会指向项目外并被 `OUTPUT_PATH_INVALID` 拒绝，因此这里使用项目内绝对输出路径。
已有适配的 rule-id 或 delivery descriptor 时按 help/交付契约选一种输入，不能混合 Candidate 与 descriptor；缺运行包/依赖不应靠 harness 参数假装成功（`contracts/http/rf-cli-v1.yaml:18`、`contracts/http/rf-cli-v1.yaml:50`）。
需要显式增量时使用成对 `--base`/`--head`，不猜 HEAD~1；图或策略事实不能证明时 Runtime 扩大范围，不静默缩小（`contracts/http/rf-cli-v1.yaml:22`）。

### 4. 结果判读（两条路径通用）

| 看什么 | 正确解释 | 证据 |
| --- | --- | --- |
| 进程 rc | scan 0=执行完整但可能 FAIL；2=输入/契约；3=INCONCLUSIVE/不完整；4=trust failure | `README.md:128` |
| gateDecision | PASS/FAIL/INCONCLUSIVE，结合 Policy、Finding、Completeness、Baseline、Exception、可信上下文，不从数量猜 | `CONTEXT.md:343` |
| completeness | 冻结计划分母中的每个必需规则/分析单元是否完整执行；0 Finding 不能证明完整 | `CONTEXT.md:239`、`CONTEXT.md:243` |
| authority / stage | LOCAL_UNATTESTED 和 SHADOW 不等于可信生产 Gate；UNKNOWN 不能转换成成功 | `CONTEXT.md:3`、`CONTEXT.md:347` |
| findingRecords / diagnostics | 只读闭合投影里的消息、身份、允许位置和结构化失败原因；不要索取原始引擎报告作跨端输出 | `contracts/http/scan-output-v1.yaml:17`；`clients/mcp/README.md:104` |
| resultDigest / 配置与 scope digest | 绑定结果与输入身份以便复核，不是独立签名或权限证明 | `contracts/http/scan-output-v1.yaml:24`；`CONTEXT.md:409` |

不要把修复失败简化为关闭规则、篡改 baseline 或把 INCONCLUSIVE 映射为 PASS。历史债务、比较起点与质量样本是不同概念，例外也必须经签名治理而非删除 Finding（`CONTEXT.md:331`、`CONTEXT.md:413`）。

### 5. 解释、反馈与例外申请

用已有闭合 FindingView 文件 `$FINDING`（不是完整引擎输出）发起：

```bash
./rf explain "$FINDING" --format json
./rf feedback draft "$FINDING" --classification "$CLASS" --reason "$REASON" --actor "$ACTOR" --output "$NEW_REQUEST"
./rf feedback submit "$NEW_REQUEST" --server "$SERVER"
./rf submission status --server "$SERVER" --artifact-digest "$REQUEST_DIGEST"
```

分类与参数必须满足现有契约，不随意发明值；例外路径是 `rf exception request` 后 `rf exception submit`（`rf:31`；`contracts/http/thin-client-v1.yaml`）。
提交显式选择 Server，未知网络状态先用 submission status 查；不自动重试、不当成获批。回执只代表收到申请，例外批准、撤回和 Gate 权力不在薄客户端（`rf:41`；`docs/adr/0141-thin-client-server-submission-transport.md:19`）。

## Agent、浏览器与本地提交检查

MCP/WebMCP 的启动和参数见 [run.md](run.md)。两者仅暴露 doctor/config_show/scan/explain；不会替用户 init、生成 Candidate、提交反馈、获取生产权力（`clients/mcp/README.md:80`）。
WebMCP 依赖已打开网页与实际实验 API 注册，纯服务端任务仍用后端 MCP；HTTP 200 必须继续看 isError 与原 CLI rc（`clients/mcp/webmcp/README.md:29`、`clients/mcp/webmcp/README.md:35`）。

Agent 提交检查为**自愿本地约束**，详见 `docs/agent-commit-hook.md`，默认不自动安装。
Git hook 检查冻结 index，不读取工作区 Java 冒充待提交内容；首版面向文件局部 Java PMD Candidate，缺跨文件/字节码上下文就不能声称全仓验证（`docs/agent-commit-hook.md:39`）。
`ALLOW_LOCAL` 只是允许本次本地操作；FAIL、INCONCLUSIVE、UNKNOWN、坏 JSON、工具缺失/超时等会阻断。非 Java 为 NOT_APPLICABLE，显式摘要审计 skip 为 SKIPPED，两者均非 PASS（`docs/agent-commit-hook.md:19`）。
启用前检查已有 hooks，不覆盖；`--no-verify` 等能绕过本地 hook，无法承诺绕过路径也有审计，更不是受保护分支权限边界（`docs/agent-commit-hook.md:55`、`docs/agent-commit-hook.md:132`）。

## digest-only 与隐私

- 跨端默认不上传源码/片段/完整 Finding；只返回契约允许的摘要、计数、消息和相对位置。digest-only 不是“所有字段都只有哈希”（`clients/mcp/README.md:104`；`clients/mcp/webmcp/README.md:39`）。
- `.scratch/` 保存本地临时材料；Candidate、规范正反例和测试 fixture 可能含源码，不应整体外发。该目录不构成沙箱或自动脱敏（`AGENTS.md:24`；`docs/agents/dev-workflow.md:30`）。
- MCP/WebMCP 的执行权限等于服务账号；只读工具也可能触发原 Runtime/Cargo 缓存和构建写入，不是任意不可信仓库的隔离环境（`clients/mcp/README.md:106`）。
- hook stdout/审计仅状态、计数、时间与摘要，不转发源码、路径、Finding 或明文 skip 原因；不得写入 token、私钥或账号凭据（`docs/agent-commit-hook.md:50`）。

## 明确不承诺

当前不承诺生产 BLOCKING、受保护分支强制、真实 SCM 回写；不把测试 PASS、doctor PASS、零告警、缓存命中、PIN_MATCH、HTTP 200 或 ALLOW_LOCAL 写成生产 Gate PASS（`docs/adr/0167-product-audit-adjudication-batch.md:23`）。
不承诺任意自然语言规范自动生成可发布规则、任意 Backend/语言均支持、独立工具已覆盖总门禁，也不承诺本地 Server 已具备生产认证/TLS/多租户与多写者耐久性（`.scratch/standard-to-rules.md:63`；`.scratch/server-rule-management.md:17`）。
真实签名、组织授权、TLS/credential store、SCM connector 与受保护分支启用须由外部部署/治理验证；未发现本次文档交付提供这些能力已启用的证据（`docs/adr/0143-status-adapter-deployment-contract.md:26`）。
