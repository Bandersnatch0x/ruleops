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
