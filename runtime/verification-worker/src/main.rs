//! Standalone worker fixture. It is deliberately a separate executable so CI
//! can run verification outside the Pack Builder process.

use erf_conformance::SuiteConfig;
use erf_verification_worker::{SandboxProof, VerificationRequest};
#[cfg(feature = "fixture-signer")]
use erf_verification_worker::{verify_and_sign, SandboxTrustAnchor};
#[cfg(feature = "fixture-signer")]
use ed25519_dalek::{Signer, SigningKey};
#[cfg(feature = "fixture-signer")]
use std::{fs, path::Path};

fn main() {
    if let Err(error) = run() {
        eprintln!("verification worker failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let host = required(&args, "--host")?;
    let resolver = required(&args, "--resolver")?;
    let output = required(&args, "--output")?;
    let cancel_window_ms = optional_u64(&args, "--cancel-window-ms").unwrap_or(150);
    let mut suite = SuiteConfig::default();
    suite.host_program = host;
    suite.resolver_program = resolver;
    suite.cancel_probe_delay_ms = cancel_window_ms;
    #[allow(unused_mut)]
    let mut request = VerificationRequest {
        suite,
        suite_release_digest: digest_arg(&args, "--suite-release-digest")?,
        verification_worker_release_digest: digest_arg(&args, "--verification-worker-release-digest")?,
        runner_release_digest: digest_arg(&args, "--runner-release-digest")?,
        backend_pack_digest: digest_arg(&args, "--backend-pack-digest")?,
        resolver_pack_digest: digest_arg(&args, "--resolver-pack-digest")?,
        sandbox_provider_id: required(&args, "--sandbox-provider-id")?,
        sandbox_policy_digest: digest_arg(&args, "--sandbox-policy-digest")?,
        resolver_sandbox_provider_id: required(&args, "--resolver-sandbox-provider-id")?,
        resolver_sandbox_policy_digest: digest_arg(&args, "--resolver-sandbox-policy-digest")?,
        platform: required(&args, "--platform")?,
        sandbox_proof: SandboxProof::unsigned(),
    };
    #[cfg(feature = "fixture-signer")]
    {
        if std::env::var("ERF_ALLOW_FIXTURE_SIGNER").as_deref() != Ok("1") {
            return Err("fixture signer is disabled; production deployments must inject an external workload-identity/HSM signer".into());
        }
        let key_hex = required(&args, "--fixture-signing-key-hex")?;
        let key_bytes = decode_hex(&key_hex)?;
        let key_array: [u8; 32] =
            key_bytes.try_into().map_err(|_| "signing key must be 32 bytes")?;
        let key = SigningKey::from_bytes(&key_array);
        let key_id = required(&args, "--key-id")?;
        let (sandbox_proof, sandbox_trust_anchor) = if has_flag(&args, "--fixture-sandbox-proof") {
            if std::env::var("ERF_ALLOW_FIXTURE_SANDBOX").as_deref() != Ok("1") {
                return Err("fixture sandbox proof is disabled; production deployments must inject an external provider proof".into());
            }
            let sandbox_key = SigningKey::from_bytes(&[2u8; 32]);
            (SandboxProof::fixture(&request, &sandbox_key), SandboxTrustAnchor::fixture())
        } else {
            let proof_path = required(&args, "--sandbox-proof")?;
            let proof = serde_json::from_str::<SandboxProof>(&fs::read_to_string(proof_path)?)?;
            (proof, SandboxTrustAnchor::from_deployment_env()?)
        };
        request.sandbox_proof = sandbox_proof;
        let result = verify_and_sign(&request, &sandbox_trust_anchor, key_id, |bytes| {
            key.sign(bytes).to_bytes().to_vec()
        })?;
        let asset = result.signed_evidence;
        fs::write(
            Path::new(&output),
            serde_json::to_vec_pretty(&asset.to_dsse_json())?,
        )?;
        println!(
            "verification=PASS evidence={output} suite_version={} suite_digest={} checks={}/{} key_id={}",
            result.report.suite_version,
            hex(&result.report.suite_digest()),
            result.report.checks.iter().filter(|check| check.passed).count(),
            result.report.checks.len(),
            asset.key_id,
        );
        return Ok(());
    }
    #[cfg(not(feature = "fixture-signer"))]
    {
        let _ = (request, output);
        Err("fixture signer support is not compiled; production deployments must inject an external workload-identity/HSM signer".into())
    }
}

fn required(args: &[String], name: &str) -> Result<String, Box<dyn std::error::Error>> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("missing {name}").into())
}

fn digest_arg(args: &[String], name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let value = required(args, name)?;
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{name} must be a 64-character hex digest").into());
    }
    Ok(value)
}

fn optional_u64(args: &[String], name: &str) -> Option<u64> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .and_then(|pair| pair[1].parse().ok())
}

#[cfg(feature = "fixture-signer")]
fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|value| value == name)
}

#[cfg(feature = "fixture-signer")]
fn decode_hex(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if value.len() % 2 != 0 {
        return Err("hex value has odd length".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| "invalid hex value".into())
        })
        .collect()
}

#[cfg(feature = "fixture-signer")]
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
