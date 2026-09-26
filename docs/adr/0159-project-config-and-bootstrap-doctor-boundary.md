# ADR-0159: ProjectConfig 与 Bootstrap Doctor fail-closed 边界

- Status: Accepted
- Date: 2026-09-20
- Scope: `rf init`、`rf config`、`rf doctor` 与带配置身份的扫描入口
- Decision record: GitHub issue #108

## Context

现有 `rf` 需要调用方直接提供内部 Host/Resolver 参数，项目范围没有稳定的可审阅配置
身份。新环境中的依赖、Bundle cache 或 TrustState 缺失也不能被误认为可安全扫描。

## Decision

引入版本化 `.ruleops/config.yaml`（闭合的 scalar/inline-array YAML 子集），冻结 Java
语言、Maven/Gradle 构建系统、module、source/test/generated/vendor roots、include/exclude、
execution mode、cache 和 TrustState 路径。`rf init` 只读识别工作区；交互模式要求确认，
`--non-interactive` 遇到不确定、未支持语言、路径越界、重复字段或冲突配置直接失败，不写
配置。来源优先级固定为 CLI > environment > project > defaults，同一来源重复值拒绝。

有效配置确定性派生 `effectiveConfigDigest`、WorkspaceDescriptor digest 和 config ScanPlan
digest。`rf scan` 要求先有有效配置，并把三项身份带入 ScanResult；没有配置不得退回旧的
隐式扫描范围。

Thin Client 在进入 Runner 前重新解析同一份有效配置，并将 module、root、include/exclude
形成的闭合 scope 传给 Runner；Runner 只从 Resolver 描述中保留该 scope 的文件，空 scope
直接返回计划错误。ScanResult 的三项项目身份为必填字段，缺失或非 SHA-256 身份时不生成
结果。

项目控制路径（配置、输出、cache、TrustState 及其已存在父目录）拒绝符号链接；配置路径和
工作区之外的 `rf init --output` 直接失败。缺失的可选配置字段只使用契约默认值，之后仍按
CLI > environment > project > defaults 解析，并在 Doctor 报告中输出来源。

`rf doctor` 独立报告配置、依赖、cache digest 重检、TrustState 和可选 scan probe。Connected
cache miss 只返回需要 Server fetch 的明确状态；Offline/Standalone 缺失、损坏、过期或无法
确认的 cache/trust 保持 `INCONCLUSIVE`。cache 命中不等于签名、freshness、revocation 或
execution identity 已验证；这些检查仍由 RuleBundle delivery/trust verifier 完成。

该边界不负责下载 Bundle、初始化 TrustState、执行 Gate、上传源码或写 SCM。

## Verification

```text
cargo test -q -p erf-project-config --all-targets
bash scripts/bootstrap-check.sh .scratch/bootstrap-check
bash scripts/cli-surface-check.sh .scratch/cli-surface-check
bash scripts/check.sh
```
