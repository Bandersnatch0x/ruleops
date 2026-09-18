# ADR-0111: Executable protected-branch authority boundary

- Status: Accepted
- Date: 2026-09-17
- Scope: Phase 4 protected-branch authority evaluation

## Context

The Phase 4 pilot deliberately remains Shadow-only. A future protected-branch
adapter needs a small, auditable seam that cannot accidentally treat local
feedback, an unsupported runner platform, or an incomplete scan as an
authoritative result.

## Decision

Introduce `erf-authority` as a pure evaluator over `UnifiedResult`. It first
validates the result, then requires `ResultTrust::Trusted` and the exact
trusted platform `linux/amd64`. Only that combination is authoritative.

The evaluator preserves the Gate tri-state:

- PASS becomes `AllowMerge` and `Success`.
- FAIL becomes `BlockMerge` and `Failure`.
- INCONCLUSIVE becomes `ActionRequired` and `ActionRequired`.

The returned digest is SHA-256 over the canonical UnifiedResult payload, so an
adapter can bind a platform status to the exact result it evaluated. The
module has no SCM, network, credential, persistence, or repository-write
capability; those concerns remain outside this boundary.

## Consequences

- Local and Shadow results cannot acquire protected-branch authority by
  status mapping alone.
- Unsupported architectures fail closed before any merge action is derived.
- An incomplete PASS is rejected by UnifiedResult validation and cannot be
  converted into an allow decision.
- A future transport adapter has one narrow input/output seam, but production
  enforcement is still disabled until a separate deployment decision.

## Verification

Run `scripts/phase4-authority.sh .scratch/phase4-authority-<run>` and inspect
the six focused tests and the generated evidence report. The full repository
gate remains the delivery gate.
