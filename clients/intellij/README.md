# ERF Thin Client（IntelliJ 平台）

Enterprise Rule Factory 的 IntelliJ IDE 薄客户端。它是纯 UX 适配器：调用 `rf`
启动器、以 digest-only 姿态展示/导航/解释 Finding、起草 Feedback、发起 Exception
申请、可选向 Server 提交（仅 `RECEIVED_ONLY`）。它**绝不**持有
Runtime/Factory/Server/Registry/SCM/Gate 权威。

## 架构：薄绑定层 + 零依赖纯逻辑核

- `src/main/java/io/ruleops/ide/core/ThinClientCore.java` —— 零依赖纯逻辑核：
  rf 参数构造、`explain` 文本解析、退出码语义、路径安全门禁
  （镜像 `erf-thin-client` 的 `validate_relative_path`）、FindingView 闭世界/隐私校验、
  SubmissionReceipt 只读渲染与三条虚假权威守卫。无 IntelliJ SDK、无网络依赖。
- `src/main/java/io/ruleops/ide/**` —— 薄 IntelliJ 绑定层（`AnAction`、服务、rf 调用），
  仅把用户操作转发给纯核与 `rf`。
- `selftest/io/ruleops/ide/core/CoreSelfTest.java` —— 零依赖离线自测入口，
  可用 plain `javac` + `java` 运行（与 VS Code 端 `node --test` 逐条对齐）。

## 权威边界（fail-closed）

- digest-only：`FORBIDDEN_FIELDS`（source/sourceCode/sourceContent/sourceSnippet/
  fullFinding/findings）递归拒绝；FindingView 闭世界只允许既定字段。
- 导航前双重路径门禁：`validateRelativePath` + 解析后绝对路径必须落在项目根内。
- 退出码：0 可解释；2 输入非法；3 拒绝/PENDING/UNKNOWN（绝不按成功处理）。
- SubmissionReceipt 的 `exceptionApproved` / `gateMutated` / `scmPublished`
  任一为真即判为契约违背，拒绝渲染成功。
- 提交/状态查询要求显式 `serverEndpoint`（禁用自动发现），遥测默认关闭。

## 配置（项目级）

| 项 | 默认 | 说明 |
| --- | --- | --- |
| `rfPath` | `rf` | rf 启动器路径 |
| `serverEndpoint` | 空 | 为空时提交/状态查询禁用 |
| `telemetryEnabled` | `false` | 遥测默认关闭 |
| `requestTimeoutMs` | `5000` | rf 调用超时 |

## 离线核自测（无需 IDE SDK / 无需联网）

```bash
javac -d /tmp/erf-intellij-selftest \
  src/main/java/io/ruleops/ide/core/ThinClientCore.java \
  selftest/io/ruleops/ide/core/CoreSelfTest.java
java -cp /tmp/erf-intellij-selftest io.ruleops.ide.core.CoreSelfTest
```

门禁脚本 `scripts/ide-client-check.sh` 会真实驱动 `rf` 并交叉校验本核与 VS Code 端核。

## 完整插件构建与安装（人工离线步骤）

完整 IntelliJ 插件构建需要 IntelliJ 平台 SDK 与 Gradle 插件依赖，须在**离线镜像**下解析：

```bash
gradle buildPlugin   # 产出 build/distributions/*.zip
```

随后在 IntelliJ：Settings → Plugins → ⚙ → Install Plugin from Disk…，选择上述 zip。
交互式真实 IDE 验收与插件仓库维护属人工最后一步。
