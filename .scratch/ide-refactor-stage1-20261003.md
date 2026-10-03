# IDE 重优化阶段 1：原生入口与统一执行状态

- 日期：2026-10-03；依据：交互评审 §5.1 / §6 阶段 1、ADR-0178、共享命令映射与 #139 的本次委派边界。
- 工作树：`kali-linux2:/home/kali/文档/ruleops`，未使用 `/mnt/d`。
- 基线：`master acc24b7`；分支：`codex/ide-refactor-stage1-20261003`；本报告与实现同提交。
- 范围：阶段 1 源码与定向测试；没有 push、票据写入、全量门禁或真实 IDE 安装验收。

## 实现与不变量

**不变量：原有八个命令/action 只选择工具窗场景，执行只发生在一个宿主执行入口，argv 与业务语义仍由既有 core/CLI 定义。**

| 位置 | 改动与证据 |
| --- | --- |
| `clients/vscode/package.json` | 注册 Activity Bar ERF `viewsContainers`、`erf.scenarios` TreeView、`erf.details` WebviewView 和刷新/设置/取消菜单；新增 prepublish 资源准备。 |
| `clients/vscode/src/extension.js:6` | 遍历同一 `SCENARIOS` 为八个旧命令注册 `sidebar.open(scene.id)`；删除旧弹窗/独立执行流程。 |
| `clients/vscode/src/sidebar.js` | 原生七分区 + 会话内运行记录；八场景表单、显式多根工作区选择、未配置/不可用/运行/失败/取消/完成；命令与树共享 `open → execute → invoke`。 |
| `clients/vscode/src/rf-invoker.js` | 唯一 rf 子进程适配器；argv 数组、无 shell、超时、1 MiB 单流输出上限、AbortSignal、进程树终止、可见不可用原因。 |
| `clients/vscode/scripts/prepare-resources.js` | prepublish 从 `clients/ide-command-map.json` 生成插件资源；生成件忽略，不创建第二份手写命令清单。 |
| `clients/intellij/src/main/resources/META-INF/plugin.xml` | 注册 ERF toolWindow 与项目设置；服务使用既有 `@Service(PROJECT)`，移除重复 XML 注册。 |
| `clients/intellij/src/main/java/io/ruleops/ide/ErfToolWindowFactory.java` | Swing/JTree 原生表单、共享分区、统一 `Panel.execute → RfInvoker.submit → perform → RfInvoker.run`，不执行未接入命令模板。 |
| `clients/intellij/src/main/java/io/ruleops/ide/actions/` | 八个 action 仅以 `super(Scene.*)` 委派，基类 final `actionPerformed` 只打开同一工具窗；删除 `SubmitActionBase.java` 的旧提交弹窗流程。 |
| `clients/intellij/src/main/java/io/ruleops/ide/RfInvoker.java` | `Task.Backgroundable` 后台执行，指标取消触发进程树终止；回调使用 Task 的 EDT `onSuccess/onCancel`，显式断言线程；进程读取/等待拒绝在 EDT 运行。 |
| `clients/intellij/src/main/java/io/ruleops/ide/ErfSettings.java` / `ErfConfigurable.java` | 项目设置表单与执行配置快照；修改设置取消在途请求并废弃旧结果。 |
| `clients/intellij/build.gradle.kts` | `processResources` 打包同一份共享映射，无新外部依赖。 |

两端全部 31 条 CLI 元数据可发现（另有显示/导航两条本地场景）；只有原有八场景可执行，其他项显式显示“阶段 1 尚无参数面板”。可发现不等于完整 CLI 覆盖或该安装可执行。

## 安全与范围复核

- 两端 core 与 VS Code `path-safety.js` 与基线逐字节相同，SHA-256 证据见 `core-preservation.txt`。
- Finding 先过闭世界/隐私字段拒绝，再展示/构造请求；导航保留词法、项目根、符号链接组件与真实路径门禁。
- 退出码 2/3 仍分类为输入非法/拒绝或待定，不呈现业务成功；非 JSON 回执显式失败，不再静默吞解析错误。
- 提交与状态查询均经过既有回执权威守卫；“已完成”只指本次客户端执行，不授予例外批准、Gate PASS 或 SCM 写入。
- Webview CSP 为 `default-src 'none'` + 独立 nonce；无外部/本地资源许可；文本转义，消息动作与字段逐场景白名单校验，不接收命令、文件路径或任意 argv。
- 工作区/设置变更废弃旧执行结果；运行中阻止重复执行/场景切换，取消明确提示远端请求可能已接收，不能声称回滚。
- actor/理由由操作者填写；例外 scope 限已验证 Finding 的当前相对文件，不默认全仓库、不提供扩范围或自动身份。
- 没有新增批准例外、Gate、SCM 写入、鉴权/网络绕行、缓存权威或后端业务逻辑；未引入 React/重型前端依赖。

## 定向验收

证据目录：`.scratch/ide-stage1-20261003-evidence/`（独立目录，未写 `.scratch/gate/`）。

| 检查 | 结果 / 证据 |
| --- | --- |
| `timeout 60 node --test clients/vscode/test/*.test.js` | **30/30 PASS**：既有 18 个纯核 + 12 个新增测试；`vscode-tests.log`。 |
| VS Code 宿主 API 替身测试 | 实际 activate 八命令转发、原生分区/菜单、31 清单发现、CSP、消息拒绝、工作区选择、状态迁移、取消/重复执行/旧结果隔离、隐私与回执拒绝、导航符号链接门禁、表单保留/上下文清理。不是真实 VS Code。 |
| VS Code 进程适配器 | 真实 Node 子进程验证 argv 保真、缺失二进制、超时、输出上限与取消；不需要 Server 或业务写操作。 |
| Java 纯核 javac/java | JDK `/home/kali/.local/opt/jdk-21.0.5+11`，**44 PASS / 0 FAIL**；`intellij-core.log`。 |
| IntelliJ 源码/资源约束检查 | Node 测试核对 toolWindow/settings 注册、八 action 委派和后台/EDT 代码入口；只属于源码证据，不冒充宿主测试。 |
| IntelliJ SDK 宿主测试 | 已新增 `clients/intellij/src/test/java/io/ruleops/ide/ErfHostTest.java`：原生面板/打包描述符、后台真实 Java 子进程、EDT 完成回调与取消。**未运行成功、未完成插件编译**。 |
| 离线 SDK 尝试 | 初次 Gradle daemon 启动超时；`--offline --no-daemon -Dorg.gradle.jvmargs= test` 复试明确失败：离线无法解析 `org.jetbrains.intellij:1.17.4` 插件。见 `intellij-host.log` 与 `intellij-host-retry.log`；没有联网安装或用纯核冒充 SDK 通过。 |
| 差异复核 | `git diff --check` 通过；检查无旧命令独立弹窗/第二套 rf 调用、无 core 扩权、无静默 JSON 回执降级。 |

Java 纯核复跑（每条限时 60 秒）：

```bash
JDK=/home/kali/.local/opt/jdk-21.0.5+11
OUT=.scratch/ide-stage1-20261003-evidence/classes
mkdir -p "$OUT"
timeout 60 "$JDK/bin/javac" -d "$OUT" clients/intellij/src/main/java/io/ruleops/ide/core/ThinClientCore.java clients/intellij/selftest/io/ruleops/ide/core/CoreSelfTest.java
timeout 60 "$JDK/bin/java" -cp "$OUT" io.ruleops.ide.core.CoreSelfTest
```

## 剩余边界

1. 需匹配的离线 Gradle 插件和 IntelliJ SDK 才能编译完整插件并运行 `ErfHostTest`；当前没有已证实的 SDK API/运行兼容结论。
2. 没有 VSIX/IntelliJ 安装、截图、真实 IDE 手工验收；Windows 进程取消路径仅源码实现，未在 Windows IDE 内验证。
3. 运行记录仅会话内最近 20 条状态摘要；动态下拉/草稿落盘到提交交接/回执索引、远程资源交付与其余命令参数面板仍属后续阶段，不在本次伪造完成。
4. 按本次指令未跑 `scripts/check.sh`，因此不宣称总门禁或最终产品交付验收全绿。
