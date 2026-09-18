---
status: accepted
---

# Use ephemeral scoped Gate signing

Connected CI signs GateAttestation through a workload-identity broker or signing service with authority scoped to the current run and attestation digest, while air-gapped Runtime calls a restricted local signer socket or HSM bound to run identity and predicate type. Runner never receives an exportable long-lived private key or general-purpose signing authority.
