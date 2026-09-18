---
status: accepted
---

# ADR-0130: Select fixed public GitHub Java repositories for the Shadow pilot

The approved Pilot Success Contract requires a pre-registered sample of three
Java repositories, while the existing Phase 4 manifest intentionally proves
only three local fixture identities. Select three small public, non-fork,
non-archived Maven repositories owned by distinct GitHub organizations, pin
each to an exact commit, and analyze read-only local checkouts under
`authority: SHADOW`. Keep the local-fixture manifest unchanged: acquisition
from GitHub is evidence for the live sample, not a replacement for the
deterministic seam qualification.

The selected sample is `davidmoten/word-wrap`, `gitbucket/markedj`, and
`jacekkow/keycloak-protocol-cas`. The ERF scan uses no SCM credentials,
repository writes, or source upload; the sample does not authorize protected
branch enforcement.

## Consequences

- The live sample has a reproducible repository/commit denominator and can be
  re-acquired without resolving a mutable `latest` reference.
- The sample spans a tiny Java 8 utility, a small Java 8 parser library, and a
  small Java 17 integration plugin.
- Maven dependency acquisition is a preflight concern; the ERF analysis itself
  runs against the fixed local checkout and its output remains digest-only
  evidence under `.scratch/`.
