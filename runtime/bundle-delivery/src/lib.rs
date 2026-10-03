//! Bounded Server-to-Runner RuleBundle delivery.
//!
//! The client is deliberately small and transport-specific only at the HTTP
//! edge: it accepts explicit `http://host:port/base` endpoints, bounds every
//! response, validates the closed delivery descriptor, verifies the received
//! bytes while they are decoded, and writes through the immutable cache. It
//! does not grant Gate or SCM authority.

use erf_bundle_cache::{
    resolve_cached, BundleCache, BundleDeliveryDescriptor, CacheError, DeploymentProfile,
    DeliveryError, ResolvedBundle, VerificationEvidence, DELIVERY_PROTOCOL_VERSION,
    DELIVERY_DESCRIPTOR_VERSION, MAX_BUNDLE_BYTES,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use thiserror::Error;

/// The server response is JSON containing a base64 representation of at most
/// `MAX_BUNDLE_BYTES`. The extra allowance covers the closed descriptor and
/// HTTP/JSON framing; it is still a hard upper bound, not a sizing hint.
pub const MAX_RESPONSE_BYTES: usize = ((MAX_BUNDLE_BYTES / 3) + 1) * 4 + 1024 * 1024;
pub const MAX_SINGLE_FLIGHTS: usize = 1024;
const MAX_HEADER_BYTES: usize = 64 * 1024;
const API_VERSION: &str = DELIVERY_PROTOCOL_VERSION;

#[derive(Debug, Clone)]
pub struct DeliveryClient {
    endpoint: Endpoint,
    timeout: Duration,
}

impl DeliveryClient {
    pub fn new(endpoint: &str, timeout: Duration) -> Result<Self, BundleDeliveryError> {
        if timeout.is_zero() {
            return Err(BundleDeliveryError::InvalidEndpoint(
                "delivery timeout must be positive".into(),
            ));
        }
        Ok(Self { endpoint: Endpoint::parse(endpoint)?, timeout })
    }

    fn flight_identity(&self) -> String {
        format!("{}:{}{}", self.endpoint.host_header, self.endpoint.port, self.endpoint.path)
    }

    pub fn fetch(
        &self,
        requested: &BundleDeliveryDescriptor,
    ) -> Result<FetchedDelivery, BundleDeliveryError> {
        requested.validate()?;
        let digest = prefixed_digest(&requested.bundle_digest);
        let path = format!(
            "{}/rule-bundles/{digest}/delivery",
            self.endpoint.path.trim_end_matches('/')
        );
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nAccept: application/json\r\nConnection: close\r\nX-Request-Id: {digest}\r\n\r\n",
            host = self.endpoint.host_header,
        );
        let mut stream = connect(&self.endpoint, self.timeout)?;
        stream
            .write_all(request.as_bytes())
            .map_err(|error| BundleDeliveryError::Transport(format!("request write failed: {error}")))?;
        stream.shutdown(Shutdown::Write).map_err(|error| {
            BundleDeliveryError::Transport(format!("request shutdown failed: {error}"))
        })?;
        let response = read_response(&mut stream, self.timeout)?;
        parse_delivery_response(&response, requested)
    }

    /// Resolve from the cache first. Only a Connected cache miss may perform
    /// a fetch; all other failures remain explicit and fail closed.
    pub fn resolve<C: BundleCache>(
        &self,
        cache: &C,
        descriptor: &BundleDeliveryDescriptor,
        profile: DeploymentProfile,
        evidence: VerificationEvidence,
        now_epoch_secs: u64,
    ) -> Result<ResolvedBundle, BundleDeliveryError> {
        match resolve_cached(cache, descriptor, profile, evidence, now_epoch_secs) {
            Ok(bundle) => Ok(bundle),
            Err(DeliveryError::ServerFetchRequired) => {
                let fetched = self.fetch(descriptor)?;
                if fetched.descriptor != *descriptor {
                    return Err(BundleDeliveryError::DescriptorMismatch);
                }
                if let Some(code) = fetched.evidence.first_failure() {
                    return Err(BundleDeliveryError::Delivery(
                        DeliveryError::Inconclusive(code),
                    ));
                }
                cache.put(&fetched.descriptor, &fetched.bytes)?;
                resolve_cached(
                    cache,
                    &fetched.descriptor,
                    profile,
                    fetched.evidence,
                    now_epoch_secs,
                )
                .map_err(BundleDeliveryError::Delivery)
            }
            Err(error) => Err(BundleDeliveryError::Delivery(error)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedDelivery {
    pub descriptor: BundleDeliveryDescriptor,
    pub bytes: Vec<u8>,
    pub evidence: VerificationEvidence,
}

/// Merge concurrent cold requests for one digest. A failed flight is removed
/// after publication so a later caller may retry; successful bytes remain
/// immutable in the cache and therefore make retries cheap.
#[derive(Debug, Clone, Default)]
pub struct SingleFlightDelivery {
    in_flight: Arc<Mutex<HashMap<String, Arc<OnceLock<Result<ResolvedBundle, BundleDeliveryError>>>>>>,
}

impl SingleFlightDelivery {
    pub fn resolve<C: BundleCache>(
        &self,
        client: &DeliveryClient,
        cache: &C,
        cache_identity: &str,
        descriptor: &BundleDeliveryDescriptor,
        profile: DeploymentProfile,
        evidence: VerificationEvidence,
        now_epoch_secs: u64,
    ) -> Result<ResolvedBundle, BundleDeliveryError> {
        let key = format!(
            "{}|{}|{}|{:?}",
            client.flight_identity(),
            cache_identity,
            prefixed_digest(&descriptor.descriptor_digest()),
            profile
        );
        match resolve_cached(cache, descriptor, profile, evidence, now_epoch_secs) {
            Ok(bundle) => return Ok(bundle),
            Err(DeliveryError::ServerFetchRequired) => {}
            Err(error) => return Err(BundleDeliveryError::Delivery(error)),
        }
        let (cell, leader) = {
            let mut flights = self
                .in_flight
                .lock()
                .map_err(|_| BundleDeliveryError::Concurrency("single-flight lock poisoned"))?;
            match flights.get(&key) {
                Some(cell) => (Arc::clone(cell), false),
                None => {
                    if flights.len() >= MAX_SINGLE_FLIGHTS {
                        return Err(BundleDeliveryError::Capacity);
                    }
                    let cell = Arc::new(OnceLock::new());
                    flights.insert(key.clone(), Arc::clone(&cell));
                    (cell, true)
                }
            }
        };
        let result = cell
            .get_or_init(|| client.resolve(cache, descriptor, profile, evidence, now_epoch_secs))
            .clone();
        if leader {
            if let Ok(mut flights) = self.in_flight.lock() {
                flights.remove(&key);
            }
        }
        match result {
            Ok(_) => resolve_cached(cache, descriptor, profile, evidence, now_epoch_secs)
                .map_err(BundleDeliveryError::Delivery),
            Err(error) => Err(error),
        }
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum BundleDeliveryError {
    #[error("invalid delivery endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("delivery transport failed: {0}")]
    Transport(String),
    #[error("delivery response is too large")]
    ResponseTooLarge,
    #[error("delivery response is invalid: {0}")]
    InvalidResponse(String),
    #[error("server rejected delivery request with HTTP {status} ({code}): {message}")]
    Server { status: u16, code: String, message: String },
    #[error("delivery descriptor does not match the requested identity")]
    DescriptorMismatch,
    #[error("delivery resolution failed: {0}")]
    Delivery(#[from] DeliveryError),
    #[error("delivery cache write failed: {0}")]
    Cache(#[from] CacheError),
    #[error("delivery concurrency control failed: {0}")]
    Concurrency(&'static str),
    #[error("delivery single-flight capacity is exhausted")]
    Capacity,
}

#[derive(Debug, Clone)]
struct Endpoint {
    host_header: String,
    host: String,
    port: u16,
    path: String,
}

impl Endpoint {
    fn parse(value: &str) -> Result<Self, BundleDeliveryError> {
        let rest = value.strip_prefix("http://").ok_or_else(|| {
            BundleDeliveryError::InvalidEndpoint(
                "only explicit http:// endpoints are supported by this local seam".into(),
            )
        })?;
        if rest.is_empty() || rest.chars().any(|character| character.is_control() || character.is_whitespace()) {
            return Err(BundleDeliveryError::InvalidEndpoint(
                "endpoint authority is empty or contains whitespace".into(),
            ));
        }
        let authority_end = rest.find('/').unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        let suffix = &rest[authority_end..];
        if authority.is_empty() || authority.contains('@') || authority.contains('?') || authority.contains('#') {
            return Err(BundleDeliveryError::InvalidEndpoint(
                "endpoint must not contain credentials, query, or fragment".into(),
            ));
        }
        let (host, port) = parse_authority(authority)?;
        if suffix.contains('?') || suffix.contains('#') || suffix.contains("//") {
            return Err(BundleDeliveryError::InvalidEndpoint(
                "endpoint path must be a single base path".into(),
            ));
        }
        Ok(Self {
            host_header: authority.into(),
            host,
            port,
            path: if suffix.is_empty() { "/".into() } else { suffix.into() },
        })
    }
}

fn parse_authority(authority: &str) -> Result<(String, u16), BundleDeliveryError> {
    if let Some(rest) = authority.strip_prefix('[') {
        let end = rest.find(']').ok_or_else(|| {
            BundleDeliveryError::InvalidEndpoint("IPv6 endpoint is missing ]".into())
        })?;
        let host = &rest[..end];
        let port = rest[end + 1..]
            .strip_prefix(':')
            .ok_or_else(|| {
                BundleDeliveryError::InvalidEndpoint("IPv6 endpoint must include a port".into())
            })?
            .parse::<u16>()
            .map_err(|_| BundleDeliveryError::InvalidEndpoint("endpoint port is invalid".into()))?;
        if host.is_empty() || port == 0 {
            return Err(BundleDeliveryError::InvalidEndpoint(
                "endpoint host or port is invalid".into(),
            ));
        }
        return Ok((host.into(), port));
    }
    if authority.matches(':').count() > 1 {
        return Err(BundleDeliveryError::InvalidEndpoint(
            "IPv6 endpoints must use brackets".into(),
        ));
    }
    let (host, port) = authority.rsplit_once(':').ok_or_else(|| {
        BundleDeliveryError::InvalidEndpoint("endpoint must include an explicit port".into())
    })?;
    let port = port
        .parse::<u16>()
        .map_err(|_| BundleDeliveryError::InvalidEndpoint("endpoint port is invalid".into()))?;
    if host.is_empty() || port == 0 {
        return Err(BundleDeliveryError::InvalidEndpoint(
            "endpoint host or port is invalid".into(),
        ));
    }
    Ok((host.into(), port))
}

fn connect(endpoint: &Endpoint, timeout: Duration) -> Result<TcpStream, BundleDeliveryError> {
    let addresses = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .map_err(|error| BundleDeliveryError::Transport(format!("endpoint lookup failed: {error}")))?;
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => {
                stream
                    .set_read_timeout(Some(timeout))
                    .and_then(|_| stream.set_write_timeout(Some(timeout)))
                    .map_err(|error| {
                        BundleDeliveryError::Transport(format!("socket timeout setup failed: {error}"))
                    })?;
                return Ok(stream);
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(BundleDeliveryError::Transport(format!(
        "endpoint connection failed: {}",
        last_error.map(|error| error.to_string()).unwrap_or_else(|| "no addresses resolved".into())
    )))
}

fn read_response(stream: &mut TcpStream, timeout: Duration) -> Result<Vec<u8>, BundleDeliveryError> {
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(BundleDeliveryError::Transport("response deadline exceeded".into()));
        }
        stream.set_read_timeout(Some(remaining)).map_err(|error| {
            BundleDeliveryError::Transport(format!("socket timeout setup failed: {error}"))
        })?;
        let read = stream
            .read(&mut buffer)
            .map_err(|error| BundleDeliveryError::Transport(format!("response read failed: {error}")))?;
        if read == 0 {
            break;
        }
        if bytes.len().saturating_add(read) > MAX_RESPONSE_BYTES {
            return Err(BundleDeliveryError::ResponseTooLarge);
        }
        bytes.extend_from_slice(&buffer[..read]);
        if Instant::now() >= deadline {
            return Err(BundleDeliveryError::Transport("response deadline exceeded".into()));
        }
    }
    if bytes.is_empty() {
        return Err(BundleDeliveryError::InvalidResponse("server returned an empty response".into()));
    }
    Ok(bytes)
}

fn parse_delivery_response(
    bytes: &[u8],
    requested: &BundleDeliveryDescriptor,
) -> Result<FetchedDelivery, BundleDeliveryError> {
    let separator = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| BundleDeliveryError::InvalidResponse("missing HTTP header boundary".into()))?;
    if separator > MAX_HEADER_BYTES {
        return Err(BundleDeliveryError::InvalidResponse("HTTP headers exceed the limit".into()));
    }
    let header = std::str::from_utf8(&bytes[..separator])
        .map_err(|_| BundleDeliveryError::InvalidResponse("HTTP headers are not UTF-8".into()))?;
    if header.lines().any(|line| line.to_ascii_lowercase().starts_with("transfer-encoding:")) {
        return Err(BundleDeliveryError::InvalidResponse("chunked responses are not supported".into()));
    }
    let status = header
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| BundleDeliveryError::InvalidResponse("HTTP response status is invalid".into()))?;
    let content_length = header.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length").then(|| value.trim())
    });
    let content_length = content_length
        .ok_or_else(|| BundleDeliveryError::InvalidResponse("Content-Length is required".into()))?
        .parse::<usize>()
        .map_err(|_| BundleDeliveryError::InvalidResponse("Content-Length is invalid".into()))?;
    if content_length > MAX_RESPONSE_BYTES {
        return Err(BundleDeliveryError::ResponseTooLarge);
    }
    let body = &bytes[separator + 4..];
    if body.len() != content_length {
        return Err(BundleDeliveryError::InvalidResponse("Content-Length does not match the response".into()));
    }
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| BundleDeliveryError::InvalidResponse(format!("response JSON is invalid: {error}")))?;
    parse_delivery_json(status, &value, requested)
}

/// Parse the same delivery wire contract after an approved transport fetched JSON.
pub fn parse_delivery_json(
    status: u16,
    value: &Value,
    requested: &BundleDeliveryDescriptor,
) -> Result<FetchedDelivery, BundleDeliveryError> {
    let object = value
        .as_object()
        .ok_or_else(|| BundleDeliveryError::InvalidResponse("response must be a JSON object".into()))?;
    if object.get("apiVersion").and_then(Value::as_str) != Some(API_VERSION) {
        return Err(BundleDeliveryError::InvalidResponse("response apiVersion is unsupported".into()));
    }
    if !(200..300).contains(&status) {
        let code = object.get("code").and_then(Value::as_str).unwrap_or("SERVER_ERROR");
        let message = object.get("message").and_then(Value::as_str).unwrap_or("server rejected delivery");
        return Err(BundleDeliveryError::Server { status, code: code.into(), message: message.into() });
    }
    if status != 200 {
        return Err(BundleDeliveryError::InvalidResponse("delivery success status must be 200".into()));
    }
    if object.get("kind").and_then(Value::as_str) != Some("RuleBundleDelivery")
        || object.get("schemaVersion").and_then(Value::as_str) != Some(DELIVERY_DESCRIPTOR_VERSION)
    {
        return Err(BundleDeliveryError::InvalidResponse("delivery response kind or schema is unsupported".into()));
    }
    let descriptor = parse_descriptor(object.get("descriptor"))?;
    if descriptor != *requested {
        return Err(BundleDeliveryError::DescriptorMismatch);
    }
    let (bytes, actual) = decode_base64(
        object
            .get("bundleBytesBase64")
            .and_then(Value::as_str)
            .ok_or_else(|| BundleDeliveryError::InvalidResponse("bundleBytesBase64 is missing".into()))?,
    )?;
    if bytes.is_empty() || bytes.len() > MAX_BUNDLE_BYTES {
        return Err(BundleDeliveryError::InvalidResponse("bundle bytes are empty or too large".into()));
    }
    if actual != descriptor.bundle_digest {
        return Err(BundleDeliveryError::InvalidResponse("bundle bytes digest does not match descriptor".into()));
    }
    let evidence = parse_evidence(object.get("verification"))?;
    Ok(FetchedDelivery { descriptor, bytes, evidence })
}

fn parse_descriptor(value: Option<&Value>) -> Result<BundleDeliveryDescriptor, BundleDeliveryError> {
    let object = value
        .and_then(Value::as_object)
        .ok_or_else(|| BundleDeliveryError::InvalidResponse("descriptor is missing or not an object".into()))?;
    let descriptor = BundleDeliveryDescriptor {
        protocol_version: required_text(object, "protocolVersion")?,
        descriptor_version: required_text(object, "descriptorVersion")?,
        bundle_digest: required_digest(object, "bundleDigest")?,
        bundle_schema_version: required_text(object, "bundleSchemaVersion")?,
        compatibility_identity: required_text(object, "compatibilityIdentity")?,
        platform: required_text(object, "platform")?,
        rule_release_digest: required_digest(object, "ruleReleaseDigest")?,
        policy_profile_digest: required_digest(object, "policyProfileDigest")?,
        execution_lock_digest: required_digest(object, "executionLockDigest")?,
        ontology_contract_pack_digest: required_digest(object, "ontologyContractPackDigest")?,
        runtime_pack_digests: object
            .get("runtimePackDigests")
            .and_then(Value::as_array)
            .ok_or_else(|| BundleDeliveryError::InvalidResponse("runtimePackDigests is missing".into()))?
            .iter()
            .map(|value| {
                let text = value.as_str().ok_or_else(|| {
                    BundleDeliveryError::InvalidResponse("runtimePackDigests contains a non-text value".into())
                })?;
                parse_digest(text, "runtimePackDigests")
            })
            .collect::<Result<Vec<_>, _>>()?,
        artifact_statement_digest: required_digest(object, "artifactStatementDigest")?,
        revocation_snapshot_digest: required_digest(object, "revocationSnapshotDigest")?,
        issued_at_epoch_secs: required_u64(object, "issuedAtEpochSecs")?,
        expires_at_epoch_secs: required_u64(object, "expiresAtEpochSecs")?,
    };
    descriptor.validate()?;
    Ok(descriptor)
}

fn parse_evidence(value: Option<&Value>) -> Result<VerificationEvidence, BundleDeliveryError> {
    let object = value
        .and_then(Value::as_object)
        .ok_or_else(|| BundleDeliveryError::InvalidResponse("verification is missing or not an object".into()))?;
    Ok(VerificationEvidence {
        signature_valid: required_bool(object, "signatureValid")?,
        trust_metadata_fresh: required_bool(object, "trustMetadataFresh")?,
        revocation_snapshot_fresh: required_bool(object, "revocationSnapshotFresh")?,
        referenced_packs_available: required_bool(object, "referencedPacksAvailable")?,
        trusted_context_valid: required_bool(object, "trustedContextValid")?,
    })
}

fn required_text(object: &Map<String, Value>, name: &str) -> Result<String, BundleDeliveryError> {
    object
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(|| BundleDeliveryError::InvalidResponse(format!("{name} is missing or invalid")))
}

fn required_digest(object: &Map<String, Value>, name: &str) -> Result<[u8; 32], BundleDeliveryError> {
    let value = required_text(object, name)?;
    parse_digest(&value, name)
}

fn parse_digest(value: &str, field: &str) -> Result<[u8; 32], BundleDeliveryError> {
    let hex = value.strip_prefix("sha256:").ok_or_else(|| {
        BundleDeliveryError::InvalidResponse(format!("{field} is not a SHA-256 digest"))
    })?;
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(BundleDeliveryError::InvalidResponse(format!("{field} is not a lowercase SHA-256 digest")));
    }
    let mut digest = [0_u8; 32];
    for (index, slot) in digest.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| BundleDeliveryError::InvalidResponse(format!("{field} is not hexadecimal")))?;
    }
    Ok(digest)
}

fn required_u64(object: &Map<String, Value>, name: &str) -> Result<u64, BundleDeliveryError> {
    object
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| BundleDeliveryError::InvalidResponse(format!("{name} is missing or invalid")))
}

fn required_bool(object: &Map<String, Value>, name: &str) -> Result<bool, BundleDeliveryError> {
    object
        .get(name)
        .and_then(Value::as_bool)
        .ok_or_else(|| BundleDeliveryError::InvalidResponse(format!("{name} is missing or invalid")))
}

fn decode_base64(value: &str) -> Result<(Vec<u8>, [u8; 32]), BundleDeliveryError> {
    if value.len() % 4 != 0 {
        return Err(BundleDeliveryError::InvalidResponse("bundleBytesBase64 has invalid padding".into()));
    }
    let mut output = Vec::with_capacity(value.len() / 4 * 3);
    let mut digest = Sha256::new();
    for (index, chunk) in value.as_bytes().chunks_exact(4).enumerate() {
        let last = index + 1 == value.len() / 4;
        let pad = usize::from(chunk[3] == b'=') + usize::from(chunk[2] == b'=');
        if !last && pad != 0 || pad == 1 && chunk[2] == b'=' {
            return Err(BundleDeliveryError::InvalidResponse("bundleBytesBase64 has invalid padding".into()));
        }
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c = if chunk[2] == b'=' { 0 } else { base64_value(chunk[2])? };
        let d = if chunk[3] == b'=' { 0 } else { base64_value(chunk[3])? };
        let start = output.len();
        output.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            output.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            output.push((c << 6) | d);
        }
        digest.update(&output[start..]);
        if output.len() > MAX_BUNDLE_BYTES {
            return Err(BundleDeliveryError::ResponseTooLarge);
        }
    }
    Ok((output, digest.finalize().into()))
}

fn base64_value(value: u8) -> Result<u8, BundleDeliveryError> {
    match value {
        b'A'..=b'Z' => Ok(value - b'A'),
        b'a'..=b'z' => Ok(value - b'a' + 26),
        b'0'..=b'9' => Ok(value - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(BundleDeliveryError::InvalidResponse("bundleBytesBase64 contains an invalid character".into())),
    }
}

fn prefixed_digest(digest: &[u8; 32]) -> String {
    format!("sha256:{}", digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erf_bundle_cache::{BundleCache, DELIVERY_PROTOCOL_VERSION, DELIVERY_DESCRIPTOR_VERSION};
    use std::fs;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    fn digest(byte: u8) -> [u8; 32] { [byte; 32] }

    fn descriptor(bytes: &[u8]) -> BundleDeliveryDescriptor {
        BundleDeliveryDescriptor {
            protocol_version: DELIVERY_PROTOCOL_VERSION.into(),
            descriptor_version: DELIVERY_DESCRIPTOR_VERSION.into(),
            bundle_digest: Sha256::digest(bytes).into(),
            bundle_schema_version: "rule-bundle-v0".into(),
            compatibility_identity: "java21-pmd7".into(),
            platform: "linux/amd64".into(),
            rule_release_digest: digest(1),
            policy_profile_digest: digest(2),
            execution_lock_digest: digest(3),
            ontology_contract_pack_digest: digest(4),
            runtime_pack_digests: vec![digest(5)],
            artifact_statement_digest: digest(6),
            revocation_snapshot_digest: digest(7),
            issued_at_epoch_secs: 10,
            expires_at_epoch_secs: 100,
        }
    }

    fn evidence() -> VerificationEvidence {
        VerificationEvidence {
            signature_valid: true,
            trust_metadata_fresh: true,
            revocation_snapshot_fresh: true,
            referenced_packs_available: true,
            trusted_context_valid: true,
        }
    }

    fn encode_base64(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut output = String::new();
        for chunk in bytes.chunks(3) {
            let a = chunk[0];
            let b = *chunk.get(1).unwrap_or(&0);
            let c = *chunk.get(2).unwrap_or(&0);
            output.push(TABLE[(a >> 2) as usize] as char);
            output.push(TABLE[((a & 0x03) << 4 | b >> 4) as usize] as char);
            output.push(if chunk.len() > 1 { TABLE[((b & 0x0f) << 2 | c >> 6) as usize] as char } else { '=' });
            output.push(if chunk.len() > 2 { TABLE[(c & 0x3f) as usize] as char } else { '=' });
        }
        output
    }

    fn response(descriptor: &BundleDeliveryDescriptor, bytes: &[u8], facts: VerificationEvidence) -> String {
        let descriptor_json = serde_json::json!({
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
        });
        serde_json::json!({
            "apiVersion": API_VERSION,
            "kind": "RuleBundleDelivery",
            "schemaVersion": DELIVERY_DESCRIPTOR_VERSION,
            "descriptor": descriptor_json,
            "bundleBytesBase64": encode_base64(bytes),
            "verification": {
                "signatureValid": facts.signature_valid,
                "trustMetadataFresh": facts.trust_metadata_fresh,
                "revocationSnapshotFresh": facts.revocation_snapshot_fresh,
                "referencedPacksAvailable": facts.referenced_packs_available,
                "trustedContextValid": facts.trusted_context_valid,
            }
        }).to_string()
    }

    fn server(body: String, calls: Arc<AtomicUsize>, max_wait: Duration) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + max_wait;
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        calls.fetch_add(1, Ordering::SeqCst);
                        let mut request = Vec::new();
                        let mut buffer = [0_u8; 1024];
                        loop {
                            let read = stream.read(&mut buffer).unwrap();
                            if read == 0 { break; }
                            request.extend_from_slice(&buffer[..read]);
                            if request.windows(4).any(|window| window == b"\r\n\r\n") { break; }
                        }
                        let response = wire_response(200, &body);
                        stream.write_all(response.as_bytes()).unwrap();
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(5)),
                    Err(error) => panic!("mock server accept failed: {error}"),
                }
            }
        });
        (format!("http://127.0.0.1:{}/v1", address.port()), handle)
    }

    fn wire_response(status: u16, body: &str) -> String {
        format!(
            "HTTP/1.1 {status} TEST\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn connected_cold_fetch_then_warm_restart_hit_is_deterministic() {
        let bytes = b"signed-rule-bundle-v1";
        let descriptor = descriptor(bytes);
        let body = response(&descriptor, bytes, evidence());
        let calls = Arc::new(AtomicUsize::new(0));
        let (endpoint, handle) = server(body, Arc::clone(&calls), Duration::from_secs(2));
        let root = std::env::temp_dir().join(format!("erf-delivery-{}", unique_suffix()));
        let cache = erf_bundle_cache::FileBundleCache::new(&root);
        let client = DeliveryClient::new(&endpoint, Duration::from_secs(2)).unwrap();
        let first = client.resolve(&cache, &descriptor, DeploymentProfile::Connected, evidence(), 20).unwrap();
        let second = DeliveryClient::new(&endpoint, Duration::from_secs(2)).unwrap()
            .resolve(&erf_bundle_cache::FileBundleCache::new(&root), &descriptor, DeploymentProfile::Connected, evidence(), 20)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        handle.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn offline_hit_reuses_only_verified_bytes_and_missing_is_inconclusive() {
        let bytes = b"signed-rule-bundle-v2";
        let descriptor = descriptor(bytes);
        let root = std::env::temp_dir().join(format!("erf-delivery-offline-{}", unique_suffix()));
        let cache = erf_bundle_cache::FileBundleCache::new(&root);
        cache.put(&descriptor, bytes).unwrap();
        let hit = resolve_cached(&cache, &descriptor, DeploymentProfile::EnterpriseAirGapped, evidence(), 20).unwrap();
        assert_eq!(hit.bytes, bytes);
        fs::remove_file(root.join("sha256").join(descriptor.bundle_digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>())).unwrap();
        let miss = resolve_cached(&cache, &descriptor, DeploymentProfile::EnterpriseAirGapped, evidence(), 20);
        assert_eq!(miss, Err(DeliveryError::Inconclusive("CACHE_MISS")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn response_tamper_signature_expiry_and_descriptor_mismatch_fail_closed() {
        let bytes = b"signed-rule-bundle-v3";
        let descriptor = descriptor(bytes);
        let tampered = response(&descriptor, b"tampered", evidence());
        assert!(matches!(
            parse_delivery_response(wire_response(200, &tampered).as_bytes(), &descriptor),
            Err(BundleDeliveryError::InvalidResponse(message)) if message.contains("digest")
        ));
        let mut response_mismatch = descriptor.clone();
        response_mismatch.platform = "linux/arm64".into();
        let mismatched = response(&response_mismatch, bytes, evidence());
        assert_eq!(
            parse_delivery_response(wire_response(200, &mismatched).as_bytes(), &descriptor),
            Err(BundleDeliveryError::DescriptorMismatch)
        );
        let server_error = serde_json::json!({
            "apiVersion": API_VERSION,
            "code": "BUNDLE_UNAVAILABLE",
            "message": "upstream unavailable",
            "requestId": "request-1"
        }).to_string();
        assert_eq!(
            parse_delivery_response(wire_response(503, &server_error).as_bytes(), &descriptor),
            Err(BundleDeliveryError::Server {
                status: 503,
                code: "BUNDLE_UNAVAILABLE".into(),
                message: "upstream unavailable".into(),
            })
        );
        let calls = Arc::new(AtomicUsize::new(0));
        let mut bad_evidence = evidence();
        bad_evidence.signature_valid = false;
        let (endpoint, handle) = server(response(&descriptor, bytes, bad_evidence), Arc::clone(&calls), Duration::from_secs(2));
        let cache = erf_bundle_cache::FileBundleCache::new(std::env::temp_dir().join(format!("erf-delivery-bad-{}", unique_suffix())));
        let client = DeliveryClient::new(&endpoint, Duration::from_secs(2)).unwrap();
        assert_eq!(
            client.resolve(&cache, &descriptor, DeploymentProfile::Connected, evidence(), 20),
            Err(BundleDeliveryError::Delivery(DeliveryError::Inconclusive("SIGNATURE_INVALID")))
        );
        handle.join().unwrap();

        let mut expired = descriptor.clone();
        expired.expires_at_epoch_secs = 20;
        assert_eq!(
            resolve_cached(&cache, &expired, DeploymentProfile::EnterpriseAirGapped, evidence(), 20),
            Err(DeliveryError::Inconclusive("BUNDLE_EXPIRED"))
        );
        let _ = fs::remove_dir_all(cache.root());
    }

    #[test]
    fn slow_network_returns_a_bounded_transport_error() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(250));
        });
        let client = DeliveryClient::new(
            &format!("http://127.0.0.1:{}/v1", address.port()),
            Duration::from_millis(40),
        ).unwrap();
        let error = client.fetch(&descriptor(b"slow-bundle")).unwrap_err();
        assert!(matches!(error, BundleDeliveryError::Transport(message) if message.contains("response read failed")));
        handle.join().unwrap();
    }

    #[test]
    fn different_digest_cold_fetches_keep_independent_flights() {
        let first_bytes = b"signed-rule-bundle-first";
        let second_bytes = b"signed-rule-bundle-second";
        let first_descriptor = descriptor(first_bytes);
        let second_descriptor = descriptor(second_bytes);
        let first_calls = Arc::new(AtomicUsize::new(0));
        let second_calls = Arc::new(AtomicUsize::new(0));
        let (first_endpoint, first_server) = server(
            response(&first_descriptor, first_bytes, evidence()),
            Arc::clone(&first_calls),
            Duration::from_secs(2),
        );
        let (second_endpoint, second_server) = server(
            response(&second_descriptor, second_bytes, evidence()),
            Arc::clone(&second_calls),
            Duration::from_secs(2),
        );
        let root = std::env::temp_dir().join(format!("erf-delivery-multi-flight-{}", unique_suffix()));
        let cache = Arc::new(erf_bundle_cache::FileBundleCache::new(&root));
        let flight = Arc::new(SingleFlightDelivery::default());
        let first_client = Arc::new(DeliveryClient::new(&first_endpoint, Duration::from_secs(2)).unwrap());
        let second_client = Arc::new(DeliveryClient::new(&second_endpoint, Duration::from_secs(2)).unwrap());
        let first_flight = Arc::clone(&flight);
        let first_cache = Arc::clone(&cache);
        let first = thread::spawn(move || {
            first_flight
                .resolve(&first_client, first_cache.as_ref(), "test-cache", &first_descriptor, DeploymentProfile::Connected, evidence(), 20)
                .unwrap()
                .bytes
        });
        let second_flight = Arc::clone(&flight);
        let second_cache = Arc::clone(&cache);
        let second = thread::spawn(move || {
            second_flight
                .resolve(&second_client, second_cache.as_ref(), "test-cache", &second_descriptor, DeploymentProfile::Connected, evidence(), 20)
                .unwrap()
                .bytes
        });
        assert_eq!(first.join().unwrap(), first_bytes);
        assert_eq!(second.join().unwrap(), second_bytes);
        assert_eq!(first_calls.load(Ordering::SeqCst), 1);
        assert_eq!(second_calls.load(Ordering::SeqCst), 1);
        first_server.join().unwrap();
        second_server.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_digest_cold_fetch_is_single_flight() {
        let bytes = b"signed-rule-bundle-single-flight";
        let descriptor = descriptor(bytes);
        let body = response(&descriptor, bytes, evidence());
        let calls = Arc::new(AtomicUsize::new(0));
        let (endpoint, handle) = server(body, Arc::clone(&calls), Duration::from_secs(2));
        let root = std::env::temp_dir().join(format!("erf-delivery-flight-{}", unique_suffix()));
        let cache = Arc::new(erf_bundle_cache::FileBundleCache::new(&root));
        let client = Arc::new(DeliveryClient::new(&endpoint, Duration::from_secs(2)).unwrap());
        let flight = Arc::new(SingleFlightDelivery::default());
        let threads = (0..8).map(|_| {
            let cache = Arc::clone(&cache);
            let client = Arc::clone(&client);
            let flight = Arc::clone(&flight);
            let descriptor = descriptor.clone();
            thread::spawn(move || flight.resolve(&client, cache.as_ref(), "test-cache", &descriptor, DeploymentProfile::Connected, evidence(), 20).unwrap().bytes)
        }).collect::<Vec<_>>();
        for thread in threads { assert_eq!(thread.join().unwrap(), bytes); }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        handle.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    fn unique_suffix() -> String {
        format!("{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos())
    }
}
