---
status: accepted
---

# Use a Jena-centered ontology toolchain

The Control Plane uses Apache Jena as its primary RDF dataset, JSON-LD, SHACL Core and graph-processing toolchain, with an explicitly allowlisted and bounded OWL 2 RL materialization ruleset. RDFC-1.0 canonicalization sits behind a library-neutral OntologyCanonicalizer interface and its implementation must pass the standard test vectors, preventing semantic-web library types from leaking into the domain model.
