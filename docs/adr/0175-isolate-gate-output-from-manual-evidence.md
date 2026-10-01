# ADR-0175: 总门禁输出与人工取证区分离

- Status: Accepted
- Date: 2026-10-01
- Issue: #135
- Decision record: 本次用户明确批准建议 A 的最小迁移范围；2026-10-01 用户继续指令确认协调者裁决 `msg_ea0c74adc0ab`，补充批准三个固定 Phase 3 门禁根的重跑清理。

## Context

总门禁复用独立脚本的旧 `.scratch/` 输出根，可能覆盖构建原件与人工取证。
采用入口隔离和显式传参，而非全局迁移独立默认与历史路径；只隔离总门禁命名，不承诺整个 `.scratch/` 不可变。

## Decision

- `scripts/check.sh` 固定使用 `.scratch/gate/`，不增加环境变量覆盖。47 个固定输出参数保留后缀与阶段顺序；Runtime Pack 两阶段仍共用同一根。
- Java Host E2E 显式下传阶段根的 `compatibility`、`shadow` 子目录；三个 Phase 3 证据脚本接受首参数。无参数仍保持原有时间戳默认。
- 打包检查只在规范化路径精确等于 `.scratch/rule-bundle-pack-check` 或 `.scratch/gate/rule-bundle-pack-check` 时清理自有 `first`、`second`。不放宽至 gate 父目录或任意调用者输出根；自定义已有 bundle 仍拒绝，保留 #126 幂等及数据保护语义。
- 补充重跑范围只在 `scripts/check.sh`：阶段调用前仅清理 `.scratch/gate/phase3-incomplete`、`.scratch/gate/phase3-timeout`、`.scratch/gate/phase3-repeat` 三个固定目录本身；物理 gate 路径不等于固定根时拒绝清理。不按通配、父目录或任意 out 删除，也不扩大上述两个打包自有根的清理权限。
- 原因：首轮成功后，第二轮的 Phase 3 fixture init 拒绝已有 `repo/.ruleops/config.yaml`，repeat 的 `digests.tsv` 也会累计。每轮重建这三个门禁自有阶段目录，保留 init 拒绝覆盖的语义；不加 `--force`，不改三个脚本独立无参的时间戳默认，原失败日志保留。
- 独立脚本保留旧默认，人工取证须显式传唯一输出目录。历史根、历史 ADR 引用与原件不搬、不删；当前文档只更新总门禁最新输出入口。
- `.scratch/github-java-repositories-20260918/candidates` 是模拟输入，保持原位。阈值、样本、权限、判定口径不变。

## Consequences

连续两次完整门禁应成功，旧取证区文件集合与字节摘要应不变；另外验证旧独立默认与自定义 bundle 拒绝行为。
符号链接或人为指定路径可绕开命名约定，执行前须确认 gate 与取证根不互为别名；这不是访问控制机制。
Maven/Cargo 的 target 与系统临时目录仍按原流程写入，不属于此次输出根隔离。
