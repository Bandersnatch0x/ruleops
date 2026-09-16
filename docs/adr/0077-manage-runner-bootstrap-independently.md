---
status: accepted
---

# Manage Runner Bootstrap independently

RunnerBootstrap is independently signed and distributed through enterprise CI base images, OS package management or endpoint management, embeds the initial Root trust anchor and accepts only sequential threshold-valid Root rotation. rf and rf-runner cannot update the Bootstrap binary, anchors or highest-seen trust state, and bootstrap failure cannot fall back to an unmeasured trusted execution.
