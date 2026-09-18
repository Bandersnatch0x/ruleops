# ADR-0112: Align the Phase 4 pilot report with qualified Shadow evidence

- Status: Accepted
- Date: 2026-09-17
- Scope: Phase 4 generated pilot acceptance snapshot

## Context

At the time of this decision, the persistent Phase 4 baseline recorded three
distinct Shadow repository identities and ten governed Java RuleIntents. The
generated pilot report still described the older one-repository/five-rule
slice and did not execute the qualification script, creating a reporting gap
rather than an implementation gap.

## Decision

`phase4-pilot-report.sh` now composes and verifies the release registry,
governance registry, unified-result, three-repository Shadow pilot, and
protected-branch authority evidence. At the time, it asserted the Shadow
report's ten RuleIntents and three repository count before writing the dated
snapshot. The current twenty-rule expansion and its `>=20` assertion are
recorded in ADR-0115.

The report distinguishes count-based qualification from product metrics that
need a live, independently selected pilot window: new-versus-historical debt
trend, false-positive regression outcomes, rollback/exception age and count,
and scan latency/resource consumption. None of those metrics is inferred from
the focused suites.

The snapshot continues to state that authority is Shadow-only. The authority
evaluator proves the input boundary but does not enable SCM credentials,
repository writes, source upload, or protected-branch enforcement.

## Consequences

- The generated report cannot silently regress to the old five-rule or
  one-repository description while the qualification script remains green.
- A report consumer can distinguish evidence of executable seams from evidence
  of live product outcomes.
- Running the full snapshot is more expensive because it reruns the current
  twenty rule paths and three real trusted handoffs, but the result is
  reproducible and inspectable under `.scratch/`.

## Verification

Run `scripts/phase4-pilot-report.sh .scratch/phase4-pilot-report-<run>` and
inspect the generated `phase4-pilot-report.md`, `shadow/phase4-shadow-pilot.md`,
and authority evidence. The full `bash scripts/check.sh` gate remains required.
