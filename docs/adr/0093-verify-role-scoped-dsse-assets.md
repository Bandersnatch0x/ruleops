---
status: accepted
---

# Verify role-scoped DSSE assets

Trusted CI assets use a DSSE-style Pre-Auth Encoding over the fixed ERF asset
payload type. `erf-trust` verifies the Ed25519 signature and returns the
payload subject digest only when the key is known and explicitly authorized for
the requested role. Payload type, key id, role, signature length and signature
bytes are all checked; a valid signature under one role cannot be replayed under
another role. Verification can additionally consume versioned `TrustMetadata`:
expired metadata and revoked signer key ids reject the asset before role
authorization can be used.

The verifier has no registry, signing, network or authority-escalation path.
Local callers may use it to validate an immutable asset, but the result remains
`LOCAL_UNATTESTED` until the surrounding TrustedContext verifies trust metadata,
execution identity and policy.
