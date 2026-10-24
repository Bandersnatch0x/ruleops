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

/// The fake host remains `fake-backend` by default for protocol tests.  The
/// delivery vertical can opt it into one of the two signed Java identities so
/// Runner's fail-closed backend selection is exercised without weakening the
/// production identity rule.
pub fn capabilities_for_runtime_backend() -> pb::CapabilityDescriptor {
    let mut capabilities = canned_capabilities();
    if std::env::var_os("ERF_REFERENCE_DELIVERY_BACKEND").is_none() {
        return capabilities;
    }
    match std::env::var("ERF_BACKEND").as_deref() {
        Ok("pmd") => {
            capabilities.backend_id = "pmd-java".into();
            capabilities.backend_version = "7.9.0".into();
        }
        Ok("spotbugs") => {
            capabilities.backend_id = "spotbugs-java".into();
            capabilities.backend_version = "4.9.8".into();
            capabilities.facts.insert("java.bytecode".into(), true);
        }
        _ => {}
    }
    if let Ok(name) = std::env::var("FAKE_FALSE_CAPABILITY") {
        capabilities.facts.insert(name, false);
    }
    capabilities
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
