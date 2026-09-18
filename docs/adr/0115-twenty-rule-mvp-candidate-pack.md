# ADR-0115: Expand the governed Java MVP candidate pack to twenty rules

- Status: Accepted
- Date: 2026-09-18
- Scope: Phase 2 candidate authoring and Phase 4 Shadow qualification

## Context

The repository had a reproducible ten-rule Java pilot pack, while the MVP
direction requires a broader candidate inventory before a live pilot can be
trusted. The next bounded increment should increase coverage without changing
the authority boundary or pretending that a count-based fixture run is a live
product measurement.

## Decision

Add ten governed Standard/RuleIntent/CoverageLedger pairs, bringing the
current Java candidate pack to twenty paths. The increment uses existing PMD
`ExistingRule` bindings for these intents:

- hard-coded IP addresses;
- explicit garbage collection;
- throwing `NullPointerException`;
- octal values;
- multiple unary operators;
- simplified ternaries;
- useless parentheses;
- primitive wrapper construction;
- decimal `BigDecimal` constructors;
- idempotent operations.

Every path must pass authoring validation, bad/good/holdout/near-miss fixture
verification, deterministic recompilation, and the three-repository Shadow
harness. The harness requires at least twenty RuleIntents and continues to
exercise only Shadow status; it does not enable protected-branch authority,
SCM credentials, repository writes, backend expansion, or network access.

## Consequences

- The current qualified candidate and coverage denominator is twenty, while the
  completed 100-Standard inventory remains a separate promotion boundary.
- The existing ten-rule ADRs remain historical records of the earlier pack;
  this ADR records the current expansion boundary.
- Live pilot metrics still require an independently selected measurement
  window and must not be inferred from these deterministic fixtures.

## Verification

Run the following acceptance commands and inspect their `.scratch/` evidence:

```text
scripts/phase4-shadow-pilot.sh .scratch/phase4-shadow-pilot-20
scripts/phase4-coverage-ledger.sh .scratch/phase4-coverage-ledger-20
bash scripts/check.sh
```

All three-repository Shadow handoffs must remain PASS, with the injected crash
path remaining `INCONCLUSIVE -> ActionRequired`.
