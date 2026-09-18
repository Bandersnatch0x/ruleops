---
status: accepted
---

# Bind Workspace attestations to CI workload context

Connected WorkspaceAttestation verification binds an authorized workload issuer and audience, repository, ref, workflow and run identity, commit or tree, ExecutionLock digest and validity interval so an old or cross-context statement cannot be replayed. Air-gapped CI uses an organization-managed key restricted to workspace attestation scope while retaining the same subject, material and freshness requirements.
