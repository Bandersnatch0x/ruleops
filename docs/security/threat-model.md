# ERF V0 Threat Model

- Status: fixed-denominator local threat model
- Date: 2026-09-18
- Authority: `LOCAL_UNATTESTED` / `SHADOW`
- Machine asset: [`specs/security/threat-model.yaml`](/home/kali/文档/ruleops/specs/security/threat-model.yaml)

The model covers sixteen in-scope threats across authoring, workspace
resolution, execution integrity, governance, filesystem isolation, resource
exhaustion, data isolation, cache integrity and release lifecycle. Each row
names the protected asset, ERF control, verification entrypoint, status and
residual risk. Secret exfiltration and cache poisoning are explicit rows rather
than assumptions inferred from network, privacy or trust controls.

The most important boundary is explicit: the model does not turn local tests
into a production attestation. Archive ingestion is boundary-only because the
current V0 path does not accept untrusted archives. Source files, credentials,
external SCM writes and full Finding uploads remain outside the local slice.

Run the machine check with:

```text
scripts/threat-model-check.sh .scratch/threat-model
bash scripts/check.sh
```
