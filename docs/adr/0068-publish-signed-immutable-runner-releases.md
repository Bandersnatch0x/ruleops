---
status: accepted
---

# Publish signed immutable Runner Releases

The trusted Rust rf and rf-runner core is published as RunnerRelease with exact binary digest, SBOM, provenance and protocol, schema, CanonicalFindingSet, Sandbox Provider and platform compatibility. ExecutionLock pins an exact RunnerRelease digest, while an unsigned or locally compiled Runner remains useful for development but can produce only LOCAL_UNATTESTED results.
