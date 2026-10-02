//! Digest-only evidence package and governance adjudication boundary.
//!
//! This crate intentionally stores no source, complete Finding, SCM state or
//! credential. It binds only identities, metric status, provenance and
//! artifact digests. Missing or invalid evidence can never become `GO`.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const PILOT_EVIDENCE_PACKAGE_VERSION: &str = "pilot-evidence-package-v1";
pub const PILOT_ADJUDICATION_VERSION: &str = "pilot-adjudication-v1";
pub const PACKAGE_FORMAT: &str = "erf-pilot-evidence-package-v1";
pub const INPUT_FORMAT: &str = "erf-pilot-evidence-input-v1";
pub const ADJUDICATION_FORMAT: &str = "erf-pilot-adjudication-v1";
pub const DECISION_OWNER: &str = "platform-engineering";
pub const APPROVED_CONTRACT_ID: &str = "pilot-success-contract-v0";
pub const APPROVED_CONTRACT_VERSION: &str = "1.1.0";
pub const APPROVED_WINDOW_START_EPOCH_SECS: u64 = 1_790_899_200;
pub const APPROVED_WINDOW_END_EPOCH_SECS: u64 = 1_791_503_999;

pub const REQUIRED_METRICS: [&str; 11] = [
    "author-active-time",
    "precision",
    "recall",
    "false-positive-regression",
    "complete-rate",
    "inconclusive-rate",
    "scan-latency",
    "resource-consumption",
    "new-violation-trend",
    "historical-debt-trend",
    "false-pass-count",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceStatus {
    Pass,
    Fail,
    Inconclusive,
}

impl EvidenceStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Inconclusive => "INCONCLUSIVE",
        }
    }

    pub fn parse(value: &str) -> Result<Self, EvidenceError> {
        match value {
            "PASS" => Ok(Self::Pass),
            "FAIL" => Ok(Self::Fail),
            "INCONCLUSIVE" => Ok(Self::Inconclusive),
            _ => Err(EvidenceError::InvalidField("status")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceCompleteness {
    Complete,
    Missing,
    Invalid,
    Conflicting,
    Tampered,
    Incomplete,
}

impl EvidenceCompleteness {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "COMPLETE",
            Self::Missing => "MISSING",
            Self::Invalid => "INVALID",
            Self::Conflicting => "CONFLICTING",
            Self::Tampered => "TAMPERED",
            Self::Incomplete => "INCOMPLETE",
        }
    }

    pub fn parse(value: &str) -> Result<Self, EvidenceError> {
        match value {
            "COMPLETE" => Ok(Self::Complete),
            "MISSING" => Ok(Self::Missing),
            "INVALID" => Ok(Self::Invalid),
            "CONFLICTING" => Ok(Self::Conflicting),
            "TAMPERED" => Ok(Self::Tampered),
            "INCOMPLETE" => Ok(Self::Incomplete),
            _ => Err(EvidenceError::InvalidField("completeness")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PilotAdjudicationDecision {
    Go,
    NoGo,
    Inconclusive,
}

impl PilotAdjudicationDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Go => "GO",
            Self::NoGo => "NO_GO",
            Self::Inconclusive => "INCONCLUSIVE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub metric_id: String,
    pub provenance: String,
    pub window_start: u64,
    pub window_end: u64,
    pub completeness: EvidenceCompleteness,
    pub artifact_digest: [u8; 32],
    pub status: EvidenceStatus,
}

impl EvidenceRecord {
    pub fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        append_string(&mut payload, &self.metric_id);
        append_string(&mut payload, &self.provenance);
        payload.extend_from_slice(&self.window_start.to_be_bytes());
        payload.extend_from_slice(&self.window_end.to_be_bytes());
        payload.push(completeness_byte(self.completeness));
        payload.extend_from_slice(&self.artifact_digest);
        payload.push(status_byte(self.status));
        payload
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PilotEvidencePackage {
    pub contract_id: String,
    pub contract_version: String,
    pub contract_digest: [u8; 32],
    pub window_start: u64,
    pub window_end: u64,
    pub sample_digest: [u8; 32],
    pub records: Vec<EvidenceRecord>,
    pub package_digest: [u8; 32],
}

impl PilotEvidencePackage {
    pub fn new(
        contract_id: impl Into<String>,
        contract_version: impl Into<String>,
        contract_digest: [u8; 32],
        window_start: u64,
        window_end: u64,
        sample_digest: [u8; 32],
        records: Vec<EvidenceRecord>,
    ) -> Self {
        Self {
            contract_id: contract_id.into(),
            contract_version: contract_version.into(),
            contract_digest,
            window_start,
            window_end,
            sample_digest,
            records,
            package_digest: [0; 32],
        }
    }

    pub fn content_digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_payload()).into()
    }

    pub fn aggregate_status(&self) -> Result<EvidenceStatus, EvidenceError> {
        self.validate_structure()?;
        let mut by_metric = BTreeMap::<&str, &EvidenceRecord>::new();
        for record in &self.records {
            if by_metric.insert(&record.metric_id, record).is_some() {
                return Err(EvidenceError::ConflictingMetric(record.metric_id.clone()));
            }
        }
        let mut has_fail = false;
        let mut has_inconclusive = false;
        for metric in REQUIRED_METRICS {
            let Some(record) = by_metric.get(metric) else {
                has_inconclusive = true;
                continue;
            };
            if record.status == EvidenceStatus::Fail {
                has_fail = true;
            }
            if record.status == EvidenceStatus::Inconclusive
                || record.completeness != EvidenceCompleteness::Complete
            {
                has_inconclusive = true;
            }
        }
        if has_fail {
            Ok(EvidenceStatus::Fail)
        } else if has_inconclusive {
            Ok(EvidenceStatus::Inconclusive)
        } else {
            Ok(EvidenceStatus::Pass)
        }
    }

    pub fn seal(&mut self) -> Result<(), EvidenceError> {
        self.validate_structure()?;
        self.records.sort_by(|left, right| {
            left.metric_id
                .cmp(&right.metric_id)
                .then_with(|| left.provenance.cmp(&right.provenance))
        });
        self.package_digest = self.content_digest();
        Ok(())
    }

    pub fn validate(&self) -> Result<EvidenceStatus, EvidenceError> {
        self.validate_structure()?;
        if self.package_digest == [0; 32] {
            return Err(EvidenceError::UnsignedPackage);
        }
        if self.package_digest != self.content_digest() {
            return Err(EvidenceError::TamperedPackage);
        }
        self.aggregate_status()
    }

    pub fn validate_against(
        &self,
        expected_contract_digest: [u8; 32],
        expected_sample_digest: [u8; 32],
    ) -> Result<EvidenceStatus, EvidenceError> {
        self.validate_bindings(expected_contract_digest, expected_sample_digest)?;
        self.validate()
    }

    pub fn validate_bindings(
        &self,
        expected_contract_digest: [u8; 32],
        expected_sample_digest: [u8; 32],
    ) -> Result<(), EvidenceError> {
        self.validate_structure()?;
        if self.contract_id != APPROVED_CONTRACT_ID || self.contract_version != APPROVED_CONTRACT_VERSION {
            return Err(EvidenceError::ApprovedContractMetadataMismatch);
        }
        if self.window_start != APPROVED_WINDOW_START_EPOCH_SECS
            || self.window_end != APPROVED_WINDOW_END_EPOCH_SECS
        {
            return Err(EvidenceError::ApprovedWindowMismatch);
        }
        if self.contract_digest != expected_contract_digest {
            return Err(EvidenceError::ContractMismatch);
        }
        if self.sample_digest != expected_sample_digest {
            return Err(EvidenceError::SampleMismatch);
        }
        Ok(())
    }

    fn adjudicate(
        &self,
        owner: &str,
        rationale: impl Into<String>,
    ) -> Result<PilotAdjudication, EvidenceError> {
        if owner != DECISION_OWNER {
            return Err(EvidenceError::UnauthorizedOwner);
        }
        let aggregate = self.validate()?;
        let decision = match aggregate {
            EvidenceStatus::Pass => PilotAdjudicationDecision::Go,
            EvidenceStatus::Fail => PilotAdjudicationDecision::NoGo,
            EvidenceStatus::Inconclusive => PilotAdjudicationDecision::Inconclusive,
        };
        let mut adjudication = PilotAdjudication {
            package_digest: self.package_digest,
            owner: owner.into(),
            decision,
            rationale: rationale.into(),
            adjudication_digest: [0; 32],
        };
        adjudication.validate_fields()?;
        adjudication.adjudication_digest = adjudication.content_digest();
        Ok(adjudication)
    }

    pub fn adjudicate_against(
        &self,
        owner: &str,
        rationale: impl Into<String>,
        expected_contract_digest: [u8; 32],
        expected_sample_digest: [u8; 32],
    ) -> Result<PilotAdjudication, EvidenceError> {
        self.validate_against(expected_contract_digest, expected_sample_digest)?;
        self.adjudicate(owner, rationale)
    }

    fn validate_structure(&self) -> Result<(), EvidenceError> {
        validate_text("contract_id", &self.contract_id)?;
        validate_text("contract_version", &self.contract_version)?;
        if self.contract_digest == [0; 32] {
            return Err(EvidenceError::InvalidField("contract_digest"));
        }
        if self.sample_digest == [0; 32] {
            return Err(EvidenceError::InvalidField("sample_digest"));
        }
        if self.window_start == 0 || self.window_end <= self.window_start {
            return Err(EvidenceError::InvalidField("observation_window"));
        }
        let mut seen = BTreeSet::new();
        for record in &self.records {
            validate_text("metric_id", &record.metric_id)?;
            if !REQUIRED_METRICS.contains(&record.metric_id.as_str()) {
                return Err(EvidenceError::UnsupportedMetric(record.metric_id.clone()));
            }
            if !seen.insert(record.metric_id.as_str()) {
                return Err(EvidenceError::ConflictingMetric(record.metric_id.clone()));
            }
            validate_text("provenance", &record.provenance)?;
            if record.window_start != self.window_start || record.window_end != self.window_end {
                return Err(EvidenceError::WindowMismatch(record.metric_id.clone()));
            }
            if record.artifact_digest == [0; 32] {
                return Err(EvidenceError::InvalidArtifact(record.metric_id.clone()));
            }
            if record.status == EvidenceStatus::Pass
                && record.completeness != EvidenceCompleteness::Complete
            {
                return Err(EvidenceError::PassWithIncompleteEvidence(record.metric_id.clone()));
            }
        }
        Ok(())
    }

    fn canonical_payload(&self) -> Vec<u8> {
        let mut payload = PILOT_EVIDENCE_PACKAGE_VERSION.as_bytes().to_vec();
        payload.push(0);
        append_string(&mut payload, &self.contract_id);
        append_string(&mut payload, &self.contract_version);
        payload.extend_from_slice(&self.contract_digest);
        payload.extend_from_slice(&self.window_start.to_be_bytes());
        payload.extend_from_slice(&self.window_end.to_be_bytes());
        payload.extend_from_slice(&self.sample_digest);
        let mut records = self.records.clone();
        records.sort_by(|left, right| {
            left.metric_id
                .cmp(&right.metric_id)
                .then_with(|| left.provenance.cmp(&right.provenance))
        });
        append_u64(&mut payload, records.len() as u64);
        for record in records {
            let encoded = record.canonical_payload();
            append_u64(&mut payload, encoded.len() as u64);
            payload.extend_from_slice(&encoded);
        }
        payload
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PilotAdjudication {
    package_digest: [u8; 32],
    owner: String,
    decision: PilotAdjudicationDecision,
    rationale: String,
    adjudication_digest: [u8; 32],
}

impl PilotAdjudication {
    pub fn package_digest(&self) -> [u8; 32] { self.package_digest }
    pub fn owner(&self) -> &str { &self.owner }
    pub fn decision(&self) -> PilotAdjudicationDecision { self.decision }
    pub fn rationale(&self) -> &str { &self.rationale }
    pub fn adjudication_digest(&self) -> [u8; 32] { self.adjudication_digest }

    pub fn fail_closed(package_digest: [u8; 32], rationale: impl Into<String>) -> Result<Self, EvidenceError> {
        if package_digest == [0; 32] {
            return Err(EvidenceError::InvalidField("package_digest"));
        }
        let mut adjudication = Self {
            package_digest,
            owner: DECISION_OWNER.into(),
            decision: PilotAdjudicationDecision::Inconclusive,
            rationale: rationale.into(),
            adjudication_digest: [0; 32],
        };
        adjudication.validate_fields()?;
        adjudication.adjudication_digest = adjudication.content_digest();
        Ok(adjudication)
    }

    pub fn content_digest(&self) -> [u8; 32] {
        let mut payload = PILOT_ADJUDICATION_VERSION.as_bytes().to_vec();
        payload.push(0);
        payload.extend_from_slice(&self.package_digest);
        append_string(&mut payload, &self.owner);
        payload.push(decision_byte(self.decision));
        append_string(&mut payload, &self.rationale);
        Sha256::digest(payload).into()
    }

    pub fn validate(&self) -> Result<(), EvidenceError> {
        self.validate_fields()?;
        if self.adjudication_digest == [0; 32]
            || self.adjudication_digest != self.content_digest()
        {
            return Err(EvidenceError::TamperedAdjudication);
        }
        Ok(())
    }

    fn validate_fields(&self) -> Result<(), EvidenceError> {
        if self.package_digest == [0; 32] {
            return Err(EvidenceError::InvalidField("package_digest"));
        }
        if self.owner != DECISION_OWNER {
            return Err(EvidenceError::UnauthorizedOwner);
        }
        validate_text("rationale", &self.rationale)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EvidenceError {
    #[error("evidence field is invalid: {0}")]
    InvalidField(&'static str),
    #[error("unsupported evidence metric: {0}")]
    UnsupportedMetric(String),
    #[error("evidence metric is duplicated or conflicting: {0}")]
    ConflictingMetric(String),
    #[error("evidence window does not match package window: {0}")]
    WindowMismatch(String),
    #[error("evidence artifact digest is invalid: {0}")]
    InvalidArtifact(String),
    #[error("PASS evidence is not complete: {0}")]
    PassWithIncompleteEvidence(String),
    #[error("evidence package is not sealed")]
    UnsignedPackage,
    #[error("evidence package digest does not match its content")]
    TamperedPackage,
    #[error("evidence package contract digest does not match the approved contract")]
    ContractMismatch,
    #[error("evidence package sample digest does not match the registered sample")]
    SampleMismatch,
    #[error("evidence package contract id/version does not match the approved contract")]
    ApprovedContractMetadataMismatch,
    #[error("evidence package window does not match the approved observation window")]
    ApprovedWindowMismatch,
    #[error("adjudication digest does not match its content")]
    TamperedAdjudication,
    #[error("only the platform-engineering governance owner may adjudicate")]
    UnauthorizedOwner,
    #[error("evidence input has invalid format")]
    InvalidFormat,
    #[error("evidence input is missing field: {0}")]
    MissingField(&'static str),
    #[error("evidence input has an unknown field: {0}")]
    UnknownField(String),
    #[error("evidence input has an invalid digest")]
    InvalidDigest,
    #[error("evidence input number is invalid: {0}")]
    InvalidNumber(&'static str),
}

pub fn parse_package(text: &str) -> Result<PilotEvidencePackage, EvidenceError> {
    let mut format = None;
    let mut contract_id = None;
    let mut contract_version = None;
    let mut contract_digest = None;
    let mut window_start = None;
    let mut window_end = None;
    let mut sample_digest = None;
    let mut package_digest = [0; 32];
    let mut package_digest_seen = false;
    let mut records = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        if let Some(value) = line.strip_prefix("format=") {
            if format.is_some() {
                return Err(EvidenceError::UnknownField("duplicate format".into()));
            }
            format = Some(value);
        } else if let Some(value) = line.strip_prefix("contract-id=") {
            set_text(&mut contract_id, value, "contract-id")?;
        } else if let Some(value) = line.strip_prefix("contract-version=") {
            set_text(&mut contract_version, value, "contract-version")?;
        } else if let Some(value) = line.strip_prefix("contract-digest=") {
            set_digest(&mut contract_digest, value)?;
        } else if let Some(value) = line.strip_prefix("window-start=") {
            set_number(&mut window_start, value, "window-start")?;
        } else if let Some(value) = line.strip_prefix("window-end=") {
            set_number(&mut window_end, value, "window-end")?;
        } else if let Some(value) = line.strip_prefix("sample-digest=") {
            set_digest(&mut sample_digest, value)?;
        } else if let Some(value) = line.strip_prefix("package-digest=") {
            if package_digest_seen {
                return Err(EvidenceError::UnknownField("duplicate package-digest".into()));
            }
            package_digest = parse_hex_digest(value)?;
            package_digest_seen = true;
        } else if let Some(value) = line.strip_prefix("metric=") {
            records.push(parse_record(value)?);
        } else if line.starts_with("aggregate=") {
            // Aggregate is explanatory output, never an input to trust.
        } else {
            return Err(EvidenceError::UnknownField(line.into()));
        }
    }
    let format = format.ok_or(EvidenceError::MissingField("format"))?;
    if format != INPUT_FORMAT && format != PACKAGE_FORMAT {
        return Err(EvidenceError::InvalidFormat);
    }
    Ok(PilotEvidencePackage {
        contract_id: contract_id.ok_or(EvidenceError::MissingField("contract-id"))?,
        contract_version: contract_version.ok_or(EvidenceError::MissingField("contract-version"))?,
        contract_digest: contract_digest.ok_or(EvidenceError::MissingField("contract-digest"))?,
        window_start: window_start.ok_or(EvidenceError::MissingField("window-start"))?,
        window_end: window_end.ok_or(EvidenceError::MissingField("window-end"))?,
        sample_digest: sample_digest.ok_or(EvidenceError::MissingField("sample-digest"))?,
        records,
        package_digest,
    })
}

pub fn serialize_package(package: &PilotEvidencePackage) -> Result<String, EvidenceError> {
    let aggregate = package.validate()?;
    let mut output = String::new();
    output.push_str(&format!("format={PACKAGE_FORMAT}\n"));
    output.push_str(&format!("contract-id={}\n", package.contract_id));
    output.push_str(&format!("contract-version={}\n", package.contract_version));
    output.push_str(&format!("contract-digest={}\n", hex_digest(&package.contract_digest)));
    output.push_str(&format!("window-start={}\n", package.window_start));
    output.push_str(&format!("window-end={}\n", package.window_end));
    output.push_str(&format!("sample-digest={}\n", hex_digest(&package.sample_digest)));
    let mut records = package.records.clone();
    records.sort_by(|left, right| left.metric_id.cmp(&right.metric_id));
    for record in records {
        output.push_str(&format!(
            "metric={}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            record.metric_id,
            record.provenance,
            record.window_start,
            record.window_end,
            record.completeness.as_str(),
            record.status.as_str(),
            hex_digest(&record.artifact_digest),
        ));
    }
    output.push_str(&format!("aggregate={}\n", aggregate.as_str()));
    output.push_str(&format!("package-digest={}\n", hex_digest(&package.package_digest)));
    Ok(output)
}

pub fn serialize_adjudication(adjudication: &PilotAdjudication) -> Result<String, EvidenceError> {
    adjudication.validate()?;
    Ok(format!(
        "format={ADJUDICATION_FORMAT}\npackage-digest={}\nowner={}\ndecision={}\nrationale={}\nadjudication-digest={}\n",
        hex_digest(&adjudication.package_digest),
        adjudication.owner,
        adjudication.decision.as_str(),
        adjudication.rationale,
        hex_digest(&adjudication.adjudication_digest),
    ))
}

pub fn hex_digest(value: &[u8; 32]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn parse_hex_digest(value: &str) -> Result<[u8; 32], EvidenceError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(EvidenceError::InvalidDigest);
    }
    let mut digest = [0; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        digest[index] = (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]);
    }
    Ok(digest)
}

fn parse_record(value: &str) -> Result<EvidenceRecord, EvidenceError> {
    let fields = value.split('\t').collect::<Vec<_>>();
    if fields.len() != 7 {
        return Err(EvidenceError::InvalidFormat);
    }
    Ok(EvidenceRecord {
        metric_id: fields[0].into(),
        provenance: fields[1].into(),
        window_start: fields[2].parse().map_err(|_| EvidenceError::InvalidNumber("metric window-start"))?,
        window_end: fields[3].parse().map_err(|_| EvidenceError::InvalidNumber("metric window-end"))?,
        completeness: EvidenceCompleteness::parse(fields[4])?,
        status: EvidenceStatus::parse(fields[5])?,
        artifact_digest: parse_hex_digest(fields[6])?,
    })
}

fn set_text(slot: &mut Option<String>, value: &str, field: &'static str) -> Result<(), EvidenceError> {
    if slot.is_some() {
        return Err(EvidenceError::UnknownField(format!("duplicate {field}")));
    }
    *slot = Some(value.into());
    Ok(())
}

fn set_digest(slot: &mut Option<[u8; 32]>, value: &str) -> Result<(), EvidenceError> {
    if slot.is_some() {
        return Err(EvidenceError::UnknownField("duplicate digest".into()));
    }
    *slot = Some(parse_hex_digest(value)?);
    Ok(())
}

fn set_number(slot: &mut Option<u64>, value: &str, field: &'static str) -> Result<(), EvidenceError> {
    if slot.is_some() {
        return Err(EvidenceError::UnknownField(format!("duplicate {field}")));
    }
    *slot = Some(value.parse().map_err(|_| EvidenceError::InvalidNumber(field))?);
    Ok(())
}

fn validate_text(field: &'static str, value: &str) -> Result<(), EvidenceError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(EvidenceError::InvalidField(field));
    }
    Ok(())
}

fn append_string(payload: &mut Vec<u8>, value: &str) {
    append_u64(payload, value.len() as u64);
    payload.extend_from_slice(value.as_bytes());
}

fn append_u64(payload: &mut Vec<u8>, value: u64) {
    payload.extend_from_slice(&value.to_be_bytes());
}

fn status_byte(value: EvidenceStatus) -> u8 {
    match value {
        EvidenceStatus::Pass => 0,
        EvidenceStatus::Fail => 1,
        EvidenceStatus::Inconclusive => 2,
    }
}

fn completeness_byte(value: EvidenceCompleteness) -> u8 {
    match value {
        EvidenceCompleteness::Complete => 0,
        EvidenceCompleteness::Missing => 1,
        EvidenceCompleteness::Invalid => 2,
        EvidenceCompleteness::Conflicting => 3,
        EvidenceCompleteness::Tampered => 4,
        EvidenceCompleteness::Incomplete => 5,
    }
}

fn decision_byte(value: PilotAdjudicationDecision) -> u8 {
    match value {
        PilotAdjudicationDecision::Go => 0,
        PilotAdjudicationDecision::NoGo => 1,
        PilotAdjudicationDecision::Inconclusive => 2,
    }
}

fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => unreachable!("parse_hex_digest validates hexadecimal input"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn records(status: EvidenceStatus, completeness: EvidenceCompleteness) -> Vec<EvidenceRecord> {
        REQUIRED_METRICS
            .iter()
            .enumerate()
            .map(|(index, metric_id)| EvidenceRecord {
                metric_id: (*metric_id).into(),
                provenance: format!("artifact-{index}"),
                window_start: 1_790_899_200,
                window_end: 1_791_503_999,
                completeness,
                artifact_digest: digest(index as u8 + 1),
                status,
            })
            .collect()
    }

    fn package(status: EvidenceStatus, completeness: EvidenceCompleteness) -> PilotEvidencePackage {
        let mut package = PilotEvidencePackage::new(
            "pilot-success-contract-v0",
            "1.1.0",
            digest(7),
            1_790_899_200,
            1_791_503_999,
            digest(8),
            records(status, completeness),
        );
        package.seal().unwrap();
        package
    }

    #[test]
    fn complete_pass_package_can_only_be_go() {
        let package = package(EvidenceStatus::Pass, EvidenceCompleteness::Complete);
        assert_eq!(package.validate().unwrap(), EvidenceStatus::Pass);
        let adjudication = package.adjudicate_against(DECISION_OWNER, "all required evidence passed", digest(7), digest(8)).unwrap();
        assert_eq!(adjudication.decision(), PilotAdjudicationDecision::Go);
        adjudication.validate().unwrap();
    }

    #[test]
    fn status_precedence_keeps_fail_above_missing_data() {
        let mut package = package(EvidenceStatus::Pass, EvidenceCompleteness::Complete);
        package.records[0].status = EvidenceStatus::Fail;
        package.records[1].status = EvidenceStatus::Inconclusive;
        package.records.pop();
        package.seal().unwrap();
        assert_eq!(package.validate().unwrap(), EvidenceStatus::Fail);
        assert_eq!(package.adjudicate_against(DECISION_OWNER, "known failure", digest(7), digest(8)).unwrap().decision(), PilotAdjudicationDecision::NoGo);
    }

    #[test]
    fn missing_incomplete_or_invalid_inputs_never_go() {
        let mut missing = package(EvidenceStatus::Pass, EvidenceCompleteness::Complete);
        missing.records.pop();
        missing.seal().unwrap();
        assert_eq!(missing.validate().unwrap(), EvidenceStatus::Inconclusive);
        assert_eq!(missing.adjudicate_against(DECISION_OWNER, "missing metric", digest(7), digest(8)).unwrap().decision(), PilotAdjudicationDecision::Inconclusive);

        let mut incomplete = package(EvidenceStatus::Inconclusive, EvidenceCompleteness::Incomplete);
        incomplete.seal().unwrap();
        assert_eq!(incomplete.validate().unwrap(), EvidenceStatus::Inconclusive);
        assert_eq!(incomplete.adjudicate_against(DECISION_OWNER, "incomplete", digest(7), digest(8)).unwrap().decision(), PilotAdjudicationDecision::Inconclusive);
    }

    #[test]
    fn tampering_and_wrong_owner_fail_closed() {
        let mut tampered = package(EvidenceStatus::Pass, EvidenceCompleteness::Complete);
        tampered.records[0].provenance.push_str("-tampered");
        assert_eq!(tampered.validate(), Err(EvidenceError::TamperedPackage));

        let package = package(EvidenceStatus::Pass, EvidenceCompleteness::Complete);
        assert_eq!(package.adjudicate_against("operator", "not governance", digest(7), digest(8)), Err(EvidenceError::UnauthorizedOwner));
    }

    #[test]
    fn contract_and_sample_identity_are_bound_before_adjudication() {
        let package = package(EvidenceStatus::Pass, EvidenceCompleteness::Complete);
        assert_eq!(package.validate_against(digest(9), digest(8)), Err(EvidenceError::ContractMismatch));
        assert_eq!(package.validate_against(digest(7), digest(9)), Err(EvidenceError::SampleMismatch));
        assert_eq!(package.validate_against(digest(7), digest(8)).unwrap(), EvidenceStatus::Pass);
    }

    #[test]
    fn package_round_trip_preserves_digest_only_identity() {
        let package = package(EvidenceStatus::Pass, EvidenceCompleteness::Complete);
        let text = serialize_package(&package).unwrap();
        let restored = parse_package(&text).unwrap();
        assert_eq!(restored, package);
        assert_eq!(restored.validate().unwrap(), EvidenceStatus::Pass);
    }

    #[test]
    fn approved_values_match_independent_contract_and_yaml_oracles() {
        assert_eq!(APPROVED_CONTRACT_ID, "pilot-success-contract-v0");
        assert_eq!(APPROVED_CONTRACT_VERSION, "1.1.0");
        assert_eq!(APPROVED_WINDOW_START_EPOCH_SECS, 1_790_899_200);
        assert_eq!(APPROVED_WINDOW_END_EPOCH_SECS, 1_791_503_999);
        assert_eq!(APPROVED_WINDOW_END_EPOCH_SECS - APPROVED_WINDOW_START_EPOCH_SECS + 1, 604_800);
        let contract = include_str!("../../../specs/pilot/pilot-success-contract.yaml");
        let plan = include_str!("../../../specs/pilot/pilot-observation-plan.yaml");
        let evidence = include_str!("../../../specs/pilot/pilot-evidence-package.yaml");
        for yaml in [contract, plan, evidence] {
            assert!(yaml.contains("  version: 1.1.0\n"));
        }
        assert!(contract.contains("  id: pilot-success-contract-v0\n"));
        assert!(contract.contains("    start: \"2026-10-02\"\n    end: \"2026-10-08\""));
        assert!(plan.contains("  contract: pilot-success-contract-v0\n"));
        assert!(plan.contains("    start: 2026-10-02T00:00:00Z\n    end: 2026-10-08T23:59:59Z"));
        assert!(evidence.contains("    approved: 2026-10-02T00:00:00Z/2026-10-08T23:59:59Z\n"));
        for yaml in [plan, evidence] {
            assert!(yaml.contains("    boundary: inclusive\n    timezone: UTC\n"));
        }
        assert!(evidence.contains("  format: erf-pilot-evidence-package-v1\n"));
        assert_eq!(package(EvidenceStatus::Pass, EvidenceCompleteness::Complete)
            .validate_against(digest(7), digest(8)), Ok(EvidenceStatus::Pass));
    }

    #[test]
    fn old_contract_version_and_window_are_rejected_on_both_entry_paths() {
        for (version, start, end, expected) in [
            ("1.0.0", 1_790_899_200, 1_791_503_999, EvidenceError::ApprovedContractMetadataMismatch),
            ("1.1.0", 1_790_812_800, 1_793_491_199, EvidenceError::ApprovedWindowMismatch),
            ("1.0.0", 1_790_812_800, 1_793_491_199, EvidenceError::ApprovedContractMetadataMismatch),
        ] {
            let mut old = package(EvidenceStatus::Pass, EvidenceCompleteness::Complete);
            old.contract_version = version.into();
            old.window_start = start;
            old.window_end = end;
            for record in &mut old.records {
                record.window_start = start;
                record.window_end = end;
            }
            // A self-consistent historical package is still not a current approved package.
            old.seal().unwrap();
            assert_eq!(old.validate_bindings(digest(7), digest(8)).as_ref().err(), Some(&expected));
            assert_eq!(old.adjudicate_against(DECISION_OWNER, "historical fixture", digest(7), digest(8)), Err(expected));
        }
    }

    #[test]
    fn evidence_record_window_must_equal_the_package_window() {
        for change_start in [true, false] {
            let mut mismatched = package(EvidenceStatus::Pass, EvidenceCompleteness::Complete);
            if change_start {
                mismatched.records[0].window_start = 1_790_899_201;
            } else {
                mismatched.records[0].window_end = 1_791_503_998;
            }
            let expected = EvidenceError::WindowMismatch("author-active-time".into());
            assert_eq!(mismatched.validate_bindings(digest(7), digest(8)).as_ref().err(), Some(&expected));
            assert_eq!(mismatched.seal(), Err(expected));
        }
    }
}
