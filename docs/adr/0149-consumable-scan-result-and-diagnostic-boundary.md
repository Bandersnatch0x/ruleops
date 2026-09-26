# ADR-0149: 以确定性结果投影提供可消费扫描输出

- Status: Accepted
- Date: 2026-09-19
- Scope: `rf scan` result, diagnostics and progress presentation boundary
- Decision record: GitHub issue #107

## Context

`rf-runner` 已经产生确定性 Gate preview、Completeness、Finding fingerprint 和六阶段
遥测，但 CLI 仍主要输出面向旧脚本的几行文本。调用者无法稳定消费 Finding 的文件/行/列
位置，也无法区分 Gate 三态、LOCAL_UNATTESTED 权限边界和 resolver/host/timeout/incomplete
执行错误。

## Decision

### 1. 结果投影

`erf-scan-output` 只消费真实 Runner 结果和 Gate preview，不执行扫描、不复制 Gate 逻辑、
不改变 GateDecision、不提交 Server、不写 SCM。`ScanResult` 使用 `scan-output-v1`，同时
提供：

- 人类可读文本；
- 闭合 JSON；
- 不含源码的 SARIF 2.1.0；失败也保持 SARIF 文档形状，以空 results 和结构化 invocation 表达；
- 独立 JSONL machine progress event。

源 span Finding 输出 digest-only `FindingView` 及 repository-relative path、positive line、
positive column；所有 location kind 另以 digest-only `FindingRecord` 表达。`findingId` 绑定
稳定语义 fingerprint，`findingDigest` 绑定展示视图，不能把 message 或坐标变化伪装成新的
语义 Finding。输出禁止 source/sourceSnippet/fullFinding/credentials。

### 2. 状态、权威和摘要

`gateDecision` 仍严格为 `PASS`、`FAIL`、`INCONCLUSIVE`；`authority` 单独表达
`LOCAL_UNATTESTED`/`TRUSTED`。本地 PASS 的展示状态为 `LOCAL_UNATTESTED`，但保留
`gateDecision=PASS`，避免把本地预览当成生产权威。每个结果带稳定 `resultDigest`；阶段墙钟
耗时只属于 operational progress，不进入结果摘要身份。

### 3. 结构化诊断

每个诊断固定包含 code、stage、message、retry 和 nextStep，可选 detail digest；resolver、
host、sandbox、trust、timeout、plan、rule bundle、incomplete execution、Gate 和
LOCAL_UNATTESTED 均有稳定分类。详细错误不会把源码或凭证带入结果。

### 4. 进程边界

旧退出码保持不变：完整扫描（Gate PASS 或 FAIL）为 `0`，输入/契约为 `2`，不完整或运行时
失败为 `3`，trust failure 为 `4`。结构化结果在退出前写出；机器进度默认可通过
`--progress json` 写到 stderr，不污染 JSON/SARIF stdout。失败发生在六阶段之外时，progress
事件使用闭合集合中的 terminal `gate` 阶段，诊断自身保留精确的 `input`/`trust`/`sandbox`/
`output` 等 stage。

## Consequences

- CLI、CI wrapper 和未来 IDE 可以消费同一个稳定 Finding/diagnostic/progress contract。
- 旧文本 grep 集成继续工作；新集成应选择 JSON/SARIF 和机器进度。
- 输出投影仍不产生 Gate authority；`LOCAL_UNATTESTED` 必须由下游显式处理。
- 阶段耗时可变化但不会改变语义 `resultDigest`。

## Verification

```text
cargo test -p erf-scan-output -p rf-cli
bash scripts/scan-output-check.sh .scratch/scan-output-check
bash scripts/cli-surface-check.sh .scratch/cli-surface-check
bash scripts/check.sh
```
