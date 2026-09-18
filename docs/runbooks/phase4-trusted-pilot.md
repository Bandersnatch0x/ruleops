# Phase 4 Trusted Pilot Runbook

This runbook applies to the current Shadow-only pilot slice. It is an
operator checklist, not an authorization to enable protected-branch blocking.

## Before a run

1. Use a fixed RuleBundle, PolicyProfile, ExceptionGrant set, ExecutionLock,
   RunnerRelease, WorkspaceDescriptor, and trust metadata. Do not resolve
   `latest` during execution.
2. Verify the role-scoped signatures and freshness metadata before starting.
   Policy, baseline, exception, release, pack, lock, and Gate roles remain
   separate.
3. Create a new evidence directory under `.scratch/` and retain the command
   output and digest-only report. Do not put generated artifacts at repository
   root.

## Interpret the result

| GateDecision | Status projection | Operator action |
| --- | --- | --- |
| PASS | Success | Accept only when Completeness is complete and TrustedContext is verified. |
| FAIL | Failure | Remediate the new unexempted blocking finding; do not rewrite the finding to obtain PASS. |
| INCONCLUSIVE | ActionRequired | Treat execution, trust, timeout, or comparison uncertainty as unresolved; rerun or escalate. |

`INCONCLUSIVE` is not a soft PASS and ExceptionGrant cannot exempt missing
execution, untrusted identity, or other Completeness failure. Local feedback
must remain visibly `LOCAL_UNATTESTED`.

## Failure handling

- Host crash, timeout, missing unit status, malformed observation, sandbox
  failure, or trust metadata failure: retain the attempt transcript digest,
  publish no PASS, and investigate the fixed input identities.
- Bundle, Pack, Lock, PolicyProfile, ExceptionGrant, or GateAttestation
  tampering: stop the run, reject the asset, preserve the digest and audit
  context, and obtain a newly signed asset through the governed path. Never
  substitute an unverified cached or `latest` asset.
- Status publication rejection: keep the verified GateAttestation and
  UnifiedResult; fix the publisher context or sequence. The publisher cannot
  change the GateDecision.

## Rollback and revoke

1. Confirm the target digest and current active identity from the registry.
2. Use a new, strictly increasing operation sequence with a non-empty actor
   and reason tied to a ticket or incident.
3. Select only a still-Verified target of the same identity. The current
   active entry is retired as Revoked and both state changes must appear in
   the append-only audit.
4. Re-run verification and inspect the resulting digest identities before
   allowing any future pilot consumer to use the target.

Revocation is fail-closed. A revoked release, profile, or grant is never
resurrected by cache lookup; a replacement must be separately admitted and
verified.

## Exception handling

An ExceptionGrant must be signed under `exception-signing`, have exact
repository/rule/scenario/path scope, owner, reason, approval, ticket, and a
finite effective/expiry interval. It affects only the finding's exception
decision. It does not delete the Finding, change its fingerprint, alter
BaselineState, or waive completeness.

## Evidence and escalation

Run the focused suites with:

```text
scripts/phase4-pilot-report.sh .scratch/phase4-pilot-report-<run>
bash scripts/check.sh
```

Keep the generated reports and command logs with the run record. Escalate when
the same identity mismatch, stale operation, audit-chain failure, or
`INCONCLUSIVE` cause repeats; do not manually edit evidence to force a green
status. The Phase 4 baseline report records the current repository and rule
coverage boundary.
