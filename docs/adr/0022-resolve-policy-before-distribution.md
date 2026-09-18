---
status: accepted
---

# Resolve Policy before distribution

Policy composition is deterministic: rulesets, versions, parameters and exceptions are resolved at publication time, conflicts require explicit decisions, and the result is frozen as a signed Resolved Policy Manifest. Runners never apply implicit last-wins merging or dynamically fetch the latest ruleset while scanning.
