# Production authority rollout plan (2026-10-27)

Status: PROPOSED / action-time confirmation required for each enablement step.

## Scope approved by user

1. Merge the audited branch into the main branch.
2. Prepare and enable Trusted CI, registry, production Gate and status publication.
3. Prepare and enable Root/HSM custody plus ExceptionGrant issue/revoke operations.

Approval of scope is not itself evidence that any production authority is enabled.

## Phase gates

### Phase 1: branch promotion

- Inputs: reviewable branch diff, clean focused checks, open PR, reviewer approval.
- Required evidence: commit graph, PR status, branch protection result, rollback commit.
- Rollback: revert the merge commit and restore the last known-good main ref.
- Boundary before action: `LOCAL_UNATTESTED` / `SHADOW`.

### Phase 2: Trusted CI and registry

- Inputs: workload identity, attestation verifier, registry admission policy, status publication contract.
- Required evidence: identity binding, attestation replay/tamper negatives, registry rollback, protected-branch dry run, audit retention.
- Rollback: disable admission and status publication, keep artifacts in shadow mode, restore prior registry pointer.
- Blocker: no production enablement without action-time confirmation after evidence review.

### Phase 3: Root/HSM and ExceptionGrant

- Inputs: independent root identities, two-of-three threshold, HSM/offline custody, dual approval, recovery, revocation and expiry.
- Required evidence: key ceremony record, threshold negatives, namespace/expiry/revocation tests, recovery drill, grant fingerprint binding and replay/tamper negatives.
- Rollback: revoke the new generation, restore the prior trusted generation, disable grant consumption and return to shadow.
- Blocker: no signing, issuing, revoking or grant-consumption action without action-time confirmation.

## Invariants

- Fail closed on missing identity, missing attestation, stale/revoked artifact, incomplete execution, digest mismatch or ambiguous status.
- Never infer production trust from fake-host, static grep, schema-only or local-only evidence.
- Keep `RDFC_NOT_ADMITTED` until a separate artifact/signing/migration decision is approved.
- Every enablement action records target, permission delta, evidence bundle, operator, timestamp and rollback point.
