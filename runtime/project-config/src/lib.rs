//! Project configuration and bootstrap checks for the thin `rf` client.
//!
//! The configuration format is deliberately a closed YAML subset: scalar
//! values and inline string arrays only. Keeping the accepted shape narrow
//! makes parsing deterministic without silently accepting a new field or a
//! duplicate key. The resulting canonical JSON is used only for identity;
//! it never contains source or Finding content.

use erf_bundle_cache::{BundleCache, FileBundleCache};
use erf_trust_state::{TrustStateFile, TrustStateStoreError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use thiserror::Error;

pub const CONFIG_VERSION: &str = "ruleops.config/v1";
pub const CONFIG_RELATIVE_PATH: &str = ".ruleops/config.yaml";
pub const DEFAULT_CACHE_RELATIVE_PATH: &str = ".ruleops/cache/bundles";
pub const DEFAULT_TRUST_STATE_RELATIVE_PATH: &str = ".ruleops/trust-state";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Connected,
    Offline,
    Standalone,
}

impl ExecutionMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Offline => "offline",
            Self::Standalone => "standalone",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "connected" => Ok(Self::Connected),
            "offline" | "enterprise-air-gapped" => Ok(Self::Offline),
            "standalone" | "standalone-developer" => Ok(Self::Standalone),
            _ => Err(ConfigError::InvalidValue {
                field: "execution_mode",
                value: value.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub version: String,
    pub language: String,
    pub build_system: String,
    pub modules: Vec<String>,
    pub source_roots: Vec<String>,
    pub test_roots: Vec<String>,
    pub generated_roots: Vec<String>,
    pub vendor_roots: Vec<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub execution_mode: ExecutionMode,
    pub cache_dir: String,
    pub trust_state_path: String,
}

impl ProjectConfig {
    pub fn defaults() -> Self {
        Self {
            version: CONFIG_VERSION.into(),
            language: String::new(),
            build_system: String::new(),
            modules: Vec::new(),
            source_roots: Vec::new(),
            test_roots: Vec::new(),
            generated_roots: Vec::new(),
            vendor_roots: Vec::new(),
            include: vec!["**".into()],
            exclude: Vec::new(),
            execution_mode: ExecutionMode::Connected,
            cache_dir: DEFAULT_CACHE_RELATIVE_PATH.into(),
            trust_state_path: DEFAULT_TRUST_STATE_RELATIVE_PATH.into(),
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.version != CONFIG_VERSION {
            return Err(ConfigError::UnsupportedVersion(self.version.clone()));
        }
        validate_token("language", &self.language)?;
        if self.language != "java" {
            return Err(ConfigError::UnsupportedLanguage(self.language.clone()));
        }
        validate_token("build_system", &self.build_system)?;
        if !matches!(self.build_system.as_str(), "maven" | "gradle") {
            return Err(ConfigError::UnsupportedBuildSystem(self.build_system.clone()));
        }
        if self.modules.is_empty() {
            return Err(ConfigError::MissingField("modules"));
        }
        if self.source_roots.is_empty() {
            return Err(ConfigError::MissingField("source_roots"));
        }
        validate_unique_paths("modules", &self.modules)?;
        validate_unique_paths("source_roots", &self.source_roots)?;
        validate_unique_paths("test_roots", &self.test_roots)?;
        validate_unique_paths("generated_roots", &self.generated_roots)?;
        validate_unique_paths("vendor_roots", &self.vendor_roots)?;
        validate_unique_paths("include", &self.include)?;
        validate_unique_paths("exclude", &self.exclude)?;
        for (field, values) in [
            ("modules", &self.modules),
            ("source_roots", &self.source_roots),
            ("test_roots", &self.test_roots),
            ("generated_roots", &self.generated_roots),
            ("vendor_roots", &self.vendor_roots),
            ("include", &self.include),
            ("exclude", &self.exclude),
        ] {
            for value in values {
                validate_relative_path(field, value)?;
            }
        }
        if self
            .include
            .iter()
            .any(|value| self.exclude.iter().any(|excluded| excluded == value))
        {
            return Err(ConfigError::ConflictingValues {
                field: "include/exclude",
                detail: "the same path pattern is both included and excluded".into(),
            });
        }
        ExecutionMode::parse(self.execution_mode.as_str())?;
        validate_relative_path("cache_dir", &self.cache_dir)?;
        validate_relative_path("trust_state_path", &self.trust_state_path)?;
        Ok(())
    }

    pub fn canonical_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("ProjectConfig is serializable")
    }

    pub fn config_digest(&self) -> String {
        digest_bytes(&self.canonical_json())
    }

    pub fn workspace_descriptor_digest(&self) -> String {
        let value = serde_json::json!({
            "apiVersion": "rulefactory.io/v1",
            "kind": "WorkspaceDescriptor",
            "schemaVersion": "workspace-descriptor-v1",
            "language": self.language,
            "buildSystem": self.build_system,
            "modules": self.modules,
            "sourceRoots": self.source_roots,
            "testRoots": self.test_roots,
            "generatedRoots": self.generated_roots,
            "vendorRoots": self.vendor_roots,
            "include": self.include,
            "exclude": self.exclude,
        });
        digest_bytes(&serde_json::to_vec(&value).expect("workspace descriptor is serializable"))
    }

    pub fn scan_plan_digest(&self) -> String {
        let mut payload = b"ruleops-scan-plan-v1\0".to_vec();
        payload.extend_from_slice(self.config_digest().as_bytes());
        payload.extend_from_slice(self.workspace_descriptor_digest().as_bytes());
        payload.extend_from_slice(self.execution_mode.as_str().as_bytes());
        digest_bytes(&payload)
    }

    pub fn identity(&self) -> ConfigIdentity {
        ConfigIdentity {
            config_digest: self.config_digest(),
            workspace_descriptor_digest: self.workspace_descriptor_digest(),
            scan_plan_digest: self.scan_plan_digest(),
        }
    }

    pub fn resolved_cache_dir(&self, repo_root: &Path) -> PathBuf {
        repo_root.join(&self.cache_dir)
    }

    pub fn resolved_trust_state_path(&self, repo_root: &Path) -> PathBuf {
        repo_root.join(&self.trust_state_path)
    }

    pub fn to_yaml(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("version: {}", self.version));
        lines.push(format!("language: {}", self.language));
        lines.push(format!("build_system: {}", self.build_system));
        lines.push(format!("modules: {}", yaml_list(&self.modules)));
        lines.push(format!("source_roots: {}", yaml_list(&self.source_roots)));
        lines.push(format!("test_roots: {}", yaml_list(&self.test_roots)));
        lines.push(format!("generated_roots: {}", yaml_list(&self.generated_roots)));
        lines.push(format!("vendor_roots: {}", yaml_list(&self.vendor_roots)));
        lines.push(format!("include: {}", yaml_list(&self.include)));
        lines.push(format!("exclude: {}", yaml_list(&self.exclude)));
        lines.push(format!("execution_mode: {}", self.execution_mode.as_str()));
        lines.push(format!("cache_dir: {}", self.cache_dir));
        lines.push(format!("trust_state_path: {}", self.trust_state_path));
        lines.join("\n") + "\n"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigIdentity {
    pub config_digest: String,
    pub workspace_descriptor_digest: String,
    pub scan_plan_digest: String,
}

impl ConfigIdentity {
    pub fn as_env_lines(&self) -> String {
        format!(
            "RULEOPS_EFFECTIVE_CONFIG_DIGEST={}\nRULEOPS_WORKSPACE_DESCRIPTOR_DIGEST={}\nRULEOPS_SCAN_PLAN_DIGEST={}\n",
            self.config_digest, self.workspace_descriptor_digest, self.scan_plan_digest
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigOverrides {
    pub language: Option<String>,
    pub build_system: Option<String>,
    pub modules: Option<Vec<String>>,
    pub source_roots: Option<Vec<String>>,
    pub test_roots: Option<Vec<String>>,
    pub generated_roots: Option<Vec<String>>,
    pub vendor_roots: Option<Vec<String>>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub execution_mode: Option<String>,
    pub cache_dir: Option<String>,
    pub trust_state_path: Option<String>,
}

pub fn apply_precedence(
    project: &ProjectConfig,
    overrides: &ConfigOverrides,
    environment: &BTreeMap<String, String>,
) -> Result<(ProjectConfig, BTreeMap<String, String>), ConfigError> {
    let (resolved, sources) = resolve_precedence(project, overrides, environment)?;
    resolved.validate()?;
    Ok((resolved, sources))
}

/// Resolve the four configuration sources without validating the candidate.
/// `rf init` uses this while it is still presenting an uncertain detection to
/// an interactive user; validation happens after that confirmation.
pub fn resolve_precedence(
    project: &ProjectConfig,
    overrides: &ConfigOverrides,
    environment: &BTreeMap<String, String>,
) -> Result<(ProjectConfig, BTreeMap<String, String>), ConfigError> {
    let mut resolved = project.clone();
    let mut sources = BTreeMap::new();
    resolved.language = choose_scalar(
        "language",
        overrides.language.as_deref(),
        environment.get("RULEOPS_LANGUAGE").map(String::as_str),
        &project.language,
        &mut sources,
    );
    resolved.build_system = choose_scalar(
        "build_system",
        overrides.build_system.as_deref(),
        environment.get("RULEOPS_BUILD_SYSTEM").map(String::as_str),
        &project.build_system,
        &mut sources,
    );
    resolved.execution_mode = ExecutionMode::parse(&choose_scalar(
        "execution_mode",
        overrides.execution_mode.as_deref(),
        environment.get("RULEOPS_EXECUTION_MODE").map(String::as_str),
        project.execution_mode.as_str(),
        &mut sources,
    ))?;
    resolved.cache_dir = choose_scalar(
        "cache_dir",
        overrides.cache_dir.as_deref(),
        environment.get("RULEOPS_CACHE_DIR").map(String::as_str),
        &project.cache_dir,
        &mut sources,
    );
    resolved.trust_state_path = choose_scalar(
        "trust_state_path",
        overrides.trust_state_path.as_deref(),
        environment.get("RULEOPS_TRUST_STATE_PATH").map(String::as_str),
        &project.trust_state_path,
        &mut sources,
    );

    for (field, target, explicit, env_name, project_value) in [
        ("modules", &mut resolved.modules, overrides.modules.as_ref(), "RULEOPS_MODULES", &project.modules),
        ("source_roots", &mut resolved.source_roots, overrides.source_roots.as_ref(), "RULEOPS_SOURCE_ROOTS", &project.source_roots),
        ("test_roots", &mut resolved.test_roots, overrides.test_roots.as_ref(), "RULEOPS_TEST_ROOTS", &project.test_roots),
        ("generated_roots", &mut resolved.generated_roots, overrides.generated_roots.as_ref(), "RULEOPS_GENERATED_ROOTS", &project.generated_roots),
        ("vendor_roots", &mut resolved.vendor_roots, overrides.vendor_roots.as_ref(), "RULEOPS_VENDOR_ROOTS", &project.vendor_roots),
        ("include", &mut resolved.include, overrides.include.as_ref(), "RULEOPS_INCLUDE", &project.include),
        ("exclude", &mut resolved.exclude, overrides.exclude.as_ref(), "RULEOPS_EXCLUDE", &project.exclude),
    ] {
        let environment_value = environment.get(env_name).map(|value| parse_csv(field, value)).transpose()?;
        let (value, source) = if let Some(value) = explicit {
            (value.clone(), "cli")
        } else if let Some(value) = environment_value {
            (value, "environment")
        } else {
            (project_value.clone(), "project")
        };
        *target = value;
        sources.insert(field.into(), source.into());
    }
    Ok((resolved, sources))
}

/// Return only the environment variables that are part of the project
/// configuration contract. Unknown process variables never become implicit
/// configuration inputs.
pub fn process_environment() -> BTreeMap<String, String> {
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
    .filter_map(|key| std::env::var(key).ok().map(|value| (key.into(), value)))
    .collect()
}

fn choose_scalar(
    field: &'static str,
    explicit: Option<&str>,
    environment: Option<&str>,
    project: &str,
    sources: &mut BTreeMap<String, String>,
) -> String {
    let (value, source) = if let Some(value) = explicit {
        (value, "cli")
    } else if let Some(value) = environment {
        (value, "environment")
    } else {
        (project, "project")
    };
    sources.insert(field.into(), source.into());
    value.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedWorkspace {
    pub language: Option<String>,
    pub build_system: Option<String>,
    pub modules: Vec<String>,
    pub source_roots: Vec<String>,
    pub test_roots: Vec<String>,
    pub generated_roots: Vec<String>,
    pub vendor_roots: Vec<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub notes: Vec<String>,
}

impl DetectedWorkspace {
    pub fn into_config(self) -> ProjectConfig {
        let mut config = ProjectConfig::defaults();
        config.language = self.language.unwrap_or_default();
        config.build_system = self.build_system.unwrap_or_default();
        config.modules = self.modules;
        config.source_roots = self.source_roots;
        config.test_roots = self.test_roots;
        config.generated_roots = self.generated_roots;
        config.vendor_roots = self.vendor_roots;
        config.include = self.include;
        config.exclude = self.exclude;
        config
    }
}

pub fn detect_workspace(repo_root: &Path) -> Result<DetectedWorkspace, ConfigError> {
    if !repo_root.is_dir() {
        return Err(ConfigError::RepositoryMissing(repo_root.display().to_string()));
    }
    let mut files = Vec::new();
    collect_files(repo_root, repo_root, &mut files)?;
    let has_maven = files.iter().any(|path| path == Path::new("pom.xml"));
    let has_gradle = files.iter().any(|path| {
        matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some("settings.gradle")
                | Some("settings.gradle.kts")
                | Some("build.gradle")
                | Some("build.gradle.kts")
        )
    });
    let build_system = match (has_maven, has_gradle) {
        (true, false) => Some("maven".into()),
        (false, true) => Some("gradle".into()),
        (true, true) => None,
        (false, false) => None,
    };

    let mut java_files = Vec::new();
    let mut other_languages = BTreeSet::new();
    for file in &files {
        match file.extension().and_then(|extension| extension.to_str()) {
            Some("java") => java_files.push(file.clone()),
            Some("kt") | Some("kts") => {
                other_languages.insert("kotlin".to_string());
            }
            Some("py") => {
                other_languages.insert("python".to_string());
            }
            Some("js") | Some("ts") => {
                other_languages.insert("javascript".to_string());
            }
            Some("go") => {
                other_languages.insert("go".to_string());
            }
            Some("rs") => {
                other_languages.insert("rust".to_string());
            }
            _ => {}
        }
    }
    let language = match (java_files.is_empty(), other_languages.is_empty()) {
        (false, true) => Some("java".into()),
        (true, false) => {
            if other_languages.len() == 1 {
                other_languages.iter().next().cloned()
            } else {
                None
            }
        }
        (false, false) => None,
        (true, true) => None,
    };
    let mut notes = Vec::new();
    if has_maven && has_gradle {
        notes.push("both Maven and Gradle markers were detected".into());
    } else if build_system.is_none() {
        notes.push("no supported Maven/Gradle marker was detected".into());
    }
    if language.is_none() {
        if other_languages.is_empty() && java_files.is_empty() {
            notes.push("no Java or supported-language source file was detected".into());
        } else if !other_languages.is_empty() {
            notes.push(format!("language detection is ambiguous: {}", join_set(&other_languages)));
        }
    } else if language.as_deref() != Some("java") {
        notes.push(format!("detected unsupported language: {}", language.as_deref().unwrap_or("unknown")));
    }

    let (modules, module_notes, has_explicit_modules) = detect_modules(repo_root, &files)?;
    notes.extend(module_notes);
    let (source_roots, test_roots, generated_roots, vendor_roots) = detect_roots(repo_root, &files, &java_files, &modules, has_explicit_modules);
    let include = source_roots
        .iter()
        .map(|root| if root == "." { "**".into() } else { format!("{root}/**") })
        .collect();
    let exclude = vec![
        "target/**".into(),
        "build/**".into(),
        ".gradle/**".into(),
        ".ruleops/**".into(),
    ];
    Ok(DetectedWorkspace {
        language,
        build_system,
        modules,
        source_roots,
        test_roots,
        generated_roots,
        vendor_roots,
        include,
        exclude,
        notes,
    })
}

fn collect_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<(), ConfigError> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| ConfigError::Io { path: current.display().to_string(), detail: error.to_string() })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ConfigError::Io { path: current.display().to_string(), detail: error.to_string() })?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        let metadata = fs::symlink_metadata(&path).map_err(|error| ConfigError::Io { path: path.display().to_string(), detail: error.to_string() })?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            if matches!(name.as_ref(), ".git" | ".ruleops" | "target" | "build" | ".gradle" | "node_modules") {
                continue;
            }
            collect_files(root, &path, files)?;
        } else if metadata.is_file() {
            files.push(relative.to_path_buf());
        }
    }
    Ok(())
}

fn detect_modules(root: &Path, files: &[PathBuf]) -> Result<(Vec<String>, Vec<String>, bool), ConfigError> {
    let mut modules = BTreeSet::new();
    let mut notes = Vec::new();
    let mut has_explicit_modules = false;
    if let Some(root_pom) = files.iter().find(|file| *file == Path::new("pom.xml")) {
        let path = root.join(root_pom);
        let text = fs::read_to_string(&path).map_err(|error| ConfigError::Io { path: path.display().to_string(), detail: error.to_string() })?;
        let (declared, uncertain) = parse_maven_modules(&text);
        has_explicit_modules = !declared.is_empty();
        modules.extend(declared);
        if uncertain {
            notes.push(format!("Maven module detection is ambiguous in {}", root_pom.display()));
        }
    } else if files.iter().any(|file| file.file_name().and_then(|name| name.to_str()) == Some("pom.xml")) {
        notes.push("Maven module detection is ambiguous because the repository has nested pom.xml files but no root pom.xml".into());
    }
    for file in files {
        match file.file_name().and_then(|name| name.to_str()) {
            Some("settings.gradle") | Some("settings.gradle.kts") => {
                let path = root.join(file);
                let text = fs::read_to_string(&path).map_err(|error| ConfigError::Io { path: path.display().to_string(), detail: error.to_string() })?;
                let (declared, uncertain) = parse_gradle_modules(&text);
                has_explicit_modules |= !declared.is_empty();
                modules.extend(declared);
                if uncertain {
                    notes.push(format!("Gradle module detection is ambiguous in {}", file.display()));
                }
            }
            _ => {}
        }
    }
    // An aggregator's root directory is not an analysis module merely because
    // it contains child projects. Keeping `.` alongside declared modules would
    // make ProjectScanScope::selects accept every path under the repository,
    // including nested or vendored projects that are outside the build graph.
    // Retain it only for a single-project layout or when the root itself has a
    // conventional Java source set.
    if !has_explicit_modules || has_root_java_source_set(files) {
        modules.insert(".".to_string());
    }
    Ok((modules.into_iter().collect(), notes, has_explicit_modules))
}

fn has_root_java_source_set(files: &[PathBuf]) -> bool {
    files.iter().any(|file| {
        file.starts_with(Path::new("src/main/java"))
            || file.starts_with(Path::new("src/test/java"))
            || file.starts_with(Path::new("src/generated"))
    })
}

/// Read only explicit root-POM module declarations. A nested pom is not
/// evidence that it belongs to the repository's build graph: it may be an
/// embedded example, vendored project, or unrelated fixture. Dynamic Maven
/// module expressions are deliberately reported as ambiguous so non-
/// interactive init fails closed.
fn parse_maven_modules(text: &str) -> (Vec<String>, bool) {
    let (text, comments_unclosed) = strip_xml_comments(text);
    let Some(start) = text.find("<modules") else {
        return (Vec::new(), comments_unclosed);
    };
    let Some(open_end) = text[start..].find('>') else {
        return (Vec::new(), true);
    };
    let content_start = start + open_end + 1;
    let Some(close_relative) = text[content_start..].find("</modules>") else {
        return (Vec::new(), true);
    };
    let content = &text[content_start..content_start + close_relative];
    let mut modules = BTreeSet::new();
    let mut cursor = 0usize;
    let mut uncertain = comments_unclosed;
    while let Some(open_relative) = content[cursor..].find("<module") {
        let open = cursor + open_relative;
        let Some(open_end_relative) = content[open..].find('>') else {
            uncertain = true;
            break;
        };
        let value_start = open + open_end_relative + 1;
        let Some(close_relative) = content[value_start..].find("</module>") else {
            uncertain = true;
            break;
        };
        let value_end = value_start + close_relative;
        let value = content[value_start..value_end].trim().replace('\\', "/");
        if value.is_empty() || value.contains("${") || value.contains('}') || value.starts_with('/') || value.split('/').any(|part| part.is_empty() || part == ".." || part == ".") {
            uncertain = true;
        } else {
            modules.insert(value.trim_start_matches("./").trim_end_matches("/pom.xml").to_string());
        }
        cursor = value_end + "</module>".len();
    }
    (modules.into_iter().collect(), uncertain)
}

fn strip_xml_comments(text: &str) -> (String, bool) {
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut unclosed = false;
    while let Some(start_relative) = text[cursor..].find("<!--") {
        let start = cursor + start_relative;
        output.push_str(&text[cursor..start]);
        let content_start = start + "<!--".len();
        let Some(end_relative) = text[content_start..].find("-->") else {
            unclosed = true;
            break;
        };
        cursor = content_start + end_relative + "-->".len();
    }
    output.push_str(&text[cursor..]);
    (output, unclosed)
}

fn detect_roots(
    root: &Path,
    files: &[PathBuf],
    java_files: &[PathBuf],
    modules: &[String],
    explicit_modules: bool,
) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    let standard = [
        ("src/main/java", 0usize),
        ("src/test/java", 1usize),
        ("target/generated-sources", 2usize),
        ("src/generated", 2usize),
        ("vendor", 3usize),
    ];
    let mut groups = [BTreeSet::new(), BTreeSet::new(), BTreeSet::new(), BTreeSet::new()];
    for (candidate, index) in standard {
        if root.join(candidate).is_dir() || files.iter().any(|path| path.starts_with(candidate)) {
            groups[index].insert(candidate.to_string());
        }
    }
    for file in java_files {
        if let Some(vendor_root) = vendor_like_root(file) {
            groups[3].insert(vendor_root);
            continue;
        }
        insert_suffix_root(file, &["src", "main", "java"], &mut groups[0]);
        insert_suffix_root(file, &["src", "test", "java"], &mut groups[1]);
        insert_suffix_root(file, &["target", "generated-sources"], &mut groups[2]);
        insert_suffix_root(file, &["src", "generated"], &mut groups[2]);
    }
    if groups[0].is_empty() {
        if let Some(file) = java_files.first() {
            let parent = file
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let parent = parent.to_string_lossy().replace('\\', "/");
            groups[0].insert(parent);
        }
    }
    let build_roots = files
        .iter()
        .filter(|file| {
            matches!(
                file.file_name().and_then(|name| name.to_str()),
                Some("pom.xml")
                    | Some("settings.gradle")
                    | Some("settings.gradle.kts")
                    | Some("build.gradle")
                    | Some("build.gradle.kts")
            )
        })
        .map(|file| file.parent().map(|path| path.to_string_lossy().replace('\\', "/")).unwrap_or_default())
        .collect::<BTreeSet<_>>();
    for group in groups[..3].iter_mut() {
        group.retain(|candidate| {
            !is_vendor_like_path(candidate)
                && modules.iter().any(|module| path_under(candidate, module))
                && (!explicit_modules || belongs_to_declared_module(candidate, modules, &build_roots))
        });
    }
    (
        groups[0].iter().cloned().collect(),
        groups[1].iter().cloned().collect(),
        groups[2].iter().cloned().collect(),
        groups[3].iter().cloned().collect(),
    )
}

fn belongs_to_declared_module(candidate: &str, modules: &[String], build_roots: &BTreeSet<String>) -> bool {
    let Some(owning_module) = modules
        .iter()
        .filter(|module| path_under(candidate, module))
        .max_by_key(|module| module.len())
    else {
        return false;
    };
    let nearest_build_root = build_roots
        .iter()
        .filter(|build_root| !build_root.is_empty() && *build_root != "." && path_under(candidate, build_root))
        .max_by_key(|build_root| build_root.len());
    nearest_build_root.is_none_or(|build_root| build_root == owning_module || modules.iter().any(|module| module == build_root))
}

fn path_under(path: &str, root: &str) -> bool {
    let path = path.trim_matches('/');
    let root = root.trim_matches('/');
    root.is_empty() || root == "." || path == root || path.starts_with(&format!("{root}/"))
}

fn is_vendor_like_path(path: &str) -> bool {
    path.split('/').any(|part| {
        matches!(
            part,
            "vendor" | "vendors" | "third_party" | "third-party" | "external" | "externals" | "deps" | "dependency" | "dependencies"
        )
    })
}

fn vendor_like_root(file: &Path) -> Option<String> {
    let components = file.components().filter_map(|component| match component {
        Component::Normal(value) => value.to_str(),
        _ => None,
    }).collect::<Vec<_>>();
    components.iter().enumerate().find_map(|(index, component)| {
        if matches!(*component, "vendor" | "vendors" | "third_party" | "third-party" | "external" | "externals" | "deps" | "dependency" | "dependencies") {
            Some(components[..=index].join("/"))
        } else {
            None
        }
    })
}

fn insert_suffix_root(file: &Path, suffix: &[&str], output: &mut BTreeSet<String>) {
    let components = file.components().filter_map(|component| match component {
        Component::Normal(value) => value.to_str(),
        _ => None,
    }).collect::<Vec<_>>();
    let directory_len = components.len().saturating_sub(1);
    if directory_len < suffix.len() {
        return;
    }
    let directories = &components[..directory_len];
    for start in 0..=directories.len() - suffix.len() {
        if directories[start..start + suffix.len()] == *suffix {
            output.insert(directories[..start + suffix.len()].join("/"));
            return;
        }
    }
}

fn parse_gradle_modules(text: &str) -> (Vec<String>, bool) {
    let mut modules = BTreeSet::new();
    let mut uncertain = false;
    for raw_line in text.lines() {
        let line = raw_line.trim_start();
        if line.starts_with("//") || line.starts_with('#') || !line.starts_with("include") {
            continue;
        }
        let remainder = &line["include".len()..];
        if remainder.chars().next().is_some_and(|character| character.is_ascii_alphanumeric() || character == '_') {
            continue;
        }
        let mut values = Vec::new();
        let mut quote = None;
        let mut start = 0usize;
        let mut escaped = false;
        for (index, character) in remainder.char_indices() {
            if let Some(active) = quote {
                if active == '"' && escaped {
                    escaped = false;
                } else if active == '"' && character == '\\' {
                    escaped = true;
                } else if character == active {
                    values.push(remainder[start..index].to_string());
                    quote = None;
                }
            } else if character == '"' || character == '\'' {
                quote = Some(character);
                start = index + character.len_utf8();
            }
        }
        if quote.is_some() || values.is_empty() {
            uncertain = true;
            continue;
        }
        for value in values {
            let normalized = value.trim().trim_start_matches(':').replace(':', "/");
            if normalized.is_empty() || normalized.split('/').any(|part| part.is_empty() || part == "..") || normalized.starts_with('/') {
                uncertain = true;
            } else {
                modules.insert(normalized);
            }
        }
    }
    (modules.into_iter().collect(), uncertain)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorCheck {
    pub code: String,
    pub status: String,
    pub detail: String,
    #[serde(rename = "nextStep")]
    pub next_step: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorReport {
    pub api_version: String,
    pub kind: String,
    pub repository: String,
    pub mode: Option<ExecutionMode>,
    pub overall: String,
    pub identity: Option<ConfigIdentity>,
    pub sources: BTreeMap<String, String>,
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    pub fn exit_code(&self) -> i32 {
        if self.overall == "CONFIG_INVALID" || self.overall == "CONFIG_MISSING" {
            2
        } else if self.overall == "PASS" || self.overall == "PASS_WITH_SERVER_FETCH" {
            0
        } else {
            3
        }
    }

    pub fn render_text(&self) -> String {
        let mut lines = vec![
            format!("doctor={} mode={}", self.overall, self.mode.map(ExecutionMode::as_str).unwrap_or("unknown")),
            format!("repository={}", self.repository),
        ];
        if let Some(identity) = &self.identity {
            lines.push(format!("config_digest={}", identity.config_digest));
            lines.push(format!("workspace_descriptor_digest={}", identity.workspace_descriptor_digest));
            lines.push(format!("scan_plan_digest={}", identity.scan_plan_digest));
        }
        if !self.sources.is_empty() {
            lines.push(format!(
                "sources={}",
                self.sources
                    .iter()
                    .map(|(key, value)| format!("{key}:{value}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        for check in &self.checks {
            lines.push(format!("check code={} status={} detail={} next_step={}", check.code, check.status, check.detail, check.next_step));
        }
        lines.join("\n") + "\n"
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("DoctorReport is serializable") + "\n"
    }
}

pub fn run_doctor(repo_root: &Path, config_path: Option<&Path>, scan_probe: Option<&Path>) -> DoctorReport {
    run_doctor_with_overrides(repo_root, config_path, scan_probe, &ConfigOverrides::default(), &process_environment())
}

pub fn run_doctor_with_overrides(
    repo_root: &Path,
    config_path: Option<&Path>,
    scan_probe: Option<&Path>,
    overrides: &ConfigOverrides,
    environment: &BTreeMap<String, String>,
) -> DoctorReport {
    let repository = repo_root.display().to_string();
    let path = config_path.map(PathBuf::from).unwrap_or_else(|| repo_root.join(CONFIG_RELATIVE_PATH));
    let mut checks = Vec::new();
    if let Err(error) = reject_symlink_components("config_path", &path) {
        checks.push(DoctorCheck { code: "CONFIG_INVALID".into(), status: "FAIL".into(), detail: error.to_string(), next_step: "replace the configuration symlink with a regular project file and rerun rf doctor".into() });
        checks.push(not_run("DEPENDENCY_CHECK_NOT_RUN", "configuration is required before dependency checks"));
        checks.push(not_run("CACHE_CHECK_NOT_RUN", "configuration is required before cache checks"));
        checks.push(not_run("TRUST_STATE_CHECK_NOT_RUN", "configuration is required before trust checks"));
        checks.push(not_run("SCAN_CHECK_NOT_RUN", "configuration is required before scan checks"));
        return DoctorReport { api_version: "rulefactory.io/v1".into(), kind: "DoctorReport".into(), repository, mode: None, overall: "CONFIG_INVALID".into(), identity: None, sources: BTreeMap::new(), checks };
    }
    let (config, identity, mode, sources) = match fs::read_to_string(&path) {
        Ok(text) => match parse_config(&text) {
            Ok(project) => {
                match resolve_precedence(&project, overrides, environment) {
                    Ok((config, sources)) => match (config.validate(), validate_workspace_paths(repo_root, &config)) {
                        (Ok(()), Ok(())) => {
                            let identity = config.identity();
                            checks.push(DoctorCheck { code: "CONFIG_VALID".into(), status: "PASS".into(), detail: path.display().to_string(), next_step: "none".into() });
                            (Some(config.clone()), Some(identity), Some(config.execution_mode), sources)
                        }
                        (Err(error), _) | (_, Err(error)) => {
                            checks.push(DoctorCheck { code: "CONFIG_INVALID".into(), status: "FAIL".into(), detail: error.to_string(), next_step: "run rf config validate after correcting the project config".into() });
                            (None, None, None, BTreeMap::new())
                        }
                    },
                    Err(error) => {
                        checks.push(DoctorCheck { code: "CONFIG_INVALID".into(), status: "FAIL".into(), detail: error.to_string(), next_step: "run rf config validate after correcting the project config".into() });
                        (None, None, None, BTreeMap::new())
                    }
                }
            }
            Err(error) => {
                checks.push(DoctorCheck { code: "CONFIG_INVALID".into(), status: "FAIL".into(), detail: error.to_string(), next_step: "run rf config validate after correcting the project config".into() });
                (None, None, None, BTreeMap::new())
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            checks.push(DoctorCheck { code: "CONFIG_MISSING".into(), status: "FAIL".into(), detail: path.display().to_string(), next_step: "run rf init --non-interactive after reviewing detection".into() });
            (None, None, None, BTreeMap::new())
        }
        Err(error) => {
            checks.push(DoctorCheck { code: "CONFIG_UNREADABLE".into(), status: "FAIL".into(), detail: error.to_string(), next_step: "check config permissions and rerun rf doctor".into() });
            (None, None, None, BTreeMap::new())
        }
    };
    let Some(config) = config else {
        checks.push(not_run("DEPENDENCY_CHECK_NOT_RUN", "configuration is required before dependency checks"));
        checks.push(not_run("CACHE_CHECK_NOT_RUN", "configuration is required before cache checks"));
        checks.push(not_run("TRUST_STATE_CHECK_NOT_RUN", "configuration is required before trust checks"));
        checks.push(not_run("SCAN_CHECK_NOT_RUN", "configuration is required before scan checks"));
        let overall = if checks.iter().any(|check| check.code == "CONFIG_MISSING") { "CONFIG_MISSING" } else { "CONFIG_INVALID" };
        return DoctorReport { api_version: "rulefactory.io/v1".into(), kind: "DoctorReport".into(), repository, mode, overall: overall.into(), identity, sources, checks };
    };

    let dependencies_ok = dependency_check(&config, &mut checks);
    let cache_status = cache_check(&config, repo_root, &mut checks);
    let trust_ok = trust_check(&config, repo_root, &mut checks);
    let scan_ok = scan_check(scan_probe, &mut checks);
    let cache_allows_connected = config.execution_mode == ExecutionMode::Connected
        && matches!(cache_status.as_str(), "CACHE_MISSING" | "CACHE_EMPTY");
    let cache_failure = match config.execution_mode {
        ExecutionMode::Connected => matches!(cache_status.as_str(), "CACHE_INVALID" | "CACHE_TRUST_EVIDENCE_REQUIRED"),
        ExecutionMode::Offline | ExecutionMode::Standalone => !matches!(cache_status.as_str(), "CACHE_VALID"),
    };
    let overall = if !dependencies_ok || !trust_ok || !scan_ok || cache_failure {
        "INCONCLUSIVE"
    } else if cache_allows_connected {
        "PASS_WITH_SERVER_FETCH"
    } else {
        "PASS"
    };
    DoctorReport { api_version: "rulefactory.io/v1".into(), kind: "DoctorReport".into(), repository, mode: Some(config.execution_mode), overall: overall.into(), identity, sources, checks }
}

fn not_run(code: &str, detail: &str) -> DoctorCheck {
    DoctorCheck { code: code.into(), status: "NOT_RUN".into(), detail: detail.into(), next_step: "resolve the preceding configuration failure".into() }
}

fn dependency_check(config: &ProjectConfig, checks: &mut Vec<DoctorCheck>) -> bool {
    let mut commands = vec!["java"];
    if config.build_system == "maven" {
        commands.push("mvn");
    } else if config.build_system == "gradle" {
        commands.push("gradle");
    }
    let mut okay = true;
    for command in commands {
        match Command::new(command).arg("--version").output() {
            Ok(output) if output.status.success() => checks.push(DoctorCheck { code: "DEPENDENCY_READY".into(), status: "PASS".into(), detail: command.into(), next_step: "none".into() }),
            Ok(output) => {
                okay = false;
                checks.push(DoctorCheck { code: "DEPENDENCY_UNAVAILABLE".into(), status: "FAIL".into(), detail: format!("{command} exited with {}", output.status), next_step: format!("install or expose {command} on PATH") });
            }
            Err(error) => {
                okay = false;
                checks.push(DoctorCheck { code: "DEPENDENCY_UNAVAILABLE".into(), status: "FAIL".into(), detail: format!("{command}: {error}"), next_step: format!("install or expose {command} on PATH") });
            }
        }
    }
    okay
}

fn cache_check(config: &ProjectConfig, repo_root: &Path, checks: &mut Vec<DoctorCheck>) -> String {
    let root = config.resolved_cache_dir(repo_root);
    let object_root = root.join("sha256");
    let metadata = match fs::symlink_metadata(&object_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let code = match fs::symlink_metadata(&root) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => "CACHE_EMPTY",
                Ok(_) => "CACHE_INVALID",
                Err(_) => "CACHE_MISSING",
            };
            if code == "CACHE_INVALID" {
                checks.push(DoctorCheck { code: code.into(), status: "FAIL".into(), detail: root.display().to_string(), next_step: "replace the cache path with a directory owned by the project".into() });
                return code.into();
            }
            checks.push(DoctorCheck { code: code.into(), status: "READY_WITH_FETCH".into(), detail: root.display().to_string(), next_step: if config.execution_mode == ExecutionMode::Connected { "fetch a verified bundle from the configured Server".into() } else { "populate the immutable cache before offline execution".into() } });
            return code.into();
        }
        Err(error) => {
            checks.push(DoctorCheck { code: "CACHE_INVALID".into(), status: "FAIL".into(), detail: error.to_string(), next_step: "repair or replace the cache directory after preserving its digest identity".into() });
            return "CACHE_INVALID".into();
        }
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        checks.push(DoctorCheck { code: "CACHE_INVALID".into(), status: "FAIL".into(), detail: "cache sha256 path is not a directory".into(), next_step: "remove the unsafe cache object and rerun doctor".into() });
        return "CACHE_INVALID".into();
    }
    let cache = FileBundleCache::new(&root);
    let mut count = 0usize;
    let entries = match fs::read_dir(&object_root) {
        Ok(entries) => entries,
        Err(error) => {
            checks.push(DoctorCheck { code: "CACHE_INVALID".into(), status: "FAIL".into(), detail: error.to_string(), next_step: "repair cache permissions and rerun doctor".into() });
            return "CACHE_INVALID".into();
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                checks.push(DoctorCheck { code: "CACHE_INVALID".into(), status: "FAIL".into(), detail: error.to_string(), next_step: "repair cache enumeration and rerun doctor".into() });
                return "CACHE_INVALID".into();
            }
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(digest) = parse_hex_digest(&name) else {
            checks.push(DoctorCheck { code: "CACHE_INVALID".into(), status: "FAIL".into(), detail: format!("invalid digest object name {name}"), next_step: "remove the invalid cache object".into() });
            return "CACHE_INVALID".into();
        };
        match cache.get(&digest) {
            Ok(Some(_)) => count += 1,
            Ok(None) => {}
            Err(error) => {
                checks.push(DoctorCheck { code: "CACHE_INVALID".into(), status: "FAIL".into(), detail: error.to_string(), next_step: "replace the corrupted cache object from a verified source".into() });
                return "CACHE_INVALID".into();
            }
        }
    }
    if count == 0 {
        checks.push(DoctorCheck { code: "CACHE_EMPTY".into(), status: "READY_WITH_FETCH".into(), detail: root.display().to_string(), next_step: "fetch a verified bundle before scanning".into() });
        return "CACHE_EMPTY".into();
    }
    checks.push(DoctorCheck { code: "CACHE_TRUST_EVIDENCE_REQUIRED".into(), status: "INCONCLUSIVE".into(), detail: format!("{count} digest object(s) rechecked; signature/freshness/revocation/execution identity are not inferred from a cache hit"), next_step: "resolve the bundle through RuleBundle delivery verification before scanning".into() });
    "CACHE_TRUST_EVIDENCE_REQUIRED".into()
}

fn trust_check(config: &ProjectConfig, repo_root: &Path, checks: &mut Vec<DoctorCheck>) -> bool {
    let path = config.resolved_trust_state_path(repo_root);
    match TrustStateFile::new(&path).load_for_trusted_execution() {
        Ok(state) => {
            checks.push(DoctorCheck { code: "TRUST_STATE_VALID".into(), status: "PASS".into(), detail: format!("checkpoint_version={}", state.highest_checkpoint_version()), next_step: "none".into() });
            true
        }
        Err(TrustStateStoreError::Missing) => {
            checks.push(DoctorCheck { code: "TRUST_STATE_MISSING".into(), status: "INCONCLUSIVE".into(), detail: path.display().to_string(), next_step: "initialize the trust checkpoint explicitly before trusted execution".into() });
            false
        }
        Err(error) => {
            checks.push(DoctorCheck { code: "TRUST_STATE_INVALID".into(), status: "FAIL".into(), detail: error.to_string(), next_step: "repair the trust state through its signed checkpoint workflow".into() });
            false
        }
    }
}

fn scan_check(scan_probe: Option<&Path>, checks: &mut Vec<DoctorCheck>) -> bool {
    let Some(scan_probe) = scan_probe else {
        checks.push(DoctorCheck { code: "SCAN_NOT_RUN".into(), status: "NOT_RUN".into(), detail: "doctor checks prerequisites only unless --scan-probe is supplied".into(), next_step: "run rf scan with the reviewed project config".into() });
        return true;
    };
    match Command::new(scan_probe).arg("--version").output() {
        Ok(output) if output.status.success() => {
            checks.push(DoctorCheck { code: "SCAN_READY".into(), status: "PASS".into(), detail: scan_probe.display().to_string(), next_step: "none".into() });
            true
        }
        Ok(output) => {
            checks.push(DoctorCheck { code: "SCAN_FAILED".into(), status: "FAIL".into(), detail: format!("{} exited with {}", scan_probe.display(), output.status), next_step: "inspect the structured scan failure before retrying".into() });
            false
        }
        Err(error) => {
            checks.push(DoctorCheck { code: "SCAN_FAILED".into(), status: "FAIL".into(), detail: error.to_string(), next_step: "make the scan command available or correct its path".into() });
            false
        }
    }
}

pub fn parse_config(text: &str) -> Result<ProjectConfig, ConfigError> {
    let mut values = BTreeMap::<String, String>::new();
    for (index, raw) in text.lines().enumerate() {
        let line_number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(ConfigError::Parse { line: line_number, detail: "expected key: value".into() });
        };
        let key = key.trim();
        if !known_key(key) {
            return Err(ConfigError::UnknownField(key.into()));
        }
        if values.insert(key.into(), value.trim().into()).is_some() {
            return Err(ConfigError::DuplicateField(key.into()));
        }
    }
    let get = |key: &'static str| values.get(key).ok_or(ConfigError::MissingField(key));
    let string = |key: &'static str| parse_scalar(key, get(key)?);
    let list = |key: &'static str| parse_list(key, get(key)?);
    let defaults = ProjectConfig::defaults();
    let optional_list = |key: &'static str| values.get(key).map(|value| parse_list(key, value)).transpose().map(|value| value.unwrap_or_default());
    let include = values
        .get("include")
        .map(|value| parse_list("include", value))
        .transpose()?
        .unwrap_or_else(|| defaults.include.clone());
    let optional_string = |key: &'static str, default: &str| values.get(key).map(|value| parse_scalar(key, value)).transpose().map(|value| value.unwrap_or_else(|| default.into()));
    let config = ProjectConfig {
        version: string("version")?,
        language: string("language")?,
        build_system: string("build_system")?,
        modules: list("modules")?,
        source_roots: list("source_roots")?,
        test_roots: optional_list("test_roots")?,
        generated_roots: optional_list("generated_roots")?,
        vendor_roots: optional_list("vendor_roots")?,
        include,
        exclude: optional_list("exclude")?,
        execution_mode: ExecutionMode::parse(&optional_string("execution_mode", defaults.execution_mode.as_str())?)?,
        cache_dir: optional_string("cache_dir", &defaults.cache_dir)?,
        trust_state_path: optional_string("trust_state_path", &defaults.trust_state_path)?,
    };
    config.validate()?;
    Ok(config)
}

pub fn load_config(repo_root: &Path, config_path: Option<&Path>) -> Result<(ProjectConfig, PathBuf), ConfigError> {
    let path = config_path.map(PathBuf::from).unwrap_or_else(|| repo_root.join(CONFIG_RELATIVE_PATH));
    reject_symlink_components("config_path", &path)?;
    let text = fs::read_to_string(&path).map_err(|error| ConfigError::Io { path: path.display().to_string(), detail: error.to_string() })?;
    Ok((parse_config(&text)?, path))
}

pub fn write_config(path: &Path, config: &ProjectConfig) -> Result<(), ConfigError> {
    config.validate()?;
    reject_symlink_components("config_output", path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| ConfigError::Io { path: parent.display().to_string(), detail: error.to_string() })?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(path).map_err(|error| ConfigError::Io { path: path.display().to_string(), detail: error.to_string() })?;
    use std::io::Write;
    file.write_all(config.to_yaml().as_bytes()).map_err(|error| ConfigError::Io { path: path.display().to_string(), detail: error.to_string() })?;
    file.sync_all().map_err(|error| ConfigError::Io { path: path.display().to_string(), detail: error.to_string() })
}

fn known_key(key: &str) -> bool {
    matches!(key, "version" | "language" | "build_system" | "modules" | "source_roots" | "test_roots" | "generated_roots" | "vendor_roots" | "include" | "exclude" | "execution_mode" | "cache_dir" | "trust_state_path")
}

fn parse_scalar(field: &'static str, value: &str) -> Result<String, ConfigError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ConfigError::MissingField(field));
    }
    let value = unquote(value).ok_or_else(|| ConfigError::Parse { line: 0, detail: format!("invalid quoted value for {field}") })?;
    if value.chars().any(char::is_control) {
        return Err(ConfigError::InvalidValue { field, value });
    }
    Ok(value)
}

fn parse_list(field: &'static str, value: &str) -> Result<Vec<String>, ConfigError> {
    let value = value.trim();
    if value == "[]" {
        return Ok(Vec::new());
    }
    if !value.starts_with('[') || !value.ends_with(']') {
        return Err(ConfigError::Parse { line: 0, detail: format!("{field} must be an inline array") });
    }
    let inner = &value[1..value.len() - 1];
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    split_inline_items(inner, field)?
        .into_iter()
        .map(|item| {
            let item = item.trim();
            let value = unquote(item).ok_or_else(|| ConfigError::Parse { line: 0, detail: format!("invalid list item in {field}") })?;
            if value.is_empty() || value.chars().any(char::is_control) {
                return Err(ConfigError::InvalidValue { field, value });
            }
            Ok(value)
        })
        .collect()
}

fn split_inline_items<'a>(inner: &'a str, field: &'static str) -> Result<Vec<&'a str>, ConfigError> {
    let mut items = Vec::new();
    let mut start = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut cursor = inner.char_indices().peekable();
    while let Some((index, character)) = cursor.next() {
        match quote {
            Some('"') => {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    quote = None;
                }
            }
            Some('\'') => {
                if character == '\'' {
                    if cursor.peek().is_some_and(|(_, next)| *next == '\'') {
                        cursor.next();
                    } else {
                        quote = None;
                    }
                }
            }
            Some(_) => {}
            None if character == '"' || character == '\'' => quote = Some(character),
            None if character == ',' => {
                items.push(inner[start..index].trim());
                start = index + character.len_utf8();
            }
            None => {}
        }
    }
    if quote.is_some() || escaped {
        return Err(ConfigError::Parse { line: 0, detail: format!("invalid quoted list in {field}") });
    }
    items.push(inner[start..].trim());
    Ok(items)
}

fn unquote(value: &str) -> Option<String> {
    if value.starts_with('"') || value.starts_with('\'') {
        if value.len() < 2 || !value.ends_with(value.chars().next().unwrap()) {
            return None;
        }
        if value.starts_with('"') {
            serde_json::from_str(value).ok()
        } else {
            Some(value[1..value.len() - 1].replace("''", "'"))
        }
    } else {
        Some(value.into())
    }
}

fn validate_token(field: &'static str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(ConfigError::InvalidValue { field, value: value.into() });
    }
    Ok(())
}

fn validate_unique_paths(field: &'static str, values: &[String]) -> Result<(), ConfigError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(ConfigError::ConflictingValues { field, detail: format!("duplicate value {value}") });
        }
    }
    Ok(())
}

fn validate_relative_path(field: &'static str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(ConfigError::InvalidValue { field, value: value.into() });
    }
    let path = Path::new(value);
    if path.is_absolute() || path.components().any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
        return Err(ConfigError::PathEscapesWorkspace { field, value: value.into() });
    }
    Ok(())
}

/// Existing symlinks are rejected at every component of a project-controlled
/// path. This keeps cache, TrustState, roots and glob prefixes inside the
/// workspace even when an attacker has planted a link under `.ruleops`.
pub fn validate_workspace_paths(repo_root: &Path, config: &ProjectConfig) -> Result<(), ConfigError> {
    if !repo_root.is_dir() {
        return Err(ConfigError::RepositoryMissing(repo_root.display().to_string()));
    }
    if fs::symlink_metadata(repo_root).map(|metadata| metadata.file_type().is_symlink()).unwrap_or(false) {
        return Err(ConfigError::PathUsesSymlink { field: "repo_root", value: repo_root.display().to_string() });
    }
    for (field, values) in [
        ("modules", &config.modules),
        ("source_roots", &config.source_roots),
        ("test_roots", &config.test_roots),
        ("generated_roots", &config.generated_roots),
        ("vendor_roots", &config.vendor_roots),
        ("include", &config.include),
        ("exclude", &config.exclude),
    ] {
        for value in values {
            reject_workspace_symlink(repo_root, field, value)?;
        }
    }
    reject_workspace_symlink(repo_root, "cache_dir", &config.cache_dir)?;
    reject_workspace_symlink(repo_root, "trust_state_path", &config.trust_state_path)?;
    Ok(())
}

pub fn validate_output_path(repo_root: &Path, output: &Path) -> Result<(), ConfigError> {
    if !output.starts_with(repo_root)
        || output
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(ConfigError::PathEscapesWorkspace { field: "output", value: output.display().to_string() });
    }
    let relative = output.strip_prefix(repo_root).unwrap_or(output);
    let mut current = repo_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else { continue };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ConfigError::PathUsesSymlink { field: "output", value: output.display().to_string() });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(ConfigError::Io { path: current.display().to_string(), detail: error.to_string() }),
        }
    }
    Ok(())
}

fn reject_workspace_symlink(repo_root: &Path, field: &'static str, value: &str) -> Result<(), ConfigError> {
    let mut current = repo_root.to_path_buf();
    for component in Path::new(value).components() {
        let Component::Normal(name) = component else { continue };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ConfigError::PathUsesSymlink { field, value: value.into() });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(ConfigError::Io { path: current.display().to_string(), detail: error.to_string() }),
        }
    }
    Ok(())
}

fn reject_symlink_components(field: &'static str, path: &Path) -> Result<(), ConfigError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(ConfigError::PathUsesSymlink { field, value: path.display().to_string() });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => return Err(ConfigError::Io { path: current.display().to_string(), detail: error.to_string() }),
        }
    }
    Ok(())
}

fn parse_csv(field: &'static str, value: &str) -> Result<Vec<String>, ConfigError> {
    if value.trim().is_empty() {
        return Err(ConfigError::InvalidValue { field, value: value.into() });
    }
    value.split(',').map(|item| {
        let item = item.trim().to_string();
        if item.is_empty() { Err(ConfigError::InvalidValue { field, value: value.into() }) } else { Ok(item) }
    }).collect()
}

fn yaml_list(values: &[String]) -> String {
    let items = values.iter().map(|value| serde_json::to_string(value).expect("config string is serializable")).collect::<Vec<_>>();
    format!("[{}]", items.join(", "))
}

fn join_set(values: &BTreeSet<String>) -> String {
    values.iter().cloned().collect::<Vec<_>>().join(",")
}

fn digest_bytes(bytes: &[u8]) -> String {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    format!("sha256:{}", digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>())
}

fn parse_hex_digest(value: &str) -> Result<[u8; 32], ConfigError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ConfigError::InvalidCacheObject(value.into()));
    }
    let mut digest = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        digest[index] = (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]);
    }
    Ok(digest)
}

fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => 0,
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("repository is missing or not a directory: {0}")]
    RepositoryMissing(String),
    #[error("configuration file I/O failed for {path}: {detail}")]
    Io { path: String, detail: String },
    #[error("configuration parse error on line {line}: {detail}")]
    Parse { line: usize, detail: String },
    #[error("configuration field is unknown: {0}")]
    UnknownField(String),
    #[error("configuration field is duplicated: {0}")]
    DuplicateField(String),
    #[error("configuration field is missing: {0}")]
    MissingField(&'static str),
    #[error("configuration value is invalid for {field}: {value}")]
    InvalidValue { field: &'static str, value: String },
    #[error("configuration values conflict for {field}: {detail}")]
    ConflictingValues { field: &'static str, detail: String },
    #[error("configuration path escapes the workspace for {field}: {value}")]
    PathEscapesWorkspace { field: &'static str, value: String },
    #[error("configuration path uses a symbolic link for {field}: {value}")]
    PathUsesSymlink { field: &'static str, value: String },
    #[error("configuration version is unsupported: {0}")]
    UnsupportedVersion(String),
    #[error("language is unsupported: {0}")]
    UnsupportedLanguage(String),
    #[error("build system is unsupported: {0}")]
    UnsupportedBuildSystem(String),
    #[error("cache object name is not a sha256 digest: {0}")]
    InvalidCacheObject(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn java_config() -> ProjectConfig {
        ProjectConfig {
            version: CONFIG_VERSION.into(),
            language: "java".into(),
            build_system: "maven".into(),
            modules: vec![".".into()],
            source_roots: vec!["src/main/java".into()],
            test_roots: vec!["src/test/java".into()],
            generated_roots: Vec::new(),
            vendor_roots: Vec::new(),
            include: vec!["src/main/java/**".into()],
            exclude: vec!["target/**".into()],
            execution_mode: ExecutionMode::Connected,
            cache_dir: DEFAULT_CACHE_RELATIVE_PATH.into(),
            trust_state_path: DEFAULT_TRUST_STATE_RELATIVE_PATH.into(),
        }
    }

    #[test]
    fn generated_config_round_trips_and_has_stable_identity() {
        let config = java_config();
        let parsed = parse_config(&config.to_yaml()).unwrap();
        assert_eq!(parsed, config);
        assert_eq!(parsed.config_digest(), config.config_digest());
        assert_eq!(parsed.identity(), config.identity());
    }

    #[test]
    fn duplicate_and_unknown_fields_fail_closed() {
        let yaml = format!("{}\nlanguage: java\n", java_config().to_yaml());
        assert!(matches!(parse_config(&yaml), Err(ConfigError::DuplicateField(field)) if field == "language"));
        assert!(matches!(parse_config("unknown: value\n"), Err(ConfigError::UnknownField(_))));
    }

    #[test]
    fn precedence_is_cli_then_environment_then_project() {
        let config = java_config();
        let mut environment = BTreeMap::new();
        environment.insert("RULEOPS_EXECUTION_MODE".into(), "offline".into());
        environment.insert("RULEOPS_INCLUDE".into(), "src/main/java/**,src/generated/**".into());
        let mut overrides = ConfigOverrides::default();
        overrides.execution_mode = Some("standalone".into());
        let (resolved, sources) = apply_precedence(&config, &overrides, &environment).unwrap();
        assert_eq!(resolved.execution_mode, ExecutionMode::Standalone);
        assert_eq!(resolved.include, vec!["src/main/java/**", "src/generated/**"]);
        assert_eq!(sources["execution_mode"], "cli");
        assert_eq!(sources["include"], "environment");
    }

    #[test]
    fn conflicting_include_and_exclude_is_rejected() {
        let mut config = java_config();
        config.exclude.push("src/main/java/**".into());
        assert!(matches!(config.validate(), Err(ConfigError::ConflictingValues { field: "include/exclude", .. })));
    }

    #[test]
    fn detector_is_java_maven_and_does_not_follow_symlinks() {
        let root = std::env::temp_dir().join(format!("erf-project-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src/main/java/example")).unwrap();
        fs::create_dir_all(root.join("app")).unwrap();
        fs::write(root.join("pom.xml"), "<project/>").unwrap();
        fs::write(root.join("src/main/java/example/App.java"), "class App {}").unwrap();
        let detected = detect_workspace(&root).unwrap();
        assert_eq!(detected.language.as_deref(), Some("java"));
        assert_eq!(detected.build_system.as_deref(), Some("maven"));
        assert_eq!(detected.source_roots, vec!["src/main/java"]);
        let outside = root.with_extension("outside");
        let mut link_file = fs::File::create(&outside).unwrap();
        writeln!(link_file, "not a directory").unwrap();
        fs::remove_file(outside).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_trust_state_is_not_reported_as_ready() {
        let root = std::env::temp_dir().join(format!("erf-project-doctor-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".ruleops")).unwrap();
        write_config(&root.join(CONFIG_RELATIVE_PATH), &java_config()).unwrap();
        let report = run_doctor(&root, None, None);
        assert_eq!(report.overall, "INCONCLUSIVE");
        assert!(report.checks.iter().any(|check| check.code == "TRUST_STATE_MISSING"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn quoted_commas_round_trip_without_changing_identity() {
        let mut config = java_config();
        config.include = vec!["src/a,b/**".into()];
        let parsed = parse_config(&config.to_yaml()).unwrap();
        assert_eq!(parsed.include, config.include);
        assert_eq!(parsed.config_digest(), config.config_digest());
    }

    #[test]
    fn root_level_java_file_gets_dot_source_root() {
        let root = std::env::temp_dir().join(format!("erf-project-config-root-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("pom.xml"), "<project/>").unwrap();
        fs::write(root.join("App.java"), "class App {}").unwrap();
        let detected = detect_workspace(&root).unwrap();
        assert_eq!(detected.source_roots, vec!["."]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn maven_modules_follow_root_declarations_and_ignore_nested_projects() {
        let root = std::env::temp_dir().join(format!("erf-project-config-maven-modules-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("app/src/main/java/example")).unwrap();
        fs::create_dir_all(root.join("unrelated/src/main/java/example")).unwrap();
        fs::write(root.join("pom.xml"), "<project><modules><module>app</module></modules></project>").unwrap();
        fs::write(root.join("app/pom.xml"), "<project/>").unwrap();
        fs::write(root.join("unrelated/pom.xml"), "<project/>").unwrap();
        fs::write(root.join("app/src/main/java/example/App.java"), "class App {}\n").unwrap();
        fs::write(root.join("unrelated/src/main/java/example/Other.java"), "class Other {}\n").unwrap();
        let detected = detect_workspace(&root).unwrap();
        assert_eq!(detected.modules, vec!["app"]);
        assert_eq!(detected.source_roots, vec!["app/src/main/java"]);
        assert!(!detected.source_roots.iter().any(|root| root.starts_with("unrelated/")));
        assert!(!detected.notes.iter().any(|note| note.contains("ambiguous")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_maven_root_source_excludes_untracked_nested_and_vendor_roots() {
        let root = std::env::temp_dir().join(format!("erf-project-config-maven-boundary-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src/main/java/example")).unwrap();
        fs::create_dir_all(root.join("app/src/main/java/example")).unwrap();
        fs::create_dir_all(root.join("unrelated/src/main/java/example")).unwrap();
        fs::create_dir_all(root.join("vendor/lib/src/main/java/example")).unwrap();
        fs::write(root.join("pom.xml"), "<project><modules><module>app</module></modules></project>").unwrap();
        fs::write(root.join("app/pom.xml"), "<project/>").unwrap();
        fs::write(root.join("unrelated/pom.xml"), "<project/>").unwrap();
        fs::write(root.join("vendor/lib/pom.xml"), "<project/>").unwrap();
        fs::write(root.join("src/main/java/example/Root.java"), "class Root {}\n").unwrap();
        fs::write(root.join("app/src/main/java/example/App.java"), "class App {}\n").unwrap();
        fs::write(root.join("unrelated/src/main/java/example/Other.java"), "class Other {}\n").unwrap();
        fs::write(root.join("vendor/lib/src/main/java/example/Vendor.java"), "class Vendor {}\n").unwrap();
        let detected = detect_workspace(&root).unwrap();
        assert_eq!(detected.modules, vec![".", "app"]);
        assert_eq!(detected.source_roots, vec!["app/src/main/java", "src/main/java"]);
        assert_eq!(detected.vendor_roots, vec!["vendor"]);
        assert!(!detected.source_roots.iter().any(|root| root.starts_with("unrelated/") || root.starts_with("vendor/")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detected_identity_is_stable_when_files_are_created_in_different_orders() {
        let first = std::env::temp_dir().join(format!("erf-project-config-order-a-{}", std::process::id()));
        let second = std::env::temp_dir().join(format!("erf-project-config-order-b-{}", std::process::id()));
        let _ = fs::remove_dir_all(&first);
        let _ = fs::remove_dir_all(&second);
        for root in [&first, &second] {
            fs::create_dir_all(root.join("app/src/main/java/example")).unwrap();
            fs::create_dir_all(root.join("lib/src/test/java/example")).unwrap();
            fs::write(root.join("settings.gradle"), "include ':lib', ':app'\n").unwrap();
            fs::write(root.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
        }
        fs::write(first.join("app/src/main/java/example/App.java"), "class App {}\n").unwrap();
        fs::write(first.join("lib/src/test/java/example/LibTest.java"), "class LibTest {}\n").unwrap();
        fs::write(second.join("lib/src/test/java/example/LibTest.java"), "class LibTest {}\n").unwrap();
        fs::write(second.join("app/src/main/java/example/App.java"), "class App {}\n").unwrap();
        let first_identity = detect_workspace(&first).unwrap().into_config().identity();
        let second_identity = detect_workspace(&second).unwrap().into_config().identity();
        assert_eq!(first_identity, second_identity);
        fs::remove_dir_all(first).unwrap();
        fs::remove_dir_all(second).unwrap();
    }

    #[test]
    fn maven_aggregator_keeps_root_module_only_when_root_has_sources() {
        let root = std::env::temp_dir().join(format!("erf-project-config-maven-root-source-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src/main/java/example")).unwrap();
        fs::create_dir_all(root.join("app")).unwrap();
        fs::write(root.join("pom.xml"), "<project><modules><module>app</module></modules></project>").unwrap();
        fs::write(root.join("app/pom.xml"), "<project/>").unwrap();
        fs::write(root.join("src/main/java/example/Root.java"), "class Root {}\n").unwrap();
        let detected = detect_workspace(&root).unwrap();
        assert_eq!(detected.modules, vec![".", "app"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dynamic_maven_modules_are_ambiguous() {
        let root = std::env::temp_dir().join(format!("erf-project-config-maven-dynamic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src/main/java")).unwrap();
        fs::write(root.join("pom.xml"), "<project><modules><module>${module.path}</module></modules></project>").unwrap();
        fs::write(root.join("src/main/java/App.java"), "class App {}\n").unwrap();
        let detected = detect_workspace(&root).unwrap();
        assert!(detected.notes.iter().any(|note| note.contains("Maven module detection is ambiguous")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detector_resolves_gradle_modules_and_module_scoped_source_sets() {
        let root = std::env::temp_dir().join(format!("erf-project-config-gradle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("app/src/main/java/example")).unwrap();
        fs::create_dir_all(root.join("app/src/test/java/example")).unwrap();
        fs::create_dir_all(root.join("lib/core/src/main/java/example")).unwrap();
        fs::write(root.join("settings.gradle"), "include ':app', ':lib:core'\n").unwrap();
        fs::write(root.join("build.gradle"), "plugins { id 'java' }\n").unwrap();
        fs::write(root.join("app/src/main/java/example/App.java"), "class App {}\n").unwrap();
        fs::write(root.join("app/src/test/java/example/AppTest.java"), "class AppTest {}\n").unwrap();
        fs::write(root.join("lib/core/src/main/java/example/Core.java"), "class Core {}\n").unwrap();
        let detected = detect_workspace(&root).unwrap();
        assert_eq!(detected.language.as_deref(), Some("java"));
        assert_eq!(detected.build_system.as_deref(), Some("gradle"));
        assert_eq!(detected.modules, vec!["app", "lib/core"]);
        assert_eq!(detected.source_roots, vec!["app/src/main/java", "lib/core/src/main/java"]);
        assert_eq!(detected.test_roots, vec!["app/src/test/java"]);
        assert!(detected.notes.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dynamic_gradle_module_expression_is_ambiguous() {
        let root = std::env::temp_dir().join(format!("erf-project-config-gradle-dynamic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src/main/java")).unwrap();
        fs::write(root.join("settings.gradle.kts"), "val projects = listOf(\":app\")\ninclude(*projects)\n").unwrap();
        fs::write(root.join("build.gradle.kts"), "plugins { java }\n").unwrap();
        fs::write(root.join("src/main/java/App.java"), "class App {}\n").unwrap();
        let detected = detect_workspace(&root).unwrap();
        assert!(detected.notes.iter().any(|note| note.contains("Gradle module detection is ambiguous")));
        fs::remove_dir_all(root).unwrap();
    }
}
