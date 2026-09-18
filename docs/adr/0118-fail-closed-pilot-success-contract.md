# ADR-0118: Keep the Pilot Success Contract pending until human inputs are locked

- Status: Accepted
- Date: 2026-09-18
- Scope: MVP pilot success criteria and live measurement authorization

## Context

The MVP specification requires a pre-registered Pilot Metrics Baseline and
Pilot Success Contract. The repository now has a digest-only measurement seam,
but it does not have human-approved numerical thresholds, sample strata,
observation dates, comparison method, or adjudicator. Deriving those values
from the deterministic fixture results would invalidate the pre-registration
boundary.

## Decision

Add `PilotSuccessContract` as a versioned machine asset with explicit
`PENDING_HUMAN_APPROVAL` status. Keep all human-owned fields marked
`REQUIRED_HUMAN_INPUT`; the validator rejects an approved status unless the
contract is changed through a future governed decision. The only fixed safety
invariant at this boundary is zero false PASS: failure injection and
incomplete execution cannot be accepted as a successful outcome.

Phase 4 reports and the repository gate may prove that the pending boundary is
well-formed, but must state that the live contract is not approved and must
not report synthetic fixture counters as product outcomes. Approval remains a
human governance action and does not follow automatically from a green gate.

## Consequences

- The next live-pilot implementation has a stable input shape and cannot
  accidentally run with missing thresholds.
- The current Shadow-only authority boundary remains unchanged.
- A human must supply and approve the efficiency threshold, quality comparison
  and tolerance, operations/outcome thresholds, sample strata, observation
  window, comparison method, and decision owner before live-pilot conclusions.

## Verification

```text
bash scripts/pilot-success-contract-check.sh .scratch/pilot-success-contract
bash scripts/check.sh
```

The focused evidence must report `PENDING_HUMAN_APPROVAL`, not `APPROVED`.

## Supersession

This pending-only decision applied before the governed values were supplied.
ADR-0129 records the subsequent human approval of the current contract. The
fail-closed validation rules, zero-false-pass invariant, and Shadow-only
authority boundary remain in force.
