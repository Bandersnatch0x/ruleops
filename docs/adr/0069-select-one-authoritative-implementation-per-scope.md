---
status: accepted
---

# Select one authoritative implementation per scope

Resolved Policy selects exactly one authoritative RuleImplementation for each Scenario and applicability scope to produce governed Findings, Baseline state and Gate impact. Other verified implementations may run in SHADOW mode for comparison evidence but cannot enter the authoritative CanonicalFindingSet or block Gate; V1 excludes voting or merging multiple authoritative implementations.
