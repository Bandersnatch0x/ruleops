# ERF Thin Client — VS Code 内部兼容维护说明

本说明仅面向内部兼容维护与受控验收；既有功能和维护职责不变。
Digest-only Finding 的 IDE 薄客户端。它**只**通过 `rf` 启动器复用 Runner / Thin Client seam，
不列为当前公开功能承诺。
它**不**复制 Runtime/Factory/Server/Gate 逻辑，不提供本地 Gate PASS、
Exception 批准或 SCM 写入入口，默认不上传源码或完整 Finding。

## 架构：薄绑定 + 纯逻辑核

- `src/core.js` — 零依赖纯逻辑核：构造 `rf` 调用、解析输出、导航前路径安全门禁、
  隐私（forbidden fields）门禁、退出码分类、状态标签、回执权威守卫（`exceptionApproved` /
  `gateMutated` / `scmPublished` 恒为 false，否则 fail closed）。可离线单测。
- `src/extension.js` — 薄 VS Code 绑定层：注册命令、打开/跳转编辑器、通过 `rf` 子进程执行动作。
- `test/core.test.js` — `node --test` 离线核测试，执行真实行为，零依赖零网络。

## 状态与权威边界

- 反馈起草 = `PROPOSED`；例外申请 = `REQUESTED`；提交回执 = `RECEIVED_ONLY`。
- 提交“已接收”**绝不**等于“已采纳/已批准/Gate PASS/已写入 SCM”，UI 恒以只读姿态展示守卫。
- 导航目标必须是工作区内相对路径（拒绝绝对路径、`..`、`\`、`:`、控制字符）。

## 配置

- `erf.rfPath`（默认 `rf`）：`rf` 启动器路径。
- `erf.serverEndpoint`（默认空）：可选 Server endpoint；为空则禁用提交，不自动发现。
- `erf.telemetryEnabled`（默认 `false`）：隐私优先，默认关闭。
- `erf.requestTimeoutMs`（默认 `5000`）。

## 维护者：离线构建与人工兼容验收

核逻辑与门禁证据在离线环境即可用 `node --test` 验证（见仓库 `scripts/ide-client-check.sh`）。
打包为 VSIX 并安装到真实 VS Code 需要离线依赖缓存与 `@vscode/vsce`，属发布负责人执行的最后一步：
本节仅供维护者按既有商定范围复现，不构成面向普通用户的安装或发布承诺。

```bash
# 需预置离线 npm 缓存与 vsce
npx @vscode/vsce package        # 生成 erf-thin-client-0.1.0.vsix
code --install-extension erf-thin-client-0.1.0.vsix
```

随后在真实 VS Code 上完成 导航→解释→草稿→可选提交 的交互闭环验收。
上述人工验收职责保持不变；本说明不宣称真实 IDE 验收已完成。

## 阶段 1 原生入口（尚非真实 IDE 验收）

Activity Bar 的 ERF 入口提供分组任务树与轻量场景表单，原 8 个命令只负责打开并选中同一场景。
7 个分区从 `clients/ide-command-map.json` 派生，31 条 CLI 清单均可发现；尚未接入的命令显示不可用，不执行命令模板。
多根工作区须显式选择；rf/endpoint 仅使用设置，调用期间可取消，配置切换使旧结果失效。
当前阶段只提供已有输入能力，例外范围限已验证 Finding 的当前文件；草稿文件衔接、完整动态选项及高级能力仍属后续阶段。

仓库开发或打包前运行 `node clients/vscode/scripts/prepare-resources.js`，将共享元数据复制为插件资源（生成件不入 Git）；VSIX prepublish 自动执行同一步。
定向测试：`node --test clients/vscode/test/*.test.js`（包含宿主 API 替身测试，不等于真实 VS Code 验收）。


## 阶段 3：显式连接与独立发布记录

规则库分区提供 `registry list rules|standards`、`registry show` 和 `registry sync rules`。列表与下一页只调用 `rf`；cursor 保持不透明，JSON 编码为一个 `--cursor-json` argv（包括 NUL），不拆身份或自行发 HTTP。

连接 profile 设置仅保存 **FILE 路径**，不填 JSON、token、密码或证书内容；非空时仅传 `--connection-profile FILE`，否则使用显式 Server endpoint。两者不同时传给 CLI，profile 失败不回退。配置文件解析、安全凭据引用及连接授权由 CLI/运维负责；插件不会创建代理/隧道或绕过连接限制。反馈/例外原有 endpoint 路径不变。

通过“选择独立发布记录”载入外部发布系统提供的 JSON：

```json
{
  "pins": [
    {
      "resource": "rules",
      "id": "your-exact-rule-id",
      "version": "your-exact-version",
      "expectedDigest": "sha256:<64 lowercase hex characters from the independent release source>",
      "provenance": "release job / approved manifest reference",
      "deliveryDescriptor": "signed-delivery.json"
    }
  ]
}
```

此示意摘要占位符不能执行。该文件是**独立来源输入，不是信任根或发布证明**；CLI 最终裁决。每个下拉选项绑定 resource/id/version/expectedDigest/provenance 整条记录，并限定当前工作区和连接；切换配置后须重新载入。不会从列表/show 响应补齐或替换 pin。`deliveryDescriptor` 可省略并另选外部文件；记录中的相对路径按发布记录文件目录解析。descriptor 改变不会改变独立 pin，换选 pin 时会清除旧 descriptor 配对。

- 浏览：`FORMAT_ONLY / authority=UNKNOWN / 未安装`，不把 registry state 当执行授权。
- 核对：`PIN_MATCH / UNKNOWN / 未安装` 仅表示身份/版本/存储摘要相符，不是 RuleBundle 摘要或签名证明；不符、缺来源、未知值保持失败/UNKNOWN。
- 缓存：选择外部签名 `BundleDeliveryDescriptor`（含 `cacheRoot`）后显式确认 sync，传入完整 pin、descriptor FILE 和当前 repo-root。artifactKind、bundleDigest、destination、不可变覆盖策略、offlineAvailable 与 provenance 只展示 CLI receipt 的字段，不从 pin、目录元数据或缓存命中推断。
- `CACHED` 不是“安装成功”或 Gate PASS；后续扫描仍须 ExecutionLock、独立 trust root、签名/兼容/撤销验证。来源缺失和 transport 不可用会显式失败，不自动重试或改连服务。

验证范围是纯核与宿主绑定替身/状态绑定；真实 IDE 安装、真实企业 endpoint 与发布资产仍待负责人提供，不能由这些测试代替。

VS Code 设置名：`erf.connectionProfile`（默认空）。定向命令：`node --test clients/vscode/test/core.test.js clients/vscode/test/sidebar.test.js clients/vscode/test/registry.test.js`。

## 阶段 4/5：统一命令面、扫描工作台与当前状态

### 真实功能（离线测试已建立证据）

- **32 项命令面**：树视图与场景面板均从 `clients/ide-command-map.json` 派生；16 个场景可执行（`SCENARIOS`），其余命令节点显式标记“不可用 · 待后续阶段”，不执行命令模板。argv 由 generic builder 按 map params 机械展开（枚举/digest/number/csv/bool 类型校验 fail closed）。
- **扫描工作台**（`erf.scan`）：三路线互斥（`--rule-id` / `--candidate` / `--delivery-descriptor`）、base/head 成对校验、stderr progress 行透出；exit=0/2/3/4 全部按 ScanResult/ScanFailure 契约渲染，**绝不把 exit=0 显示为 PASS**，以 status/gateDecision/authority 为准。
- **registry 三命令**：`registry list rules|standards`（分页 cursor 保持不透明、JSON 编码为单个 `--cursor-json` argv，NUL 不拆参数）、`registry show`（独立 pin 全字段绑定核对）、`registry sync rules`（外部签名 descriptor + 显式 repo-root）。连接 profile 仅 FILE 路径。
- **负例与边界**（阶段 5 新增 `test/stage5-negatives.test.js`，11 项）：NUL cursor 往返、分页回显不一致拒绝、PIN digest/identity mismatch、缺独立 pin、release JSON schema 非法、endpoint 带鉴权信息/查询串/哈希拒绝、submission status UNKNOWN 回执 fail closed、schema 非法 receipt 拒绝、FindingView 路径越界（`..\`/绝对/控制字符）、写文件失败不产出请求且表单保留、scan 垃圾输出绝不渲染 PASS、delivery-descriptor 路线端到端、limit 非法不调用 rf。

### 限制（不可用项清单与原因）

- **Gradle `org.jetbrains.intellij:1.17.4` 离线不可解析**（`gradle --offline buildPlugin` 失败：插件仓库缺依赖，与阶段 2 结论一致）→ IntelliJ 插件打包 INCONCLUSIVE。
- **VSIX 打包**：`npx --no-install @vscode/vsce`（4.0.0）离线可用，可产出 VSIX（见复现步骤）；但**真实 VS Code 安装后的交互验收未完成**，属人工步骤。
- **真实 IDE 行为**：多 workspace 实际切换、键盘导航/主题/缩放、长任务取消后的真实进程状态、重启恢复、真实网络断连——均只能人工验收（见 `docs/agents/` 下阶段 5 人工验收清单）。
- **状态查询 authority 校验差异**：Java 核对 status=UNKNOWN 强制 authority=UNKNOWN（`STATUS_AUTHORITY_GUARD`）；JS 核只要求 authority ∈ {UNKNOWN, RECEIVED_ONLY}。两端各自渲染、不在 parity harness 内，属已知分歧。

### 复现步骤

```bash
# 定向测试（离线零依赖）
node --test clients/vscode/test/*.test.js

# 跨端 parity（真实 rf 输出喂两端核，diff 必须为 0）
bash scripts/ide-client-check.sh .scratch/ide-client-check

# VSIX 离线打包（vsce 4.0.0 已验证可用）
node clients/vscode/scripts/prepare-resources.js
cd clients/vscode && npx --no-install @vscode/vsce package --out ../../.scratch/vsix/erf-thin-client-0.1.0.vsix
code --install-extension ../../.scratch/vsix/erf-thin-client-0.1.0.vsix   # 真实 IDE 人工验收起点
```

真实 IDE 人工验收清单（多 workspace、断网、NUL cursor、多页、PIN mismatch、取消长任务、恶意路径、重启恢复、UNKNOWN 回执、键盘/主题/缩放等）见 `docs/` 阶段 5 人工验收清单；清单中每项标注“离线已测/待人工”，未建立证据的项不得声称通过。
