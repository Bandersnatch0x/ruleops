# ruleops

Enterprise Rule Factory (ERF) — 把企业代码规范转化为可验证、可发布、可执行、可审计、可演进的机器检查资产。

- Spec: `docs/Enterprise_Rule_Factory_Architecture_Spec_Plan_V1.2_Final.md`
- 白皮书: `docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md`
- 领域统一语言: `CONTEXT.md`(单上下文 + `docs/adr/`)

## Build & check

| 侧 | 命令 | 位置 |
| --- | --- | --- |
| Java(Factory 单体) | `mvn verify` | 仓库根(`control-plane/*`) |
| Rust(Runtime) | `cargo test` | 仓库根(`runtime/*`) |
| 两侧一键检查 | `./scripts/check.sh` | 仓库根 |

前置:JDK 21+(`JAVA_HOME` 或 PATH)、Cargo。若工具链不在标准路径,可提供可选的
`~/.local/opt/erf-env.sh`(契约:导出 `JAVA_HOME`、`PATH`、可选 `RUSTFLAGS`);
`check.sh` 存在则加载,并显式校验 Java 版本 ≥ 21。

可复现性:Java 插件/JUnit 版本锁定于根 `pom.xml`,`project.build.outputTimestamp`
为固定值;同一 commit 两次 `mvn clean verify` 应产出 byte-identical jar
(`find control-plane -name "*.jar" \| sort \| xargs sha256sum` 前后对比)。
Rust 版本由 `rust-toolchain.toml` 锁定,依赖由 `Cargo.lock` 锁定。

## Layout

- `control-plane/authoring|factory|verification` — Java 21 JPMS 模块化单体,
  按 RuleOps 生命周期能力划分(ADR-0018);Control Plane/Factory 为 Java 单体,
  分析引擎经跨进程契约接入(ADR-0027)
- `runtime/` — Rust Runtime(`rf-cli`、`rf-runner`、`gate`)
- `contracts/proto/` — 版本化跨进程契约(Backend Host / Workspace Resolution)
- `specs/` — 治理资产规范文件
