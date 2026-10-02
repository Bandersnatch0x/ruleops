//! Author verified rule-explanation metadata deterministically.
//!
//! Reads an unsigned metadata projection (every field except `digest`) and
//! writes the canonical, self-digested metadata so acceptance fixtures match
//! exactly what `rf explain --rule-meta` verifies.
//!
//! usage: rule_meta_sign INPUT.json OUTPUT.json

use erf_thin_client::RuleExplanationMetadata;
use serde_json::{to_string_pretty, Value};
use std::fs;
use std::process::exit;

fn main() {
    let mut args = std::env::args().skip(1);
    let input = args.next();
    let output = args.next();
    let (Some(input), Some(output)) = (input, output) else {
        eprintln!("usage: rule_meta_sign INPUT.json OUTPUT.json");
        exit(2);
    };
    if args.next().is_some() {
        eprintln!("usage: rule_meta_sign INPUT.json OUTPUT.json");
        exit(2);
    }
    if let Err(error) = run(&input, &output) {
        eprintln!("error: {error}");
        exit(2);
    }
}

fn run(input: &str, output: &str) -> Result<(), String> {
    let text = fs::read_to_string(input).map_err(|error| error.to_string())?;
    let json: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let signed = RuleExplanationMetadata::sign(&json).map_err(|error| error.to_string())?;
    let rendered = to_string_pretty(&signed).map_err(|error| error.to_string())? + "\n";
    fs::write(output, rendered).map_err(|error| error.to_string())?;
    Ok(())
}
