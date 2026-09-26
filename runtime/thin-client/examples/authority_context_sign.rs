//! Test-fixture signer for the Rich Explain authority proof.
//!
//! This example is intentionally not a production signer. It uses a fixed
//! fixture key so the executable acceptance script can create a DSSE proof
//! and a matching local verifier configuration without credentials.
//!
//! usage: authority_context_sign INPUT.json OUTPUT.json TRUST.json

use ed25519_dalek::{Signer, SigningKey};
use erf_thin_client::{AuthorityContext, CLIENT_API_VERSION};
use erf_trust::{pae, DSSE_PAYLOAD_TYPE};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::process::exit;
use std::time::{SystemTime, UNIX_EPOCH};

const ROLE: &str = "gate-signing";
const KEY_ID: &str = "gate-key";

fn main() {
    let mut args = env::args().skip(1);
    let (Some(input), Some(output), Some(trust)) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: authority_context_sign INPUT.json OUTPUT.json TRUST.json");
        exit(2);
    };
    if args.next().is_some() {
        eprintln!("usage: authority_context_sign INPUT.json OUTPUT.json TRUST.json");
        exit(2);
    }
    if let Err(error) = run(&input, &output, &trust) {
        eprintln!("error: {error}");
        exit(2);
    }
}

fn run(input: &str, output: &str, trust: &str) -> Result<(), String> {
    let text = fs::read_to_string(input).map_err(|error| error.to_string())?;
    let mut value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let object = value.as_object().ok_or("authority context must be an object")?;
    if object.get("apiVersion").and_then(Value::as_str) != Some(CLIENT_API_VERSION)
        || object.get("kind").and_then(Value::as_str) != Some("AuthorityContext")
        || object.get("authority").and_then(Value::as_str) != Some("TRUSTED_CI")
    {
        return Err("input must be an unsigned TRUSTED_CI AuthorityContext".into());
    }
    let gate = object.get("gateDecision").and_then(Value::as_str).ok_or("missing gateDecision")?;
    let rule_id = object.get("ruleId").and_then(Value::as_str).ok_or("missing ruleId")?;
    let binding = object.get("bindingDigest").and_then(Value::as_str).ok_or("missing bindingDigest")?;
    let fingerprint = object.get("findingFingerprint").and_then(Value::as_str).ok_or("missing findingFingerprint")?;
    let issued_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    let expires_at = issued_at.saturating_add(3600);
    let payload = AuthorityContext::authority_payload(gate, rule_id, binding, fingerprint, issued_at, expires_at).map_err(|error| error.to_string())?;
    let key = SigningKey::from_bytes(&[77; 32]);
    let signature = key.sign(&pae(DSSE_PAYLOAD_TYPE, &payload));
    value["proof"] = json!({
        "payloadType": DSSE_PAYLOAD_TYPE,
        "payloadHex": hex(&payload),
        "keyId": KEY_ID,
        "signatureHex": hex(&signature.to_bytes()),
        "role": ROLE,
        "issuedAtEpochSecs": issued_at,
        "expiresAtEpochSecs": expires_at
    });
    fs::write(output, serde_json::to_string_pretty(&value).map_err(|error| error.to_string())? + "\n")
        .map_err(|error| error.to_string())?;

    let trust_value = json!({
        "apiVersion": CLIENT_API_VERSION,
        "kind": "AuthorityTrust",
        "role": ROLE,
        "keys": [{"keyId": KEY_ID, "publicKeyHex": hex(&key.verifying_key().to_bytes())}],
        "metadata": {"version": 1, "expiresAtEpochSecs": expires_at, "revokedKeyIds": []}
    });
    fs::write(trust, serde_json::to_string_pretty(&trust_value).map_err(|error| error.to_string())? + "\n")
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
