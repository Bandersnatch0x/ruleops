# ADR-0106: Signed PolicyProfile and ExceptionGrant lifecycle

- Status: Accepted
- Date: 2026-09-17
- Scope: Phase 4 governance registry core

## Context

RuleRelease lifecycle is now content-addressed and independently auditable,
but policy selection and exception governance still need an explicit control
plane boundary. A runtime must not fetch an implicit latest profile, accept an
unsigned exception, or treat a revoked grant as a valid gate input.

## Decision

Introduce `erf-governance-registry` as a transport-neutral registry for signed
`PolicyProfile` and `ExceptionGrant` assets.

### PolicyProfile

`PolicyProfile` has a stable `profile_id` plus an immutable `version`. Its
canonical payload contains the complete rule identity set, blocking rule set,
referenced ExceptionGrant digests, and the completeness requirement. Blocking
rules must be a subset of declared rules. The `policy-profile-signing` role is
separate from gate-result signing. A profile is admitted only after canonical
payload and signature verification; the registry stores it as `Verified` and
allows one `Active` version per `profile_id`.

### ExceptionGrant

`ExceptionGrant` is signed under the independent `exception-signing` role and
contains grant identity, repository/rule/scenario selectors, a restricted
repository-relative path glob, optional normalized symbol and exact finding
fingerprint, owner, reason, ticket, approvals, and an effective/expiry
interval. Empty, absolute, traversal, whole-repository, regex-like, or
malformed selectors are rejected. An expired or not-yet-effective grant cannot
be admitted or promoted. Multiple immutable revisions may share a grant ID;
the registry permits only one active revision for that ID.

### Lifecycle and audit

Both asset kinds use `Verified -> Active -> Revoked`. Promotion, revocation,
and rollback require a non-empty actor/reason and a strictly increasing
operation sequence. Rollback selects a still-verified revision and retires
the current active revision as revoked. Signed asset bytes are never rewritten.
Every successful state change appends an audit record with asset kind, digest,
actor, reason, operation sequence, and resulting state. Exact duplicate
admission is idempotent; a different signed envelope at the same content
digest is rejected.

The registry has no filesystem, SCM, network, database, or signing-key storage
authority. Adapters own persistence and transport while preserving these
verification and monotonicity checks.

## Consequences

- Policy and exception inputs have independently verifiable, immutable
  identities before they reach Gate or a future resolver.
- Rollback is explicit and auditable; it cannot silently overwrite a release,
  profile, or grant.
- Persistence, API authentication, and signer key custody remain separate
  implementation concerns.

## Verification

`runtime/governance-registry` covers signature/metadata/role rejection,
selector and validity failures, duplicate/conflicting admission, lifecycle
transitions, stale operations, rollback, and byte immutability. The focused
acceptance command is `scripts/phase4-governance-registry.sh`.
