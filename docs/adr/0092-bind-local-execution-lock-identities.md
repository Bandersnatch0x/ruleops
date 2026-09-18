---
status: accepted
---

# Bind local execution lock identities

Before a scan starts, Runtime may consume a reviewable `ExecutionLock` that
binds the exact RuleBundle content digest, Workspace Resolver identity/version,
Backend Host identity/version, protocol major and CanonicalFindingSet version.
The lock has its own deterministic content digest and uses a closed key/value
format in the local slice; unknown or duplicate keys are rejected.

The lock is an identity and compatibility check, not a trust assertion. Phase 2
and the current local Phase 3 foundation have no signing authority, so a valid
local lock remains `LOCAL_UNATTESTED`. Trusted CI may later add a separately
verified signature/trust chain without changing the binding fields or allowing
an unsigned lock to escalate authority. The `SignedExecutionLock` wrapper
enforces that the DSSE payload is exactly the lock's canonical payload and that
its key is authorized for the execution-lock role; authority escalation still
belongs to the surrounding TrustedContext.
