---
status: accepted
---

# Delete attempt staging by default

Attempt staging is Runner-only, sandbox-local and deleted after both successful and failed attempts without being uploaded; routine ExecutionEvidence retains only digests, structured errors and resource metrics. Explicit diagnostic policy may instead create a minimum-content DiagnosticBundle encrypted to an organization key with a short TTL and access audit, but retaining raw staging is never a default debug mode.
