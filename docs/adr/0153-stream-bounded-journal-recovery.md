# ADR-0153: Journal 恢复必须有界流式读取并严格拒绝非法 UTF-8

- Status: Accepted
- Date: 2026-09-19
- Scope: `control-plane/server` local append-only journal recovery
- Decision record: GitHub issue #117
- Related: ADR-0138, issue #94

## Context

`FileRuleOpsJournal` 原先在启动时使用 `Files.readAllBytes()` 加载最多约 4 GiB 的 journal，
并用 `new String(bytes, UTF_8)` 解码。前者把启动内存成本绑定到整个文件，后者会替换非法
UTF-8 字节，可能让损坏记录进入 JSON/审计回放。

## Decision

- `StreamBoundedJournalRecovery` 通过 buffered input stream 逐字节推进 JSON Lines，累计校验
  总 journal 不超过 4 GiB，单条记录不超过 4 MiB，换行边界和尾项完整性必须成立。
- `FileRuleOpsJournal.append` 在同一 append channel 上检查已有文件大小与新记录大小之和，超过
  4 GiB 时拒绝写入，不能把超限问题推迟到下一次恢复。
- 每条记录先用 `CharsetDecoder` 配合 `CodingErrorAction.REPORT` 严格解码，再进入 JSON 和现有
  摘要链/序号/资源/隐私校验；非法 UTF-8、同一行尾随 JSON token 不允许替换、截断或静默跳过。
- 空行、截断尾项、超限记录、超限总文件、损坏 JSON 和既有回放不变量均继续转换为结构化
  `JOURNAL_CORRUPT` 启动错误；成功恢复和 append-before-mutation 语义保持不变。
- 首次启动时明确不存在的 journal 文件仍表示空状态；权限错误、非普通文件、断链路径或其他
  无法读取状态的情况必须抛出启动错误，不得伪装成空 journal。
- 4 GiB 上限用稀疏文件 fixture 验证，避免测试为证明边界而分配真实 4 GiB 磁盘空间；4 MiB
  单条记录用受控 chunk fixture 验证。

## Consequences

- 启动恢复的内存占用由单条记录上限约束，而不是由 journal 总大小决定。
- 损坏字节在 UTF-8 边界就失败，运维得到明确的 fail-closed 启动错误。
- 本地 adapter 仍不承诺跨进程并发写入、数据库事务、远程备份或保留期清理；这些仍属于
  后续部署/adapter 边界。

## Verification

```text
mvn -q -pl control-plane/server -am -Dtest=RuleOpsJournalTest -Dsurefire.failIfNoSpecifiedTests=false test
bash scripts/server-persistence-check.sh .scratch/server-persistence-check
bash scripts/check.sh
```
