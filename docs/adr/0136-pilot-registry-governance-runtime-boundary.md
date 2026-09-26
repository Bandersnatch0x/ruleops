# ADR-0136: 固化真实 Shadow Pilot 的 Registry/Governance 运行边界

- Status: Accepted
- Date: 2026-09-19
- Scope: Real Shadow Pilot persistence, governance lifecycle and recovery boundary
- Decision record: GitHub issue #91

## Context

RuleRelease Registry、Policy Governance Registry 与 Feedback Ledger 已经在核心层
验证签名身份、内容摘要、不可变状态、撤销/回滚、幂等和审计语义，但当前存储仍是
进程内实现。真实 Shadow Pilot 需要跨进程、跨运行可复核的最小状态；它仍然不应
被误认为生产 Registry、SCM 写入能力或 protected-branch authority。

## Decision

### 1. 真实 Shadow Pilot 的最小持久化边界

Pilot 必须使用追加式、digest-only 的本地 journal/evidence 记录，至少保留：

- RuleRelease、PolicyProfile、ExceptionGrant 的固定身份、版本、签名职责、内容摘要，
  以及能重新解析其已登记签名资产的内容引用；
- 观察窗口、仓库/样本、运行和反馈的身份，治理快照及每条证据的摘要和完整性状态；
- 每个生命周期转换的状态、操作序号、actor、reason、审计序号和链式摘要；
- 最近可信 `TrustCheckpoint` 的身份、摘要、有效期和最高已见版本。

journal 必须支持幂等追加、顺序与篡改检查，并能在状态缺失或不确定时保留
`INCONCLUSIVE` 事实。Pilot 不要求生产级多写入数据库、SCM transport 或远端
Registry API。

### 2. 资产生命周期与保留

RuleRelease、PolicyProfile 和 ExceptionGrant 的身份不可覆盖或物理删除；其签名
摘要、版本、验证结果、激活/撤销/回滚转换及审计上下文必须可追溯。撤销和回滚是
新的、带操作序号的状态事件，不是对历史记录的改写。ExceptionGrant 继续要求
精确目标、责任人、原因、批准/票据和有限有效期。

Feedback 只要求保留追加式的摘要/标签记录、幂等身份、关联运行/窗口和审计上下文，
不把源码或完整 Finding 作为 Shadow Pilot 的默认治理资产。

Pilot evidence、治理快照和审计链至少保留到观察窗口结束、最终
`PilotAdjudication` 完成以及治理负责人登记的后续保留期；不得在报告生成后立即
删除。保留责任人和期限必须在进入窗口前登记。源码、完整 Finding 和 SCM 传输日志
可以由另行授权的诊断或外部系统保管，但不属于本边界的默认持久化要求。

### 3. 信任角色、操作与恢复

发布、策略、例外、门禁证明和信任 checkpoint 使用分离的签名职责。操作人员不能
签署或批准自己负责的资产，治理负责人拥有 Pilot 最终裁决权；作者、运行人员和
标注人员只能提供带身份的证据。

恢复只能通过更高版本的可信 checkpoint 或有审计记录的恢复流程完成。状态丢失、
回滚不明、版本回退、审计链断裂或校验无法确定时，结果必须为 `INCONCLUSIVE`。
不使用共享凭据，也不允许代理、操作员或恢复流程绕过签名、新鲜度、撤销和审计检查。

### 4. Pilot 与生产授权的边界

Pilot Go/No-Go 的必要条件是：上述本地 digest-only journal 可复核，资产身份/版本、
审计、撤销/回滚和幂等追加均成立，保留责任人已登记，恢复演练通过，且
`PilotEvidencePackage` 完整。它不要求开启任何 SCM 权限。

生产授权阶段另行处理：多写入 Registry、SCM Publisher 与凭据隔离、protected-branch
策略、备份/高可用/SLO、完整反馈 UI/API，以及需要保留原始源码或完整 Finding 的产品
存储。Shadow Pilot 的工程验收或 `GO` 不自动开启这些能力。

## Consequences

- 真实 Shadow Pilot 获得可跨进程复核和可恢复的最小治理状态，同时继续保持
  `SHADOW` 的只读权限边界。
- 运行核心可以保持 transport-neutral；本地 journal 是 Pilot 证据适配器，而不是
  生产 Registry 的替代品。
- 撤销、回滚、恢复和保留都成为可审计的治理事实；不确定性不能被清理、重试或
  例外记录伪装成成功。
- 进入 Advisory 或 Blocking 前仍需单独规格、实现、权限隔离和治理批准。

## Verification

```text
bash scripts/check.sh
```

真实窗口开始前，Pilot 操作票还必须记录 journal 重启/恢复演练、撤销/回滚演练、
审计链完整性和保留责任人；这些属于人工 Pilot 证据，不由本 ADR 虚构为已完成。
