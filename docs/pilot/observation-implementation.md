# Pilot Observation Implementation

状态：工程 MVP 已具备可复跑的观测闭环；2026-10-01 前不宣称 live pilot outcome。

## 已落地能力

- `LOCAL_SIMULATION`：对三份本地 clone 追加合成提交，验证新增违规、修复、再次新增、回放、反馈、治理快照和窗口聚合。
- `SYNTHETIC_SHADOW`：内部受控仓库走 Runner -> Trusted CI 资格验证，绑定合成 repository/commit 身份；它只证明工程链路，不进入产品指标。
- `LIVE_SHADOW`：按登记清单只读获取固定默认分支提交，绑定 repository/commit/rule-bundle 身份，不写回 GitHub；checkout commit 漂移会 fail closed。
- `erf-pilot-observer`：直接提供 `baseline`、`run`、`feedback`、`governance`、`report` 五个命令；`rf pilot` 包装层将其暴露为 `baseline`、`observe`、`feedback`、`governance`、`window-report`。
- `erf-pilot-metrics`：以 mode、repository/commit、rule-bundle 与 run identity 参与 digest，journal 只保存摘要、计数、状态、资源指标和审计信息。
- `erf-pilot-evidence`：提供 digest-only `PilotEvidencePackage` 封存与
  `PilotAdjudication` 裁决入口；合同、登记样本、窗口、provenance、完整性和
  artifact digest 均参与 canonical digest，状态严格按 `FAIL > INCONCLUSIVE > PASS`
  聚合。
- 窗口报告：必须显式按一个模式聚合；登记样本和 commit 集合错配 fail closed；覆盖率、完整率、INCONCLUSIVE 率、误 PASS、违规趋势、延迟、内存与质量审定缺测保持 `INCONCLUSIVE`。

## 已复跑证据

| 运行 | 结果 | 证据 |
| --- | --- | --- |
| 本地三仓库模拟，3 个阶段/仓库，9 次观测 | `PASS`（脚本）；报告 `INCONCLUSIVE`（质量/内存基线缺测） | `.scratch/pilot-local-simulation-20260918-4/local-simulation.md` |
| 三仓库只读 Shadow 离线演练，3 次观测 | `PASS`（脚本）；报告 `INCONCLUSIVE`（窗口前缺测） | `.scratch/pilot-live-shadow-20260918-3/live-shadow.md` |
| 三仓库只读 Shadow 在线 GitHub 观测，3 次观测 | `PASS`（脚本）；报告 `INCONCLUSIVE`（正式窗口前缺测） | `.scratch/pilot-live-shadow-online-20260918-1/live-shadow.md` |
| 观测契约、CLI、三模式身份隔离与错配负例检查 | `PASS` | `.scratch/pilot-observation-check-20260918-3/pilot-observation.md` |
| Shadow MVP 交付清单预检 | `STATIC_ONLY` | `.scratch/pilot-delivery-check/pilot-release-candidate.md` |
| PilotEvidencePackage / PilotAdjudication 完整与负例检查 | `PASS`（`EXECUTABLE`） | `.scratch/pilot-evidence-check-113-final/pilot-evidence.md` |

本地模拟的阶段序列实际产生 `1 -> 0 -> 1` 个新增 Finding；三仓库固定提交的
Shadow 演练均为完整执行，历史债务分别为 7、4、4。以上是机制验收和预演，
不是对外部仓库质量或受保护分支的授权结论。

报告中的 `Engineering gate` 与 `Product outcome` 分开显示。内部
`SYNTHETIC_SHADOW` 即使工程检查通过，产品 outcome 仍固定为
`INCONCLUSIVE`；只有登记样本、commit 和模式均匹配的 `LIVE_SHADOW` 记录才
有资格进入产品窗口聚合。

## 运行入口

```text
scripts/pilot-local-simulation.sh .scratch/pilot-local-simulation-<run>
scripts/pilot-live-observation.sh .scratch/pilot-live-shadow-<run>
scripts/pilot-live-observation.sh .scratch/pilot-live-shadow-<run> --offline SOURCE_ROOT
```

交付前先运行：

```text
./rf pilot delivery-check .scratch/pilot-delivery-check
./rf pilot evidence package INPUT OUTPUT --contract-digest HEX --sample-digest HEX
./rf pilot evidence adjudicate PACKAGE OUTPUT --contract-digest HEX --sample-digest HEX
bash scripts/check.sh
```

正式窗口使用 `LIVE_SHADOW`，计划和样本分别见
`specs/pilot/pilot-observation-plan.yaml` 与
`specs/pilot/live-java-repositories.yaml`。真实窗口开始后仍需补录经批准的
延迟/内存基线和人工 precision/recall/false-positive adjudication；在这些
输入缺失时，报告必须继续为 `INCONCLUSIVE`。
