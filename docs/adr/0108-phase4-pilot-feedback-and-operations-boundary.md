# ADR-0108: Phase 4 pilot feedback and operations boundary

- Status: Accepted
- Date: 2026-09-17
- Scope: Feedback, pilot reporting, and operator handling

## Context

ERF needs a feedback loop from Finding to triage and regression verification,
plus a report and runbook that make the current pilot boundary explicit.
Feedback must not become an uncontrolled source upload or a side channel for
changing a Gate result after publication.

## Decision

Add `erf-feedback` as a digest-only ledger. A `FeedbackRecord` references the
repository, Rule identity, exact finding fingerprint, UnifiedResult digest,
reporter, rationale, ticket, and classification. The four explicit
classifications are confirmed valid finding, documentation gap, scope issue,
and confirmed false positive. Only the confirmed false-positive class creates
a `RegressionCase` whose expectation is `MustBeClean`; the record contains no
source or full Finding report.

The ledger admits records by immutable content digest, makes exact replays
idempotent, rejects a changed record under the same feedback ID, and assigns a
strictly increasing audit operation sequence. Persistence and transport remain
adapter responsibilities.

The pilot report and runbook are deliberately honest about authority: the
current slice exercises one representative Java repository path and
linux/amd64 trusted execution, but remains Shadow-only and does not claim the
three-repository or pilot-qualified-rule exit thresholds. Reports are
generated under `.scratch/`; the persistent baseline is documentation, not a
mutable metrics database.

Operationally, PASS requires complete TrustedContext, FAIL requires remediation
or governed exception, and INCONCLUSIVE requires rerun or escalation. Asset
tampering, stale operations, and rollback always fail closed. No operator path
edits an evidence digest or changes a GateDecision after publication.

## Verification

`runtime/feedback` tests cover validation, classification, regression handoff,
idempotent replay, identity conflict, monotonic operations, digest binding,
and digest-only privacy. `scripts/phase4-pilot-report.sh` runs the three Phase 4
focused suites and the real trusted handoff, then records the Shadow-only
boundary in a generated report.
