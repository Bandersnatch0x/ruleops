---
status: accepted
---

# Combine TrustMetadata with portable ArtifactStatements

TrustMetadata protects repository delegation, freshness, rollback and revocation, while every RuleBundle, OntologyContractPack, ExecutionLock, RunnerRelease and executable OCI Pack also carries a DSSE ArtifactStatement binding content digest, asset type, signing role, provenance, SBOM and compatibility identity. Both layers must verify, preserving independent asset verification after offline copying without confusing portable authenticity with repository freshness.
