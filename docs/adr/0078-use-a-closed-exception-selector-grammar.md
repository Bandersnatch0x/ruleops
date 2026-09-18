---
status: accepted
---

# Use a closed Exception selector grammar

ExceptionGrant requires repository, RuleIntent and Scenario selectors and may add a repository-root-anchored restricted path glob, normalized symbol identity and exact Finding fingerprint, with every supplied selector combined by AND. Arbitrary regular expressions, scripts, parent traversal, dynamic queries and implicit repository-wide wildcards are forbidden to keep matching deterministic and reviewable.
