---
status: accepted
---

# Separate ontology and runtime-pack signing roles

OntologyContractPack and Backend or Runtime Dependency Pack publication use separate ontology-signing and runtime-pack-signing identities, isolated from RuleRelease, Policy or Bundle and Baseline signing roles. A pilot may share KMS or HSM infrastructure but uses distinct key identities, authorization policies and audit streams, and builders or compilers can submit but cannot sign release assets.
