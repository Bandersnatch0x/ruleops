//! Canned fixtures of the reference fake Backend Host, shared between the
//! host binary and its contract tests so they can never drift.

use erf_contracts::backend::v0 as pb;
use std::collections::HashMap;

pub fn canned_capabilities() -> pb::CapabilityDescriptor {
    pb::CapabilityDescriptor {
        languages: vec!["java".to_string()],
        facts: HashMap::from([
            ("java.ast".to_string(), true),
            ("java.symbol-resolution".to_string(), true),
        ]),
        rule_kinds: vec!["SymbolRule".to_string()],
        backend_id: "fake-backend".to_string(),
        backend_version: "0.1.0".to_string(),
    }
}

pub fn canned_observation(unit_id: &str) -> pb::BackendObservation {
    pb::BackendObservation {
        rule_id: "corp.java.no-system-exit".to_string(),
        location: Some(pb::ObservationLocation {
            kind: Some(pb::observation_location::Kind::SourceSpan(pb::SourceSpan {
                path: "src/main/java/A.java".to_string(),
                blob_sha256: vec![0u8; 32],
                start_byte: 0,
                end_byte: 17,
                line: 1,
                column: 5,
            })),
        }),
        message: format!("canned observation in {unit_id}"),
        engine_severity: "error".to_string(),
    }
}
