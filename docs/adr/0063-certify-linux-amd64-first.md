---
status: accepted
---

# Certify linux/amd64 first

Executable Packs use OCI Image Index and platform descriptors from the beginning, but V0.1 trusted execution certifies only linux/amd64. Linux/arm64 and later platforms require their own executable and RuntimeDependencyPack manifests, Sandbox Provider evidence and cross-architecture semantic-equivalence tests before joining the trusted matrix.
