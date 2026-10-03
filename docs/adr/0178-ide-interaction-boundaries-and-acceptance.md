# ADR-0178: IDE 交互边界与验收口径

- Status: Accepted
- Date: 2026-10-03
- Approval source: 本次阶段 0 任务明确用户已采纳 D1–D6 全部推荐；[GitHub #139](https://github.com/Bandersnatch0x/ruleops/issues/139)「已裁定」逐项记录同一结论。
- Scope: 冻结两端 IDE 的交互与验收边界，仅新增 UI 元数据和说明；不改变产品逻辑、既有 ADR、契约语义或阈值。
- Baseline: `master` / `0b4cb358a331c5850f3a0afae099c09d8d1a4280`。

## 背景

[交互评审](../../.scratch/ide-interaction-review-20261003.md) §4 按真实 `./rf --help` 列出 31 条命令模板，§6 阶段 0 要求先冻结边界再实施侧栏/工具窗。
现有 IDE 独立动作并不等于完整 CLI 覆盖；registry 目录与版本核对也不等于获得可扫描包。CLI/IDE 仍是 ThinClient，执行与可信裁决归 Runner、Server 及既有治理流程。
评审 §7 的 D1–D6 已由用户裁定，本 ADR 将结论转为持久约束，不将批准设计写成已实现能力。
本次指定 WSL 工作树未找到 `.scratch/logs/ide-refactor-ticket.md`，批准来源以任务指令及只读核实的 #139 为准，不补造缺失日志。

## 决策

### D1：拉取完成物是可扫描签名包

最终完成定义为获得可供既有 Runner 路径消费的**可扫描签名包**，优先复用既有 delivery/cache；目录浏览、版本选择、PIN_MATCH 都只是前置步骤。
“同步最新”只用于发现候选版本，不把 `latest` 或可变 tag 当执行身份；执行仍须绑定精确摘要、descriptor/ExecutionLock 并完成既有验证。
需要可编辑规则正文时另立导出契约；本次不新增 `pull` 命令，也不假定 registry 元数据带有规则正文或 bundle 字节。

### D2：31 项全部可发现，常用优先，高级/维护显式受限

两端共用 [ide-command-map.json](../../clients/ide-command-map.json) 的 UI 元数据。按帮助顺序保留 31 条模板；registry 的 `rules|standards` 是资源选择，不能拆成额外命令。帮助入口不计业务命令。
按工作区、规则库、扫描与 Findings、反馈与例外、创作与覆盖、试点与治理、运维状态分区，优先呈现常用任务，避免按钮墙。
“可发现”不等于“本安装可执行”：高级/维护项显示所需安装资产、维护环境及能力不足原因，不隐藏条目，不在 UI 自创角色或权限，也不自动构建缺失资产。
可执行覆盖须能从工具窗直接打开承接所选对象的参数面板；raw-args 或复制命令仅为高级补充。

### D3：远程安全路径在 CLI 层统一实现

两端只消费 CLI 暴露的经批准远程安全路径，不分别实现网络、鉴权或安全绕行。
真实 endpoint、服务环境、TLS/企业 CA、鉴权/代理/隧道方案及连接授权必须来自负责人批准的配置；本 ADR 不提供这些事实。
不绕过当前 loopback 限制、不以明文公网作为替代、不自行建隧道。现有 seam 不支持的连接显式不可用；后续实现及验证归 #139 阶段 3。

### D4：独立 pin 与 trust root 来自发布/交付系统

独立 expected-digest、rule→bundle 映射、BundleDeliveryDescriptor、ExecutionLock、外部 trust root，以及撤销/兼容资料，由已有发布/交付系统提供并展示 provenance。
不能从同一次待验证响应自填可信 pin 或 trust root，不能把 registry store digest 当 RuleBundle digest，也不能因目录核对成功或缓存命中跳过签名/绑定验证。
来源缺失时明确显示缺项，不能标为已同步可执行；不把演示资产视为生产发布资产。

### D5：身份先确认，例外 scope 默认当前文件

actor/OWNER、理由、观测信号与时间须由真实来源提供并经操作者确认，不用 git 用户名、默认字符串或当前时间猜测治理事实。
例外申请默认范围是当前选中 Finding 对应的仓库相对文件；选项由实际 workspace/Finding 生成。缺少文件上下文时须选择对象，不回退为整个仓库。
扩大到目录/module 等范围前展示准确范围并显式确认；UI 不改变 CLI 的既有 scope 语义。
保存草稿、提交请求、收到回执与批准分别显示；提交不等于批准，CLI 成功退出不等于 Gate PASS。

### D6：支持矩阵按真实证据建立

以现有源码声明为候选范围，先明确实际支持矩阵再声称兼容；声明本身不是兼容或安装验收。

| Client | 声明基线 | 运行 OS / 真实 IDE 版本 | 打包与生产能力 | 验收状态 / 负责人 |
| --- | --- | --- | --- | --- |
| VS Code | `engines.vscode: ^1.85.0`，见 `clients/vscode/package.json` | 待提供实际组合，不预承诺任意 OS/版本 | 离线依赖缓存、`@vscode/vsce`、VSIX 安装及安装包 CLI 能力待核实 | 未验证；发布/验收负责人待指定 |
| IntelliJ | `2024.1` / IC、Java 21、sinceBuild `241`；untilBuild 留空不代表无限兼容，见 `clients/intellij/build.gradle.kts` | 待提供实际组合，不预承诺任意 OS/版本 | 离线 SDK/Gradle、插件包安装及安装包 CLI 能力待核实 | 未验证；发布/验收负责人待指定 |

每个实际矩阵行须记录 IDE 精确版本、OS/架构、运行环境、插件与 rf 包版本/摘要、依赖资产、可用命令范围、安装与交互证据、验收人和日期。上述待提供项是后续实施前置条件，不是未决设计选项，不由代理代填。

## 后果与验收

- 收益：两端共享一个 UI 命令清单并遵循同一安全/身份边界；不复制业务逻辑或授予治理权。成本：真实远程路径、发布 provenance、安装能力及宿主兼容证据仍须逐项准备。
- 元数据中的 commandId 是共享清单标识，不是已注册的 IDE 动作；commandTemplate 原样记录帮助文本，fieldSources 记录来源而非完整参数 schema，outputTypes 是展示类别而非新 wire contract。客户端不得把元数据直接拼接成 shell 命令或当权限/验证权威。
- 根 `rf` 帮助不等于全部 parser 细节。例如 evidence package/adjudicate 的 parser 要求 `--contract-digest`、`--sample-digest`，OWNER 由 `--owner` 解析；清单保留现有帮助，阶段 4 逐项核实参数，不在阶段 0 修订语义或伪造参数。
- 阶段 0 验收：JSON 的 31 项与真实 `./rf --help` 按顺序精确一致，每项有命令 ID、分区、字段来源、副作用和输出类型；契约只加注释；Git diff 只含本 ADR、共享清单与必要契约说明。
- 副作用描述是已查证的 UI 提示下限，不宣称脚本纯只读或完整审计；执行/写入/网络前应预览，尚未核实的维护环境能力须明确显示。后续阶段需按 parser、安装产物及真实运行证据补全，不把“可发现”报成“可执行”。
- 后续实施统一跟踪 [#139](https://github.com/Bandersnatch0x/ruleops/issues/139)：阶段 1 容器与执行状态，阶段 2 动态选项及申请闭环，阶段 3 远程规则路径，阶段 4 全命令覆盖，阶段 5 打包及真实 IDE 验收。
- 本次依任务限制仅做定向静态核对，未运行全量门禁、产品扫描、远程拉取或真实 IDE 验收；不声称侧栏、工具窗、动态下拉、31 项可执行覆盖或远程同步已经实现。后续实现交付仍按项目工作流执行门禁与验收。
