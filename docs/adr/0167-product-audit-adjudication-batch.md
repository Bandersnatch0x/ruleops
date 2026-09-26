# ADR-0167: 六镜产品审计待裁批次裁定

- Status: Accepted
- Date: 2026-09-24
- Scope: 对外定位、README 首屏、fast-decision、node-fake host、创作/工厂/本体/IDE 拆分、目标产出
- Decision record: `docs/research/2026-09-24-product-audit-six-lens-summary.md` §7 待裁批次

## Context

以外部产品审计 skill 套件（positioning / scope / users-jtbd / goals-metrics /
feature-triage / idea-stress-test）对本仓库做了六项只读审计，收敛出一个
「待裁批次」（6 项产品/范围决策）。审计本身不改产品；按 AGENTS.md「一切决策问题由
用户作答」，这些决策由用户裁定，本 ADR 记录裁定结果与其执行边界。

机器实现前沿已出清（见 auto-memory `v12-machine-frontier-exhausted`）：`bash scripts/check.sh`
全绿，剩余不确定性全在代码之外（真实用户、真实数据、部署决策）。因此本批裁定聚焦
「收窄承诺、恢复主线、止血冗余」，而非新增能力。

## Decision

用户对 6 项逐条裁定（A/B 为 §7 所列选项）：

**决策 1 — 对外定位收窄（裁定：A，已执行）。**
README/CONTEXT 开篇的「可执行」从产品承诺中移出，收窄为「可信规则执行层：以
Completeness + 三态 Gate + 签名执行证明取代『0 告警即通过』」。「可执行 / 受保护分支
强制」显式标注为**待验证假设与后续阶段目标**，不作对外承诺。依据：shipped `./rf`
禁用 release/gate/promote 与 SCM 回写（`rf:272-274`），权威恒 `LOCAL_UNATTESTED`/`SHADOW`。
可逆的文档改动。

**决策 2 — README 首屏去脚手架（裁定：A，已执行）。**
`./rf` CLI 清单从平铺 ~30 条重排为三区：主线（执行层，新手 10 分钟：init/doctor/
config/scan）、创作/工厂（monorepo 受治理组件，见决策 5）、Shadow 窗口内部/预发布（pilot 族、center、
explain、feedback/exception/submission、status、decision）。不删任何命令，仅分区。

**决策 3 — fast-decision 降级为 experimental（裁定：A，已执行）。**
`rf decision evaluate`（`runtime/fast-decision`）从新手主线移出，标注 experimental
（README 内部区 + 本 ADR），**保留代码、保留 `scripts/fast-decision-check.sh` 门禁、
ADR-0158 不变**。零破坏、完全可逆；未采纳「真删」（B），因删除牵动 `check.sh`/工作区
编译而在需求未验证前无收益。

**决策 4 — 移除 node-fake backend host（裁定：A，已执行）。**
删除 `backend-hosts/node-fake/`（4 个 JS 文件，非 Cargo 工作区成员）。跨语言契约一致性
由 rust fake host/resolver + 真实 Java host 承担，node 探针与之覆盖重复。连带改动已同步：
`workspace-resolvers/fake/tests/integration.rs` 移除两个 node 解析器互操作测试；
`scripts/phase1-exit-demo.sh` 移除 Node Host/Resolver 证据段与证据表行。**代价**：跨语言
一致性覆盖从「Rust+Java+Node」收敛为「Rust+Java」；此为审计判定可接受的冗余收敛。
git 可回滚。

**决策 5 — 创作/工厂/本体/IDE 的仓库形态（裁定：B 拆分，执行形态定为 monorepo，不拆外部仓）。**
用户裁定拆分（B）以恢复执行层主线，反转了审计的「暂缓」推荐；随后进一步指示「你来减仓
或者使用 monorepo」。据此**执行形态定为：保留单一 monorepo,不切外部仓**——物理外部抽取
(建独立仓、切 git 历史、拆 Maven reactor)被从默认路径撤除,因其破坏性、会使门禁在过程中变红、
需人建外部仓,且与决策 6(目标产出未定、先跑访谈)排序冲突,属验证前的大重构反目标。

拆分意图改由 monorepo 内的**强制 seam**兑现,而非物理搬仓:执行核心(`runtime/*` Rust
工作区 + `backend-hosts/{fake,java}` + `contracts/*`)与创作簇(`control-plane/authoring|
factory|coverage|ontology`)、IDE 客户端(`clients/*`)只经 `contracts/http` 交付契约 +
签名 RuleBundle(`runtime/bundle-delivery`)相接。该 seam 现已是干净的:创作侧 Java 模块图
浅而闭合(authoring 为叶,factory/coverage requires authoring,ontology/server 独立),
**无任何 Rust 执行 crate 依赖创作侧源码**,两侧仅在契约/bundle 层相遇。因此本裁定为
「确认并记录既有 monorepo 边界 + README 主线执行层优先」,无需破坏性搬迁,门禁保持全绿。
边界与(若未来目标产出定档后确需)物理拆分的前置门见
`docs/refactor/monorepo-boundary.md`。README 已把创作/工厂命令与执行主线分区。

**决策 6 — 目标产出定档（裁定：先跑需求访谈再定）。**
目标产出（VC-scale 战略产品 / 受监管细分工具 / bootstrapped 小生意）**暂不锁定**，前置
于 8–12 场需求发现访谈（脚本见
`docs/research/2026-09-24-demand-discovery-interview-script.md`）与已登记 LIVE_SHADOW 窗口
（2026-10-01→10-31）的真值。访谈与窗口结论回填后另立 ADR 定档，并据此重校决策 5 的物理
拆分是否启动、决策 1 的收窄是否进一步调整。

## Consequences

- 对外承诺与已交付边界一致：不再把未开放的「可执行/强制」当卖点。
- 新手主线清晰：README 首屏只剩执行层 5 条命令，其余按用途分区、不丢信息。
- fast-decision 止血但不失血：降级即撤出叙事，代码与门禁完整可回滚。
- 产品主路径仍不引入 JavaScript 规则引擎；架构验收另保留无依赖 node-fake 协议探针，跨语言一致性由同一 suite 记录 Rust 与 Node 两套结果。
- 拆分方向以 monorepo seam 兑现，不切外部仓：执行核心与创作簇/IDE 仅经契约 + 签名
  RuleBundle 相接，seam 现已干净（无 Rust→创作侧源码依赖）；避免了「验证前大重构」反目标，
  门禁保持全绿。若决策 6 定档后确需物理拆分，前置门见 monorepo 边界文档。
- 目标产出仍是开放前置决策，绑定十月访谈/窗口证据，不由代理推断。

## 2026-09-26 架构计划一致性复核

V1.2 架构计划的 Phase 1/Phase 4 exit criteria 仍明确要求 Fake Node.js
Host/Resolver，并要求它们与 Java path 通过同一 Conformance Suite。此前「删
node-fake」裁定针对的是产品主路径和维护面，不应被解释为删除架构计划要求的
语言中立协议证明。为完成架构计划验收，本仓库恢复四个无依赖 Node.js 协议探针
文件；它们不引入 JavaScript 规则引擎、不改变主产品定位，且必须与 Rust Fake
Host/Resolver 通过同一 31 项 suite。Phase 2 独立验收同时记录两套日志，任何一
套失败都不能生成 PASS 证据。

## References

- `docs/research/2026-09-24-product-audit-six-lens-summary.md`（六镜审计汇总 + §7 待裁批次 + 问题修改建议）
- `docs/research/2026-09-24-demand-discovery-interview-script.md`（决策 6 需求访谈）
- `docs/pilot/live-shadow-window-execution-checklist.md`（决策 6 能力跑道）
- `docs/refactor/monorepo-boundary.md`（决策 5 monorepo 边界与 seam）
- ADR-0086 backend-neutral 由 fake host 证明（决策 4）
- ADR-0158 fast-decision evidence shadow boundary（决策 3）
- auto-memory `v12-machine-frontier-exhausted`
