---
status: accepted
---

# Certify only the Linux OS family for trusted execution in V0.1

V0.1 certifies authoritative Gate execution only within the Linux OS family, with ADR-0063 further limiting the first certified architecture to linux/amd64. macOS and Windows support developer feedback as LOCAL_UNATTESTED until their Sandbox Providers pass filesystem, network, credential, resource, process-cleanup and cross-platform result-equivalence conformance suites, preventing platform support from implying unverified trust guarantees.
