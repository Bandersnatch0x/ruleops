---
status: accepted
---

# Reference and embed Ontology Contract Packs

OntologyContractPack retains an independent asset identity, signature, version and revocation lifecycle, and each RuleBundle references its exact digest. Standard and air-gapped bundles physically embed the referenced pack for atomic offline verification while Runtime still validates it independently and never substitutes a cached latest ontology.
