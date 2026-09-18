# ADR-0114: Publish Coverage Ledgers for the ten-rule pilot pack

- Status: Accepted
- Date: 2026-09-17
- Scope: Phase 4 Standard and RuleIntent coverage evidence

## Context

The Coverage Analyzer and one SEC-JAVA-017 ledger already establish a fixed
denominator, but the current pilot had ten governed Standard revisions and
only one durable CoverageLedger file. A report that counts ten RuleIntents
without corresponding scenario ledgers leaves the Standard-to-coverage
boundary implicit.

## Decision

Add one `CoverageLedger` YAML for each current Standard revision SEC-JAVA-017
through SEC-JAVA-026. `phase4-coverage-ledger.sh` checks that the ten ledger
identities match the ten Standards, compares every declared Scenario set, and
rejects invalid rule bindings. Existing PMD-backed paths are explicitly
classified as `EXISTING_RULE`; the six structural pilot paths are
`NEW_RULE_REQUIRED` with `rule: null` and `partial` status.

The script also runs the Java coverage module tests and emits evidence under
`.scratch/`. These ledgers describe the current ten-rule pilot boundary; they
do not inflate it into the architecture plan's 100-standard MVP inventory and
do not claim live pilot metrics or production authority.

## Consequences

- Every current Standard Scenario has a durable, reviewable coverage row.
- Existing capability and new rule work remain distinguishable in both YAML
  and the generated report.
- Adding a Standard now requires a matching ledger before the Phase 4 snapshot
  can pass.

## Verification

Run `scripts/phase4-coverage-ledger.sh .scratch/phase4-coverage-<run>` and
inspect the ten scenario diff files, module test log, and generated report.
The full `bash scripts/check.sh` gate remains required.
