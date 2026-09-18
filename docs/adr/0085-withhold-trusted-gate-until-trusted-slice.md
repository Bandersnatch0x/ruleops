---
status: accepted
---

# Withhold trusted Gate until the trusted slice

Contract and Java Local slices may produce only LOCAL_UNATTESTED or SHADOW results and cannot block a protected branch. Blocking authority begins only after the Trusted CI slice verifies signed assets, ExecutionLock, Bootstrap, Sandbox, attestations, Baselines, failure behavior and independent publication.
