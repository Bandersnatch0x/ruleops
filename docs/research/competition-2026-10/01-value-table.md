---
title: 比赛申报价值表（研发运营效能）
date: 2026-09-30
repository: ruleops
status: draft-for-submission
authority: SHADOW
sources:
  - docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md (§21)
  - specs/pilot/pilot-success-contract.yaml
  - specs/pilot/live-java-repositories.yaml
  - docs/pilot/live-java-repositories.md
---

# 比赛申报价值表

把白皮书 §21「企业运营驾驶舱」的四组 KPI 映射到赛则「综合价值评判」三维
（客户价值 / 产品价值 / 项目价值），作为 50 分那一栏的数据底表。

**登记纪律**：本表只登记可核验、可复算证据。preflight（窗口前）证据一律标注为
preflight，**不得充当 live pilot outcome**；缺测项记 `待测`，不填 0、不填估计值。

状态口径：`已实测` = 仓库内有固定样本与可复算命令；`待测` = 机制就绪、缺真实输入；
`边界` = 机制存在但当前权限/范围受限，如实登记；`Defer` = 仅保留内部兼容面，不作公开分发承诺。

## 1. 客户价值（对客户实际业务能力与满意度的提升）

| KPI | 证据来源（可复算） | 现值 | 状态 |
| --- | --- | --- | --- |
| 规范资产总量 | `specs/standard/SEC-JAVA-017..116.yaml` | 100 条 Standard | 已实测 |
| 可机器检查的候选场景 | `specs/coverage/SEC-JAVA-017..036.yaml` | 20 条 Scenario | 已实测 |
| 覆盖分类 | `specs/pilot/phase0-template-selection.yaml:16` | 14 已有规则 / 6 须新写 / 0 语义待澄清(blocked) / 0 不可机器检测 | 已实测 |
| 三态 Gate + Completeness | `specs/pilot/live-java-repositories.yaml` | preflight 3/3 仓库 `complete`，Gate `FAIL`（发现违规） | 已实测(preflight) |
| 真实仓库违规发现 | 同上 | word-wrap 7 / markedj 4 / keycloak-cas 4 findings，重复扫描 2 次一致 | 已实测(preflight) |
| 新增 vs 历史债务趋势 | `pilot-metrics-baseline.yaml:21-34` | 机制就绪，无窗口数据 | 待测 |
| 例外/回滚治理 | `pilot-metrics-baseline.yaml:56-76` | 机制就绪，无窗口数据 | 待测 |
| 扫描延迟 / 资源 | `pilot-metrics-baseline.yaml:77-90` | 机制就绪，无窗口数据 | 待测 |

**核心卖点（可写入申报）**：把「0 告警即通过」换成「Completeness + 三态 Gate + 签名执行证明」；
对 3 个真实公开 Java 仓库的 preflight 已证明扫描 **complete=3/3** 且可重复，而非「没报错就当作通过」。

## 2. 产品价值（推广价值与受益范围）

| KPI | 证据来源 | 现值 | 状态 |
| --- | --- | --- | --- |
| Backend-neutral 架构 | 白皮书 §10、ADR-0027 | PMD 主 / SpotBugs 补，Seam 未绑死单一引擎 | 已实测 |
| 模板路径 | `phase0-template-selection.yaml:31` | 3 类：ExistingRule 13 / StructuralRule 6 / SymbolRule 1 | 已实测 |
| Rule 成熟度谱 | 白皮书 §13 | 当前全部 `SHADOW`，未晋级 `BLOCKING` | 边界 |
| 受益范围声明 | 白皮书 §24 | Java 起，可扩语言与引擎（声明，非实测） | 声明 |
| 已交付执行路径 | `docs/pilot/operator-quickstart.md`、`docs/pilot/competition-evidence-index.md` | rf-runner / `./rf` 本地扫描与证据执行路径；不代表 Server/IDE 公开分发已验收 | 已实测（限定本地执行） |
| 内部兼容面 | 白皮书公开范围说明、`clients/vscode/README.md`、`clients/intellij/README.md` | RuleOps Server 与 IDE/CLI 薄客户端仅供内部兼容维护，不作公开分发承诺；真实 IDE 验收未证实 | Defer |

## 3. 项目价值（提效与里程碑）

| KPI | 证据来源 | 现值 | 状态 |
| --- | --- | --- | --- |
| 规则作者提效（Author Active Time） | `specs/pilot/author-cost-baseline.yaml`、`docs/pilot/author-cost-baseline.md` | 工具与空记录就绪，无真实作者工时 | 待测 |
| 可复现构建 | `docs/pilot/competition-evidence-index.md` | 6 个 JAR byte-identical，5 项 PASS | 已实测 |
| 工程规模 | `runtime/*`、`control-plane/*`、`scripts/*` | Rust 40+ crates / Java 6 模块 / 50+ 检查脚本 | 已实测 |
| Verification / Evidence | ADR-0070、`specs/pilot/pilot-evidence-package.yaml` | 独立验证 + 证据包封存机制就绪 | 已实测 |

## 4. 诚实边界（申报时必须如实标注）

- preflight 数字来自窗口前只读演练，`authority = SHADOW`，不授予 protected-branch 权力。
- 当前 checkout 显式禁用 release/gate/promote 与 SCM 回写（ADR-0167）；扫描权威恒为 `LOCAL_UNATTESTED` / `SHADOW`。
- 效率类 KPI（author-active-time）与质量类 KPI（precision/recall）不得用机器耗时或预演数据顶替人工输入；仅无已知 `FAIL` 且缺必需证据时报告为 `INCONCLUSIVE`，已知必需指标 `FAIL` 优先 `NO_GO`（ADR-0157、`specs/pilot/pilot-evidence-package.yaml`）。
- 推广范围与长期演进为路线声明，不是已交付事实。
- 日期落窗不等于治理已激活：ADR-0176 的七天批准目标不证明全部事前条件已满足；当前材料未证实正式短窗激活，须由负责人核对起点前的治理批准、实现验收及人员/基线就绪证据，不得追溯补造或把本周采集直接列为正式短窗成果。
