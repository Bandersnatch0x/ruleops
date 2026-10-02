use erf_project_config::{
    apply_precedence, detect_workspace, load_config, process_environment, resolve_precedence,
    run_doctor_with_overrides, validate_output_path, validate_workspace_paths, write_config, ConfigError,
    ConfigOverrides, CONFIG_RELATIVE_PATH,
};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rf bootstrap failed: {error}");
            ExitCode::from(match error {
                ConfigError::UnsupportedLanguage(_)
                | ConfigError::UnsupportedBuildSystem(_)
                | ConfigError::RepositoryMissing(_)
                | ConfigError::MissingField(_)
                | ConfigError::InvalidValue { .. }
                | ConfigError::ConflictingValues { .. }
                | ConfigError::PathEscapesWorkspace { .. }
                | ConfigError::PathUsesSymlink { .. }
                | ConfigError::UnsupportedVersion(_)
                | ConfigError::UnknownField(_)
                | ConfigError::DuplicateField(_)
                | ConfigError::Parse { .. }
                | ConfigError::Io { .. }
                | ConfigError::InvalidCacheObject(_) => 2,
            })
        }
    }
}

fn run() -> Result<(), ConfigError> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let action = args.first().cloned().unwrap_or_default();
    if !args.is_empty() {
        args.remove(0);
    }
    match action.as_str() {
        "init" => run_init(&args),
        "doctor" => run_doctor_command(&args),
        "validate" => run_config_validate(&args),
        "show" => run_config_show(&args),
        "identity" => run_identity(&args),
        _ => Err(ConfigError::Parse { line: 0, detail: "usage: erf-project-config init|doctor|validate|show|identity".into() }),
    }
}

fn run_init(args: &[String]) -> Result<(), ConfigError> {
    validate_options(
        args,
        &["--non-interactive", "--force"],
        &["--repo-root", "--output", "--language", "--build-system", "--execution-mode", "--cache-dir", "--trust-state-path", "--module", "--source-root", "--test-root", "--generated-root", "--vendor-root", "--include", "--exclude"],
    )?;
    let repo_root = path_option(args, "--repo-root")?.unwrap_or_else(|| PathBuf::from("."));
    if fs::symlink_metadata(&repo_root).map(|metadata| metadata.file_type().is_symlink()).unwrap_or(false) {
        return Err(ConfigError::PathUsesSymlink { field: "repo_root", value: repo_root.display().to_string() });
    }
    let repo_root = fs::canonicalize(&repo_root).map_err(|error| ConfigError::Io { path: repo_root.display().to_string(), detail: error.to_string() })?;
    let output = path_option(args, "--output")?
        .map(|path| if path.is_absolute() { path } else { repo_root.join(path) })
        .unwrap_or_else(|| repo_root.join(CONFIG_RELATIVE_PATH));
    let non_interactive = flag(args, "--non-interactive")?;
    let force = flag(args, "--force")?;
    let overrides = parse_overrides(args)?;
    if output.exists() && !force {
        return Err(ConfigError::Io { path: output.display().to_string(), detail: "configuration already exists; use --force only after reviewing the replacement".into() });
    }
    let detected = detect_workspace(&repo_root)?;
    let mut base = detected.clone().into_config();
    if base.language.is_empty() {
        base.language = "unknown".into();
    }
    if base.build_system.is_empty() {
        base.build_system = "unknown".into();
    }
    let environment = project_environment();
    let (config, sources) = resolve_precedence(&base, &overrides, &environment)?;
    // A repository may intentionally contain a supported Java backend and an
    // unsupported frontend. When the caller explicitly selects Java, the
    // detected Java source roots already define the safe analysis boundary;
    // the presence of the other language is no longer unresolved ambiguity.
    // Keep every other detection note fail-closed until it is explicitly
    // modelled and resolved.
    let unresolved_notes = detected
        .notes
        .iter()
        .filter(|note| {
            if note.starts_with("language detection is ambiguous:") {
                let language_is_explicit = overrides.language.is_some() || environment.contains_key("RULEOPS_LANGUAGE");
                !(language_is_explicit && config.language == "java" && !config.source_roots.is_empty())
            } else {
                true
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    if !unresolved_notes.is_empty() || config.language != "java" || config.build_system == "unknown" || config.source_roots.is_empty() {
        let detail = if unresolved_notes.is_empty() { "workspace detection is incomplete".into() } else { unresolved_notes.join("; ") };
        if non_interactive || !io::stdin().is_terminal() {
            return Err(ConfigError::ConflictingValues { field: "workspace_detection", detail });
        }
        println!("Detection requires confirmation: {detail}");
        println!("Resolved config:\n{}", config.to_yaml());
        println!("Write this configuration? [y/N]");
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).map_err(|error| ConfigError::Io { path: "stdin".into(), detail: error.to_string() })?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            return Err(ConfigError::ConflictingValues { field: "workspace_detection", detail: "user did not confirm the ambiguous configuration".into() });
        }
    } else if !non_interactive {
        println!("Detected workspace:\n{}", config.to_yaml());
        if io::stdin().is_terminal() {
            println!("Write this configuration? [Y/n]");
            let mut answer = String::new();
            io::stdin().read_line(&mut answer).map_err(|error| ConfigError::Io { path: "stdin".into(), detail: error.to_string() })?;
            if matches!(answer.trim().to_ascii_lowercase().as_str(), "n" | "no") {
                return Err(ConfigError::ConflictingValues { field: "workspace_detection", detail: "user declined configuration".into() });
            }
        } else {
            return Err(ConfigError::ConflictingValues { field: "workspace_detection", detail: "non-interactive stdin requires --non-interactive".into() });
        }
    }
    config.validate()?;
    validate_workspace_paths(&repo_root, &config)?;
    validate_output_path(&repo_root, &output)?;
    if output.exists() && force {
        let temporary = output.with_extension(format!("yaml.tmp-{}", std::process::id()));
        write_config(&temporary, &config)?;
        fs::rename(&temporary, &output).map_err(|error| ConfigError::Io { path: output.display().to_string(), detail: error.to_string() })?;
    } else {
        write_config(&output, &config)?;
    }
    let identity = config.identity();
    println!("CONFIG_WRITTEN {}", output.display());
    println!("config_digest={}", identity.config_digest);
    println!("workspace_descriptor_digest={}", identity.workspace_descriptor_digest);
    println!("scan_plan_digest={}", identity.scan_plan_digest);
    println!("sources={}", sources.into_iter().map(|(key, value)| format!("{key}:{value}")).collect::<Vec<_>>().join(","));
    Ok(())
}

fn run_doctor_command(args: &[String]) -> Result<(), ConfigError> {
    validate_options(args, &[], &["--repo-root", "--config", "--scan-probe", "--format", "--language", "--build-system", "--execution-mode", "--cache-dir", "--trust-state-path", "--module", "--source-root", "--test-root", "--generated-root", "--vendor-root", "--include", "--exclude"])?;
    let repo_root = path_option(args, "--repo-root")?.unwrap_or_else(|| PathBuf::from("."));
    let config_path = path_option(args, "--config")?;
    let scan_probe = path_option(args, "--scan-probe")?;
    let format = value_option(args, "--format")?.unwrap_or_else(|| "text".into());
    let overrides = parse_overrides(args)?;
    let report = run_doctor_with_overrides(&repo_root, config_path.as_deref(), scan_probe.as_deref(), &overrides, &process_environment());
    if format == "json" {
        print!("{}", report.to_json());
    } else if format == "text" {
        print!("{}", report.render_text());
    } else {
        return Err(ConfigError::InvalidValue { field: "format", value: format });
    }
    if report.exit_code() != 0 {
        std::process::exit(report.exit_code());
    }
    Ok(())
}

fn run_config_validate(args: &[String]) -> Result<(), ConfigError> {
    validate_options(args, &[], &["--repo-root", "--config"])?;
    let repo_root = path_option(args, "--repo-root")?.unwrap_or_else(|| PathBuf::from("."));
    let config_path = path_option(args, "--config")?;
    let (config, path) = load_config(&repo_root, config_path.as_deref())?;
    validate_workspace_paths(&repo_root, &config)?;
    let identity = config.identity();
    println!("CONFIG_VALID {}", path.display());
    println!("config_digest={}", identity.config_digest);
    println!("workspace_descriptor_digest={}", identity.workspace_descriptor_digest);
    println!("scan_plan_digest={}", identity.scan_plan_digest);
    Ok(())
}

fn run_config_show(args: &[String]) -> Result<(), ConfigError> {
    validate_options(
        args,
        &[],
        &["--repo-root", "--config", "--format", "--language", "--build-system", "--execution-mode", "--cache-dir", "--trust-state-path", "--module", "--source-root", "--test-root", "--generated-root", "--vendor-root", "--include", "--exclude"],
    )?;
    let repo_root = path_option(args, "--repo-root")?.unwrap_or_else(|| PathBuf::from("."));
    let config_path = path_option(args, "--config")?;
    let format = value_option(args, "--format")?.unwrap_or_else(|| "text".into());
    let (project, path) = load_config(&repo_root, config_path.as_deref())?;
    validate_workspace_paths(&repo_root, &project)?;
    let overrides = parse_overrides(args)?;
    let environment = project_environment();
    let (config, sources) = apply_precedence(&project, &overrides, &environment)?;
    validate_workspace_paths(&repo_root, &config)?;
    let identity = config.identity();
    match format.as_str() {
        "text" => {
            print!("{}", config.to_yaml());
            println!("config_path={}", path.display());
            println!("config_digest={}", identity.config_digest);
            println!("workspace_descriptor_digest={}", identity.workspace_descriptor_digest);
            println!("scan_plan_digest={}", identity.scan_plan_digest);
            println!("sources={}", sources.into_iter().map(|(key, value)| format!("{key}:{value}")).collect::<Vec<_>>().join(","));
        }
        "json" => {
            let value = serde_json::json!({
                "config": config,
                "configPath": path,
                "identity": identity,
                "sources": sources,
            });
            println!("{}", serde_json::to_string_pretty(&value).expect("config show is serializable"));
        }
        _ => return Err(ConfigError::InvalidValue { field: "format", value: format }),
    }
    Ok(())
}

fn run_identity(args: &[String]) -> Result<(), ConfigError> {
    validate_options(
        args,
        &[],
        &["--repo-root", "--config", "--language", "--build-system", "--execution-mode", "--cache-dir", "--trust-state-path", "--module", "--source-root", "--test-root", "--generated-root", "--vendor-root", "--include", "--exclude"],
    )?;
    let repo_root = path_option(args, "--repo-root")?.unwrap_or_else(|| PathBuf::from("."));
    let config_path = path_option(args, "--config")?;
    let (project, _) = load_config(&repo_root, config_path.as_deref())?;
    validate_workspace_paths(&repo_root, &project)?;
    let (config, _) = apply_precedence(&project, &parse_overrides(args)?, &project_environment())?;
    validate_workspace_paths(&repo_root, &config)?;
    print!("{}", config.identity().as_env_lines());
    Ok(())
}

fn parse_overrides(args: &[String]) -> Result<ConfigOverrides, ConfigError> {
    let mut overrides = ConfigOverrides::default();
    overrides.language = unique_value(args, "--language")?;
    overrides.build_system = unique_value(args, "--build-system")?;
    overrides.execution_mode = unique_value(args, "--execution-mode")?;
    overrides.cache_dir = unique_value(args, "--cache-dir")?;
    overrides.trust_state_path = unique_value(args, "--trust-state-path")?;
    overrides.modules = unique_csv(args, "--module")?;
    overrides.source_roots = unique_csv(args, "--source-root")?;
    overrides.test_roots = unique_csv(args, "--test-root")?;
    overrides.generated_roots = unique_csv(args, "--generated-root")?;
    overrides.vendor_roots = unique_csv(args, "--vendor-root")?;
    overrides.include = unique_csv(args, "--include")?;
    overrides.exclude = unique_csv(args, "--exclude")?;
    Ok(overrides)
}

fn project_environment() -> BTreeMap<String, String> {
    [
        "RULEOPS_LANGUAGE",
        "RULEOPS_BUILD_SYSTEM",
        "RULEOPS_MODULES",
        "RULEOPS_SOURCE_ROOTS",
        "RULEOPS_TEST_ROOTS",
        "RULEOPS_GENERATED_ROOTS",
        "RULEOPS_VENDOR_ROOTS",
        "RULEOPS_INCLUDE",
        "RULEOPS_EXCLUDE",
        "RULEOPS_EXECUTION_MODE",
        "RULEOPS_CACHE_DIR",
        "RULEOPS_TRUST_STATE_PATH",
    ]
    .into_iter()
    .filter_map(|key| env::var(key).ok().map(|value| (key.into(), value)))
    .collect()
}

fn flag(args: &[String], name: &str) -> Result<bool, ConfigError> {
    let count = args.iter().filter(|arg| arg.as_str() == name).count();
    if count > 1 {
        return Err(ConfigError::ConflictingValues { field: "cli", detail: format!("flag {name} was repeated") });
    }
    Ok(count == 1)
}

fn value_option(args: &[String], name: &str) -> Result<Option<String>, ConfigError> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == name {
            let value = args.get(index + 1).ok_or_else(|| ConfigError::InvalidValue { field: "cli", value: format!("missing value for {name}") })?;
            values.push(value.clone());
            index += 2;
        } else {
            index += 1;
        }
    }
    if values.len() > 1 {
        return Err(ConfigError::ConflictingValues { field: "cli", detail: format!("option {name} was repeated") });
    }
    Ok(values.into_iter().next())
}

fn unique_value(args: &[String], name: &str) -> Result<Option<String>, ConfigError> {
    value_option(args, name)
}

fn unique_csv(args: &[String], name: &str) -> Result<Option<Vec<String>>, ConfigError> {
    let indices = args.iter().enumerate().filter_map(|(index, arg)| (arg == name).then_some(index)).collect::<Vec<_>>();
    if indices.is_empty() {
        return Ok(None);
    }
    if indices.len() > 1 {
        return Err(ConfigError::ConflictingValues { field: "cli", detail: format!("option {name} was repeated") });
    }
    let value = args.get(indices[0] + 1).ok_or_else(|| ConfigError::InvalidValue { field: "cli", value: format!("missing value for {name}") })?;
    Ok(Some(value.split(',').map(|value| value.trim().to_string()).collect()))
}

fn path_option(args: &[String], name: &str) -> Result<Option<PathBuf>, ConfigError> {
    Ok(value_option(args, name)?.map(PathBuf::from))
}

fn validate_options(args: &[String], flags: &[&str], values: &[&str]) -> Result<(), ConfigError> {
    let mut index = 0;
    while index < args.len() {
        let argument = args[index].as_str();
        if flags.contains(&argument) {
            index += 1;
            continue;
        }
        if values.contains(&argument) {
            if args.get(index + 1).is_none() {
                return Err(ConfigError::InvalidValue { field: "cli", value: format!("missing value for {argument}") });
            }
            index += 2;
            continue;
        }
        return Err(ConfigError::InvalidValue { field: "cli", value: format!("unexpected argument {argument}") });
    }
    Ok(())
}
