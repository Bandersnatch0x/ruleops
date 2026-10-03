# ERF Thin Client — IntelliJ 内部兼容维护说明

本说明仅面向内部兼容维护与受控验收；既有功能和维护职责不变，不列为当前公开功能承诺。
Enterprise Rule Factory 的 IntelliJ IDE 薄客户端。它是纯 UX 适配器：调用 `rf`
启动器。既有提交回执（仅 `RECEIVED_ONLY`）。它**绝不**持有
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
# 从仓库根运行；Java 21；只读共享 scope 夹具。
OUT=clients/intellij/build/stage2-selftest
mkdir -p "$OUT"
timeout 60 javac -d "$OUT" \
  clients/intellij/src/main/java/io/ruleops/ide/core/ThinClientCore.java \
  clients/intellij/src/main/java/io/ruleops/ide/WorkflowFiles.java \
  clients/intellij/selftest/io/ruleops/ide/core/*.java \
  clients/intellij/selftest/io/ruleops/ide/WorkflowFilesSelfTest.java
timeout 60 java -cp "$OUT" io.ruleops.ide.core.CoreSelfTest
timeout 60 java -cp "$OUT" io.ruleops.ide.WorkflowFilesSelfTest
timeout 60 java -cp "$OUT" io.ruleops.ide.core.ParityHarness
```

门禁脚本 `scripts/ide-client-check.sh` 会真实驱动 `rf` 并交叉校验本核与 VS Code 端核。

## 维护者：完整插件离线构建与人工兼容验收

完整 IntelliJ 插件构建需要 IntelliJ 平台 SDK 与 Gradle 插件依赖，须在**离线镜像**下解析：

```bash
gradle buildPlugin   # 产出 build/distributions/*.zip
```

随后在 IntelliJ：Settings → Plugins → ⚙ → Install Plugin from Disk…，选择上述 zip。
交互式真实 IDE 验收与插件仓库维护属人工最后一步。
本节仅供维护者按既有商定范围复现，不构成公开安装或发布承诺，也不宣称真实 IDE 验收已完成。

## 阶段 1 工具窗与宿主测试

ERF 工具窗提供原生 Swing 场景表单和分组树，8 个 action 只进入同一工具窗；Settings → ERF 配置项目级路径、endpoint 和超时。
共享 `clients/ide-command-map.json` 由 `processResources` 打包；未接入命令保持可见且不可执行。
所有 rf 进程通过 `RfInvoker.submit` 的可取消 `Task.Backgroundable` 启动，完成/失败/取消回调回 EDT，设置变化废弃旧回调。
阶段 1 建立了工具窗骨架；阶段 2 的保存/提交闭环见下节，31 项完整执行覆盖仍不作完成声明。

SDK 宿主测试：`src/test/java/io/ruleops/ide/ErfHostTest.java`，需可用离线 IntelliJ SDK 与相匹配 Gradle 环境：
`gradle -p clients/intellij --offline test --tests io.ruleops.ide.ErfHostTest`。
该测试核实原生面板/描述符、后台实际 Java 子进程、EDT 回调与取消；不能以 `coreSelfTest` 或源码注册断言替代。
未在真实 IDE 中验收，不据声明推导安装兼容性。

## 阶段 2：对象与文件闭环

- actions 继续统一委派工具窗；分类取自 CLI 四值，actor 文本须勾选确认。reason 使用多行文本控件，但现有 CLI 拒绝换行/控制符及超过 2000 UTF-8 字节；原文不被静默归一化，错误保留表单。
- `ThinClientCore.scopeOptions` 只消费纯数据。`WorkflowFiles` 根据实际文件与 `rf config show --format json` 的内部 `config.modules/source_roots` 验证路径；选项按当前文件、直接父目录、关联模块、source root 去重。仅新 Finding 默认文件，刷新保留原选项；失效后禁用而不自动选第一项。扩大需确认，缺少可信文件或配置时阻塞；拒绝越界、符号链接、控制符与裸星。
- 草稿/申请经原生输出选择器与参数预览，显式 `--output` 后核实真实新写入文件、字段与身份，才显示保存成功并进入项目本地 `ErfArtifactIndex`。输出失败/取消不清表单；覆盖须确认，不覆盖输入文件。已有文件时间戳未变化视为未证实新写入，而不是假成功。
- “提交刚保存的同一对象”直接引用该路径；执行前重读文件、比较内容指纹和身份。提交前展示 endpoint、requestId、artifactDigest 与字段范围；回执另选显式文件并留存索引。UNKNOWN/PENDING/超时不当作批准，不自动重提。
- 状态页下拉选择已保存对象，选择 requestId 或 artifactDigest 查询，不手抄摘要。endpoint 变化取消任务、废弃旧回调并清除选择，索引按工作区与 endpoint 隔离。
- explain 的 result/authority-trust 使用成对文件选择并原样传给 CLI；插件不推导 trust root 或审批权威。

`WorkflowFilesSelfTest` 是真实文件系统检查，`CoreSelfTest`/`ParityHarness` 是纯核检查；均不替代 SDK 类型检查、打包安装或真实 IDE 验收。完整编译需要离线可用的 `org.jetbrains.intellij:1.17.4` 与 IntelliJ 2024.1 SDK；依赖不可解析时必须报告未编译。


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

IntelliJ 设置：ERF → Registry connection profile FILE（默认空）；持久字段 `connectionProfile` 只含路径。离线纯核和真实状态绑定定向测试：`bash clients/intellij/selftest/run-stage3.sh`（需要 JDK 21，输出在 `clients/intellij/build/stage3-selftest/`）。`ErfHostTest` 另需真实 SDK/test application；语法解析不是 SDK 编译或真实 IDE 验收。

## 阶段 4/5：统一命令面、扫描工作台与当前状态

### 真实功能（离线测试已建立证据）

- **32 项命令面**：`ThinClientCore.buildMappedArgs`/`commandPrefix` 与 VS Code 端消费同一份 `clients/ide-command-map.json` params，机械展开 argv（枚举/digest/number/csv/bool 校验 fail closed，identity 字段由 UI 显式收集）。
- **扫描工作台**：`buildScanArgs` 三路线互斥（`--rule-id` / `--candidate` / `--delivery-descriptor`）、base/head 成对；`renderScanReport` 按 ScanResult/ScanFailure 契约渲染，**绝不把 exit=0 显示为 PASS**。
- **registry 三命令**：`ErfRegistry` 绑定 `registry list`（NUL cursor JSON 编码为单个 `--cursor-json` argv）、`registry show`（独立 pin 全字段绑定）、`registry sync rules`（SYNC_RULES_ONLY）。profile 仅 FILE 路径；连接/工作区切换使 pin 与 descriptor 失效。
- **负例与边界**（阶段 5 新增 `Stage5SelfTest`，54 项断言）：buildScanArgs 路线冲突/缺路线/base-head 不成对、buildMappedArgs 必填缺失/枚举非法/digest 形状/控制字符、NUL cursor JSON 转义且 argv 无裸控制字符、分页回显不一致拒绝、PIN digest/identity mismatch、release JSON schema 非法、endpoint 带鉴权信息拒绝、status UNKNOWN 回执 authority 矛盾拒绝（`STATUS_AUTHORITY_GUARD`）、schema 非法 receipt 拒绝、location 路径越界、scan 垃圾输出 fail closed。已挂入 `run-stage3.sh` 自测链。

### 限制（不可用项清单与原因）

- **Gradle 离线构建不可用**：`gradle --offline buildPlugin` 无法解析 `org.jetbrains.intellij:1.17.4`（插件仓库缺依赖；阶段 2/5 两次确认）→ 插件 zip 打包 **INCONCLUSIVE**，须人工在离线镜像下解析后复现。
- **SDK 宿主测试未运行**：`ErfHostTest` 需要真实 IntelliJ SDK test application；当前仅有纯核自测 + Java 语法解析（非 SDK 类型检查）。
- **真实 IDE 行为**：工具窗/表单真实交互、多项目窗口、长任务取消（`Task.Backgroundable` 真实中断）、重启恢复、真实网络断连——均只能人工验收（见 `docs/` 阶段 5 人工验收清单）。
- **状态查询 authority 校验差异**：Java 核对 status=UNKNOWN 强制 authority=UNKNOWN（`STATUS_AUTHORITY_GUARD`）；JS 核只要求 authority ∈ {UNKNOWN, RECEIVED_ONLY}。属已知分歧，已记入阶段 5 报告。

### 复现步骤

```bash
# 定向自测（离线零依赖，含阶段 5 Stage5SelfTest）
source ~/.local/opt/erf-env.sh
bash clients/intellij/selftest/run-stage3.sh

# 跨端 parity（真实 rf 输出喂两端核，diff 必须为 0）
bash scripts/ide-client-check.sh .scratch/ide-client-check

# 插件打包（INCONCLUSIVE：须人工提供离线镜像）
gradle buildPlugin   # 产出 build/distributions/*.zip 后 Install Plugin from Disk…
```

真实 IDE 人工验收清单（多 workspace、断网、NUL cursor、多页、PIN mismatch、取消长任务、恶意路径、重启恢复、UNKNOWN 回执、键盘/主题/缩放等）见 `docs/` 阶段 5 人工验收清单；清单中每项标注“离线已测/待人工”，未建立证据的项不得声称通过。
