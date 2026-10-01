# 竞赛证据索引：preflight 与构建摘要

- disclosure: `DIGEST_ONLY`
- selection: `B`
- capture_id: `competition-20261001-5f6f79ebfb3c`
- observed_head: `1e6b56851a9703a0a0ec8f177336686036c9895c`
- generation_head: `UNKNOWN`
- pilot_outcome: `INCONCLUSIVE`

本索引执行 #133 任务 3 的已裁决 B 形态，只收录三仓两轮 preflight 与五份构建证据文件的摘要。冻结目录为 `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/`；目录以排他方式创建，保留源相对路径层级，仅复制 11 份原件，没有移动、删除或修改原件。快照与本地 manifest 均不进 Git；不索引源码、完整 Findings，也不上传日志包。

**第三方干净 checkout 只有摘要，无法独立验证原字节，digest 不构成可访问证据包或签名证明。** 下列路径均相对于持有本地原件的仓库根，属于本地定位符而不是公开下载链接；缺少快照时复验必须失败，不能据摘要声称证据已可访问。

## 1. 摘要与结论边界

### 已证实

- 从本次冻结字节读取，三仓首轮 complete=true 为 3/3，findings 分别为 word-wrap 7、markedj 4、keycloak-protocol-cas 4；第二轮相同。每仓两轮日志全文 SHA-256 相同，因而其中 plan_digest、semantic_result_digest 与所列指标一致。
- 六份扫描日志均记载 gate=Fail、trust=LOCAL_UNATTESTED；执行完整不等于规则门禁通过，findings 不是已确认缺陷数、误报率或收益。
- 构建报告记载同一工作树连续两次 mvn clean verify，6 个控制面 JAR 的路径集、SHA-256 清单与字节比较均 PASS。仅支持**同环境双跑**限制下的报告结论；不证明跨环境可复现、签名信任或生产授权。本次没有重新构建，也没有核验当前 target/ 产物。
- 11 份原件的拷前 SHA、源文件拷后 SHA、快照拷后 SHA 全部一致；其中 9 份与草案旧摘要相同，2 份 Maven 日志不同，见第 3 节。快照文件设为只读用于降低误写风险，不等于不可篡改存储。

| evidence_id | summary |
| --- | --- |
| E01 | word-wrap 第 1 轮：findings=7；complete=true；planned/executed/failed=1/1/0；gate=Fail；LOCAL_UNATTESTED。 |
| E02 | word-wrap 第 2 轮：findings=7；complete=true；planned/executed/failed=1/1/0；gate=Fail；LOCAL_UNATTESTED。 |
| E03 | markedj 第 1 轮：findings=4；complete=true；planned/executed/failed=1/1/0；gate=Fail；LOCAL_UNATTESTED。 |
| E04 | markedj 第 2 轮：findings=4；complete=true；planned/executed/failed=1/1/0；gate=Fail；LOCAL_UNATTESTED。 |
| E05 | keycloak-protocol-cas 第 1 轮：findings=4；complete=true；planned/executed/failed=1/1/0；gate=Fail；LOCAL_UNATTESTED。 |
| E06 | keycloak-protocol-cas 第 2 轮：findings=4；complete=true；planned/executed/failed=1/1/0；gate=Fail；LOCAL_UNATTESTED。 |
| E07 | 留存报告记载同一未变工作树双跑 mvn clean verify、6 个控制面 JAR，两个构建及路径集/SHA 清单/字节比较共五项 PASS；这是报告记载，不是本次重跑。 |
| E08 | 6 条 JAR 历史摘要；条目路径相对于 control-plane/。本索引的 SHA 是清单文件自身摘要，不是 JAR 摘要；本次未复制或校验 JAR。 |
| E09 | 6 条控制面 JAR 相对路径；只有路径清单，不含产物字节。 |
| E10 | 当前留存 Maven 日志；含日志框架提示与 DELIVERY_CAPACITY_PROFILE，未见命令退出码或 BUILD SUCCESS；与草案旧摘要不同，不能独立证明历史构建成功或与报告同轮。 |
| E11 | 当前留存 Maven 日志；含日志框架提示与 DELIVERY_CAPACITY_PROFILE，未见命令退出码或 BUILD SUCCESS；与草案旧摘要不同，不能独立证明历史构建成功或与报告同轮。 |

### 推断

固定样本的双轮一致结果支持本地结果复核能力；它不是企业规模、生产适用性或业务收益证明。不能从同名路径或其余文件摘要一致，推断五份构建文件必定来自同一次执行。

### 未发现证据及历史窗口限制

pilot 总体仍为 **INCONCLUSIVE**。此前本地模拟及只读演练属于历史窗口，不能作为 2026-10-01 至 2026-10-31 正式窗口的完成证据；完整率或单次 PASS 不覆盖总体 INCONCLUSIVE。本 B 索引不扩展收录 pilot 报告，不改变阈值、样本、窗口或判定口径。未测质量人工裁定、资源/延迟基线不得补零或猜测收益；未发现生成 HEAD、跨环境复现、签名证明或正式 pilot 成功证据。

## 2. 逐文件冻结记录

每条 captured_at_utc 是此次本地捕获时刻，不是历史生成时刻；observed_head 是核查时本仓 HEAD，不是样本 commit 或生成版本。generation_head 全部明确 UNKNOWN，不能从当前 HEAD、文件名日期或 mtime 补推。sha256 为原字节 SHA-256；status=CAPTURED 表示本次已保全，不是运行 PASS。

| evidence_id | source_relative_path | snapshot_relative_path | captured_at_utc | bytes | sha256 | observed_head | generation_head | evidence_category | status |
| --- | --- | --- | --- | ---: | --- | --- | --- | --- | --- |
| `E01` | `.scratch/github-java-repositories-20260918/scans/word-wrap/duplicate.log` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/github-java-repositories-20260918/scans/word-wrap/duplicate.log` | `2026-10-01T01:54:05+00:00` | `1007` | `c1f6fb2a986475884d303b54acc5bfcfb9b227f060cd9b7fddb52e7537b395aa` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `scan-log` | `CAPTURED` |
| `E02` | `.scratch/github-java-repositories-20260918/scans/word-wrap/duplicate-run-2.log` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/github-java-repositories-20260918/scans/word-wrap/duplicate-run-2.log` | `2026-10-01T01:54:05+00:00` | `1007` | `c1f6fb2a986475884d303b54acc5bfcfb9b227f060cd9b7fddb52e7537b395aa` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `scan-log` | `CAPTURED` |
| `E03` | `.scratch/github-java-repositories-20260918/scans/markedj/duplicate.log` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/github-java-repositories-20260918/scans/markedj/duplicate.log` | `2026-10-01T01:54:05+00:00` | `1007` | `99a14dcd2e537932ec66ba18903225b269a426bd320abe59af849a3a25d7b585` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `scan-log` | `CAPTURED` |
| `E04` | `.scratch/github-java-repositories-20260918/scans/markedj/duplicate-run-2.log` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/github-java-repositories-20260918/scans/markedj/duplicate-run-2.log` | `2026-10-01T01:54:05+00:00` | `1007` | `99a14dcd2e537932ec66ba18903225b269a426bd320abe59af849a3a25d7b585` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `scan-log` | `CAPTURED` |
| `E05` | `.scratch/github-java-repositories-20260918/scans/keycloak-protocol-cas/duplicate-run-1.log` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/github-java-repositories-20260918/scans/keycloak-protocol-cas/duplicate-run-1.log` | `2026-10-01T01:54:05+00:00` | `1007` | `bbc2e2703cbeff51cf06c1a992fe18390a12434422cea558857b78bf32aafe9b` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `scan-log` | `CAPTURED` |
| `E06` | `.scratch/github-java-repositories-20260918/scans/keycloak-protocol-cas/duplicate-run-2.log` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/github-java-repositories-20260918/scans/keycloak-protocol-cas/duplicate-run-2.log` | `2026-10-01T01:54:05+00:00` | `1007` | `bbc2e2703cbeff51cf06c1a992fe18390a12434422cea558857b78bf32aafe9b` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `scan-log` | `CAPTURED` |
| `E07` | `.scratch/reproducible-java-build/reproducible-java-build.md` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/reproducible-java-build/reproducible-java-build.md` | `2026-10-01T01:54:05+00:00` | `744` | `8708305cc74eb481c05e2640a6a7ed3f53e3a60222269fbfd87e60bc096254da` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `build-report` | `CAPTURED` |
| `E08` | `.scratch/reproducible-java-build/sha256.txt` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/reproducible-java-build/sha256.txt` | `2026-10-01T01:54:05+00:00` | `712` | `9438fa5e84c4257228a9c3f5a75cb486ce7abd9ac3c914b0abe2fab9cc6ead9e` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `sha256-manifest` | `CAPTURED` |
| `E09` | `.scratch/reproducible-java-build/artifacts.txt` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/reproducible-java-build/artifacts.txt` | `2026-10-01T01:54:05+00:00` | `316` | `ff18246bf56d5d1487df8ced37fb6d444b5b932496b70fcd8bffc8828621b9f5` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `artifact-path-list` | `CAPTURED` |
| `E10` | `.scratch/reproducible-java-build/mvn-run-1.log` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/reproducible-java-build/mvn-run-1.log` | `2026-10-01T01:54:05+00:00` | `509` | `a5048a146c001354861d5cdcccab65400ce2133d8e44fa03544d12fbbc5676d5` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `maven-log` | `CAPTURED` |
| `E11` | `.scratch/reproducible-java-build/mvn-run-2.log` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/reproducible-java-build/mvn-run-2.log` | `2026-10-01T01:54:05+00:00` | `509` | `8c80d961cdd11e69f8bd3f5942f95d29c9e994bc061c6e64bfb9c296bf6d21cc` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `maven-log` | `CAPTURED` |

## 3. 与历史候选的差异

对照对象是本地草案 .scratch/evidence-index-draft/README.md 的旧摘要。本次两份日志只称**当前快照，与历史候选不同**，不刷新旧摘要后仍宣称同一轮；变化原因未核实，不认定篡改、删除或正常重跑。草案旧日志原字节未被本次保全；报告与路径/摘要清单虽与草案相同，仍没有可核验的生成版本或跨文件同轮绑定。

| evidence_id | draft_sha256 | captured_sha256 | comparison |
| --- | --- | --- | --- |
| E10 | `77e44a0f9b5b55c3c9a7b56d190af169a6c8bb902dfeae0e2ecc19388457a6e7` | `a5048a146c001354861d5cdcccab65400ce2133d8e44fa03544d12fbbc5676d5` | `DIFFERENT_CURRENT_SNAPSHOT` |
| E11 | `ec34626f74600f1c8d444875bafc71c8c10e37cc9ea4a25b6982c3c4e368de0d` | `8c80d961cdd11e69f8bd3f5942f95d29c9e994bc061c6e64bfb9c296bf6d21cc` | `DIFFERENT_CURRENT_SNAPSHOT` |

## 4. 五个历史缺件：未发现证据

以下仍为 NOT_FOUND，不填 digest、不创建替代快照，不用后续同类报告顶替。目录存在不代表报告存在；当前观测不能判断从未生成、覆盖或删除。snapshot_relative_path 只记录预定位置，不声称文件存在；bytes 为 null。

| source_relative_path | snapshot_relative_path | captured_at_utc | bytes | observed_head | generation_head | evidence_category | status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `.scratch/pilot-live-shadow-114-1/live-shadow.md` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/pilot-live-shadow-114-1/live-shadow.md` | `2026-10-01T01:54:05+00:00` | `null` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `expected-report` | `NOT_FOUND` |
| `.scratch/pilot-live-shadow-114-1/live-shadow-report.md` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/pilot-live-shadow-114-1/live-shadow-report.md` | `2026-10-01T01:54:05+00:00` | `null` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `expected-report` | `NOT_FOUND` |
| `.scratch/pilot-local-simulation-20260918-1/local-simulation.md` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/pilot-local-simulation-20260918-1/local-simulation.md` | `2026-10-01T01:54:05+00:00` | `null` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `expected-report` | `NOT_FOUND` |
| `.scratch/pilot-local-simulation-20260918-1/local-simulation-report.md` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/pilot-local-simulation-20260918-1/local-simulation-report.md` | `2026-10-01T01:54:05+00:00` | `null` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `expected-report` | `NOT_FOUND` |
| `.scratch/pilot-observation-check-20260918/pilot-observation.md` | `.scratch/evidence-archive/competition-20261001-5f6f79ebfb3c/.scratch/pilot-observation-check-20260918/pilot-observation.md` | `2026-10-01T01:54:05+00:00` | `null` | `1e6b56851a9703a0a0ec8f177336686036c9895c` | `UNKNOWN` | `expected-report` | `NOT_FOUND` |

## 5. 原件保管责任

本地工作区持有人（用户/协调者）负责保管上述冻结目录、manifest 与源原件，防止清理 .scratch/、覆盖或遗失；该责任不会因摘要提交 Git 而转移给第三方。代理只完成本次复制与核验，不提供长期存储服务；本次未验证异机备份、访问授权或保留期限，不承诺已具备这些措施。

后续复跑必须使用新的唯一输出目录与 capture_id，不改旧快照、不覆盖旧原件。需要向第三方提供可访问证据包时，应另行确认披露授权、保管与交接方式；当前交付不承担此发布动作。

## 6. 只读复验命令

在持有快照的仓库根执行；同时检查源文件与快照，不运行会覆盖旧证据的扫描/构建/总门禁脚本。任何缺件、漂移、记录畸形或历史缺件重新出现都显式失败；重新出现不自动成为历史证据。

```bash
python3 - <<'PY'
from pathlib import Path
import hashlib, re
index = Path('docs/pilot/competition-evidence-index.md').read_text(encoding='utf-8')
checked = missing = 0
for line in index.splitlines():
    cells = [s.strip().strip('`') for s in line.strip('|').split('|')]
    if len(cells) == 10 and re.fullmatch(r'E[0-9]{2}', cells[0]):
        _, source, snapshot, captured, size, digest, head, generation, category, status = cells
        assert status == 'CAPTURED' and generation == 'UNKNOWN'
        assert re.fullmatch(r'[0-9a-f]{64}', digest) and re.fullmatch(r'[0-9a-f]{40}', head)
        for name in (source, snapshot):
            data = Path(name).read_bytes()
            assert len(data) == int(size), f'SIZE_MISMATCH: {name}'
            assert hashlib.sha256(data).hexdigest() == digest, f'SHA256_MISMATCH: {name}'
        checked += 1
    elif len(cells) == 8 and cells[-1] == 'NOT_FOUND':
        assert cells[3] == 'null' and cells[5] == 'UNKNOWN'
        for name in cells[:2]:
            assert not Path(name).exists(), f'HISTORICAL_MISSING_REAPPEARED: {name}'
        missing += 1
assert (checked, missing) == (11, 5), (checked, missing)
print(f'PASS: {checked} source/snapshot pairs; {missing} historical NOT_FOUND')
PY
```

此命令只验证现存字节与本索引一致，不追溯生成时刻，不证明摘要由可信签署人发布。后续复跑的新输出属于新证据，不能补写此次历史结论。
