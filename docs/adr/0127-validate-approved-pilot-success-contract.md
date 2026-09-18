# ADR-0127: Validate the approved Pilot Success Contract state without granting authority

- Status: Accepted
- Date: 2026-09-18
- Scope: Pilot Success Contract state validation

## Context

The current Pilot Success Contract is intentionally
`PENDING_HUMAN_APPROVAL`. Its boundary checker previously validated only that
pending shape. A later governed decision needs a machine-checkable way to
review an approved contract without changing the current asset or treating a
synthetic fixture as an approval.

## Decision

Allow the checker to validate an explicitly supplied contract file in either
of two states:

- `PENDING_HUMAN_APPROVAL` requires all fourteen exact human-owned placeholders;
- `APPROVED` requires all fourteen fields to be present and non-empty, no
  placeholder anywhere, and a non-empty approval reference.

Both states must retain the fixed scope, `authority: SHADOW`, and the
zero-false-pass security invariant. Any other state, missing field, changed
scope, changed security threshold, or authority promotion fails closed. The
approved state validates contract shape only; it does not measure the live
pilot, publish a RuleRelease, or enable protected-branch blocking.

The repository-level gate validates the current contract in whichever supported
state is recorded by the governed asset. The state regression script creates a
temporary fixture for the opposite state and exercises both acceptance and
rejection paths, so approval does not weaken the pending boundary.

## Consequences

- Human approval remains a required external governance action.
- Future contract edits can be checked before they are used by pilot tooling.
- The validator no longer needs to be rewritten merely to inspect a governed
  approval, while the current Shadow-only boundary remains unchanged.

## Verification

```text
bash scripts/pilot-success-contract-approval-check.sh .scratch/pilot-success-contract-approval
bash scripts/check.sh
```
