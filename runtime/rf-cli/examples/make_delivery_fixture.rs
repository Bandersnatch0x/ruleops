use ed25519_dalek::{Signer, SigningKey};
use erf_bundle_cache::{
    BundleCache, BundleDeliveryDescriptor, FileBundleCache, DELIVERY_DESCRIPTOR_VERSION,
    DELIVERY_PROTOCOL_VERSION,
};
use erf_packs::{
    OciPackRelease, BACKEND_RUNTIME_PACK_ASSET_TYPE, OCI_MANIFEST_MEDIA_TYPE,
    RUNTIME_PACK_SIGNING_ROLE, WORKSPACE_RESOLVER_PACK_ASSET_TYPE,
};
use erf_rule_bundle::{RuleBundleInputs, RuleBundleManifest, RULE_BUNDLE_SCHEMA_VERSION};
use erf_trust::{ArtifactStatement, RevocationSnapshot, SignedAsset, TrustMetadata, TrustStore};
use rf_runner::ExecutionLock;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const COMPATIBILITY: &str = "java21-pmd7-spotbugs4.9.8";
const PLATFORM: &str = "linux/amd64";
const RULE_ID: &str = "corp.java.no-system-exit";
const EXPIRES: u64 = 4_000_000_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 7 || args.len() > 9 {
        return Err("usage: make_delivery_fixture REPO_ROOT RULESET BACKEND_ASSET RESOLVER_ASSET DESCRIPTOR LOCK PROFILE [pmd|spotbugs] [RUNNER_ASSET]".into());
    }
    let repo_root = PathBuf::from(&args[0]);
    let ruleset = fs::read(&args[1])?;
    let backend_asset = fs::read(&args[2])?;
    let resolver_asset = fs::read(&args[3])?;
    let descriptor_path = PathBuf::from(&args[4]);
    let lock_path = PathBuf::from(&args[5]);
    let profile = &args[6];
    let backend = args.get(7).map(String::as_str).unwrap_or("pmd");
    let runner_asset = args.get(8).map(PathBuf::from);
    let (backend_id, backend_version) = match backend {
        "pmd" => ("pmd-java", "7.9.0"),
        "spotbugs" => ("spotbugs-java", "4.9.8"),
        other => return Err(format!("unsupported backend: {other}; expected pmd or spotbugs").into()),
    };

    let bundle_bytes = canonical_payload(&[("ruleset.xml", ruleset.as_slice())]);
    let bundle_digest: [u8; 32] = Sha256::digest(&bundle_bytes).into();
    let backend_asset_digest: [u8; 32] = Sha256::digest(&backend_asset).into();
    let resolver_asset_digest: [u8; 32] = Sha256::digest(&resolver_asset).into();
    let bundle_key = SigningKey::from_bytes(&[101; 32]);
    let pack_key = SigningKey::from_bytes(&[102; 32]);
    let delivery_root_key = SigningKey::from_bytes(&[100; 32]);
    let execution_lock_key = SigningKey::from_bytes(&[103; 32]);
    let snapshot_key = SigningKey::from_bytes(&[104; 32]);
    let backend_manifest = pack_manifest("backendRuntime", &backend_asset_digest, backend_asset.len() as u64);
    let resolver_manifest = pack_manifest("workspaceResolver", &resolver_asset_digest, resolver_asset.len() as u64);
    let backend_pack = pack(
        &backend_manifest,
        BACKEND_RUNTIME_PACK_ASSET_TYPE,
        &pack_key,
    );
    let resolver_pack = pack(
        &resolver_manifest,
        WORKSPACE_RESOLVER_PACK_ASSET_TYPE,
        &pack_key,
    );
    let backend_pack_digest: [u8; 32] = Sha256::digest(&backend_manifest).into();
    let resolver_pack_digest: [u8; 32] = Sha256::digest(&resolver_manifest).into();
    let revocation_snapshot = RevocationSnapshot {
        version: 1,
        issued_at_epoch_secs: 1,
        expires_at_epoch_secs: EXPIRES,
        revoked_asset_digests: Default::default(),
    };
    let revocation_snapshot_digest = revocation_snapshot.subject_digest();
    let signed_revocation_snapshot = revocation_snapshot.sign_with("snapshot-key", |payload| {
        snapshot_key.sign(payload).to_bytes().to_vec()
    });

    let mut lock = ExecutionLock {
        rule_bundle_digest: hex_digest(&bundle_digest),
        runner_release_digest: runner_asset
            .as_ref()
            .map(|path| Sha256::digest(fs::read(path).expect("runner asset is readable")))
            .map(|digest| hex_digest(&digest.into()))
            .unwrap_or_else(|| "11".repeat(32)),
        backend_pack_digest: hex_digest(&backend_pack_digest),
        resolver_pack_digest: hex_digest(&resolver_pack_digest),
        sandbox_provider_id: "rootless-oci-docker-v0".into(),
        sandbox_policy_digest: "22".repeat(32),
        resolver_sandbox_provider_id: "rootless-oci-docker-v0".into(),
        resolver_sandbox_policy_digest: "44".repeat(32),
        conformance_suite_digest: "33".repeat(32),
        resolver_id: "erf-maven-static-resolver".into(),
        resolver_version: "0.1.0".into(),
        backend_id: backend_id.into(),
        backend_version: backend_version.into(),
        protocol_major: erf_host_client::PROTOCOL_MAJOR,
        canonical_set_version: rf_gate::CANONICAL_SET_VERSION.into(),
        lock_digest: String::new(),
    };
    lock.lock_digest = lock.content_digest();
    let execution_lock_digest = hex_bytes(&lock.content_digest())?;
    write_lock(&lock_path, &lock, &execution_lock_key)?;

    let inputs = RuleBundleInputs {
        rule_release_digest: [1; 32],
        policy_profile_digest: [2; 32],
        execution_lock_digest,
        ontology_contract_pack_digest: [4; 32],
    };
    let manifest = RuleBundleManifest {
        bundle_version: RULE_BUNDLE_SCHEMA_VERSION.into(),
        bundle_digest,
        rule_release_digest: inputs.rule_release_digest,
        policy_profile_digest: inputs.policy_profile_digest,
        execution_lock_digest: inputs.execution_lock_digest,
        ontology_contract_pack_digest: inputs.ontology_contract_pack_digest,
        compatibility_identity: COMPATIBILITY.into(),
        platform: PLATFORM.into(),
    };
    let signed_bundle = manifest.sign_with("bundle-key", |payload| {
        bundle_key.sign(payload).to_bytes().to_vec()
    });
    let descriptor = BundleDeliveryDescriptor {
        protocol_version: DELIVERY_PROTOCOL_VERSION.into(),
        descriptor_version: DELIVERY_DESCRIPTOR_VERSION.into(),
        bundle_digest,
        bundle_schema_version: RULE_BUNDLE_SCHEMA_VERSION.into(),
        compatibility_identity: COMPATIBILITY.into(),
        platform: PLATFORM.into(),
        rule_release_digest: inputs.rule_release_digest,
        policy_profile_digest: inputs.policy_profile_digest,
        execution_lock_digest: inputs.execution_lock_digest,
        ontology_contract_pack_digest: inputs.ontology_contract_pack_digest,
        runtime_pack_digests: vec![
            Sha256::digest(&backend_manifest).into(),
            Sha256::digest(&resolver_manifest).into(),
        ],
        artifact_statement_digest: backend_pack.signed_statement.subject_digest(),
        revocation_snapshot_digest,
        issued_at_epoch_secs: 1,
        expires_at_epoch_secs: EXPIRES,
    };
    descriptor.validate()?;

    let cache_root = repo_root.join(".ruleops/cache/bundles");
    if let Some(parent) = cache_root.parent() {
        fs::create_dir_all(parent)?;
    }
    FileBundleCache::new(&cache_root).put(&descriptor, &bundle_bytes)?;
    let mut trust_store = TrustStore::default();
    trust_store.insert_key("bundle-key", bundle_key.verifying_key())?;
    trust_store.authorize_role(erf_rule_bundle::RULE_BUNDLE_SIGNING_ROLE, "bundle-key")?;
    trust_store.insert_key("pack-key", pack_key.verifying_key())?;
    trust_store.authorize_role(RUNTIME_PACK_SIGNING_ROLE, "pack-key")?;
    trust_store.insert_key("lock-key", execution_lock_key.verifying_key())?;
    trust_store.authorize_role("execution-lock-signing", "lock-key")?;
    trust_store.insert_key("snapshot-key", snapshot_key.verifying_key())?;
    trust_store.authorize_role("revocation-snapshot-signing", "snapshot-key")?;
    let trust_metadata = TrustMetadata {
        version: 1,
        expires_at_epoch_secs: EXPIRES,
        revoked_key_ids: Default::default(),
    };
    let delegated_keys = vec![
        (
            "bundle-key".to_string(),
            bundle_key.verifying_key().to_bytes().to_vec(),
            vec![erf_rule_bundle::RULE_BUNDLE_SIGNING_ROLE.to_string(), "registry-mapping-signing".to_string()],
        ),
        (
            "pack-key".to_string(),
            pack_key.verifying_key().to_bytes().to_vec(),
            vec![RUNTIME_PACK_SIGNING_ROLE.to_string()],
        ),
        (
            "lock-key".to_string(),
            execution_lock_key.verifying_key().to_bytes().to_vec(),
            vec!["execution-lock-signing".to_string()],
        ),
        (
            "snapshot-key".to_string(),
            snapshot_key.verifying_key().to_bytes().to_vec(),
            vec!["revocation-snapshot-signing".to_string()],
        ),
    ];
    let delegation_payload = trust_delegation_payload(&delegated_keys, &trust_metadata);
    let trust_delegation = sign_asset(
        delegation_payload,
        "delivery-root",
        &delivery_root_key,
    );

    // Test publishing identity is intentionally distinct from the RuleBundle digest.
    let mut registry_pin = json!({
        "type": "rules", "id": RULE_ID, "version": "1.0.0",
        "storeDigest": prefixed_digest(&Sha256::digest(b"registry-test-rule-release").into()),
        "bundleDigest": prefixed_digest(&descriptor.bundle_digest),
        "descriptorDigest": prefixed_digest(&descriptor.descriptor_digest()),
        "provenance": "test-fixture:publishing-system:not-production"
    });
    let mut mapping_payload = b"erf-registry-delivery-mapping-v1\0".to_vec();
    for field in ["type", "id", "version", "storeDigest", "bundleDigest", "descriptorDigest", "provenance"] {
        let value = registry_pin[field].as_str().unwrap();
        mapping_payload.extend_from_slice(&(value.len() as u64).to_be_bytes());
        mapping_payload.extend_from_slice(value.as_bytes());
    }
    registry_pin["signedMapping"] = signed_asset_json(&sign_asset(mapping_payload, "bundle-key", &bundle_key));
    let descriptor_json = json!({
        "apiVersion": DELIVERY_PROTOCOL_VERSION,
        "kind": "BundleDeliveryConfig",
        "schemaVersion": "rf-delivery-v1",
        "ruleId": RULE_ID,
        "registryPin": registry_pin,
        "endpoint": "http://127.0.0.1:9/v1",
        "cacheRoot": ".ruleops/cache/bundles",
        "profile": profile,
        "expectedCompatibilityIdentity": COMPATIBILITY,
        "expectedPlatform": PLATFORM,
        "timeoutMs": 2000,
        "executionLockPath": ".ruleops/delivery-execution.lock",
        "descriptor": descriptor_json(&descriptor),
        "verification": {
            "evidence": {
                "signatureValid": true,
                "trustMetadataFresh": true,
                "revocationSnapshotFresh": true,
                "referencedPacksAvailable": true,
                "trustedContextValid": true
            },
            "bundle": {
                "manifest": {
                    "bundleVersion": manifest.bundle_version.clone(),
                    "bundleDigest": prefixed_digest(&manifest.bundle_digest),
                    "ruleReleaseDigest": prefixed_digest(&manifest.rule_release_digest),
                    "policyProfileDigest": prefixed_digest(&manifest.policy_profile_digest),
                    "executionLockDigest": prefixed_digest(&manifest.execution_lock_digest),
                    "ontologyContractPackDigest": prefixed_digest(&manifest.ontology_contract_pack_digest),
                    "compatibilityIdentity": manifest.compatibility_identity.clone(),
                    "platform": manifest.platform.clone()
                },
                "manifestBytesHex": hex_bytes_string(&manifest.canonical_payload()),
                "signedManifest": signed_asset_json(&signed_bundle.signed_manifest)
            },
            "inputs": {
                "ruleReleaseDigest": prefixed_digest(&inputs.rule_release_digest),
                "policyProfileDigest": prefixed_digest(&inputs.policy_profile_digest),
                "executionLockDigest": prefixed_digest(&inputs.execution_lock_digest),
                "ontologyContractPackDigest": prefixed_digest(&inputs.ontology_contract_pack_digest)
            },
            "trust": {
                "keys": [
                    {
                        "keyId": "bundle-key",
                        "publicKeyHex": hex_bytes_string(&bundle_key.verifying_key().to_bytes()),
                        "roles": [erf_rule_bundle::RULE_BUNDLE_SIGNING_ROLE, "registry-mapping-signing"]
                    },
                    {
                        "keyId": "pack-key",
                        "publicKeyHex": hex_bytes_string(&pack_key.verifying_key().to_bytes()),
                        "roles": [RUNTIME_PACK_SIGNING_ROLE]
                    }
                    ,{
                        "keyId": "lock-key",
                        "publicKeyHex": hex_bytes_string(&execution_lock_key.verifying_key().to_bytes()),
                        "roles": ["execution-lock-signing"]
                    },
                    {
                        "keyId": "snapshot-key",
                        "publicKeyHex": hex_bytes_string(&snapshot_key.verifying_key().to_bytes()),
                        "roles": ["revocation-snapshot-signing"]
                    }
                ],
                "metadata": {
                    "version": trust_metadata.version,
                    "expiresAtEpochSecs": trust_metadata.expires_at_epoch_secs,
                    "revokedKeyIds": []
                },
                "delegation": signed_asset_json(&trust_delegation)
            },
            "revocationSnapshotDigest": prefixed_digest(&descriptor.revocation_snapshot_digest),
            "revocationSnapshot": {
                "version": revocation_snapshot.version,
                "issuedAtEpochSecs": revocation_snapshot.issued_at_epoch_secs,
                "expiresAtEpochSecs": revocation_snapshot.expires_at_epoch_secs,
                "revokedAssetDigests": [],
                "signedSnapshot": signed_asset_json(&signed_revocation_snapshot)
            },
            "packs": [
                pack_json("backendRuntime", &backend_pack, &backend_manifest, &backend_asset),
                pack_json("workspaceResolver", &resolver_pack, &resolver_manifest, &resolver_asset)
            ]
        }
    });
    if let Some(parent) = descriptor_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(descriptor_path, serde_json::to_vec_pretty(&descriptor_json)?)?;
    println!(
        "delivery-fixture=PASS bundleDigest={} profile={} cache={}",
        prefixed_digest(&descriptor.bundle_digest),
        profile,
        cache_root.display()
    );
    Ok(())
}

fn descriptor_json(descriptor: &BundleDeliveryDescriptor) -> serde_json::Value {
    json!({
        "protocolVersion": descriptor.protocol_version.clone(),
        "descriptorVersion": descriptor.descriptor_version.clone(),
        "bundleDigest": prefixed_digest(&descriptor.bundle_digest),
        "bundleSchemaVersion": descriptor.bundle_schema_version.clone(),
        "compatibilityIdentity": descriptor.compatibility_identity.clone(),
        "platform": descriptor.platform.clone(),
        "ruleReleaseDigest": prefixed_digest(&descriptor.rule_release_digest),
        "policyProfileDigest": prefixed_digest(&descriptor.policy_profile_digest),
        "executionLockDigest": prefixed_digest(&descriptor.execution_lock_digest),
        "ontologyContractPackDigest": prefixed_digest(&descriptor.ontology_contract_pack_digest),
        "runtimePackDigests": descriptor.runtime_pack_digests.iter().map(prefixed_digest).collect::<Vec<_>>(),
        "artifactStatementDigest": prefixed_digest(&descriptor.artifact_statement_digest),
        "revocationSnapshotDigest": prefixed_digest(&descriptor.revocation_snapshot_digest),
        "issuedAtEpochSecs": descriptor.issued_at_epoch_secs,
        "expiresAtEpochSecs": descriptor.expires_at_epoch_secs
    })
}

fn pack_manifest(kind: &str, asset_digest: &[u8; 32], asset_size: u64) -> Vec<u8> {
    let asset_type = match kind {
        "backendRuntime" => BACKEND_RUNTIME_PACK_ASSET_TYPE,
        "workspaceResolver" => WORKSPACE_RESOLVER_PACK_ASSET_TYPE,
        _ => panic!("unsupported Runtime Pack kind"),
    };
    let config = serde_json::to_vec(&json!({
        "kind": kind,
        "assetType": asset_type,
        "platform": PLATFORM,
        "compatibilityIdentity": COMPATIBILITY
    }))
    .expect("pack config is serializable");
    let config_digest: [u8; 32] = Sha256::digest(&config).into();
    serde_json::to_vec(&json!({
        "schemaVersion": 2,
        "mediaType": OCI_MANIFEST_MEDIA_TYPE,
        "config": {
            "mediaType": "application/vnd.erf.runtime-pack.config.v0",
            "digest": prefixed_digest(&config_digest),
            "size": config.len()
        },
        "layers": [{
            "mediaType": "application/vnd.erf.runtime-pack.asset.v0",
            "digest": prefixed_digest(asset_digest),
            "size": asset_size
        }],
        "kind": kind,
        "platform": PLATFORM,
        "assetDigest": prefixed_digest(asset_digest),
        "compatibilityIdentity": COMPATIBILITY,
        "annotations": {
            "org.rulefactory.kind": kind,
            "org.rulefactory.platform": PLATFORM,
            "org.rulefactory.compatibilityIdentity": COMPATIBILITY
        }
    }))
    .expect("pack manifest is serializable")
}

fn pack(manifest: &[u8], asset_type: &str, key: &SigningKey) -> OciPackRelease {
    let subject_digest: [u8; 32] = Sha256::digest(manifest).into();
    let statement = ArtifactStatement {
        subject_digest,
        asset_type: asset_type.into(),
        schema_version: "oci-pack/v0".into(),
        signing_role: RUNTIME_PACK_SIGNING_ROLE.into(),
        provenance_digest: [9; 32],
        sbom_digest: [10; 32],
        compatibility_identity: COMPATIBILITY.into(),
    };
    OciPackRelease {
        manifest_media_type: OCI_MANIFEST_MEDIA_TYPE.into(),
        platform: PLATFORM.into(),
        signed_statement: statement.sign_with("pack-key", |payload| {
            key.sign(payload).to_bytes().to_vec()
        }),
        artifact_statement: statement,
    }
}

fn pack_json(kind: &str, pack: &OciPackRelease, manifest: &[u8], asset: &[u8]) -> serde_json::Value {
    let config = pack_config(kind);
    let manifest_digest: [u8; 32] = Sha256::digest(manifest).into();
    let layout = br#"{"imageLayoutVersion":"1.0.0"}
"#;
    let index = serde_json::to_vec(&json!({
        "schemaVersion": 2,
        "manifests": [{
            "mediaType": OCI_MANIFEST_MEDIA_TYPE,
            "digest": prefixed_digest(&manifest_digest),
            "size": manifest.len(),
            "platform": {"architecture": "amd64", "os": "linux"}
        }]
    }))
    .expect("OCI index is serializable");
    json!({
        "kind": kind,
        "manifestBytesHex": hex_bytes_string(manifest),
        "ociLayout": {
            "ociLayoutBytesHex": hex_bytes_string(layout),
            "indexBytesHex": hex_bytes_string(&index),
            "configBytesHex": hex_bytes_string(&config),
            "layerBytesHex": hex_bytes_string(asset)
        },
        "release": {
            "manifestMediaType": pack.manifest_media_type.clone(),
            "platform": pack.platform.clone(),
            "artifactStatement": artifact_statement_json(&pack.artifact_statement),
            "signedStatement": signed_asset_json(&pack.signed_statement)
        }
    })
}

fn pack_config(kind: &str) -> Vec<u8> {
    let asset_type = match kind {
        "backendRuntime" => BACKEND_RUNTIME_PACK_ASSET_TYPE,
        "workspaceResolver" => WORKSPACE_RESOLVER_PACK_ASSET_TYPE,
        _ => panic!("unsupported Runtime Pack kind"),
    };
    serde_json::to_vec(&json!({
        "kind": kind,
        "assetType": asset_type,
        "platform": PLATFORM,
        "compatibilityIdentity": COMPATIBILITY
    }))
    .expect("pack config is serializable")
}

fn artifact_statement_json(statement: &ArtifactStatement) -> serde_json::Value {
    json!({
        "subjectDigest": prefixed_digest(&statement.subject_digest),
        "assetType": statement.asset_type.clone(),
        "schemaVersion": statement.schema_version.clone(),
        "signingRole": statement.signing_role.clone(),
        "provenanceDigest": prefixed_digest(&statement.provenance_digest),
        "sbomDigest": prefixed_digest(&statement.sbom_digest),
        "compatibilityIdentity": statement.compatibility_identity.clone()
    })
}

fn signed_asset_json(asset: &SignedAsset) -> serde_json::Value {
    asset.to_dsse_json()
}

fn sign_asset(payload: Vec<u8>, key_id: &str, key: &SigningKey) -> SignedAsset {
    let unsigned = SignedAsset {
        payload_type: erf_trust::DSSE_PAYLOAD_TYPE.into(),
        payload: payload.clone(),
        key_id: key_id.into(),
        signature: Vec::new(),
    };
    SignedAsset {
        signature: key.sign(&unsigned.signing_bytes()).to_bytes().to_vec(),
        ..unsigned
    }
}

fn trust_delegation_payload(
    delegated_keys: &[(String, Vec<u8>, Vec<String>)],
    metadata: &TrustMetadata,
) -> Vec<u8> {
    let mut keys = delegated_keys.to_vec();
    keys.sort_by(|left, right| left.0.cmp(&right.0));
    let key_values = keys
        .into_iter()
        .map(|(key_id, public_key, mut roles)| {
            roles.sort();
            let mut value = BTreeMap::new();
            value.insert("keyId".to_string(), Value::String(key_id));
            value.insert("publicKeyHex".to_string(), Value::String(hex_bytes_string(&public_key)));
            value.insert("roles".to_string(), Value::Array(roles.into_iter().map(Value::String).collect()));
            Value::Object(value.into_iter().collect())
        })
        .collect::<Vec<_>>();
    let mut metadata_value = BTreeMap::new();
    metadata_value.insert("version".to_string(), Value::from(metadata.version));
    metadata_value.insert("expiresAtEpochSecs".to_string(), Value::from(metadata.expires_at_epoch_secs));
    metadata_value.insert(
        "revokedKeyIds".to_string(),
        Value::Array(metadata.revoked_key_ids.iter().cloned().map(Value::String).collect()),
    );
    let mut root = BTreeMap::new();
    root.insert("keys".to_string(), Value::Array(key_values));
    root.insert("metadata".to_string(), Value::Object(metadata_value.into_iter().collect()));
    serde_json::to_vec(&Value::Object(root.into_iter().collect())).expect("trust delegation is serializable")
}

fn write_lock(path: &Path, lock: &ExecutionLock, key: &SigningKey) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let signed = sign_asset(lock.canonical_payload(), "lock-key", key);
    let text = format!(
        "rule_bundle_digest={}
runner_release_digest={}
backend_pack_digest={}
resolver_pack_digest={}
sandbox_provider_id={}
sandbox_policy_digest={}
resolver_sandbox_provider_id={}
resolver_sandbox_policy_digest={}
conformance_suite_digest={}
resolver_id={}
resolver_version={}
backend_id={}
backend_version={}
protocol_major={}
canonical_set_version={}
lock_digest={}
key_id={}
role=execution-lock-signing
public_key_hex={}
signature_hex={}
metadata_version=1
expires_at_epoch_secs={}
",
        lock.rule_bundle_digest,
        lock.runner_release_digest,
        lock.backend_pack_digest,
        lock.resolver_pack_digest,
        lock.sandbox_provider_id,
        lock.sandbox_policy_digest,
        lock.resolver_sandbox_provider_id,
        lock.resolver_sandbox_policy_digest,
        lock.conformance_suite_digest,
        lock.resolver_id,
        lock.resolver_version,
        lock.backend_id,
        lock.backend_version,
        lock.protocol_major,
        lock.canonical_set_version,
        lock.lock_digest,
        signed.key_id,
        hex_bytes_string(&key.verifying_key().to_bytes()),
        hex_bytes_string(&signed.signature),
        EXPIRES,
    );
    fs::write(path, text)?;
    Ok(())
}

fn canonical_payload(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut files = files.to_vec();
    files.sort_by(|left, right| left.0.cmp(right.0));
    let mut payload = Vec::new();
    for (path, bytes) in files {
        payload.extend_from_slice(&(path.len() as u64).to_be_bytes());
        payload.extend_from_slice(path.as_bytes());
        payload.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        payload.extend_from_slice(bytes);
    }
    payload
}

fn prefixed_digest(digest: &[u8; 32]) -> String {
    format!("sha256:{}", hex_digest(digest))
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_bytes_string(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_bytes(value: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    if value.len() != 64 {
        return Err("digest must contain 64 hex characters".into());
    }
    let mut result = [0_u8; 32];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(result)
}
