---
status: accepted
---

# Use offline-root delegated trust metadata

ERF-owned, TUF-style TrustMetadata uses an offline Root to delegate scoped signing roles through versioned and expiring metadata. Rotation permits bounded old and new key overlap, signed snapshots carry both key and asset-digest revocations with effective times, and offline Runtime fails closed when its complete metadata chain is missing, rolled back, expired or outside Deployment Policy TTL.
