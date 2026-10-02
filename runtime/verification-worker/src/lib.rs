//! Verification Worker boundary for ConformanceEvidence.
//!
//! The worker owns the only production-facing aggregation-and-signing helper:
//! it runs the complete fixed suite, refuses a partial report, binds every
//! supplied immutable identity, and signs with the verification-signing role.
//! A deployment runs this crate in an isolated worker process; Pack Builders
//! submit candidate executable paths and do not receive the signing key.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use erf_conformance::{
    run_acceptance, ConformanceEvidence, EvidenceError, SuiteConfig, SuiteReport,
    TRUSTED_PLATFORM, VERIFICATION_SIGNING_ROLE,
};
use erf_trust::SignedAsset;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const SANDBOX_PROOF_VERSION: &str = "erf-sandbox-proof-v1";
pub const SANDBOX_TRUST_ROOT_ENV: &str = "ERF_SANDBOX_TRUST_PUBLIC_KEY_HEX";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxEnforcement {
    pub rootless: bool,
    pub read_only_root: bool,
    pub network_isolated: bool,
    pub no_new_privileges: bool,
    pub all_capabilities_dropped: bool,
    pub memory_limit_mb: u64,
    pub cpu_limit_millis: u64,
    pub pids_limit: u64,
}

/// Deployment-owned trust anchor for SandboxProof verification. The key is
/// intentionally not constructible from request data or a normal CLI option;
/// production workers load it from their deployment environment, while the
/// fixture-only constructor is available only in the explicit fixture build.
#[derive(Clone)]
pub struct SandboxTrustAnchor {
    verifying_key: VerifyingKey,
}

impl SandboxTrustAnchor {
    pub fn from_deployment_env() -> Result<Self, WorkerError> {
        let value = std::env::var(SANDBOX_TRUST_ROOT_ENV).map_err(|_| {
            WorkerError::SandboxProof(format!(
                "deployment trust root {SANDBOX_TRUST_ROOT_ENV} is missing"
            ))
        })?;
        let bytes = decode_hex(&value)
            .map_err(|error| WorkerError::SandboxProof(format!("deployment trust root is invalid: {error}")))?;
        let key_bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| WorkerError::SandboxProof("deployment trust root must be 32 bytes".into()))?;
        let verifying_key = VerifyingKey::from_bytes(&key_bytes)
            .map_err(|error| WorkerError::SandboxProof(format!("deployment trust root is invalid: {error}")))?;
        Ok(Self { verifying_key })
    }

    #[cfg(feature = "fixture-signer")]
    pub fn fixture() -> Self {
        Self {
            verifying_key: SigningKey::from_bytes(&[2u8; 32]).verifying_key(),
        }
    }
}

/// Provider-signed proof that both candidate processes were prepared with the
/// exact sandbox identities in the request. The Worker trusts only its
/// deployment-configured provider key, never a key supplied by the request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxProof {
    pub version: String,
    pub provider_id: String,
    pub policy_digest: String,
    pub resolver_provider_id: String,
    pub resolver_policy_digest: String,
    pub host_command_digest: String,
    pub resolver_command_digest: String,
    pub platform: String,
    pub measurement_digest: String,
    pub enforcement: SandboxEnforcement,
    pub signature_hex: String,
}

impl SandboxProof {
    pub fn unsigned() -> Self {
        Self {
            version: SANDBOX_PROOF_VERSION.into(),
            provider_id: String::new(),
            policy_digest: String::new(),
            resolver_provider_id: String::new(),
            resolver_policy_digest: String::new(),
            host_command_digest: String::new(),
            resolver_command_digest: String::new(),
            platform: String::new(),
            measurement_digest: String::new(),
            enforcement: SandboxEnforcement {
                rootless: false,
                read_only_root: false,
                network_isolated: false,
                no_new_privileges: false,
                all_capabilities_dropped: false,
                memory_limit_mb: 0,
                cpu_limit_millis: 0,
                pids_limit: 0,
            },
            signature_hex: String::new(),
        }
    }

    #[cfg(feature = "fixture-signer")]
    pub fn fixture(request: &VerificationRequest, key: &SigningKey) -> Self {
        let mut proof = Self {
            version: SANDBOX_PROOF_VERSION.into(),
            provider_id: request.sandbox_provider_id.clone(),
            policy_digest: request.sandbox_policy_digest.clone(),
            resolver_provider_id: request.resolver_sandbox_provider_id.clone(),
            resolver_policy_digest: request.resolver_sandbox_policy_digest.clone(),
            host_command_digest: command_digest(&request.suite.host_program, &request.suite.host_args, &request.suite.host_envs),
            resolver_command_digest: command_digest(&request.suite.resolver_program, &request.suite.resolver_args, &request.suite.resolver_envs),
            platform: request.platform.clone(),
            measurement_digest: digest_text("fixture-sandbox-provider/rootless-oci-v0"),
            enforcement: SandboxEnforcement {
                rootless: true,
                read_only_root: true,
                network_isolated: true,
                no_new_privileges: true,
                all_capabilities_dropped: true,
                memory_limit_mb: 512,
                cpu_limit_millis: 1000,
                pids_limit: 64,
            },
            signature_hex: String::new(),
        };
        proof.signature_hex = hex(&key.sign(&proof.canonical_payload()).to_bytes());
        proof
    }

    fn canonical_payload(&self) -> Vec<u8> {
        let mut bytes = Vec::from(b"erf-sandbox-proof-v1\0".as_slice());
        for value in [
            &self.version,
            &self.provider_id,
            &self.policy_digest,
            &self.resolver_provider_id,
            &self.resolver_policy_digest,
            &self.host_command_digest,
            &self.resolver_command_digest,
            &self.platform,
            &self.measurement_digest,
        ] {
            append_string(&mut bytes, value);
        }
        for value in [
            self.enforcement.rootless,
            self.enforcement.read_only_root,
            self.enforcement.network_isolated,
            self.enforcement.no_new_privileges,
            self.enforcement.all_capabilities_dropped,
        ] {
            bytes.push(u8::from(value));
        }
        for value in [
            self.enforcement.memory_limit_mb,
            self.enforcement.cpu_limit_millis,
            self.enforcement.pids_limit,
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes
    }

    fn verify(&self, request: &VerificationRequest, trust_anchor: &SandboxTrustAnchor) -> Result<(), WorkerError> {
        if self.version != SANDBOX_PROOF_VERSION
            || self.provider_id != request.sandbox_provider_id
            || self.policy_digest != request.sandbox_policy_digest
            || self.resolver_provider_id != request.resolver_sandbox_provider_id
            || self.resolver_policy_digest != request.resolver_sandbox_policy_digest
            || self.platform != request.platform
        {
            return Err(WorkerError::SandboxProof("sandbox proof identity does not match the request".into()));
        }
        let expected_host = command_digest(&request.suite.host_program, &request.suite.host_args, &request.suite.host_envs);
        let expected_resolver = command_digest(&request.suite.resolver_program, &request.suite.resolver_args, &request.suite.resolver_envs);
        if self.host_command_digest != expected_host || self.resolver_command_digest != expected_resolver {
            return Err(WorkerError::SandboxProof("sandbox proof command identity does not match the suite".into()));
        }
        if !is_digest(&self.policy_digest)
            || !is_digest(&self.resolver_policy_digest)
            || !is_digest(&self.host_command_digest)
            || !is_digest(&self.resolver_command_digest)
            || !is_digest(&self.measurement_digest)
        {
            return Err(WorkerError::SandboxProof("sandbox proof contains an invalid digest".into()));
        }
        let enforcement = &self.enforcement;
        if !(enforcement.rootless
            && enforcement.read_only_root
            && enforcement.network_isolated
            && enforcement.no_new_privileges
            && enforcement.all_capabilities_dropped
            && enforcement.memory_limit_mb > 0
            && enforcement.cpu_limit_millis > 0
            && enforcement.pids_limit > 0)
        {
            return Err(WorkerError::SandboxProof("sandbox proof does not attest the required enforcement controls".into()));
        }
        let signature_bytes = decode_hex(&self.signature_hex)
            .map_err(|error| WorkerError::SandboxProof(format!("sandbox proof signature is invalid: {error}")))?;
        let signature = Signature::from_slice(&signature_bytes)
            .map_err(|error| WorkerError::SandboxProof(format!("sandbox proof signature is invalid: {error}")))?;
        trust_anchor
            .verifying_key
            .verify(&self.canonical_payload(), &signature)
            .map_err(|error| WorkerError::SandboxProof(format!("sandbox proof signature verification failed: {error}")))
    }
}

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("conformance suite failed: {0}/{1} checks passed")]
    SuiteFailed(usize, usize),
    #[error("conformance evidence rejected: {0}")]
    Evidence(#[from] EvidenceError),
    #[error("sandbox proof rejected: {0}")]
    SandboxProof(String),
}

#[derive(Clone)]
pub struct VerificationRequest {
    pub suite: SuiteConfig,
    pub suite_release_digest: String,
    pub verification_worker_release_digest: String,
    pub runner_release_digest: String,
    pub backend_pack_digest: String,
    pub resolver_pack_digest: String,
    pub sandbox_provider_id: String,
    pub sandbox_policy_digest: String,
    pub resolver_sandbox_provider_id: String,
    pub resolver_sandbox_policy_digest: String,
    pub platform: String,
    pub sandbox_proof: SandboxProof,
}

#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub report: SuiteReport,
    pub evidence: ConformanceEvidence,
    pub signed_evidence: SignedAsset,
}

/// Run the complete suite and sign the resulting evidence inside the Worker
/// boundary. The caller supplies only the worker-held signer closure.
pub fn verify_and_sign(
    request: &VerificationRequest,
    sandbox_trust_anchor: &SandboxTrustAnchor,
    key_id: impl Into<String>,
    signer: impl FnOnce(&[u8]) -> Vec<u8>,
) -> Result<VerificationResult, WorkerError> {
    request.sandbox_proof.verify(request, sandbox_trust_anchor)?;
    let report = run_acceptance(&request.suite);
    let passed = report.checks.iter().filter(|check| check.passed).count();
    if passed != report.checks.len() {
        return Err(WorkerError::SuiteFailed(passed, report.checks.len()));
    }
    let evidence = ConformanceEvidence::from_report(
        &report,
        request.suite_release_digest.clone(),
        request.verification_worker_release_digest.clone(),
        request.runner_release_digest.clone(),
        request.backend_pack_digest.clone(),
        request.resolver_pack_digest.clone(),
        request.sandbox_provider_id.clone(),
        request.sandbox_policy_digest.clone(),
        request.resolver_sandbox_provider_id.clone(),
        request.resolver_sandbox_policy_digest.clone(),
        request.platform.clone(),
    )?;
    let signed_evidence = sign_verified_evidence(&evidence, key_id, signer)?;
    Ok(VerificationResult {
        report,
        evidence,
        signed_evidence,
    })
}

/// The signing operation is intentionally kept in this crate so application
/// code only receives an already-issued verification asset.
pub fn sign_verified_evidence(
    evidence: &ConformanceEvidence,
    key_id: impl Into<String>,
    signer: impl FnOnce(&[u8]) -> Vec<u8>,
) -> Result<SignedAsset, WorkerError> {
    let payload = evidence.canonical_payload();
    Ok(SignedAsset {
        payload_type: erf_trust::DSSE_PAYLOAD_TYPE.into(),
        payload: payload.clone(),
        key_id: key_id.into(),
        signature: signer(&erf_trust::pae(erf_trust::DSSE_PAYLOAD_TYPE, &payload)),
    })
}

/// Small deterministic signer used by the standalone fixture binary. Real
/// deployments replace it with an HSM/workload-identity signer.
pub fn sign_with_key(
    evidence: &ConformanceEvidence,
    key: &SigningKey,
    key_id: &str,
) -> Result<SignedAsset, WorkerError> {
    sign_verified_evidence(evidence, key_id, |bytes| {
        key.sign(bytes).to_bytes().to_vec()
    })
}

pub fn default_platform() -> &'static str {
    TRUSTED_PLATFORM
}

pub fn signing_role() -> &'static str {
    VERIFICATION_SIGNING_ROLE
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if value.len() % 2 != 0 {
        return Err("hex value has odd length".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).map_err(|_| "invalid hex value".to_string()))
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn append_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(feature = "fixture-signer")]
fn digest_text(value: &str) -> String {
    hex(&Sha256::digest(value.as_bytes()))
}

fn command_digest(program: &str, args: &[String], envs: &[(String, String)]) -> String {
    let mut bytes = Vec::from(b"erf-verification-command-v1\0".as_slice());
    append_string(&mut bytes, program);
    bytes.extend_from_slice(&(args.len() as u64).to_be_bytes());
    for arg in args {
        append_string(&mut bytes, arg);
    }
    bytes.extend_from_slice(&(envs.len() as u64).to_be_bytes());
    for (key, value) in envs {
        append_string(&mut bytes, key);
        append_string(&mut bytes, value);
    }
    hex(&Sha256::digest(bytes))
}
