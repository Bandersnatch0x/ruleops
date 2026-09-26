# ADR-0150: Feature check 必须区分结构证据与行为证据

- Status: Accepted
- Date: 2026-09-19
- Scope: `scripts/*-check.sh` 及其 `.scratch/` evidence report
- Decision record: GitHub issue #120

## Context

部分 feature check 通过文件存在性和 `rg` 关键词确认代码与契约的形状，然后直接生成
`PASS` 报告。这能发现缺失文件或明显漂移，但不能证明运行时行为、关键负例和实际输入已经
通过。全量门禁因此可能出现“包装存在但实现未被执行”的 false PASS。

## Decision

### 1. 封闭证据等级

报告统一使用 `feature-evidence-v1` 的三态：

- `PASS`：可复跑的 Cargo/Maven/fixture 行为命令成功，日志包含被执行的测试或 fixture；
- `STATIC_ONLY`：只执行契约、文件、符号或源码形状检查，不宣称实现行为通过；
- `BLOCKED`：要求行为证据但命令失败、工具不可用或输入无法解析；不得降级成 `PASS`。

结构检查仍保留，因为它能定位契约/命名/装配问题；但必须在报告中单独标为
`STATIC_ONLY`。`bundle-delivery-check.sh`、`server-contract-check.sh`、
`thin-client-check.sh` 和同类边界检查必须同时保存实际命令、输入身份、stdout/stderr 日志，
并在行为失败时保留具体测试、fixture 或契约定位。

### 2. 失败与产品边界

脚本退出非零并生成 `BLOCKED` 报告时，全量门禁停止；缺少测试工具不允许静默生成实现级
`PASS`。这些报告只证明当前 checkout 的工程行为，不升级 Shadow/LOCAL_UNATTESTED 为
Trusted，不创建生产或 Pilot GO。

## Consequences

- 证据消费者可以区分“契约形状正确”和“实现行为实际跑过”。
- feature check 的运行时间略有增加，但失败日志具备可复核的测试名/fixture/输入身份。
- 未来新增结构型检查必须显式选择 `STATIC_ONLY`，新增行为检查必须提供可复跑命令。

## Verification

```text
bash scripts/feature-evidence-check.sh .scratch/feature-evidence-check
bash scripts/bundle-delivery-check.sh .scratch/bundle-delivery-check
bash scripts/server-contract-check.sh .scratch/server-contract-check
bash scripts/thin-client-check.sh .scratch/thin-client-check
bash scripts/check.sh
```
