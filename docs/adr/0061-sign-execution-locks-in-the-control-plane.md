---
status: accepted
---

# Sign Execution Locks in the Control Plane

The Control Plane deterministically resolves and signs ExecutionLock together with Policy and RuleBundle publication, and identical publication inputs must yield the same lock digest. Trusted Runtime only verifies, caches and executes that lock; a Developer Profile may resolve a temporary local lock for LOCAL_UNATTESTED feedback, while every Pack, protocol, schema or Sandbox Profile identity change creates a new authoritative lock.
