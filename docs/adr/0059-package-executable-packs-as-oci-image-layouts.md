---
status: accepted
---

# Package executable packs as OCI Image Layouts

BackendRuntimePack, WorkspaceResolverPack and RuntimeDependencyPack use OCI Image Layout as their physical format, with an ERF media-typed config blob and executable layers. The OCI manifest digest is the Pack content identity and signature subject, offline archives preserve the layout and digests, and tags are never trusted as identity; RuleBundle remains an ERF rfbundle rather than becoming an OCI artifact.
