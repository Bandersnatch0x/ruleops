# Author Active-Time Baseline

状态：测量工具与空记录已落地；当前结果为 `INCONCLUSIVE`

## 目的

本切片只测量作者在三条代表性 Java RuleIntent 上的主动工作时间，比较：

1. `manual`：作者手写等价规则产物与 fixtures；
2. `template`：作者填写 RuleIntent，由对应 typed template 生成 Candidate 与 fixtures。

三条固定样本各覆盖一种 Phase 0 模板：

| RuleIntent | Scenario | Target kind | Template |
| --- | --- | --- | --- |
| `corp.java.no-system-exit` | `direct-system-exit` | `method-call` | `SymbolRule` |
| `corp.java.no-clone-method` | `no-clone-method` | `structural-node` | `StructuralRule` |
| `corp.java.avoid-duplicate-literals` | `avoid-duplicate-literals` | `existing-rule` | `ExistingRule` |

机器可读口径见 `specs/pilot/author-cost-baseline.yaml`，空记录见
`specs/pilot/author-cost-record-template.yaml`。

## 测量口径

每条规则使用同一份冻结 RuleIntent、examples、source scope、severity 和验证边界，形成配对比较。两条路径均从作者收到冻结输入开始，结束于等价规则产物及 fixtures 通过验证。

`author-active-time` 只记录作者主动工作的分钟数。编译器、验证器和其他机器等待时间另行记录，不得替代作者时间。每条样本记录：

- manual 路径的主动分钟数、机器耗时和证据引用；
- template 路径的主动分钟数、机器耗时和证据引用。

一份可判定为 `MEASURED` 的记录还必须填写 run id、author id、UTC 起止时间、
路径顺序和测量备注；这些运行元数据缺失时仍保持 `INCONCLUSIVE`。

计算式固定为：

- `delta_minutes = manual_active_minutes - template_active_minutes`；
- `relative_reduction_percent = (manual_active_minutes - template_active_minutes) / manual_active_minutes * 100`；
- 汇总值使用两条路径各自的三样本中位数，再计算中位数相对下降。

预注册阈值来自 `specs/pilot/pilot-success-contract.yaml`：模板路径中位数不高于手写基线中位数的 80%。测量开始后不得调低或重写阈值。

## 复跑

先复制空记录到临时证据目录，填写真实作者数据，再生成报告：

```text
mkdir -p .scratch/author-cost-baseline-<run>
cp specs/pilot/author-cost-record-template.yaml \
  .scratch/author-cost-baseline-<run>/author-cost-record.yaml
scripts/pilot-author-cost-baseline-check.sh \
  .scratch/author-cost-baseline-<run> \
  .scratch/author-cost-baseline-<run>/author-cost-record.yaml
```

不填数据时也可直接运行：

```text
scripts/pilot-author-cost-baseline-check.sh \
  .scratch/author-cost-baseline-definition
```

校验器会交叉核对三条样本的 Standard、Scenario、target kind 和 template，输出 Markdown 报告及 TSV 汇总。任何样本缺少真实主动时间，或 run 元数据仍为占位符，报告均保持 `INCONCLUSIVE`。

## 当前结果与边界

当前仓库只有空记录，没有真实作者工时，因此：

- 手写路径绝对值：`HUMAN_INPUT_REQUIRED`；
- 模板路径绝对值：`HUMAN_INPUT_REQUIRED`；
- 相对变化：`HUMAN_INPUT_REQUIRED`；
- Pilot Go/No-Go：`HUMAN_INPUT_REQUIRED` / `INCONCLUSIVE`。
- run 元数据：`HUMAN_INPUT_REQUIRED`。

该产物不授予 protected-branch authority，不修改 SCM，不上传源码，也不把机器执行时间冒充为作者主动时间。真实作者数据和治理负责人裁决仍需人工补录。
