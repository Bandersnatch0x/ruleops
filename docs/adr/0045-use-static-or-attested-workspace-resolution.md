---
status: accepted
---

# Use static or attested workspace resolution

Workspace resolution has exactly two explicit modes: SAFE_STATIC performs no project-script execution and exposes unresolved facts to Completeness, while ATTESTED_BUILD_EXPORT consumes a WorkspaceDescriptor, artifact digests, toolchain identity and provenance exported by a separate trusted build stage. Backend Hosts never execute project builds and missing facts cannot silently fall back to machine-local guesses.
