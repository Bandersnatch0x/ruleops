---
status: accepted
---

# Refine the global ScanPlan inside hosts

Rust Planner freezes the global ScanPlan and authoritative Completeness denominator before execution. Each Backend Host returns a deterministic BackendExecutionPlan that may refine assigned units into engine batches, modules and invocation parameters but cannot add, remove or obscure planned units; unsupported or failed units are returned individually with structured reasons.
