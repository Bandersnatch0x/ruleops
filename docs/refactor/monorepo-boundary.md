# Monorepo 边界（决策 5，ADR-0167）

> 裁定：**保留单一 monorepo，不拆外部仓。** 决策 5 的「拆分」意图改由 monorepo 内的强制
> seam 兑现，使执行层主线不被创作侧淹没，同时不做验证前的破坏性搬迁。本文件记录既有边界、
> seam，以及仅在目标产出定档后才考虑的物理拆分前置门。

## 为什么是 monorepo 而非外部拆分

- **非破坏、门禁全绿**：外部抽取（建独立仓、切 git 历史、拆 Maven reactor、重接 `rf center
  check` 与 `check.sh`）会使门禁在过程中变红、需人建外部仓，属多会话破坏性重构。
- **与决策 6 一致**：目标产出未定（先跑访谈），验证前做仓库级大重构正是审计点名的反目标。
- **seam 已干净**：拆分要达到的「执行核心与创作侧解耦」在当前 monorepo 里已成立（见下），
  无需搬仓即已实现主线聚焦。

## 两簇与强制 seam

**执行核心（对外主线）**
- `runtime/*`（Rust 工作区，~35 crate：rf-cli、rf-runner、gate、scan-plan、trust、
  bundle-delivery 等）
- `backend-hosts/{fake,java}`、`workspace-resolvers/*`
- `contracts/{proto,http,rust}`（两簇公共依赖，版本化演进）

**创作簇（monorepo 内受治理组件，不进新手主线）**
- `control-plane/authoring|factory|coverage|ontology`（Java 创作/工厂/本体）
- `control-plane/server`、`control-plane/verification`（控制面/验证）
- `clients/vscode|intellij`（IDE 薄客户端，ADR-0166 已薄壳化，只调用 `rf`）

**唯一 seam**：创作簇产出**签名 RuleBundle**，经 `contracts/http` delivery 契约 +
`runtime/bundle-delivery` 交给执行核心；执行核心只消费 bundle digest。IDE 只以 `rf` 为接口。

## seam 现状（已验证，2026-09-24）

- Java 创作侧模块图浅而闭合：`authoring` 为叶，`factory`/`coverage` requires `authoring`，
  `ontology`/`server` 无内部 ruleops requires，`verification` 无内部 requires。
- **无任何 Rust 执行 crate 依赖创作侧源码**（`grep` 全仓为空）；两簇仅在契约/bundle 层相遇。
- 结论：边界已是契约级，非源码级；README 主线只列执行核心 5 条命令，创作/工厂命令分区呈现。

## 维持边界的规则

- 执行核心**不得**新增对 `control-plane/*` 源码或 Java 产物的直接依赖；跨簇只走契约 + 签名 bundle。
- 新增创作能力落在创作簇，经 RuleBundle 交付；不得回写执行核心权威（沿用 ADR-0166 纯核边界）。
- 契约变更走 `contracts/*` 版本化流程，作为两簇公共约定。

## 物理外部拆分（仅在满足前置门后才考虑）

- **G0 前置门**：需求访谈（8–12 场）+ LIVE_SHADOW 窗口（2026-10-01→10-31）结论回填、
  目标产出经 ADR 定档，且该目标下确认外部拆分收益大于成本。未满足则不启动。
- 满足后如启动，仍须：为拆出对象保留 git 历史（`git filter-repo`）、从 reactor 摘模块、
  把创作侧闭环门禁迁到创作仓、核心仓门禁只验「消费签名 bundle」，并逐阶段保持各仓门禁全绿。
- 该步为破坏性、须用户主动驱动，代理不在无监督下执行。

## 参考

- ADR-0167 六镜产品审计待裁批次裁定（决策 5）
- ADR-0166 IDE Thin Client 适配边界（`clients/*` 已薄壳化）
- `contracts/http` delivery 契约、`runtime/bundle-delivery`、`runtime/rule-bundle`
- `docs/research/2026-09-24-product-audit-six-lens-summary.md` §4/§7/§8
