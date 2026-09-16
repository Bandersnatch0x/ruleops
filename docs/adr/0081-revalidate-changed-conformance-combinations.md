---
status: accepted
---

# Revalidate changed conformance combinations

ConformanceEvidence binds exact Pack, suite, RunnerRelease, Sandbox Profile and Provider, and platform digests, so any identity change requires a new aggregate evidence release. Verification Worker may reference immutable test results that remain applicable but reruns affected tests and signs the new aggregate; Runtime cannot infer compatibility from old evidence.
