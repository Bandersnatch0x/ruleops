---
status: accepted
---

# Keep ontology authority in Git

Versioned RDF, Shapes, Semantic Projections and manifests in Git remain the authoritative ontology write source and change only through review. Jena Dataset or TDB is a digest-bound, fully rebuildable query read model that cannot accept authoritative writes, avoiding a competing graph-database source of truth.
