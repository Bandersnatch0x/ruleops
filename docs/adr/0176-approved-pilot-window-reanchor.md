# ADR-0176: 事前重锚七天观察窗口与合同 1.1.0

- Status: Accepted
- Date: 2026-10-01
- Issue: #136
- Supersedes: ADR-0129 / ADR-0157 的批准窗口与合同版本部分；其余约束不变。
- Approval source: 项目负责人 2026-10-01 会话指令「裁决由 worker 给出建议，然后按建议推进」，据此采纳 `.scratch/adjudication-recommendations.md` 建议 C / C1 / C2。
- Approval time: 来源仅提供 2026-10-01 日期，未提供时分秒或时区；不补造审批时刻、姓名、审批件或文号。#136 是计划票引用，不将票创建当作独立审批事件。

## Context

旧合同 `pilot-success-contract-v0 / 1.0.0` 固定整月窗口为
`2026-10-01T00:00:00Z` 至 `2026-10-31T23:59:59Z`。
本次会话授权采纳事前建议的完整七天，不依据已出现结果择窗，不倒改历史 ADR。
当前没有人员/基线均已就绪的证据；批准窗口方案不等于已完成全部激活条件。

## Decision

- 合同 id 保持 `pilot-success-contract-v0`，版本从 `1.0.0` 升为 `1.1.0`。
- 新窗口为 `2026-10-02T00:00:00Z` 至 `2026-10-08T23:59:59Z`，UTC、inclusive。
- `start_epoch_secs = 1790899200`，`end_epoch_secs = 1791503999`；
  `end - start + 1 = 604800` 秒，覆盖七个完整 UTC 日（直接相减是 604799 秒）。
- 版本升 minor 记录同一合同系列的新批准观察条件，不改变 wire format：
  `erf-pilot-evidence-package-v1` 保持；不引入通用多窗口框架，也不同时接受新旧版本。
- 阈值、owner、三仓样本、冻结规则、SHADOW 权限、SCM/源码上传限制全部不变。
  包与 record 窗口等值、批准合同/窗口等值、digest 校验保持。
- `approvalReference` 如实引用 #136 及上述会话原话、来源日期和本 ADR。
  仓库审批检查只验证结构与非空字段，不认证会话或凭空生成审批件。

## Activation

配置自本变更起表达新的批准目标；真实采集生效点最早为 `2026-10-02T00:00:00Z`。
治理批准、实现验收、人员/基线就绪三项必须**全部早于**该时刻，等于起点也不满足。
负责人须保留各项完成证据；本 ADR 不代替人员签字、基线批准或实现验收结果。
任一项未及时完成时，新版不得激活，不自动顺延、不缩短、不追溯生效。
仍按旧整月合同及原工具口径处理旧版本活动，并重新申请明确的未来窗口；
不得将旧证据直接送入新版本工具冒充新窗采集。

## Evidence retention and historical scope

旧 1.0.0 合同、观察计划、证据规范的准确字节可由 Git `fb53e19` 对象复核。
落码前将现有旧输入/包/journal 原字节复制到 B 的证据保全根下独立历史区：
`.scratch/evidence-archive/window-reanchor-20261001-ctx11/`，逐件 SHA-256 与来源记录在
`manifest.json`，标为 `superseded-for-new-collection`。源文件与已有 B 快照不改动。
生成版本不能从现物证实的条目标记 `UNKNOWN`；捕获时 HEAD 不冒充生成时 HEAD。
不重写旧窗口、版本或 digest，不重 seal 旧包；历史复核使用原合同与原提交工具。
这些包是旧工程 fixture，并不是已经完成的整月 LIVE_SHADOW 结果。
历史 ADR-0167 等保留当时事实；只替代其对后续采集仍按旧窗口/版本操作的解释，
不替代既有审计事实、人员缺项和人工裁决义务。

## Consequences and acceptance

七天减少覆盖面，不能保证代表完整业务周期或足够的真实作者/质量样本。
须披露样本数量、覆盖、缺项与统计局限，不降低阈值、补造样本或从合成 PASS 推导真实 GO。
聚合保持 `FAIL > INCONCLUSIVE > PASS`；已知必需指标 FAIL 优先 NO_GO，
仅在无已知 FAIL 且缺必需证据时 INCONCLUSIVE；GO 不授予 SCM 或 protected-branch 权限。

回归覆盖独立新值与三份 YAML、旧版本/旧窗拒绝、inclusive 首尾与前后 1 秒、
模式隔离、record 窗口不等、digest/篡改、owner、缺项和已知 FAIL 优先。
定向检查与连续两轮完整门禁须保留日志；实施报告及实测 rc 位于
`.scratch/window-reanchor-implementation.md`。审批记录若不满足校验器真实格式须报告失败，
不得为全绿捏造审批件。本 ADR 本身不宣称上述验收已通过或真实采集已激活。
