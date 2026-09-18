---
status: accepted
---

# Minimize Runner data upload by default

The default Runner privacy profile uploads only execution identities, digests, Completeness, Gate evidence and aggregate metrics. Detailed Findings require an explicit Deployment Policy, source snippets are denied by default, and path or symbol fields require allowlisting or redaction so distributed execution does not silently become centralized source-code collection.
