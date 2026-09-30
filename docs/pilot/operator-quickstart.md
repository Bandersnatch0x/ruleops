# ERF 使用路径说明

这份说明面向规则使用者和规则作者；已登记窗口观察员按内部清单执行。日常使用不要求编辑 Pilot 证据 YAML。

## 日常使用：三步

在目标 Java 仓库根目录执行：

```text
./rf init --repo-root .
./rf config validate --repo-root .
./rf scan --repo-root . --candidate CANDIDATE_DIR --format text
```

在 CI 或其他没有交互终端的环境，把初始化改为
`./rf init --repo-root . --non-interactive`；识别不确定时该选项会安全失败，要求先审阅
配置，而不会静默猜测。

需要接入 CI 时，将最后一步改为：

```text
./rf scan --repo-root . --candidate CANDIDATE_DIR --format sarif --output scan.sarif
```

用户只需要查看 Finding、Completeness 和 `PASS` / `FAIL` / `INCONCLUSIVE`。当前版本仍是本地
或 Shadow 权限，不会写 SCM，也不会把结果自动升级为受保护分支决策。

## 规则作者：生成与验证

规则作者只处理规则资产本身：

```text
./rf rule validate specs/rule-intent/<intent>.yaml
./rf rule generate specs/rule-intent/<intent>.yaml .scratch/candidate
./rf rule test .scratch/candidate .scratch/candidate-test
```

作者不需要填写作者编号、路径顺序、机器耗时或证据路径。若要参加正式 Pilot 的作者成本
研究，才另行按照 `docs/pilot/author-cost-baseline.md` 采集配对数据。

## 已登记窗口角色指引

已登记窗口的观察负责人按[内部窗口执行清单](live-shadow-window-execution-checklist.md)与[观测运行说明](observation-implementation.md#运行入口)执行。

登记窗口观测按登记清单只读获取三个固定仓库，自动绑定 commit、规则包摘要并生成 digest-only
journal。观察员不上传源码、不写 SCM；质量人工复核和治理裁决属于窗口后处理。

## 哪些内容属于内部取证

以下内容只用于正式 Pilot 的审计附件，不是日常操作：

- 手写与模板路径的作者主动时间；
- 机器等待时间和资源基线；
- precision、recall、holdout、near-miss 与 false-positive 复核；
- `AuthorCostRecord`、证据包和最终 `PilotAdjudication`。

缺少这些研究输入时，Pilot 结论保持 `INCONCLUSIVE`；不需要为了让日常用户继续使用而填入
占位数据。
