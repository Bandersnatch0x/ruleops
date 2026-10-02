# ADR-0166: IDE Thin Client 适配边界与双端离线交付

- Status: Accepted
- Date: 2026-09-24
- Scope: IntelliJ / VS Code IDE 薄客户端（`clients/intellij`, `clients/vscode`）
- Decision record: GitHub issue #101

## Context

`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md` 的 V0.3 层要求
把既有的 digest-only Thin Client action 契约兑现为真实的 IDE 使用体验。契约侧此前已冻结：
`contracts/http/thin-client-v1.yaml` 声明 `nonGoals: idePluginUi`，ADR-0140 冻结了
digest-only 的 IDE action 契约，ADR-0164 冻结 Rich Explain 与 authority-status 边界，
ADR-0165/#121 冻结 governance `requestId` 的内容寻址。运行时 `erf-thin-client`
（`runtime/thin-client`）已实现 `FindingView` 闭世界解析、`validate_relative_path`
路径安全、feedback/exception 请求构造、向 Server 提交与 `SUBMISSION_UNKNOWN` 恢复，
并由 `rf` 启动器以空格分隔子命令暴露（`rf explain` / `rf feedback draft` /
`rf exception request` / `rf feedback submit` / `rf exception submit` /
`rf submission status`）。

缺口是纯人机层：真实 IDE 里没有可展示/导航/解释 Finding、起草 Feedback、发起 Exception
的宿主。治理负责人已裁决（#101，三项取值）：目标 IDE = **IntelliJ 与 VS Code 两端**；
插件仓库/构建方式 = **离线**；其余按推荐。据此需要确定：IDE 客户端能持有什么、
不能持有什么，以及在无联网、无 IDE SDK 下如何让交付门禁给出 EXECUTABLE 证据。

## Decision

**1. IDE 客户端是纯 UX 适配器，零新增权威。** 两端插件只做：以 digest-only 姿态
展示 `FindingView`、经路径安全门禁后导航到位置、调用 `rf explain` 只读呈现解释、
起草 Feedback（`PROPOSED`）、发起 Exception 申请（`REQUESTED`）、可选向显式配置的
Server 提交（回执仅 `RECEIVED_ONLY`）、查询提交状态。客户端**绝不**复制
Runtime/Factory/Server/Registry/SCM/Gate 权威，绝不批准 Exception、改写 Gate、写 SCM，
绝不把提交回执渲染成批准或 Gate PASS。这延续 ADR-0140 的 action 契约，仅补齐其 UI 宿主，
因此契约的 `nonGoals: idePluginUi` 从"暂不实现 UI"收敛为"UI 只作适配、无独立权威"。

**2. 架构强制"薄绑定层 + 零依赖纯逻辑核"两分。** 每端的所有可判定适配逻辑
（rf 参数构造、`explain` 文本解析、退出码语义 0/2/3、路径安全门禁、`FindingView`
闭世界与隐私 `FORBIDDEN_FIELDS` 递归拒绝、`SubmissionReceipt` 三条虚假权威守卫
`exceptionApproved`/`gateMutated`/`scmPublished` 只读渲染）落在一个**无 IDE SDK、
无网络依赖**的纯核里：VS Code 端 `clients/vscode/src/core.js`，IntelliJ 端
`clients/intellij/src/main/java/io/ruleops/ide/core/ThinClientCore.java`。IDE 绑定层
（`extension.js` / `AnAction`）只把用户操作转发给纯核与 `rf`。纯核语义两端逐条镜像
`erf-thin-client`，且两端彼此镜像。

**3. 离线交付：门禁验 EXECUTABLE 纯核，完整插件构建归人工。** 交付门禁
`scripts/ide-client-check.sh` 在无联网、无 IDE SDK 下给出 EXECUTABLE 证据：
(a) 真实运行 `rf explain` 于一份 digest-only Finding；(b) 用 `node --test` 跑 VS Code
纯核单测、用 plain `javac`+`java` 跑 IntelliJ 纯核自测；(c) 令两端 parity harness 消费
**同一份真实 rf 输出**并 `diff` 其规范化输出，证明两端对相同输入产出 byte-identical
的参数向量、解析结果与门禁判定。完整 VSIX 打包（`@vscode/vsce`）、Gradle IntelliJ 平台
插件构建、交互式真实 IDE 验收、插件仓库维护属**人工离线最后一步**，因其需要离线镜像下
解析 IDE SDK/工具链、并需人在真实 IDE 内操作，代理无法在此环境伪造这些证据。

**4. 隐私与安全默认关闭。** 遥测默认 `false`；提交/状态查询要求显式 `serverEndpoint`
（禁用自动发现）；导航在纯核路径门禁之外，绑定层再确认解析后的绝对路径仍落在项目根内，
任一不过即拒绝。

## Consequences

- V0.3 IDE 缺口以真实、可离线单测的两端客户端兑现，且不引入任何新权威面，审计边界不变。
- 门禁保持离线可跑（只需 `node` 与 `javac`，均在位），并以跨端 `diff` 把"两端行为一致"
  变成可执行证据，而非声明。
- 完整插件产物与真实 IDE 验收显式留给人工离线步骤；代理不伪造 VSIX/Gradle 构建或
  交互验收证据。
- 新增顶层 `clients/`（`clients/intellij`、`clients/vscode`）与 `backend-hosts/`、
  `workspace-resolvers/` 同为组件族目录，不违反"仓库根级不新增杂项目录"。

## References

- ADR-0140 digest-only IDE action 契约
- ADR-0164 Rich Explain 与 authority-status 边界
- ADR-0165 / #121 governance requestId 内容寻址
- `contracts/http/thin-client-v1.yaml`（`nonGoals: idePluginUi`）
- `runtime/thin-client`（`erf-thin-client`）、`rf` 启动器
- V1.2 Final 架构计划 §29.1 / §40.3 / §48 / §49 / §51
