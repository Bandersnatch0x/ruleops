use erf_fast_decision::{
    parse_hex_digest, serialize_evidence, BaselineInput, DecisionSpec, DeterministicBaselineAdapter,
    FastDecisionAdapter, DecisionStatus,
};
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("fast decision failed: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<u8, erf_fast_decision::FastDecisionError> {
    let mut args = env::args().skip(1);
    if args.next().as_deref() != Some("evaluate") {
        eprintln!("usage: erf-fast-decision evaluate INPUT_DIGEST SIGNAL NOW OUTPUT [--near-miss|--provider-unavailable|--conflict]");
        return Err(erf_fast_decision::FastDecisionError::InvalidInput);
    }
    let input_digest = parse_hex_digest(&args.next().ok_or(erf_fast_decision::FastDecisionError::InvalidInput)?)?;
    let signal = args.next().ok_or(erf_fast_decision::FastDecisionError::InvalidInput)?.parse().map_err(|_| erf_fast_decision::FastDecisionError::InvalidInput)?;
    let now = args.next().ok_or(erf_fast_decision::FastDecisionError::InvalidInput)?.parse::<u64>().map_err(|_| erf_fast_decision::FastDecisionError::InvalidInput)?;
    let output = args.next().ok_or(erf_fast_decision::FastDecisionError::InvalidInput)?;
    let mut near_miss = false;
    let mut provider_available = true;
    let mut conflict = false;
    for flag in args {
        match flag.as_str() {
            "--near-miss" => near_miss = true,
            "--provider-unavailable" => provider_available = false,
            "--conflict" => conflict = true,
            _ => return Err(erf_fast_decision::FastDecisionError::InvalidInput),
        }
    }
    let spec = DecisionSpec::route_v1();
    let evidence = DeterministicBaselineAdapter.evaluate(&spec, &BaselineInput {
        input_digest,
        signal,
        created_at_epoch_secs: now.saturating_sub(100),
        provider_available,
        near_miss,
        conflict,
    }, now)?;
    evidence.validate_against(&spec)?;
    write_atomic(Path::new(&output), serialize_evidence(&evidence, &spec)?)?;
    println!("fast-decision={} status={} evidence-digest={}", output, evidence.status().as_str(), erf_fast_decision::hex_digest(&evidence.evidence_digest()));
    Ok(if evidence.status() == DecisionStatus::Decided { 0 } else { 3 })
}

fn write_atomic(path: &Path, contents: String) -> Result<(), erf_fast_decision::FastDecisionError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| erf_fast_decision::FastDecisionError::InvalidInput)?;
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temp, contents).map_err(|_| erf_fast_decision::FastDecisionError::InvalidInput)?;
    fs::rename(&temp, path).map_err(|_| erf_fast_decision::FastDecisionError::InvalidInput)
}
