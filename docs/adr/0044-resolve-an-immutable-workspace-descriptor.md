---
status: accepted
---

# Resolve an immutable Workspace Descriptor

A deep Workspace Resolver module uses ecosystem adapters for Maven, Gradle and later JavaScript tooling to produce an immutable, content-addressed WorkspaceDescriptor before ScanPlan creation. Backend Hosts consume that descriptor rather than independently guessing project structure; resolution defaults to static parsing or trusted exported build metadata, arbitrary project build scripts do not run by default, and unresolved facts flow into Completeness.
