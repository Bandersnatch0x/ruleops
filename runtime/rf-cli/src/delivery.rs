use erf_bundle_cache::{BundleDeliveryDescriptor, DeploymentProfile, VerificationEvidence};
use erf_packs::{OciPackRelease, OCI_MANIFEST_MEDIA_TYPE};
use erf_rule_bundle::{RuleBundleInputs, RuleBundleManifest, SignedRuleBundle};
use erf_trust::{ArtifactStatement, RevocationSnapshot, SignedAsset, TrustMetadata, TrustStore, DSSE_PAYLOAD_TYPE, IN_TOTO_STATEMENT_PAYLOAD_TYPE};
use rf_runner::{BundleDeliveryConfig, BundleDeliveryVerification, BundlePackKind, BundlePackVerification};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const API_VERSION: &str = "rulefactory.io/v1";
const KIND: &str = "BundleDeliveryConfig";
const SCHEMA_VERSION: &str = "rf-delivery-v1";
const DESCRIPTOR_VERSION: &str = "rule-bundle-delivery-v1";
const DELIVERY_TRUST_ROOT_ENV: &str = "RULEOPS_DELIVERY_TRUST_ROOT_PUBLIC_KEY_HEX";
const DELIVERY_TRUST_ROOT_ROLE: &str = "delivery-trust-root";
const MAX_DELIVERY_DESCRIPTOR_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENCODED_FIELD_CHARS: usize = 64 * 1024 * 1024;

#[derive(Debug)]
pub struct LoadedDelivery {
    pub config: BundleDeliveryConfig,
    pub execution_lock_path: Option<PathBuf>,
    pub rule_id: String,
    pub registry_pin: Option<RegistryPin>,
}

pub fn load_delivery(
    path: &Path,
    repo_root: &Path,
    expected_profile: &str,
    now_epoch_secs: u64,
) -> Result<LoadedDelivery, String> {
    let path = canonical_workspace_file(path, repo_root, "delivery descriptor")?;
    let metadata = fs::metadata(&path).map_err(|error| format!("cannot stat delivery descriptor: {error}"))?;
    if metadata.len() > MAX_DELIVERY_DESCRIPTOR_BYTES {
        return Err("delivery descriptor exceeds the configured size limit".into());
    }
    let mut text = String::new();
    fs::File::open(&path)
        .map_err(|error| format!("cannot read delivery descriptor: {error}"))?
        .take(MAX_DELIVERY_DESCRIPTOR_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("cannot read delivery descriptor: {error}"))?;
    if text.len() as u64 > MAX_DELIVERY_DESCRIPTOR_BYTES {
        return Err("delivery descriptor exceeds the configured size limit".into());
    }
    let value = serde_json::from_str::<Value>(&text).map_err(|error| format!("delivery descriptor is invalid JSON: {error}"))?;
    let object = object(&value, "delivery descriptor")?;
    reject_unknown(
        object,
        &[
            "apiVersion", "kind", "schemaVersion", "ruleId", "endpoint", "cacheRoot", "profile",
            "expectedCompatibilityIdentity", "expectedPlatform", "timeoutMs",
            "descriptor", "verification", "executionLockPath", "registryPin",
        ],
        "delivery descriptor",
    )?;
    expect_string(object, "apiVersion", API_VERSION)?;
    expect_string(object, "kind", KIND)?;
    expect_string(object, "schemaVersion", SCHEMA_VERSION)?;
    let rule_id = required_string(object, "ruleId")?;
    validate_rule_id(&rule_id)?;
    let endpoint = required_string(object, "endpoint")?;
    let cache_root_value = required_string(object, "cacheRoot")?;
    let cache_root = workspace_path(repo_root, &cache_root_value, "cacheRoot", false)?;
    let profile_value = required_string(object, "profile")?;
    let profile = parse_profile(&profile_value)?;
    if profile_name(profile) != expected_profile {
        return Err(format!(
            "delivery descriptor profile does not match project execution mode: expected {expected_profile}, got {}",
            profile_name(profile)
        ));
    }
    let expected_compatibility_identity = required_string(object, "expectedCompatibilityIdentity")?;
    let expected_platform = required_string(object, "expectedPlatform")?;
    reject_empty_identity("expectedCompatibilityIdentity", &expected_compatibility_identity)?;
    reject_empty_identity("expectedPlatform", &expected_platform)?;
    let timeout_ms = required_u64(object, "timeoutMs")?;
    if timeout_ms == 0 {
        return Err("delivery descriptor timeoutMs must be positive".into());
    }
    let descriptor = parse_descriptor(required_object(object, "descriptor")?)?;
    let verification = parse_verification(required_object(object, "verification")?, now_epoch_secs)?;
    if descriptor.runtime_pack_digests.len() != verification.context.packs.len() {
        return Err("descriptor.runtimePackDigests must match verification.packs".into());
    }
    let execution_lock_path = optional_string(object, "executionLockPath")?
        .map(|value| workspace_path(repo_root, &value, "executionLockPath", true))
        .transpose()?;

    descriptor.validate().map_err(|error| format!("invalid delivery descriptor: {error}"))?;
    if descriptor.compatibility_identity != expected_compatibility_identity {
        return Err("delivery descriptor compatibility identity does not match the selected execution context".into());
    }
    if descriptor.platform != expected_platform {
        return Err("delivery descriptor platform does not match the selected execution context".into());
    }
    if descriptor.revocation_snapshot_digest != verification.revocation_snapshot_digest {
        return Err("delivery descriptor revocation snapshot does not match verification context".into());
    }
    let inputs = verification.context.inputs;
    for (name, descriptor_digest, input_digest) in [
        ("ruleReleaseDigest", descriptor.rule_release_digest, inputs.rule_release_digest),
        ("policyProfileDigest", descriptor.policy_profile_digest, inputs.policy_profile_digest),
        ("executionLockDigest", descriptor.execution_lock_digest, inputs.execution_lock_digest),
        (
            "ontologyContractPackDigest",
            descriptor.ontology_contract_pack_digest,
            inputs.ontology_contract_pack_digest,
        ),
    ] {
        if descriptor_digest != input_digest {
            return Err(format!("delivery descriptor {name} does not match verification.inputs"));
        }
    }
    let manifest = &verification.context.signed_bundle.manifest;
    if manifest.compatibility_identity != descriptor.compatibility_identity
        || manifest.platform != descriptor.platform
        || manifest.rule_release_digest != descriptor.rule_release_digest
        || manifest.policy_profile_digest != descriptor.policy_profile_digest
        || manifest.execution_lock_digest != descriptor.execution_lock_digest
        || manifest.ontology_contract_pack_digest != descriptor.ontology_contract_pack_digest
    {
        return Err("delivery descriptor does not match the signed RuleBundle manifest".into());
    }
    if verification.context.manifest_bytes != manifest.canonical_payload()
        || verification.context.signed_bundle.signed_manifest.payload != manifest.canonical_payload()
    {
        return Err("verification.bundle manifest bytes are not canonical".into());
    }
    if manifest.bundle_digest != descriptor.bundle_digest {
        return Err("signed RuleBundle manifest does not match the delivered bundle digest".into());
    }
    if verification.context.packs[0].release.signed_statement.subject_digest()
        != descriptor.artifact_statement_digest
    {
        return Err("delivery descriptor artifact statement does not match the backend Runtime Pack".into());
    }

    let registry_pin = object.get("registryPin")
        .map(|value| parse_registry_pin(value, &rule_id, &descriptor, &verification.context, now_epoch_secs))
        .transpose()?;
    Ok(LoadedDelivery {
        registry_pin,
        config: BundleDeliveryConfig {
            endpoint,
            descriptor,
            expected_compatibility_identity,
            expected_platform,
            cache_root: cache_root.to_string_lossy().into_owned(),
            profile,
            evidence: verification.evidence,
            verification: Some(verification.context),
            now_epoch_secs,
            timeout_ms,
        },
        execution_lock_path,
        rule_id,
    })
}

struct ParsedVerification {
    evidence: VerificationEvidence,
    revocation_snapshot_digest: [u8; 32],
    context: BundleDeliveryVerification,
}

fn parse_descriptor(object: &Map<String, Value>) -> Result<BundleDeliveryDescriptor, String> {
    reject_unknown(
        object,
        &[
            "protocolVersion", "descriptorVersion", "bundleDigest", "bundleSchemaVersion",
            "compatibilityIdentity", "platform", "ruleReleaseDigest", "policyProfileDigest",
            "executionLockDigest", "ontologyContractPackDigest", "runtimePackDigests",
            "artifactStatementDigest", "revocationSnapshotDigest", "issuedAtEpochSecs", "expiresAtEpochSecs",
        ],
        "descriptor",
    )?;
    let runtime_pack_digests = required_array(object, "runtimePackDigests")?
        .iter()
        .enumerate()
        .map(|(index, value)| parse_digest_value(value, &format!("descriptor.runtimePackDigests[{index}]")))
        .collect::<Result<Vec<_>, _>>()?;
    let descriptor = BundleDeliveryDescriptor {
        protocol_version: required_string(object, "protocolVersion")?,
        descriptor_version: required_string(object, "descriptorVersion")?,
        bundle_digest: parse_digest(object, "bundleDigest")?,
        bundle_schema_version: required_string(object, "bundleSchemaVersion")?,
        compatibility_identity: required_string(object, "compatibilityIdentity")?,
        platform: required_string(object, "platform")?,
        rule_release_digest: parse_digest(object, "ruleReleaseDigest")?,
        policy_profile_digest: parse_digest(object, "policyProfileDigest")?,
        execution_lock_digest: parse_digest(object, "executionLockDigest")?,
        ontology_contract_pack_digest: parse_digest(object, "ontologyContractPackDigest")?,
        runtime_pack_digests,
        artifact_statement_digest: parse_digest(object, "artifactStatementDigest")?,
        revocation_snapshot_digest: parse_digest(object, "revocationSnapshotDigest")?,
        issued_at_epoch_secs: required_u64(object, "issuedAtEpochSecs")?,
        expires_at_epoch_secs: required_u64(object, "expiresAtEpochSecs")?,
    };
    if descriptor.descriptor_version != DESCRIPTOR_VERSION {
        return Err(format!(
            "descriptor.descriptorVersion must be {DESCRIPTOR_VERSION}"
        ));
    }
    Ok(descriptor)
}

fn parse_verification(root: &Map<String, Value>, now_epoch_secs: u64) -> Result<ParsedVerification, String> {
    reject_unknown(root, &["evidence", "bundle", "inputs", "trust", "revocationSnapshotDigest", "revocationSnapshot", "packs"], "verification")?;
    let evidence_object = required_object(root, "evidence")?;
    reject_unknown(
        evidence_object,
        &[
            "signatureValid", "trustMetadataFresh", "revocationSnapshotFresh", "referencedPacksAvailable",
            "trustedContextValid",
        ],
        "verification.evidence",
    )?;
    let evidence = VerificationEvidence {
        signature_valid: required_bool(evidence_object, "signatureValid")?,
        trust_metadata_fresh: required_bool(evidence_object, "trustMetadataFresh")?,
        revocation_snapshot_fresh: required_bool(evidence_object, "revocationSnapshotFresh")?,
        referenced_packs_available: required_bool(evidence_object, "referencedPacksAvailable")?,
        trusted_context_valid: required_bool(evidence_object, "trustedContextValid")?,
    };
    let revocation_snapshot_digest = parse_digest(root, "revocationSnapshotDigest")?;
    let inputs_object = required_object(root, "inputs")?;
    reject_unknown(
        inputs_object,
        &["ruleReleaseDigest", "policyProfileDigest", "executionLockDigest", "ontologyContractPackDigest"],
        "verification.inputs",
    )?;
    let inputs = RuleBundleInputs {
        rule_release_digest: parse_digest(inputs_object, "ruleReleaseDigest")?,
        policy_profile_digest: parse_digest(inputs_object, "policyProfileDigest")?,
        execution_lock_digest: parse_digest(inputs_object, "executionLockDigest")?,
        ontology_contract_pack_digest: parse_digest(inputs_object, "ontologyContractPackDigest")?,
    };

    let bundle_object = required_object(root, "bundle")?;
    reject_unknown(bundle_object, &["manifest", "manifestBytesHex", "signedManifest"], "verification.bundle")?;
    let manifest_object = required_object(bundle_object, "manifest")?;
    reject_unknown(
        manifest_object,
        &[
            "bundleVersion", "bundleDigest", "ruleReleaseDigest", "policyProfileDigest", "executionLockDigest",
            "ontologyContractPackDigest", "compatibilityIdentity", "platform",
        ],
        "verification.bundle.manifest",
    )?;
    let manifest = RuleBundleManifest {
        bundle_version: required_string(manifest_object, "bundleVersion")?,
        bundle_digest: parse_digest(manifest_object, "bundleDigest")?,
        rule_release_digest: parse_digest(manifest_object, "ruleReleaseDigest")?,
        policy_profile_digest: parse_digest(manifest_object, "policyProfileDigest")?,
        execution_lock_digest: parse_digest(manifest_object, "executionLockDigest")?,
        ontology_contract_pack_digest: parse_digest(manifest_object, "ontologyContractPackDigest")?,
        compatibility_identity: required_string(manifest_object, "compatibilityIdentity")?,
        platform: required_string(manifest_object, "platform")?,
    };
    let signed_bundle = SignedRuleBundle {
        manifest,
        signed_manifest: parse_signed_asset(required_object(bundle_object, "signedManifest")?, "verification.bundle.signedManifest")?,
    };
    let manifest_bytes = parse_hex_string(required_string(bundle_object, "manifestBytesHex")?, "verification.bundle.manifestBytesHex")?;

    let trust_object = required_object(root, "trust")?;
    reject_unknown(trust_object, &["keys", "metadata", "delegation"], "verification.trust")?;
    let mut trust_store = TrustStore::default();
    let mut delegated_keys = Vec::new();
    for (index, key_value) in required_array(trust_object, "keys")?.iter().enumerate() {
        let key_object = object(key_value, &format!("verification.trust.keys[{index}]"))?;
        reject_unknown(key_object, &["keyId", "publicKeyHex", "roles"], "verification.trust.keys")?;
        let key_id = required_string(key_object, "keyId")?;
        let public_key = parse_hex_string(required_string(key_object, "publicKeyHex")?, "verification.trust.publicKeyHex")?;
        trust_store.insert_public_key_bytes(&key_id, &public_key).map_err(|error| error.to_string())?;
        let mut roles = Vec::new();
        for role in required_array(key_object, "roles")?.iter() {
            let role = role.as_str().ok_or_else(|| format!("verification.trust.keys[{index}].roles must contain strings"))?;
            trust_store.authorize_role(role, &key_id).map_err(|error| error.to_string())?;
            roles.push(role.to_string());
        }
        roles.sort();
        roles.dedup();
        delegated_keys.push((key_id, public_key, roles));
    }
    let trust_metadata = parse_trust_metadata(required_object(trust_object, "metadata")?)?;
    verify_trust_delegation(
        required_object(trust_object, "delegation")?,
        &delegated_keys,
        &trust_metadata,
    )?;
    let snapshot_object = required_object(root, "revocationSnapshot")?;
    reject_unknown(
        snapshot_object,
        &[
            "version", "issuedAtEpochSecs", "expiresAtEpochSecs", "revokedAssetDigests", "signedSnapshot",
        ],
        "verification.revocationSnapshot",
    )?;
    let revoked_asset_digests = required_array(snapshot_object, "revokedAssetDigests")?
        .iter()
        .map(|value| parse_digest_value(value, "verification.revocationSnapshot.revokedAssetDigests"))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let revocation_snapshot = RevocationSnapshot {
        version: required_u64(snapshot_object, "version")?,
        issued_at_epoch_secs: required_u64(snapshot_object, "issuedAtEpochSecs")?,
        expires_at_epoch_secs: required_u64(snapshot_object, "expiresAtEpochSecs")?,
        revoked_asset_digests,
    };
    if revocation_snapshot.subject_digest() != revocation_snapshot_digest {
        return Err("verification.revocationSnapshot does not match revocationSnapshotDigest".into());
    }
    let snapshot_asset = parse_signed_asset(
        required_object(snapshot_object, "signedSnapshot")?,
        "verification.revocationSnapshot.signedSnapshot",
    )?;
    revocation_snapshot
        .verify_signed_with_metadata(
            &snapshot_asset,
            &trust_store,
            "revocation-snapshot-signing",
            &trust_metadata,
            now_epoch_secs,
        )
        .map_err(|error| format!("revocation snapshot verification failed: {error}"))?;

    let packs = required_array(root, "packs")?
        .iter()
        .enumerate()
        .map(|(index, value)| parse_pack(value, index))
        .collect::<Result<Vec<_>, _>>()?;
    if packs.len() != 2
        || packs[0].kind != BundlePackKind::BackendRuntime
        || packs[1].kind != BundlePackKind::WorkspaceResolver
    {
        return Err(
            "verification.packs must contain exactly backendRuntime followed by workspaceResolver"
                .into(),
        );
    }
    Ok(ParsedVerification {
        evidence,
        revocation_snapshot_digest,
        context: BundleDeliveryVerification {
            signed_bundle,
            manifest_bytes,
            inputs,
            trust_store,
            trust_metadata: Some(trust_metadata),
            revocation_snapshot: Some(revocation_snapshot),
            revocation_snapshot_digest,
            packs,
        },
    })
}

fn verify_trust_delegation(
    object: &Map<String, Value>,
    delegated_keys: &[(String, Vec<u8>, Vec<String>)],
    metadata: &TrustMetadata,
) -> Result<(), String> {
    reject_unknown(
        object,
        &["payloadType", "payloadHex", "keyId", "signatureHex", "payload", "signatures"],
        "verification.trust.delegation",
    )?;
    let delegation = parse_signed_asset(object, "verification.trust.delegation")?;
    let expected_payload = canonical_trust_delegation_payload(delegated_keys, metadata)?;
    if delegation.payload != expected_payload {
        return Err("verification.trust.delegation payload does not match delegated keys and metadata".into());
    }
    let root_hex = env::var(DELIVERY_TRUST_ROOT_ENV)
        .map_err(|_| format!("{DELIVERY_TRUST_ROOT_ENV} is required for delivery trust verification"))?;
    let root_public_key = parse_hex_string(root_hex, DELIVERY_TRUST_ROOT_ENV)?;
    let mut root_store = TrustStore::default();
    root_store
        .insert_public_key_bytes(&delegation.key_id, &root_public_key)
        .map_err(|error| format!("delivery trust root is invalid: {error}"))?;
    root_store
        .authorize_role(DELIVERY_TRUST_ROOT_ROLE, &delegation.key_id)
        .map_err(|error| format!("delivery trust root role is invalid: {error}"))?;
    root_store
        .verify(&delegation, DELIVERY_TRUST_ROOT_ROLE)
        .map_err(|error| format!("delivery trust delegation signature verification failed: {error}"))?;
    Ok(())
}

fn canonical_trust_delegation_payload(
    delegated_keys: &[(String, Vec<u8>, Vec<String>)],
    metadata: &TrustMetadata,
) -> Result<Vec<u8>, String> {
    let mut keys = delegated_keys.to_vec();
    keys.sort_by(|left, right| left.0.cmp(&right.0));
    let key_values = keys
        .into_iter()
        .map(|(key_id, public_key, roles)| {
            let mut value = BTreeMap::new();
            value.insert("keyId".to_string(), Value::String(key_id));
            value.insert("publicKeyHex".to_string(), Value::String(hex_bytes(&public_key)));
            value.insert(
                "roles".to_string(),
                Value::Array(roles.into_iter().map(Value::String).collect()),
            );
            Value::Object(value.into_iter().collect())
        })
        .collect::<Vec<_>>();
    let mut metadata_value = BTreeMap::new();
    metadata_value.insert("version".to_string(), Value::from(metadata.version));
    metadata_value.insert(
        "expiresAtEpochSecs".to_string(),
        Value::from(metadata.expires_at_epoch_secs),
    );
    metadata_value.insert(
        "revokedKeyIds".to_string(),
        Value::Array(
            metadata
                .revoked_key_ids
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    let mut root = BTreeMap::new();
    root.insert("keys".to_string(), Value::Array(key_values));
    root.insert(
        "metadata".to_string(),
        Value::Object(metadata_value.into_iter().collect()),
    );
    serde_json::to_vec(&Value::Object(root.into_iter().collect()))
        .map_err(|error| format!("delivery trust delegation payload cannot be canonicalized: {error}"))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn parse_pack(value: &Value, index: usize) -> Result<BundlePackVerification, String> {
    let pack_object = object(value, &format!("verification.packs[{index}]"))?;
    reject_unknown(pack_object, &["kind", "manifestBytesHex", "ociLayout", "release"], "verification.pack")?;
    let kind = match required_string(pack_object, "kind")?.as_str() {
        "backendRuntime" => BundlePackKind::BackendRuntime,
        "workspaceResolver" => BundlePackKind::WorkspaceResolver,
        other => return Err(format!("verification.packs[{index}].kind is unsupported: {other}")),
    };
    let manifest_bytes = parse_hex_string(required_string(pack_object, "manifestBytesHex")?, "verification.pack.manifestBytesHex")?;
    let oci_object = required_object(pack_object, "ociLayout")?;
    reject_unknown(
        oci_object,
        &["ociLayoutBytesHex", "indexBytesHex", "configBytesHex", "layerBytesHex"],
        "verification.pack.ociLayout",
    )?;
    let oci_layout = rf_runner::OciLayoutVerification {
        layout_bytes: parse_hex_string(required_string(oci_object, "ociLayoutBytesHex")?, "verification.pack.ociLayout.ociLayoutBytesHex")?,
        index_bytes: parse_hex_string(required_string(oci_object, "indexBytesHex")?, "verification.pack.ociLayout.indexBytesHex")?,
        config_bytes: parse_hex_string(required_string(oci_object, "configBytesHex")?, "verification.pack.ociLayout.configBytesHex")?,
        layer_bytes: parse_hex_string(required_string(oci_object, "layerBytesHex")?, "verification.pack.ociLayout.layerBytesHex")?,
    };
    let manifest_value = serde_json::from_slice::<Value>(&manifest_bytes)
        .map_err(|error| format!("verification.packs[{index}].manifestBytesHex is not valid JSON: {error}"))?;
    let manifest_object = object(&manifest_value, &format!("verification.packs[{index}].manifest"))?;
    reject_unknown(
        manifest_object,
        &[
            "schemaVersion", "mediaType", "config", "layers", "annotations", "kind", "platform", "assetDigest",
            "compatibilityIdentity",
        ],
        "verification.pack.manifest",
    )?;
    if manifest_object.get("schemaVersion").and_then(Value::as_u64) != Some(2) {
        return Err(format!("verification.packs[{index}].manifest.schemaVersion must be 2"));
    }
    if required_string(manifest_object, "mediaType")? != OCI_MANIFEST_MEDIA_TYPE {
        return Err(format!("verification.packs[{index}].manifest.mediaType is unsupported"));
    }
    let config = required_object(manifest_object, "config")?;
    reject_unknown(config, &["mediaType", "digest", "size"], "verification.pack.manifest.config")?;
    if required_string(config, "mediaType")? != "application/vnd.erf.runtime-pack.config.v0"
        || parse_digest(config, "digest")? == [0; 32]
        || required_u64(config, "size")? == 0
    {
        return Err(format!("verification.packs[{index}].manifest.config is invalid"));
    }
    let layers = required_array(manifest_object, "layers")?;
    if layers.len() != 1 {
        return Err(format!("verification.packs[{index}].manifest.layers must contain exactly one asset layer"));
    }
    let layer = object(&layers[0], &format!("verification.packs[{index}].manifest.layers[0]"))?;
    reject_unknown(layer, &["mediaType", "digest", "size"], "verification.pack.manifest.layer")?;
    if required_string(layer, "mediaType")? != "application/vnd.erf.runtime-pack.asset.v0"
        || required_u64(layer, "size")? == 0
    {
        return Err(format!("verification.packs[{index}].manifest.layers[0] is invalid"));
    }
    let expected_kind = match kind {
        BundlePackKind::BackendRuntime => "backendRuntime",
        BundlePackKind::WorkspaceResolver => "workspaceResolver",
    };
    if required_string(manifest_object, "kind")? != expected_kind {
        return Err(format!("verification.packs[{index}].manifest.kind does not match the pack role"));
    }
    let manifest_platform = required_string(manifest_object, "platform")?;
    let manifest_compatibility_identity = required_string(manifest_object, "compatibilityIdentity")?;
    let annotations = required_object(manifest_object, "annotations")?;
    reject_unknown(
        annotations,
        &[
            "org.rulefactory.kind", "org.rulefactory.platform", "org.rulefactory.compatibilityIdentity",
        ],
        "verification.pack.manifest.annotations",
    )?;
    if required_string(annotations, "org.rulefactory.kind")? != expected_kind
        || required_string(annotations, "org.rulefactory.platform")? != manifest_platform
        || required_string(annotations, "org.rulefactory.compatibilityIdentity")? != manifest_compatibility_identity
    {
        return Err(format!("verification.packs[{index}].manifest annotations do not match the manifest identity"));
    }
    let asset_digest = parse_digest(manifest_object, "assetDigest")?;
    if parse_digest(layer, "digest")? != asset_digest {
        return Err(format!("verification.packs[{index}].manifest layer digest does not match assetDigest"));
    }
    let release_object = required_object(pack_object, "release")?;
    reject_unknown(release_object, &["manifestMediaType", "platform", "artifactStatement", "signedStatement"], "verification.pack.release")?;
    let release = OciPackRelease {
        manifest_media_type: required_string(release_object, "manifestMediaType")?,
        platform: required_string(release_object, "platform")?,
        artifact_statement: parse_artifact_statement(required_object(release_object, "artifactStatement")?)?,
        signed_statement: parse_signed_asset(required_object(release_object, "signedStatement")?, "verification.pack.signedStatement")?,
    };
    if release.manifest_media_type != OCI_MANIFEST_MEDIA_TYPE {
        return Err(format!("verification.packs[{index}].release.manifestMediaType is unsupported"));
    }
    Ok(BundlePackVerification {
        kind,
        release,
        manifest_bytes,
        manifest_platform,
        manifest_compatibility_identity,
        asset_digest,
        oci_layout: Some(oci_layout),
    })
}

fn parse_artifact_statement(object: &Map<String, Value>) -> Result<ArtifactStatement, String> {
    reject_unknown(
        object,
        &["subjectDigest", "assetType", "schemaVersion", "signingRole", "provenanceDigest", "sbomDigest", "compatibilityIdentity"],
        "verification.artifactStatement",
    )?;
    Ok(ArtifactStatement {
        subject_digest: parse_digest(object, "subjectDigest")?,
        asset_type: required_string(object, "assetType")?,
        schema_version: required_string(object, "schemaVersion")?,
        signing_role: required_string(object, "signingRole")?,
        provenance_digest: parse_digest(object, "provenanceDigest")?,
        sbom_digest: parse_digest(object, "sbomDigest")?,
        compatibility_identity: required_string(object, "compatibilityIdentity")?,
    })
}

fn parse_signed_asset(object: &Map<String, Value>, field: &str) -> Result<SignedAsset, String> {
    if object.contains_key("payload") || object.contains_key("signatures") {
        let asset = SignedAsset::from_dsse_json(&Value::Object(object.clone()))
            .map_err(|error| format!("{field} is not a valid DSSE envelope: {error}"))?;
        if asset.payload_type != DSSE_PAYLOAD_TYPE
            && asset.payload_type != IN_TOTO_STATEMENT_PAYLOAD_TYPE
        {
            return Err(format!("{field}.payloadType is unsupported"));
        }
        return Ok(asset);
    }
    // Keep reading the pre-DSSE delivery form for already-issued local
    // descriptors. New portable artifacts must use the branch above.
    reject_unknown(object, &["payloadType", "payloadHex", "keyId", "signatureHex"], field)?;
    let payload_type = required_string(object, "payloadType")?;
    if payload_type != DSSE_PAYLOAD_TYPE && payload_type != IN_TOTO_STATEMENT_PAYLOAD_TYPE {
        return Err(format!("{field}.payloadType is unsupported"));
    }
    Ok(SignedAsset {
        payload_type,
        payload: parse_hex_string(required_string(object, "payloadHex")?, &format!("{field}.payloadHex"))?,
        key_id: required_string(object, "keyId")?,
        signature: parse_hex_string(required_string(object, "signatureHex")?, &format!("{field}.signatureHex"))?,
    })
}

fn parse_trust_metadata(object: &Map<String, Value>) -> Result<TrustMetadata, String> {
    reject_unknown(object, &["version", "expiresAtEpochSecs", "revokedKeyIds"], "verification.trust.metadata")?;
    let revoked_key_ids = required_array(object, "revokedKeyIds")?
        .iter()
        .map(|value| value.as_str().map(str::to_string).ok_or_else(|| "revokedKeyIds must contain strings".to_string()))
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(TrustMetadata {
        version: required_u64(object, "version")?,
        expires_at_epoch_secs: required_u64(object, "expiresAtEpochSecs")?,
        revoked_key_ids,
    })
}

fn parse_profile(value: &str) -> Result<DeploymentProfile, String> {
    match value {
        "connected" => Ok(DeploymentProfile::Connected),
        "enterpriseAirGapped" => Ok(DeploymentProfile::EnterpriseAirGapped),
        "standaloneDeveloper" => Ok(DeploymentProfile::StandaloneDeveloper),
        _ => Err(format!("unsupported delivery profile: {value}")),
    }
}

fn profile_name(profile: DeploymentProfile) -> &'static str {
    match profile {
        DeploymentProfile::Connected => "connected",
        DeploymentProfile::EnterpriseAirGapped => "enterpriseAirGapped",
        DeploymentProfile::StandaloneDeveloper => "standaloneDeveloper",
    }
}

fn canonical_workspace_file(path: &Path, repo_root: &Path, field: &str) -> Result<PathBuf, String> {
    let canonical_repo = fs::canonicalize(repo_root)
        .map_err(|error| format!("{field} repository root cannot be resolved: {error}"))?;
    let canonical_path = fs::canonicalize(path)
        .map_err(|error| format!("{field} cannot be resolved: {error}"))?;
    if !canonical_path.starts_with(&canonical_repo) {
        return Err(format!("{field} must remain inside the repository root"));
    }
    Ok(canonical_path)
}

fn workspace_path(
    repo_root: &Path,
    value: &str,
    field: &str,
    must_exist: bool,
) -> Result<PathBuf, String> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(format!("{field} must be a non-empty workspace-relative path"));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!("{field} must be a workspace-relative path without '..'"));
    }
    let path = repo_root.join(path);
    if must_exist {
        canonical_workspace_file(&path, repo_root, field)
    } else {
        let parent = path.parent().ok_or_else(|| format!("{field} has no parent directory"))?;
        let canonical_parent = fs::canonicalize(parent)
            .map_err(|error| format!("{field} parent cannot be resolved: {error}"))?;
        let canonical_repo = fs::canonicalize(repo_root)
            .map_err(|error| format!("{field} repository root cannot be resolved: {error}"))?;
        if !canonical_parent.starts_with(&canonical_repo) {
            return Err(format!("{field} must remain inside the repository root"));
        }
        Ok(canonical_parent.join(
            path.file_name()
                .ok_or_else(|| format!("{field} has no file name"))?,
        ))
    }
}

fn validate_rule_id(rule_id: &str) -> Result<(), String> {
    let mut chars = rule_id.chars();
    let Some(first) = chars.next() else {
        return Err("ruleId must be non-empty".into());
    };
    if !first.is_ascii_alphanumeric()
        || !chars.all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
    {
        return Err("ruleId must use ASCII letters, digits, '.', '_' or '-'".into());
    }
    if rule_id.len() > 256 {
        return Err("ruleId is too long".into());
    }
    Ok(())
}

fn reject_empty_identity(field: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(format!("{field} must be non-empty and contain no control characters"));
    }
    Ok(())
}

fn object<'a>(value: &'a Value, field: &str) -> Result<&'a Map<String, Value>, String> {
    value.as_object().ok_or_else(|| format!("{field} must be an object"))
}

fn required_object<'a>(source: &'a Map<String, Value>, field: &str) -> Result<&'a Map<String, Value>, String> {
    object(source.get(field).ok_or_else(|| format!("{field} is required"))?, field)
}

fn required_array<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a Vec<Value>, String> {
    let array = object
        .get(field)
        .ok_or_else(|| format!("{field} is required"))?
        .as_array()
        .ok_or_else(|| format!("{field} must be an array"))?;
    if array.len() > 1024 {
        return Err(format!("{field} exceeds the configured item limit"));
    }
    Ok(array)
}

fn required_string(object: &Map<String, Value>, field: &str) -> Result<String, String> {
    object
        .get(field)
        .ok_or_else(|| format!("{field} is required"))?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("{field} must be a string"))
}

fn optional_string(object: &Map<String, Value>, field: &str) -> Result<Option<String>, String> {
    object
        .get(field)
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{field} must be a string"))
        })
        .transpose()
}

fn required_u64(object: &Map<String, Value>, field: &str) -> Result<u64, String> {
    object
        .get(field)
        .ok_or_else(|| format!("{field} is required"))?
        .as_u64()
        .ok_or_else(|| format!("{field} must be an unsigned integer"))
}

fn required_bool(object: &Map<String, Value>, field: &str) -> Result<bool, String> {
    object
        .get(field)
        .ok_or_else(|| format!("{field} is required"))?
        .as_bool()
        .ok_or_else(|| format!("{field} must be a boolean"))
}

fn expect_string(object: &Map<String, Value>, field: &str, expected: &str) -> Result<(), String> {
    let actual = required_string(object, field)?;
    if actual != expected {
        return Err(format!("{field} must be {expected}"));
    }
    Ok(())
}

fn parse_digest(object: &Map<String, Value>, field: &str) -> Result<[u8; 32], String> {
    parse_digest_value(
        object.get(field).ok_or_else(|| format!("{field} is required"))?,
        field,
    )
}

fn parse_digest_value(value: &Value, field: &str) -> Result<[u8; 32], String> {
    let value = value
        .as_str()
        .ok_or_else(|| format!("{field} must be a sha256 digest string"))?;
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| format!("{field} must use the sha256: prefix"))?;
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(format!("{field} must be lowercase 64-character hexadecimal"));
    }
    let mut digest = [0u8; 32];
    for (index, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        digest[index] = (hex_nibble(chunk[0])? << 4) | hex_nibble(chunk[1])?;
    }
    Ok(digest)
}

fn parse_hex_string(value: String, field: &str) -> Result<Vec<u8>, String> {
    if value.len() > MAX_ENCODED_FIELD_CHARS {
        return Err(format!("{field} exceeds the configured size limit"));
    }
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(format!("{field} must be lowercase even-length hexadecimal"));
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for chunk in value.as_bytes().chunks_exact(2) {
        bytes.push((hex_nibble(chunk[0])? << 4) | hex_nibble(chunk[1])?);
    }
    Ok(bytes)
}

fn hex_nibble(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err("invalid lowercase hexadecimal".into()),
    }
}

fn reject_unknown(object: &Map<String, Value>, allowed: &[&str], field: &str) -> Result<(), String> {
    if let Some(unknown) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{field} contains unsupported field: {unknown}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn delivery_profiles_are_closed_and_explicit() {
        assert_eq!(parse_profile("connected").unwrap(), DeploymentProfile::Connected);
        assert_eq!(parse_profile("enterpriseAirGapped").unwrap(), DeploymentProfile::EnterpriseAirGapped);
        assert_eq!(parse_profile("standaloneDeveloper").unwrap(), DeploymentProfile::StandaloneDeveloper);
        assert!(parse_profile("offline").is_err());
    }

    #[test]
    fn digest_parser_requires_lowercase_sha256() {
        let value = json!("sha256:000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        assert_eq!(parse_digest_value(&value, "digest").unwrap()[0], 0);
        assert!(parse_digest_value(&json!("000102"), "digest").is_err());
        assert!(parse_digest_value(&json!("sha256:000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1E"), "digest").is_err());
    }

    #[test]
    fn descriptor_unknown_fields_are_rejected() {
        let value = json!({"known": true, "future": true});
        let object = value.as_object().unwrap();
        assert!(reject_unknown(object, &["known"], "descriptor").is_err());
    }

    #[test]
    fn workspace_paths_reject_escape_and_allow_existing_file() {
        let root = std::env::temp_dir().join(format!(
            "rf-delivery-parser-{}",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("descriptor.json"), b"{}").unwrap();
        assert!(workspace_path(&root, "../outside", "cacheRoot", false).is_err());
        assert!(canonical_workspace_file(&root.join("descriptor.json"), &root, "descriptor").is_ok());
        fs::remove_dir_all(root).unwrap();
    }
}

/// A publishing-system statement, independently checked against the operator's pin.
#[derive(Debug)]
pub struct RegistryPin {
    pub version: String,
    pub store_digest: String,
    pub provenance: String,
}

fn parse_registry_pin(
    value: &Value,
    rule_id: &str,
    descriptor: &BundleDeliveryDescriptor,
    verification: &BundleDeliveryVerification,
    now: u64,
) -> Result<RegistryPin, String> {
    let fields = object(value, "registryPin")?;
    reject_unknown(fields, &["type", "id", "version", "storeDigest", "bundleDigest", "descriptorDigest", "provenance", "signedMapping"], "registryPin")?;
    expect_string(fields, "type", "rules")?;
    expect_string(fields, "id", rule_id)?;
    let version = required_string(fields, "version")?;
    let provenance = required_string(fields, "provenance")?;
    reject_empty_identity("registryPin.version", &version)?;
    reject_empty_identity("registryPin.provenance", &provenance)?;
    let store_digest = required_string(fields, "storeDigest")?;
    parse_digest(fields, "storeDigest")?;
    if parse_digest(fields, "bundleDigest")? != descriptor.bundle_digest
        || parse_digest(fields, "descriptorDigest")? != descriptor.descriptor_digest() {
        return Err("registryPin does not bind the selected delivery descriptor".into());
    }
    let mut payload = b"erf-registry-delivery-mapping-v1\0".to_vec();
    for field in ["type", "id", "version", "storeDigest", "bundleDigest", "descriptorDigest", "provenance"] {
        let text = required_string(fields, field)?;
        payload.extend_from_slice(&(text.len() as u64).to_be_bytes());
        payload.extend_from_slice(text.as_bytes());
    }
    let asset = parse_signed_asset(required_object(fields, "signedMapping")?, "registryPin.signedMapping")?;
    if asset.payload != payload {
        return Err("registryPin signed mapping payload mismatch".into());
    }
    let metadata = verification.trust_metadata.as_ref().ok_or("registryPin trust metadata missing")?;
    verification.trust_store.verify_with_metadata(&asset, "registry-mapping-signing", metadata, now)
        .map_err(|error| format!("registryPin signature invalid: {error}"))?;
    if verification.revocation_snapshot.as_ref().ok_or("registryPin revocation snapshot missing")?
        .is_asset_revoked(&asset.subject_digest()) {
        return Err("registryPin mapping is revoked".into());
    }
    Ok(RegistryPin { version, store_digest, provenance })
}
