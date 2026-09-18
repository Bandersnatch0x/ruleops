---
status: accepted
---

# ADR-0125: Parse Candidate manifest identity from its JSON structure

- Date: 2026-09-18
- Scope: `rf-runner` Candidate validation

## Context

The Candidate manifest is a JSON document whose payload digest is the
integrity boundary for the RuleBundle. A string search for the first `id` or
`digest` field can accidentally select a nested or unrelated field. That
would make malformed or adversarial manifests ambiguous at the point where
the Runner establishes rule identity and content integrity.

## Decision

Parse the manifest as a JSON object and accept only `intent.id` for the rule
identity and the top-level `digest` for the payload digest. Rule identities
remain restricted to the closed ASCII identifier alphabet already used by
the Candidate contract. Malformed JSON, wrong JSON types, missing fields,
empty values, invalid identities, and non-hex or wrong-length digests fail
closed before a Candidate can enter the scan plan.

The payload digest and content-addressed directory checks remain unchanged.
The Runner still derives the effective ScanPlan identity only after all
Candidate validation succeeds.

## Consequences

- Nested decoy fields cannot alter Candidate identity or integrity validation.
- The manifest contract now uses a real JSON parser rather than a
  format-sensitive field search.
- The `serde_json` dependency is part of the locked Rust runtime dependency
  set.

## Verification

`rf-runner` tests cover a valid manifest, missing identity, malformed/invalid
identity behavior, digest tampering, directory mismatch, and decoy nested
identity/digest fields. The Phase 2/3/4 acceptance suites and
`bash scripts/check.sh` remain required.
