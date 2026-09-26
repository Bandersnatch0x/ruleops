use erf_pilot_evidence::{
    parse_package, serialize_adjudication, serialize_package, EvidenceError, PilotAdjudication,
    PilotAdjudicationDecision, DECISION_OWNER,
};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("pilot evidence failed: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<u8, EvidenceError> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("package") => {
            let input = args.next().ok_or(EvidenceError::MissingField("input"))?;
            let output = args.next().ok_or(EvidenceError::MissingField("output"))?;
            let (contract_digest, sample_digest) = parse_bindings(args.collect())?;
            let mut package = parse_package(&fs::read_to_string(&input).map_err(|_| EvidenceError::InvalidFormat)?)?;
            package.validate_bindings(contract_digest, sample_digest)?;
            package.seal()?;
            write_atomic(Path::new(&output), serialize_package(&package)?)?;
            println!("pilot-evidence-package={} aggregate={} digest={}", output, package.aggregate_status()?.as_str(), erf_pilot_evidence::hex_digest(&package.package_digest));
            Ok(0)
        }
        Some("adjudicate") => {
            let input = args.next().ok_or(EvidenceError::MissingField("package"))?;
            let output = args.next().ok_or(EvidenceError::MissingField("output"))?;
            let remaining = args.collect::<Vec<_>>();
            let (owner, contract_digest, sample_digest) = parse_adjudication_options(remaining)?;
            let package_text = fs::read_to_string(&input).map_err(|_| EvidenceError::InvalidFormat)?;
            let package = match parse_package(&package_text) {
                Ok(value) => value,
                Err(error) => {
                    let adjudication = fail_closed_text_adjudication(&package_text, &error);
                    write_atomic(Path::new(&output), serialize_adjudication(&adjudication)?)?;
                    println!("pilot-adjudication={} decision={} digest={}", output, adjudication.decision().as_str(), erf_pilot_evidence::hex_digest(&adjudication.adjudication_digest()));
                    return Ok(3);
                }
            };
            let adjudication = match package.adjudicate_against(&owner, "digest-only pilot evidence review", contract_digest, sample_digest) {
                Ok(value) => value,
                Err(error) => fail_closed_adjudication(&package, &owner, error),
            };
            write_atomic(Path::new(&output), serialize_adjudication(&adjudication)?)?;
            println!("pilot-adjudication={} decision={} digest={}", output, adjudication.decision().as_str(), erf_pilot_evidence::hex_digest(&adjudication.adjudication_digest()));
            Ok(match adjudication.decision() {
                PilotAdjudicationDecision::Go => 0,
                PilotAdjudicationDecision::NoGo => 1,
                PilotAdjudicationDecision::Inconclusive => 3,
            })
        }
        _ => {
            eprintln!("usage: erf-pilot-evidence package INPUT OUTPUT --contract-digest HEX --sample-digest HEX | adjudicate PACKAGE OUTPUT --contract-digest HEX --sample-digest HEX [--owner OWNER]");
            Err(EvidenceError::InvalidFormat)
        }
    }
}

fn parse_bindings(args: Vec<String>) -> Result<([u8; 32], [u8; 32]), EvidenceError> {
    let mut contract = None;
    let mut sample = None;
    let mut index = 0;
    while index < args.len() {
        let flag = args.get(index).ok_or(EvidenceError::InvalidFormat)?;
        let value = args.get(index + 1).ok_or(EvidenceError::InvalidFormat)?;
        match flag.as_str() {
            "--contract-digest" if contract.is_none() => contract = Some(erf_pilot_evidence::parse_hex_digest(value)?),
            "--sample-digest" if sample.is_none() => sample = Some(erf_pilot_evidence::parse_hex_digest(value)?),
            _ => return Err(EvidenceError::InvalidFormat),
        }
        index += 2;
    }
    Ok((
        contract.ok_or(EvidenceError::MissingField("contract-digest"))?,
        sample.ok_or(EvidenceError::MissingField("sample-digest"))?,
    ))
}

fn parse_adjudication_options(args: Vec<String>) -> Result<(String, [u8; 32], [u8; 32]), EvidenceError> {
    let mut owner = DECISION_OWNER.into();
    let mut bindings = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args.get(index).ok_or(EvidenceError::InvalidFormat)?;
        let value = args.get(index + 1).ok_or(EvidenceError::InvalidFormat)?;
        match flag.as_str() {
            "--owner" if owner == DECISION_OWNER => owner = value.clone(),
            "--contract-digest" | "--sample-digest" => {
                bindings.push(flag.clone());
                bindings.push(value.clone());
            }
            _ => return Err(EvidenceError::InvalidFormat),
        }
        index += 2;
    }
    let (contract, sample) = parse_bindings(bindings)?;
    Ok((owner, contract, sample))
}

fn fail_closed_adjudication(
    package: &erf_pilot_evidence::PilotEvidencePackage,
    owner: &str,
    error: EvidenceError,
) -> PilotAdjudication {
    let owner = if owner == DECISION_OWNER { owner } else { DECISION_OWNER };
    let package_digest = if package.package_digest == [0; 32] {
        package.content_digest()
    } else {
        package.package_digest
    };
    let _ = owner;
    PilotAdjudication::fail_closed(package_digest, format!("fail-closed evidence validation: {error}"))
        .expect("non-zero package digest creates a valid fail-closed adjudication")
}

fn fail_closed_text_adjudication(text: &str, error: &EvidenceError) -> PilotAdjudication {
    PilotAdjudication::fail_closed(Sha256::digest(text.as_bytes()).into(), format!("fail-closed package parsing: {error}"))
        .expect("non-zero raw package digest creates a valid fail-closed adjudication")
}

fn write_atomic(path: &Path, contents: String) -> Result<(), EvidenceError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| EvidenceError::InvalidFormat)?;
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temp, contents).map_err(|_| EvidenceError::InvalidFormat)?;
    fs::rename(&temp, path).map_err(|_| EvidenceError::InvalidFormat)
}
