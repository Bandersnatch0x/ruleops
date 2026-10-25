# ADR-0181: RDFC artifact admission implementation plan

- Status: Proposed for implementation
- Date: 2026-10-25
- Scope: Ontology/RDFC production admission

## Decision

RDFC admission requires a deterministic, signed artifact pack before any
production consumer can rely on it. The pack must bind canonical bytes,
ontology revision, shapes/dependencies, toolchain identity and artifact digest.

## Admission gates

1. Freeze the pack wire format and cross-language canonical test vectors.
2. Verify the signed pack with a dedicated ontology-signing role.
3. Enforce dependency digests, resource limits, namespace and revocation.
4. Admit only the exact verified digest into the production registry.
5. Provide migration, rollback and rejection evidence for every version.

Until these gates are implemented and independently verified, the runtime must
keep `RDFC_NOT_ADMITTED`; no SHACL/RDFC dependency is silently introduced into
the current production path.
