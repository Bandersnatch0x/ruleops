# ADR-0120: Maintain a fixed-denominator threat model

- Status: Accepted
- Date: 2026-09-18
- Scope: V0 security and isolation evidence

## Context

Security controls had accumulated across protocol, sandbox, trust, governance
and authoring ADRs, but Phase 0 requires a threat model that makes coverage and
residual risk reviewable. A prose-only list could silently omit a threat or
present a local test as production certification.

## Decision

Maintain twenty-one threat identities in
`specs/security/threat-model.yaml`: sixteen baseline identities plus five
explicitly marked `BOUNDARY_ONLY` identities for trust-boundary gaps that
remain outside the current runtime claim. Every row must contain a protected asset,
ERF-owned control, verification reference, status and residual risk. The
machine checker rejects missing or duplicate identities, incomplete rows, and
an unstated `linux/amd64` / `LOCAL_UNATTESTED_SHADOW` boundary.

The current model distinguishes `CONTROLLED` from `BOUNDARY_ONLY`. In
particular, zip-bomb handling remains boundary-only because no untrusted
archive ingestion path exists. Local tests and fake providers provide seam
evidence only; they do not authorize protected-branch enforcement. Secret
exfiltration and cache poisoning are explicit identities so reviewers do not
have to infer their coverage from the network, privacy, digest and trust
controls.

The model does not introduce external dependencies or move source, credentials
or full Findings to the server. Future adapters must extend the model and its
evidence before entering trusted execution.

## Verification

```text
scripts/threat-model-check.sh .scratch/threat-model
bash scripts/check.sh
```
