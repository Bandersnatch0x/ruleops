# 完整测试说明

## 范围与验收层级

基线 `04019de`，说明仓库总门禁的全部阶段、独立新增工具的定向检查、证据判读和复证纪律；不是本次全绿报告。
本次只交付文档并抽查引用，**按任务限制未运行 `bash scripts/check.sh`**，也不把历史报告 PASS 当作本次行为复验。
仓库通常交付门禁仍是 `AGENTS.md:28` 的总门禁；落码→验收需全量套件、标准轴/规格轴评审及符合度核查（`docs/agents/dev-workflow.md:16`、`docs/agents/dev-workflow.md:26`）。

## 总门禁怎么运行、怎么判定

前置工具见 [run.md](run.md)。总门禁加载可选 `~/.local/opt/erf-env.sh`，校验 JDK≥21、mvn/cargo/python3/rg/openssl/jq，前置失败 rc=2（`scripts/check.sh:14`）。
经授权运行全量时，保留唯一运行日志，不截断为最后几行；以下是操作示例，**不是本次执行记录**：

```bash
mkdir -p .scratch/gate; LOG="$(mktemp "$PWD/.scratch/gate/full.XXXXXX.log")"; bash scripts/check.sh >"$LOG" 2>&1; rc=$?; printf 'GATE_RC=%s LOG=%s\n' "$rc" "$LOG"; printf '%s\n' "$rc" >"$LOG.rc"
```

总入口 `set -euo pipefail`，阶段失败即退出；成功必须 rc=0 且到达 `check: all sides green`，还需完整读取本轮日志与阶段证据（`scripts/check.sh:8`、`scripts/check.sh:149`）。
默认阶段输出统一落 `.scratch/gate/`，可以覆盖；独立人工取证必须显式指定另一个唯一目录，不能拿新输出冒充历史原件（`docs/agents/dev-workflow.md:28`）。
没有授权时不为补一份报告私自运行全量；文档核对、定向行为通过和全量通过必须分开陈述。

## 全部阶段总览（按入口实际顺序）

以下路径均是 `scripts/check.sh` 的实际调用，不增删门禁、不修改阈值。每阶段共同判据：命令成功、适用的契约/行为断言成功、当轮证据可追溯；含负例的测试必须证明其被拒绝，不能要求每个 fixture 都业务 PASS。

| 顺序 | 阶段、检查内容与调用 | 入口依据 |
| --- | --- | --- |
| 1 | Java：`mvn -q verify`，模块化单体测试/构建 | `scripts/check.sh:35` |
| 2 | Java 可复现构建：`scripts/reproducible-java-build-check.sh`；两次干净构建 JAR 身份一致 | `scripts/check.sh:37` |
| 3 | Rust：`cargo test --quiet --workspace --all-targets` | `scripts/check.sh:39` |
| 4 | Java Host E2E：`scripts/java-host-e2e.sh` | `scripts/check.sh:42` |
| 5 | Phase 2 独立本地切片：`scripts/phase2-independent-acceptance.sh` | `scripts/check.sh:44` |
| 6 | 独立 Verification Worker：`scripts/verification-worker-check.sh` | `scripts/check.sh:46` |
| 7 | 五条 Java 规则本地出口包：`scripts/phase2-java-local-demo.sh` | `scripts/check.sh:48` |
| 8 | 确定性多规则 Bundle：`scripts/rule-bundle-pack-check.sh` | `scripts/check.sh:50` |
| 9 | Trusted CI 验收：`scripts/phase3-trusted-ci.sh` | `scripts/check.sh:52` |
| 10 | Real Runner handoff：`scripts/phase3-trusted-scan.sh` | `scripts/check.sh:54` |
| 11 | 失败与重复性：`scripts/phase3-incomplete-evidence.sh`、`scripts/phase3-timeout-evidence.sh`、`scripts/phase3-repeatability.sh` | `scripts/check.sh:56` |
| 12 | 创作编排/检索基线：`scripts/authoring-capability-check.sh` | `scripts/check.sh:68` |
| 13 | Java Standard 库存：`scripts/standard-inventory-check.sh` | `scripts/check.sh:70` |
| 14 | Java PMD/SpotBugs/CI 配置库存：`scripts/java-ci-inventory-check.sh` | `scripts/check.sh:72` |
| 15 | Pilot Success Contract 边界：`scripts/pilot-success-contract-check.sh` | `scripts/check.sh:74` |
| 16 | Pilot Success Contract 状态：`scripts/pilot-success-contract-approval-check.sh` | `scripts/check.sh:76` |
| 17 | Phase 0 模板选择证据：`scripts/phase0-template-selection-check.sh` | `scripts/check.sh:78` |
| 18 | 代表 Java 仓库清单：`scripts/representative-java-repositories-check.sh` | `scripts/check.sh:80` |
| 19 | 指标基线：`scripts/pilot-metrics-baseline-check.sh` | `scripts/check.sh:82` |
| 20 | 证据包与裁决：`scripts/pilot-evidence-check.sh` | `scripts/check.sh:84` |
| 21 | Fast Decision 的 shadow 边界：`scripts/fast-decision-check.sh` | `scripts/check.sh:86` |
| 22 | 作者主动时间基线：`scripts/pilot-author-cost-baseline-check.sh` | `scripts/check.sh:88` |
| 23 | Observation/observer seam：`scripts/pilot-observation-check.sh` | `scripts/check.sh:90` |
| 24 | Shadow MVP 清单：`scripts/pilot-delivery-check.sh` | `scripts/check.sh:92` |
| 25 | CLI 命令面：`scripts/cli-surface-check.sh` | `scripts/check.sh:94` |
| 26 | 配置/doctor：`scripts/bootstrap-check.sh` | `scripts/check.sh:96` |
| 27 | 可安装、CI-neutral scan：`scripts/ci-neutral-scan-check.sh` | `scripts/check.sh:98` |
| 28 | Java Runtime Pack：`scripts/production-runtime-pack-check.sh` | `scripts/check.sh:100` |
| 29 | 签名 Bundle delivery CLI：`scripts/delivery-cli-vertical-check.sh`，复用上一阶段输出目录 | `scripts/check.sh:102` |
| 30 | Server HTTP/API：`scripts/server-contract-check.sh` | `scripts/check.sh:104` |
| 31 | Server 持久化/恢复：`scripts/server-persistence-check.sh` | `scripts/check.sh:106` |
| 32 | feature evidence 状态：`scripts/feature-evidence-check.sh` | `scripts/check.sh:108` |
| 33 | Bundle 分发/缓存/离线：`scripts/bundle-delivery-check.sh` | `scripts/check.sh:110` |
| 34 | Server→Runner 交付闭环：`scripts/bundle-delivery-e2e-check.sh` | `scripts/check.sh:112` |
| 35 | 薄客户端动作：`scripts/thin-client-check.sh` | `scripts/check.sh:114` |
| 36 | 客户端申请提交：`scripts/thin-client-submit-check.sh` | `scripts/check.sh:116` |
| 37 | UNKNOWN 提交状态恢复：`scripts/submission-status-check.sh` | `scripts/check.sh:118` |
| 38 | Rich Explain/权威边界：`scripts/rich-explain-check.sh` | `scripts/check.sh:120` |
| 39 | IDE 双端纯核/一致性：`scripts/ide-client-check.sh` | `scripts/check.sh:122` |
| 40 | requestId 跨语言内容寻址：`scripts/request-identity-check.sh` | `scripts/check.sh:124` |
| 41 | scan 输出/诊断/进度：`scripts/scan-output-check.sh` | `scripts/check.sh:126` |
| 42 | StatusPublication artifact：`scripts/status-publication-check.sh` | `scripts/check.sh:128` |
| 43 | Status Adapter 部署/权限：`scripts/status-adapter-deployment-check.sh` | `scripts/check.sh:130` |
| 44 | TrustCheckpoint handoff：`scripts/trust-checkpoint-check.sh` | `scripts/check.sh:132` |
| 45 | Git ChangedSet：`scripts/git-change-scope-check.sh` | `scripts/check.sh:134` |
| 46 | Java affected scope：`scripts/affected-scope-check.sh` | `scripts/check.sh:136` |
| 47 | 冻结增量计划/缓存：`scripts/scan-plan-check.sh` | `scripts/check.sh:138` |
| 48 | 增量 fallback/性能基线：`scripts/incremental-baseline-check.sh` | `scripts/check.sh:140` |
| 49 | 威胁模型：`scripts/threat-model-check.sh` | `scripts/check.sh:142` |
| 50 | Phase 4 聚合 Shadow 验收：`scripts/phase4-pilot-report.sh` | `scripts/check.sh:144` |
| 51 | RuleOps Center E2E：`scripts/ruleops-center-check.sh` | `scripts/check.sh:146` |

第 11 组会清理三个固定 gate fixture 目录，并先检查 gate 根；不要复制其清理逻辑到人工原件目录（`scripts/check.sh:57`）。
这 51 个阶段组不等于 51 项单测；第 11 组含三个脚本，Maven/Cargo 各自还有测试集合。验收不能只统计阶段标题。
即使名称包含 Trusted CI、production、status，测试通过也只证明被测 fixture/契约；当前 checkout 的 SHADOW 权限不因测试改变（`docs/adr/0167-product-audit-adjudication-batch.md:23`）。

## 定向检查与新增独立工具

先在 WSL 加载环境，再创建唯一父目录 `OUT="$(mktemp -d "$PWD/.scratch/targeted.XXXXXX")"`。下列子目录必须使用新名称；不要混入 `.scratch/gate/`。
后端单元检查通常限 60 秒；集成脚本包含构建与多个步骤时按既有实测预算放宽，超时记录为未完成，不删除 timeout 后冒称通过。

| 目标 | 可复跑命令（在已设置 OUT 的 shell） | 验收重点 / 证据来源 |
| --- | --- | --- |
| Server contract | `timeout 60s bash scripts/server-contract-check.sh "$OUT/server-contract"` | 不可变身份、幂等冲突、版本歧义、隐私与只读 registry；要求当轮非空 Surefire 报告，不凭关键词 PASS；`scripts/server-contract-check.sh:143` |
| Server persistence | `timeout 60s bash scripts/server-persistence-check.sh "$OUT/server-persistence"` | journal-before-mutation、重启恢复、损坏拒绝；`scripts/server-persistence-check.sh:139` |
| registry client | `timeout 60s python3 scripts/registry-client-check.py "$OUT/registry"` | 真 Java Server+rf、两类型/双版本/分页、独立 pin、错误/不可达非成功、审计无追加；`.scratch/multi-end-rule-fetch.md:45` |
| plugin ruleset | `python3 scripts/plugin-ruleset-check-test.py "$PMD_HOME" "$OUT/plugin"` | 本地真实 PMD、通过/违规/解析失败/坏 XML/空范围/拒覆盖/摘要隐私；单次 PMD 有 60 秒上限；`scripts/plugin-ruleset-check-test.py:4`、`.scratch/plugin-rule-extension.md:58` |
| standard-rule-extract | `python3 scripts/standard-rule-extract-test.py "$STANDARD_SOURCE" "$OUT/standard"` | 原文样例、分母、稳定摘要、needs_review、拒覆盖、类型语义及负例；内部子进程限 60 秒、Maven 离线；`.scratch/standard-to-rules.md:93` |
| agent-commit-hook 消费边界 | `timeout 60s python3 scripts/agent-commit-hook-test.py "$OUT/hook-boundary"` | 测试替身只证明消费/阻断边界，不能冒充 PMD；`docs/agent-commit-hook.md:150` |
| agent-commit-hook 真实离线 | `timeout 90s python3 scripts/agent-commit-hook-test.py "$OUT/hook-real" --real-rf "$INSTALLED_RF" --candidate "$CANDIDATE" --runtime-home "$RUNTIME_HOME"` | 真暂存 Java、完整 FAIL 阻断、合规 ALLOW_LOCAL、离线隔离；`docs/agent-commit-hook.md:155` |
| 后端 MCP | `timeout 60s node --test clients/mcp/test/` | 真 stdio/HTTP/doctor、只读白名单、argv、Origin/Host、错误 envelope；`clients/mcp/README.md:112` |
| WebMCP | `timeout 60s node --test clients/mcp/webmcp/test/app.test.js clients/mcp/webmcp/test/server.test.js` | 页面注册/错误与 HTTP 安全边界；不等于真浏览器代理执行；`.scratch/webmcp-registration-fix.md:16` |
| Java 最小受影响测试 | `timeout 60s mvn -q -pl control-plane/server -am test` | Server reactor；不替代总门禁；`docs/adr/0137-versioned-ruleops-server-api-seam.md:54` |
| Rust 最小受影响测试 | `timeout 60s cargo test -p erf-thin-client` | crate 名以 `runtime/thin-client/Cargo.toml:2` 为准；不替代跨端验收 |

`$PMD_HOME` 是操作者提供的本地 PMD distribution，`$STANDARD_SOURCE` 是本地规范源文件；外部文件只读使用，不复制进仓库、不联网安装。
plugin checker 扫当前 tracked Java，排除 src/test；它不是 staged-only，也不是 Core Gate。其 passed/violations/tool_error 与 rf 的退出码语义不同（`.scratch/plugin-rule-extension.md:58`；`scripts/plugin-ruleset-check.py:116`）。
标准抽取目前只有有限已核实映射；测试通过不补齐缺失的独立 holdout/nearMiss，也不证明所有规范可自动变成规则（`.scratch/standard-to-rules.md:63`）。

**接入事实**：当前 `scripts/check.sh` 没有直接调用 registry-client、plugin-ruleset、standard-rule-extract、agent-commit-hook 的 Python 检查，也没有直接调用 MCP/WebMCP Node 测试；Rust/Maven 全量可能覆盖所属组件测试，但不能替代这些端到端脚本。本文仅登记入口，不私自修改总门禁。

## 读日志与证据

1. 先核对 repo/commit、命令、输入与工具 digest、时间/唯一输出路径及 rc；缺 provenance 就不能称为本次验证（`CONTEXT.md:149`；`contracts/http/feature-evidence-v1.yaml`）。
2. 总日志按阶段顺序完整读取；发生失败时确认后续阶段是否根本未执行。不要只 grep 最后一个 PASS；Server check 还验证当轮 Surefire XML 新鲜性和非零测试数（`scripts/server-contract-check.sh:154`）。
3. Feature evidence 的 `PASS` 仅表示行为命令成功；`STATIC_ONLY` 是结构/关键词证据；`BLOCKED` 表示行为失败或无法执行。它们不是 GateDecision（`CONTEXT.md:149`）。
4. scan 的 rc=0 仍可能 Gate FAIL；同时读 gateDecision、authority、completeness、diagnostics、resultDigest，完整性不能从 Finding 数量推导（`README.md:128`；`contracts/http/scan-output-v1.yaml:17`）。
5. `.scratch/` 为临时证据位置，不是自动脱敏区。对外报告使用 digest-only/计数/状态/摘要引用；不得直接上传源码、完整 Finding、引擎原始报告、token、私钥或明文 skip 原因（`AGENTS.md:24`；`docs/agent-commit-hook.md:50`；`clients/mcp/README.md:104`）。
6. 本地测试的 Candidate、原始规范样例、构建日志可能含源码/路径；保留所需原件用于本地复核，不把“目录在 .scratch”当隐私合规。人工原件与可覆盖的 gate 输出分离（`docs/agents/dev-workflow.md:29`）。
7. registry 的原始 JSON 可见摘要身份，但 PIN_MATCH 不等于 payload 重算/签名/新鲜性；MCP isError 和 HTTP 状态也不等于 Gate 结论（`.scratch/multi-end-rule-fetch.md:61`；`clients/mcp/webmcp/README.md:35`）。

## 失败归因与全量纪律

- 先区分输入/契约、工具链、依赖/环境、timeout/负载、trust、代码缺陷及既有红；保留失败 rc 和原日志，不静默删证据或放宽阈值（`docs/agents/dev-workflow.md:31`）。
- 负载 flake 在**空载**条件复证，记录环境变化和复证结果；自伤先清理自伤状态；既有红向历史溯源再决定修复或重锚，裁决不是代理擅改（同上）。
- 先跑最窄相关检查定位原因，修复后按交付粒度回到既定全量门槛；定向 PASS 不抵消尚未执行/未完成的全量。
- flake 有空载复证记录后收尾；不为追求绿无限重复全量，不用最近一次覆盖历史红（`docs/agents/dev-workflow.md:32`）。
- 验收记录同时写清已做、未做、未核实；当前文件的引用抽查结果另见 `.scratch/project-docs.md`，不把文档验收扩称为运行时验收。
