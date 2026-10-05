# 域 6/9：扫描与判定规格符合度审计

## 域范围

- 审计对象：WSL `kali-linux2` 中 `/home/kali/文档/ruleops` 的当前工作树，不是 Windows 同名工作树。
- 规格：`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md`，§19–§23.2，L1015–1299；使用 `nl -ba … | sed -n` 分段精读，L1301 的 §24 不纳入本域。
- 日期：2026-10-05。方式：只读源码、契约与现有测试；仅在 `.scratch/spec-audit/` 写审计产物。
- 收尾约束：收到协调者“立即落盘（第二次，强制）”后停止进一步核验。本报告是已核查证据的限时交付，不是全域通过证明。
- 下表规格行号均指上述规格文件；代码路径相对于 WSL 仓库根。判定为“一致 / 部分一致 / 不一致 / 未实现 / 无法验证”。“无法验证”不等于没有实现。

## 逐声明核验表

| ID / 声明（规格原文定位） | 代码实证（每声明至多两个关键落点） | 判定 | 理由 |
| --- | --- | --- | --- |
| S19-01：Scan 输入包含 Snapshot、WorkspaceDescriptor、RuleBundle、OntologyContractPack、ExecutionLock、Policy、TrustedContext、Baseline（§19 L1017–1028） | 定位到 `runtime/rf-runner/src/main.rs:1032` 的 `scan(&ScanConfig)`；未逐项读完 ScanConfig 与调用链 | 无法验证 | 仅定位函数不能证明八类输入全部绑定，尤其 OntologyContractPack 与可信身份链。 |
| S19-02：输出 Findings、CompletenessReport、ExecutionEvidence（§19 L1030–1036） | `contracts/http/scan-output-v1.yaml:11–33`；`runtime/gate/src/lib.rs:235–245` | 部分一致 | 有 findingRecords / findings、completeness、semanticResultDigest；GatePreview 不是完整 ScanResult，未验证 ExecutionEvidence 的完整输出契约与落盘。 |
| S19-03：Scan 命令本身不决定 PASS/FAIL（§19 L1038） | `runtime/rf-cli/src/main.rs:1433`；`contracts/http/scan-output-v1.yaml:14–16` | 部分一致 | scan 输出契约明确携带三态，CLI 调用共享 preview。尚未完整读命令分派，无法判断这是独立 Gate 投影还是 Scan 命令承担裁决职责。 |
| S20-01：Finding 字段包括 ruleId、ruleRelease、scenario、location、symbol、message、severity、fingerprint、baselineState、backend/engineVersion（§20 L1044–1069） | `runtime/gate/src/lib.rs:211–223`；`contracts/http/scan-output-v1.yaml:80–93` | 部分一致 | 有 rule_id、location、message、fingerprint；核心 Finding 未包含 ruleRelease、scenario、severity、engineVersion，也无独立 symbol owner/name 字段，外部 FindingRecord 必填项同样不完整。其他旁路字段是否补齐未验证。 |
| S20-02：BaselineState 为 NEW / EXISTING / RESOLVED / SUPPRESSED / AMBIGUOUS（§20 L1071–1079） | `runtime/gate/src/lib.rs:15–16`；`runtime/gate/src/lib.rs:577–581` | 不一致 | 实际枚举只有 New / Historical，分类仅做 fingerprint 精确集合命中；不能表达后三类状态，EXISTING 也以不同名称表示。 |
| S20-03：Host 产生 policy-neutral BackendObservation，Runtime 形成 Finding（§20 L1081–1086） | `contracts/proto/backend_host.proto:215–223`；`runtime/gate/src/lib.rs:621–653` | 部分一致 | 有 BackendObservation，engine_severity 注释明确仅诊断，Rust normalize 形成 Finding；未见该函数绑定 RuleRelease / Scenario。 |
| S20-04：Host 不得决定 blocking/advisory、Gate 或 Baseline；Runtime 不解析引擎私有报告（§20 L1088–1089） | `runtime/gate/src/lib.rs:548–590`；`contracts/proto/backend_host.proto:215–223` | 部分一致 | 已读 Gate 只消费 protobuf observation 并在 Runtime 判断 blocking；未扫描所有 Host 和 Runtime 的私有格式解析路径，不能作全局排除性结论。 |
| S20-05：fingerprint 记录算法版本（§20.1 L1093） | `runtime/gate/src/lib.rs:9`；`runtime/gate/src/lib.rs:651–653` | 一致 | 常量 finding-v0 既写入身份输入与 fingerprint 前缀，也保留 fingerprint_version 字段。 |
| S20-06：主身份包含 RuleIntent、Scenario、规范化仓库相对路径、semantic anchor（§20.1 L1093–1100） | `runtime/gate/src/lib.rs:621–653` | 不一致 | 哈希输入只有版本、rule_id、anchor，没有 Scenario；Symbol/Bytecode 分支将 owner/class 当作 path，也未绑定仓库相对路径。不同场景或不同文件相同符号可能无法区分。 |
| S20-07：severity、message、RuleRelease / Backend 版本不得进入主身份（§20.1 L1102–1103） | `runtime/gate/src/lib.rs:651–653` | 一致 | 已读 fingerprint 算法未把这些表现或版本字段纳入主身份；此结论仅针对当前 finding-v0 算法。 |
| S20-08：优先规范化 symbol/AST/bytecode；没有时显式 location fallback（§20.1 L1103–1104） | `runtime/gate/src/lib.rs:624–647` | 部分一致 | 支持 Symbol/Bytecode/SourceSpan/File，并带 location_kind；但按输入 oneof 直接选分支，没有“优先语义锚”选择或显式 fallback 状态。SourceSpan 主身份含 byte offset，会随前置代码插入而变化。 |
| S20-09：move/copy 需解释性映射；多对多或低置信为 AMBIGUOUS，不得并入债务（§20.1 L1106–1107） | `runtime/gate/src/lib.rs:15–16`；`runtime/gate/src/lib.rs:577–581` | 未实现 | 在已读 Gate 路径中没有 AMBIGUOUS 可表示状态，也无重映射输入；未找到本路径实现证据，不宣称全仓不存在迁移工具。 |
| S20-10：fingerprint 算法版本变更需迁移映射与同 commit double scan（§20.1 L1107–1108） | 已定位 `runtime/incremental-baseline/`，未继续核查版本迁移入口 | 无法验证 | 时间收尾，不能从当前版本常量推导版本迁移已实现或未实现。 |
| S21-01：完整性按 rule × module/file analysis unit 表达（§21 L1114–1118） | `runtime/scan-plan/src/lib.rs:299–325`；`contracts/proto/backend_host.proto:139` | 部分一致 | 按 planned unit ID 逐项记账；PlannedUnit 的具体 rule/module/file 身份构成尚未完整核验。 |
| S21-02：状态 COMPLETE / PARTIAL / FAILED / NOT_APPLICABLE（§21 L1120–1127） | `runtime/scan-plan/src/lib.rs:341–348`；`runtime/scan-plan/src/lib.rs:408–412` | 一致 | 有完整四态枚举及汇总计算分支；单元 NotApplicable 会计入失败，而零 planned units 的汇总才为 NotApplicable，策略适用性另见边界。 |
| S21-03：0 findings 不等于完整（§21 L1129–1139） | `runtime/gate/src/lib.rs:558–576`；`runtime/gate/src/lib.rs:679–684` | 一致 | 完整性取决于 planned/accounted/failed/missing/observation_errors，不依赖 findings 数量；现有测试覆盖零 finding 且缺单元为 INCONCLUSIVE。测试源码证据，不冒充本次运行通过。 |
| S21-04：缺依赖、stale bytecode、timeout、crash、parse failure 都进入 Completeness（§21 L1141–1142） | `runtime/gate/src/lib.rs:542–546,568–576`；`runtime/scan-plan/src/lib.rs:326–354` | 部分一致 | 结构化 observation_errors 与失败单元进入完整性；未逐一追踪五类外部故障到此的映射，因此不能保证全部不丢失。 |
| S21-05：冻结分母、plan digest，每 planned unit 必须且只能一个终态（§21 L1144–1147） | `runtime/scan-plan/src/lib.rs:278–283,299–325`；`runtime/gate/src/lib.rs:558–567` | 部分一致 | ScanPlan 记账拒绝未知/重复单元并计算 missing；另一个 Gate preview 记账实现却忽略未知 ID、去重重复 ID。尚未验证所有入口是否强制先经过 ScanPlan 校验，存在双重真相源。 |
| S21-06：排除/跳过/降级保留结构化 reason（§21 L1146–1147） | `runtime/scan-plan/src/lib.rs:350–355`；`runtime/scan-plan/src/lib.rs:401–405` | 部分一致 | 有缺失、Unsupported、NotApplicable 等聚合 reason；account.reason 是 Option，所读 account 函数未把该单元 reason 逐项复制到报告，完整保真尚未验证。 |
| S21-07：WorkspaceResolver 静态/可信 metadata，缺事实进入 planning，Host 不自行推断，bytecode 绑定可信 digest（§21 L1149–1157） | 定位 `runtime/rf-runner/src/main.rs:1269–1292`，未读完整分支 | 无法验证 | 未对 resolver、backend 及 bytecode provenance 建立完整链；不以命名作为实现证明。 |
| S21-08：SAFE_STATIC / ATTESTED_BUILD_EXPORT 两模式，模式进入 Descriptor/Evidence，Host 不启动构建（§21 L1159–1168） | `runtime/gate/src/lib.rs:542–546` 说明 SAFE_STATIC 缺事实计入 observation_errors；其余未核验 | 无法验证 | 只有注释与通用错误计数不足以证明两模式执行约束、证据绑定和禁止构建。 |
| S21-09：ResolverPack 签名/内容寻址/精确 digest 选择/独立进程沙箱（§21 L1170–1173） | 定位 `runtime/resolver-client/`、`workspace-resolvers/static/`，未继续读取 | 无法验证 | 已到收尾界限，未核查。 |
| S21-10：WorkspaceDescriptor envelope 绑定配置、工具链、输出等身份；可信构建需独立 WorkspaceAttestation（§21 L1175–1180） | 定位 `runtime/rf-runner/src/main.rs:1286–1292` 的 attestation.verify 调用 | 无法验证 | 只证明存在验证调用，未读取验证材料范围与 descriptor 字段。 |
| S21-11：WorkspaceAttestation 为 DSSE in-toto，校验签名、predicate、subject、身份和 freshness（§21 L1182–1187） | 未完成 `runtime/trust/` WorkspaceAttestation 验证实现核验 | 无法验证 | 不把 GateAttestation 的同类测试外推为 WorkspaceAttestation 已通过。 |
| S21-12：工作负载身份绑定 issuer/audience/repo/ref/workflow/run/commit/lock/time，禁止重放；离线专用 scope（§21 L1189–1192） | 未完成 Workspace workload identity 与离线签名器核验 | 无法验证 | 此安全声明需要独立 trust/verifier 链，未找到足以判定的已读证据。 |
| S21-13：Host 返回 deterministic BackendExecutionPlan，不得改变 planned unit 可追溯分母（§21 L1194–1197） | `runtime/scan-plan/src/lib.rs:278–283` | 部分一致 | 有执行 ID 与冻结 planned ID 完全相等的校验；未核验 Host BackendExecutionPlan 的生成、确定性和批次细化协议。 |
| S21-14：UNSUPPORTED/NOT_APPLICABLE/error 逐项返回；成功但少覆盖仍不完整（§21 L1199–1201） | `runtime/scan-plan/src/lib.rs:299–355`；`runtime/gate/src/lib.rs:558–576` | 一致 | 已读汇总路径将未执行/缺单元均计入不完整，不会因 Host 表面成功或零发现转为 complete；上游 reason 来源未全量验证。 |
| S22-01：Debt / Comparison / QualityBenchmark 不可混用（§22 L1207–1213） | `runtime/gate/src/lib.rs:21–30`；`runtime/gate/src/lib.rs:577–580` | 部分一致 | 类型枚举明确，但 preview 只读取 fingerprints，未检查 kind：直接传 QualityBenchmark 也会成为 Historical。外层是否禁止这种输入尚未验证，不能把类型存在视为不混用保证。 |
| S22-02：升级同源 double scan 并产生 Impact Delta（§22 L1215–1223） | 未完成 `runtime/incremental-baseline/` 的升级路径读取 | 无法验证 | 没有本次已读证据可证明完整升级流程。 |
| S22-03：Debt 中央批准/签名/不可变，绑定 repo snapshot、Policy、RuleRelease（§22 L1225–1226） | `runtime/gate/src/lib.rs:24–26`；`runtime/gate/src/lib.rs:59–78` | 部分一致 | 有签名接口，但 Baseline 本体及签名 payload 仅包含 kind 与 fingerprint 集合，不含三类绑定身份。外层资产是否强绑定尚未核验，存在跨上下文复用风险。 |
| S22-04：Comparison 来自可信 merge-base；升级经接受 Impact Delta 才发布新 Debt（§22 L1226–1228） | 未完成 SCM 与治理发布调用链核验 | 无法验证 | 不凭 `runtime/git-scope/`、`runtime/incremental-baseline/` 目录名作实现结论。 |
| S23-01：Gate 组合 Policy、Findings、Completeness、Baseline、Exception、TrustedContext（§23 L1234–1242） | `runtime/gate/src/lib.rs:548–592`；`runtime/trusted-ci/src/lib.rs:165` | 部分一致 | 本地核心含前五类，固定 LOCAL_UNATTESTED；Trusted CI 调用同一 preview，但外层 TrustedContext 完整校验未读完。 |
| S23-02：BaselineState 与 ExceptionDecision 独立，豁免不得删除 Finding/改 fingerprint/覆盖 baseline（§23 L1244–1246） | `runtime/gate/src/lib.rs:577–590`；`runtime/gate/src/lib.rs:694–702` | 一致 | 两种 assessment 独立计算，原 findings 保留；现有测试明确例外不隐藏 finding，也不能掩盖不完整。 |
| S23-03：报告与 GateAttestation 同时保留 BaselineState、ExceptionDecision 和匹配 grant digest（§23 L1246–1247） | `runtime/gate/src/lib.rs:243,248–272`；`runtime/gate/src/lib.rs:577–581` | 部分一致 | preview 有 assessments，attestation 只有全局 exception_grant_digests，无逐 finding baseline/exception/grant 映射；与 §23.2 默认不嵌 finding 的隐私约束需通过受控报告/摘要设计对齐。 |
| S23-04：PASS 需 complete、trusted、无未豁免新 blocking（§23.1 L1251–1255） | `runtime/gate/src/lib.rs:575–592` | 部分一致 | preview 保证 complete 与无新 blocking，但允许 LOCAL_UNATTESTED 的 preview PASS；规格 L1273 允许本地 preview，不能单凭此指控授权绕过。是否只有经验证结果可作权威证明未完整核验。 |
| S23-05：确定的新未豁免 blocking 为 FAIL（§23.1 L1257–1259） | `runtime/gate/src/lib.rs:583–590` | 部分一致 | complete 且存在新 blocking 为 FAIL；若同时 incomplete，代码优先 INCONCLUSIVE。L1263 与 L1268 对“已知阻断+缺覆盖”存在优先级张力，应明确真值表后裁决，不能直接称其违反全部原文。 |
| S23-06：无确定 blocking，但不完整/不可比较/不可信为 INCONCLUSIVE（§23.1 L1261–1264） | `runtime/gate/src/lib.rs:575–592`；`runtime/scan-plan/src/lib.rs:328–332` | 部分一致 | 不完整路径 fail-closed；可比较性和身份可信不在 preview 入参中，需外层信任/基线链证明，尚未完成。 |
| S23-07：严格三态；错误分类不是第四态（§23.1 L1266–1270） | `runtime/gate/src/lib.rs:12–13`；`contracts/http/scan-output-v1.yaml:14–16` | 一致 | GateDecision 只有三态；status/authority 单独承载 LOCAL_UNATTESTED，未混为第四个 GateDecision。 |
| S23-08：只有 Policy 显式 non-required/advisory 缺失可 PASS，带 coverage.degraded 与 reason（§23.1 L1266–1269） | `runtime/gate/src/lib.rs:201–205`；`runtime/gate/src/lib.rs:575–590` | 未实现 | 已读 GatePolicy 仅控制 blocking_rule_ids，不表达 required/advisory coverage；任何缺失均 INCONCLUSIVE，未找到 degraded PASS 支持。这是功能缺口，不是误放行。 |
| S23-09：INCONCLUSIVE 不得被接受为成功（§23.1 L1261–1275） | `runtime/status/src/lib.rs:36–40,354–360`；`runtime/rf-cli/src/main.rs:1574–1584` | 部分一致 | SCM 映射明确 Inconclusive → ActionRequired，非 Success；publisher 拒绝 incomplete PASS。CLI 已定位包含 Inconclusive 的 exit 分支，但未完整读取返回值，故不宣称全部 CLI/CI 入口已证明非零。 |
| S23-10：本地/CI/Runner 共用确定性引擎，本地仅 LOCAL_UNATTESTED，权威证明需全部身份验证（§23.1 L1272–1275） | `runtime/gate/src/lib.rs:592`；`runtime/trusted-ci/src/lib.rs:165` | 部分一致 | 本地固定未证明标签，CI 调同一个 preview；runner、service 所有消费入口的防绕过链尚未核验。 |
| S23-11：可信 Gate 输出 DSSE in-toto，subject 为 Decision/Evidence digest，predicate 绑定材料（§23.2 L1279–1282） | `runtime/gate/src/lib.rs:248–278`；`runtime/gate/src/lib.rs:814–836` | 部分一致 | 类型覆盖列举材料与三态，现有测试检查 in-toto 类型/predicateType；未完整读取 subject_digest、canonical_payload 和 DSSE 验证体，不能以测试源码宣称端到端证明完成。 |
| S23-12：Policy 允许的 workload 签名及 freshness/全部材料验证；exit code 非权威证明（§23.2 L1284–1287） | `runtime/gate/src/lib.rs:261–271`；`runtime/status/src/lib.rs:348–360` | 部分一致 | 有身份/时间字段，publisher 校验 repo、commit、policy 后调用签名验证；issuer/audience/ref/environment 全约束、材料校验和签名能力来源未完成核验。 |
| S23-13：Attestation 默认不内嵌 finding/path/symbol/message；报告独立受控且 digest 一致（§23.2 L1289–1292） | `runtime/gate/src/lib.rs:248–272`；`runtime/gate/src/lib.rs:814–836` | 部分一致 | 类型无 finding 明细字段，测试检查不泄漏源码路径；独立报告存储/上传 Policy 与 digest 一致性未核查。 |
| S23-14：在线单 digest 短期签名能力、禁长期 key；离线 signer/HSM 限 run/predicate/digest，禁任意签名（§23.2 L1294–1297） | 定位 `runtime/trusted-ci/src/lib.rs:233` 的 signer 调用，未读取签名器实现 | 无法验证 | Fn signer 接口的存在不足以证明密钥隔离和能力约束，未验证 Connected/Air-gapped 运行环境。 |

## 问题清单（按严重度）

### 高：已确认模型差异或核心路径风险

1. **F06-01：Finding 身份缺 Scenario，符号/字节码身份缺仓库相对路径。** `runtime/gate/src/lib.rs:624–653` 的 finding-v0 输入只有版本、rule_id、anchor；不满足 §20.1 L1093–1100，且 SourceSpan 把 byte offset 放入身份。可能导致跨场景合并及无关位置移动造成虚假 NEW；影响范围还需真实多 Scenario / 多 module 样本验证。
2. **F06-02：BaselineState 无法表达 AMBIGUOUS 等规范状态。** `runtime/gate/src/lib.rs:16,577–581` 仅 New/Historical，缺少可解释重映射及歧义状态。无法按规格实现低置信匹配的显式呈现与处理。
3. **F06-03：Baseline 输入类型的隔离没有在共享 preview 层强制执行。** `BaselineKind` 已存在，但分类忽略 kind；QualityBenchmark 可在此直接变成历史命中。外层可能有防线，尚未核验，因此记为“共享函数可达风险”，不是已证实的生产绕过。
4. **F06-04：Baseline 签名 payload 缺治理上下文。** `runtime/gate/src/lib.rs:59–66` 只签 kind/fingerprints；规格要求的 repository snapshot、Policy、RuleRelease 未在该资产本体绑定。需要确认外层是否不可绕过地补齐，否则存在跨上下文复用风险。

### 中：功能/契约差距与尚未封闭的不变量

5. **F06-05：Finding schema 不完整。** 核心结构与公开 FindingRecord 未完整表达 ruleRelease、scenario、severity、backend/engineVersion 等规格字段。
6. **F06-06：完整性有两套记账逻辑。** ScanPlan 拒绝未知/重复单元，Gate preview 忽略未知并去重；需要证明所有权威 Gate 必经严格记账，否则“每单元一个终态”的约束会依入口变化。
7. **F06-07：required/advisory 覆盖与 degraded PASS 未在已读 Gate 实现。** 当前选择保守 INCONCLUSIVE，不是放行漏洞，但不满足允许 advisory 缺失并附原因的规范能力。
8. **F06-08：attestation 未保留逐 Finding Baseline/Exception/grant 对应关系。** 当前只有全局 grant digest 列表；需在不破坏默认隐私限制的前提下，明确受控报告与 attestation 摘要绑定方式。

### 低 / 规格待裁决

9. **F06-09：“已知 blocking + required incomplete”优先级需明确。** 当前 INCONCLUSIVE 优先；§23.1 L1259/L1263 支持先认定 FAIL，而 L1268 又称任何 required 不完整都是 INCONCLUSIVE。应登记规格歧义并用真值表裁决，不能在审计中擅自确定优先级。

### 正向证据（不等于全域通过）

- 三态 GateDecision 真实存在，未混入配置/信任错误第四态。
- 零 findings 不推出 complete；required missing/failed/observation error 在已读共享函数中不能 PASS。
- Status Publisher 把 INCONCLUSIVE 映射为 ActionRequired，并显式拒绝 incomplete PASS。
- fingerprint 确实版本化且排除 message/severity/实现版本；豁免不删除 finding、不覆盖 baseline 分类。

## 整改建议

1. 首先统一产品级不变量：规范化 Finding 身份（含 Scenario 与 repo-relative path）、完整 BaselineState、基线用途隔离及身份绑定；在现有模型上补齐，不新增并行“兼容”分类层。
2. 保留 ScanPlan 为完整性唯一分母及终态记账入口，让共享 Gate 消费经核验 CompletenessReport；删除或收敛 preview 的重复记账实现。增加一个覆盖重复/未知/缺失单元的最小回归检查。
3. 为 Gate 真值表增加明确 required/advisory 与比较性维度，并先裁决“blocking + incomplete”优先级；保持 INCONCLUSIVE 在 CLI、SCM、protected-branch 消费者中均不得转 Success。
4. 补齐 Finding 输出契约及逐项评估/匹配 grant 的受控报告绑定；再核验 Workspace/Gate DSSE 的身份、material、freshness 与短期签名能力，不能仅用字段齐全作为安全完成标准。
5. 后续优先补查本报告标“无法验证”的信任链、double scan / Impact Delta、可信 merge-base、签名器隔离和全部 CLI 退出分支。此处仅提出审计后续，不修改源码、不自动开票。

## 证据边界与验收

- 首次访问 `kali-linux` 未找到标的；随后确认标的实际在 `kali-linux2`。所有判定引用均来自后者 `/home/kali/文档/ruleops`。没有将 Windows 或另一个发行版的源码当作证据。
- 规格按要求精读指定行段。源码检索限定 contracts、gate、scan-plan、rf-runner、trusted-ci、status、rf-cli 等直接相关路径；每条声明仅保留 1–2 处关键证据。没有执行无界全仓语义检索，也未创建索引。
- `runtime/rf-cli/src/main.rs` 含 NUL，普通 rg 将其视为 binary；已使用 `rg -a` 定位退出分支，未忽略该文件。尚未完整读取退出码分支，因此 CLI 非零守卫仍是部分验证。
- 采用 spec-to-code-compliance 的规格→代码→判定框架；本次运行未获得其专用 slash workflow 工具，也未启动独立反驳代理。结论不冒充经过独立交叉复核。
- 已尝试 `timeout 60s cargo test -p rf-gate -p erf-scan-plan -p erf-status --offline --quiet`；命令退出 1，未宣称测试通过。原始输出保存在 `.scratch/spec-audit/06-targeted-tests.log`，最终读取日志确认：timeout 无法运行 cargo（没有那个文件或目录）；即当前非交互 WSL shell 的 PATH 无可执行 cargo，测试未开始，并非测试断言失败。
- 未执行全仓 `bash scripts/check.sh`：本任务为只读、限时符合度审计，且协调者强制停止核验；因此不能给出项目全量门禁通过结论。
- 已读现有测试只能证明覆盖意图与断言存在，不能替代本次动态运行证据。未验证项明确列在表内，未使用“没读到”推定全仓未实现。
- 唯一交付为本 Markdown 报告和测试尝试日志；不改源码、docs、scripts，不发布 GitHub/Slack 消息。
