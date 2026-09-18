---
status: accepted
---

# Sandbox Backend Hosts by default

Backend Hosts run in a platform-specific default-deny sandbox with no network, credentials or workspace writes; repository inputs and build outputs are read-only, only isolated temporary storage is writable, and CPU, memory, time, process, file and output limits are enforced. Child processes require a signed pack declaration plus Deployment Policy approval, and inability to establish the required sandbox yields INCONCLUSIVE in trusted execution rather than an unsandboxed fallback.
