# ADR-0119: Keep authoring orchestration and retrieval outside execution authority

- Status: Accepted
- Date: 2026-09-18
- Scope: V0.1 Authoring Orchestrator and retrieval baseline

## Context

The V0.1 architecture requires a small Authoring Orchestrator and an
ID/metadata/FTS baseline. The repository already had strict authoring parsers
and a Candidate compiler, but no deterministic task boundary or retrieval
contract. Adding an external review product, vector database, or model output
directly to the runtime would violate the product's trust and source-locality
boundaries.

## Decision

Add two ERF-owned, in-memory capabilities under the Java Authoring module:

- `AuthoringOrchestrator` freezes a caller-provided source/config identity and
  task set, sorts and groups tasks deterministically, records exactly one
  terminal enum state per planned task, and falls back from a failed or empty
  group to single-task execution. Every result carries fixed planned/accounted
  coverage and remains non-authoritative.
- `AuthoringRetrieval` builds immutable index generations over governed
  metadata and approved summaries. It supports exact authority-ID lookup,
  metadata filters, ACL/data-class filtering, lifecycle filtering, and a
  deterministic lexical BM25 baseline with stable tie-breaking. Its index
  digest binds the full searchable document and ACL content.

Both capabilities emit only `AuthoringSuggestion` (`erf.authoring/v0alpha1`).
The contract requires source snapshot and config digest identities, records
scope and provenance references, and fixes all trust flags to false. Missing
identity, invalid ACL references, capability failure, timeout, rejection and
empty output never become a verified/pass state. No suggestion is parsed as a
Finding, EvidenceBundle, RuleRelease or GateDecision.

No OpenCodeReview or zvec dependency is introduced. Dense retrieval remains a
future experiment and cannot enter the execution, signing or Gate path.

## Consequences

- Authoring failures are observable as degraded task outcomes without losing
  the denominator or mutating authoritative assets.
- Retrieval can be rebuilt from a fixed source snapshot and index generation,
  while ACL and revoked/retired asset filtering are applied before ranking.
- The implementation is a baseline seam, not evidence of live pilot quality,
  reviewer precision, or a human-approved Pilot Success Contract.

## Verification

Run:

```text
scripts/authoring-capability-check.sh .scratch/authoring-capability
bash scripts/check.sh
```

The focused tests cover deterministic grouping, fallback, fixed coverage,
provenance/ACL rejection, exact lookup, metadata filters, BM25 repeatability,
index digest binding, lifecycle filtering, and the non-authoritative output
boundary.
