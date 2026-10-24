# ERF V0 Threat Model

- Status: fixed-denominator local threat model
- Date: 2026-09-18
- Authority: `LOCAL_UNATTESTED` / `SHADOW`
- Machine asset: [`specs/security/threat-model.yaml`](/home/kali/文档/ruleops/specs/security/threat-model.yaml)

The model covers twenty-one in-scope threats: sixteen baseline rows plus five
explicit `BOUNDARY_ONLY` rows for `attempt-staging-disclosure`,
`bootstrap-measurement-confusion`, `completion-accounting-integrity`,
`root-threshold-delegation` and `exception-grant-consumption`. The boundary-only
rows are STATIC_ONLY disclosures of gaps and must not be read as production
Trusted, SCM or security attestations. Each row names the protected asset, ERF
control, verification entrypoint, status and residual risk. Secret exfiltration
and cache poisoning are explicit rows rather than assumptions inferred from
network, privacy or trust controls.

The most important boundary is explicit: the model does not turn local tests
into a production attestation. Archive ingestion is boundary-only because the
current V0 path does not accept untrusted archives. Source files, credentials,
external SCM writes and full Finding uploads remain outside the local slice.

Run the machine check with:

```text
scripts/threat-model-check.sh .scratch/threat-model
bash scripts/check.sh
```
