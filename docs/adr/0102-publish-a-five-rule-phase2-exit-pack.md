---
status: accepted
---

# Publish a five-rule Phase 2 exit pack

The Phase 2 Java local exit pack is emitted only after exactly five governed
RuleIntents compile twice to equal Candidate digests, each Candidate is copied
into the pack, and each independent EvidenceBundle passes bad, good, holdout
and near-miss expectations. Each category must be present at least once; a
rule may have multiple examples in a category. The pack binds each intent file
stem to the Candidate manifest identity and also includes independent contract
conformance, Java compatibility and representative shadow evidence.

The frozen five are the Intent paths for SEC-JAVA-017 through SEC-JAVA-021:

- `corp.java.no-system-exit`
- `corp.java.avoid-duplicate-literals`
- `corp.java.no-forbidden-method`
- `corp.java.avoid-object-allocation-in-loop`
- `corp.java.utility-class`

Later candidate additions are qualified by the Phase 4 pilot and do not alter
this Phase 2 pack.

The pack may label the local maturity conclusion `VERIFIED` because the
evidence requirements are complete, but its trust remains
`LOCAL_UNATTESTED`/`SHADOW`. It does not fabricate RuleRelease signing, SCM
publication, protected-branch blocking or human approval.
