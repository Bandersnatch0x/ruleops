# Authoring Capability Baseline

- Status: implemented baseline, not a live pilot result
- Date: 2026-09-18
- Authority: non-authoritative authoring perimeter

The Java Authoring module now provides the minimum V0.1 perimeter required by
the architecture:

- deterministic `AuthoringOrchestrator` grouping, fixed task coverage,
  explicit terminal states, source provenance, and single-task fallback;
- immutable `AuthoringRetrieval` index generations with exact ID lookup,
  metadata filters, ACL/lifecycle filtering, and lexical BM25 ranking;
- `AuthoringSuggestion` output with required snapshot/config provenance and
  all trust flags fixed to false.

Only governed metadata and approved summaries enter the retrieval index. No
repository source, credentials, full Finding report, external review product,
or vector database is used. Capability errors, timeouts, empty results and
unauthorized references degrade or discard authoring output; they do not alter
Rule maturity, EvidenceBundle, RuleRelease, Policy or Gate state.

Run the focused evidence pack with:

```text
scripts/authoring-capability-check.sh .scratch/authoring-capability
```

Reviewer precision, active-time improvement, holdout quality, observation
window and other live pilot thresholds remain owned by the human Pilot Success
Contract. The three-rule author-active-time measurement protocol is now
registered in `specs/pilot/author-cost-baseline.yaml`; its empty record remains
`HUMAN_INPUT_REQUIRED` until real author timings are supplied.
