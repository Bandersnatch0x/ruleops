# ADR-0107: Unified result and append-only audit chain

- Status: Accepted
- Date: 2026-09-17
- Scope: Phase 4 result and audit closure

## Context

The Gate, TrustedContext, and Status Publisher already enforce separate
boundaries, but a consumer still needs one stable summary that cannot lose
Completeness, BaselineState, ExceptionDecision, or the distinction between a
trusted result and local feedback. Audit records also need a deterministic
relationship across planning, execution, Gate, attestation, and publication.

## Decision

Introduce `erf-unified-result` with two cooperating values:

- `UnifiedResult` binds repository/commit, policy, RuleBundle,
  ExecutionLock, RunnerRelease, WorkspaceDescriptor, and SemanticResultDigest
  identities to Completeness, finding counts, BaselineState/ExceptionDecision
  counts, the tri-state `GateDecision`, mapped `ResultStatus`, trust level, and
  optional GateAttestation subject.
- `AuditTrail` is an append-only hash chain. Each event records a monotonic
  sequence, event kind, subject digest, previous event digest, and its own
  digest. The chain covers scan planning, execution completion, finding-set
  computation, Gate evaluation, attestation, and publication without storing
  source or full Finding content.

`ResultStatus` is a projection only: PASS maps to Success, FAIL to Failure,
and INCONCLUSIVE to ActionRequired. It is not a fourth Gate state. A unified
result rejects an incomplete PASS. A Trusted result must carry a
GateAttestation subject; a LocalUnattested result must not claim one.

The trusted CI handoff constructs `UnifiedResult` only after the unchanged
GateAttestation has crossed TrustedContext and the Status Publisher. It checks
attestation identity, SemanticResultDigest, Completeness, and GateDecision
against the same GatePreview used for publication.

## Consequences

- Local feedback and trusted CI share one result shape without implying equal
  authority.
- Consumers can map tri-state Gate outcomes to platform status semantics
  without losing `INCONCLUSIVE`.
- Audit tampering, reordering, and event omission are detectable from the
  chain, while default privacy remains aggregate and digest-only.
- The crate remains transport-neutral; SCM writes, persistence, and secret
  custody stay in adapters.

## Verification

`runtime/unified-result` tests cover construction, tri-state mapping,
fail-closed invariants, attestation mismatch, deterministic digest, and chain
tampering. `scripts/phase4-unified-result.sh` additionally runs the real
Runner -> Trusted CI integration and requires all seven handoff checks to pass.
