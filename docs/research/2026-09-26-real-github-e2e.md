# 2026-09-26：真实 GitHub 仓库全流程演练

## 范围

本次演练从 GitHub 公开仓库选择样本，未向上游写入任何内容；仓库均 shallow clone 到
`.scratch/real-e2e-20260926/`。规则、扫描结果、Pilot journal 和本地整改提交均属于
临时演练材料，不是生产发布或上游 PR。

样本固定为：

| 仓库 | 初始 commit | 用途 |
| --- | --- | --- |
| `davidmoten/word-wrap` | `53fbb8aec1b2c6f6f6150b7405aec94dcabdb364` | 小型 Maven 正常路径 |
| `iluwatar/java-design-patterns` | `4cabb204f240240bea319605a5ffa46ded728001` | 多模块 Maven、真实发现与整改 |

## 执行链路

1. 对两个仓库执行 `rf init --non-interactive`、`rf config validate` 和 `rf doctor`。
   Doctor 如实报告本地缓存和 TrustState 尚未形成可信生产证明；没有把本地结果伪装成
   Trusted CI。
2. 编写 `pilot.java.no-debug-method` 与 `pilot.java.avoid-duplicate-literals` 两个
   RuleIntent，分别完成 Standard 交叉引用校验、Candidate 生成和 Candidate 测试。
3. 在 `word-wrap` 上扫描：`planned=1`、`executed=1`、`findings=0`、`PASS`。
4. 在 `java-design-patterns` 上扫描重复字面量规则：`planned=183`、`executed=183`，
   首次得到 13 条真实 Finding，Gate 为 `FAIL`。
5. 先建立精确指纹历史债务基线，复扫得到 `PASS`，同时 Finding 仍保留在结果中；这验证
   了 baseline 不是删除 Finding 的旁路。
6. 复核并整改本地演练副本中的重复字面量，涉及 caching、commander、dao-factory、
   fallback、hexagonal-architecture 和 retry 六个模块。第一次整改复扫又暴露了
   `AppAllCases` 中的 `ABCD` 重复字面量，继续修复后最终为 `findings=0`、
   `planned=183`、`executed=183`、`observationErrors=0`、`gateDecision=PASS`。
7. 对整改后的本地 commit `aae3e4c70ec3477f4102fbd6ba7275de36c3a930` 运行 Pilot baseline、
   observe、feedback、governance 和 window-report。结果为：完整率 `1.0000`、新发现趋势
   `0`、历史债务趋势 `0`、p95 `3702 ms`、峰值内存 `8409088 bytes`，Shadow window
   report 为 `PASS`。
8. 对六个整改模块执行 `mvn -q -pl caching,commander,dao-factory,fallback,hexagonal-architecture,retry -DskipTests compile`，通过。

## 演练中修复的问题

- Maven `packaging=pom` 聚合器被误算分析单元：见 [ADR-0171](../adr/0171-maven-aggregator-resolution-boundary.md)。
- Pilot 观察器未绑定 `.ruleops/config.yaml`，同一输入出现 13/119 两个分母：见
  [ADR-0172](../adr/0172-pilot-observer-project-scope-binding.md)。
- Phase 4 每条候选重复 Maven classpath bootstrap 的耗时问题已在本轮之前修复；20 条
  intent 的 Shadow pilot 实测约 91 秒，单条 warm compile/verify 从约 28 秒降到约 2 秒。

## 权限与剩余边界

扫描结果仍标记为 `LOCAL_UNATTESTED`，不是生产 Gate proof；Pilot 报告的 `PASS` 只表示
Shadow 观察窗口工程条件通过，不授权 SCM 写入、发布、受保护分支或生产执行。反馈中的
false-positive regression 是本地演练者对一个意图明确的日志模板 fixture 做的自审，用于
跑通报告聚合，不替代真实 Pilot 所需的人类复核。
