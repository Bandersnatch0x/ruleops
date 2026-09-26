# Phase 4 Pilot Report Baseline

- Status: Shadow-only pilot evidence
- Date: 2026-09-18
- Authority: no protected-branch enforcement

## Current evidence

The current implementation has three independently identified Java Shadow
repository paths, recorded in `specs/pilot/representative-java-repositories.yaml`,
and a linux/amd64 trusted execution test path. The Java
inventory contains 100 Standard records; 20 governed RuleIntent paths now have
candidate and fixture evidence, while 80 remain inventory-only. The following
focused suites are available:

- immutable RuleRelease lifecycle;
- signed PolicyProfile and ExceptionGrant lifecycle;
- UnifiedResult and append-only audit chain;
- real Java Runner -> TrustedContext -> Status -> UnifiedResult handoff.

The evidence demonstrates the seams and failure behavior and reaches the
count-based Shadow/twenty-rule qualification target. It remains qualification
evidence, not a protected-branch authorization or a claim that product-level
pilot metrics have been completed.

## Authority boundary

Local feedback and test evidence remain `LOCAL_UNATTESTED` or Shadow-only.
Only a future explicitly approved pilot configuration may turn a verified
trusted status into protected-branch authority. No SCM credential adapter is
part of this repository slice.

## Required next measurements

- paired `author-active-time` baseline across one RuleIntent per selected
  template; the measurement protocol and empty record are in
  `docs/pilot/author-cost-baseline.md`;
- independently selected repositories: maintain 3 or more;
- pilot-qualified rules: maintain 20 or more and measure against the
  pre-registered contract;
- standards inventory: maintain 100 records and promote candidates explicitly;
- new-violation and historical-debt trend;
- complete and `INCONCLUSIVE` rates;
- false-positive regression outcomes;
- rollback and exception age/count;
- scan latency and resource consumption.

Generate a dated acceptance snapshot with:

```text
scripts/phase4-pilot-report.sh .scratch/phase4-pilot-report-<run>
```

The generated report is evidence under `.scratch/`; this document remains the
human-readable baseline and must not be edited to inflate measured coverage.
