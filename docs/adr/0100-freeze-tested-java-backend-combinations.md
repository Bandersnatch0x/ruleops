---
status: accepted
---

# Freeze tested Java Backend combinations

Phase 2 publishes a compatibility matrix for the exact Java Host dependency
versions and declared Java source-level set. The current tested combinations
are PMD Java 7.9.0 on the source/symbol path and SpotBugs 4.9.8 on the
bytecode path, with protocol v0.1 and required capability facts recorded in
the matrix. The five Phase 2 RuleIntents must declare source levels 11, 17 and
21; a drift in either the POM or an Intent fails the checker.

`VERIFIED` means the repository's current Java Host entrypoint and integration
tests exercise the combination. It does not infer support for another engine
version or platform; such combinations are recorded as `untested`. All matrix
evidence remains `LOCAL_UNATTESTED`/`SHADOW` and has no production authority.
