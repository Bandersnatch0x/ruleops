---
status: accepted
---

# Use bounded append-only attempt staging

Runtime validates each Observation frame and writes it to an append-only sandbox-local staging file while computing AttemptTranscriptDigest, rather than buffering unbounded results in memory. Deployment Policy limits frame size, observation count, total staging bytes and disk use; exceeding any limit cancels and terminates the Host, discards the attempt and marks execution incomplete, with no truncation followed by Gate evaluation.
