---
status: accepted
---

# Define Canonical Finding Set v1

Rust Runtime computes SemanticResultDigest as SHA-256 over a version-domain-separated sequence of deterministic Protobuf CanonicalFindingSet v1 records sorted by a stable canonical key. The representation includes only governed Finding semantics and required identities while excluding timestamps, duration, log ordering, ScanAttempt identity and runtime metrics so execution scheduling cannot change the result identity.
