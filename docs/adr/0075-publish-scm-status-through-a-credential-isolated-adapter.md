---
status: accepted
---

# Publish SCM status through a credential-isolated adapter

Runner emits GateAttestation without SCM write credentials, and a separate thin Status Publisher re-verifies signature, repository, commit, freshness and critical digests before using a least-privilege status or check credential. Backend Hosts, Workspace Resolvers and Runner cannot write SCM state, while Publisher can only reject or faithfully publish the verified GateDecision rather than recompute or modify it.
