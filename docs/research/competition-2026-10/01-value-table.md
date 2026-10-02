---
title: 比赛申报价值表（研发运营效能）
date: 2026-09-30
repository: ruleops
status: draft-for-submission
authority: SHADOW
sources:
  - docs/Enterprise_Rule_Factory_Whitepaper_V1.1.md (§21)
  - specs/pilot/pilot-success-contract.yaml
  - specs/pilot/live-java-repositories.yaml
  - docs/pilot/live-java-repositories.md
---

# 比赛申报价值表

把白皮书 §21「企业运营驾驶舱」的四组 KPI 映射到赛则「综合价值评判」三维
（客户价值 / 产品价值 / 项目价值），作为 50 分那一栏的数据底表。

**登记纪律**：本表只登记可核验、可复算证据。preflight（窗口前）证据一律标注为
preflight，**不得充当 live pilot outcome**；缺测项记 `待测`，不填 0、不填估计值。

状态口径：`已实测` = 仓库内有固定样本与可复算命令；`待测` = 机制就绪、缺真实输入；
`边界` = 机制存在但当前权限/范围受限，如实登记；`Defer` = 仅保留内部兼容面，不作公开分发承诺。

## 1. 客户价值（对客户实际业务能力与满意度的提升）

| KPI | 证据来源（可复算） | 现值 | 状态 |
| --- | --- | --- | --- |
| 规范资产总量 | `specs/standard/SEC-JAVA-017..116.yaml` | 100 条 Standard | 已实测 |
| 可机器检查的候选场景 | `specs/coverage/SEC-JAVA-017..036.yaml` | 20 条 Scenario | 已实测 |
| 覆盖分类 | `specs/pilot/phase0-template-selection.yaml:16` | 14 已有规则 / 6 须新写 / 0 语义待澄清(blocked) / 0 不可机器检测 | 已实测 |
| 三态 Gate + Completeness | `specs/pilot/live-java-repositories.yaml` | preflight 3/3 仓库 `complete`，Gate `FAIL`（发现违规） | 已实测(preflight) |
| 真实仓库违规发现 | 同上 | word-wrap 7 / markedj 4 / keycloak-cas 4 findings，重复扫描 2 次一致 | 已实测(preflight) |
| 新增 vs 历史债务趋势 | `pilot-metrics-baseline.yaml:21-34` | 机制就绪，无窗口数据 | 待测 |
| 例外/回滚治理 | `pilot-metrics-baseline.yaml:56-76` | 机制就绪，无窗口数据 | 待测 |
| 扫描延迟 / 资源 | `pilot-metrics-baseline.yaml:77-90` | 机制就绪，无窗口数据 | 待测 |

**核心卖点（可写入申报）**：把「0 告警即通过」换成「Completeness + 三态 Gate + 签名执行证明」；
对 3 个真实公开 Java 仓库的 preflight 已证明扫描 **complete=3/3** 且可重复，而非「没报错就当作通过」。

## 2. 产品价值（推广价值与受益范围）

| KPI | 证据来源 | 现值 | 状态 |
| --- | --- | --- | --- |
| Backend-neutral 架构 | 白皮书 §10、ADR-0027 | PMD 主 / SpotBugs 补，Seam 未绑死单一引擎 | 已实测 |
| 模板路径 | `phase0-template-selection.yaml:31` | 3 类：ExistingRule 13 / StructuralRule 6 / SymbolRule 1 | 已实测 |
| Rule 成熟度谱 | 白皮书 §13 | 当前全部 `SHADOW`，未晋级 `BLOCKING` | 边界 |
| 受益范围声明 | 白皮书 §24 | Java 起，可扩语言与引擎（声明，非实测） | 声明 |
| 已交付执行路径 | `docs/pilot/operator-quickstart.md`、`docs/pilot/competition-evidence-index.md` | rf-runner / `./rf` 本地扫描与证据执行路径；不代表 Server/IDE 公开分发已验收 | 已实测（限定本地执行） |
| 内部兼容面 | 白皮书公开范围说明、`clients/vscode/README.md`、`clients/intellij/README.md` | RuleOps Server 与 IDE/CLI 薄客户端仅供内部兼容维护，不作公开分发承诺；真实 IDE 验收未证实 | Defer |

## 3. 项目价值（提效与里程碑）

| KPI | 证据来源 | 现值 | 状态 |
| --- | --- | --- | --- |
| 规则作者提效（Author Active Time） | `specs/pilot/author-cost-baseline.yaml`、`docs/pilot/author-cost-baseline.md` | 工具与空记录就绪，无真实作者工时 | 待测 |
| 可复现构建 | `docs/pilot/competition-evidence-index.md` | 6 个 JAR byte-identical，5 项 PASS | 已实测 |
| 工程规模 | `runtime/*`、`control-plane/*`、`scripts/*` | Rust 40+ crates / Java 6 模块 / 50+ 检查脚本 | 已实测 |
| Verification / Evidence | ADR-0070、`specs/pilot/pilot-evidence-package.yaml` | 独立验证 + 证据包封存机制就绪 | 已实测 |

## 4. 诚实边界（申报时必须如实标注）

- preflight 数字来自窗口前只读演练，`authority = SHADOW`，不授予 protected-branch 权力。
- 当前 checkout 显式禁用 release/gate/promote 与 SCM 回写（ADR-0167）；扫描权威恒为 `LOCAL_UNATTESTED` / `SHADOW`。
- 效率类 KPI（author-active-time）与质量类 KPI（precision/recall）不得用机器耗时或预演数据顶替人工输入；仅无已知 `FAIL` 且缺必需证据时报告为 `INCONCLUSIVE`，已知必需指标 `FAIL` 优先 `NO_GO`（ADR-0157、`specs/pilot/pilot-evidence-package.yaml`）。
- 推广范围与长期演进为路线声明，不是已交付事实。
- 日期落窗不等于治理已激活：ADR-0176 的七天批准目标不证明全部事前条件已满足；当前材料未证实正式短窗激活，须由负责人核对起点前的治理批准、实现验收及人员/基线就绪证据，不得追溯补造或把本周采集直接列为正式短窗成果。


## 5. 价值数字化（口径、分母与复算路径）

本节补充前三节的测量口径，不把机制测试折算为业务收益。基线为 `5730539`；
以下命令在 Linux 仓库根运行。源码计数是当前资产盘点；历史运行数字必须先验原件，
缺件即不可复验。新运行一律用唯一 `.scratch/` 目录，不覆盖历史证据。

### 5.1 已实测资产与 preflight

| 指标 / 公式 | 已有数字与证据 | 复算路径与限制 |
| --- | --- | --- |
| 规范资产数 = 指定编号区间实际存在的 Standard 文件数；候选数 = selection 条目数 | 100 个 Standard、20 个候选；`specs/standard/`、`specs/pilot/phase0-template-selection.yaml` | 执行 V1；20/100 只能表示本期候选规模相对盘点规模，不能作企业规范覆盖率或规则准确率。 |
| 复用候选比例 = EXISTING_RULE 数 / 候选数；新增候选比例同理 | 14/20、6/20；模板分布 13/6/1 | V1 从 selection 逐条计数，不仅引用 summary；SymbolRule 可来自 EXISTING_RULE，分类数与模板数不混用。 |
| 首轮完整率 = 首轮 complete=true 仓库数 / 首轮仓库数；重复一致率 = 两轮日志字节相同仓库数 / 双轮仓库数 | 均为 3/3；首轮 findings 为 7、4、4 | V2 验证 `docs/pilot/competition-evidence-index.md` 原件摘要，再按 E01–E06 复算；只是固定规则、固定提交的 preflight，不是整个仓库所有规范扫描，更不是 15 个已确认缺陷。 |
| 同环境构建一致率 = 报告中双跑字节一致 JAR 数 / 报告中比较 JAR 数 | 留存报告为 6/6，五项检查 PASS | V2 验原件后阅读 E07；E08/E09 为六条摘要/路径清单，可用 `wc -l` 复核清单规模；本次不核验 JAR 字节，不据此宣称跨环境可复现。E10/E11 无独立构建成功证明，不能把五份文件当成同轮的五项 PASS。 |

**V1：资产与候选盘点（只用 Python 标准库）。**

```bash
python3 - <<'PY'
from pathlib import Path
from collections import Counter
import re
standards = [Path(f'specs/standard/SEC-JAVA-{n:03}.yaml') for n in range(17, 117)]
coverage = [Path(f'specs/coverage/SEC-JAVA-{n:03}.yaml') for n in range(17, 37)]
assert all(p.is_file() for p in standards + coverage), 'INVENTORY_MISSING'
s = Path('specs/pilot/phase0-template-selection.yaml').read_text()
selection = s.split('\nselection:\n', 1)[1].split('\nadjacentGapScenarios:', 1)[0]
n = len(re.findall(r'^  - id:', selection, re.M))
categories = Counter(re.findall(r'^    category: (\S+)', selection, re.M))
templates = Counter(re.findall(r'^    template: (\S+)', selection, re.M))
assert sum(categories.values()) == sum(templates.values()) == n
print('standards=', len(standards), 'coverage_files=', len(coverage), 'candidates=', n)
print('categories=', dict(categories), 'templates=', dict(templates))
PY
```

**V2：历史原件先验真、再复算。** 以下会执行索引 §6 的既有只读校验代码；
11 对原件/快照和五项历史缺件检查失败即停止，不重建或补写任何历史文件。

```bash
python3 - <<'PY'
from pathlib import Path
import re
text = Path('docs/pilot/competition-evidence-index.md').read_text()
check = text.split('## 6. 只读复验命令', 1)[1].split("python3 - <<'PY'\n", 1)[1].split('\nPY', 1)[0]
exec(compile(check, 'competition-evidence-index:section6', 'exec'), {})
rows = {}
for line in text.splitlines():
    c = [v.strip().strip('`') for v in line.strip('|').split('|')]
    if len(c) == 10 and re.fullmatch(r'E\d{2}', c[0]):
        rows[c[0]] = Path(c[2])
first = [rows[k].read_text() for k in ('E01', 'E03', 'E05')]
complete = [re.findall(r'\bcomplete: (true|false)\b', s) for s in first]
assert all(len(v) == 1 for v in complete), 'COMPLETENESS_FIELD_MISSING_OR_DUPLICATE'
print('first_round_complete=', sum(v == ['true'] for v in complete), '/', len(first))
print('identical_pairs=', sum(rows[a].read_bytes() == rows[b].read_bytes()
    for a, b in [('E01','E02'), ('E03','E04'), ('E05','E06')]), '/3')
for k in ('E01', 'E03', 'E05'):
    counts = re.findall(r'^findings=(\d+)\b', rows[k].read_text(), re.M)
    assert len(counts) == 1, 'FINDING_COUNT_MISSING_OR_DUPLICATE'
    print(k, 'findings=', int(counts[0]))
for k in ('E07', 'E08', 'E09'):
    print(k, rows[k], 'lines=', len(rows[k].read_text().splitlines()))
PY
```

Backend-neutral、独立验证和封存属于**能力证据**，不是可直接相加的收益数字。
若要量化多 Backend 覆盖，应冻结同一 RuleIntent 与各 Backend 支持集合，分别报告
完成/拒绝/失败数，不将不同引擎结果简单相加。工程目录数量仅为规模盘点，
可用 `find runtime -name Cargo.toml`、`find control-plane -name pom.xml` 与
`git ls-files 'scripts/*'` 查清单；不以 crate、模块或脚本数量证明提效。
本地执行、Verification/Evidence 的重跑入口见 `docs/pilot/competition-evidence-index.md`
与 `docs/pilot/observation-implementation.md`；机制通过不替代实际窗口 outcome。

### 5.2 待测项的采集规程（不是测量结果）

共同前置：负责人先确认正式窗口激活、登记样本/提交、冻结 rule-bundle、责任人和
合同已有阈值；本节不批准激活、不新设或改写阈值。保留起止 UTC、环境、命令、退出码、
输入/输出摘要及真实缺测原因。仅匹配登记样本的 `LIVE_SHADOW` 进入正式窗口；
preflight、LOCAL_SIMULATION 和 SYNTHETIC_SHADOW 分开存放。

| 待测项 | 公式与真实输入 | 具体采集、复算和缺测处理 |
| --- | --- | --- |
| 新增违规 / 历史债务趋势 | 每次 `new_finding_count`、`historical_finding_count`；窗口变化 = 末次计数 − 首次计数 | 固定 baseline，按时间顺序对登记提交执行 `rf pilot observe`；记录 repository/commit、measurement-id 与 journal。按仓库/规则包分组对比，不用重新建 baseline 掩盖新增项；至少有两次可比较观测才写趋势，否则待测。 |
| 窗口完整率 / INCONCLUSIVE 率 | complete runs / runs；decision=INCONCLUSIVE runs / runs | 从同模式、同窗口 journal 用 `rf pilot window-report` 聚合；分母为零或记录不完整不填 0。完整执行与 Gate PASS 分列。 |
| 例外 / 回滚治理 | 最新 governance 快照的 rollback_count、active_exception_count、oldest_active_exception_age_secs | 由治理负责人提供事件账本和有效例外清单，以窗口末时刻计算最老有效例外年龄（秒），通过 `rf pilot governance` 写入具 actor/reason/time 的摘要；累计快照不可逐行求和，未确认空集合不填 0。 |
| 延迟 / 资源 | latency_millis 的 count/sum/max/窗口分布；peak_memory_bytes 的 max/分布 | 同一冻结环境连续观测，保留每次计时与峰值采集来源，记录冷热缓存/机器配置；先登记真实基线，才比较窗口分布。机器耗时不得替代人工耗时；未采到内存不填 0。 |
| 误报回归、precision / recall | regression outcome 分类计数；precision=TP/(TP+FP)，recall=TP/(TP+FN) | 人工独立裁定 finding 并保留证据，以 `rf pilot feedback` 记录分类和回归结果；recall 还需独立已知正例/漏报全集，不能仅看告警。分母为零或缺独立裁定均待测；开发者 fixtures 不算独立质量测量。 |
| false-pass | 独立注入无效执行却返回 PASS 的次数 | 按合同独立注入失败/不完整条件，留输入、预期、实际 decision 和验证记录；正常扫描未见错误不等于 0 次误 PASS。 |
| 作者主动时间 | 每规则 delta=manual−template；降幅=delta/manual×100%；汇总用两路径各自中位数再计算降幅 | 按 `specs/pilot/author-cost-baseline.yaml` 三条固定样本配对，冻结等价输入与验证边界；人工记录主动分钟、机器等待、顺序及运行元数据，结束于等价产物/fixtures 通过验证。空记录、非正 manual 或缺任一样本均不宣称提效。 |

**规程命令（需真实输入，不是可无条件执行的成功演示）：**

```bash
# 新建一次采集目录；下列定义检查 PASS 只证明口径，不证明 KPI 达标。
RUN=$(mktemp -d "$PWD/.scratch/competition-measure-XXXXXXXX")
bash scripts/pilot-metrics-baseline-check.sh "$RUN/definition"
cp specs/pilot/author-cost-record-template.yaml "$RUN/author-cost-record.yaml"
# 人工填写上述真实主动时间和元数据后再运行；空模板仍应 INCONCLUSIVE。
bash scripts/pilot-author-cost-baseline-check.sh "$RUN/author-report" "$RUN/author-cost-record.yaml"
# 查实际参数契约与既有调用链；不可用猜测参数拼出“成功”报告。
rg -n 'fn print_usage|--journal|--actor|--reason|--observed-at|--latency-baseline|--memory-baseline' runtime/pilot-observer/src/main.rs
sed -n '60,114p' scripts/pilot-live-observation.sh
```

观测完整调用模板及前置构建见 `scripts/pilot-live-observation.sh`，人工操作规程见
`docs/pilot/live-shadow-window-execution-checklist.md`。该脚本当前是预演入口，
按执行时刻生成区间并预期报告 INCONCLUSIVE，**不能直接作为正式窗口成果采集器**；
正式采集应按批准区间使用上述 observer 命令，调用 `window-report` 显式传
`--window-start`、`--window-end`、`--mode live-shadow`、`--registered-sample`，
资源基线只传经批准的真实数值。完整聚合规则由 `runtime/pilot-metrics/src/lib.rs`
实现，不另写一套阈值判定；已知必需项 FAIL 优先 NO_GO，其余必需证据缺失保持 INCONCLUSIVE。

## 6. AI 加分（可证明的整合面，不预判赛方分数）

| 整合面 | 文件与证据命令 | 已证明能力 / 边界 |
| --- | --- | --- |
| AI 辅助规则创作接缝 | `scripts/standard-rule-extract.py`：`rg -n 'class StandardRuleExtractor|class DeterministicStandardRuleExtractor|needs_review|usableForGate' scripts/standard-rule-extract.py`；A1 计数 | 调用方注入 OFFLINE provider，保留 source/config/provider/extractor 摘要，未知形状进 needs_review；当前默认是确定性映射，不是已接入 LLM，没有在线模型、网络适配器或质量收益证据。 |
| 草稿到语义验证 | `scripts/standard-rule-extract-test.py`：`rg -n 'ValidateCmd|CandidateCmd|VerifyCmd|semantic|holdout|nearMiss' scripts/standard-rule-extract-test.py` | 经已有校验、编译、javac 与真实 PMD 7.9 验证，并检查类型语义正负例；provider 只提议，不能授予 trust、coverage 或发布权。开发者自写语义用例不是独立 holdout，空 holdout/nearMiss 仍阻止独立质量声明。 |
| agent 提交钩子 | `scripts/agent-commit-check.py`、`scripts/agent-commit-hook-test.py`；`timeout 60s python3 scripts/agent-commit-hook-test.py "$RUN/hook"` | 对 staged Java 的 index 快照逐文件扫描，FAIL/INCONCLUSIVE、超时、未知 JSON、完整性或审计失败均 BLOCK；不是仅检查工作目录。显式 opt-in，非 Java 为 NOT_APPLICABLE；有理由的 skip 为 SKIPPED，不是扫描通过。ALLOW_LOCAL 仅本地许可，不是 protected-branch/SCM 权威；默认测试含 doubles，不等于真实 PMD 端到端。 |
| 后端 MCP 只读工具面 | `clients/mcp/erf-mcp-server.js`；`node -e "console.log(require('./clients/mcp/erf-mcp-server.js').tools.map(t=>t.name))"`；`timeout 60s node --test clients/mcp/test/mcp.test.js` | 精确四工具：rf_doctor、rf_config_show、rf_scan、rf_explain；固定 rf、argv 数组、参数白名单、输出投影与超时，保留 CLI decision。只读指不暴露治理/写类命令，不保证 scan/构建零文件写入，也不是目录沙箱。 |
| WebMCP 查看面 | `clients/mcp/webmcp/app.js`、`server.js`、`README.md`；`rg -n 'modelContext|registerTool|readOnlyHint|127.0.0.1' clients/mcp/webmcp/{app,server}.js`；`timeout 60s node --test clients/mcp/webmcp/test/*.test.js` | 页面复用四工具与后端验证器，人与 agent 共用结果；仅 loopback/同源可信本机使用。浏览器 API 实验性，版本号不证明激活；单测不证明真实浏览器注册。WebMCP 补充后端 MCP，不是服务端 MCP transport，不产生 Gate/SCM 权威。 |

**A1：历史草稿计数复算（本地留存；不是 LLM 准确率）。**

```bash
python3 - <<'PY'
from pathlib import Path
from collections import Counter
import hashlib, json
p = Path('.scratch/standard-to-rules-20261001-192848/tests-final/draft/inventory.json')
x = json.loads(p.read_text())
claimed = x.pop('draftDigest')
actual = 'sha256:' + hashlib.sha256(json.dumps(x, sort_keys=True, ensure_ascii=False,
    separators=(',', ':')).encode()).hexdigest()
assert actual == claimed, 'DRAFT_DIGEST_MISMATCH'
c = Counter(r['status'] for r in x['records'])
assert sum(c.values()) == x['census']['entryCount']
assert not any(x['trust'].values())
print('provider=', x['source']['provider'], 'entries=', sum(c.values()), dict(c))
print('sourceSnapshot=', x['source']['sourceSnapshot'], 'draftDigest=', claimed)
PY
```

本地固定样本为 142 条、1 条 draft、141 条 needs_review；待人工复核比例 = 141/142，
仅描述该文档在当前映射下的处理分布，不与 Standard inventory 的 100 条混为同一分母。
历史 inventory 不进 Git，缺件即无法复验；摘要自洽不证明独立来源认证。原文不在当前
受限 Linux 仓库内，本次不读取外部挂载、不重跑原文抽取；需经授权把真实输入放在
Linux 路径，再用 `python3 scripts/standard-rule-extract.py SOURCE.html NEW_OUTPUT_DIRECTORY`
及 `python3 scripts/standard-rule-extract-test.py SOURCE.html NEW_TEST_DIRECTORY` 复跑，
输入摘要必须与历史 sourceSnapshot 对应；新输入不能冒充旧样本。

**申报可说**：已有非权威草稿接缝、受限 agent 提交检查和四工具人机共用查看面。
**申报不可说**：已由 AI 自动生成全部规则、已测得 AI 节省工时或准确率、工具调用成功
等于规则通过、WebMCP 已在任意浏览器激活、或任一入口获得生产/SCM 授权。
