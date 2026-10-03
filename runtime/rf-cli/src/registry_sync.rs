use crate::delivery::load_delivery;
use erf_bundle_cache::{resolve_cached, BundleCache, DeliveryError, DeploymentProfile, FileBundleCache};
use erf_bundle_delivery::{parse_delivery_json, DeliveryClient, MAX_RESPONSE_BYTES};
use erf_thin_client::{registry, transport::ConnectionProfile};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type SyncResult<T> = Result<T, &'static str>;

fn options(args: &[String]) -> SyncResult<(&str, BTreeMap<&str, &str>)> {
    if args.len() < 3 || args[0] != "sync" || args[1] != "rules" {
        return Err("REGISTRY_SYNC_INPUT_INVALID");
    }
    let id = args[2].as_str();
    let mut options = BTreeMap::new();
    let mut pairs = args[3..].chunks_exact(2);
    for pair in &mut pairs {
        if !matches!(pair[0].as_str(), "--version" | "--expected-digest" | "--delivery-descriptor" | "--repo-root" | "--server" | "--connection-profile" | "--timeout-ms")
            || pair[1].is_empty() || options.insert(pair[0].as_str(), pair[1].as_str()).is_some() {
            return Err("REGISTRY_SYNC_INPUT_INVALID");
        }
    }
    if !pairs.remainder().is_empty()
        || ["--version", "--expected-digest", "--delivery-descriptor", "--repo-root"].iter().any(|key| !options.contains_key(key))
        || options.contains_key("--server") == options.contains_key("--connection-profile") {
        return Err("REGISTRY_SYNC_INPUT_INVALID");
    }
    Ok((id, options))
}

pub fn run(args: &[String]) -> SyncResult<Value> {
    let (id, options) = options(args)?;
    let timeout_ms = options.get("--timeout-ms").unwrap_or(&"10000").parse::<u64>()
        .ok().filter(|n| *n > 0 && *n <= 600_000).ok_or("REGISTRY_SYNC_INPUT_INVALID")?;
    let timeout = Duration::from_millis(timeout_ms);
    let profile = options.get("--connection-profile")
        .map(|path| ConnectionProfile::load(Path::new(path)))
        .transpose().map_err(|_| "REGISTRY_CONNECTION_PROFILE_INVALID")?;
    let endpoint = match &profile {
        Some(profile) => profile.endpoint(),
        None => options["--server"],
    };
    let root = Path::new(options["--repo-root"]).canonicalize().map_err(|_| "REGISTRY_SYNC_REPO_INVALID")?;
    let descriptor_path = Path::new(options["--delivery-descriptor"]);
    let descriptor_path = if descriptor_path.is_absolute() { descriptor_path.to_path_buf() } else { root.join(descriptor_path) };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| "REGISTRY_SYNC_CLOCK_INVALID")?.as_secs();
    let loaded = load_delivery(&descriptor_path, &root, "connected", now)
        .map_err(|_| "REGISTRY_DELIVERY_INVALID")?;
    if loaded.rule_id != id || loaded.config.endpoint != endpoint {
        return Err("REGISTRY_DELIVERY_IDENTITY_MISMATCH");
    }
    let pin = loaded.registry_pin.as_ref().ok_or("REGISTRY_SIGNED_MAPPING_REQUIRED")?;
    if pin.version != options["--version"] || pin.store_digest != options["--expected-digest"] {
        return Err("REGISTRY_MAPPING_PIN_MISMATCH");
    }
    let receipt = match &profile {
        Some(profile) => registry::show_with_profile(profile, "rules", id, options["--version"], options["--expected-digest"], timeout),
        None => registry::show(endpoint, "rules", id, options["--version"], options["--expected-digest"], timeout),
    }?;
    let config = &loaded.config;
    let descriptor = &config.descriptor;
    let lock_path = loaded.execution_lock_path.as_ref().ok_or("REGISTRY_EXECUTION_LOCK_REQUIRED")?;
    let lock = rf_runner::SignedExecutionLock::from_file(lock_path).map_err(|_| "REGISTRY_EXECUTION_LOCK_INVALID")?;
    crate::validate_delivery_execution_lock_trust(&loaded, &lock).map_err(|_| "REGISTRY_EXECUTION_LOCK_UNTRUSTED")?;
    if lock.lock.content_digest() != hex(&descriptor.execution_lock_digest)
        || lock.lock.rule_bundle_digest != hex(&descriptor.bundle_digest)
        || descriptor.runtime_pack_digests.len() != 2
        || lock.lock.backend_pack_digest != hex(&descriptor.runtime_pack_digests[0])
        || lock.lock.resolver_pack_digest != hex(&descriptor.runtime_pack_digests[1]) {
        return Err("REGISTRY_EXECUTION_LOCK_MISMATCH");
    }
    // Verify actual signatures and revocation before either cache lookup or writes.
    rf_runner::verify_delivery_context(descriptor, config.verification.as_ref().ok_or("REGISTRY_DELIVERY_INVALID")?,
        &config.expected_compatibility_identity, &config.expected_platform, now)
        .map_err(|_| "REGISTRY_DELIVERY_UNTRUSTED")?;
    let cache = FileBundleCache::new(&config.cache_root);
    match resolve_cached(&cache, descriptor, DeploymentProfile::Connected, config.evidence, now) {
        Ok(_) => {},
        Err(DeliveryError::ServerFetchRequired) => {
            let fetched = match &profile {
                Some(profile) => {
                    let digest = format!("sha256:{}", hex(&descriptor.bundle_digest));
                    let (status, value) = profile.get_json(&format!("/rule-bundles/{digest}/delivery"), &digest, timeout, MAX_RESPONSE_BYTES)
                        .map_err(|_| "REGISTRY_DELIVERY_UNAVAILABLE")?;
                    parse_delivery_json(status, &value, descriptor).map_err(|_| "REGISTRY_DELIVERY_RESPONSE_INVALID")?
                },
                // registry::show already enforced literal loopback before this fetch.
                None => DeliveryClient::new(endpoint, timeout)
                    .and_then(|client| client.fetch(descriptor)).map_err(|_| "REGISTRY_DELIVERY_UNAVAILABLE")?,
            };
            if fetched.evidence.first_failure().is_some() { return Err("REGISTRY_DELIVERY_UNTRUSTED"); }
            cache.put(descriptor, &fetched.bytes).map_err(|_| "REGISTRY_CACHE_WRITE_FAILED")?;
        },
        Err(_) => return Err("REGISTRY_CACHE_INVALID"),
    }
    resolve_cached(&cache, descriptor, DeploymentProfile::StandaloneDeveloper, config.evidence, now)
        .map_err(|_| "REGISTRY_CACHE_INVALID")?;
    Ok(json!({
        "status": "CACHED", "authority": "UNKNOWN", "digestVerification": "PIN_MATCH",
        "artifactKind": "SignedRuleBundle", "bundleDigest": format!("sha256:{}", hex(&descriptor.bundle_digest)),
        "destination": Path::new(&config.cache_root).join("sha256").join(hex(&descriptor.bundle_digest)),
        "overwritePolicy": "IMMUTABLE_DIGEST_ADDRESSED", "offlineAvailable": true,
        "provenance": {"descriptor": descriptor_path, "trustRoot": "env:RULEOPS_DELIVERY_TRUST_ROOT_PUBLIC_KEY_HEX",
            "mapping": pin.provenance, "executionLock": lock_path,
            "descriptorDigest": format!("sha256:{}", hex(&descriptor.descriptor_digest())),
            "offlineCondition": "same descriptor, valid signatures, fresh trust/revocation metadata and compatible runtime"},
        "registry": receipt["registry"]
    }))
}

fn hex(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sync_requires_explicit_immutable_pin_and_one_transport() {
        let base = "sync rules corp.test --version 1.0.0 --expected-digest digest --delivery-descriptor descriptor --repo-root root --server http://127.0.0.1:9/v1";
        let args = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        assert!(options(&args(base)).is_ok());
        assert!(options(&args(&base.replace(" --version 1.0.0", ""))).is_err());
        assert!(options(&args(&format!("{base} --connection-profile approved.json"))).is_err());
        assert!(options(&args(&format!("{base} --version 2.0.0"))).is_err());
        assert!(options(&args(&base.replace(" --expected-digest digest", ""))).is_err());
        assert!(options(&args(&base.replace("rules", "standards"))).is_err());
    }
}
