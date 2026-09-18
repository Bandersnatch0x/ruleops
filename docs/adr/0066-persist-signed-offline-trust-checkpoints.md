---
status: accepted
---

# Persist signed offline trust checkpoints

Each offline mirror publication includes a signed TrustCheckpoint carrying repository identity, highest role versions, snapshot digest, issue time and expiry. Runtime persists highest-seen versions in protected local state and returns INCONCLUSIVE on rollback, excessive trusted-clock skew, unexpected state loss or checkpoint expiry; recovery requires a newer checkpoint or an audited trust-state recovery rather than disabling freshness checks.
