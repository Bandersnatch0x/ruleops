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
仓库门禁通过 `scripts/reproducible-java-build-check.sh` 自动执行这两次构建并
比较 JAR 路径、SHA-256 与字节内容，证据写入 `.scratch/`。
Rust 版本由 `rust-toolchain.toml` 锁定,依赖由 `Cargo.lock` 锁定。

## Layout

- `control-plane/authoring|factory|verification` — Java 21 JPMS 模块化单体,
  按 RuleOps 生命周期能力划分(ADR-0018);Control Plane/Factory 为 Java 单体,
  分析引擎经跨进程契约接入(ADR-0027)
- `runtime/` — Rust Runtime(`rf-cli`、`rf-runner`、`gate`)
- `contracts/proto/` — 版本化跨进程契约(Backend Host / Workspace Resolution)
- `specs/` — 治理资产规范文件

## CLI

当前 checkout 的 V0.1 薄 CLI 为 `./rf`：

```text
./rf standard validate FILE...
./rf rule validate FILE...
./rf rule generate INTENT.yaml OUTPUT_DIR
./rf rule test CANDIDATE_DIR OUTPUT_DIR
./rf coverage analyze OUTPUT_DIR
./rf pilot report OUTPUT_DIR
./rf scan --repo-root PATH --host COMMAND (--rule-id ID | --candidate DIR)
```

`rf scan` 返回 `0` 表示执行完整（Gate 可为 PASS 或 FAIL），`3` 表示
INCONCLUSIVE/执行不完整，`2` 表示输入或契约错误，`4` 表示 trust failure。

release promote/revoke、gate authority、feedback 与 exception 命令会显式
拒绝；它们需要后续的人工作业或外部权限，不由本地 V0.1 dispatcher 伪造。
