# Pilot Observation Implementation

状态：工程 MVP 已具备可复跑的观测闭环；2026-10-01 前不宣称 live pilot outcome。

## 已落地能力

- `LOCAL_SIMULATION`：对三份本地 clone 追加合成提交，验证新增违规、修复、再次新增、回放、反馈、治理快照和窗口聚合。
- `LIVE_SHADOW`：按登记清单只读获取固定默认分支提交，绑定 repository/commit/rule-bundle 身份，不写回 GitHub。
- `erf-pilot-observer`：提供 baseline、observe、feedback、governance、window-report 五个命令。
- `erf-pilot-metrics`：以 mode 参与 digest，journal 只保存摘要、计数、状态、资源指标和审计信息。
- 窗口报告：覆盖率、完整率、INCONCLUSIVE 率、误 PASS、违规趋势、延迟、内存与质量审定；缺测保持 `INCONCLUSIVE`。

## 已复跑证据

| 运行 | 结果 | 证据 |
| --- | --- | --- |
| 本地三仓库模拟，3 个阶段/仓库，9 次观测 | `PASS`（脚本）；报告 `INCONCLUSIVE`（质量/内存基线缺测） | `.scratch/pilot-local-simulation-20260918-4/local-simulation.md` |
| 三仓库只读 Shadow 离线演练，3 次观测 | `PASS`（脚本）；报告 `INCONCLUSIVE`（窗口前缺测） | `.scratch/pilot-live-shadow-20260918-3/live-shadow.md` |
| 三仓库只读 Shadow 在线 GitHub 观测，3 次观测 | `PASS`（脚本）；报告 `INCONCLUSIVE`（正式窗口前缺测） | `.scratch/pilot-live-shadow-online-20260918-1/live-shadow.md` |
| 观测契约、CLI、模式隔离与单元测试检查 | `PASS` | `.scratch/pilot-observation-check-20260918-3/pilot-observation.md` |
| Shadow MVP 交付清单预检 | `PASS` | `.scratch/pilot-delivery-check/pilot-release-candidate.md` |

本地模拟的阶段序列实际产生 `1 -> 0 -> 1` 个新增 Finding；三仓库固定提交的
Shadow 演练均为完整执行，历史债务分别为 7、4、4。以上是机制验收和预演，
不是对外部仓库质量或受保护分支的授权结论。

## 运行入口

```text
scripts/pilot-local-simulation.sh .scratch/pilot-local-simulation-<run>
scripts/pilot-live-observation.sh .scratch/pilot-live-shadow-<run>
scripts/pilot-live-observation.sh .scratch/pilot-live-shadow-<run> --offline SOURCE_ROOT
```

交付前先运行：

```text
./rf pilot delivery-check .scratch/pilot-delivery-check
bash scripts/check.sh
```

正式窗口使用 `LIVE_SHADOW`，计划和样本分别见
`specs/pilot/pilot-observation-plan.yaml` 与
`specs/pilot/live-java-repositories.yaml`。真实窗口开始后仍需补录经批准的
延迟/内存基线和人工 precision/recall/false-positive adjudication；在这些
输入缺失时，报告必须继续为 `INCONCLUSIVE`。
