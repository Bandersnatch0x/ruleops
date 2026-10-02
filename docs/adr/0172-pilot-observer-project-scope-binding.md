# ADR-0172：Pilot 观察器绑定项目扫描作用域

- 状态：Accepted
- 日期：2026-09-26
- 决策者：ERF Pilot Observer / Project Config

## 背景

同一真实仓库和同一 Candidate 在执行 CLI 与 Pilot 观察器时，若观察器不加载项目配置，
会采用底层 Runner 的开放作用域。演练中 CLI 按 `.ruleops/config.yaml` 发现 13 条，观察器
却发现 119 条，造成观察结果无法与日常扫描比较。

## 决策

`rf pilot baseline` 与 `rf pilot observe` 接受 `--project-config FILE`，在启动 Runner 前：

1. 校验配置路径位于仓库根目录内；
2. 加载并解析 ProjectConfig，应用相同的环境优先级；
3. 转换为 Runner 的 `ProjectScanScope`，绑定 modules、source roots、include 和 exclude；
4. 配置缺失时保留底层观察器的低级调用语义，但登记窗口的正式命令必须显式传入配置。

## 后果

- Pilot baseline、observation 与 `rf scan` 使用同一分母和文件选择规则；
- 作用域配置错误在观察开始前失败，不会用开放作用域产生看似完整的错误数据；
- 真实演练中作用域基线从 119 条收敛到 CLI 相同的 13 条；整改复扫为 0 条。

## 验收

- `cargo test -p erf-pilot-observer`：6 个测试通过，包含项目作用域加载测试；
- 真实 `java-design-patterns` Pilot：`complete=true`、`new_findings=0`、
  `historical_findings=0`，窗口报告的 Shadow engineering gate 为 `PASS`。
