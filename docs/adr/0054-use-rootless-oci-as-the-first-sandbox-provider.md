---
status: accepted
---

# Use rootless OCI as the first Sandbox Provider

V0.1's first certified Linux Sandbox Provider uses rootless OCI with a read-only root filesystem and inputs, isolated writable temporary storage, disabled networking, dropped capabilities, no-new-privileges, seccomp and cgroup limits. The provider emits enforcement identity and evidence, cannot fall back to an ordinary process, and remains behind a replaceable interface whose future native Linux adapters must pass the same conformance suite.
