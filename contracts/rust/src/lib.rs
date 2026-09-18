//! Generated Rust types for ERF versioned cross-process contracts.
//! Sources of truth live in `contracts/proto/`; field numbers are frozen per
//! ADR-0030 (additive evolution within a major version only).

/// Backend Host Contract v0 (`erf.backend.v0`).
pub mod backend {
    pub mod v0 {
        include!(concat!(env!("OUT_DIR"), "/erf.backend.v0.rs"));
    }
}

/// Workspace Resolution Contract v0 (`erf.workspace.v0`).
pub mod workspace {
    pub mod v0 {
        include!(concat!(env!("OUT_DIR"), "/erf.workspace.v0.rs"));
    }
}
