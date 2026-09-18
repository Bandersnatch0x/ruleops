---
status: accepted
---

# Bootstrap and measure Runner independently

A minimal RunnerBootstrap outside RunnerRelease verifies the TrustMetadata chain, RunnerRelease ArtifactStatement and actual binary or OCI manifest digest before execution, then passes an immutable RunnerMeasurement to Runtime. Bootstrap does not parse rules or compute Findings or Gate, GateAttestation verifiers cross-check its measurement, and Runner self-report or an unbootstrapped local binary can produce only LOCAL_UNATTESTED results.
