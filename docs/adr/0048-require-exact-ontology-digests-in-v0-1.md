---
status: accepted
---

# Require exact ontology digests in V0.1

V0.1 requires RuleRelease, Resolved Policy Manifest, RuleBundle and OntologyContractPack to match the exact ontology digest used for verification. V0.2 may accept signed OntologyCompatibilityEvidence only for ADDITIVE revisions; constraint-tightening, behavioral and breaking changes require re-verification and a new RuleRelease rather than a compatibility waiver.
