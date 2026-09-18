---
status: accepted
---

# Keep Baseline and Exception decisions orthogonal

Gate computes BaselineState first and ExceptionDecision independently afterward. Applying ExceptionGrant affects only the current Gate outcome and never deletes the Finding or changes its fingerprint or BaselineState; reports and GateAttestation retain both decisions and the matching grant digest for auditability.
