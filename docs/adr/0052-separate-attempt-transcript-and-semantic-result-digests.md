---
status: accepted
---

# Separate attempt transcript and semantic result digests

Host and Runtime independently compute an AttemptTranscriptDigest over the exact ordered length-delimited observation frames to verify transport completeness. Runtime separately validates, canonicalizes and stably sorts committed Findings before computing SemanticResultDigest, ensuring parallel or engine-native output ordering cannot change the auditable semantic result.
