---
status: accepted
---

# Fail closed on trust checkpoint rollback

Offline Runtime keeps the highest accepted TrustCheckpoint version in
protected local state. A checkpoint is accepted only when its version is
non-zero, its snapshot digest is non-zero, and its validity interval contains
the current trusted time. A lower version is a rollback and is rejected even
when its signatures would otherwise verify; expiry and invalid intervals are
also rejected.

The state transition is explicit and monotonic. It does not fetch metadata,
write credentials, or grant execution authority by itself. The surrounding
TrustedContext must still verify the signed checkpoint chain and Deployment
Policy before a result can become authoritative.
