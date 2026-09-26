# ADR-0171：Maven 聚合 POM 不作为 Java 分析单元

- 状态：Accepted
- 日期：2026-09-26
- 决策者：ERF Workspace Resolver / Java Runtime

## 背景

在真实 GitHub 仓库 `iluwatar/java-design-patterns` 上运行自定义重复字面量规则时，
静态 Resolver 把 `packaging=pom` 的 Maven 聚合模块也登记成 Java 分析单元。聚合模块没有
自己的 Java source root，Runner 因此产生 6 个 `java-source-roots:none` 观察错误，完整扫描被
错误降为 `INCONCLUSIVE`。

## 决策

Resolver 读取每个 Maven POM 的 `packaging`：

- 根 POM 仍作为工作区入口处理；
- `packaging=pom` 的子模块只负责递归发现其 children，不加入分析单元集合；
- 其他 packaging 的子模块按原规则加入分析单元；
- 模块访问仍使用 `visited` 集合，重复模块或模块环继续 fail closed。

聚合器是拓扑容器，不是 Java 源码执行单元；子模块的 source root、unit id 和执行分母保持
独立可追踪。

## 验收

- `cargo test -p erf-static-resolver`：8 个库测试与协议测试通过；
- 真实 `java-design-patterns`：`planned=183`、`executed=183`、`failed=0`、
  `observationErrors=0`、`gateDecision=PASS`；
- 该修复只改变工作区单元解析，不放宽 Gate、Trust 或 Completeness 条件。
