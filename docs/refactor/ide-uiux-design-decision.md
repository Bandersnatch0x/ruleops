# IDE 双端设计验收与最小改造决策

- date: 2026-10-05
- baseline: `2c8914a07872ddec6b850bec3393457660f6f46e`
- scope: `clients/intellij/`、`clients/vscode/` 与本报告；所有手工编辑经 WSL UNC 路径，未改 D: 工作树。
- design_verdict: **Recirculate（示意稿可参考，不通过“原生、高保真、全场景、全状态、无障碍已达标”的整体声明）**。
- implementation_decision: **保留现有薄客户端；只增加原生场景树检索、主题化错误边框，不重做工作台。**
- approval: 协调者对 D1/D2/D3 裁决批次明确回复“批准最小范围”；不新增 ADR、不改共享命令契约。此报告记录本次局部 UI 裁决，不替代 ADR-0166。

## 1. 输入、方法与证据边界

已逐幅查看以下 8 个图片文件，并读取对应 HTML 与 `.scratch/ide-uiux-design/design-spec.md`。原图、设计规范、原评审 `.scratch/ide-uiux-review/review.md` 和修复报告 `.scratch/ide-uiux-fixes/report.md` 均只读。对照基线源码、共享命令表、ADR-0166 和 `docs/refactor/ide-thin-client-real-ide-acceptance-checklist.md`，不把 HTML 注释中的平台类名当作实际组件。

资产前缀均为 `.scratch/ide-uiux-design/assets/`：

| ID | 文件名（图片与同名 HTML） | 实际图片证据 |
|---|---|---|
| V1 | `vscode_01_treeview_commands.png` | **实际为 JPEG**，512×410，扩展名与内容不符 |
| V2 | `vscode_02_scan_workbench.png` | PNG，512×410 |
| V3 | `vscode_03_governance_feedback_exception.png` | PNG，512×487 |
| V4 | `vscode_04_registry_server_submissions.png` | PNG，512×410 |
| I1 | `intellij_01_toolwindow_scenetree.png` | PNG，512×410 |
| I2 | `intellij_02_scan_workbench.png` | PNG，512×410 |
| I3 | `intellij_03_exceptions_feedback.png` | PNG，512×410 |
| I4 | `intellij_04_settings_formbuilder.png` | PNG，512×410 |

机器审计摘要在 `.scratch/ide-uiux-decision/asset-audit.json`；原件与共享命令表 SHA-256 清单在同目录 `readonly-hashes.json`。7 幅仅 512×410，不能由这些缩略图推出 1080p/4K 字号、DPI、滚动和焦点表现已验收。

全部 8 个 HTML 都引用 Tailwind CDN；V1–V3 还引用 Font Awesome，V4/I1–I4 使用远程字体与 Material Symbols。I1–I4 的 `TreeSpeedSearch`、`OnePixelSplitter`、`FormBuilder` 只是注释/排版意图，实际为 HTML 元素。V1/V2/V3 的 `--vscode-` 引用次数分别为 22/38/13，V4 为 0；I1–I4 的 `aria-` 属性数均为 0。即使存在 token/ARIA，也不等于已运行主题或读屏测试。

### 约束不变量

1. 插件只适配 `rf` 与现有本地展示/导航；不得获得 Runtime、Registry、Server、SCM、Gate 权威（ADR-0166 §决策 1）。
2. 提交不等于批准，退出码 0 不等于 Gate PASS，`PIN_MATCH` 不等于已安装或获准执行，`CACHED` 不等于当前一定可离线执行。
3. 现有共享命令表是字段/argv 契约；UI 不新增命令、不复制规则执行、签名判断、审批状态机或自动重试。
4. 键盘与错误文本不能被纯颜色取代；不为外观放宽 CSP、引入远程资源或把 digest-only Finding 扩展为源码载体。

## 2. 阶段一：逐幅视觉效果验收

| 图 | 可取之处 | 缺陷、不实之处与真实 IDE 偏差 | 状态证据与结论 |
|---|---|---|---|
| V1 | Activity Bar、侧边树、命令面板与状态栏的相对位置有 VS Code 辨识度；入口层级清楚。 | 整个 IDE chrome 都是网页仿制，不是真实 TreeView/QuickPick。图标来自 Font Awesome，不是 ThemeIcon。HTML 给展示/导航标注 `Shift+Alt+F`/`F8`，不能当作插件已有快捷键（前者会与宿主格式化习惯冲突）。树文字、运行记录徽章、编辑区密度高，缩略图难辨认；全局蓝状态栏不能由插件复制控制。 | 仅“命令发现+历史”正常态；没有无工作区/不可用态。入口示意部分合格，原生组件与17场景运行覆盖不成立。 |
| V2 | 参数、操作、输出三段的层级明显；执行/取消与日志比单一长页更易定位。 | 大面积空白表单区和密集底部日志失衡，宽窗口多列表单不能证明窄 Sidebar 适配。CPU/内存/计时及定量进度缺 CLI 契约来源；彩色终端、源码摘录与终端式装饰混淆“回执查看”定位。虽有 token，仍大量固定暗色。 | 表现运行中并含扫描发现/错误日志；不是独立的失败恢复态。无空态、取消后状态、主题/缩放证据。采纳既有布局原则，不照搬整页。 |
| V3 | Finding → 申请字段 → 错误提示 → 操作的顺序明确；必填红框同时有文字；治理提醒可见。 | 页宽/留白更像独立网页，顶部 Daemon 状态无契约；反馈/例外 Tab 与已有场景树重复。`0 / 500`、最少字数等文案不是当前字段契约；不能新增限制。画面中的 Finding 源码片段不能要求客户端扩展 digest-only 载荷。已画红框不证明触发、聚焦、播报均有效。 | 展示必填错误态，没有保存成功/失败/取消/未选 Finding 的独立证据。仅采纳现有错误状态的主题边框，保留文本和原生校验。 |
| V4 | 规则记录、独立 pin、提交回执分区有阅读层级；下一页与独立发布记录提示可借鉴。 | **没有 `--vscode-*` token**，硬编码暗色；多列清单/右侧详情在窄侧栏拥挤。新增 Deploy Rule、Sync Daemon、Governance/Audit Logs 标签、自动连通状态都不是插件能力。绿色 MATCHED 易被理解成安全授权；“审批人/24h”与 PENDING 的审批解释无当前回执依据。分页总量/上一页不应凭空推导。 | 同屏静态列表、匹配、PENDING；未表现空页、游标结束/失效、网络失败、UNKNOWN。不能作为功能规格直接实现。 |
| I1 | 右侧 ToolWindow、场景树、详情/输出的外形接近 JetBrains New UI，较 V4 克制。 | 树/详情的左右分割与现实现的上下分割不同；画面极窄，不证明更易用。固定 HTML 搜索框冒充 TreeSpeedSearch；真实平台是树聚焦后键入的速度搜索，不必常驻占一行。图标/配色来自网页主题，非 AllIcons/JBColor。控件字小，输出与树分隔细。 | 只证明选中“展示 Finding”的静态样貌；无空/加载/错误态。采纳平台速度搜索，不采纳复制搜索框或改分割方向。 |
| I2 | 执行/取消在顶部、进度和输出分区可识别；扫描场景选中态清晰。 | 13 项长表单并未逐项展示；不能证明全表单滚动。固定百分比/ETA/资源指标是示意；任务结果和实时进度混在同一区，未证明先流式到达再结束。网页 1px 分割线不是可拖动 OnePixelSplitter。 | 运行中单态；没有输入错误、超时、取消和重试证据。保留现有不定进度、stderr 与真实 splitter。 |
| I3 | 草稿路径、Finding、scope、actor、reason 的业务顺序基本可理解；原生表单密度方向正确。 | 标题把“申请例外/起草反馈”合并，实际是两场景；工具栏主按钮在窄列出现挤压/折行。静态绿勾不能取代 actor 的真实确认；broader scope 的独立确认没有充分表现。源码底纹/告警标记属编辑器示意，不应由插件重复绘制。 | 一份已填申请的正常态；缺必填错误、无可靠 scope、保存对话框取消及写失败。不抄合并表单和权限暗示。 |
| I4 | Tools→ERF 设置位置、标签/输入对齐与分组可读，明显优于仪表盘式设置。 | FormBuilder 只是注释，不能证明真实尺寸行为；测试连接是新增功能，未声明测试对象/超时/副作用。遥测框在 HTML `checked`，且无已保存配置说明，不能据此更改默认关闭的隐私策略。远程字体和固定颜色不代表平台缩放。 | 只有已配置正常态；没有必填失败、超时值错误或连接失败。保留当前 FormBuilder，不引入连接探针或遥测默认开启。 |

### 状态证据台账（不以“规范写到了”代替“画面验到了”）

| criterion | required | observed | result |
|---|---|---|---|
| visual.8 | 8 幅逐图检查 | 已看 8 图并核对 HTML；文件格式、组件/颜色问题如上 | pass（仅审阅动作） |
| native.vscode | 宿主 TreeView/命令面板、主题 token 一致 | V1 为宿主仿图，V4 无 token，均含外链资源 | fail（“100% 原生”声明） |
| native.intellij | 真正平台组件、原生缩放/输入 | I1–I4 仅 HTML 注释和外观 | blocked（不能由示意图证明原生运行） |
| states | 空/加载/失败/取消/成功及上下文切换 | 仅 V2/I2 运行中、V3 必填错误、V4 部分回执状态；无完整五态集 | fail（完整覆盖声明） |
| G1 | 真 IDE 纯键盘任务流及可见焦点 | 本次没有真 IDE 键盘取证；自动测试只验证实现接线 | blocked / 待人工 |
| G2 | 亮/暗/高对比下真实可读性 | 图片均暗色；token 静态检查不是对比度测量 | blocked / 待人工 |
| G3 | 窄侧栏与 50%–200% 缩放不截断 | 仅缩略图/HTML，未在真实 IDE 缩放 | blocked / 待人工 |
| G4 | NVDA/Narrator 等实际播报 | live region/AccessibleContext 接线可测，未听取真实读屏 | blocked / 待人工 |

设计规范“可百分百捕获”“完全满足无障碍高标准”“永不挤压”“AA 达标”等结论均须撤回为目标，不可作已验事实。禁用控件也不能仅由 opacity=1 推出 4.5:1；正常正文对比度应在具体主题实测。

## 3. 17 场景覆盖核对

源码 `ErfToolWindowFactory.Scene` 与 `sidebar.js:SCENARIOS` 均有17场景；规范第5节也列17项。但17个名字出现不等于34套双端场景状态已渲染。下表“入口”仅指 V1/I1 树/命令示意，不代表全部节点在缩略图里清晰可辨。

| # | 场景 | 稿中证据 | 当前能力/裁剪 |
|---|---|---|---|
| 1 | 展示 Finding | I1 详情；V1 入口 | 双端本地验证后展示；不新增源码预览卡或伪 CLI 命令 |
| 2 | 导航到位置 | V1/I1 入口 | 现有路径校验与编辑器导航；不复制编辑器/行号装饰 |
| 3 | 解释 Finding | 入口，缺独立详情态 | 已委托 `rf.explain`；保留 result/authority-trust 成对选择 |
| 4 | 扫描工作台 | V2/I2 运行中 | 13 参数、argv 预览、进度/取消已有；不虚构百分比、CPU/ETA |
| 5 | 起草反馈 | V3 切换标签，I3 合并标题 | 已有 classification/reason/actor、确认、预览/保存；不新增 Tab 和字符下限 |
| 6 | 申请例外 | V3 错误、I3 已填态 | 已有可靠 scope 来源、扩范围确认、保存；不能用静态绿勾代替 |
| 7 | 提交反馈 | V4 通用提交区域 | 已提交同一草稿并渲染回执；不推定审批已开始 |
| 8 | 提交例外 | V4 通用提交区域 | 同上；草稿保存不自动提交，不自动重试 |
| 9 | 查询状态 | V4 PENDING 单态 | 显式查询 RECEIVED_ONLY/REPLAYED/PENDING/UNKNOWN；不加后台轮询或 COMPLETED 审批模型 |
| 10 | 浏览规则库 | V4 一页列表 | 已保留不透明 cursor 与 hasMore；不推导总页数或将条目变成授权目录 |
| 11 | 核对独立 pin | V4 MATCHED 卡 | 已核对独立发布来源并保持 authority=UNKNOWN；不作“安全安装”承诺 |
| 12 | 缓存签名包 | V4 按钮，缺运行/失败态 | 已要求外部 descriptor 与独立 pin；不等于安装、执行许可或离线可用保证 |
| 13 | 环境诊断 | V1/I1 入口 | 已委托 `rf.doctor`，显式 scanProbe；无自动修复/全绿健康看板 |
| 14 | 校验工作区配置 | 入口，缺详情态 | 已调用 CLI；不在插件重写 YAML/schema 检验 |
| 15 | 查看有效配置 | 入口，缺详情态 | 已调用 CLI并展示输出；不额外合并默认值/环境变量 |
| 16 | 校验标准文件 | 入口，缺详情态 | 已通过映射向 CLI 传文件；不增加创作编辑器 |
| 17 | 校验规则文件 | 入口，缺详情态 | 已调用规则校验命令；规范把 XPath/AST 单测与误报率阈值写入此场景，无本次契约依据（`rf.rule.test` 是另一命令） |

## 4. 阶段二：设计点决策矩阵

判定“部分采纳”包含**保留现实现已有要点，不再重复造一套**。工作量以本次实际改造范围计；0 表示无需改代码。D1/D2 为唯二实现项；其余否决或已有能力，不以改动数量衡量成果。

| ID / 设计点 | 当前实现（基线） | 判定 | 理由 / 边界 | 改造工作量 | 风险 |
|---|---|---|---|---|---|
| M01 V1 Activity Bar/Sidebar/场景树 | package.json 贡献容器；sidebar.js 使用 createTreeView | 部分采纳（已具备） | 采用宿主入口，不复制 HTML 中整个 IDE 外壳 | 0 | 复制 chrome 会破坏宿主行为 |
| M02 V1 17场景 Command Palette/QuickPick | 命令贡献已全覆盖；选择操作使用宿主 API | 部分采纳（已具备） | 复用内建命令检索，不另写命令面板 | 0 | 自建选择器多维护焦点/快捷键 |
| M03 V1 示意快捷键 Shift+Alt+F/F8 | 无该两项插件快捷键声明 | 否决（过度设计） | 与宿主快捷键语义冲突；命令面板已可达 | 0 | 抢占宿主按键 |
| M04 V1/I1 全局连接/Daemon 状态栏 | 现有局部状态；连接由 CLI 验证 | 否决（越界） | 已配置≠已连通，未定义 daemon 健康契约 | 0 | 把缓存/配置误报为在线 |
| M05 V1/I1 图标 | ThemeIcon/AllIcons 已接入 | 部分采纳（已具备） | 保留现有平台图标；不导入 FA/Material 字体包 | 0 | 外链/CSP/图标语义漂移 |
| M06 V1/I1 历史回执 | 不可变运行记录与时间戳，点击恢复输出 | 部分采纳（已具备） | 快照回看已实现；拒绝“一键重填/重跑”混入历史查看 | 0 | 旧上下文重放、远端重复副作用 |
| M07 V2 三段式双滚动 | sidebar-webview.js 已有 grid、独立 overflow、固定操作区 | 部分采纳（已具备） | 沿用现有窄侧栏结构，不抄宽屏多列表单 | 0 | 多列在缩放下挤压 |
| M08 V2/I2 13扫描参数 | 两端已有 shared-map 参数、路由校验与预览 | 部分采纳（已具备） | 不新增参数和路由，不改 ide-command-map.json | 0 | 契约分叉 |
| M09 V2/I2 stderr/取消/进度 | 实时 stderr、后台执行、取消、合理本地超时已有 | 部分采纳（已具备） | 保留不定进度与 CLI 文本，不模拟真实指标 | 0 | UI假进度掩盖阻塞 |
| M10 V2/I2 百分比/CPU/内存/ETA | 没有相应结构化权威输入 | 否决（越界） | 无来源时不展示；也不写解析日志的估算器 | 0 | 精确数字误导、性能采集复杂度 |
| M11 V2 输出彩色终端/源码 | 当前 pre/JBTextArea 显示可信解析后的回执/文本 | 否决（过度设计） | 不引入终端/语法高亮依赖，不扩展 Finding 源码字段 | 0 | 输出转义、隐私、渲染成本 |
| M12 V2 主次按钮/token/等宽输出 | 现有 button-*、editor-font-*、contrastBorder | 部分采纳（已具备） | 保留平台 token，不复制固定深色值 | 0 | 固定颜色破坏主题 |
| M13 V3 actor/reason 行内错误红框（D2） | 已有 required、trim、reportValidity、aria-invalid 和错误文字；缺字段错误边框 | **采纳** | 用现有 aria-invalid 选择器与 inputValidation-errorBorder；不再造校验器 | **1条CSS + 原测试增强** | 低；保留 focus-visible 与文本，不仅靠颜色 |
| M14 V3 500字计数/最少字数 | 已有输入上限与 CLI 校验，无该规则 | 否决（越界） | 示意字数不能变成新业务约束 | 0 | 拒绝本应合法的申请 |
| M15 V3/I3 反馈/例外 Tab 或合并表单 | 场景树已分别路由两个场景 | 否决（过度设计） | 单一场景源足够；不增第二份选择状态 | 0 | 场景/字段切换状态不同步 |
| M16 V3/I3 Finding 源码摘要 | Finding 为 digest-only；已有编辑器导航 | 否决（越界） | 阅读代码走原编辑器，不在 Finding/回执里传源码 | 0 | 隐私和契约边界 |
| M17 V3/I3 scope、actor确认、保存路径 | 已验证候选、扩范围确认、actor确认与原生保存流程 | 部分采纳（已具备） | 保留真实确认；不采用静态绿勾或预填确认 | 0 | 错认主体/范围被扩大 |
| M18 V3/I3 治理声明 | 双端已显示提交≠批准≠Gate PASS/不写SCM | 部分采纳（已具备） | 保留文字；不把所有正常页面都绘成高危警告 | 0 | 告警疲劳/语义噪声 |
| M19 V4 分页表格、过滤/排序、上一页和总数 | 当前 CLI 页文本与下一页，cursor 不透明、hasMore守卫 | 部分采纳（仅保留既有分页） | 不新增表格状态或假总量，未证实大规模浏览需求 | 0 | 伪造分页、排序跨页不一致 |
| M20 V4 独立发布 pin / MATCHED | 已要求独立来源，PIN_MATCH/FORMAT_ONLY + UNKNOWN | 部分采纳（已具备） | 不把相等着成“安全/可信运行”的绿色认证卡 | 0 | 元数据变权威 |
| M21 V4 缓存/安装/离线承诺 | 已校验 descriptor、CACHED回执并提示后续锁/签名/撤销检查 | 部分采纳（已具备） | 保留缓存操作；否决“缓存即安装/离线保证” | 0 | 跳过真实执行信任链 |
| M22 V4 Deploy Rule / Sync Daemon | 不存在该插件职责 | 否决（越界） | 与薄客户端只委托 rf 的边界冲突 | 0 | 新控制面/运行时权威 |
| M23 V4 Governance/Audit Logs 页签 | 现有显式提交/查询与本地历史 | 否决（过度设计） | 不建设审批治理台；服务端业务由服务端持有 | 0 | 数据、权限、分页、生命周期膨胀 |
| M24 V4 审批人、24h SLA、COMPLETED | 当前严格校验 RECEIVED_ONLY/REPLAYED/PENDING/UNKNOWN | 否决（越界） | 未定义审批数据，PENDING不是审批启动的证明 | 0 | 误导批准/工作流状态 |
| M25 V4/规范§5 自动轮询/刷新/重试 | 显式查询、取消提示远端副作用未知 | 否决（过度设计） | 不引入后台计时器、重试、恢复调度器 | 0 | 重复提交、生命周期泄漏 |
| M26 I1–I3 真 ToolWindow/JB*、可拖分割 | 已用 Tree、JBScrollPane、JBText*、OnePixelSplitter、JBUI | 部分采纳（已具备） | 原生实现优于 HTML 模拟，不改组件栈 | 0 | 仿 UI 反而丢原生行为 |
| M27 I1–I3 场景树速度搜索（D1） | Tree 存在但未安装 speed search | **采纳** | installOn(tree,true,显示名)，支持折叠分组内查找；复用平台，无常驻框 | **1调用 + 1 import + SDK测试** | 低；平台负责键盘/展开；真 IDE 输入法体验待验 |
| M28 I1 常驻搜索框/快捷键/左右拆分 | 当前上下 splitter，树聚焦后可用原生键入搜索 | 否决（过度设计） | 不为一张窄图另造筛选模型/调宽策略 | 0 | 额外占位与收窄详情 |
| M29 I2 固定操作区/双滚动 | 当前已独立滚动与固定 footer | 部分采纳（已具备） | 保留真实长表单可达性，不照抄顶部按钮位置 | 0 | 工具栏在窄列截断 |
| M30 I4 FormBuilder、路径引用、超时 | 已有 FormBuilder，profile仅文件路径，短请求超时与600秒本地预算分离 | 部分采纳（已具备） | 不重建设置页，不合并网络与进程超时 | 0 | 5秒误杀本地任务回归 |
| M31 I4 测试连接按钮 | CLI已有显式诊断/读取；设置保存不发请求 | 否决（过度设计） | 未声明探测对象和副作用；不在设置页复制网络逻辑 | 0 | 保存设置伴随意外网络访问 |
| M32 I4 遥测勾选 | 当前默认关闭 | 否决（隐私边界） | 截图可能是已配置示意，不能证明默认；明确不采纳默认开启 | 0 | 隐私默认值变化 |
| M33 全图远程字体/CDN/新图标依赖 | VS Code CSP nonce + 禁外链；IntelliJ平台组件 | 否决（过度设计） | 不引入运行时远程资源、框架、CSS工具链 | 0 | 离线失败/CSP削弱/供应链 |
| M34 全图完整五态/AA/200%/读屏保证 | 自动测试接线已有，真机 G1–G4 待人工 | 部分采纳（目标保留，结论待人工确认） | 不以图片、DOM seam 或 JVM测试冒充真实宿主验收 | 0 | 假验收造成无障碍回归 |
| M35 规范§5 诊断/配置/规则验证的额外业务 | 当前 map 传参给 CLI、输出契约渲染 | 否决（越界） | 不补“误报率超标/AST内置单测/自动修复”等未声明业务 | 0 | 复制后端/运行时逻辑 |

### 否决项集中索引与裁剪依据

- **平台重复建设**：M03/M05/M06/M11/M15/M28/M33。已有宿主命令、图标、编辑器、历史、树搜索与分割器；第二套 UI/框架没有明确新增收益。
- **无输入却展示确定事实**：M04/M10/M19/M20/M21/M24。配置状态不能代表在线，日志不能提供可靠百分比/ETA，cursor 不能生成总页数，pin相等/缓存不能赋权。
- **超出薄客户端职责**：M16/M22/M23/M24/M25/M35。ADR-0166 禁复制 Runtime/Server/Registry/SCM/Gate 权威；不新增源码载荷、部署、审批台、后台调度或验证引擎。
- **破坏现有输入/隐私契约**：M14/M17/M30/M31/M32。字数限制、静态确认、统一5秒超时、隐式连通探针、默认遥测都不能从示意稿推导。

这里否决的是本次引入，不是声称复杂交互永远无价值。如果日后有真实任务量/用户证据和已批准的数据契约，应另行立项，不偷偷塞入本次视觉改造。

## 5. 与原17项修复的关系

| 原项 | 本次结论 |
|---|---|
| CRIT-01 | IJ 长表单 JBScrollPane + splitter 已有，不重构 |
| CRIT-02 | 本地长任务/短请求预算已有，不采纳画面中统一5秒暗示 |
| CRIT-03 | 无 Finding 的执行守卫已有，不删 |
| HIGH-01 | Webview 骨架单次注入 + 增量 DOM已有，不拷静态整页 |
| HIGH-02 | 必填校验/ARIA/错误文字已有，D2只补视觉边框 |
| HIGH-03 | IJ 可访问名称/状态事件已有；读屏实际效果仍待人工 |
| HIGH-04 | 双端实时 stderr/后台执行已有，不做伪进度 |
| HIGH-05 | 输出可聚焦/Escape取消/焦点引导已有，保留 |
| MED-01 | 主次按钮/高对比token已有；D2沿用主题机制 |
| MED-02 | Webview等宽输出已有，不导入外链字体 |
| MED-03 | IJ JB* / JBUI 已有，不重绘为网页 |
| MED-04 | IJ FormBuilder 已有，不重写设置 |
| MED-05 | Webview三段独立滚动已有，保留 |
| LOW-01 | 双端场景名称已统一，纠正稿中混写而不改现名 |
| LOW-02 | AllIcons/ThemeIcon已有；D1搜索是额外原生便利，不冒充图标修复 |
| LOW-03 | 历史回执回看已有，不加一键重放 |
| LOW-04 | 17场景命令贡献已有，不新增冲突快捷键 |

## 6. 阶段三：实际改造清单

| 裁决 | 文件 | 实施 |
|---|---|---|
| D1 | `clients/intellij/src/main/java/io/ruleops/ide/ErfToolWindowFactory.java` | 安装 `TreeSpeedSearch`，沿用树节点显示名，允许搜索折叠分组；没有新常驻输入框、过滤状态或命令 |
| D1验证 | `clients/intellij/src/test/java/io/ruleops/ide/ErfHostTest.java` | 真实 SDK 中折叠树，调用平台 speed search 找到“校验规则文件”，检查选中/展开与正常场景按钮 |
| D2 | `clients/vscode/src/sidebar-webview.js` | 仅为 `aria-invalid="true"` 的 input/textarea/select 使用 `--vscode-inputValidation-errorBorder`（缺失时使用 errorForeground）；保留 focus-visible |
| D2验证 | `clients/vscode/test/ui-regressions.test.js` | 检查错误边框token、焦点轮廓；原 DOM用例补断言 invalid true→false 与错误文字清空 |

未新增依赖、未改后端、scripts、命令表、进程/网络流程、权限/状态模型、遥测或输出契约。原 17 项修复没有被当作“新增设计能力”重复实现。

## 7. 自动验收与复现

测试证据目录：`.scratch/ide-uiux-decision/`；门禁完整日志：`.scratch/gate/ide-uiux-design-decision-full.log`。

1. 回归红灯：VS Code 新边框检查在修改前失败；IntelliJ 新 speed search 用例在修改前失败（1 test / 1 failed）。日志 `vscode-red.log`、`intellij-red.log`。红灯是缺目标能力，而非测试工具错误。
2. VS Code 定向：`timeout 60s node --test clients/vscode/test/ui-regressions.test.js`，**9/9 passed**，`vscode-targeted.log`。
3. VS Code 全套：先 `source ~/.local/opt/erf-env.sh`，再 `ERF_REAL_RF=/home/kali/文档/ruleops/rf timeout 120s node --test clients/vscode/test/*.test.js`，**105/105 passed，0 failed，0 skipped**；`vscode-real-cli.log`。真实 CLI 场景使用 loopback contract fixture，不冒充生产 Server。
4. IntelliJ：先加载同一环境，运行 `timeout 180s bash /mnt/c/Users/amsterdam/.gradle/wrapper/dists/gradle-8.10.2-bin/a04bxjujx95o3nb99gddekhwo/gradle-8.10.2/bin/gradle -p clients/intellij test buildPlugin --console=plain`；**11/11 passed，0 failures/errors/skipped，BUILD SUCCESSFUL**；结果见 `intellij-delivery.log` 和 `clients/intellij/build/test-results/test/TEST-io.ruleops.ide.ErfHostTest.xml`。
5. 总门禁：在仓根加载同一环境后 `timeout 5400s bash scripts/check.sh`，输出完整落盘至上述 `.scratch/gate/` 日志；本次 **exit=0，check: all sides green**；2026-10-05 19:38:56–19:51:38（Asia/Shanghai），耗时12分42秒。
6. 提交前检查允许文件清单、原件hash与 `git diff --check`；只在 WSL 内 commit，不 push。

### 本次验收结果

| 检查 | 结果 | 证据与限制 |
|---|---|---|
| 全量门禁 | **PASS / exit 0** | 完整读取1067行日志，51个阶段结束，末行 `check: all sides green`；机器摘要 `gate-summary.json` |
| IntelliJ真实SDK + buildPlugin | **11/11、构建成功** | 非纯Java替身；目标SDK 2024.1，Gradle 8.10.2；不是运行中的IDE截图 |
| VS Code全套（含真实CLI） | **105/105、0 skipped** | 真实CLI与本地契约服务；非真实VS Code宿主交互 |
| 标准轴审查 | **通过** | 仅4个既有客户端文件与本报告；无依赖/权威/权限/后端/脚本变更；没有新增校验分支、静默回退或吞错 |
| 规格轴审查 | **通过（约定交付范围）** | D1/D2已实施，D3裁剪明确；35项矩阵、8图、17场景/原修复都有记录；未把人工项报绿 |
| 原件与共享契约 | **20个SHA-256均未变化** | `readonly-hashes.json`覆盖原稿全部文件、原评审、修复报告与共享命令表 |
| 格式/范围 | **git diff --check通过** | D:树已有未跟踪文档为用户所有，未操作；本次改动仅WSL允许清单 |

门禁通过并不升级原证据等级：日志仍有 `author-cost-baseline=INCONCLUSIVE`、`pilot-delivery=STATIC_ONLY`、`feature-evidence=STATIC_ONLY`，这是既有明确的证据限制，不是本次UI测试失败，亦不能包装成生产业务/人工验收通过。完整门禁日志 SHA-256：`7a9eb3d1d164aae93ffd27d9d96d35a2495187bd2536daa9146750ec08be45a3`。

## 8. 待人工确认与验收边界

- **G1**：分别在真实 VS Code/IntelliJ 用键盘完成 explain→draft；IntelliJ 在树聚焦后输入中文场景名，确认输入法、搜索弹层、Escape行为与取消长任务不冲突。SDK调用搜索证明接线，不证明真实按键/IME体验。
- **G2**：亮/暗/高对比下触发空 actor/reason，检查错误边框、关联文本和聚焦轮廓同时可见；修正输入后边框回到普通态。需实测颜色对比，不把token存在当 AA证书。
- **G3**：窄侧栏、长路径、200%缩放下检查最后一个扫描参数、执行/取消、输出区可达；静态小图不足以裁定左右/上下分割哪个更好，本次不调整。
- **G4**：NVDA/Narrator/适用平台读屏实际播报就绪→运行→取消/失败→完成；确认错误文字关联与树搜索结果可感知。
- 未验证真实 Registry 多页/特殊 cursor、真实 Server 各回执、重启、多工作区、断网与宿主保存对话框的全部组合；沿用原真实 IDE 清单的待人工状态，不把本次两处UI小改变为全产品真机验收。
- 数据表格/复杂历史重放是否值得做：当前无真实需求量证据，按批准范围**本次否决引入**；长期收益仍待用户研究，不声称已证明永远无用。

## 9. 覆盖与最终结论

**Coverage statement**：8/8 图片与对应 HTML 已审；35行主要设计点和17场景均列出；原17项修复逐条核对。没有运行设计稿浏览器交互，也没有执行真实 IDE 的 G1–G4，未审项明确列于第8节。

**Limitations statement**：对齐/密度/一致性属于基于缩略图与HTML的视觉判断；没有用户研究、对比度实测或屏幕阅读器录制。原生感评价区分“长得像”和“实际用原生组件”。自动检查只能证明代码/SDK/CLI边界，不能证明所有人机交互。

**Verdict**：设计稿整体 **Recirculate**，不照稿重写；已获批的 D1/D2 以原生最小改动落地，D3保留已有能力并裁掉新增复杂度。代码交付只在双端测试与总门禁均绿后成立，真实宿主体验仍以人工清单为准。
