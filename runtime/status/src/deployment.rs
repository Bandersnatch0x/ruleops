use super::{canonical_json, hex, is_sha256_digest, AdapterOutcome, FakeStatusAdapter, StatusPublication, StatusPublicationAdapter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const STATUS_ADAPTER_DEPLOYMENT_SCHEMA_VERSION: &str = "status-adapter-deployment-v1";
pub const STATUS_ADAPTER_DEPLOYMENT_KIND: &str = "StatusAdapterDeployment";
pub const STATUS_ADAPTER_RECEIPT_KIND: &str = "StatusAdapterReceipt";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusAdapterDeliveryStatus {
    Accepted,
    Idempotent,
    Rejected,
    Unknown,
}

impl StatusAdapterDeliveryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "ACCEPTED",
            Self::Idempotent => "IDEMPOTENT",
            Self::Rejected => "REJECTED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusAdapterDeployment {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub status: String,
    pub provider: String,
    pub endpoint: String,
    #[serde(rename = "credentialRef")]
    pub credential_ref: String,
    #[serde(rename = "permissionScope")]
    pub permission_scope: Vec<String>,
    #[serde(rename = "publicationDigest")]
    pub publication_digest: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: String,
    #[serde(rename = "externalConnectorRequired")]
    pub external_connector_required: bool,
    #[serde(rename = "scmWritten")]
    pub scm_written: bool,
    #[serde(rename = "deploymentDigest")]
    pub deployment_digest: String,
}

impl StatusAdapterDeployment {
    pub fn new(
        provider: impl Into<String>,
        endpoint: impl Into<String>,
        credential_ref: impl Into<String>,
        publication: &StatusPublication,
        request_id: impl Into<String>,
        idempotency_key: impl Into<String>,
    ) -> Result<Self, StatusAdapterDeploymentError> {
        let mut deployment = Self {
            api_version: super::STATUS_PUBLICATION_API_VERSION.into(),
            kind: STATUS_ADAPTER_DEPLOYMENT_KIND.into(),
            schema_version: STATUS_ADAPTER_DEPLOYMENT_SCHEMA_VERSION.into(),
            status: "READY_FOR_EXTERNAL_CONNECTOR".into(),
            provider: provider.into(),
            endpoint: endpoint.into(),
            credential_ref: credential_ref.into(),
            permission_scope: vec!["status:write".into()],
            publication_digest: publication.publication_digest.clone(),
            request_id: request_id.into(),
            idempotency_key: idempotency_key.into(),
            external_connector_required: true,
            scm_written: false,
            deployment_digest: String::new(),
        };
        deployment.deployment_digest = deployment.content_digest();
        deployment.validate()?;
        Ok(deployment)
    }

    pub fn as_json(&self) -> Value {
        serde_json::to_value(self).expect("StatusAdapterDeployment is serializable")
    }

    pub fn validate(&self) -> Result<(), StatusAdapterDeploymentError> {
        if self.api_version != super::STATUS_PUBLICATION_API_VERSION
            || self.kind != STATUS_ADAPTER_DEPLOYMENT_KIND
            || self.schema_version != STATUS_ADAPTER_DEPLOYMENT_SCHEMA_VERSION
            || self.status != "READY_FOR_EXTERNAL_CONNECTOR"
        {
            return Err(StatusAdapterDeploymentError::InvalidDeployment("version or status".into()));
        }
        validate_provider(&self.provider)?;
        validate_https_endpoint(&self.endpoint)?;
        validate_credential_ref(&self.credential_ref)?;
        if self.permission_scope != ["status:write"] {
            return Err(StatusAdapterDeploymentError::InvalidDeployment("permission scope is not minimal".into()));
        }
        if !is_sha256_digest(&self.publication_digest) {
            return Err(StatusAdapterDeploymentError::InvalidDeployment("publication digest".into()));
        }
        validate_request_token("request id", &self.request_id)?;
        validate_request_token("idempotency key", &self.idempotency_key)?;
        if !self.external_connector_required || self.scm_written {
            return Err(StatusAdapterDeploymentError::InvalidDeployment("SCM authority boundary".into()));
        }
        if !is_sha256_digest(&self.deployment_digest) || self.deployment_digest != self.content_digest() {
            return Err(StatusAdapterDeploymentError::DeploymentDigestMismatch);
        }
        Ok(())
    }

    pub fn validate_for(
        &self,
        publication: &StatusPublication,
        now_epoch_secs: u64,
    ) -> Result<(), StatusAdapterDeploymentError> {
        self.validate()?;
        publication
            .validate_at(now_epoch_secs)
            .map_err(|error| StatusAdapterDeploymentError::PublicationRejected(error.to_string()))?;
        if self.publication_digest != publication.publication_digest {
            return Err(StatusAdapterDeploymentError::PublicationMismatch);
        }
        Ok(())
    }

    fn content_digest(&self) -> String {
        let mut value = self.as_json();
        value
            .as_object_mut()
            .expect("StatusAdapterDeployment serializes as an object")
            .remove("deploymentDigest");
        digest_json(&value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusAdapterReceipt {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub outcome: String,
    pub status: String,
    #[serde(rename = "deploymentDigest")]
    pub deployment_digest: String,
    #[serde(rename = "publicationDigest")]
    pub publication_digest: String,
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: String,
    #[serde(rename = "scmWritten")]
    pub scm_written: bool,
    #[serde(rename = "receiptDigest")]
    pub receipt_digest: String,
}

impl StatusAdapterReceipt {
    fn new(
        deployment: &StatusAdapterDeployment,
        publication: &StatusPublication,
        outcome: StatusAdapterDeliveryStatus,
    ) -> Self {
        let mut receipt = Self {
            api_version: super::STATUS_PUBLICATION_API_VERSION.into(),
            kind: STATUS_ADAPTER_RECEIPT_KIND.into(),
            schema_version: STATUS_ADAPTER_DEPLOYMENT_SCHEMA_VERSION.into(),
            outcome: outcome.as_str().into(),
            status: "RECEIVED_ONLY".into(),
            deployment_digest: deployment.deployment_digest.clone(),
            publication_digest: publication.publication_digest.clone(),
            request_id: deployment.request_id.clone(),
            idempotency_key: deployment.idempotency_key.clone(),
            scm_written: false,
            receipt_digest: String::new(),
        };
        receipt.receipt_digest = receipt.content_digest();
        receipt
    }

    pub fn as_json(&self) -> Value {
        serde_json::to_value(self).expect("StatusAdapterReceipt is serializable")
    }

    pub fn validate(&self) -> Result<(), StatusAdapterDeploymentError> {
        if self.api_version != super::STATUS_PUBLICATION_API_VERSION
            || self.kind != STATUS_ADAPTER_RECEIPT_KIND
            || self.schema_version != STATUS_ADAPTER_DEPLOYMENT_SCHEMA_VERSION
            || self.status != "RECEIVED_ONLY"
            || !matches!(self.outcome.as_str(), "ACCEPTED" | "IDEMPOTENT")
        {
            return Err(StatusAdapterDeploymentError::InvalidReceipt("version, outcome or status".into()));
        }
        if !is_sha256_digest(&self.deployment_digest) || !is_sha256_digest(&self.publication_digest) {
            return Err(StatusAdapterDeploymentError::InvalidReceipt("receipt identity digest".into()));
        }
        validate_request_token("request id", &self.request_id)?;
        validate_request_token("idempotency key", &self.idempotency_key)?;
        if self.scm_written || !is_sha256_digest(&self.receipt_digest) || self.receipt_digest != self.content_digest() {
            return Err(StatusAdapterDeploymentError::InvalidReceipt("receipt authority or digest".into()));
        }
        Ok(())
    }

    fn content_digest(&self) -> String {
        let mut value = self.as_json();
        value
            .as_object_mut()
            .expect("StatusAdapterReceipt serializes as an object")
            .remove("receiptDigest");
        digest_json(&value)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StatusAdapterDeploymentError {
    #[error("deployment descriptor is invalid: {0}")]
    InvalidDeployment(String),
    #[error("deployment digest does not match its content")]
    DeploymentDigestMismatch,
    #[error("publication does not match the deployment descriptor")]
    PublicationMismatch,
    #[error("publication rejected by the deployment adapter: {0}")]
    PublicationRejected(String),
    #[error("adapter receipt is invalid: {0}")]
    InvalidReceipt(String),
    #[error("adapter delivery result is unknown")]
    UnknownTransport,
}

impl StatusAdapterDeploymentError {
    pub fn delivery_status(&self) -> StatusAdapterDeliveryStatus {
        match self {
            Self::UnknownTransport => StatusAdapterDeliveryStatus::Unknown,
            Self::InvalidDeployment(_)
            | Self::DeploymentDigestMismatch
            | Self::PublicationMismatch
            | Self::PublicationRejected(_)
            | Self::InvalidReceipt(_) => StatusAdapterDeliveryStatus::Rejected,
        }
    }
}

impl FakeStatusAdapter {
    pub fn deploy(
        &mut self,
        deployment: &StatusAdapterDeployment,
        publication: &StatusPublication,
        now_epoch_secs: u64,
    ) -> Result<StatusAdapterReceipt, StatusAdapterDeploymentError> {
        deployment.validate_for(publication, now_epoch_secs)?;
        let outcome = self
            .accept(publication, now_epoch_secs)
            .map_err(|error| StatusAdapterDeploymentError::PublicationRejected(error.to_string()))?;
        let outcome = match outcome {
            AdapterOutcome::Accepted(_) => StatusAdapterDeliveryStatus::Accepted,
            AdapterOutcome::Idempotent(_) => StatusAdapterDeliveryStatus::Idempotent,
        };
        let receipt = StatusAdapterReceipt::new(deployment, publication, outcome);
        receipt.validate()?;
        Ok(receipt)
    }
}

fn validate_provider(provider: &str) -> Result<(), StatusAdapterDeploymentError> {
    if provider.is_empty()
        || provider.len() > 32
        || !provider.bytes().next().is_some_and(|byte| byte.is_ascii_lowercase())
        || !provider.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(StatusAdapterDeploymentError::InvalidDeployment("provider identifier".into()));
    }
    Ok(())
}

fn validate_https_endpoint(endpoint: &str) -> Result<(), StatusAdapterDeploymentError> {
    let authority_and_path = endpoint.strip_prefix("https://").ok_or_else(|| {
        StatusAdapterDeploymentError::InvalidDeployment("endpoint must use HTTPS".into())
    })?;
    if authority_and_path.is_empty()
        || authority_and_path.chars().any(|character| character.is_whitespace() || character.is_control())
        || authority_and_path.contains('@')
        || authority_and_path.contains('?')
        || authority_and_path.contains('#')
    {
        return Err(StatusAdapterDeploymentError::InvalidDeployment("endpoint authority or query".into()));
    }
    let authority = authority_and_path.split('/').next().unwrap_or_default();
    let (host, port) = authority.split_once(':').map_or((authority, None), |(host, port)| (host, Some(port)));
    if host.is_empty() || !host.bytes().any(|byte| byte.is_ascii_alphanumeric()) {
        return Err(StatusAdapterDeploymentError::InvalidDeployment("endpoint host".into()));
    }
    if let Some(port) = port {
        if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(StatusAdapterDeploymentError::InvalidDeployment("endpoint port".into()));
        }
    }
    Ok(())
}

fn validate_credential_ref(credential_ref: &str) -> Result<(), StatusAdapterDeploymentError> {
    let reference = credential_ref.strip_prefix("secretref://").ok_or_else(|| {
        StatusAdapterDeploymentError::InvalidDeployment("credentialRef must be an opaque secretref".into())
    })?;
    if reference.is_empty()
        || reference.len() > 256
        || reference.split('/').any(|segment| segment.is_empty() || segment == "." || segment == "..")
        || !reference
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_'))
    {
        return Err(StatusAdapterDeploymentError::InvalidDeployment("credentialRef contains secret material".into()));
    }
    Ok(())
}

fn validate_request_token(name: &str, value: &str) -> Result<(), StatusAdapterDeploymentError> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b':' | b'_' | b'-'))
    {
        return Err(StatusAdapterDeploymentError::InvalidDeployment(format!("{name} is not a closed token")));
    }
    Ok(())
}

fn digest_json(value: &Value) -> String {
    let canonical = canonical_json(value);
    let bytes = serde_json::to_vec(&canonical).expect("JSON serialization is infallible");
    format!("sha256:{}", hex(&Sha256::digest(bytes)))
}
