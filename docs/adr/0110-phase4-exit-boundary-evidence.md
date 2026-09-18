# ADR-0110: Phase 4 exit boundary evidence

- Status: Accepted
- Date: 2026-09-17
- Scope: Ontology determinism, privacy, and trusted platform

## Context

The Phase 4 plan requires more than lifecycle code and pilot counts. The
ontology pack must rebuild deterministically, default evidence must not become
source collection, and the trusted platform boundary must remain explicit.

## Decision

`scripts/phase4-exit-boundary.sh` compiles the fixed Core Ontology plus Java
extension at a named revision twice, compares canonical N-Triples bytes and
manifest identity/digest, and proves an unknown IRI fails the closed-world
compiler. The compiler output is an immutable digest-bound read model; Turtle
files in Git remain the authoritative write source.

The script also runs the existing GateAttestation aggregate-disclosure test,
the UnifiedResult test suite, and TrustedContext tests. Static checks pin the
trusted conformance and RuleRelease platform declarations to `linux/amd64`,
while Gate previews remain `LOCAL_UNATTESTED` and the pilot report remains
`SHADOW`-only.

No SCM, network, database, source upload, full Finding upload, or protected
branch mutation is added by this evidence slice.

## Verification

Run `scripts/phase4-exit-boundary.sh .scratch/phase4-exit-boundary-<run>` and
inspect the generated report, both ontology packs/manifests, the invalid-IRI
log, and the focused test logs.
