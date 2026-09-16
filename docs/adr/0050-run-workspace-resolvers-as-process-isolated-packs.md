---
status: accepted
---

# Run Workspace Resolvers as process-isolated packs

Rust Runtime owns workspace-resolution orchestration and WorkspaceDescriptor validation, while Maven, Gradle and future JavaScript ecosystem adapters run as signed, content-addressed WorkspaceResolverPacks in separate sandboxed processes. They reuse the framed Protobuf transport mechanics but have a distinct Workspace Resolution schema rather than overloading Backend Host messages, keeping project modeling reusable across Backends.
