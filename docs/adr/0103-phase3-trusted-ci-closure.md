---
status: accepted
---

# Establish a local Trusted CI closure harness

Phase 3 needs one independently rerunnable seam that composes the already
separate trust boundaries: measured RunnerBootstrap, measured backend and
resolver OCI packs, signed ConformanceEvidence, signed ExecutionLock and
signed GateAttestation (including the canonical policy digest). `scripts/phase3-trusted-ci.sh` runs that seam against
the real fake-host conformance suite, verifies the cross-asset identity with
`TrustedContextVerifier`, and then exercises the transport-neutral Status
Publisher for all three Gate decisions.

The harness treats a local `TrustedContext` verification as an acceptance
boundary, not as production authority. It proves tampering, cross-commit,
expired/revoked trust metadata, incomplete PASS and status ordering failures
are rejected. It deliberately does not pull a registry, launch an OCI runtime,
call an SCM API, provision a write credential or mutate a protected branch.
