---
status: accepted
---

# Freeze the ScanPlan before execution

Completeness is measured against a deterministic, hashed ScanPlan frozen before backend execution, containing every required `RuleRelease × analysis unit`. Backend output cannot redefine the denominator; every planned unit reaches exactly one terminal state and every exclusion or degradation records a structured reason.
