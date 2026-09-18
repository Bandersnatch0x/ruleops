# ADR-0116: Keep the 100-standard inventory separate from the 20-rule candidate pack

- Status: Accepted
- Date: 2026-09-18
- Scope: MVP Java Standard inventory and candidate promotion boundary

## Context

The MVP scope calls for 100 Java standards and 20 candidate rules. A single
directory stores both kinds of governed Standard records, so a count-only
coverage check could accidentally treat an inventory record as an implemented
rule. The current candidate pack already has RuleIntent, CoverageLedger,
Candidate, fixture, and Shadow evidence; the remaining inventory does not.

## Decision

Complete the inventory as `SEC-JAVA-017` through `SEC-JAVA-116`, all version
`1.0.0` and language `java`. Keep exactly 20 RuleIntents and use their explicit
Standard references as the candidate-pack boundary. CoverageLedger and pilot
qualification scripts must resolve only those 20 referenced Standards, while a
separate inventory check enforces the 100-record denominator and authoring
schema.

The 80 inventory-only Standards are planning and governance records. They do
not imply a backend binding, Candidate artifact, EvidenceBundle, live metric,
protected-branch authority, SCM access, network access, or source upload. Each
must be separately promoted through the existing Standard -> RuleIntent ->
Candidate -> Verification workflow before it can enter the candidate pack.

## Consequences

- The MVP inventory target is measurable without inflating executable coverage.
- Current Phase 4 reports can state both denominators: 100 inventory records
  and 20 qualified candidate paths.
- Adding a future candidate requires an explicit RuleIntent and matching
  CoverageLedger; adding an inventory row alone cannot change pilot evidence.

## Verification

```text
bash scripts/standard-inventory-check.sh .scratch/standard-inventory-check
scripts/phase4-coverage-ledger.sh .scratch/phase4-coverage-ledger-100-20
bash scripts/check.sh
```

The full gate must remain green, with all pilot authority boundaries intact.
