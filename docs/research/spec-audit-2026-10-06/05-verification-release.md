# 域 5：验证与发布——独立规格符合度审计

## 域范围

- 目标树：WSL `kali-linux2:/home/kali/文档/ruleops`；HEAD `7e7d2820de5f2948bd936b64f5907ae211f00645`。
- 权威规格：`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md`，§14–§18，L846–1013；按要求一次读取 L846–1015，§19 不纳入。
- 本报告独立重做，未打开任何既有 `05-*.md`；仅覆盖本报告，不修改源码、docs 或 scripts。
- 判定口径：一致=所引代码落实该条窄声明；部分一致=存在相关能力，但缺少完整约束或接线；不一致=实际模型/执行路径与声明冲突；未实现=限定检索未找到实现证据（不是全仓不存在的证明）；无法验证=现有静态证据不足以判定。
- 以下证据为静态源码审阅，不代表实际部署已通过验收。尤其不能把本地 `LOCAL_UNATTESTED`、Backend/Resolver Conformance、Rule Candidate 的独立验证三者等同。

### 证据路径缩写

每个 `缩写:Lx–Ly` 均指下列完整文件的行号；每条声明最多给出两个关键证据。

| 缩写 | 仓库相对路径 |
|---|---|
| LV | `control-plane/verification/src/main/java/io/ruleops/verification/LocalVerification.java` |
| EB | `control-plane/verification/src/main/java/io/ruleops/verification/EvidenceBundle.java` |
| CC | `control-plane/factory/src/main/java/io/ruleops/factory/candidate/CandidateCompiler.java` |
| CA | `control-plane/factory/src/main/java/io/ruleops/factory/candidate/Candidate.java` |
| RR | `runtime/release/src/lib.rs` |
| REG | `runtime/release-registry/src/lib.rs` |
| GOV | `runtime/governance-registry/src/lib.rs` |
| CF | `runtime/conformance/src/lib.rs` |
| VW | `runtime/verification-worker/src/lib.rs` |
| RB | `runtime/rule-bundle/src/lib.rs` |
| PACK | `runtime/packs/src/lib.rs` |
| DEL | `runtime/bundle-delivery/src/lib.rs` |
| CI | `runtime/trusted-ci/src/lib.rs` |
| GATE | `runtime/gate/src/lib.rs` |
| PILOT | `runtime/pilot-observer/src/main.rs` |
| STORE | `control-plane/server/src/main/java/io/ruleops/server/RuleOpsStore.java` |
| HTTP | `control-plane/server/src/main/java/io/ruleops/server/RuleOpsHttpServer.java` |

## 逐声明核验表

- 核验统计：56 条；一致 19、部分一致 26、不一致 4、未实现 6、无法验证 1；**不一致 + 未实现共 10 条**。

| ID / 声明（规格章节与行号） | 代码证据 | 判定 | 理由 |
|---|---|---|---|
| 01 §14 L848：Candidate 必须独立 Verification | LV:L25–34；VW:L255–285 | 部分一致 | 本地执行明确不具权威性；独立 worker 签的是运行时 conformance，不是 Candidate 全层语义验证。未见 Candidate 晋级接入独立验证结果。 |
| 02 §14.1 L852：Contract Validation | CC:L50–59；LV:L49–50 | 部分一致 | 编译前有 Intent 验证，验证时有内容摘要检查；未见独立 Lab 重新验证完整 Contract 并出具对应证据。 |
| 03 §14.1 L853：Build / Load Validation | LV:L64–80 | 部分一致 | 实际加载 PMD ruleset 并执行；只有本地 PMD 路径，不足证明所有实现的独立 build/load 层。 |
| 04 §14.1 L854：Positive Tests | LV:L83–104 | 部分一致 | bad 样例要求命中；仅检查存在的文件，没有必须存在正例类别的约束。 |
| 05 §14.1 L855：Negative Tests | LV:L83–104 | 部分一致 | good 样例要求无命中；未强制该类别非空，且这里只消费 violations。 |
| 06 §14.1 L856：Near-miss Tests | LV:L126–133；LV:L83–104 | 部分一致 | 识别 near-miss 并要求 clean，但没有该类别最低样本门禁。 |
| 07 §14.1 L857：Boundary Tests | LV:L126–133；CF:L32–64 | 未实现 | 未找到规则语义 Boundary 层实现证据；本地类别不含 boundary，协议边界检查不能替代规则边界样例。 |
| 08 §14.1 L858：Holdout Tests | CA:L53–56；LV:L83–104 | 部分一致 | 会执行 holdout；样例来自 Intent，尚未证明独立保留集隔离与必需存在。 |
| 09 §14.1 L859：适用时 Mutation Tests | LV:L126–133；CF:L32–64 | 未实现 | 未找到规则 mutation 生成、适用性判定及分数门禁；conformance 的 message 独立性测试不等于规则变异验证。 |
| 10 §14.1 L860：代表仓库 Shadow Scan | PILOT:L319；PILOT:L425–454 | 部分一致 | 有实际 scan 路径与 LiveShadow/覆盖率报告；未找到将代表仓库采样证据绑定到 Rule Candidate 晋级的接线。 |
| 11 §14.1 L861：Compatibility | CF:L32–48；RR:L177–180 | 部分一致 | 具协议兼容检查与 release 兼容身份比对；后者是标识校验，不是规则×引擎版本兼容矩阵执行。 |
| 12 §14.1 L862：Performance | PILOT:L431–438 | 部分一致 | 有 p95 延迟、峰值内存阈值与缺失时 INCONCLUSIVE；未见成为 Candidate Lab 必需或按风险追加的发布门禁。 |
| 13 §14.1 L863：Security / Isolation | VW:L255–266；VW:L231–243 | 部分一致 | worker 校验 sandbox proof 后执行套件，绑定隔离身份；未见覆盖每个 Rule Candidate 的安全验证结果。 |
| 14 §14.1 L864：Repeatability | CF:L491–506；LV:L76–81 | 部分一致 | 固定本地 observations 的十次 digest 检查存在；不是同一 Candidate 独立重跑并比较语义结果。 |
| 15 §14.2 L868–885：EvidenceBundle 关联 rule/implementation、分层证据及结论 | EB:L14–24；CF:L199–214 | 部分一致 | 本地 bundle 有 Candidate/engine/fixtures/verdict，conformance 有执行身份；没有规格所述统一的规则实现级分层 bundle。示例字段拼写本身不作为硬性错误。 |
| 16 §14.2 L887–890：VERIFIED 固定七项最低证据，不得削减 | REG:L102–118；RR:L159–175 | 不一致 | admission 验签与比较 evidence digest 后直接标 Verified，没有解读/验证七项最低规则证据。此 Verified 若仅指验签状态，则与规则成熟度 VERIFIED 尚未接通。 |
| 17 §14.2 L888–890：Policy 可按风险追加 Shadow/Precision/Performance/Security/审批阈值 | GOV:L24–30；PILOT:L425–451 | 部分一致 | pilot 有固定评估阈值；PolicyProfile 无风险阈值字段或晋级配置，未见不能削弱框架底线的组合规则。 |
| 18 §15 L896–911：DRAFT→CANDIDATE→VERIFIED→SHADOW→ADVISORY→BLOCKING→DEPRECATED→RETIRED | REG:L15–19；STORE:L633–659 | 不一致 | release registry 仅 Verified/Active/Revoked；API seam 允许 Candidate→Active，均不是成熟度状态机。不能将 API Active 等同 BLOCKING。 |
| 19 §15 L914–919：REJECTED、REVOKED 旁路状态 | REG:L15–19；REG:L144–162 | 部分一致 | Revoked 已有命令与状态；限定实现中未见 Rejected 成熟度分支。 |
| 20 §15 L921：BLOCKING 只能由治理晋级，不是生成默认值 | GOV:L82–107；REG:L115 | 部分一致 | 发布注册不默认称 Blocking，但 Profile 对 blocking_rule_ids 仅检查集合包含关系，没有验证规则已达 BLOCKING 成熟度。 |
| 21 §15 L923–924：显式可审计状态命令、验证源/目标 | REG:L122–141；REG:L212–221 | 一致 | 已实现命令对源状态、目标路径、操作序号检查并记录 actor/reason/digest/resulting_state；此结论仅限现有三态路径。 |
| 22 §15 L923–924：每次转换核验 Evidence、角色、审批 | REG:L122–141；REG:L225–228 | 不一致 | promote 只检查状态/序号/冲突，actor/reason 只判非空，没有证据、角色或审批输入。 |
| 23 §15 L924：Rule Author 不得独立 Candidate→Verified | REG:L91–118；VW:L267–280 | 未实现 | 未找到作者与验证者身份分离检查；worker 签名提供另一资产身份，但没有绑定 Candidate 作者并比较角色的晋级实现。 |
| 24 §15 L925：只有发布签名身份可产生有效 RuleRelease | RR:L12–15；RR:L191–198 | 一致 | verifier 要求 rule-release-signing 角色；本地构造/签名对象不等于成为受信有效资产。 |
| 25 §15 L925–926：只有 Policy Owner 可加入 Blocking Profile | GOV:L24–30；GOV:L82–107 | 不一致 | blocking 列表只校验规则存在，没有 Policy Owner 身份/规则成熟度校验；另有 policy-profile-signing 验签，但没有证据表明其等同 Policy Owner 授权契约。 |
| 26 §16 L936：Release 包含 RuleIntent | RR:L18–30；RR:L159–161 | 一致 | intent_digest 进入 manifest 并与期望值比对，内容寻址引用视为包含绑定。 |
| 27 §16 L937：Release 包含 RuleImplementation | RR:L18–30 | 部分一致 | 有 backend、artifact、intent，但缺少显式 implementation 身份/摘要，无法完整绑定实现描述。 |
| 28 §16 L938：Release 包含 RuleArtifact | RR:L18–24；RR:L186–189 | 一致 | 对实际 artifact 字节计算摘要并匹配 manifest。 |
| 29 §16 L939：Release 包含 Backend Identity | RR:L21–24；RR:L154–158 | 一致 | backend id/version 签入 manifest 并严格比对。 |
| 30 §16 L940：Release 包含 Backend Engine Lock | RR:L24；RR:L162–164 | 一致 | engine_lock_digest 被签入并验证。 |
| 31 §16 L941：Release 包含 Compatibility | RR:L29–30；RR:L177–180 | 一致 | compatibility_identity/platform 被签入并匹配；不据此声称兼容性测试已完成。 |
| 32 §16 L942：Release 包含 Evidence Bundle | RR:L26；RR:L168–170 | 部分一致 | 有 evidence_digest 绑定，但没有统一规则 Evidence Bundle 内容校验；完整性不足见 15、16。 |
| 33 §16 L943：Release 包含 Remediation Metadata | RR:L18–30 | 未实现 | manifest 未找到 remediation 元数据或其摘要；限定关键词检索也无绑定实现证据。 |
| 34 §16 L944：Release 包含 SBOM | RR:L27；RR:L171–173 | 一致 | SBOM 摘要进入 manifest 并匹配期望值；未审 SBOM 内容质量。 |
| 35 §16 L945：Release 包含 Provenance | RR:L28；RR:L199–205 | 一致 | provenance 摘要与签名 ArtifactStatement 绑定。 |
| 36 §16 L946–947：Release 包含 Signature 与 Manifest | RR:L55–80；RR:L182–198 | 一致 | canonical manifest、DSSE、ArtifactStatement 均存在，验证字节与签名。 |
| 37 §16 L932、950：语义/Artifact/Backend lock/Evidence 变化产生新不可变 Release | RR:L34–52；REG:L107–118 | 一致 |相关摘要进入 canonical payload，变化导致新 manifest digest；注册以 digest 存储、不覆盖旧条目。按资产内容身份判定，未要求版本字符串自动递增。 |
| 38 §17 L958：Registry 保存 immutable releases | REG:L67–74；REG:L107–118 | 一致 | digest 索引及冲突/幂等处理保护 release 内容；治理状态变化不改 signed release。 |
| 39 §17 L959：Registry 提供 metadata index | REG:L67–74；GOV:L223–230 | 部分一致 | 有 release digest/intent 与 profile id/version 索引；未证明统一、持久、完整 metadata 检索服务。 |
| 40 §17 L960：Registry 管 signatures | REG:L102–106；RR:L191–198 | 一致 | 入库前验证签名并保留 signed release。 |
| 41 §17 L961：Registry 管 compatibility | REG:L102–106；RR:L177–180 | 一致 | admission 调用 verifier 检查兼容身份；结论不扩展到兼容矩阵查询。 |
| 42 §17 L962：Registry 管 revocation | REG:L144–162 | 一致 | 显式撤销、移除活动索引并审计；消费端全链路传播时效未在本域动态验证。 |
| 43 §17 L963：Registry 做 policy bundle resolution | RB:L18–29；DEL:L51–73 | 部分一致 | 已有绑定 release/profile/lock 的 bundle 与按 digest 交付；没有看到从 rulesets 解析合成策略的 registry resolver。 |
| 44 §17 L964：Registry 管 provenance | REG:L102–106；RR:L199–205 | 一致 | 入库验证绑定来源摘要的 statement；未要求此条等同可解释的来源全文检索。 |
| 45 §17 L965：Backend Runtime Pack metadata、compatibility、revocation | PACK:L105–140；STORE:L31–39 | 部分一致 | pack verifier 检查身份/兼容及可选 trust metadata，覆盖密钥撤销；未找到 Registry 的 pack 目录与按 pack 资产撤销操作，不把密钥撤销等同 pack 撤销。 |
| 46 §17 L966：Registry audit | REG:L219–221；STORE:L21–27 | 一致 | Rust 记录审计事件；Java seam 另提供链式审计与可选 journal。未把内存日志当作已验证的生产持久存储。 |
| 47 §17 L968：Registry 不负责扫描源码 | REG:L91–106；GOV:L205–216 | 一致 | 所读 registry 输入是签名资产、摘要、artifact 字节及治理上下文，未调用源码扫描；扫描存在独立 pilot/runner 路径。 |
| 48 §18 L974–993：项目消费 Profile，Blocking maturity 与不完整 INCONCLUSIVE | GOV:L24–30；GATE:L575–588 | 部分一致 | 有 Profile、blocking rules 和 completeness Gate；缺少 maturity 字段、rulesets 模型及示例所表达的成熟度准入。 |
| 49 §18 L997–999：发布时确定性展开 ruleset/版本/参数/Exception/冲突到不可变 Manifest 和 Signed Bundle | GOV:L24–45；RB:L18–43 | 部分一致 | 排序集合、摘要签名有确定性；Profile 没有 ruleset/version 参数展开模型，Bundle 绑定的是 profile digest 而非完整 Resolved Policy Manifest。 |
| 50 §18 L999–1000：重复 Intent/Release/severity 冲突必须显式解决，禁止 last-wins | GOV:L24–30；REG:L129–138 | 部分一致 | 活动 intent 冲突会报错，但这不是策略展开冲突解析；Profile 不能表达候选 release/severity 冲突，更无显式决议记录。没有发现 last-wins 的正证据。 |
| 51 §18 L1000：Runner 不动态获取最新 ruleset | DEL:L51–59；RB:L175–183 | 一致 | 所审交付路径按 bundle digest 获取并验证固定 release/profile/lock，非 latest 解析。此为当前实现路径，不证明所有未来 Runner。 |
| 52 §18 L1002–1003：每 Scenario×scope 恰一个 authoritative implementation | GOV:L24–30；RB:L18–29 | 未实现 | 未找到 Scenario/scope→implementation 唯一约束模型；一个 backend 或单个 release 字段不能证明逐场景恰一权威实现。 |
| 53 §18 L1003–1005：SHADOW 可并行产对比 Evidence，但不进入权威 Finding/Baseline/Gate | CI:L165–174；GATE:L570–579 | 未实现 | 当前 Gate 接收 committed observations 直接归一化，没有 implementation authority 标记或 shadow 分流证据；pilot 的 LiveShadow 是运行模式，不等于并行实现影子通道。未断言现存 shadow 数据已经污染 Gate。 |
| 54 §18 L1005–1006：禁止权威实现投票/合并，多实现分歧进入 Feedback/Verification | GOV:L24–30；GATE:L570–574 | 无法验证 | 没有发现多实现选择/投票模型；通用 findings 去重不是投票。未建立 implementation 分歧路由，不能从未支持该场景推定禁令已被落实或已被违反。 |
| 55 §18 L1008–1009：权威失败/unsupported 不切 SHADOW，按 Completeness INCONCLUSIVE | GATE:L558–588；CI:L186–190 | 部分一致 | 失败或缺失会 INCONCLUSIVE，非权威 fallback 被拒绝；没有权威/SHADOW 实现模型，故不能完整验证自动切换禁止约束。 |
| 56 §18 L1009–1011：更换权威实现须等价验证并重发 Manifest/Bundle/Lock，Runtime 无晋升权 | RB:L175–183；CF:L199–214 | 部分一致 | bundle 对固定 lock/profile/release 绑定，不能静默换其摘要；未找到 implementation 等价验证和治理重发的完整流程。 |

## 问题清单（按严重度）

### 高：会阻断按规格启用权威规则发布

1. **七项最低 Evidence 没有落实为 VERIFIED 晋级门禁（16、01、15）。** REG admission 把有效签名与输入摘要匹配直接映射 Verified；没有确认 Contract、Positive、Negative、Near-miss、Holdout、Compatibility、Repeatability。可信签名不能替代这些证据内容。若当前 Verified 仅代表资产验签，需明确与成熟度解耦，不能作为完成 Lab 的证据。
2. **成熟度和权限审批链不完整（18、22、23、25）。** 三态资产生命周期不等于八态规则成熟度；promote 的 actor/reason 是字符串而非受验证身份/审批。Profile 接受 blocking ids 也未校验 BLOCKING 成熟度。当前证据说明库级契约缺口，**不直接宣称已发现线上越权攻击入口**：Java Store 自称 API seam，发布部署边界未验证。
3. **Scenario/scope 权威实现选择及 SHADOW 隔离未找到（52、53、56）。** 当前单 release/profile 表达不了规格的唯一权威约束、实现分歧证据与等价迁移；不能靠无多实现支持就认定隔离机制完成。

### 中：发布资产和验证覆盖不完整

4. **Profile resolve 缺少输入与决议模型（43、49、50）。** 有 deterministic signed digest，不等于已实现 ruleset/版本/参数/Exception/冲突的确定性展开。
5. **验证层只有分散能力，没有完整 Candidate Lab（02–14、17）。** 本地 fixtures、运行时 conformance、pilot 性能与 Shadow 报告分别有实现，不应混成独立规则验证通过的结论。Boundary/Mutation 未找到实现证据；Repeatability 只检查固定 observation 的 digest。
6. **RuleRelease 元数据欠缺（27、32、33）。** implementation 身份不明确、Evidence 仅引用摘要、Remediation Metadata 未绑定。
7. **Registry pack 治理只见 verifier 一侧（45）。** 签名与兼容检查存在，但 pack 目录、资产级撤销及统一 registry 接线未找到。

### 低 / 验证可靠性

8. **本地样例覆盖可空、PMD 错误没有进入结果判定（04–08）。** LV:L76–80 只提取 violations，L104 对现有 results 执行 allMatch；代码中没有类别非空门禁。空 fixtures 的 allMatch 可得 true，只有 negative/near-miss 的执行若有处理错误也没有在这段显式映射 FAIL。它明确标记 LOCAL_UNATTESTED，故风险是本地证据质量，不是已证明可获得受信 RuleRelease。未执行构造复现，不扩大结论。

## 整改建议

1. 首先明确资产验签状态与 Rule Maturity 的边界；建立单一成熟度晋级入口，消费结构化且绑定 Candidate/implementation 的最低 Evidence，验证独立验证者、角色和审批，禁止用任意非零 evidence_digest 替代证据满足性。
2. 在既有 Profile/Bundle 上补齐 Resolved Policy 的必要表达：Scenario/scope、固定 release/implementation、参数与 Exception、显式冲突决议。发布时校验恰一权威实现，Runtime 只消费锁定结果；有 SHADOW 时必须在归一化和 Gate 之前隔离。
3. 将现有本地样例、conformance、pilot 能力按真实边界接入 Lab，不重写已有执行器；补规则级 Boundary/适用 Mutation、独立 holdout 和同 Candidate 重复执行证据。首先让空类别、处理错误无法得到本地 PASS。
4. Release manifest 绑定 implementation 与 remediation 元数据；pack Registry 增加资产目录/撤销契约，并验证消费侧撤销传播。完善后再跑发布/晋级/撤销/不完整扫描的端到端验收。

## 证据边界

- 检索限制：首先对指定 runtime 起点作符号检索；发现 `runtime/release-registry`、`runtime/verification-worker` 及 Java `control-plane/verification` 后跟随关键代码，不作无界全仓阅读。
- 未找到证据的主要检索：在 `runtime` 的 Rust 文件中搜 `EvidenceBundle|Holdout|NearMiss|RuleMaturity|CANDIDATE|authoritative|ResolvedPolicy`；在相关 verification/factory/server Java 与 worker/registry 中搜 `holdout|near.miss|repeatability|approval|CANDIDATE|BLOCKING`；在 runtime/control-plane 源码（排除 target）补搜 `ResolvedPolicy|authoritative_implementation|shadow_implementation|remediation_digest|implementation_digest|RuleMaturity`，该补搜无命中。路径限定检查 `LocalVerification.categoryOf`、`EXPECTED_ACCEPTANCE_CHECKS` 与 manifest/profile structs，作为缺口证据。不是“无命中即全仓不存在”的结论。
- `promote_profile` 与 release `promote` 的限定调用点检索主要返回库测试；未发现生产授权包装，因此报告不假定外部包装替其完成角色检查，也不声称已穷尽所有部署入口。
- 代码中的 ADR 注释只用于区分本地/权威边界；没有用其他文档或旧审计替代本次规格。API seam 的 Candidate→Active 不被解释成真实规则直接获得 Blocking 权限。
- 没有网络请求，没有 cargo/Maven/全量门禁执行，没有改源码或运行产出副作用的测试。依任务明确限制，本次交付仅静态审计；`bash scripts/check.sh` 未运行，不能声称门禁全绿。既有 target/surefire 产物未作为本次测试证据。
- 最终文件做机械核对：声明编号连续、判定统计、引用路径与行号范围有效、报告结构完整；“一致”仅说明对应窄条款可由所列源码支持，不是全链路生产认证。