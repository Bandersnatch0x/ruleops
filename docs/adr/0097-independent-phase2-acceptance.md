---
status: accepted
---

# Use an independent local-slice acceptance oracle

Phase 2 acceptance separates protocol behavior from semantic Gate behavior.
The versioned conformance runner drives a fresh reference Host for negotiation,
framing, accounting, cancellation, deadline and crash probes. A second set of
checks consumes only the shared typed `BackendObservation` and `UnitStatus`
contracts and invokes the local Gate preview; it does not import PMD,
SpotBugs, Fake Host implementation details or renderer wording.

The acceptance oracle requires deterministic canonical Finding ordering and
deduplication, keeps `BaselineState` and `ExceptionDecision` visible and
orthogonal, excludes diagnostic message text from `SemanticResultDigest`, and
rejects malformed typed locations. Missing, failed or otherwise incomplete
required execution is always `INCONCLUSIVE`, including when a matching
exception exists. Acceptance output is reviewer-readable evidence under
`.scratch/` and remains `LOCAL_UNATTESTED`/`SHADOW`; it has no SCM or protected
branch authority.
