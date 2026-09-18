---
status: accepted
---

# Keep RuleRelease bytes immutable across registry lifecycle changes

The Registry admits only a `SignedRuleRelease` that passes the complete
`RuleReleaseVerifier`, indexes it by the measured manifest digest, and stores
the signed release independently from lifecycle state. Promotion, revocation
and rollback selection change only state and append audit records; they never
rewrite the admitted manifest, signatures or artifact identity.

The Phase 4 core is deliberately transport-neutral and in-memory. A future
registry adapter must preserve duplicate-idempotency, monotonic operation
sequences, one active release per RuleIntent, explicit rollback retirement and
the append-only audit contract before it can add persistence or SCM/API
integration.
