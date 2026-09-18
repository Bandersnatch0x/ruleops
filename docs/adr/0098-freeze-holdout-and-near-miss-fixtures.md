---
status: accepted
---

# Freeze independent holdout and near-miss fixture classes

Phase 2 RuleIntent examples may carry two optional, data-only fixture classes
in addition to the original `bad` and `good` examples. `holdout` is an
independent positive case and must produce a Finding; `nearMiss` is an
independent boundary case and must remain clean. Candidate compilation emits
these classes under content-addressed `fixtures/holdout/` and
`fixtures/near-miss/` paths, so changing a sample changes the Candidate
digest.

Verification records the class, expectation, hit count and result in the
EvidenceBundle. Unknown fixture classes fail closed. The samples are frozen
review inputs rather than model-generated tuning data; they remain
`LOCAL_UNATTESTED` evidence and cannot create a RuleRelease or production Gate
authority.
