# ERF 端到端（E2E）测试套件

覆盖 ERF **全生命周期 × 全功能 × 全运行形态**的可跑 E2E 套件。它把仓库既有的验收入口
（`scripts/*-check.*`、`clients/*/test`）编排成按生命周期排序的用例，并产出一份统一报告；
它**不替换**总门禁 `bash scripts/check.sh`，而是补上「跨形态串联」与「形态/功能逐项」两维覆盖。

## 组成

| 文件 | 作用 |
| --- | --- |
| `scripts/e2e/e2e-cases.json` | 用例注册表：**单一事实来源**（ID/标题/生命周期/形态/类型/命令/期望 rc/证据/前置）。 |
| `scripts/e2e/e2e_runner.py` | 执行器：按筛选跑用例、收集 rc/耗时/日志，产出 `report.json` + `report.md`；可生成用例目录。 |
| `scripts/e2e/run-e2e.sh` | 入口：加载仓库运行环境（`~/.local/opt/erf-env.sh`）后调用执行器。 |
| `scripts/e2e/spine.sh` | 全生命周期主线：串联 L0→L8 的真实产物路径并断言可验证的跨跳连续性，产出 `spine.json`。 |
| `scripts/e2e/intellij-selftest.sh` | IntelliJ 薄客户端纯核自测（`javac`+`java`，零 SDK）。 |
| `docs/e2e/catalog.md` | 用例目录（由注册表自动生成）。 |
| `docs/e2e/README.md` | 本文件。 |

## 运行

在 WSL `kali-linux2` 内、仓库根执行（环境由入口自动加载）：

```bash
# 列出全部用例
bash scripts/e2e/run-e2e.sh --list

# 只跑某一条 / 某几条
bash scripts/e2e/run-e2e.sh --case E2E-L7-003 --case E2E-L8-005

# 按维度筛选
bash scripts/e2e/run-e2e.sh --lifecycle L7-publish
bash scripts/e2e/run-e2e.sh --form IDE
bash scripts/e2e/run-e2e.sh --kind run

# 全量（重；约等于总门禁 + 定向脚本，20–30 分钟）
bash scripts/e2e/run-e2e.sh --all --out .scratch/e2e/full

# 重新生成用例目录
bash scripts/e2e/run-e2e.sh --catalog docs/e2e/catalog.md
```

可选外部输入（默认值写死在执行器内，可用同名环境变量覆盖）：
`PMD_HOME`、`RULESET`、`STANDARD_SOURCE`、`CANDIDATE`、`RUNTIME_HOME`、`INSTALLED_RF`。
缺前置的用例会被标记 `SKIP`，不会伪造通过。

报告落在 `.scratch/e2e/<时间戳>/`（`report.md`、`report.json`、每案例 `<ID>/run.log` 与证据）。
`.scratch/` 按仓库约定保持 gitignore，属临时证据，不入版本库。

### 用例间依赖

少数用例共享同一运行根目录（`{RUNOUT}`），必须按注册表顺序执行：

- `E2E-L3-004`（`production-runtime-pack-check.sh`）产出 `{RUNOUT}/production-runtime-pack-check/`，
  `E2E-L2-002`（`delivery-cli-vertical-check.sh`）复用同一目录——与总门禁 `scripts/check.sh:101→103`
  共用同一输出根一致。`E2E-L2-002` 带 `requires` 前置，缺失时记 `SKIP` 而非失败。

`--case` 只选其一时若前置未就绪，会如实记 `SKIP`；需要这两条时请一并选择或先跑 `E2E-L3-004`。

## 覆盖模型

### 生命周期（横向）

| 阶段 | 含义 |
| --- | --- |
| `SPINE` | 全生命周期串联主线 |
| `L0-authoring` | 规范/规则创作与校验、编排、库存（Standard/RuleIntent/Candidate） |
| `L1-verification` | 候选验证、独立 Verification Worker、本地出口包 |
| `L2-packaging` | RuleBundle 确定性打包、签名发布、可复现构建 |
| `L3-delivery` | Bundle 分发/缓存/离线、Server→Runner 交付、可安装 Release、Runtime Pack |
| `L4-server` | RuleOps Server HTTP/API、持久化、注册表、提交/状态 |
| `L5-execution` | 配置/doctor、Java Host、scan 输出、ChangedSet/affected/增量 |
| `L6-gate` | Gate 判定、Trusted CI、失败/超时/可重复性、TrustCheckpoint、威胁模型、requestId |
| `L7-publish` | Status Publisher、Status Adapter 部署契约、SCM（GitHub）状态适配器 |
| `L8-consume` | Rich Explain、薄客户端、IDE 客户端、MCP、WebMCP、agent commit hook |
| `L9-observe` | Pilot 指标/证据/观察、Fast Decision、RuleOps Center、Feature Evidence、参考 PMD |

### 运行形态（纵向，规范 §51）

`RuleOps Server + rf-runner + Backend Host + IDE Plugin/CLI + Factory Worker + Signed Rule Bundle
+ Signed BackendRuntimePack`，外加本仓库新增的 **MCP server、WebMCP 查看面、SCM Status Adapter**。
`catalog.md` 的「形态」列逐条标注。

### 功能面

CLI 全部子命令（`rf` usage）、Server 全部端点（`contracts/http/ruleops-api-v1.yaml`）、
MCP 工具（`rf_doctor`/`rf_config_show`/`rf_scan`/`rf_explain`）、WebMCP HTTP API
（`GET /api/tools`、`POST /api/rf`）、VS Code 8 条命令、IntelliJ 10 个 action、agent commit hook。

## 状态语义

| 状态 | 含义 |
| --- | --- |
| `PASS` | 命令 rc == 期望 rc（**不**等于业务结论为绿，见下） |
| `FAIL` | 命令 rc != 期望 rc |
| `TIMEOUT` | 超过用例超时 |
| `SKIP` | 前置缺失（如 `{INSTALLED_RF}` 未构建），未执行 |
| `MANUAL` | 需人工/外部主体，附运行手册，不由执行器判定 |

**关键区分**：`PASS` 只表示「命令行为符合预期」。两类用例的 rc 语义要单独读：

- `E2E-L9-011`（参考 PMD 补充门禁）期望 rc=**1**，表示扫出违规（`violationCount>0`），**不是**绿；
- 主线 `E2E-SPINE-001` 的 scan 使用 **fake host/resolver**，证明执行链与 JSON 契约，
  `gateDecision=FAIL` 亦非缺陷（fake host 产出 canned finding）。

## 主线断言什么

`spine.sh` 逐跳记录 rc，并断言**可验证的跨跳连续性**：

1. L0→L5（`cli-surface-check`）：standard/rule 校验 → 生成候选 → 验证 → init → scan，真实产物；
2. L5/L6 确定性：同一 fixture 连扫两次，`resultDigest` 必须一致（`spine.json` 内 `stable`）；
3. L7：发布产物 + 适配器部署契约；
4. L7：SCM 适配器（GitHub，离线 fake provider）；
5. L8：Rich Explain 权威边界；
6. L8：agent commit hook（真实 PMD，前置缺失则记 `skipped-missing-runtime-pack`，不冒充）。

## 明确不覆盖（MANUAL / 边界）

以下形态**必须人工或外部主体**，套件只登记运行手册，绝不冒充通过：

- `E2E-MAN-001`：真实 IDE UI 内交互（需 IDE SDK 与人工）；
- `E2E-MAN-002`：真实访问窗口 Pilot 观察；
- `E2E-MAN-003`：真实 GitHub 状态发布（联网 + 受控凭证）；
- `E2E-MAN-004`：生产可信（TRUSTED）Gate（当前 checkout 为 SHADOW 权限，属治理裁决）。

此外：`PASS` 不代表 Gate/SCM 权威；`doctor` 的 `CONFIG_MISSING`、MCP 的 `isError`、
HTTP 状态码都不等于 Gate 结论。

### 人工项与其离线替身

MANUAL 用例自带 `double` 字段指向可离线复跑的替身；替身**只覆盖对应契约/纯核**，
不产生真实 IDE/窗口/网络/TRUSTED 权威。

| MANUAL 用例 | 离线替身 | 替身覆盖 / 仍缺 |
| --- | --- | --- |
| `E2E-MAN-001` 真实 IDE UI | `E2E-L8-003`、`E2E-L8-004`、`E2E-L8-005` | 双端纯核 + 跨端一致性；仍缺真实 IDE 内交互（导航→解释→草稿→提交） |
| `E2E-MAN-002` 真实访问窗口 | `E2E-L9-013`、`E2E-L9-007` | 本地合成观察（`pilot-local-simulation.sh`，SYNTHETIC_SHADOW）+ phase4 shadow 聚合；仍缺注册窗口与人工 |
| `E2E-MAN-003` 真实 GitHub 发布 | `E2E-L7-003` | 适配器全边界（离线 fake provider）；仍缺真实网络与凭证 |
| `E2E-MAN-004` 生产可信 Gate | **无** | TRUSTED 需控制面签发密钥与授权，离线无法伪造；`E2E-L6-001/002/005` 只覆盖判定路径，不产生 TRUSTED 权威 |

### 用例域（domain）

`catalog.md` 与报告含 `domain` 列，缺省 `core`。参考 PMD 补充扫描为独立用例域
**`reference-pmd`**（`E2E-L9-011`、`E2E-L9-012`）：`E2E-L9-011` 保持 `expectRc=1`
语义——**rc=1 表示扫出违规，不是绿**，且**不并入核心门禁、不提升为强制**。
按域运行：`bash scripts/e2e/run-e2e.sh --domain reference-pmd`。

## 与既有门禁的关系

`scripts/check.sh` 是交付门禁（51 阶段组，全绿才可交付）；本套件的多数 `PASS` 用例是其阶段入口的
**单跑编排**，另补上不在 `check.sh` 内的定向入口（MCP/WebMCP/registry/plugin/standard-extract/
agent-commit-hook/scm-adapter/IDE 纯核）。因此本套件用于**验收与回归取证**，不替代门禁，
也不把「套件 PASS」扩称为「仓库全绿」。
