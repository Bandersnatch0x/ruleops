---
status: accepted
---

# Atomically commit successful Host attempts

Runtime stages every ScanAttempt's observations and completeness facts and atomically commits them only after validating a successful Completion carrying matching request and attempt identities, observation count and digest. Failed or partial attempts are discarded; only transient startup or transport failures may retry once in a fresh Host process, while deterministic input, unsupported, resource and engine errors do not retry.
