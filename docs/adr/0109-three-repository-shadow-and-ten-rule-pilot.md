# ADR-0109: Three-repository Shadow and ten-rule pilot qualification

- Status: Accepted
- Date: 2026-09-17
- Scope: Phase 4 pilot qualification evidence

## Context

Phase 4 requires evidence beyond a single representative rule and repository:
three independent Shadow repository paths and at least ten qualified rule
intents. The existing Phase 2 exit pack must remain reproducible and must not
be silently redefined.

## Decision

Keep the original five RuleIntents as the frozen Phase 2 local exit pack and
add five distinct structural Java RuleIntents under the same governed
Standard/RuleIntent cross-reference model. `phase4-shadow-pilot.sh` validates
all ten, compiles every candidate twice, verifies bad/good/holdout/near-miss
EvidenceBundles, and records each candidate digest.

The script creates three separate repository snapshots with distinct
repository and commit identities and runs the real Runner -> Trusted CI
handoff against each. The handoff remains a test of the trusted seam; the
pilot result is explicitly `SHADOW` and cannot enable protected-branch
enforcement.

Crash injection is run within every repository path. The existing tri-state
rules must therefore continue to produce `INCONCLUSIVE` and
`ActionRequired`, never a false PASS. No SCM credentials, repository writes,
or network integrations are introduced.

## Consequences

- The pilot has a reproducible ten-rule qualification count and three distinct
  repository identities without weakening the earlier Phase 2 contract.
- Structural rules are clearly bounded by the current template; expanding
  beyond that capability still requires a new RuleIntent and evidence.
- The Phase 4 exit target is measurable, while production authority remains a
  separate governance decision.

## Verification

Run `scripts/phase4-shadow-pilot.sh .scratch/phase4-shadow-pilot-<run>` and
inspect `phase4-shadow-pilot.md`, all ten EvidenceBundles, `digests.txt`, and
the three real-scan reports.
