//! Read-only registry seam. Pins verify identity/content-digest equality, never publication authority.
use crate::transport::{encode_query_component, registry_response};
use crate::{is_sha256_digest, require_exact_fields, ClientError, CLIENT_API_VERSION};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::time::Duration;

type ReadResult = Result<Value, &'static str>;
const INVALID: &str = "REGISTRY_RESPONSE_INVALID";

/// Read one explicit page. FORMAT_ONLY does not verify the content behind a digest.
pub fn list(endpoint: &str, resource_type: &str, cursor: &str, limit: usize, timeout: Duration) -> ReadResult {
    validate_type(resource_type)?;
    if !(1..=256).contains(&limit) || cursor.len() > 4096 || cursor.contains(['\r', '\n']) {
        return Err("REGISTRY_QUERY_INVALID");
    }
    // Store cursors contain NUL separators; keep them opaque and URL-encode, never reject/reinterpret them.
    let response = read(endpoint, &format!("{resource_type}?cursor={}&limit={limit}", encode_query_component(cursor)), timeout)?;
    let object = closed(&response, &["apiVersion", "schemaVersion", "authority", "items", "cursor", "nextCursor", "hasMore", "requestId"])?;
    let items = object["items"].as_array().ok_or(INVALID)?;
    let has_more = object["hasMore"].as_bool().ok_or(INVALID)?;
    let next = object["nextCursor"].as_str().ok_or(INVALID)?;
    if object["cursor"].as_str() != Some(cursor) || items.len() > limit
        || (has_more && items.len() != limit) || next.len() > 4096 || next.contains(['\r', '\n'])
        || (items.is_empty() && next != cursor) || (!items.is_empty() && next == cursor)
    {
        return Err(INVALID);
    }
    let mut identities = BTreeSet::new();
    for item in items {
        let resource = validate_resource(item, resource_type)?;
        if !identities.insert((text(resource, "id")?, text(resource, "version")?)) {
            return Err(INVALID);
        }
    }
    Ok(receipt(response, "FORMAT_ONLY"))
}

/// The expected digest must come from an independent trusted source, not this response.
pub fn show(endpoint: &str, resource_type: &str, id: &str, version: &str, expected_digest: &str, timeout: Duration) -> ReadResult {
    validate_type(resource_type)?;
    if !valid_text(id) || !valid_text(version) || matches!(id, "." | "..") || !is_sha256_digest(expected_digest) {
        return Err("REGISTRY_PIN_REQUIRED");
    }
    let response = read(endpoint, &format!("{resource_type}/{}?version={}", encode_query_component(id), encode_query_component(version)), timeout)?;
    let object = closed(&response, &["apiVersion", "schemaVersion", "authority", "resource", "requestId"])?;
    let resource = validate_resource(&object["resource"], resource_type)?;
    if resource["id"] != id || resource["version"] != version {
        return Err("REGISTRY_IDENTITY_MISMATCH");
    }
    if resource["digest"] != expected_digest {
        return Err("REGISTRY_DIGEST_MISMATCH");
    }
    Ok(receipt(response, "PIN_MATCH"))
}

fn receipt(response: Value, verification: &str) -> Value {
    json!({"status": "RECEIVED_ONLY", "authority": "UNKNOWN", "digestVerification": verification, "registry": response})
}

fn read(endpoint: &str, suffix: &str, timeout: Duration) -> ReadResult {
    let (status, response) = registry_response(endpoint, suffix, timeout).map_err(|error| match error {
        ClientError::InvalidEndpoint(_) => "REGISTRY_ENDPOINT_INVALID",
        _ => "REGISTRY_UNAVAILABLE",
    })?;
    if status != 200 {
        // Do not reflect untrusted response messages, credentials or bodies into client output.
        return Err(match (status, response.get("code").and_then(Value::as_str)) {
            (409, Some("RESOURCE_VERSION_REQUIRED")) => "RESOURCE_VERSION_REQUIRED",
            (404, Some("NOT_FOUND")) => "NOT_FOUND",
            _ => "REGISTRY_HTTP_ERROR",
        });
    }
    if response.get("apiVersion").and_then(Value::as_str) != Some(CLIENT_API_VERSION)
        || response.get("schemaVersion").and_then(Value::as_str) != Some("asset-registry-v1")
        || response.get("authority").and_then(Value::as_str) != Some("UNKNOWN")
        || response.get("requestId").and_then(Value::as_str) != Some("registry-read")
    {
        return Err(INVALID);
    }
    Ok(response)
}

fn validate_type(resource_type: &str) -> Result<(), &'static str> {
    if matches!(resource_type, "rules" | "standards") { Ok(()) } else { Err("REGISTRY_TYPE_INVALID") }
}

fn closed<'a>(value: &'a Value, fields: &[&str]) -> Result<&'a Map<String, Value>, &'static str> {
    let object = value.as_object().ok_or(INVALID)?;
    require_exact_fields(object, fields).map_err(|_| INVALID)?;
    Ok(object)
}

fn validate_resource<'a>(value: &'a Value, resource_type: &str) -> Result<&'a Map<String, Value>, &'static str> {
    let object = closed(value, &["type", "id", "version", "state", "digest"])?;
    for field in ["id", "version", "state"] { text(object, field)?; }
    if object["type"] != resource_type || !is_sha256_digest(text(object, "digest")?) { return Err(INVALID); }
    Ok(object)
}

fn valid_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

fn text<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a str, &'static str> {
    object.get(field).and_then(Value::as_str).filter(|value| valid_text(value)).ok_or(INVALID)
}
