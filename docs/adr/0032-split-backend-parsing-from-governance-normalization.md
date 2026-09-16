---
status: accepted
---

# Split backend parsing from governance normalization

Backend Hosts parse engine-specific reports, locations and errors into policy-neutral BackendObservations. Rust Runtime alone binds observations to RuleReleases and normalized paths, computes Finding fingerprints and Baseline state, and aggregates Completeness, keeping engine knowledge local to hosts and governance semantics consistent across all Backends.
