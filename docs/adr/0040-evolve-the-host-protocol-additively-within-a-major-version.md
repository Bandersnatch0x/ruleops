---
status: accepted
---

# Evolve the Host protocol additively within a major version

Runtime and Host reject incompatible protocol major versions, while a shared major version permits only additive Protobuf evolution with permanent field numbers and explicit required-capability negotiation. Governance messages cannot use Any or opaque JSON to escape the contract, and Rust, Java and future JavaScript implementations must pass bidirectional cross-version golden compatibility tests.
