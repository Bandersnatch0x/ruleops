# ADR-0148: 增量扫描全量兜底、阶段遥测与可重复基线

- Status: Accepted
- Date: 2026-09-19
- Scope: V0.4 Runner / Pilot evidence

## Context

`erf-scan-plan` 已冻结增量计划、覆盖分母和内容寻址缓存，但 Pilot 还需要
证明两个边界：安全性无法证明时必须回到完整扫描；性能结论必须来自可复跑、
按规模和冷热缓存分层的证据。单看一次增量耗时既不能说明缓存命中是否安全，
也不能区分“没有规则命中”和“没有执行完整”。

## Decision

新增 `erf-incremental-baseline` 深模块，消费 `ScanPlan`/`CompletenessReport`
的 digest、模式、authority 和覆盖摘要，不重新解析 Git、构建脚本或缓存对象。
`rf-runner` 通过可选的 `FallbackInputs` 接入同一判定：如果判定需要全量，Runner
拒绝用非 `FULL` 计划继续执行；成功或不完整结果同时暴露回退决定和六阶段摘要。

### 全量兜底

以下事实任一成立，增量请求的有效路线变为 `FULL`，并留下结构化原因：

- build configuration、RuleBundle、Policy 或 Resolver identity 变化；
- 依赖图、生成源或 merge-base 无法证明稳定；
- 缓存损坏、过期、identity 不匹配或覆盖不完整；
- 影响范围超过预算；
- build 失败、计划不具备 authority、计划不完整或已被 ScanPlan 放宽为 FULL。

冷缓存 miss 与热缓存 hit 本身不是安全异常；它们分别形成冷/热性能切片。
增量输出只代表快速诊断反馈，直到有完整的全量 authority 结果与之配对。

### 遥测与隐私

`StageTimer` 使用单调时钟 `Instant`，仅保存 resolve、affected-scope、compile、
backend、cache、gate 六阶段的 elapsed nanoseconds 和 64 位摘要输入。模块不接触
source、path、SCM、credential、network、IDE 或完整 Finding；持久报告只含摘要、
计数、状态和 digest。StageTelemetry、BenchmarkCase、CoverageObservation 和
PerformanceReport 的反序列化均重新进入构造器/完整性 digest 校验；缺少阶段、非法
覆盖或篡改报告不能绕过 fail-closed 边界。

### 基线与安全判定

仓库内 Java fixture 固定 SMALL/MEDIUM/LARGE 三种规模并由 JDK 21 编译验收；基线
矩阵再固定 FULL/增量冷缓存/增量热缓存三条切片，同时覆盖 rule hit/zero-hit、build
failure 和 cache corruption。验收脚本对每个规模执行本地 FULL、增量冷缓存和增量热
缓存编译，每条切片重复三次，并把 27 个 compile-stage elapsed nanos 样本及其 digest
注入对应 P50/P95 切片；FULL 编译整个 fixture source set，冷/热增量 probe 编译
确定性的叶子源文件，分别使用 clean/primed output directory。Runner 及其 CLI、Trusted
CI、Pilot observer 的实际 resolve、affected-scope、compile、backend、cache、gate
路径则用 `StageTimer` 记录单调 elapsed nanos，其中 Gate measurement 覆盖实际
`rf_gate` preview。故障注入样本仍保留在回退/完整性计数中，但从正常延迟切片的
P50/P95 分布中排除，避免异常路径污染性能预算。两者均不把源文件写入报告。每条切片报告确定性的 P50/P95、覆盖率、回退计数与输入摘要。预算不满足、缺少
切片、build/cache fault、增量与全量 SemanticResultDigest 或 CompletenessDigest
不一致，均为 `INCONCLUSIVE`/诊断证据，不得转成权威 PASS，也不得降低 Gate 门槛。

测试只证明 seam、回退闭包和报告可重复；它不冒充真实外部仓库、真实 SCM、真实
缓存命中率或生产延迟。真实 Pilot 结果仍需另行登记 observation window、样本和
人工裁决。

## Consequences

- 本地与 CI 可以同时展示“快速反馈 + 权威全量兜底”，且异常原因可逐项审计。
- 性能报告把规模、冷热、命中/零命中和故障注入显式分开，P50/P95 不会把不同
  路线混在一起。
- synthetic baseline 默认可能为 `INCONCLUSIVE`；这是缺乏真实窗口或存在故障
  注入时的安全结果，不是自动放宽策略的理由。
- 生产 SCM connector、IDE UI、凭据和部署继续留在票据范围之外。

## Verification

运行 `scripts/incremental-baseline-check.sh .scratch/incremental-baseline-check`
并以 `bash scripts/check.sh` 作为全量门禁。
