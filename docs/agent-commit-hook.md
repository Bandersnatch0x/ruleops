# Agent 提交校验（自愿本地约束）

## 方案与决策

| 选项 | 收益 | 边界 |
| --- | --- | --- |
| A：Claude Code `PreToolUse` → 共享脚本 | 在工具执行前提示 agent | 宿主事件 matcher 是 `Bash`，不是 `Bash(git commit*)`；需从 stdin JSON 检查命令，shell 包装、alias 和其他工具会漏匹配 |
| B：Git `pre-commit` → 共享脚本 | agent 无关，Git 已准备好本次提交 index，覆盖 `commit -a`、部分提交等 | 本地可用 `--no-verify` 绕过，不是远端权限边界 |
| **C（推荐）：同一共享脚本，两种挂载** | 仅一处策略；Git 作为实际 index 校验入口，Claude 可提前反馈 | 本次最小交付 B 的可执行挂载与 A 的可复制示例，不自动启用任何 hook |

2026-10-01 协调者已确认 C、显式摘要审计 skip，以及非 Java 提交
`NOT_APPLICABLE` 放行。本次不写 ADR，不改变现有 ERF 判定、阈值或 `runtime/status`。

**不变量**：hook 只消费 `rf scan` 的 JSON 结果，绝不从 Finding 数量推导 Gate PASS。
`ALLOW_LOCAL` 只是继续一次本地操作，不是 GateDecision、CI `BLOCKING` 阶段或合并权限。
ADR-0118 的零虚假 PASS、ADR-0135 的当前 `SHADOW` 与历史债务边界仍有效；
本工具不进行基线/例外/阈值重新分类，也不能替代完整仓库 Gate。

## 策略与退出码

| 输入 | 本地动作 / 共享脚本 rc |
| --- | --- |
| `rf rc=0`、`ScanResult`/`scan-output-v1`、明确 `gateDecision=PASS`、合法 authority/status/trust、非空且完整的 Completeness、有效 resultDigest | `ALLOW_LOCAL` / 0 |
| 完整 `FAIL`（**rf 本身仍 rc=0**） | `BLOCK` / 1 |
| `INCONCLUSIVE`、`UNKNOWN`、未知/冲突状态、缺字段、坏/重复键 JSON、零计划 | `BLOCK` / 1 |
| rf rc 2 输入/契约、3 不完整/运行失败、4 信任失败、其他非零 | `BLOCK` / 1 |
| 工具/Candidate/暂存 config 缺失、超时、网络隔离不可用、index 变化、审计不可写 | `BLOCK` / 1 |
| 无新增/修改/重命名后存活的暂存 `.java`（含仅删除 Java） | `NOT_APPLICABLE` / 0，**不是 PASS** |
| 显式非空跳过原因且本地摘要审计落盘成功 | `SKIPPED` / 0，**不是 PASS** |

参数错误由 argparse 返回 2，Git 同样阻断。缺 Python 时 shell 返回非零，也不能放行。
校验器不可用意味着没有可消费的完整结果，不能当作“没有违规”；因此默认 fail closed。
超时默认整个扫描循环 60 秒，GNU timeout 在 TERM 后 2 秒 KILL；每次 Git 调用也限时。

## 安装前提与范围

- Linux/WSL、Python 3、Git、GNU `timeout`、允许用户/网络命名空间的 `unshare`。
- 安装好的本地 rf/runtime（生产 Java Host 需要 JDK 21+）和本地 Candidate；不在提交时安装或编译依赖。
- `.ruleops/config.yaml` 必须进入 index；Candidate/runtime 路径通过环境显式指定。
- 从冻结 index blob 复制**当前待扫 Java 文件**与暂存的 Maven/Gradle 构建描述、项目 config，
  不读工作区 Java 内容，不复制整个源码树；使用原相对路径和 `--include` 精确限制。
- 每个 Java 文件单独扫描，避免某文件被配置排除后同批其他文件成功造成误放行。
  任一文件在 config/resolver 范围外会失败关闭；含 CSV/glob 特殊字符的文件名明确拒绝。
- **首版适合文件局部 Java PMD Candidate**。不提供跨文件类型/依赖闭包、字节码产物、
  全仓历史债务重算或完整发布证明；需要这些上下文的规则不得把缩小范围当全仓验证。
  无法完成的规则由原 Runner 返回不完整并阻断。其他语言/构建规则另走既有 Gate。
- 成本为 O(变更文件数 × rf 启动)，不是全仓扫描；待 rf 提供可验证的逐文件覆盖契约后再合批。
- 强制 `offline`，另用 Linux 网络命名空间切断 IP 网络；工具不配置服务器、凭据或外发。
  这是可信本地工具的网络隔离，不宣称隔离恶意可执行文件/本地 Unix socket。
- stdout 和 `.git/ruleops-commit-audit.jsonl` 只含状态、计数、时间、索引/结果/错误/原因摘要；
  不转发 rf 原始 stdout/stderr、源码、路径、Finding 或 skip 原因明文。临时源码快照自动清理。

## B：Git 挂载（建议先用；以下命令由操作者自行执行）

**本次交付未修改当前仓库 hooks 配置。** 先检查已有 hooks，不覆盖其他人的 hook；
如果已有 pre-commit，应在原脚本中显式串接共享入口并保留非零退出码。

```sh
export RULEOPS_COMMIT_RF=/absolute/installed/rf
export RULEOPS_RUNTIME_HOME=/absolute/installed/runtime
export RULEOPS_COMMIT_CANDIDATE=/absolute/local/candidate
# 仅在确认没有要保留的 hooksPath/其他 hooks 时启用此目录：
git config --local core.hooksPath scripts/hooks
# 或手工串接：python3 /absolute/repo/scripts/agent-commit-check.py
```

上例 hook 从目标仓库 `scripts/agent-commit-check.py` 加载实现。若将脚本用于其他项目，
应把共享脚本与 hook 一起拷入该项目，或在自有 pre-commit 中固定受信脚本绝对路径。
所有受检仓库变更应先纳入版本控制，再由使用者明确启用；不要让 agent 私自安装。

## A：Claude Code 提前提示（示例，未写入本仓 `.claude/settings.json`）

Claude 的 `PreToolUse` 对 `Bash` 匹配后向 command hook 的 stdin 发送 JSON；
`tool_input.command` 才是待执行的命令。下面的小适配器保存到**操作者选定**的
绝对路径，例如 `$HOME/.local/bin/ruleops-claude-pretool.py`：

```python
import json, os, shlex, subprocess, sys
try:
    payload = json.load(sys.stdin)
    words = shlex.split(payload.get("tool_input", {}).get("command", ""))
    if payload.get("tool_name") != "Bash" or words[:2] != ["git", "commit"]:
        sys.exit(0)  # 非目标事件，不代表校验通过
    result = subprocess.run(
        ["python3", os.environ["RULEOPS_COMMIT_CHECK"]],
        cwd=payload["cwd"], capture_output=True, text=True, timeout=75,
    )
    if result.returncode == 0:
        sys.exit(0)
except (KeyError, OSError, ValueError, AttributeError, subprocess.TimeoutExpired):
    pass  # 下面统一向宿主明确报错，不放行
print("Local commit validation blocked; inspect the local digest audit.", file=sys.stderr)
sys.exit(2)  # Claude command hook 阻断码；Git/shared 1 不能直接用作宿主阻断码

```

项目或用户 settings 的示意（路径由操作者替换，继承上节三个环境变量，
另将 `RULEOPS_COMMIT_CHECK` 指到受信共享脚本）：

```json
{
  "hooks": {
    "PreToolUse": [{
      "matcher": "Bash",
      "hooks": [{
        "type": "command",
        "command": "python3 /absolute/ruleops-claude-pretool.py || exit 2",
        "timeout": 90
      }]
    }]
  }
}
```

**A 只提前检查直接的 `git commit ...`，不作为唯一约束。** `git -C`、alias、shell
包装/复合命令不保证匹配，`git commit -a/--only` 等还会改变待提交 index；因此必须保留 B
在 Git 真正准备提交内容后再检查。A 不匹配或 rc=0 不表示 Gate PASS。
Claude 宿主版本须自行验证上述 stdin/exit-2 协议；本次实测 Git 挂载及适配器 stdin/退出码，未启动 Claude。

## 绕过、审计与退出

```sh
RULEOPS_COMMIT_SKIP_REASON='人工确认的本次紧急原因' git commit -m 'emergency'
```

只有非空原因且审计 fsync 成功才 `SKIPPED`。不要在原因中放秘密；记录的是 reasonDigest，
不是原因正文，也不更改 RuleException、Baseline 或 SCM 状态。该环境变量只用于一次命令，
不要永久 export。缺工具且不能形成校验结果时仍须由人明确使用这条跳过入口。

退出使用：恢复安装前的 `core.hooksPath`（原本未设才执行
`git config --local --unset core.hooksPath`），并移除自行添加的 Claude hook 条目。
`git commit --no-verify`、更改 hook 文件/环境和卸载 hook 都能绕过本地约束，
**这些路径不会经过本脚本，无法承诺审计**；若需强制执行与不可篡改审计须另行治理，
不能借本工具越过 ADR-0135。执行前后 index digest 检查也不等于原子 SCM 权限控制。

## 一条端到端样例

以下在一个已启用 B、已配置本地 `corp.java.no-system-exit` Candidate 的临时 Java
项目内运行，项目 config/pom 已暂存。`rf` 完整 FAIL 仍 rc=0，由 hook 的 JSON 策略阻断：

```sh
printf '%s\n' 'class A { void run() { System.exit(1); } }' > src/main/java/A.java
git add src/main/java/A.java
git commit -m 'violation'  # Git 非零，摘要 code=FAIL，commit 未创建
printf '%s\n' 'class A { void run() {} }' > src/main/java/A.java
git add src/main/java/A.java
git commit -m 'compliant'  # Git rc=0；摘要 action=ALLOW_LOCAL，非生产 Gate PASS
```

## 可复跑验证

```sh
# 测试替身只验证消费/阻断边界，不冒充真实 PMD：
timeout 60s python3 scripts/agent-commit-hook-test.py .scratch/hook-boundary-UNIQUE
# 加上真实离线 runtime/Candidate：
timeout 90s python3 scripts/agent-commit-hook-test.py .scratch/hook-real-UNIQUE \
  --real-rf /absolute/installed/rf \
  --candidate /absolute/local/corp.java.no-system-exit \
  --runtime-home /absolute/installed/runtime
```

缺本地资产时，可在提交检查**之外**用已有 `scripts/production-runtime-pack-check.sh OUT`
构建 runtime，用 `scripts/candidate-compile.sh specs/rule-intent/corp.java.no-system-exit.yaml OUT`
生成 Candidate；遵循既有本地工具链配置，不把构建/联网下载塞进 hook。
测试只在系统临时仓库启用 hooks，结果摘要写显式指定输出目录。
本任务按授权只跑针对性验证，**未跑 `scripts/check.sh`，不声称全量门禁通过**。
