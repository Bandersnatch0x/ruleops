---
status: accepted
---

# Govern signed immutable Exception Grants

An ExceptionGrant is a signed, immutable and revocable risk-acceptance asset binding RuleIntent and Scenario, repository and path or symbol scope, reason, owner, ticket, approvals, effective time and expiry. Gate consumes only exact grant digests referenced by resolved policy; invalid, expired, revoked or out-of-scope grants do not apply, permanent wildcard grants are forbidden, and no grant can waive missing execution or trust failures.
