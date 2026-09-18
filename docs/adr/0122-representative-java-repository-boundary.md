# ADR-0122: 固化 representative Java repository 的本地资格边界

- Status: Accepted
- Date: 2026-09-18
- Scope: Phase 0/Phase 4 Java Shadow repository qualification

## Context

Phase 0 要求三个 representative repositories，Phase 4 的 Shadow harness
已经在一次运行中生成三个不同的 repository/commit identity。但这些身份
此前只存在于脚本参数和临时目录，无法区分“独立 handoff 身份资格”与“已选
外部生产仓库”。

## Decision

以 `specs/pilot/representative-java-repositories.yaml` 固化三个本地
Java/Maven fixture 的身份、来源、选择理由与边界。三条记录必须与
`scripts/phase4-shadow-pilot.sh` 的运行参数一致，统一保持 `SHADOW`、
`linux/amd64`、无凭据、无网络、无仓库写入与无源代码上传。

本 manifest 只证明三个独立的、可复现的本地 handoff identity；不声称
外部生产仓库代表性、live pilot prevalence、SCM status publication 或
protected-branch authority。未来接入外部仓库时，必须另行记录来源、选择
方法、审批与凭据隔离，不得复用本地 fixture 结论。

## Consequences

- 三个 repository/commit 分母与 Shadow harness 之间可机器校验，身份漂移
  会 fail closed。
- Phase 4 报告可以准确区分 count-based fixture qualification 与 live pilot
  measurement。
- 运行期 fixture 仍只落 `.scratch/`，不会引入外部 SCM 或网络依赖。

## Verification

运行 `scripts/representative-java-repositories-check.sh .scratch/representative-java-repositories`
并以 `bash scripts/check.sh` 作为全量门禁。
