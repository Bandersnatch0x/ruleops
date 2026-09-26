# ADR-0135: 固化 CI 权威阶段与历史债务边界

- Status: Accepted
- Date: 2026-09-19
- Scope: MVP CI authority, status semantics and baseline separation
- Decision record: GitHub issue #88

## Context

Plan §31.1 要求验证 CI 能在不破坏历史债务治理的前提下阻止新增违规，但当前
交付候选仍是 `SHADOW_ONLY`：没有 SCM 状态发布、仓库写入、SCM 凭证或
protected-branch authority。现有 `erf-authority` 是纯 Gate 结果评估器，
`StatusPublisher` 只验证并映射已签名的三态证明，当前没有真实 SCM transport
adapter、持久化发布状态或凭证边界。

## Decision

### 1. CIAuthorityStage 分三阶段

- `SHADOW` 是当前 MVP 阶段：只采集 digest-only 反事实证据，不发布 SCM 状态，
  不写仓库，不持有 SCM 凭证，不产生合并权限。
- `ADVISORY` 是后续独立阶段：只有完整 `PilotEvidencePackage` 经人工裁决为
  `GO` 后，才可另开并批准 Advisory 规格；它可以发布不阻断状态，但不改变
  合并权限。
- `BLOCKING` 不属于当前 MVP：必须在 Advisory 之后经过单独治理批准，不能由
  Shadow `GO`、本地 trusted CI 测试或状态映射自动开启。

### 2. 三态结果和失败语义保持不变

所有阶段都保留 `PASS`、`FAIL`、`INCONCLUSIVE`：

- 可发布状态时，`PASS` 映射成功，`FAIL` 映射失败，`INCONCLUSIVE` 映射
  `ActionRequired`/error，永不映射为 neutral 或 success；
- 缺失单元、超时、崩溃、信任失败、过期/冲突证明、状态发布失败或比较不完整，
  均为 `INCONCLUSIVE`，不能被 ExceptionGrant 转成 PASS；
- `GateDecision` 是执行结果，`CIAuthorityStage` 是授权边界，两者不得混同。

### 3. 历史债务与新增违规分离

`BaselineState::Historical` 的 Finding 继续单独计入 historical-debt-trend，
不会仅因存量债务阻断；只有 `BaselineState::New`、未被精确 ExceptionGrant
豁免且命中 blocking policy 的 Finding 才能触发 `FAIL`。证据包和窗口报告必须
分别报告 new-violation-trend 与 historical-debt-trend；当前批准合同的两个非正
趋势门槛保持不变，但不要求历史债务归零。

ExceptionGrant 只影响精确 Finding 的例外判定，不删除 Finding、不改写
BaselineState，也不豁免 Completeness 或可信执行失败。

### 4. 阶段晋级门槛

`SHADOW → ADVISORY` 必须同时具备：完整 Evidence Package、人工 `GO`、预登记
三仓库观察窗口、质量/运行/安全证据、候选代表性限制记录，以及单独批准的
Advisory 规格。仅工程验收或 live Shadow 结果本身不够。

`ADVISORY → BLOCKING` 必须另行具备并验证：受信 Runner、GateAttestation、
SCM Publisher、凭证隔离、持久审计、单调且幂等的发布、旧证明拒绝、回滚/撤回、
发布失败和权限失败注入，以及明确的分支策略与责任人。任何一项缺失均不得
开启 Blocking；结果保持 `INCONCLUSIVE` 或停留在当前阶段。

## Consequences

- 当前 Pilot 可以证明 CI 反事实价值和历史/新增债务分离，但不会伪装成 CI
  强制执行能力。
- Advisory 与 Blocking 的产品权限不能由 GateDecision 或 StatusPublisher
  单独推导，避免“有签名结果”被误解为“已有 SCM 权力”。
- CI 失败处理保持 fail-closed：不完整和不可信运行需要重跑或升级，而不是
  用中性状态掩盖风险。
- 未来若要进入 Blocking，必须新增规格、实现票、凭证与审计方案，以及独立
  的治理批准记录。

## Verification

```text
scripts/phase3-trusted-scan.sh .scratch/phase3-trusted-scan
scripts/phase4-pilot-report.sh .scratch/phase4-pilot-report
bash scripts/check.sh
```
