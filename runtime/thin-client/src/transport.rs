use super::{
    is_sha256_digest, ClientError, ExceptionRequest, FeedbackDraft, CLIENT_API_VERSION, EXCEPTION_REQUEST_VERSION,
    FEEDBACK_DRAFT_VERSION,
};
use serde::Serialize;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// An explicitly selected, operator-approved HTTPS profile. This is configuration,
/// not publication authority; credential values are resolved only at request time.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionProfile {
    schema_version: String,
    approved: bool,
    endpoint: String,
    provenance: String,
    bearer_token_env: Option<String>,
    ca_certificate_file: Option<std::path::PathBuf>,
    client_identity_pem_file: Option<std::path::PathBuf>,
    proxy: Option<String>,
    #[serde(skip)]
    directory: std::path::PathBuf,
}

fn profile_error() -> ClientError {
    ClientError::InvalidEndpoint("REGISTRY_PROFILE_INVALID".into())
}

impl ConnectionProfile {
    pub fn load(path: &std::path::Path) -> Result<Self, ClientError> {
        let path = path.canonicalize().map_err(|_| profile_error())?;
        let file = std::fs::File::open(&path).map_err(|_| profile_error())?;
        let mut bytes = Vec::new();
        file.take(64 * 1024 + 1).read_to_end(&mut bytes).map_err(|_| profile_error())?;
        if bytes.len() > 64 * 1024 { return Err(profile_error()); }
        let mut profile: Self = serde_json::from_slice(&bytes).map_err(|_| profile_error())?;
        profile.directory = path.parent().ok_or_else(profile_error)?.to_owned();
        profile.validate()?;
        Ok(profile)
    }

    pub fn endpoint(&self) -> &str { &self.endpoint }
    pub fn provenance(&self) -> &str { &self.provenance }

    fn validate(&self) -> Result<(), ClientError> {
        let url = secure_url(&self.endpoint)?;
        if self.schema_version != "rf-connection-profile-v1" || !self.approved
            || self.provenance.trim().is_empty() || self.provenance.chars().any(char::is_control)
            || url.path() != "/v1"
            || (self.bearer_token_env.is_none() && self.client_identity_pem_file.is_none())
        { return Err(profile_error()); }
        if let Some(name) = &self.bearer_token_env {
            if name.is_empty() || !name.bytes().enumerate().all(|(index, c)|
                c == b'_' || c.is_ascii_alphabetic() || (index > 0 && c.is_ascii_digit()))
            { return Err(profile_error()); }
        }
        if let Some(proxy) = &self.proxy {
            let url = secure_url(proxy)?;
            if url.path() != "/" { return Err(profile_error()); }
        }
        Ok(())
    }

    /// Read only this approved origin. Redirects and ambient proxy discovery are
    /// disabled so authorization headers cannot migrate to a second endpoint.
    pub fn get_json(&self, suffix: &str, request_id: &str, timeout: Duration, max_bytes: usize)
        -> Result<(u16, Value), ClientError>
    {
        self.validate()?;
        if !(suffix.starts_with("/registry/") || suffix.starts_with("/rule-bundles/"))
            || suffix.contains('#') || timeout.is_zero() || timeout > Duration::from_secs(600)
            || max_bytes == 0
        { return Err(profile_error()); }
        let unavailable = || ClientError::SubmissionUnknown("REGISTRY_UNAVAILABLE".into());
        let mut builder = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none()).no_proxy().https_only(true)
            .min_tls_version(reqwest::tls::Version::TLS_1_2).timeout(timeout).connect_timeout(timeout);
        if let Some(path) = &self.ca_certificate_file {
            let bytes = read_profile_material(&self.directory.join(path))?;
            let certificates = reqwest::Certificate::from_pem_bundle(&bytes).map_err(|_| profile_error())?;
            if certificates.is_empty() { return Err(profile_error()); }
            for certificate in certificates { builder = builder.add_root_certificate(certificate); }
        }
        if let Some(path) = &self.client_identity_pem_file {
            let bytes = read_profile_material(&self.directory.join(path))?;
            builder = builder.identity(reqwest::Identity::from_pem(&bytes).map_err(|_| profile_error())?);
        }
        if let Some(proxy) = &self.proxy {
            builder = builder.proxy(reqwest::Proxy::all(proxy).map_err(|_| profile_error())?);
        }
        let client = builder.build().map_err(|_| profile_error())?;
        let mut request = client.get(format!("{}{suffix}", self.endpoint))
            .header("Accept", "application/json").header("X-Request-Id", request_id);
        if let Some(name) = &self.bearer_token_env {
            let token = std::env::var(name).map_err(|_| profile_error())?;
            // Reject rather than trim malformed secrets; never echo credentials.
            if token.is_empty() || token.bytes().any(|c| !c.is_ascii_graphic()) { return Err(profile_error()); }
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(|_| unavailable())?;
        let status = response.status().as_u16();
        if response.content_length().is_some_and(|length| length > max_bytes as u64) {
            return Err(unavailable());
        }
        let mut body = Vec::new();
        response.take(max_bytes as u64 + 1).read_to_end(&mut body).map_err(|_| unavailable())?;
        if body.len() > max_bytes { return Err(unavailable()); }
        let value = serde_json::from_slice(&body).map_err(|_| unavailable())?;
        Ok((status, value))
    }
}

fn secure_url(value: &str) -> Result<reqwest::Url, ClientError> {
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) || value.contains('\\') {
        return Err(profile_error());
    }
    let url = reqwest::Url::parse(value).map_err(|_| profile_error())?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty()
        || url.password().is_some() || url.query().is_some() || url.fragment().is_some()
    { return Err(profile_error()); }
    Ok(url)
}

fn read_profile_material(path: &std::path::Path) -> Result<Vec<u8>, ClientError> {
    let mut bytes = Vec::new();
    std::fs::File::open(path).map_err(|_| profile_error())?.take(1024 * 1024 + 1)
        .read_to_end(&mut bytes).map_err(|_| profile_error())?;
    if bytes.len() > 1024 * 1024 { return Err(profile_error()); }
    Ok(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubmissionReceipt {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub status: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "artifactDigest")]
    pub artifact_digest: String,
    #[serde(rename = "serverRequestId")]
    pub server_request_id: String,
    #[serde(rename = "resourceType")]
    pub resource_type: String,
    #[serde(rename = "resourceId")]
    pub resource_id: String,
    #[serde(rename = "resourceVersion")]
    pub resource_version: String,
    #[serde(rename = "resourceState")]
    pub resource_state: String,
    pub replayed: bool,
    #[serde(rename = "governanceOutcome")]
    pub governance_outcome: String,
    #[serde(rename = "exceptionApproved")]
    pub exception_approved: bool,
    #[serde(rename = "gateMutated")]
    pub gate_mutated: bool,
    #[serde(rename = "scmPublished")]
    pub scm_published: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubmissionResource {
    #[serde(rename = "type")]
    pub resource_type: String,
    pub id: String,
    pub version: String,
    pub state: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubmissionAuditContext {
    pub operation: String,
    #[serde(rename = "operationId")]
    pub operation_id: String,
    pub actor: String,
    pub reason: String,
    #[serde(rename = "resourceType")]
    pub resource_type: String,
    #[serde(rename = "resourceId")]
    pub resource_id: String,
    #[serde(rename = "resourceVersion")]
    pub resource_version: String,
    #[serde(rename = "eventDigest")]
    pub event_digest: String,
    #[serde(rename = "recordedAt")]
    pub recorded_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubmissionStatus {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub status: String,
    #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(rename = "artifactDigest", skip_serializing_if = "Option::is_none")]
    pub artifact_digest: Option<String>,
    #[serde(rename = "serverRequestId")]
    pub server_request_id: String,
    pub resource: Option<SubmissionResource>,
    #[serde(rename = "auditContext")]
    pub audit_context: Option<SubmissionAuditContext>,
    #[serde(rename = "automaticRetry")]
    pub automatic_retry: bool,
    pub authority: String,
    pub replayed: bool,
}

pub fn submit_feedback(
    draft: &FeedbackDraft,
    endpoint: &str,
    timeout: Duration,
) -> Result<SubmissionReceipt, ClientError> {
    submit_artifact(
        draft.as_json(),
        "feedback",
        FEEDBACK_DRAFT_VERSION,
        &draft.request_id,
        &draft.actor,
        &draft.reason,
        &draft.digest(),
        endpoint,
        timeout,
    )
}

pub fn submit_exception(
    request: &ExceptionRequest,
    endpoint: &str,
    timeout: Duration,
) -> Result<SubmissionReceipt, ClientError> {
    submit_artifact(
        request.as_json(),
        "exceptions",
        EXCEPTION_REQUEST_VERSION,
        &request.request_id,
        &request.actor,
        &request.reason,
        &request.digest(),
        endpoint,
        timeout,
    )
}

pub fn query_submission_status(
    request_id: Option<&str>,
    artifact_digest: Option<&str>,
    endpoint: &str,
    timeout: Duration,
) -> Result<SubmissionStatus, ClientError> {
    if request_id.is_none() && artifact_digest.is_none() {
        return Err(ClientError::InvalidField("status query selector"));
    }
    for (field, value) in [("requestId", request_id), ("artifactDigest", artifact_digest)] {
        if let Some(value) = value {
            if !is_sha256_digest(value) {
                return Err(ClientError::InvalidField(field));
            }
        }
    }
    let endpoint = Endpoint::parse(endpoint)?;
    let mut selectors = Vec::new();
    if let Some(value) = request_id {
        selectors.push(format!("requestId={}", encode_query_component(value)));
    }
    if let Some(value) = artifact_digest {
        selectors.push(format!("artifactDigest={}", encode_query_component(value)));
    }
    let path = format!(
        "{}/submissions/status?{}",
        endpoint.path.trim_end_matches('/'),
        selectors.join("&")
    );
    let correlation_id = request_id.or(artifact_digest).unwrap_or("status-query");
    let response = get_response(&endpoint, &path, correlation_id, timeout)?;
    parse_submission_status_response(&response, request_id, artifact_digest)
}

pub(crate) fn registry_response(endpoint: &str, suffix: &str, timeout: Duration) -> Result<(u16, Value), ClientError> {
    let endpoint = Endpoint::parse(endpoint)?;
    if !endpoint.host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
        || endpoint.path.trim_end_matches('/') != "/v1"
        || timeout.is_zero() || timeout > Duration::from_secs(600)
    {
        return Err(ClientError::InvalidEndpoint("registry requires a literal loopback /v1 endpoint and bounded timeout".into()));
    }
    let path = format!("/v1/registry/{suffix}");
    parse_json_response(&get_response(&endpoint, &path, "registry-read", timeout)?)
}

fn get_response(endpoint: &Endpoint, path: &str, correlation_id: &str, timeout: Duration) -> Result<Vec<u8>, ClientError> {
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nAccept: application/json\r\nConnection: close\r\nX-Request-Id: {correlation_id}\r\n\r\n",
        path = path,
        host = endpoint.host_header,
        correlation_id = correlation_id,
    );
    let mut stream = connect(&endpoint, timeout)?;
    stream
        .write_all(request.as_bytes())
        .map_err(|error| ClientError::SubmissionUnknown(format!("status request write failed: {error}")))?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|error| ClientError::SubmissionUnknown(format!("status request close failed: {error}")))?;
    read_response(&mut stream, timeout)
}

#[allow(clippy::too_many_arguments)]
fn submit_artifact(
    payload: Value,
    resource_path: &str,
    schema_version: &str,
    request_id: &str,
    actor: &str,
    reason: &str,
    artifact_digest: &str,
    endpoint: &str,
    timeout: Duration,
) -> Result<SubmissionReceipt, ClientError> {
    let endpoint = Endpoint::parse(endpoint)?;
    let envelope = serde_json::json!({
        "id": request_id,
        "version": schema_version,
        "payload": payload,
    });
    let body = serde_json::to_vec(&envelope).map_err(|error| ClientError::Serialization(error.to_string()))?;
    let path = format!("{}/{}", endpoint.path.trim_end_matches('/'), resource_path);
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\nIdempotency-Key: {request_id}\r\nX-Request-Id: {request_id}\r\nX-Actor: {actor}\r\nX-Reason: {reason}\r\n\r\n",
        path = path,
        host = endpoint.host_header,
        length = body.len(),
        request_id = request_id,
        actor = actor,
        reason = reason,
    );

    let mut stream = connect(&endpoint, timeout)?;
    stream
        .write_all(request.as_bytes())
        .and_then(|_| stream.write_all(&body))
        .map_err(|error| ClientError::SubmissionUnknown(format!("request write failed: {error}")))?;
    stream
        .shutdown(Shutdown::Write)
        .map_err(|error| ClientError::SubmissionUnknown(format!("request close failed: {error}")))?;
    let response = read_response(&mut stream, timeout)?;
    parse_submission_response(
        &response,
        request_id,
        artifact_digest,
        resource_path,
    )
}

#[derive(Debug)]
struct Endpoint {
    host_header: String,
    host: String,
    port: u16,
    path: String,
}

impl Endpoint {
    fn parse(value: &str) -> Result<Self, ClientError> {
        let rest = value
            .strip_prefix("http://")
            .ok_or_else(|| ClientError::InvalidEndpoint("only explicit http:// endpoints are supported by this local seam".into()))?;
        if rest.is_empty() || rest.chars().any(|character| character.is_control() || character.is_whitespace()) {
            return Err(ClientError::InvalidEndpoint("endpoint authority is empty or contains whitespace".into()));
        }
        let authority_end = rest.find('/').unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        let suffix = &rest[authority_end..];
        if authority.is_empty() || authority.contains('@') || authority.contains('?') || authority.contains('#') {
            return Err(ClientError::InvalidEndpoint("endpoint must not contain credentials, query, or fragment".into()));
        }
        let (host, port) = parse_authority(authority)?;
        if suffix.contains('?') || suffix.contains('#') || suffix.contains("//") {
            return Err(ClientError::InvalidEndpoint("endpoint path must be a single base path".into()));
        }
        Ok(Self {
            host_header: authority.into(),
            host,
            port,
            path: if suffix.is_empty() { "/".into() } else { suffix.into() },
        })
    }
}

fn parse_authority(authority: &str) -> Result<(String, u16), ClientError> {
    if let Some(rest) = authority.strip_prefix('[') {
        let end = rest
            .find(']')
            .ok_or_else(|| ClientError::InvalidEndpoint("IPv6 endpoint is missing ]".into()))?;
        let host = &rest[..end];
        let port = rest[end + 1..]
            .strip_prefix(':')
            .ok_or_else(|| ClientError::InvalidEndpoint("IPv6 endpoint must include a port".into()))?
            .parse::<u16>()
            .map_err(|_| ClientError::InvalidEndpoint("endpoint port is invalid".into()))?;
        if host.is_empty() || port == 0 {
            return Err(ClientError::InvalidEndpoint("endpoint host or port is invalid".into()));
        }
        return Ok((host.into(), port));
    }
    if authority.matches(':').count() > 1 {
        return Err(ClientError::InvalidEndpoint("IPv6 endpoints must use brackets".into()));
    }
    let (host, port) = authority
        .rsplit_once(':')
        .ok_or_else(|| ClientError::InvalidEndpoint("endpoint must include an explicit port".into()))?;
    let port = port
        .parse::<u16>()
        .map_err(|_| ClientError::InvalidEndpoint("endpoint port is invalid".into()))?;
    if host.is_empty() || port == 0 {
        return Err(ClientError::InvalidEndpoint("endpoint host or port is invalid".into()));
    }
    Ok((host.into(), port))
}

fn connect(endpoint: &Endpoint, timeout: Duration) -> Result<TcpStream, ClientError> {
    let addresses = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .map_err(|error| ClientError::SubmissionUnknown(format!("endpoint lookup failed: {error}")))?;
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(stream) => {
                stream
                    .set_read_timeout(Some(timeout))
                    .and_then(|_| stream.set_write_timeout(Some(timeout)))
                    .map_err(|error| ClientError::SubmissionUnknown(format!("socket timeout setup failed: {error}")))?;
                return Ok(stream);
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(ClientError::SubmissionUnknown(format!(
        "endpoint connection failed: {}",
        last_error.map(|error| error.to_string()).unwrap_or_else(|| "no addresses resolved".into())
    )))
}

fn read_response(stream: &mut TcpStream, timeout: Duration) -> Result<Vec<u8>, ClientError> {
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| ClientError::SubmissionUnknown(format!("socket timeout setup failed: {error}")))?;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = stream
            .read(&mut buffer)
            .map_err(|error| ClientError::SubmissionUnknown(format!("response read failed: {error}")))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(ClientError::SubmissionUnknown("server response exceeds the client limit".into()));
        }
    }
    if bytes.is_empty() {
        return Err(ClientError::SubmissionUnknown("server returned an empty response".into()));
    }
    Ok(bytes)
}

fn parse_json_response(bytes: &[u8]) -> Result<(u16, Value), ClientError> {
    let separator = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| ClientError::SubmissionUnknown("server response has no HTTP header boundary".into()))?;
    let header = std::str::from_utf8(&bytes[..separator])
        .map_err(|_| ClientError::SubmissionUnknown("server response headers are not UTF-8".into()))?;
    let body = &bytes[separator + 4..];
    let status = header
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| ClientError::SubmissionUnknown("server response status is invalid".into()))?;
    if header.lines().any(|line| line.to_ascii_lowercase().starts_with("transfer-encoding:")) {
        return Err(ClientError::SubmissionUnknown("chunked server responses are not supported".into()));
    }
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| ClientError::SubmissionUnknown(format!("server response JSON is invalid: {error}")))?;
    Ok((status, value))
}

fn parse_submission_response(
    bytes: &[u8],
    request_id: &str,
    artifact_digest: &str,
    resource_path: &str,
) -> Result<SubmissionReceipt, ClientError> {
    let (status, value) = parse_json_response(bytes)?;
    let object = value
        .as_object()
        .ok_or_else(|| ClientError::SubmissionUnknown("server response must be a JSON object".into()))?;
    if object.get("apiVersion").and_then(Value::as_str) != Some(CLIENT_API_VERSION) {
        return Err(ClientError::SubmissionUnknown("server response apiVersion is unsupported".into()));
    }
    let response_request_id = object
        .get("requestId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
        .ok_or_else(|| ClientError::SubmissionUnknown("server response requestId is missing".into()))?;
    if !(200..300).contains(&status) {
        let code = object.get("code").and_then(Value::as_str).unwrap_or("SERVER_ERROR");
        let message = object.get("message").and_then(Value::as_str).unwrap_or("server rejected the request");
        return Err(classify_only_known_4xx_as_rejected(
            status,
            code,
            message,
            response_request_id,
        ));
    }
    let resource = object
        .get("resource")
        .and_then(Value::as_object)
        .ok_or_else(|| ClientError::SubmissionUnknown("successful server response has no resource".into()))?;
    let resource_type = resource.get("type").and_then(Value::as_str).ok_or_else(|| {
        ClientError::SubmissionUnknown("successful server response resource type is missing".into())
    })?;
    if resource_type != resource_path {
        return Err(ClientError::SubmissionUnknown("server resource type does not match the submission".into()));
    }
    let resource_id = resource.get("id").and_then(Value::as_str).ok_or_else(|| {
        ClientError::SubmissionUnknown("successful server response resource id is missing".into())
    })?;
    let resource_version = resource.get("version").and_then(Value::as_str).ok_or_else(|| {
        ClientError::SubmissionUnknown("successful server response resource version is missing".into())
    })?;
    let resource_state = resource.get("state").and_then(Value::as_str).ok_or_else(|| {
        ClientError::SubmissionUnknown("successful server response resource state is missing".into())
    })?;
    let replayed = object
        .get("replayed")
        .and_then(Value::as_bool)
        .ok_or_else(|| ClientError::SubmissionUnknown("successful server response replay flag is missing".into()))?;
    if resource_id != request_id {
        return Err(ClientError::SubmissionUnknown("server resource id does not match the request".into()));
    }
    Ok(SubmissionReceipt {
        api_version: CLIENT_API_VERSION.into(),
        kind: "SubmissionReceipt".into(),
        schema_version: "submission-receipt-v1".into(),
        status: if replayed { "REPLAYED" } else { "SUBMITTED" }.into(),
        request_id: request_id.into(),
        artifact_digest: artifact_digest.into(),
        server_request_id: response_request_id.into(),
        resource_type: resource_type.into(),
        resource_id: resource_id.into(),
        resource_version: resource_version.into(),
        resource_state: resource_state.into(),
        replayed,
        governance_outcome: "RECEIVED_ONLY".into(),
        exception_approved: false,
        gate_mutated: false,
        scm_published: false,
    })
}

fn parse_submission_status_response(
    bytes: &[u8],
    requested_request_id: Option<&str>,
    requested_artifact_digest: Option<&str>,
) -> Result<SubmissionStatus, ClientError> {
    let (status, value) = parse_json_response(bytes)?;
    let object = value
        .as_object()
        .ok_or_else(|| ClientError::SubmissionUnknown("status response must be a JSON object".into()))?;
    if object.get("apiVersion").and_then(Value::as_str) != Some(CLIENT_API_VERSION) {
        return Err(ClientError::SubmissionUnknown("status response apiVersion is unsupported".into()));
    }
    let response_server_request_id = object
        .get("serverRequestId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        .map(str::to_owned);
    if !(200..300).contains(&status) {
        let code = object.get("code").and_then(Value::as_str).unwrap_or("SERVER_ERROR");
        let message = object.get("message").and_then(Value::as_str).unwrap_or("status query failed");
        let error_request_id = response_server_request_id
            .as_deref()
            .or_else(|| object.get("requestId").and_then(Value::as_str))
            .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
            .ok_or_else(|| ClientError::SubmissionUnknown("status error correlation id is missing".into()))?;
        return Err(classify_only_known_4xx_as_rejected(
            status,
            code,
            message,
            error_request_id,
        ));
    }
    let server_request_id = response_server_request_id
        .ok_or_else(|| ClientError::SubmissionUnknown("status response serverRequestId is missing".into()))?;
    if object.get("kind").and_then(Value::as_str) != Some("SubmissionStatus")
        || object.get("schemaVersion").and_then(Value::as_str) != Some("submission-status-v1")
    {
        return Err(ClientError::SubmissionUnknown("status response schema is unsupported".into()));
    }
    let status_value = required_response_text(object, "status")?;
    if !matches!(status_value.as_str(), "PENDING" | "RECEIVED_ONLY" | "REPLAYED" | "UNKNOWN") {
        return Err(ClientError::SubmissionUnknown("status response status is invalid".into()));
    }
    let response_request_id = optional_response_digest(object, "requestId")?;
    let response_artifact_digest = optional_response_digest(object, "artifactDigest")?;
    if requested_request_id.is_some() && response_request_id.as_deref() != requested_request_id {
        return Err(ClientError::SubmissionUnknown("status response requestId does not match the query".into()));
    }
    if requested_artifact_digest.is_some()
        && response_artifact_digest.as_deref() != requested_artifact_digest
    {
        return Err(ClientError::SubmissionUnknown(
            "status response artifactDigest does not match the query".into(),
        ));
    }
    let replayed = object
        .get("replayed")
        .and_then(Value::as_bool)
        .ok_or_else(|| ClientError::SubmissionUnknown("status response replay flag is missing".into()))?;
    let automatic_retry = object
        .get("automaticRetry")
        .and_then(Value::as_bool)
        .ok_or_else(|| ClientError::SubmissionUnknown("status response retry policy is missing".into()))?;
    if automatic_retry {
        return Err(ClientError::SubmissionUnknown("status response permits an unsafe automatic retry".into()));
    }
    let authority = required_response_text(object, "authority")?;
    let resource = parse_submission_resource(object.get("resource"))?;
    let audit_context = parse_audit_context(object.get("auditContext"))?;
    match status_value.as_str() {
        "RECEIVED_ONLY" | "REPLAYED" => {
            if response_request_id.is_none() || response_artifact_digest.is_none()
                || resource.is_none() || audit_context.is_none() || authority != "RECEIVED_ONLY"
            {
                return Err(ClientError::SubmissionUnknown(
                    "known status response is missing identity, resource or audit context".into(),
                ));
            }
            let resource = resource.as_ref().expect("known status resource checked above");
            let audit_context = audit_context.as_ref().expect("known status audit checked above");
            let expected_version = match resource.resource_type.as_str() {
                "feedback" => "feedback-draft-v1",
                "exceptions" => "exception-request-v1",
                _ => {
                    return Err(ClientError::SubmissionUnknown(
                        "status response resource type is not a submission".into(),
                    ));
                }
            };
            if resource.id.as_str() != response_request_id.as_deref().unwrap_or_default()
                || audit_context.resource_id.as_str() != resource.id.as_str()
                || audit_context.resource_type.as_str() != resource.resource_type.as_str()
                || audit_context.resource_version.as_str() != resource.version.as_str()
                || audit_context.operation != "CREATE"
                || resource.version.as_str() != expected_version
            {
                return Err(ClientError::SubmissionUnknown(
                    "status response resource and audit identity is inconsistent".into(),
                ));
            }
            if (status_value == "REPLAYED") != replayed {
                return Err(ClientError::SubmissionUnknown("status response replay state is inconsistent".into()));
            }
        }
        "PENDING" => {
            if response_request_id.is_none() && response_artifact_digest.is_none() {
                return Err(ClientError::SubmissionUnknown("pending status has no query identity".into()));
            }
            if replayed || authority != "UNKNOWN" {
                return Err(ClientError::SubmissionUnknown("pending status has unsafe authority".into()));
            }
        }
        "UNKNOWN" => {
            if response_request_id.is_none() && response_artifact_digest.is_none() {
                return Err(ClientError::SubmissionUnknown("unknown status has no query identity".into()));
            }
            if replayed || resource.is_some() || audit_context.is_some() || authority != "UNKNOWN" {
                return Err(ClientError::SubmissionUnknown("unknown status contains an authority-bearing result".into()));
            }
        }
        _ => unreachable!(),
    }
    Ok(SubmissionStatus {
        api_version: CLIENT_API_VERSION.into(),
        kind: "SubmissionStatus".into(),
        schema_version: "submission-status-v1".into(),
        status: status_value,
        request_id: response_request_id,
        artifact_digest: response_artifact_digest,
        server_request_id,
        resource,
        audit_context,
        automatic_retry,
        authority,
        replayed,
    })
}

fn parse_submission_resource(value: Option<&Value>) -> Result<Option<SubmissionResource>, ClientError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let object = value
        .as_object()
        .ok_or_else(|| ClientError::SubmissionUnknown("status resource is not an object".into()))?;
    require_exact_response_fields(object, &["type", "id", "version", "state", "digest"], "resource")?;
    let resource = SubmissionResource {
        resource_type: required_response_text(object, "type")?,
        id: required_response_text(object, "id")?,
        version: required_response_text(object, "version")?,
        state: required_response_text(object, "state")?,
        digest: required_response_digest(object, "digest")?,
    };
    Ok(Some(resource))
}

fn parse_audit_context(value: Option<&Value>) -> Result<Option<SubmissionAuditContext>, ClientError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let object = value
        .as_object()
        .ok_or_else(|| ClientError::SubmissionUnknown("status auditContext is not an object".into()))?;
    require_exact_response_fields(
        object,
        &[
            "operation", "operationId", "actor", "reason", "resourceType", "resourceId",
            "resourceVersion", "eventDigest", "recordedAt",
        ],
        "auditContext",
    )?;
    Ok(Some(SubmissionAuditContext {
        operation: required_response_text(object, "operation")?,
        operation_id: required_response_text(object, "operationId")?,
        actor: required_response_text(object, "actor")?,
        reason: required_response_text(object, "reason")?,
        resource_type: required_response_text(object, "resourceType")?,
        resource_id: required_response_text(object, "resourceId")?,
        resource_version: required_response_text(object, "resourceVersion")?,
        event_digest: required_response_digest(object, "eventDigest")?,
        recorded_at: required_response_text(object, "recordedAt")?,
    }))
}

fn require_exact_response_fields(
    object: &serde_json::Map<String, Value>,
    fields: &[&str],
    label: &str,
) -> Result<(), ClientError> {
    if object.len() != fields.len()
        || fields.iter().any(|field| !object.contains_key(*field))
        || object.keys().any(|key| !fields.iter().any(|field| field == key))
    {
        return Err(ClientError::SubmissionUnknown(format!(
            "status {label} contains an unsupported or missing field"
        )));
    }
    Ok(())
}

fn required_response_text(object: &serde_json::Map<String, Value>, field: &str) -> Result<String, ClientError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(|| ClientError::SubmissionUnknown(format!("status response {field} is missing or invalid")))
}

fn required_response_digest(object: &serde_json::Map<String, Value>, field: &str) -> Result<String, ClientError> {
    let value = required_response_text(object, field)?;
    if !is_sha256_digest(&value) {
        return Err(ClientError::SubmissionUnknown(format!("status response {field} is not a SHA-256 digest")));
    }
    Ok(value)
}

fn optional_response_digest(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<String>, ClientError> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let value = value
        .as_str()
        .ok_or_else(|| ClientError::SubmissionUnknown(format!("status response {field} is not text")))?;
    if !is_sha256_digest(value) {
        return Err(ClientError::SubmissionUnknown(format!("status response {field} is not a SHA-256 digest")));
    }
    Ok(Some(value.to_owned()))
}

pub(crate) fn encode_query_component(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn classify_only_known_4xx_as_rejected(
    status: u16,
    code: &str,
    message: &str,
    response_request_id: &str,
) -> ClientError {
    if (400..500).contains(&status) {
        return ClientError::SubmissionRejected {
            status,
            code: code.into(),
            message: message.into(),
        };
    }
    ClientError::SubmissionUnknown(format!(
        "server HTTP {status} {code}: {message} (requestId={response_request_id})"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{make_exception_request, make_feedback_draft, FindingView, CLIENT_API_VERSION, FINDING_VIEW_KIND};
    use std::net::TcpListener;
    use std::thread;

    fn finding() -> FindingView {
        FindingView::from_json(&serde_json::json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": FINDING_VIEW_KIND,
            "findingId": "finding-transport",
            "ruleId": "corp.java.transport",
            "fingerprint": "fp-transport",
            "message": "transport test",
            "location": {"path": "src/Main.java", "line": 4, "column": 2}
        }))
        .unwrap()
    }

    fn server(response_status: &str, body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let response_status = response_status.to_owned();
        let body = body.to_owned();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            stream.read_to_end(&mut request).ok();
            let request = String::from_utf8_lossy(&request);
            assert!(request.starts_with("POST /v1/feedback HTTP/1.1"));
            assert!(request.contains("Idempotency-Key: sha256:"));
            let response = format!(
                "HTTP/1.1 {response_status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        format!("http://127.0.0.1:{}/v1", address.port())
    }

    fn status_server(response_status: &str, body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let response_status = response_status.to_owned();
        let body = body.to_owned();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            stream.read_to_end(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request);
            assert!(request.starts_with("GET /v1/submissions/status?requestId=sha256%3A"));
            let response = format!(
                "HTTP/1.1 {response_status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        format!("http://127.0.0.1:{}/v1", address.port())
    }

    #[test]
    fn feedback_submission_returns_received_only_receipt() {
        let draft = make_feedback_draft(&finding(), "true-fp", "transport test", "alice").unwrap();
        let body = serde_json::json!({
            "apiVersion": CLIENT_API_VERSION,
            "requestId": "server-request-1",
            "resource": {
                "type": "feedback",
                "id": draft.request_id,
                "version": "feedback-draft-v1",
                "state": "OPEN"
            },
            "replayed": false
        });
        let receipt = submit_feedback(&draft, &server("201 Created", &body.to_string()), Duration::from_secs(2)).unwrap();
        assert_eq!(receipt.status, "SUBMITTED");
        assert_eq!(receipt.governance_outcome, "RECEIVED_ONLY");
        assert!(!receipt.exception_approved);
        assert!(!receipt.gate_mutated);
        assert!(!receipt.scm_published);
    }

    #[test]
    fn malformed_draft_and_server_rejection_fail_closed() {
        let draft = make_feedback_draft(&finding(), "not-fp", "transport test", "alice").unwrap();
        let mut json = draft.as_json();
        json["reason"] = Value::String("changed".into());
        assert!(FeedbackDraft::from_json(&json).is_err());
        let body = serde_json::json!({
            "apiVersion": CLIENT_API_VERSION,
            "requestId": "server-request-2",
            "code": "IDEMPOTENCY_CONFLICT",
            "message": "conflict"
        });
        let error = submit_feedback(&draft, &server("409 Conflict", &body.to_string()), Duration::from_secs(2)).unwrap_err();
        assert!(matches!(error, ClientError::SubmissionRejected { status: 409, .. }));
        let error = submit_feedback(&draft, &server("400 Bad Request", "not-json"), Duration::from_secs(2)).unwrap_err();
        assert!(matches!(error, ClientError::SubmissionUnknown(_)));
    }

    #[test]
    fn only_4xx_responses_are_known_rejections() {
        let draft = make_feedback_draft(&finding(), "true-fp", "transport test", "alice").unwrap();
        let body = serde_json::json!({
            "apiVersion": CLIENT_API_VERSION,
            "requestId": "server-request-status",
            "code": "SUBMISSION_OUTCOME",
            "message": "submission outcome"
        })
        .to_string();

        for (status, expected_rejection) in [
            ("400 Bad Request", true),
            ("499 Client Closed Request", true),
            ("301 Moved Permanently", false),
            ("307 Temporary Redirect", false),
            ("308 Permanent Redirect", false),
            ("500 Internal Server Error", false),
            ("503 Service Unavailable", false),
        ] {
            let error = submit_feedback(&draft, &server(status, &body), Duration::from_secs(2)).unwrap_err();
            if expected_rejection {
                assert!(matches!(error, ClientError::SubmissionRejected { .. }), "status={status}: {error:?}");
            } else {
                assert!(matches!(error, ClientError::SubmissionUnknown(_)), "status={status}: {error:?}");
            }
        }
    }

    #[test]
    fn missing_response_identity_is_unknown_even_for_4xx() {
        let draft = make_feedback_draft(&finding(), "true-fp", "transport test", "alice").unwrap();
        for body in [
            serde_json::json!({
                "apiVersion": CLIENT_API_VERSION,
                "code": "BAD_REQUEST",
                "message": "request identity was not returned"
            }),
            serde_json::json!({
                "apiVersion": CLIENT_API_VERSION,
                "requestId": "",
                "code": "BAD_REQUEST",
                "message": "request identity was empty"
            }),
        ] {
            let error = submit_feedback(
                &draft,
                &server("400 Bad Request", &body.to_string()),
                Duration::from_secs(2),
            )
            .unwrap_err();
            assert!(matches!(error, ClientError::SubmissionUnknown(_)));
        }
    }

    #[test]
    fn exception_submission_uses_exception_path_and_replay_status() {
        let request = make_exception_request(&finding(), "src/**", "migration", "alice").unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let request_id = request.request_id.clone();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).ok();
            assert!(String::from_utf8_lossy(&bytes).starts_with("POST /v1/exceptions HTTP/1.1"));
            let body = serde_json::json!({
                "apiVersion": CLIENT_API_VERSION,
                "requestId": "server-request-3",
                "resource": {"type": "exceptions", "id": request_id, "version": "exception-request-v1", "state": "REQUESTED"},
                "replayed": true
            }).to_string();
            let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            stream.write_all(response.as_bytes()).unwrap();
        });
        let receipt = submit_exception(&request, &format!("http://127.0.0.1:{}/v1", address.port()), Duration::from_secs(2)).unwrap();
        assert_eq!(receipt.status, "REPLAYED");
        assert_eq!(receipt.resource_state, "REQUESTED");
        assert!(!receipt.exception_approved);
    }

    #[test]
    fn status_query_binds_resource_audit_and_received_only_authority() {
        let request_id = "sha256:".to_owned() + &"a".repeat(64);
        let artifact_digest = "sha256:".to_owned() + &"b".repeat(64);
        let body = serde_json::json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": "SubmissionStatus",
            "schemaVersion": "submission-status-v1",
            "status": "RECEIVED_ONLY",
            "requestId": request_id,
            "artifactDigest": artifact_digest,
            "serverRequestId": "server-status-1",
            "replayed": false,
            "automaticRetry": false,
            "authority": "RECEIVED_ONLY",
            "resource": {
                "type": "feedback",
                "id": request_id,
                "version": "feedback-draft-v1",
                "state": "OPEN",
                "digest": "sha256:".to_owned() + &"c".repeat(64)
            },
            "auditContext": {
                "operation": "CREATE",
                "operationId": request_id,
                "actor": "alice",
                "reason": "status test",
                "resourceType": "feedback",
                "resourceId": request_id,
                "resourceVersion": "feedback-draft-v1",
                "eventDigest": "sha256:".to_owned() + &"d".repeat(64),
                "recordedAt": "2026-09-20T00:00:00Z"
            }
        });
        let status = query_submission_status(
            Some(&request_id),
            None,
            &status_server("200 OK", &body.to_string()),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(status.status, "RECEIVED_ONLY");
        assert_eq!(status.request_id.as_deref(), Some(request_id.as_str()));
        assert_eq!(status.artifact_digest.as_deref(), Some(artifact_digest.as_str()));
        assert_eq!(status.resource.as_ref().unwrap().resource_type, "feedback");
        assert_eq!(status.audit_context.as_ref().unwrap().actor, "alice");
        assert!(!status.automatic_retry);
        assert_eq!(status.authority, "RECEIVED_ONLY");
    }

    #[test]
    fn unknown_status_has_no_resource_and_never_becomes_a_retry_instruction() {
        let request_id = "sha256:".to_owned() + &"e".repeat(64);
        let body = serde_json::json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": "SubmissionStatus",
            "schemaVersion": "submission-status-v1",
            "status": "UNKNOWN",
            "requestId": request_id,
            "artifactDigest": Value::Null,
            "serverRequestId": "server-status-unknown",
            "replayed": false,
            "automaticRetry": false,
            "authority": "UNKNOWN",
            "resource": Value::Null,
            "auditContext": Value::Null
        });
        let status = query_submission_status(
            Some(&request_id),
            None,
            &status_server("200 OK", &body.to_string()),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(status.status, "UNKNOWN");
        assert!(status.resource.is_none());
        assert!(status.audit_context.is_none());
        assert!(!status.automatic_retry);
        assert_eq!(status.authority, "UNKNOWN");
    }

    #[test]
    fn status_client_requires_server_correlation_and_cross_checks_resource_audit_identity() {
        let request_id = "sha256:".to_owned() + &"f".repeat(64);
        let artifact_digest = "sha256:".to_owned() + &"0".repeat(64);
        let mut missing_server_request_id = serde_json::json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": "SubmissionStatus",
            "schemaVersion": "submission-status-v1",
            "status": "RECEIVED_ONLY",
            "requestId": request_id,
            "artifactDigest": artifact_digest,
            "replayed": false,
            "automaticRetry": false,
            "authority": "RECEIVED_ONLY",
            "resource": {
                "type": "feedback",
                "id": request_id,
                "version": "feedback-draft-v1",
                "state": "OPEN",
                "digest": "sha256:".to_owned() + &"1".repeat(64)
            },
            "auditContext": {
                "operation": "CREATE",
                "operationId": request_id,
                "actor": "alice",
                "reason": "status test",
                "resourceType": "feedback",
                "resourceId": request_id,
                "resourceVersion": "feedback-draft-v1",
                "eventDigest": "sha256:".to_owned() + &"2".repeat(64),
                "recordedAt": "2026-09-20T00:00:00Z"
            }
        });
        missing_server_request_id
            .as_object_mut()
            .unwrap()
            .remove("serverRequestId");
        let error = query_submission_status(
            Some(&request_id),
            None,
            &status_server("200 OK", &missing_server_request_id.to_string()),
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(matches!(error, ClientError::SubmissionUnknown(_)));

        let mismatch = serde_json::json!({
            "apiVersion": CLIENT_API_VERSION,
            "kind": "SubmissionStatus",
            "schemaVersion": "submission-status-v1",
            "status": "RECEIVED_ONLY",
            "requestId": request_id,
            "artifactDigest": artifact_digest,
            "serverRequestId": "server-status-mismatch",
            "replayed": false,
            "automaticRetry": false,
            "authority": "RECEIVED_ONLY",
            "resource": {
                "type": "feedback",
                "id": request_id,
                "version": "feedback-draft-v1",
                "state": "OPEN",
                "digest": "sha256:".to_owned() + &"1".repeat(64)
            },
            "auditContext": {
                "operation": "CREATE",
                "operationId": request_id,
                "actor": "alice",
                "reason": "status test",
                "resourceType": "exceptions",
                "resourceId": request_id,
                "resourceVersion": "exception-request-v1",
                "eventDigest": "sha256:".to_owned() + &"2".repeat(64),
                "recordedAt": "2026-09-20T00:00:00Z"
            }
        });
        let error = query_submission_status(
            Some(&request_id),
            None,
            &status_server("200 OK", &mismatch.to_string()),
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert!(matches!(error, ClientError::SubmissionUnknown(_)));
    }
}

#[cfg(test)]
mod connection_profile_tests {
    use super::*;

    fn profile() -> Value {
        serde_json::json!({"schemaVersion":"rf-connection-profile-v1", "approved":true,
            "endpoint":"https://registry.example.test/v1", "provenance":"release-ops approval 42",
            "bearerTokenEnv":"REGISTRY_TOKEN"})
    }

    #[test]
    fn approved_profile_requires_https_explicit_approval_and_reference_only_credentials() {
        let valid: ConnectionProfile = serde_json::from_value(profile()).unwrap();
        valid.validate().unwrap();
        for (field, value) in [
            ("approved", Value::Bool(false)),
            ("provenance", Value::String(String::new())),
            ("endpoint", Value::String("http://registry.example.test/v1".into())),
            ("endpoint", Value::String("https://user:secret@registry.example.test/v1".into())),
            ("endpoint", Value::String("https://registry.example.test/v1?token=secret".into())),
            ("endpoint", Value::String("https://registry.example.test/other".into())),
            ("bearerTokenEnv", Value::String("literal token".into())),
            ("bearerTokenEnv", Value::Null),
            ("proxy", Value::String("http://proxy.example.test:8080".into())),
            ("proxy", Value::String("https://user:secret@proxy.example.test".into())),
        ] {
            let mut bad = profile(); bad[field] = value;
            let bad: ConnectionProfile = serde_json::from_value(bad).unwrap();
            assert!(bad.validate().is_err(), "accepted {field}");
        }
        let mut bad = profile(); bad["bearerToken"] = "do-not-store".into();
        assert!(serde_json::from_value::<ConnectionProfile>(bad).is_err());
    }

    #[test]
    fn mtls_and_explicit_https_proxy_are_supported_without_bearer() {
        let mut value = profile();
        value.as_object_mut().unwrap().remove("bearerTokenEnv");
        value["clientIdentityPemFile"] = "client.pem".into();
        value["caCertificateFile"] = "enterprise-ca.pem".into();
        value["proxy"] = "https://proxy.example.test:8443".into();
        let profile: ConnectionProfile = serde_json::from_value(value).unwrap();
        profile.validate().unwrap();
    }
}
