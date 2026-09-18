---
status: accepted
---

# Distribute signed Backend Runtime Packs

Each Backend Host, analysis engine and required runtime configuration is released as an immutable, content-addressed and signed BackendRuntimePack with compatibility metadata, SBOM and provenance. EngineLock references the pack digest; RuleBundle does not absorb its logical identity, although an air-gapped export may physically carry both assets.
