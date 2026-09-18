---
status: accepted
---

# Issue DSSE in-toto Gate attestations

Trusted Gate execution emits a DSSE in-toto GateAttestation whose subject and predicate bind repository and commit, RuleBundle, ExecutionLock, RunnerRelease, WorkspaceDescriptor and attestation, Baseline, exceptions, CanonicalFindingSet and SemanticResultDigest, Completeness and the three-state GateDecision. An authorized Runner workload identity signs it, and servers or branch protection accept it only after identity, digest and freshness verification; a CLI exit code remains process control rather than portable authority.
