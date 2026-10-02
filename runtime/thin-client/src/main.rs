use erf_thin_client::{
    explain_finding, make_exception_request, make_feedback_draft, query_submission_status,
    submit_exception, submit_feedback, AuthorityContext, ExceptionRequest, FeedbackDraft,
    AuthorityTrust, FindingAssessmentView, FindingView, RuleExplanationMetadata,
};
use serde_json::to_string_pretty;
use std::fs;
use std::path::Path;
use std::time::Duration;

fn usage() {
    eprintln!(
        "usage:\n  rf-client explain FINDING.json [--rule-meta FILE] [--result FILE --authority-trust FILE] [--assessment FILE] [--format text|json] [--output FILE]\n  rf-client feedback-draft FINDING.json --classification CLASS --reason TEXT --actor ACTOR [--output FILE]\n  rf-client exception-request FINDING.json --scope GLOB --reason TEXT --actor ACTOR [--output FILE]\n  rf-client feedback-submit REQUEST.json --server URL [--timeout-ms N] [--output FILE]\n  rf-client exception-submit REQUEST.json --server URL [--timeout-ms N] [--output FILE]\n  rf-client submission-status --server URL (--request-id DIGEST | --artifact-digest DIGEST) [--timeout-ms N] [--output FILE]"
    );
}

fn main() {
    if let Err(error) = run() {
        let code = if error.starts_with("submission outcome is unknown")
            || error.starts_with("submission status is UNKNOWN")
            || error.starts_with("submission status is PENDING")
        {
            3
        } else {
            2
        };
        fail(&error, code);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        return Err("missing client command".into());
    };
    let result = match command.as_str() {
        "registry" => registry_command(&mut args),
        "explain" => explain_command(&mut args),
        "feedback-draft" => draft_feedback(&mut args),
        "exception-request" => draft_exception(&mut args),
        "feedback-submit" => submit_feedback_request(&mut args),
        "exception-submit" => submit_exception_request(&mut args),
        "submission-status" => submission_status_request(&mut args),
        _ => Err(format!("unsupported client command: {command}")),
    };
    result
}

fn explain_command(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let finding_path = args.next().ok_or("missing FINDING.json")?;
    let mut rule_meta = None;
    let mut result_path = None;
    let mut authority_trust = None;
    let mut assessment_path = None;
    let mut format = "text".to_owned();
    let mut format_seen = false;
    let mut output = None;
    parse_options(args, &mut |option, value| {
        match option {
            "--rule-meta" => rule_meta = Some(value),
            "--result" => result_path = Some(value),
            "--authority-trust" => authority_trust = Some(value),
            "--assessment" => assessment_path = Some(value),
            "--format" => {
                if format_seen {
                    return Err("--format may appear only once".into());
                }
                format_seen = true;
                if value != "text" && value != "json" {
                    return Err("--format must be text or json".into());
                }
                format = value;
            }
            "--output" => output = Some(value),
            _ => return Err(format!("unexpected option: {option}")),
        }
        Ok(())
    })?;
    let finding = FindingView::from_file(&finding_path).map_err(|error| error.to_string())?;
    let metadata = match rule_meta {
        Some(path) => Some(RuleExplanationMetadata::from_file(&path).map_err(|error| error.to_string())?),
        None => None,
    };
    let authority = match (result_path, authority_trust) {
        (Some(path), Some(trust_path)) => {
            let trust = AuthorityTrust::from_file(&trust_path).map_err(|error| error.to_string())?;
            Some(AuthorityContext::from_file_with_trust(&path, &trust).map_err(|error| error.to_string())?)
        }
        (Some(path), None) => Some(AuthorityContext::from_file(&path).map_err(|error| error.to_string())?),
        (None, Some(_)) => return Err("--authority-trust requires --result".into()),
        (None, None) => None,
    };
    let assessment = match assessment_path {
        Some(path) => Some(FindingAssessmentView::from_file(&path).map_err(|error| error.to_string())?),
        None => None,
    };
    let outcome = explain_finding(&finding, metadata.as_ref(), authority.as_ref(), assessment.as_ref());
    let text = if format == "json" {
        to_string_pretty(&outcome.to_json()).map_err(|error| error.to_string())? + "\n"
    } else {
        outcome.render_text()
    };
    if let Some(path) = output.as_deref() {
        if Path::new(path).is_dir() {
            return Err("--output must name a file".into());
        }
        fs::write(path, &text).map_err(|error| error.to_string())?;
    } else {
        print!("{text}");
    }
    if outcome.is_refusal() {
        std::process::exit(outcome.exit_code());
    }
    Ok(())
}

fn draft_feedback(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let finding_path = args.next().ok_or("missing FINDING.json")?;
    let mut classification = None;
    let mut reason = None;
    let mut actor = None;
    let mut output = None;
    parse_options(args, &mut |option, value| {
        match option {
            "--classification" => classification = Some(value),
            "--reason" => reason = Some(value),
            "--actor" => actor = Some(value),
            "--output" => output = Some(value),
            _ => return Err(format!("unexpected option: {option}")),
        }
        Ok(())
    })?;
    let finding = FindingView::from_file(&finding_path).map_err(|error| error.to_string())?;
    let draft = make_feedback_draft(
        &finding,
        classification.as_deref().ok_or("missing --classification")?,
        reason.as_deref().ok_or("missing --reason")?,
        actor.as_deref().ok_or("missing --actor")?,
    )
    .map_err(|error| error.to_string())?;
    write_output(output.as_deref(), &draft.as_json())
}

fn draft_exception(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let finding_path = args.next().ok_or("missing FINDING.json")?;
    let mut scope = None;
    let mut reason = None;
    let mut actor = None;
    let mut output = None;
    parse_options(args, &mut |option, value| {
        match option {
            "--scope" => scope = Some(value),
            "--reason" => reason = Some(value),
            "--actor" => actor = Some(value),
            "--output" => output = Some(value),
            _ => return Err(format!("unexpected option: {option}")),
        }
        Ok(())
    })?;
    let finding = FindingView::from_file(&finding_path).map_err(|error| error.to_string())?;
    let request = make_exception_request(
        &finding,
        scope.as_deref().ok_or("missing --scope")?,
        reason.as_deref().ok_or("missing --reason")?,
        actor.as_deref().ok_or("missing --actor")?,
    )
    .map_err(|error| error.to_string())?;
    write_output(output.as_deref(), &request.as_json())
}

fn submit_feedback_request(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let request_path = args.next().ok_or("missing REQUEST.json")?;
    let (server, timeout, output) = parse_submit_options(args)?;
    let draft = FeedbackDraft::from_file(&request_path).map_err(|error| error.to_string())?;
    let receipt = submit_feedback(&draft, &server, timeout).map_err(|error| error.to_string())?;
    write_output(output.as_deref(), &serde_json::to_value(receipt).map_err(|error| error.to_string())?)
}

fn submit_exception_request(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let request_path = args.next().ok_or("missing REQUEST.json")?;
    let (server, timeout, output) = parse_submit_options(args)?;
    let request = ExceptionRequest::from_file(&request_path).map_err(|error| error.to_string())?;
    let receipt = submit_exception(&request, &server, timeout).map_err(|error| error.to_string())?;
    write_output(output.as_deref(), &serde_json::to_value(receipt).map_err(|error| error.to_string())?)
}

fn submission_status_request(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let mut server = None;
    let mut request_id = None;
    let mut artifact_digest = None;
    let mut timeout_ms = 10_000_u64;
    let mut timeout_seen = false;
    let mut output = None;
    parse_options(args, &mut |option, value| {
        match option {
            "--server" => {
                if server.is_some() {
                    return Err("--server may appear only once".into());
                }
                server = Some(value);
            }
            "--request-id" => {
                if request_id.is_some() {
                    return Err("--request-id may appear only once".into());
                }
                request_id = Some(value);
            }
            "--artifact-digest" => {
                if artifact_digest.is_some() {
                    return Err("--artifact-digest may appear only once".into());
                }
                artifact_digest = Some(value);
            }
            "--timeout-ms" => {
                if timeout_seen {
                    return Err("--timeout-ms may appear only once".into());
                }
                timeout_seen = true;
                timeout_ms = value.parse::<u64>().map_err(|_| "--timeout-ms must be an integer".to_owned())?;
                if timeout_ms == 0 || timeout_ms > 600_000 {
                    return Err("--timeout-ms must be between 1 and 600000".into());
                }
            }
            "--output" => {
                if output.is_some() {
                    return Err("--output may appear only once".into());
                }
                output = Some(value);
            }
            _ => return Err(format!("unexpected option: {option}")),
        }
        Ok(())
    })?;
    if request_id.is_none() && artifact_digest.is_none() {
        return Err("missing --request-id or --artifact-digest".into());
    }
    let status = query_submission_status(
        request_id.as_deref(),
        artifact_digest.as_deref(),
        &server.ok_or("missing --server")?,
        Duration::from_millis(timeout_ms),
    )
    .map_err(|error| error.to_string())?;
    let status_name = status.status.clone();
    write_output(output.as_deref(), &serde_json::to_value(status).map_err(|error| error.to_string())?)?;
    if matches!(status_name.as_str(), "UNKNOWN" | "PENDING") {
        return Err(format!("submission status is {status_name}"));
    }
    Ok(())
}

fn parse_submit_options(args: &mut impl Iterator<Item = String>) -> Result<(String, Duration, Option<String>), String> {
    let mut server = None;
    let mut timeout_ms = 10_000_u64;
    let mut output = None;
    parse_options(args, &mut |option, value| {
        match option {
            "--server" => server = Some(value),
            "--timeout-ms" => {
                timeout_ms = value.parse::<u64>().map_err(|_| "--timeout-ms must be an integer".to_owned())?;
                if timeout_ms == 0 || timeout_ms > 600_000 {
                    return Err("--timeout-ms must be between 1 and 600000".into());
                }
            }
            "--output" => output = Some(value),
            _ => return Err(format!("unexpected option: {option}")),
        }
        Ok(())
    })?;
    Ok((
        server.ok_or("missing --server")?,
        Duration::from_millis(timeout_ms),
        output,
    ))
}

fn parse_options(
    args: &mut impl Iterator<Item = String>,
    handler: &mut impl FnMut(&str, String) -> Result<(), String>,
) -> Result<(), String> {
    while let Some(option) = args.next() {
        let value = args.next().ok_or_else(|| format!("missing value for {option}"))?;
        handler(&option, value)?;
    }
    Ok(())
}

fn write_output(output: Option<&str>, value: &serde_json::Value) -> Result<(), String> {
    let text = to_string_pretty(value).map_err(|error| error.to_string())? + "\n";
    if let Some(path) = output {
        if Path::new(path).is_dir() {
            return Err("--output must name a file".into());
        }
        fs::write(path, text).map_err(|error| error.to_string())?;
    } else {
        print!("{text}");
    }
    Ok(())
}

fn fail(message: &str, code: i32) -> ! {
    eprintln!("error: {message}");
    usage();
    std::process::exit(code);
}

fn registry_command(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let result = registry_request(args);
    match result {
        Ok(receipt) => write_output(None, &receipt),
        Err(code) => {
            // Static codes only: never echo endpoint/arguments or untrusted server content.
            println!("{}", serde_json::json!({"status": "UNKNOWN", "authority": "UNKNOWN", "code": code}));
            std::process::exit(3);
        }
    }
}

fn registry_request(args: &mut impl Iterator<Item = String>) -> Result<serde_json::Value, &'static str> {
    let action = args.next().ok_or("REGISTRY_QUERY_INVALID")?;
    let resource_type = args.next().ok_or("REGISTRY_QUERY_INVALID")?;
    if !matches!(action.as_str(), "list" | "show") { return Err("REGISTRY_QUERY_INVALID"); }
    let id = if action == "show" { Some(args.next().ok_or("REGISTRY_PIN_REQUIRED")?) } else { None };
    let mut options = std::collections::BTreeMap::new();
    while let Some(key) = args.next() {
        let allowed = match action.as_str() {
            "list" => matches!(key.as_str(), "--server" | "--timeout-ms" | "--cursor-json" | "--limit"),
            _ => matches!(key.as_str(), "--server" | "--timeout-ms" | "--version" | "--expected-digest"),
        };
        if !allowed || options.insert(key, args.next().ok_or("REGISTRY_QUERY_INVALID")?).is_some() {
            return Err("REGISTRY_QUERY_INVALID");
        }
    }
    let option = |key: &str| options.get(key).map(String::as_str);
    let server = option("--server").ok_or("REGISTRY_ENDPOINT_REQUIRED")?;
    let timeout: u64 = option("--timeout-ms").unwrap_or("10000").parse().map_err(|_| "REGISTRY_QUERY_INVALID")?;
    if !(1..=600_000).contains(&timeout) { return Err("REGISTRY_QUERY_INVALID"); }
    let timeout = Duration::from_millis(timeout);
    let cursor: String = serde_json::from_str(option("--cursor-json").unwrap_or("\"\"")).map_err(|_| "REGISTRY_QUERY_INVALID")?;
    match id {
        Some(id) => erf_thin_client::registry::show(server, &resource_type, &id,
            option("--version").ok_or("REGISTRY_PIN_REQUIRED")?,
            option("--expected-digest").ok_or("REGISTRY_PIN_REQUIRED")?, timeout),
        None => erf_thin_client::registry::list(server, &resource_type, &cursor,
            option("--limit").unwrap_or("256").parse().map_err(|_| "REGISTRY_QUERY_INVALID")?, timeout),
    }
}
