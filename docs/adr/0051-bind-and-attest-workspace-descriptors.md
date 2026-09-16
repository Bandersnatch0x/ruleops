---
status: accepted
---

# Bind and attest Workspace Descriptors

A WorkspaceDescriptor envelope binds repository and commit or tree identity, every consumed configuration and lockfile blob, resolution mode, Resolver Pack and toolchain identities, module graph, build-output digests and its own content digest. SAFE_STATIC records trusted Runner provenance without claiming a build attestation, while ATTESTED_BUILD_EXPORT additionally requires an independently signed WorkspaceAttestation from a trusted CI workload identity.
