# Roundtable replay pack v1

This pack preserves a three-seat decision as replayable, digest-only evidence. It is a documentation/fixture seam only: it does not change protobufs, runtime trust, Gate authority, SCM writes, or production admission.

## Required fields

Each decision record contains:

- `decisionId`: stable identifier.
- `topic`: bounded issue/ADR topic.
- `options`: explicit options with `id`, `label`, and `status` (`RECOMMENDED`, `ALTERNATIVE`, or `BLOCKED`).
- `recommendation`: one option id, which must be `RECOMMENDED`.
- `blockingConditions`: conditions that prevent implementation or authority escalation.
- `evidence`: replayable commands and commit references.
- `boundary`: must remain `LOCAL_UNATTESTED`, `SHADOW`, or `RDFC_NOT_ADMITTED` unless an approved ADR changes it.

The checker rejects missing fields, unknown recommendation ids, contradictory option statuses, non-replayable evidence, and authority-escalating boundary values.

The checked-in fixture is marked `PROPOSED`; it records the three-seat discussion but does not freeze a production decision until reviewed.
