# IDE Thin Client 真实 IDE 人工验收清单（#139 阶段 5）

适用分支：`codex/ide-refactor-stage5-20261003`（基于 master 2a2bc1c）。
两端：VS Code 扩展（VSIX 需按 `clients/vscode/README.md` 复现步骤打包安装）；IntelliJ 插件（Gradle 离线镜像解析后 `buildPlugin` → Install Plugin from Disk…）。

**状态图例**：`离线已测` = 本分支定向测试/parity harness 已建立可复现证据；`待人工` = 只能在真实 IDE 中验证，离线测试不能替代。任何一项未通过人工执行前，不得声称“真实 IDE 验收完成”。

**通用前置**：

1. rf 启动器可用且指向受控工作区（含 `.ruleops/config.yaml`）。
2. VS Code 端 `erf.rfPath` / `erf.serverEndpoint` / `erf.connectionProfile` 按场景配置；IntelliJ 端 Settings → ERF 等价配置。
3. 准备 digest-only FindingView JSON 样例（可复用 `scripts/ide-client-check.sh` 生成的 finding 夹具形状）。

---

## 已真机验证的核心流程（2026-10-05）

截止 2026-10-05，两端**核心 happy path** 已在真机人工验收，证据记录于 #139（comment-5986528053 等）与提交 `709c44b`。清单 24 项为负例/边界聚焦，**任何一项未在下方列出前不得视为已真机验收**。

| 端 | 核心流程 | 结果 | 真机环境 |
|----|----------|------|----------|
| IntelliJ | 展示 Finding（SHOW 场景，扫描 Findings 完整字段渲染） | PASS | IDEA 2026.2，`io.ruleops.ide` 插件 Gradle 8.10.2 `buildPlugin` → Install Plugin from Disk，日志确认加载 |
| IntelliJ | 导航到位置（NAVIGATE） | PASS | 打开 `CliScanSample.java:1:1` |
| IntelliJ | ERF 工具窗统一渲染（SCAN 工作台表单+argv+执行） | PASS | 工具窗 Find Action 第 2 项（第 1 项为菜单组） |
| VS Code | 显示 Finding | PASS | Remote-WSL extension host，digest-only FindingView |
| VS Code | 跳转到 Finding（NAVIGATE） | PASS | 编辑器打开目标文件，窗口标题显示 `CliScanSample.java` |
| VS Code | 扫描工作台（SCAN 表单 → rf 执行 → 结果渲染） | PASS | ScanFailure exitCode=2 fail-closed 展示 |

真机环境备注（踩坑记录，供后续人工项参考）：

- IntelliJ 构建需 Gradle **8.10.2**（系统 Gradle 9.0.0 与 `org.jetbrains.intellij:1.17.4` 不兼容引发 `DefaultArtifactPublicationSet` 缺失）。
- Windows 前台锁：后台进程 `SetForegroundWindow` 无效，需 `AttachThreadInput` 绕过（`foreground.ps1`）。
- Find Action 第一项是 ERF 菜单组（非工具窗），需 Down 一次再 Enter。

---

## A. 工作区与配置

| # | 场景 | 预期行为 | 复现方法 | 状态 |
|---|------|----------|----------|------|
| A1 | 多 workspace（multi-root） | 不默认取第一项；打开场景显示“未选择工作区”，须显式选择后进入 ready | 两个目录组成 multi-root 打开；打开“解释 Finding”观察状态；点“选择工作区” | 离线已测（sidebar.test.js multi-root 断言）/ **待人工：真实 VS Code multi-root UI 行为** |
| A2 | 配置改变（endpoint/profile/rfPath） | 运行中配置变化使旧结果失效、清空 registry pin/请求选择；不静默沿用旧值 | 执行 registry list 后改 `erf.connectionProfile`，确认 pin 面板清空、须重新载入 | 离线已测（registry.test.js、sidebar 配置失效测试）/ **待人工：真实 IDE 设置面板触发** |
| A3 | 规则版本删除/切换 pin 后旧选择 | 切换或重载发布记录后旧 pin 选择失效，不自动替换 | 载入 pins JSON → 选择第 2 条 → 改选/重载文件 → 确认旧选择不复用 | 离线已测（descriptor/pin 配对清除测试）/ **待人工：真实文件重载** |
| A4 | 空列表（registry 无结果） | 显示空 items，无崩溃；`下一页` 按钮禁用（hasMore=false） | 连接测试 endpoint 返回空页，观察列表渲染与翻页按钮 | 离线已测（分页 hasMore=false 测试）/ **待人工：真实 registry 服务返回空页** |

## B. 网络与传输

| # | 场景 | 预期行为 | 复现方法 | 状态 |
|---|------|----------|----------|------|
| B1 | 断网（server endpoint 不可达） | 提交/状态查询显式失败（超时/连接拒绝），显示 UNKNOWN/PENDING 语义，不当作批准，不自动重试 | 配置不可达 endpoint，执行提交，观察失败详情与“远端副作用未知，必要时查询回执”提示 | 离线已测（timeout/refusal 分类测试）/ **待人工：真实断网下进程行为与 UI 呈现** |
| B2 | 不支持 HTTPS/非法 endpoint | 构造阶段即拒绝（userinfo/query/hash/非 http(s)），不发起请求 | 尝试 `https://user:pass@host/v1` 等 endpoint，确认被拒 | 离线已测（两端 endpoint 负例）/ **待人工：真实 IDE 设置输入路径** |
| B3 | 鉴权失败（registry 401/403） | CLI 返回非零 → 显示 UNKNOWN · 未安装 · exit=N，不回退 endpoint | 用需鉴权的真实 registry/profile，观察失败展示 | **INCONCLUSIVE**：离线无法建立真实鉴权服务；属 CLI/运维职责，端内仅透传失败 |
| B4 | registry profile 失败不回退 | profile FILE 指定后不 fallback 到 server endpoint | 同时可配置两者，确认 EXACTLY_ONE 拒绝；profile 损坏时显式失败 | 离线已测（EXACTLY_ONE 负例）/ **待人工：真实 profile 文件损坏场景** |

## C. Registry 分页与独立 pin

| # | 场景 | 预期行为 | 复现方法 | 状态 |
|---|------|----------|----------|------|
| C1 | NUL cursor 多页翻页 | `--cursor-json` 单 argv 携带 JSON 编码 cursor（NUL/引号/反斜杠转义），argv 无裸控制字符；回执 cursor 原样往返 | 连接含特殊字符 cursor 的 registry，翻页并检查 rf 进程 argv（如 `ps` 或 CLI 日志） | 离线已测（两端 cursor JSON + 真实进程 argv 测试）/ **待人工：真实 registry 返回此类 cursor** |
| C2 | 多页浏览（>1 页结果） | 第一页渲染 items；`下一页` 仅在 hasMore=true 可用；页码往返不串页 | limit=10 浏览 >10 条的 registry，连续翻页 | 离线已测（分页往返测试）/ **待人工：真实 registry 多页数据** |
| C3 | PIN mismatch（digest 不符） | `REGISTRY_DIGEST_MISMATCH · UNKNOWN · 未安装`，失败展示，不安装不回退 | 载入 pins JSON，将 expectedDigest 改为错误值后执行 show/sync | 离线已测（mismatch 负例）/ **待人工：真实发布资产** |
| C4 | PIN identity mismatch（version 不符） | `REGISTRY_IDENTITY_MISMATCH`，失败展示 | show 响应版本与 pin 不一致（模拟：CLI 侧返回不同 version） | 离线已测 / **待人工：真实 CLI 响应** |
| C5 | 缺独立 pin 执行 show/sync | `INDEPENDENT_PIN_REQUIRED`，不调用 rf | 未载入发布记录直接执行核对/缓存 | 离线已测（两端负例）/ **待人工：确认真实 IDE 提示可读** |

## D. 提交与状态查询

| # | 场景 | 预期行为 | 复现方法 | 状态 |
|---|------|----------|----------|------|
| D1 | 取消长任务 | 状态转“已取消”，提示“远端副作用未知，必要时查询回执”；重复执行被阻止 | 发起提交后点取消（或 erf.cancel），观察状态与不可重复调用 | 离线已测（cancel/重复调用测试）/ **待人工：真实进程取消（VS Code 子进程终止 / IntelliJ Task.Backgroundable checkCanceled）** |
| D2 | 重启后恢复 | 已保存请求索引按 workspace+endpoint 持久；重启后提交/状态查询可继续引用同一对象 | 保存草稿 → 提交 → 重启 IDE → 状态查询引用该对象 | 离线部分已测（workspaceState 持久替身）/ **待人工：真实 IDE 重启** |
| D3 | UNKNOWN 回执 | 渲染“结果未知（UNKNOWN，不作成功处理）”，不显示成功 | 状态查询返回 UNKNOWN（server 延迟持久化），观察标签 | 离线已测（UNKNOWN 回执负例）/ **待人工：真实 server 返回 UNKNOWN** |
| D4 | PENDING/RECEIVED_ONLY 状态 | 对应标签正确，authority 守卫拒绝矛盾值 | 状态查询返回 PENDING/RECEIVED_ONLY | 离线已测 / **待人工：真实 server 各状态** |

## E. 路径安全与写文件

| # | 场景 | 预期行为 | 复现方法 | 状态 |
|---|------|----------|----------|------|
| E1 | 恶意路径（`..\`、绝对、控制字符、symlink） | FindingView location 或输出路径全部拒绝：ABSOLUTE_PATH/DOT_SEGMENT/CONTROL_CHAR/SYMLINK_COMPONENT | 构造含上述路径的 FindingView；导航与保存场景分别尝试 | 离线已测（两端路径负例 + symlink 测试）；正常导航路径真机 PASS（2026-10-05，见上表）；**恶意路径负例待人工：真实 IDE 导航行为** |
| E2 | 写文件失败（权限/占用） | 失败展示，不产出请求索引，表单保留原输入 | 目标目录只读后执行保存 | 离线已测（exit=2 保留表单测试）/ **待人工：真实文件系统权限** |
| E3 | 输出覆盖已存在文件 | 显式确认对话框；覆盖 Finding/输入文件被拒（OUTPUT_OVERWRITES_INPUT） | 保存到已有文件；保存到 Finding 自身路径 | 离线已测 / **待人工：真实保存对话框** |

## F. Schema 与异常输出

| # | 场景 | 预期行为 | 复现方法 | 状态 |
|---|------|----------|----------|------|
| F1 | schema 非法 FindingView/回执/scan 输出 | fail closed：INVALID_RECEIPT/SCAN_KIND_UNKNOWN 等，绝不渲染成功/PASS | 喂入缺字段/错 kind/未知 status 的 JSON | 离线已测（两端 schema 负例）/ **待人工：真实异常响应** |
| F2 | rf 缺失/不可执行 | unavailable 状态，显式提示，不静默 | 清空 erf.rfPath 或指向不存在二进制 | 离线已测（UNAVAILABLE 测试）/ **待人工** |

## G. 可访问性与外观（纯人工）

| # | 场景 | 预期行为 | 复现方法 | 状态 |
|---|------|----------|----------|------|
| G1 | 键盘导航 | 表单控件/按钮 Tab 顺序合理，submit/cancel 可键盘触达，focus-visible 外框可见 | 纯键盘走完一次 explain→draft 流程 | **待人工** |
| G2 | 主题（亮/暗/高对比） | webview 使用 `--vscode-*` 变量，两主题下文本可读、无硬编码颜色 | 切换主题观察面板 | 离线已测（CSS 变量声明）/ **待人工：真实渲染** |
| G3 | 缩放（Zoom In/Out） | webview 随窗口缩放不溢出、不截断 | Ctrl+= / Ctrl+- 后检查面板 | **待人工** |
| G4 | 屏幕阅读器（aria-live） | 状态区 `role="status"` + `aria-live="polite"` 播报阶段变化 | 开启屏幕阅读器执行一次场景 | **待人工** |

---

## INCONCLUSIVE 项汇总

| 项 | 原因 |
|----|------|
| B3 鉴权失败 | 离线无法建立真实鉴权 registry；失败透传逻辑有 CLI 契约背书，端内不伪造 |
| C1-C5 registry 分页/独立 pin（真实服务语义） | 需真实 registry server 返回多页/特殊 cursor/发布资产；当前无可用测试服务 |
| D3/D4 真实回执状态 | 需真实 server/CLI 返回 UNKNOWN/PENDING/RECEIVED_ONLY |
| SDK 宿主测试（ErfHostTest） | 需真实 IntelliJ SDK test application + Gradle 环境 |
| 清单 24 项中其余全部 `待人工` | 真实 IDE 交互（multi-root、断网、取消、重启、保存对话框、主题缩放、屏幕阅读器等）无法离线表达，且截至 2026-10-05 未在真机逐个执行 |

**已解决（原 INCONCLUSIVE）**：

- `org.jetbrains.intellij:1.17.4` **离线不可解析** → 已用本机 Gradle 8.10.2 wrapper 缓存解析并 `buildPlugin` 成功；插件已真机安装加载（IDEA 2026.2）。

## 已知跨端分歧（不影响 parity harness）

- status 查询 authority 校验：Java `STATUS_AUTHORITY_GUARD` 强制 UNKNOWN→UNKNOWN；JS 允许 {UNKNOWN, RECEIVED_ONLY}。两端渲染各自实现，不进 harness。
- registry show 的 standards pin：Java 核允许（registryResource 接受 standards）；VS Code `selectRegistryPin` 按资源过滤下拉。UI 层均已限定，核层语义差异仅备查。
