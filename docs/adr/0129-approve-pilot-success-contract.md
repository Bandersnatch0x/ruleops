# ADR-0129: Approve the preregistered Pilot Success Contract

- Status: Accepted
- Date: 2026-09-18
- Scope: Phase 4 Shadow-only pilot success criteria

## Context

The Pilot Success Contract was intentionally held at
`PENDING_HUMAN_APPROVAL` while its fourteen human-owned fields were missing.
The contract validator and the fail-closed state regression were implemented in
advance so that approval could not be represented by placeholders or inferred
from deterministic fixtures.

On 2026-09-18 the governance decision recorded in GitHub issue #79 approved the
proposed contract values. The approved asset defines the efficiency, quality,
operations, and outcomes thresholds; the three-repository sample and strata;
the paired comparison method; the 2026-10-01 through 2026-10-31 observation
window; the decision owner; and the acceptance method.

## Decision

Set `specs/pilot/pilot-success-contract.yaml` to `APPROVED` with all fourteen
human-owned fields populated. Keep the scope fixed at 100 Standard records,
20 candidate RuleIntents, and 3 Java Shadow repositories. Keep
`authority: SHADOW`, `false-pass-count` threshold `0`, and the rule that
injected failures or incomplete executions cannot pass.

This approval authorizes evaluation against the preregistered criteria only. It
does not claim that live pilot metrics have been measured, does not promote any
rule to protected-branch blocking, and does not authorize SCM credentials.

## Consequences

- Future pilot evidence can be judged against fixed, reviewable criteria.
- Missing, stale, tampered, injected, or incomplete evidence remains
  fail-closed and cannot become a successful outcome.
- The Phase 4 report must continue to distinguish contract approval from live
  outcome measurement and protected-branch authority.

## Verification

```text
bash scripts/pilot-success-contract-check.sh .scratch/pilot-success-contract
bash scripts/pilot-success-contract-approval-check.sh .scratch/pilot-success-contract-approval
bash scripts/check.sh
```
