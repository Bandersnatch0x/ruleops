---
status: accepted
---

# Separate Gate computation from authority

Local, CI and controlled environments use the same deterministic Gate Engine, but only a decision backed by an accepted TrustedContext is authoritative for protected branches. Local execution produces an unattested preview, keeping evaluation semantics identical without treating developer-controlled identities as production evidence.
