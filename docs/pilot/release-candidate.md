# ERF Shadow MVP 交付候选

状态：`READY_FOR_SHADOW_WINDOW`。

本交付候选已经把 Java 规则工厂的 MVP 工程闭环收敛为一个可交接包：

- 100 条 Standard inventory、20 条候选 RuleIntent 与 3 个固定 Java Shadow 仓库；
- 本地机制模拟与只读 `LIVE_SHADOW` 观测入口；
- digest-only journal、窗口聚合、反馈与治理快照；
- `rf pilot delivery-check` 交付预检与 `bash scripts/check.sh` 全量门禁；
- `rf center check` 对用户约定的 RuleOps Center 全链路做单一验收；
- SCM 写入、源代码上传、状态发布和受保护分支权力全部保持禁用。

机器可读清单是
`specs/pilot/pilot-release-manifest.yaml`，交付预检命令是：

```text
./rf pilot delivery-check .scratch/pilot-delivery-check
./rf center check .scratch/ruleops-center-check
bash scripts/check.sh
```

当前交付状态不是生产强制执行。它可以进入登记的
`2026-10-01` 至 `2026-10-31 UTC` Shadow 观察窗口，但正式结果必须等到窗口
内补齐延迟基线、资源基线，以及人工 precision/recall/false-positive
裁决。缺少这些输入时，报告必须保持 `INCONCLUSIVE`，不能用预演数据替代。

## 交接顺序

1. 全量门禁通过并保留 `.scratch/` 证据。
2. 观察窗口内运行 `scripts/pilot-live-observation.sh OUT_DIR`。
3. 用 `rf pilot window-report ...` 生成窗口报告，只交接摘要和 digest。
4. 完成人工质量裁决和治理决定；在此之前不改变 Shadow 权限边界。

最新工程证据和入口说明见
`docs/pilot/observation-implementation.md`。
