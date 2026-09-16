---
status: accepted
---

# Map Gate states strictly to SCM status

Status Publisher maps PASS to success, FAIL to failure and INCONCLUSIVE to error or action-required, never neutral or success. Publication is idempotent by repository, commit, policy and GateAttestation subject, preserves monotonic run identity for an execution sequence, and rejects stale, expired or cross-commit attestations from overwriting newer status.
