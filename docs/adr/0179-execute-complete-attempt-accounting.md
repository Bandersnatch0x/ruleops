# ADR-0179: Bind ExecuteComplete to attempt accounting

- Status: Proposed for implementation
- Date: 2026-10-25
- Scope: Backend Host protocol and Runtime execution accounting

## Decision

Extend `ExecuteComplete` additively with:

- field `2 observation_count` (`uint64`): the number of `BackendObservation`
  events emitted for this execute request;
- field `3 attempt_digest` (`bytes`): a 32-byte SHA-256 digest of the canonical
  attempt transcript.

The existing `units_reported` field remains mandatory. Runtime recomputes the
same digest from the received event sequence and rejects missing, duplicated,
reordered, or mismatched accounting before observations can reach Gate.

The canonical digest is:

```text
SHA-256("erf-execute-attempt-v1\\0" ||
        LP(event_1) || ... || LP(event_n) ||
        LP(uint64_be(units_reported)) ||
        LP(uint64_be(observation_count)))
```

`LP(x)` is an eight-byte big-endian length followed by the exact protobuf
payload bytes for one `ExecuteEvent` in emission order. The Runtime also keeps
its existing framed transcript digest for transport evidence; `attempt_digest`
is the Host's semantic accounting statement and must agree with the Runtime's
recomputation.

## Compatibility and failure behavior

- The fields are additive within the current protobuf major version.
- A trusted execution requires all three completion facts and exactly 32 digest
  bytes; missing or malformed fields produce `INCONCLUSIVE` and never a Gate
  PASS.
- Legacy compatibility mode may parse an absent digest only when explicitly
  selected outside Trusted CI; it must remain `LOCAL_UNATTESTED` and cannot
  publish a trusted status.

## Conformance plan

Rust, Java and Node fake Hosts must emit the same digest for the same event
sequence. Conformance vectors must cover correct accounting, duplicate event,
reordered event, missing status, wrong observation count, wrong unit count and
wrong digest. The Runner must verify the digest before staging observations.

## Boundary

This ADR freezes the protocol and verification semantics only. It does not
create Root threshold authority, HSM custody, RDFC admission, SCM write
permission or a production attestation. Those remain separate decisions.

## Verification evidence

Required commands after implementation:

```text
bash scripts/threat-model-check.sh .scratch/threat-model
bash scripts/phase2-independent-acceptance.sh .scratch/phase2-independent-acceptance
bash scripts/check.sh
```

Cargo/Maven behavior evidence must be recorded separately from static schema
evidence; a static pass is not a trusted runtime result.
