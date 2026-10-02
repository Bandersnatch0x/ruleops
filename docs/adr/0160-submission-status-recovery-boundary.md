# ADR-0160: UNKNOWN submission status recovery boundary

- Status: Accepted
- Date: 2026-09-20
- Scope: V0.4 Governance / Thin Client to RuleOps Server
- Decision record: GitHub issue #109

## Context

Feedback and exception submission is a digest-only write. A client can lose the
connection after the Server has durably accepted the request, so retrying the
write blindly can create ambiguity or hide a request-identity conflict. The
existing submission receipt deliberately grants no approval, Gate, or SCM
authority; recovery must preserve that boundary.

## Decision

Add the read-only endpoint:

```text
GET /v1/submissions/status?requestId=sha256:<64-lowercase-hex>
GET /v1/submissions/status?artifactDigest=sha256:<64-lowercase-hex>
```

Both selectors may be supplied. If they resolve to different submissions, the
Server returns `409 SUBMISSION_IDENTITY_CONFLICT`. The endpoint never replays a
write and never accepts actor/reason mutation headers.

The closed response is `submission-status-v1` and binds, when a durable record
exists, the request ID, artifact digest, resource type/id/version/state/digest,
and the original create audit context (actor, reason, operation ID, event
digest, and recorded time). Source, full Finding, and arbitrary payload fields
are not added to the response.

The status values have distinct meanings:

- `PENDING`: this Server has an in-flight admission for the identity, but no
  durable resource is visible yet. It is transient and has `UNKNOWN` authority.
- `RECEIVED_ONLY`: a feedback/exception resource and its create audit event are
  durably visible. This is not approval or a Gate result.
- `REPLAYED`: the current Server process has observed an idempotent duplicate
  write for an already recorded request. It retains `RECEIVED_ONLY` authority.
- `UNKNOWN`: no matching durable record is known. It is not evidence that a
  write may be safely retried; automatic retry is always false.

The replay observation is intentionally not treated as a durable governance
decision. After a Server restart, a previously recorded request resolves to
`RECEIVED_ONLY` unless a new duplicate write is observed. This is conservative:
loss of the replay observation cannot elevate authority or cause a write.

The Thin Client exposes the query as `rf submission status`. Known statuses
exit successfully; `PENDING` and `UNKNOWN` emit their machine-readable result
and exit with the operationally inconclusive code. Identity conflicts are
known client errors and exit as rejected input. No result path performs an
automatic write retry.

## Consequences

- A client can recover a lost receipt by either request ID or artifact digest.
- A missing record, a transient in-flight request, and an observed replay are
  distinguishable without confusing any of them with approval.
- The local HTTP seam remains loopback/plain-HTTP for development; production
  TLS/authentication remains a deployment boundary.
- The transient `PENDING` marker is process-local; durable recovery after a
  restart intentionally falls back to `RECEIVED_ONLY` or `UNKNOWN`.

## Verification

```text
bash scripts/submission-status-check.sh .scratch/submission-status-check-109
bash scripts/check.sh
```

The focused evidence covers feedback and exception lookup, replay, unknown,
selector conflict, audit/resource identity, Rust transport parsing, and CLI
exit-code behavior.
