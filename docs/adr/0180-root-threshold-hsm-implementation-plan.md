# ADR-0180: Root threshold, delegation and HSM implementation plan

- Status: Proposed for implementation
- Date: 2026-10-25
- Scope: Production trust-root preparation

## Decision

Implement Root trust in four independently verifiable slices:

1. Root metadata with independent identity deduplication and a two-of-three
   threshold policy;
2. delegated role metadata with namespace, expiry, revocation and bounded
   overlap during rotation;
3. monotonic checkpoint/rollback protection for Root metadata;
4. offline/HSM custody, dual-person approval and recovery acceptance.

Routine delegated asset signatures remain one-of-N after the Root delegation
chain is verified; Root threshold and daily role authorization are separate
controls.

## Fail-closed rules

- One Root signature cannot satisfy a two-of-three Root policy.
- Duplicate public-key identities do not count as independent signatures.
- Missing, expired, revoked, namespace-mismatched or rolled-back metadata is
  not trusted.
- No threshold API is considered production capability until independent
  identities, custody and recovery evidence exist.

## Implementation order

Freeze the metadata/protobuf contract and test vectors first, then implement
Rust verification and registry integration, then add Java/CI consumers and HSM
operational evidence. Until all slices pass, authority remains
`LOCAL_UNATTESTED` / `SHADOW`.

## Safe implementation slice (2026-10-25)

`RootTrustMetadata::validate_chain` adds a pure, offline chain-edge check for
the immediately preceding generation. It requires the explicit
previous-generation digest to match, the namespace to remain stable, and both
version and `issued_at_epoch_secs` to move monotonically forward. The candidate
generation is checked against the supplied clock; an expired prior generation
is allowed as a historical link. The method does not verify signatures,
mutate `TrustStore`/`TrustState`, resolve delegated keys for a consumer, or
grant execution/publishing authority. Root metadata therefore remains
`PREPARATION_ONLY` / `LOCAL_UNATTESTED_SHADOW` until custody, recovery, and
production integration evidence exist.
