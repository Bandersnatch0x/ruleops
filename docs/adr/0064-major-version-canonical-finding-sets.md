---
status: accepted
---

# Major-version Canonical Finding Sets

Any CanonicalFindingSet schema, ordering, encoding or field-semantics change increments its major version and domain separator rather than silently changing digest meaning. A migration window may compute old and new SemanticResultDigests together and publish Baseline and fingerprint mappings, including explicit treatment of additive fields.
