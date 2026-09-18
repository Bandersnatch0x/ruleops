---
status: accepted
---

# Use typed observation locations

BackendObservation uses a closed location union for source spans, symbols, bytecode members and files. Source spans bind a repository-relative path and repository blob digest to a UTF-8 byte range, engine-native coordinates are diagnostic only, and Rust Runtime validates the content identity and derives display coordinates before producing a Finding.
