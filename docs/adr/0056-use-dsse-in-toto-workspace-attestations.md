---
status: accepted
---

# Use DSSE in-toto Workspace attestations

WorkspaceAttestation is an in-toto Statement in a DSSE envelope with a versioned ERF Workspace predicate and the WorkspaceDescriptor digest as subject. Materials and results bind source, configuration, Resolver, toolchain and build outputs, and verification requires a Deployment Policy-authorized CI workload identity plus repository, ref, environment and freshness constraints rather than accepting private or self-signed JSON claims.
