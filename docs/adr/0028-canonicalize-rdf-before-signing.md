---
status: accepted
---

# Canonicalize RDF before signing

ERF fixes every context and import by digest, canonicalizes the asserted RDF dataset with RDFC-1.0 to canonical N-Quads, and signs the resulting digest rather than source Turtle or JSON-LD text. The envelope records dataset boundaries, graph names, ontology/version IRIs, shapes and dependency digests, processing tool versions and compiled artifact digest; canonicalization failure or resource-limit exhaustion fails closed.
