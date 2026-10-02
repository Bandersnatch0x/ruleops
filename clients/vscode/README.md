# ERF Thin Client — VS Code 扩展

Digest-only Finding 的 IDE 薄客户端。它**只**通过 `rf` 启动器复用 Runner / Thin Client seam，
展示 digest-only Finding、按相对位置导航、解释、起草反馈、申请例外，并可选提交到已配置的
RuleOps Server。它**不**复制 Runtime/Factory/Server/Gate 逻辑，不提供本地 Gate PASS、
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

## 离线构建 / 安装（人工最后一步）

核逻辑与门禁证据在离线环境即可用 `node --test` 验证（见仓库 `scripts/ide-client-check.sh`）。
打包为 VSIX 并安装到真实 VS Code 需要离线依赖缓存与 `@vscode/vsce`，属发布负责人执行的最后一步：

```bash
# 需预置离线 npm 缓存与 vsce
npx @vscode/vsce package        # 生成 erf-thin-client-0.1.0.vsix
code --install-extension erf-thin-client-0.1.0.vsix
```

随后在真实 VS Code 上完成 导航→解释→草稿→可选提交 的交互闭环验收。
