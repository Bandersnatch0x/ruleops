---
status: accepted
---

# Resolve one top-level Execution Lock

A signed deterministic ExecutionLock freezes every BackendRuntimePack, transitive RuntimeDependencyPack, WorkspaceResolverPack, protocol and schema version, and Sandbox Profile compatibility requirement for a trusted execution. EngineLock remains a backend-specific nested record, preventing engine terminology from incorrectly owning resolver, general runtime and sandbox identities.
