---
status: accepted
---

# Use scan-scoped Backend Host processes

V1 starts one Backend Host process for each ScanExecution and BackendRuntimePack digest, reuses it within that scan, and destroys it on completion, cancellation, deadline or protocol failure. Cross-scan process pooling is excluded until state cleanup, tenant isolation and result equivalence can be demonstrated, trading some startup cost for reproducibility and failure containment.
