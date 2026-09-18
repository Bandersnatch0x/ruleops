---
status: accepted
---

# Test the Sandbox Provider through a failure-injection seam

The Phase 2 sandbox contract suite tests the replaceable
`DockerSandboxProvider` at its command and preflight boundary. It asserts
default-deny flags, read-only input mounts, isolated writable temporary
storage, seccomp and resource controls, and a stable policy digest. A local
fake provider injects rootless success, non-rootless rejection and unavailable
provider failure; invalid policy inputs are injected directly.

Every provider failure is terminal for the attempted sandboxed execution and
does not fall back to an ordinary process. Fake-provider output is only a
test seam: it does not establish Docker daemon, kernel, seccomp or OCI
enforcement and remains `LOCAL_UNATTESTED`/`SHADOW` evidence.
