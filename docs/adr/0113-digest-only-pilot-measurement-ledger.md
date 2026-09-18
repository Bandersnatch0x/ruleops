# ADR-0113: Digest-only pilot measurement ledger

- Status: Accepted
- Date: 2026-09-17
- Scope: Phase 4 pilot measurement seam

## Context

The Phase 4 baseline names the measurements needed for a real pilot but the
execution evidence is intentionally synthetic and Shadow-only. A report needs
a stable way to collect those observations without turning metrics into a
source or Finding upload path, and without presenting test fixtures as live
product outcomes.

## Decision

Introduce `erf-pilot-metrics` as a transport-neutral ledger. It accepts only
validated `UnifiedResult` summaries for scan observations and stores repository
and commit identities as digests. Each run records new and historical counts,
completeness, tri-state decision, trust, latency, peak memory, and an
observation timestamp. Run records remain in operation order so callers can
calculate a deterministic trend. The ledger also exports and restores a
versioned, deterministic, digest-only observation journal, allowing a local
measurement run to be replayed after process restart without uploading source
or Findings.

Separate digest-only records capture confirmed false-positive regression
outcomes and governance snapshots for rollback count, active exception count,
and oldest active exception age. All record types are content-addressed,
idempotent on exact replay, reject conflicting reuse of an identifier, and
append an operation audit record.

The ledger validates required measurements and rejects zero latency, zero peak
memory, malformed regression outcomes, invalid governance age, and invalid or
incomplete UnifiedResults. It does not claim that a focused test is a live
pilot measurement; the measurement window and success thresholds remain
deployment/governance inputs.

## Consequences

- Report and dashboard adapters receive one deterministic aggregate source and
  a replayable local journal for the explicitly named Phase 4 metrics.
- Source, paths, full Findings, SCM data, network access, credentials,
  persistence, and repository writes remain outside the crate.
- Current evidence proves the seam and its failure behavior, not the product
  outcomes that require real pilot observations.

## Verification

Run `scripts/phase4-pilot-metrics.sh .scratch/phase4-pilot-metrics-<run>` and
inspect the eight focused tests and generated report. The full repository gate
remains required.
