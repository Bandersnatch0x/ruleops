---
status: accepted
---

# Require a hermetic trusted runtime closure

Trusted execution requires the complete Backend Host, analysis engine and language-runtime dependency closure to be immutable, signed and locked by digest. Large JRE, Node or similar environments may be shared as separately signed RuntimeDependencyPacks, while use of a compatible machine-installed runtime is restricted to explicitly LOCAL_UNATTESTED developer feedback and can never substitute for trusted Gate evidence.
