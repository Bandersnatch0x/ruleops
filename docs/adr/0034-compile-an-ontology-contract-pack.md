---
status: accepted
---

# Compile an Ontology Contract Pack

Each approved Ontology Revision is compiled into a signed, immutable OntologyContractPack whose authoritative runtime representation is deterministic Protobuf and whose optional JSON representation is inspection-only. The pack contains stable identifiers, closed constraints, capability relations, Semantic Projection bindings and compatibility metadata, allowing Rust Runtime to consume deterministic semantics without RDF, OWL, SHACL or a reasoner.
