use ed25519_dalek::{Signer, SigningKey};
use erf_bundle_cache::{BundleDeliveryDescriptor, DeploymentProfile, VerificationEvidence};
use erf_packs::{
    OciPackRelease, OciPackVerifier, BACKEND_RUNTIME_PACK_ASSET_TYPE,
    OCI_MANIFEST_MEDIA_TYPE, RUNTIME_PACK_SIGNING_ROLE, WORKSPACE_RESOLVER_PACK_ASSET_TYPE,
};
use erf_rule_bundle::{RuleBundleInputs, RuleBundleManifest, RULE_BUNDLE_SCHEMA_VERSION};
use erf_trust::{ArtifactStatement, RevocationSnapshot, SignedAsset, TrustStore, DSSE_PAYLOAD_TYPE};
use rf_runner::{
    scan, BundleDeliveryConfig, BundleDeliveryVerification, BundlePackKind,
    BundlePackVerification, ExecutionLock, ScanConfig, SignedExecutionLock,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener};
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let host = args.next().ok_or("usage: bundle_delivery_scan HOST RESOLVER")?;
    let resolver = args.next().ok_or("missing RESOLVER")?;
    let root = temp_path("erf-delivery-scan-repo");
    fs::create_dir_all(root.join("src/main/java"))?;
    fs::write(root.join("src/main/java/A.java"), "public class A { void run() {} }\n")?;
    fs::write(root.join("pom.xml"), "<project/>\n")?;

    let payload = canonical_payload(&[
        ("ruleset.xml", b"<ruleset/>".as_slice()),
        ("fixtures/A.java", b"public class A {}\n".as_slice()),
    ]);
    let payload_digest: [u8; 32] = Sha256::digest(&payload).into();
    let pack_key = SigningKey::from_bytes(&[102; 32]);
    let backend_manifest = br#"{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{"mediaType":"application/vnd.erf.runtime-pack.config.v0","digest":"sha256:0808080808080808080808080808080808080808080808080808080808080808","size":1},"layers":[{"mediaType":"application/vnd.erf.runtime-pack.asset.v0","digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1}],"kind":"backendRuntime","platform":"linux/amd64","assetDigest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","compatibilityIdentity":"java21-pmd7","annotations":{"org.rulefactory.kind":"backendRuntime","org.rulefactory.platform":"linux/amd64","org.rulefactory.compatibilityIdentity":"java21-pmd7"}}"#;
    let resolver_manifest = br#"{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{"mediaType":"application/vnd.erf.runtime-pack.config.v0","digest":"sha256:0808080808080808080808080808080808080808080808080808080808080808","size":1},"layers":[{"mediaType":"application/vnd.erf.runtime-pack.asset.v0","digest":"sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","size":1}],"kind":"workspaceResolver","platform":"linux/amd64","assetDigest":"sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","compatibilityIdentity":"java21-pmd7","annotations":{"org.rulefactory.kind":"workspaceResolver","org.rulefactory.platform":"linux/amd64","org.rulefactory.compatibilityIdentity":"java21-pmd7"}}"#;
    let backend_pack = pack_release(backend_manifest, BACKEND_RUNTIME_PACK_ASSET_TYPE, &pack_key);
    let resolver_pack = pack_release(resolver_manifest, WORKSPACE_RESOLVER_PACK_ASSET_TYPE, &pack_key);
    let backend_pack_digest: [u8; 32] = Sha256::digest(backend_manifest).into();
    let resolver_pack_digest: [u8; 32] = Sha256::digest(resolver_manifest).into();
    let mut lock = execution_lock(&hex_digest(&payload_digest));
    lock.backend_pack_digest = hex_digest(&backend_pack_digest);
    lock.resolver_pack_digest = hex_digest(&resolver_pack_digest);
    lock.lock_digest = lock.content_digest();
    let lock_key = SigningKey::from_bytes(&[103; 32]);
    let unsigned_lock_asset = SignedAsset {
        payload_type: DSSE_PAYLOAD_TYPE.into(),
        payload: lock.canonical_payload(),
        key_id: "lock-key".into(),
        signature: Vec::new(),
    };
    let signed_lock = SignedExecutionLock {
        lock: lock.clone(),
        asset: SignedAsset {
            signature: lock_key.sign(&unsigned_lock_asset.signing_bytes()).to_bytes().to_vec(),
            ..unsigned_lock_asset
        },
        role: "execution-lock-signing".into(),
        trust_store: {
            let mut store = TrustStore::default();
            store.insert_key("lock-key", lock_key.verifying_key())?;
            store.authorize_role("execution-lock-signing", "lock-key")?;
            store
        },
        metadata: None,
    };
    let execution_lock_digest: [u8; 32] = hex_bytes(&lock.content_digest())?;
    let inputs = RuleBundleInputs {
        rule_release_digest: [1; 32],
        policy_profile_digest: [2; 32],
        execution_lock_digest,
        ontology_contract_pack_digest: [4; 32],
    };
    let bundle_key = SigningKey::from_bytes(&[101; 32]);
    let manifest = RuleBundleManifest {
        bundle_version: RULE_BUNDLE_SCHEMA_VERSION.into(),
        bundle_digest: payload_digest,
        rule_release_digest: inputs.rule_release_digest,
        policy_profile_digest: inputs.policy_profile_digest,
        execution_lock_digest: inputs.execution_lock_digest,
        ontology_contract_pack_digest: inputs.ontology_contract_pack_digest,
        compatibility_identity: "java21-pmd7".into(),
        platform: "linux/amd64".into(),
    };
    let signed_bundle = manifest.sign_with("bundle-key", |bytes| {
        bundle_key.sign(bytes).to_bytes().to_vec()
    });
    let mut trust_store = TrustStore::default();
    trust_store.insert_key("bundle-key", bundle_key.verifying_key())?;
    trust_store.authorize_role(erf_rule_bundle::RULE_BUNDLE_SIGNING_ROLE, "bundle-key")?;
    trust_store.insert_key("pack-key", pack_key.verifying_key())?;
    trust_store.authorize_role(RUNTIME_PACK_SIGNING_ROLE, "pack-key")?;
    trust_store.insert_key("lock-key", lock_key.verifying_key())?;
    trust_store.authorize_role("execution-lock-signing", "lock-key")?;
    let backend_measurement = OciPackVerifier::backend_runtime()
        .with_compatibility_identity("java21-pmd7")
        .verify_and_measure(
        &backend_pack,
        backend_manifest,
        &trust_store,
        None,
        10,
    )?;
    let resolver_measurement = OciPackVerifier::workspace_resolver()
        .with_compatibility_identity("java21-pmd7")
        .verify_and_measure(
        &resolver_pack,
        resolver_manifest,
        &trust_store,
        None,
        10,
    )?;
    let revocation_snapshot = RevocationSnapshot {
        version: 1,
        issued_at_epoch_secs: 1,
        expires_at_epoch_secs: 100,
        revoked_asset_digests: Default::default(),
    };
    let revocation_snapshot_digest = revocation_snapshot.subject_digest();
    let descriptor = BundleDeliveryDescriptor {
        protocol_version: erf_bundle_cache::DELIVERY_PROTOCOL_VERSION.into(),
        descriptor_version: erf_bundle_cache::DELIVERY_DESCRIPTOR_VERSION.into(),
        bundle_digest: payload_digest,
        bundle_schema_version: RULE_BUNDLE_SCHEMA_VERSION.into(),
        compatibility_identity: "java21-pmd7".into(),
        platform: "linux/amd64".into(),
        rule_release_digest: inputs.rule_release_digest,
        policy_profile_digest: inputs.policy_profile_digest,
        execution_lock_digest: inputs.execution_lock_digest,
        ontology_contract_pack_digest: inputs.ontology_contract_pack_digest,
        runtime_pack_digests: vec![backend_measurement.manifest_digest, resolver_measurement.manifest_digest],
        artifact_statement_digest: backend_measurement.statement_digest,
        revocation_snapshot_digest,
        issued_at_epoch_secs: 1,
        expires_at_epoch_secs: 100,
    };
    let verification = BundleDeliveryVerification {
        signed_bundle,
        manifest_bytes: manifest.canonical_payload(),
        inputs,
        trust_store,
        trust_metadata: None,
        revocation_snapshot: Some(revocation_snapshot),
        revocation_snapshot_digest,
        packs: vec![
            BundlePackVerification {
                kind: BundlePackKind::BackendRuntime,
                release: backend_pack,
                manifest_bytes: backend_manifest.to_vec(),
                manifest_platform: "linux/amd64".into(),
                manifest_compatibility_identity: "java21-pmd7".into(),
                asset_digest: [0xaa; 32],
                oci_layout: None,
            },
            BundlePackVerification {
                kind: BundlePackKind::WorkspaceResolver,
                release: resolver_pack,
                manifest_bytes: resolver_manifest.to_vec(),
                manifest_platform: "linux/amd64".into(),
                manifest_compatibility_identity: "java21-pmd7".into(),
                asset_digest: [0xbb; 32],
                oci_layout: None,
            },
        ],
    };
    let body = delivery_response(&descriptor, &payload);
    let (endpoint, server) = mock_server(body)?;
    let cache_root = temp_path("erf-delivery-scan-cache");
    let result = scan(&ScanConfig {
        repo_root: root.to_string_lossy().into_owned(),
        rule_id: "corp.delivery".into(),
        rule_bundle: None,
        bundle_delivery: Some(BundleDeliveryConfig {
            endpoint,
            descriptor,
            expected_compatibility_identity: "java21-pmd7".into(),
            expected_platform: "linux/amd64".into(),
            cache_root: cache_root.to_string_lossy().into_owned(),
            profile: DeploymentProfile::Connected,
            evidence: VerificationEvidence {
                signature_valid: true,
                trust_metadata_fresh: true,
                revocation_snapshot_fresh: true,
                referenced_packs_available: true,
                trusted_context_valid: true,
            },
            verification: Some(verification),
            now_epoch_secs: 10,
            timeout_ms: 2_000,
        }),
        resolver_command: resolver,
        resolver_args: Vec::new(),
        host_command: "/bin/sh".into(),
        host_args: vec![
            "-c".into(),
            "export ERF_REFERENCE_DELIVERY_BACKEND=1; exec \"$1\"".into(),
            "erf-delivery-host".into(),
            host,
        ],
        host_deadline_ms: 2_000,
        resolver_deadline_ms: 2_000,
        max_staging_bytes: 64 * 1024 * 1024,
        execution_lock: None,
        signed_execution_lock: Some(signed_lock),
        resolver_sandbox_policy: None,
        sandbox_policy: None,
        workspace_attestation: None,
        frozen_plan: None,
        incremental: None,
        scan_cache_dir: None,
        fallback_inputs: None,
        project_scope: None,
    })?;
    server.join().map_err(|_| "delivery mock server failed")?;
    let identity = result
        .bundle_delivery_identity
        .ok_or("Runner did not report delivery identity")?;
    if identity.bundle_digest != format!("sha256:{}", hex_digest(&payload_digest)) {
        return Err("Runner identity did not bind the fetched payload".into());
    }
    println!(
        "delivery-scan=PASS bundleDigest={} descriptorDigest={} planned={} executed={}",
        identity.bundle_digest, identity.descriptor_digest, result.planned, result.executed
    );
    fs::remove_dir_all(root)?;
    fs::remove_dir_all(cache_root)?;
    Ok(())
}

fn pack_release(manifest: &[u8], asset_type: &str, key: &SigningKey) -> OciPackRelease {
    let statement = ArtifactStatement {
        subject_digest: Sha256::digest(manifest).into(),
        asset_type: asset_type.into(),
        schema_version: "oci-pack/v0".into(),
        signing_role: RUNTIME_PACK_SIGNING_ROLE.into(),
        provenance_digest: [9; 32],
        sbom_digest: [10; 32],
        compatibility_identity: "java21-pmd7".into(),
    };
    OciPackRelease {
        manifest_media_type: OCI_MANIFEST_MEDIA_TYPE.into(),
        platform: "linux/amd64".into(),
        artifact_statement: statement.clone(),
        signed_statement: statement.sign_with("pack-key", |bytes| {
            key.sign(bytes).to_bytes().to_vec()
        }),
    }
}

fn execution_lock(bundle_digest: &str) -> ExecutionLock {
    let mut lock = ExecutionLock {
        rule_bundle_digest: bundle_digest.into(),
        runner_release_digest: "b".repeat(64),
        backend_pack_digest: "c".repeat(64),
        resolver_pack_digest: "d".repeat(64),
        sandbox_provider_id: "rootless-oci-docker-v0".into(),
        sandbox_policy_digest: "e".repeat(64),
        resolver_sandbox_provider_id: "rootless-oci-docker-v0".into(),
        resolver_sandbox_policy_digest: "1".repeat(64),
        conformance_suite_digest: "f".repeat(64),
        resolver_id: "erf-fake-resolver".into(),
        resolver_version: "0.1.0".into(),
        backend_id: "pmd-java".into(),
        backend_version: "7.9.0".into(),
        protocol_major: erf_host_client::PROTOCOL_MAJOR,
        canonical_set_version: rf_gate::CANONICAL_SET_VERSION.into(),
        lock_digest: String::new(),
    };
    lock.lock_digest = lock.content_digest();
    lock
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

fn mock_server(body: String) -> Result<(String, thread::JoinHandle<()>), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let address = listener.local_addr()?;
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("delivery client should connect");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let read = stream.read(&mut buffer).expect("delivery request should be readable");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        );
        stream.write_all(response.as_bytes()).expect("delivery response should be writable");
        let _ = stream.shutdown(Shutdown::Both);
    });
    Ok((format!("http://127.0.0.1:{}/v1", address.port()), handle))
}

fn delivery_response(descriptor: &BundleDeliveryDescriptor, bytes: &[u8]) -> String {
    json!({
        "apiVersion": erf_bundle_cache::DELIVERY_PROTOCOL_VERSION,
        "kind": "RuleBundleDelivery",
        "schemaVersion": erf_bundle_cache::DELIVERY_DESCRIPTOR_VERSION,
        "descriptor": {
            "protocolVersion": descriptor.protocol_version,
            "descriptorVersion": descriptor.descriptor_version,
            "bundleDigest": prefixed_digest(&descriptor.bundle_digest),
            "bundleSchemaVersion": descriptor.bundle_schema_version,
            "compatibilityIdentity": descriptor.compatibility_identity,
            "platform": descriptor.platform,
            "ruleReleaseDigest": prefixed_digest(&descriptor.rule_release_digest),
            "policyProfileDigest": prefixed_digest(&descriptor.policy_profile_digest),
            "executionLockDigest": prefixed_digest(&descriptor.execution_lock_digest),
            "ontologyContractPackDigest": prefixed_digest(&descriptor.ontology_contract_pack_digest),
            "runtimePackDigests": descriptor.runtime_pack_digests.iter().map(prefixed_digest).collect::<Vec<_>>(),
            "artifactStatementDigest": prefixed_digest(&descriptor.artifact_statement_digest),
            "revocationSnapshotDigest": prefixed_digest(&descriptor.revocation_snapshot_digest),
            "issuedAtEpochSecs": descriptor.issued_at_epoch_secs,
            "expiresAtEpochSecs": descriptor.expires_at_epoch_secs,
        },
        "bundleBytesBase64": base64(bytes),
        "verification": {
            "signatureValid": true,
            "trustMetadataFresh": true,
            "revocationSnapshotFresh": true,
            "referencedPacksAvailable": true,
            "trustedContextValid": true,
        }
    })
    .to_string()
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[((a & 3) << 4 | b >> 4) as usize] as char);
        output.push(if chunk.len() > 1 { TABLE[((b & 15) << 2 | c >> 6) as usize] as char } else { '=' });
        output.push(if chunk.len() > 2 { TABLE[(c & 63) as usize] as char } else { '=' });
    }
    output
}

fn prefixed_digest(digest: &[u8; 32]) -> String {
    format!("sha256:{}", hex_digest(digest))
}

fn hex_digest(bytes: &[u8; 32]) -> String {
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

fn temp_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after epoch")
            .as_nanos()
    ))
}
