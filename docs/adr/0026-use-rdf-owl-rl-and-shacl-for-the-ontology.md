---
status: accepted
---

# Use RDF, OWL 2 RL and SHACL Core for the ontology

ERF uses RDF 1.1 for its authoritative graph and IRI identity model, a restricted OWL 2 RL subset for Control Plane inference, and SHACL Core for closed publication-time validation; JSON-LD 1.1 is an exchange syntax and SKOS is optional for terminology. V1 forbids recursive shapes, SHACL executable extensions, runtime ontology reasoning and unfixed remote contexts or imports so ontology processing remains bounded and reproducible.
