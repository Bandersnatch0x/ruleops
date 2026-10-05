# 域 09：运行时架构规格符合度审计

## 域范围

- 标的：`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md` §40–§51，2364–3147 行，已按指定 sed 范围读取并对关键段落复读行号。
- 根：WSL **kali-linux2** `/home/kali/文档/ruleops`；HEAD `7e7d2820de5f2948bd936b64f5907ae211f00645`；2026-10-05。默认发行版无该目录，未用 Windows 树替代。
- 只读专项审计，仅写本报告；以下代码路径相对上述根。结论：**三层架构、Registry 服务端、离线缓存与可信上下文都有实现，但 V1 可信运行时尚不能认定完整符合。**
- 一致只指列明静态契约；部分一致表示真实但不完整；不一致表示可定位相悖行为；未实现指限定检索“未找到实现证据”；无法验证不得反推不存在。

## 逐声明核验表

| 声明（规格章节与行号） | 代码实证（至多两处） | 判定 | 理由 |
|---|---|---|---|
| R01 §40/§51 L2366–2426、3131–3144：Server + Runtime + Thin Clients | `control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:79`；`runtime/rf-cli/src/main.rs:1390` | 一致 | 有真实 HTTP Server、CLI 调共享 scan；IDE 见 R15，不只是目录占位。 |
| R02 §40.1 L2430–2446：全部治理/生产职责 | `control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:31-42`；`control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:335-338` | 部分一致 | 有 Standards/Coverage/Rule/Factory/Verification/Release/Policy/Feedback/Exception 资源和作业入口；不能据此证明 AI Authoring、Analytics、生产调度/签发全部闭环。 |
| R03 §40.1 L2448：服务端默认不承担日常业务扫描 | `runtime/rf-runner/src/main.rs:1032`；`control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:227-230` | 部分一致 | 实际 scan 在 Runtime；Server 公共写入口拒绝指定源码字段。全部署扫描策略未验证。 |
| R04 §40.2 L2452–2478：Rust、Resolver/Planner/Host/Normalizer/Completeness/Baseline/Gate 共享 | `runtime/rf-runner/src/main.rs:1032`；`runtime/trusted-ci/src/lib.rs:114-139` | 一致 | 本地与可信 CI 共享 Rust scan，Gate/Baseline 外围组合，不要求同一个 crate。 |
| R05 §40.2 L2480–2484：独立 Bootstrap 测量真实 binary 并启动 | `runtime/bootstrap/src/lib.rs:68-112`；`runtime/trusted-ci/examples/phase3_real_scan.rs:268-275` | 部分一致 | 有字节测量/验签库，无独立启动边界证据；示例测量固定字符串而非当前运行 binary，只能算夹具。 |
| R06 §40.2 L2486–2489：Bootstrap 独立分发、Root rotation、Runner 不可写 anchor/state | `runtime/bootstrap/src/lib.rs:50-75`；`runtime/trust-state/src/lib.rs:180-255` | 未实现 | 前者接收调用者 TrustStore，后者文件持久化；未找到独立 OS/CI 分发、权限隔离或连续阈值 Root 轮换实现证据。 |
| R07 §40.2 L2491–2494：签名 RunnerRelease、精确锁定，本地构建不可信 | `runtime/bootstrap/src/lib.rs:16-29`；`runtime/rf-runner/src/main.rs:1708` | 部分一致 | Release/measurement 模型和 LOCAL_UNATTESTED 有实现；实际二进制供应链绑定受 R05 限制。 |
| R08 §40.2 L2496–2499：Host 深模块、跨进程 framed Protobuf | `runtime/rf-runner/src/main.rs:1582-1595`；`runtime/host-client/src/lib.rs:403-427` | 一致 | Runner 启动 Host，握手、规划并读取结构化观测，不直接嵌入 Java 引擎。 |
| R09 §40.2 L2501–2504：每扫描 Host、不跨扫描池化、终止回收 | `runtime/rf-runner/src/main.rs:1587`；`runtime/host-client/src/lib.rs:88-95` | 部分一致 | scan 新建 Host，Drop kill/reap；多 Pack、全进程树/各平台回收未完整验证。 |
| R10 §40.2 L2506–2515：完成时校验 attempt identity、observation count/digest 再提交 | `contracts/proto/backend_host.proto:170-173,284-294`；`runtime/host-client/src/lib.rs:403-441` | 不一致 | Completion 只有 units，无所需 count/digest/attempt identity；本地 transcript 包含全部 execute 响应帧，不与 Host 独立摘要比较。 |
| R11 §40.2 L2517–2520：有界 append-only staging/观测/磁盘限额 | `runtime/rf-runner/src/main.rs:1912-1925`；`runtime/host-client/src/lib.rs:419-423` | 部分一致 | 有 staging bytes/event limit；未找到独立隔离卷磁盘配额。成功后会读回 Vec，不是全程流式。 |
| R12 §40.2 L2522–2525：Runner-only 隔离 staging、立即删除、受控诊断 | `runtime/rf-runner/src/main.rs:1897-1909`；`runtime/rf-runner/src/main.rs:1955-1957` | 不一致 | 使用全局 temp_dir，未显式 0600/隔离目录；删除错误被忽略。加密/TTL/审计 DiagnosticBundle 未找到实现证据。 |
| R13 §40.2 L2527–2537：CanonicalFindingSet v1 deterministic Protobuf、版本化摘要 | `runtime/gate/src/lib.rs:573-598`；`runtime/gate/src/lib.rs:598-618` | 不一致 | 有排序和版本前缀，但摘要是若干字段 NUL 拼接，并混入 delivery descriptor；不是约定的 deterministic Protobuf FindingSet。迁移映射未验证。 |
| R14 §40.2 L2539–2541：仅瞬时失败可最多重试一次、attempt 留证 | `runtime/rf-runner/src/main.rs:1587-1606`；`runtime/rf-runner/src/main.rs:1657-1662` | 部分一致 | 当前单次失败退出，不会对确定性错误重试。“可重试”不要求必须重试；多 attempt 证据模型未验证。 |
| R15 §40.3 L2569–2594：CLI/IntelliJ/VS Code 薄交互，不自带引擎 | `clients/vscode/src/rf-invoker.js:35`；`clients/intellij/src/main/java/io/ruleops/ide/RfInvoker.java:73` | 一致 | 两类 IDE 都调用 rf 子进程；所检调用路径没有独立 PMD/SpotBugs runtime。作者侧全部 CLI 功能未由此项认证。 |
| R16 §40.2 L2543–2549：default-deny 沙箱，可信执行不裸跑 | `runtime/sandbox/src/lib.rs:73-87`；`runtime/trusted-ci/src/lib.rs:119-125` | 部分一致 | 只读/无网络/cap drop/seccomp/资源限制真实，可信入口要求两个沙箱；完整子进程能力授权、open-files 限制未找到证据。 |
| R17 §40.2 L2551–2565：V0.1 linux/amd64、rootless OCI、platform index | `runtime/trusted-context/src/lib.rs:135-157`；`runtime/rf-runner/src/main.rs:616-645` | 部分一致 | 平台及 OCI index 校验存在，当前只接受一个 manifest；未来 arm64 认证不判作当前缺陷，跨平台实际执行未验证。 |
| R18 §41 L2600–2629：规则到代码，默认只上传治理信息 | `runtime/rf-runner/src/main.rs:1561-1568`；`control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:227-230` | 部分一致 | 本地 repo 绑定和 Store 公共拒绝源码字段是真实强制点；禁止名单不足以限制任意字段携带源码。 |
| R19 §42 L2635–2646：版本化获取 Policy/Bundle/Lock/兼容/撤回/可选 Baseline | `control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:225-278`；`runtime/bundle-cache/src/lib.rs:50-84` | 部分一致 | /v1 registry 和 digest delivery 真实；descriptor 有撤回/期限身份，未逐个证明全部资产解析链。 |
| R20 §42 L2648–2662：默认 digest-only、Findings opt-in、path/symbol allowlist | `control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:406-429`；`control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:183-184,227-230` | 部分一致 | Feedback/Exception 闭合 digest-only；公共 Store 递归禁止名单存在，但普通 SCAN payload 不是同样闭合 schema，未见统一字段脱敏/许可策略。 |
| R21 §42 L2664–2675：version/schema/digest/signature/idempotency/retry/offline | `runtime/bundle-delivery/src/lib.rs:51-109`；`control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:225-232` | 部分一致 | digest GET、cache-first、幂等摘要真实；完整后向兼容治理、网络重试策略未验证。 |
| R22 §40 图 L2385–2387：HTTPS/Bundle Protocol | `runtime/bundle-delivery/src/lib.rs:218-220`；`runtime/thin-client/src/transport.rs:35-90` | 部分一致 | thin-client 有 TLS 配置，但 Bundle Delivery 专用通道仅接受 http://；两者不能混算。 |
| R23 §43 L2681–2710：Bundle 不变/寻址/签名/版本/离线/镜像/cache | `runtime/rule-bundle/src/lib.rs:165-206`；`runtime/bundle-cache/src/lib.rs:206-235` | 部分一致 | manifest 和缓存 digest 核验、离线复用真实；建议目录格式非硬要求，不按目录名机械判缺失。 |
| R24 §43 L2712–2716：signing role/key/授权审计隔离 | `runtime/trust/src/lib.rs:620-622`；`runtime/packs/src/lib.rs:53-71` | 部分一致 | 按 role 约束 key，Pack role 独立；KMS/HSM 权限与审计是外部部署事实，未验证。 |
| R25 §43 L2718–2722：完整 Root/delegation chain、namespace/freshness/revocation | `runtime/trust/src/lib.rs:625-641`；`runtime/trusted-ci/src/lib.rs:123-125` | 部分一致 | 有 version/expiry/key 和 asset 撤回，CI 拒绝缺失 metadata；模型未表达完整签名委派链、namespace 或轮换范围。 |
| R26 §43 L2724–2726：Root 三身份 2-of-3、日常 HSM/双人审批 | `runtime/trust/src/lib.rs:620-629`；`runtime/bootstrap/src/lib.rs:69-75` | 未实现 | 核心 trust/bootstrap 限定检索 threshold/rotation 未找到实现证据；单签 TrustStore 不等于生产阈值，外部审批亦未验证。 |
| R27 §43 L2728–2732：checkpoint 持久化 highest-seen、防过期回滚丢失 | `runtime/trust/src/lib.rs:937-965`；`runtime/trust-state/src/lib.rs:208-255` | 部分一致 | repository/version/TTL/role floor 和必需状态校验存在；阻止 Runner 写入以及审计恢复未证明。 |
| R28 §43 L2734–2739：各类资产 DSSE ArtifactStatement + chain 双验 | `runtime/trust/src/lib.rs:218-294`；`runtime/rule-bundle/src/lib.rs:188-206` | 部分一致 | 通用 Statement 有实现；Bundle 此处直接签 canonical manifest，不足以证明每类资产 Statement + 完整 chain。 |
| R29 §43 L2741：不依赖 Server 实时拼装规则 | `runtime/bundle-delivery/src/lib.rs:76-109`；`runtime/bundle-cache/src/lib.rs:223-235` | 一致 | 优先缓存，只有 Connected miss 才 fetch。 |
| R30 §43.1 L2745–2751：Ontology 独立签名/撤回/精确 digest/内嵌与兼容 | `runtime/rule-bundle/src/lib.rs:184-185`；`control-plane/ontology/src/main/java/io/ruleops/ontology/OntologyContractPack.java:7-13` | 部分一致 | 独立 content identity 和 manifest digest 引用存在；内嵌、独立验签与全语义兼容链未找到本次实现证据。 |
| R31 §43.2 L2755–2772：独立 Backend OCI Pack，启动前验证，不本机替代 | `runtime/rf-runner/src/main.rs:559-600`；`runtime/packs/src/lib.rs:97-144` | 部分一致 | 交付验证/测量存在；开发路径仍可指定 host，应与本地无权威标记一起理解，不代表所有路径均生产锁定。 |
| R32 §43.2 L2774–2777：RuntimeDependencyPack 传递闭包全锁定/验证 | `runtime/rf-runner/src/main.rs:769-785`；`runtime/packs/src/lib.rs:16-29` | 未实现 | Lock 是单 backend/resolver 结构；限定检索 RuntimeDependency/runtime_dependency/dependency_pack 未找到闭包模型或遍历验证。 |
| R33 §43.2 L2779–2783：开发系统 runtime 仅本地，合包不合身份 | `runtime/rf-runner/src/main.rs:1556-1559,1708`；`runtime/bundle-cache/src/lib.rs:56-84` | 部分一致 | 指定 Host 后普通 scan 仍本地；Pack 分开记录身份，完整运输格式未验证。 |
| R34 §43.3 L2787–2790：独立签名 WorkspaceResolverPack | `runtime/rf-runner/src/main.rs:579-580`；`runtime/trusted-context/src/lib.rs:153-157` | 部分一致 | 后端/Resolver 不同 asset type、分别锁定；全部运行依赖/SBOM/provenance 实物未验证。 |
| R35 §43.4 L2794–2804：Lock 冻结每 Backend/依赖/协议/schema/沙箱/Runner/Suite | `runtime/rf-runner/src/main.rs:769-785`；`runtime/rf-runner/src/main.rs:800-823` | 部分一致 | 单 Backend/Resolver/Runner/Suite/沙箱/major 存在；多 Backend、依赖闭包和完整 schema 版本集合不足。 |
| R36 §43.4 L2806–2810：Control Plane 确定性签锁，Runtime 不补解析 | `runtime/rf-runner/src/main.rs:788-823`；`runtime/trusted-ci/src/lib.rs:126-139` | 部分一致 | 确定性编码、signed lock、CI 输入一致性存在；控制面发布解析/签发全链未找到本次证据。 |
| R37 §43.4 L2812–2816：OCI layout/manifest digest，Bundle 不强制 OCI | `runtime/rf-runner/src/main.rs:616-645`；`scripts/production-runtime-pack-check.sh:281` | 部分一致 | layout/index 与 manifest digest 有校验；不能因此忽略 RuntimeDependencyPack 缺口。 |
| R38 §44 L2822–2849：Server 故障但缓存有效可继续 | `runtime/bundle-delivery/src/lib.rs:76-109`；`runtime/bundle-delivery/src/lib.rs:722` | 一致 | cache-first 路径实际实现，离线命中测试本次通过；不是生产全部缓存已经预热的证明。 |
| R39 §44 L2851–2858：过期/撤回未知/身份失效 -> INCONCLUSIVE | `runtime/bundle-cache/src/lib.rs:190-235`；`runtime/bundle-cache/src/lib.rs:250-255` | 一致 | 时间、验证失败和缓存损坏 fail closed，证据布尔值真实性依赖上游校验。 |
| R40 §43/44：撤回校验不只是缓存开关 | `runtime/rf-runner/src/main.rs:520-555`；`runtime/trust/src/lib.rs:637-695` | 一致 | snapshot 内容 digest 绑定并检测资产撤回，不能误报为只有布尔开关。 |
| R41 §45 L2864–2877：三 Deployment Profile | `runtime/bundle-cache/src/lib.rs:30-35`；`runtime/bundle-cache/src/lib.rs:223-228` | 一致 | 三种 profile 枚举和 cache miss 差异策略真实；企业镜像部署未验证。 |
| R42 §45 L2879–2893：离线 --bundle 仍验证并区别 trust | `runtime/rf-cli/src/main.rs:1390`；`runtime/rf-runner/src/main.rs:1708` | 部分一致 | CLI 走共享 scan，本地有明确标记；仅凭参数不能认证 .rfbundle 物理导入及完整签名链。 |
| R43 §46 L2899–2907、2934–2939：三个 trust level，本地不能分支证明 | `runtime/rf-runner/src/main.rs:1708`；`runtime/trusted-context/src/lib.rs:123-157` | 部分一致 | 本地标记和可信验证存在；CONTROLLED_RUNNER 独立认证流程本次未找到证据。 |
| R44 §46 L2909–2932：Gate 验证完整 identity 清单 | `runtime/trusted-context/src/lib.rs:183-255`；`runtime/trusted-ci/src/lib.rs:208-246` | 部分一致 | repo/commit/policy/lock/Runner/Pack/workspace/baseline/Sandbox 绑定真实；完整 base/merge-base、ontology、host/engine、measurement identity 独立闭环未逐项证成。 |
| R45 §46 L2941–2944：独立 Publisher 复验，SCM 最小凭证隔离 | `runtime/status/src/lib.rs:339-361`；`scripts/scm-status-adapter-github.py:1` | 部分一致 | 验签、repo/commit/policy/freshness 存在，另有适配器；脚本存在不等于凭证最小权限和进程隔离已部署。 |
| R46 §46 L2946–2949：固定映射/幂等/单调防旧覆盖 | `runtime/status/src/lib.rs:36-40`；`runtime/status/src/lib.rs:363-394` | 一致 | INCONCLUSIVE -> ActionRequired，subject/sequence/time 冲突拒绝；不扩张为跨进程水位部署已验证。 |
| R47 §47 L2955–2972：Factory/Runner 隔离，生产不消费临时 AI Candidate | `runtime/verification-worker/src/lib.rs:176-207`；`runtime/rf-runner/src/main.rs:98-105` | 部分一致 | Worker 有独立 sandbox proof；开发 scan 可以吃 Candidate，生产 Release-only 全入口强制仍需完整发布链证据。 |
| R48 §48 L2978–3017：薄配置/共用 Runner/本地标记 | `runtime/project-config/src/lib.rs:54-91`；`runtime/rf-runner/src/main.rs:1708` | 部分一致 | 配置模块和本地标记真实；示例 policy.profile/version 自动 Server resolve 未逐字段验证。 |
| R49 §49 L3023–3054：Bootstrap -> scan/Baseline/Gate -> Publisher | `runtime/trusted-ci/src/lib.rs:114-139`；`runtime/trusted-ci/src/lib.rs:165-246` | 部分一致 | scan/Gate/attestation 实际组合；Bootstrap 到真实 binary 启动段不闭合，不能整体认定 CI_TRUSTED 生产交付。 |
| R50 §50 V0.1 L3064–3101：CLI/Host/协议/Pack/Resolver/沙箱/信任/缓存薄纵切 | `runtime/trusted-ci/src/lib.rs:114-139`；`workspace-resolvers/static/src/lib.rs:1,43-44,135` | 部分一致 | 有真实纵切与 Maven/Gradle SAFE_STATIC；运行时必选项仍有 R05/R10/R12/R13/R25/R32 缺口。作者侧 IR/Compiler/Orchestrator/FTS/完整 Verification 留对应域，不假称重验完成。 |
| R51 §50 L3103–3104：一条规则一个 repo、Week 4 平台限制 | 未找到可独立验证的历史交付序列证据（本次只有当前 HEAD） | 无法验证 | 当前代码不能证明当时实现先后或验收日期，不据已有 Server 反推违反时间计划。 |
| R52 §50 V0.2 L3106–3117：Server Web/API 与治理能力 | `control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:79,234-278`；`control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java:31-42` | 部分一致 | API/Registry/模型真实；完整 Web UX 和全部生产服务未全面验收。 |
| R53 §50 V0.3 L3119–3125：两类 IDE/explain/feedback/exception UX | `clients/intellij/src/main/java/io/ruleops/ide/RfInvoker.java:73`；`clients/vscode/src/rf-invoker.js:35` | 部分一致 | 薄客户端已实现，GUI 未验收；“IDE 不进首关键路径”是历史安排，不能因当前有插件判违规。 |
| R54 §51 L3131–3147：最终形态为 V1 基线，不留白 | `control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java:79`；`runtime/trusted-ci/src/lib.rs:114-139` | 部分一致 | 三层成立、缓存和本地性有强制点；安全/信任闭环仍有差距，不能整体标未实现或全部一致。 |

## 问题清单（按严重度）

### 高：生产安全/信任

1. **P1：独立 Bootstrap 未闭环（R05–R07/R49）。** 库接收调用者字节和 TrustStore，公开 RunnerMeasurement 可构造；示例测量字符串是夹具。缺真实 binary 测量启动、独立分发、受保护 anchor/state 的证据。这是生产保证缺口，不是已经实测伪造远端 PASS。
2. **P1：完成协议缺 Observation count/attempt digest（R10）。** Schema 无字段，Runtime 只有本地 transcript；不能通过补测试把不同协议宣称符合。
3. **P1：staging 权限与删除失败（R12）。** temp_dir/default umask 不等于 Runner-only，remove_file 失败被吞；其他用户是否能读取取决于实际 umask/父目录，本次不写成已发生泄露。
4. **P1：完整 Root/委派/阈值/轮换未实现（R25–R28）。** expiry/revocation/checkpoint 真实存在，但单签 role 映射不是生产 2-of-3。Statement 类型存在也不证明全资产使用。
5. **P1：上传隐私规则不统一（R18/R20）。** Store 会递归拒绝 source/sourceCode/sourceContent/sourceSnippet/fullFinding/findings；未知字段、普通 path/symbol 或任意文本不受同等闭合约束。Feedback/Exception 更严格，不能误报为完全无防护。

### 中：身份/分发/确定性

6. **P2：CanonicalFindingSet 编码相悖（R13）。** 自定义串接与 delivery identity 不等于 deterministic Protobuf，影响跨实现语义摘要互操作。
7. **P2：传递依赖闭包缺失（R32/R35）。** 单 Backend/Resolver digest 不能证明 RuntimeDependencyPack 整体锁定、撤回和可用性。
8. **P2：Bundle Delivery HTTP-only（R22）。** thin-client HTTPS 不覆盖另一 crate；签名也不替代传输要求。
9. **P2：Ontology 独立验签/内嵌及控制面签锁证据不足（R30/R36）。** 已有 digest 引用，仍不能认定独立生命周期和生产全链。

### 低 / 验收边界

10. **P3：资源/库存在不等于生产部署验收（R02/R43/R45/R50–R53）。** Analytics、Controlled Runner、SCM 凭证、GUI、历史时序应交对应域或部署验收，不据本次静态检索宣称根本不存在。

## 整改建议

1. 优先建立独立 Bootstrap 的实际 binary 测量/启动和受保护 anchor/state；生产认证证据与 fixture 分开。
2. 在 Host Contract 一个权威位置补齐 identity/count/transcript 语义，统一 Host/Runtime/conformance；不匹配丢弃 staging，禁止各调用方分别打补丁。
3. staging 使用私有目录/卷、显式最小权限/配额；删除失败可观测，不增加隐藏 debug 留存 fallback。
4. Server 所有 Runner 上传入口统一闭合 schema；Findings、path/symbol 由明确允许策略控制，测试嵌套与未知字段负例。
5. 补完整 Root/delegation/阈值/轮换、资产 Statement 与依赖闭包、Ontology 独立验证，再认证生产 profile。
6. CanonicalFindingSet 明确升级版本，不在旧版本静默换编码；Bundle 通道补 TLS，保留已经实现的 cache-first/fail-closed。

## 证据边界与验证

- 使用精确路径、rg 有界检索；无可用语义索引，未建索引、未分派子代理。每声明仅 1–2 处证据。未找到只约束于已查模块，不等于全仓彻底穷尽。
- 重点模块：`runtime/{rf-runner,rf-cli,bundle-cache,bundle-delivery,bootstrap,trusted-context,trusted-ci,gate,status,packs,trust,trust-state,rule-bundle,verification-worker}`、Server、Ontology、clients、proto 和少量对应 scripts。
- 缺失检索词：`RuntimeDependency|runtime_dependency|dependency_pack`、`threshold|rotation|rotate_root`、`privacy|source|snippet|allowlist`、`bootstrap|verify_and_measure`；未做无界追索。
- **实际验证**：`timeout 60s /home/kali/.cargo/bin/cargo test -p erf-bundle-cache -p erf-bundle-delivery -p erf-bootstrap -p erf-trusted-context -p erf-status --lib --quiet`，退出 **0**；五包分别 **4/13/6/9/5**，合计 **37 passed**。缓存子进程重入 1 项不重复计数。
- 初次裸 cargo 因非交互 PATH 返回 127，改用已存在绝对路径成功，无安装或源码修改。测试不证明 OCI 实机隔离、KMS、SCM 权限、GUI 或生产 trusted execution；没有用 fixture PASS 替代生产证明。
- **未跑 `bash scripts/check.sh` 全量门禁**：专项只读审计有 25 分钟硬限，只执行上述 60 秒有界测试及报告检查。本报告不构成全量发布/验收通过声明。
- §50 作者侧细项不重做其他八域；历史时序、部署认证、生产签发、完整跨平台矩阵未验证。无源码/docs/scripts 改动，无 GitHub/Slack 操作。
