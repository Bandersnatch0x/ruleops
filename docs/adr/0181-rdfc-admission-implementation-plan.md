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

## Safe implementation slice (2026-10-25)

The ontology module now freezes a small `rdfc-pack-v1` envelope and a
language-neutral validator.  It checks canonical-byte, shape, dependency and
artifact digest metadata, deterministic dependency ordering, resource bounds,
and the dedicated `ontology-signing` role.  A checked-in wire vector provides
byte-identical parse/serialize coverage.  This slice does **not** implement
RDFC-1.0 canonicalization or cryptographic signature verification; its
`RdfcAdmission` boundary therefore always returns `RDFC_NOT_ADMITTED`.
