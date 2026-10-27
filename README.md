# ruleops

Enterprise Rule Factory (ERF) — 面向平台工程与安全/合规工程团队的**可信规则执行层**：用冻结
ScanPlan 的 Completeness、PASS/FAIL/INCONCLUSIVE 三态 Gate 与签名执行证明，取代「0 告警即通过」，
把企业代码规范转化为可验证、可审计、可演进的机器检查资产。

> 边界（ADR-0167 收窄）：本 checkout 的 `./rf` 显式禁用 release/gate/promote 与 SCM 回写
> （`rf:272-274`），扫描权威恒为 `LOCAL_UNATTESTED` / `SHADOW`。「可执行 / 受保护分支强制」是
> **待验证假设与后续阶段目标**，不在当前交付边界内，不作对外承诺。

- 设计基线（非当前交付清单）: `docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md`
- 白皮书（设计背景，当前承诺范围见文首说明）: `docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md`
- 领域统一语言: `CONTEXT.md`(单上下文 + `docs/adr/`)

## Build & check

| 侧 | 命令 | 位置 |
| --- | --- | --- |
| Java(Factory 单体) | `mvn verify` | 仓库根(`control-plane/*`) |
| Rust(Runtime) | `cargo test` | 仓库根(`runtime/*`) |
| 两侧一键检查 | `./scripts/check.sh` | 仓库根 |

前置:JDK 21+(`JAVA_HOME` 或 PATH)、Cargo。若工具链不在标准路径,可提供可选的
`~/.local/opt/erf-env.sh`(契约:导出 `JAVA_HOME`、`PATH`、可选 `RUSTFLAGS`);
`check.sh` 存在则加载,并显式校验 Java 版本 ≥ 21。

可复现性:Java 插件/JUnit 版本锁定于根 `pom.xml`,`project.build.outputTimestamp`
为固定值;同一 commit 两次 `mvn clean verify` 应产出 byte-identical jar
(`find control-plane -name "*.jar" \| sort \| xargs sha256sum` 前后对比)。
仓库门禁通过 `scripts/reproducible-java-build-check.sh` 自动执行这两次构建并
比较 JAR 路径、SHA-256 与字节内容，证据写入 `.scratch/`。
Rust 版本由 `rust-toolchain.toml` 锁定,依赖由 `Cargo.lock` 锁定。

## Layout

本仓库为单一 monorepo（ADR-0167 决策 5），分**执行核心**与**创作簇**两簇，经强制 seam
（`contracts/*` + 签名 RuleBundle）相接、无源码级耦合。

**执行核心（对外主线）**

- `runtime/` — Rust Runtime(`rf-cli`、`rf-runner`、`gate`、trust、scan-plan)
- `runtime/bundle-cache/` — RuleBundle delivery descriptor、digest cache 与 offline availability seam
- `runtime/bundle-delivery/`、`runtime/rule-bundle/` — 签名 RuleBundle 交付与消费（两簇 seam）
- `runtime/status/` — StatusPublication v1、Gate 三态映射与无凭据 adapter seam
- `runtime/trust-state/` — TrustCheckpoint 的有界持久化、回退保护与可信 handoff 状态边界
- `runtime/git-scope/` — 显式 base/head、merge-base、路径安全与确定性 ChangedSet seam
- `runtime/affected-scope/` — Maven/Gradle 静态图、反向依赖闭包与 PMD/SpotBugs scope seam
- `runtime/scan-plan/` — 版本化 FULL/MODULE/INCREMENTAL_PREVIEW 计划、Completeness、authority 与摘要缓存 seam
- `runtime/incremental-baseline/` — 全量兜底原因、单调时钟阶段遥测、冷/热缓存 P50/P95 与合成基线 evidence seam
- `runtime/project-config/` — 版本化项目配置、只读工作区识别、配置/WorkspaceDescriptor/ScanPlan digest 与 fail-closed doctor seam
- `backend-hosts/{fake,java}`、`workspace-resolvers/*` — Backend Host 与 Workspace Resolver
- `contracts/proto/` — 版本化跨进程契约(Backend Host / Workspace Resolution)
- `contracts/http/` — RuleOps Server 与 RuleBundle delivery 的版本化 HTTP/JSON contract
- `specs/` — 治理资产规范文件

**创作簇（monorepo 内受治理组件）**

- `control-plane/authoring|factory|coverage` — Java 21 JPMS 模块化创作/工厂/覆盖(ADR-0018)，
  产出签名 RuleBundle 经 delivery 契约交给执行核心；分析引擎经跨进程契约接入(ADR-0027)
- `control-plane/ontology|server|verification` — 本体、控制面 Server、验证

## CLI

当前 checkout 的 V0.1 薄 CLI 为 `./rf`，命令按用途分区（ADR-0167）。
Defer 项不列为本页的公开功能推荐；已有工具、内部兼容说明和已登记窗口保持不变。

### 主线（执行层，新手 10 分钟）

```text
./rf init --repo-root PATH [--non-interactive] [OPTIONS]
./rf doctor --repo-root PATH [--format text|json] [--scan-probe COMMAND]
./rf config validate --repo-root PATH [--config FILE]
./rf config show --repo-root PATH [--config FILE] [--format text|json] [OPTIONS]
./rf scan --repo-root PATH (--rule-id ID | --candidate DIR) [--runtime-home DIR] [--format text|json|sarif] [--progress text|json|none] [--output FILE]
```

### 创作 / 工厂（monorepo 内受治理组件，不在执行主线，见 ADR-0167 决策 5）

```text
./rf standard validate FILE...
./rf rule validate FILE...
./rf rule generate INTENT.yaml OUTPUT_DIR
./rf rule test CANDIDATE_DIR OUTPUT_DIR
./rf coverage analyze OUTPUT_DIR
```

### 内部工程与兼容检查（非公开能力承诺）

日常使用不需要进入本区，也不需要填写 Pilot 证据 YAML。规则作者和扫描使用者先看
`docs/pilot/operator-quickstart.md`；下面命令仅供观察员、质量裁决和治理流程使用。

```text
./rf pilot delivery-check OUTPUT_DIR
./rf submission status --server URL (--request-id DIGEST | --artifact-digest DIGEST)
./rf status contract-check OUTPUT_DIR
./rf status deployment-check OUTPUT_DIR
```

`rf init` 只读识别 Java/Maven 或 Gradle 工作区并生成可审阅的
`.ruleops/config.yaml`；`--non-interactive` 遇到不确定识别、未支持语言、路径越界或冲突
配置会 fail closed。`rf config validate/show` 冻结配置来源优先级（CLI > environment >
project > defaults），并输出配置投影与实际 Resolver 描述符的身份、ScanPlan digest。已安装
release 的标准 `rf scan` 不要求拼接内部 Host/Resolver 命令；`--host`/`--resolver` 只保留给
本地 harness，缺少发布资产会返回结构化错误。使用 `--output` 时，结果带有仓库内相对的
`artifactPath`。`rf scan`
要求先有已校验项目配置，Runner 只消费配置冻结的 module/root/include/exclude scope，扫描
结果会携带这些必填身份；Offline/Standalone 没有 Candidate 或 delivery/cache 输入时会
结构化返回 `INCONCLUSIVE`，不会降级执行。`rf doctor` 分别报告依赖、缓存摘要完整性、TrustState 和可选
scan probe，缓存命中仍需重新通过签名、freshness、撤回快照和 execution identity 验证，缺失
或不确定不会降级为可信执行。

开发环境的工具链事实（`java`/`javac`/`cargo`/`mvn`/`protoc`、Maven DNS 与本地缓存）可用只读
诊断脚本复核：

```text
bash scripts/environment-readiness-doctor.sh .scratch/environment-readiness-doctor
```

该脚本只写入指定输出目录中的 JSON、命令记录和 stdout/stderr 日志，不执行构建、不下载依赖、
不修改 DNS/代理或信任状态；报告只使用 `READY`/`BLOCKED`，`gate=NOT_RUN`、`trust=NOT_EVALUATED`。
默认即使 `BLOCKED` 也返回 0 以免阻塞开发；需要把环境事实作为门禁时显式传 `--strict`（返回 3）。

审计决策与证据索引可用 schema-only 的 Roundtable replay pack 复核：

```text
bash scripts/roundtable-replay-check.sh
bash scripts/roundtable-replay-check-test.sh
```

该 checker 只校验三席决策 fixture 的字段、推荐项、阻塞条件、证据来源行号、commit 引用和 authority boundary；fixture 当前为 `PROPOSED`、`LOCAL_UNATTESTED`，不会执行业务命令，也不会产生生产 Gate 或 Trusted 结论。详情见 `docs/research/roundtable-replay-v1.md` 与维护审计文档。

`rf scan` 返回 `0` 表示执行完整（Gate 可为 PASS 或 FAIL），`3` 表示
INCONCLUSIVE/执行不完整，`2` 表示输入或契约错误，`4` 表示 trust failure。

Feature check 的证据报告使用 `contracts/http/feature-evidence-v1.yaml`：结构或关键词校验
只能标为 `STATIC_ONLY`；实际 Cargo/Maven/fixture 行为命令成功才标为 `PASS`；工具不可用、
输入缺失或行为命令失败标为 `BLOCKED` 并保留命令及 stdout/stderr 日志。当前边界检查可用
以下命令单独复核：

```text
bash scripts/feature-evidence-check.sh .scratch/feature-evidence-check
bash scripts/bundle-delivery-check.sh .scratch/bundle-delivery-check
bash scripts/server-contract-check.sh .scratch/server-contract-check
bash scripts/server-persistence-check.sh .scratch/server-persistence-check
bash scripts/thin-client-check.sh .scratch/thin-client-check
```

上列及下列独立调用示例保留原输出路径；实际取证须改传唯一输出路径。
`bash scripts/check.sh` 的最新阶段输出统一在 `.scratch/gate/`，与人工证据区分开；
历史输出不搬、不删，详见 `docs/agents/dev-workflow.md`。

这些证据只证明当前 checkout 的工程行为，不把本地测试升级为 Trusted、生产或正式 Pilot
结论。

扫描结果可用 `--format json` 或 `--format sarif` 消费；JSON `ScanResult` 由真实 Runner
结果投影出稳定 Finding identity/digest、repository-relative path/line/column、Gate 三态、
`LOCAL_UNATTESTED` authority、Completeness、结构化诊断和六阶段进度。`--progress json`
把不含源码的 `ScanProgress` JSONL 写到 stderr；阶段墙钟耗时不进入 `resultDigest`。
启动/协议/信任错误输出闭合 `ScanFailure`，不会把错误文本或源码当作 Finding。

release promote/revoke、gate authority 与 exception approval 会显式拒绝；Thin
Client 的 `feedback draft`/`exception request` 生成 digest-only、可重放申请，
`feedback submit`/`exception submit` 只有在显式提供 Server endpoint 时才发送申请。
提交回执只表示 Server 已接收/重放，不表示批准、Gate PASS 或 SCM 写入；网络未知后可用
`rf submission status` 按 request ID 或 artifact digest 只读查询，`UNKNOWN`/`PENDING` 不自动重试。
`rf status contract-check` 只验证 StatusPublication v1 的 digest、身份、时效与三态映射，
产物交给外部 adapter；本地 Runtime 不持有 SCM 凭据，也不写入分支、检查或受保护状态。
`rf status deployment-check` 冻结外部 Status Adapter 的 HTTPS endpoint、opaque credential
reference、精确 `status:write` scope 与 receipt/error 语义；不连接 provider、不读取 secret、
不产生网络副作用。

`rf pilot observe` 只产生 `SHADOW` digest-only 观测，不写入 SCM；
`local-simulation`、`synthetic-shadow` 和 `live-shadow` 三种模式的 journal
身份与窗口聚合不可混淆。

`rf explain` 只接受闭合的 `FindingView`，输出可导航的相对位置和摘要；动作契约见
`contracts/http/thin-client-v1.yaml`，不包含源码或完整 Finding。提交 transport 只支持
本地 HTTP contract seam；只有 HTTP 400-499 才是确定性拒绝，3xx、5xx、身份缺失、畸形响应
和连接/超时都保持 `SUBMISSION_UNKNOWN`，不生成成功回执、不自动重试；生产 TLS 由部署边界负责。

仅供已登记窗口的观察员与内部演练维护者：见[内部观测运行说明](docs/pilot/observation-implementation.md#运行入口)。

本地模拟只在本地 clone 上追加合成提交，登记窗口观测按登记清单只读获取公共仓库；两者
都输出 digest-only journal 和窗口报告，缺少正式窗口基线或人工审定时报告保持
`INCONCLUSIVE`。

仅供参加正式 Pilot 作者成本研究的角色：见[内部作者成本基线说明](docs/pilot/author-cost-baseline.md#复跑)。

缺少真实作者工时的样本保持 `INCONCLUSIVE`，机器编译/验证耗时不替代作者主动时间。

Shadow MVP 交付预检与最终门禁：

```text
./rf pilot delivery-check .scratch/pilot-delivery-check
bash scripts/check.sh
```

TrustCheckpoint 的首次安装、跨重启推进和 TrustedContext 接入可单独验收：

```text
bash scripts/trust-checkpoint-check.sh .scratch/trust-checkpoint-check
```

可信扫描只能消费已经持久化的 checkpoint；状态缺失或不确定不会被自动 bootstrap 成可信结果。

交付清单见 `specs/pilot/pilot-release-manifest.yaml` 与
`docs/pilot/release-candidate.md`。当前版本可进入登记的 Shadow 观察窗口，
但不授予生产强制执行或受保护分支权力。

工程中心闭环只保留作内部验收，不列为当前公开能力承诺；
不改变既有商定范围，工程验收仍不等于生产强制执行已经启用。
