---
status: accepted
---

# ADR-0126: Preserve `rf scan` execution semantics in process exit codes

- Date: 2026-09-18
- Scope: `rf-cli` scan process boundary

## Context

The scan output contains a tri-state Gate decision, but callers also need to
distinguish a complete execution from an incomplete attempt. Treating a
structured `INCONCLUSIVE` result as process success allows CI wrappers to
mistake missing execution for a usable result.

## Decision

`rf scan` follows the architecture CLI contract:

- exit `0` for a complete scan, regardless of whether the complete Gate is
  `PASS` or `FAIL`;
- exit `3` for an incomplete execution or execution/runtime failure that
  cannot establish a complete result;
- exit `2` for input, Candidate, plan, or contract errors;
- exit `4` for execution-lock and other trust verification failures.

The structured stdout remains authoritative for `GateDecision`; the process
code only reports execution and trust usability. An incomplete result is
printed before exit `3` so operators retain the attempt transcript and
completeness evidence.

## Consequences

- CI integrations cannot interpret a timeout, crash, or missing unit as a
  successful scan merely because the Runner returned a structured result.
- A complete scan with findings still exits `0`; callers use the printed Gate
  decision for the finding outcome.
- Existing Shadow/LOCAL_UNATTESTED authority boundaries are unchanged.

## Verification

`runtime/rf-cli` unit tests cover the error category mapping. The CLI surface
and Phase 3 incomplete/timeout scripts assert exit `2` for missing identity,
exit `3` for incomplete execution, and complete scans remain exit `0`; the
mapping unit test covers exit `4` for trust errors.
