# ERF 架构规格 V1.2 Final 符合度审计总报告

**审计标的**：`docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md`（3147 行，§0–§51）
**方法**：9 个逻辑域串行派发 Orca dispatched worker（codex），逐条声明三维核验——规格原文（章节+行号）→ 代码实证（file:line）→ 判定（一致／部分一致／不一致／未实现／无法验证）。
**证据等级**：**STATIC_ONLY**（只读源码/文档/仓库结构；未运行全量门禁、未跑 cargo、未接真实 Server/Registry、未做动态利用测试）。
**编排账目**：Run `run_d5157c8c5b52`。9 域全部由 codex 完成，其中域 5、域 7 因首轮 worker 停滞而**重派 codex 独立重做**（覆盖早前接管稿）。

## 分域汇总

| # | 域 | 章节 | 声明数 | 不一致 | 未实现 | 报告 |
|---|---|---|---|---|---|---|
| 1 | 定位与边界 | §0–2 | 55 | 3 | — | `01-positioning-boundary.md` |
| 2 | 运行模型与生命周期 | §3–4 | 34 | 见报告 | — | `02-operating-model-lifecycle.md` |
| 3 | 逻辑架构与领域模型 | §5–6 | 80 | 4 | 23 | `03-logical-arch-domain-model.md` |
| 4 | 标准与规则规格 | §7–13 | 52 | 6 | 1 | `04-standard-rule-spec.md` |
| 5 | 验证与发布 | §14–18 | 56 | 4 | 6 | `05-verification-release.md` |
| 6 | 扫描与判定 | §19–23 | 见报告 | 见报告 | 见报告 | `06-scan-gate.md` |
| 7 | 安全与接口 | §24–29 | 96 | 10 | 15 | `07-security-interface.md` |
| 8 | 计划与验收 | §30–39 | 89 ADR + 25 验收 | 见报告 | 见报告 | `08-plan-acceptance.md` |
| 9 | 运行时架构 | §40–51 | 54 | 见报告 | 见报告 | `09-runtime-architecture.md` |

## 跨域重大问题（按主题归并）

### T1 可信执行链未闭合（最高优先，域 3/4/6/7/9 多域独立复现）

| 发现 | 域 | 证据 |
|---|---|---|
| **不可信 Observation 绕过仓库内容/坐标校验**：`normalize` 无仓库 blob 输入，仅校验 digest 字节数与区间相对顺序；伪造行列、任意 32 字节 digest、越界 end 均无核验 | 4 (L04) | 域 4 报告 P1-1 |
| **required capability 把键存在当能力满足**：Java Host 不拒 required 缺口，Rust Client 用 `contains_key` 接受 false | 4 (H07) | 域 4 报告 P1-2 |
| **trusted CI 直接消费裸 fingerprint 豁免集合**，未在消费边界与已授权 ExceptionGrant digest 闭合 | 7 | 域 7 P1（E10/E12） |
| **独立 Bootstrap 未闭环**：RunnerMeasurement 可构造、测量字符串是夹具；无真实 binary 测量启动与受保护 anchor/state | 9 | 域 9 P1-1 |
| **完成协议缺 Observation count/attempt digest**：Schema 无字段，无法靠补测试宣称符合 | 9 | 域 9 P1-2 |
| **RDFC-1.0 与本体签名运行契约未落地**：v0 Compiler 仅排序 N-Triples，无 dataset/命名图、无签名 envelope、无 deterministic Protobuf、无 asserted/compiled 身份分离 | 3 | 域 3 P1-1（评为域 3 最严重） |

### T2 发布晋级门禁缺证据约束（域 1/2/5 三域独立复现）

- **未要求独立 Verification**：Server release 晋级不校验 Verification（域 1 F01）。
- **七项最低 Evidence 未落实为 VERIFIED 门禁**：`REG admission` 把「有效签名 + 输入摘要匹配」直接映射 Verified，未确认 Contract/Positive/Negative/Near-miss/Holdout/Compatibility/Repeatability（域 5 高-1）。
- **成熟度与权限审批链不完整**：三态资产生命周期 ≠ 八态规则成熟度；promote 的 actor/reason 是字符串而非受验证身份（域 5 高-2）；`GovernanceState` 仅 `Verified/Active/Revoked`，**无 CANDIDATE**。
- **前序证据未进入状态机**（域 2 F02-01）。

### T3 身份与基线模型不完整（域 6，核心判定面）

- **F06-01**：Finding 身份缺 Scenario（`runtime/gate/src/lib.rs:624-653` 仅版本/rule_id/anchor，不符 §20.1）；符号/字节码身份缺仓库相对路径 → 可能跨场景合并、虚假 NEW。
- **F06-02**：`BaselineState` 仅 `New/Historical`，无法表达 `AMBIGUOUS` 等规范状态。
- **F06-03**：Baseline 输入类型隔离未在共享 preview 层强制（`BaselineKind` 存在但分类忽略 kind）。
- **F06-04**：Baseline 签名 payload 只签 kind/fingerprints（`:59-66`），缺 repository snapshot/Policy/RuleRelease 绑定。

### T4 安全 Controls 覆盖不足（域 7 重做，96 声明）

- §26.1 的 **13 项 Threats × §26.2 的 14 项 Controls** 逐项对照后：**10 不一致 + 15 未实现**。
- **14 个规格 CLI 命令缺失**，其中 `rf gate` 命令不存在（§27 承诺 vs `runtime/rf-cli` 实际）。
- 正向：`Revoked` 状态有转移守卫、sandbox proof 带 policy_digest 双身份、`path-safety` 客户端侧校验存在。

### T5 计划与验收的证据缺口（域 8）

- **ADR-001~010 无独立文件**（011~089 对应存在）——规格追溯缺口。
- Shadow 效率结论 **INCONCLUSIVE**、无真实 Pilot 数据支撑。
- **P1：产品成功与工程资格不可混同**——验收证据缺口 ≠ 已证实运行漏洞。

### T6 运行时生产保证缺口（域 9）

- staging 权限与删除失败（temp_dir/umask ≠ Runner-only；`remove_file` 失败被吞）。
- 完整 Root 委派/阈值/轮换未实现（单签 role 映射，非生产 2-of-3）。
- 上传隐私规则不统一（Store 递归拒绝 source/sourceCode 等，但未知字段、普通 path/symbol 不受同等约束）。
- 正向：Server、rf-runner、Thin Clients、Registry、离线缓存**均有实现**；37 项针对性测试通过。

## 各域明示的正向证据（不构成全域通过）

- **域 6**：`GateDecision{Pass,Fail,Inconclusive}` 三态实现完整（`runtime/gate/src/lib.rs:346-348`）；L483「incomplete execution remains INCONCLUSIVE regardless of exemptions」——**INCONCLUSIVE 守卫成立**。
- **域 5/9**：release 有完整 DSSE 签名 + ArtifactStatement + TrustStore 角色校验；「no registry/tag/network fallback」。
- **域 7**：`Revoked` 转移守卫；sandbox proof 双身份。
- **域 8/9**：ADR 011–089 与 `docs/adr/` 对应；§29 顶层仓库布局一致；CLI 32 命令面由门禁 `map-consistency` 强制。

## 证据边界（全局）

- 全部为 **STATIC_ONLY**：未运行全量门禁、未跑 cargo/网络、未接真实 Server/Registry、未做动态利用测试。
- 各域报告均明确标注「已执行/未验证」分段；本总报告**不升级**任何单域的证据等级。
- 多处发现为「**共享函数可达风险**」而非「已证实生产绕过」（如域 6 F06-03、域 9 staging 权限）——报告原文已如此限定，本汇总保持不变。
- 域 5、域 7 为重做稿；域 5 重做声明「未读取旧 05 报告」，域 7 重做同。
- **未审**：真实 Registry 多页/特殊 cursor、真实 Server 各回执、重启、多工作区、断网、宿主对话框、生产部署权限、GUI、历史交付时序。

## 建议的整改优先级（供裁决）

1. **T1 可信执行链**：`normalize` 引入仓库 blob 复验；required capability 改为语义校验而非键存在；ExceptionGrant digest 在 gate 消费边界闭合；Bootstrap 测量闭环。
2. **T2 晋级门禁**：把 §14.2 七项 Evidence 落实为 VERIFIED 前置断言；`GovernanceState` 引入 Candidate；actor/reason 改为受验证身份。
3. **T3 Finding/Baseline 身份**：补 Scenario 与仓库相对路径；`BaselineState` 支持 AMBIGUOUS；签名 payload 绑定治理上下文。
4. **T4 CLI 命令面**：补缺失 14 命令（尤其 `rf gate`）或在规格中降级。
5. **T5/T6**：补 ADR-001~010；把 staging 权限、Root 委派、隐私字段闭合落实或明确降级为「V1 未实现」。
