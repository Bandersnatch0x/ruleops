//! SAFE_STATIC Workspace Resolver adapters for Maven and Gradle.
//!
//! The adapters deliberately implement a small, closed static subset. They
//! read project metadata and conventional source roots, never evaluate build
//! scripts, and report facts that require a build as unresolved. The output
//! is sorted before its protobuf content digest is calculated.

use erf_contracts::workspace::v0 as wb;
use prost::Message as _;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

pub const RESOLVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildSystem {
    Maven,
    Gradle,
}

impl BuildSystem {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "maven" => Some(Self::Maven),
            "gradle" => Some(Self::Gradle),
            _ => None,
        }
    }

    pub const fn resolver_id(self) -> &'static str {
        match self {
            Self::Maven => "erf-maven-static-resolver",
            Self::Gradle => "erf-gradle-static-resolver",
        }
    }
}

pub fn facts() -> HashMap<String, bool> {
    HashMap::from([
        ("ecosystem.static-layout".into(), true),
        ("ecosystem.maven.safe-static".into(), true),
        ("ecosystem.gradle.safe-static".into(), true),
    ])
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("repository root is not a directory: {0}")]
    RepoRootNotDirectory(PathBuf),
    #[error("cannot inspect {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("symbolic link is not allowed in SAFE_STATIC input: {0}")]
    SymlinkRejected(PathBuf),
    #[error("path escapes repository root: {0}")]
    PathOutsideRepository(PathBuf),
    #[error("invalid {system} metadata at {path}: {detail}")]
    InvalidMetadata {
        system: &'static str,
        path: PathBuf,
        detail: String,
    },
    #[error("invalid module path '{value}': {detail}")]
    InvalidModulePath { value: String, detail: String },
    #[error("duplicate module identity: {0}")]
    DuplicateModule(String),
    #[error("duplicate file path in descriptor: {0}")]
    DuplicateFile(String),
}

#[derive(Debug, Clone)]
struct ModuleSpec {
    id: String,
    relative_path: Option<String>,
}

/// Resolve one Maven or Gradle repository snapshot without executing it.
pub fn resolve(repo_root: &Path, system: BuildSystem) -> Result<wb::WorkspaceDescriptor, ResolveError> {
    resolve_with_backend(repo_root, system, std::env::var("ERF_BACKEND").as_deref() == Ok("spotbugs"))
}

fn resolve_with_backend(
    repo_root: &Path,
    system: BuildSystem,
    bytecode_mode: bool,
) -> Result<wb::WorkspaceDescriptor, ResolveError> {
    let root = checked_root(repo_root)?;
    let mut specs = BTreeMap::<String, ModuleSpec>::new();
    let mut unresolved = vec![
        "build-output:not-available-in-safe-static".to_string(),
        "dependency-classpath:not-determined-statically".to_string(),
    ];

    match system {
        BuildSystem::Maven => {
            let mut visited = BTreeSet::new();
            collect_maven(&root, "", &mut specs, &mut unresolved, &mut visited)?;
        }
        BuildSystem::Gradle => collect_gradle(&root, &mut specs, &mut unresolved)?,
    }

    // A root project with source is a real module. An aggregator-only Maven
    // root is intentionally omitted; its declared children remain visible.
    add_root_if_present(&root, &mut specs, bytecode_mode);
    let mut modules = Vec::with_capacity(specs.len());
    for spec in specs.values() {
        modules.push(build_module(&root, spec, &mut unresolved, bytecode_mode)?);
    }
    if bytecode_mode
        && !modules.is_empty()
        && modules
            .iter()
            .all(|module| !module.source_roots.is_empty() && !module.files.is_empty())
        && !unresolved
            .iter()
            .any(|fact| fact.starts_with("bytecode-roots:none:"))
    {
        // SAFE_STATIC may close these facts from an already materialized,
        // repository-local class output. No build is executed and no output
        // is guessed: every discovered class file is content-addressed above.
        unresolved.retain(|fact| !fact.starts_with("build-output:"));
        if !has_dependency_declarations(&root, system, &specs)? {
            // In the closed v0 adapter subset, an explicit absence of Maven
            // dependency elements (or Gradle dependency declarations) means
            // the classpath closure is the empty repository-local set. A
            // declared/inherited-looking dependency remains unresolved.
            unresolved.retain(|fact| !fact.starts_with("dependency-classpath:"));
        }
    }
    unresolved.sort();
    unresolved.dedup();

    let mut descriptor = wb::WorkspaceDescriptor {
        mode: wb::WorkspaceResolutionMode::SafeStatic as i32,
        resolver_id: system.resolver_id().into(),
        resolver_version: RESOLVER_VERSION.into(),
        modules,
        unresolved_facts: unresolved,
        descriptor_sha256: vec![],
    };
    descriptor.descriptor_sha256 = descriptor_digest(&descriptor);
    Ok(descriptor)
}

pub fn descriptor_digest(descriptor: &wb::WorkspaceDescriptor) -> Vec<u8> {
    let mut blank = descriptor.clone();
    blank.descriptor_sha256.clear();
    Sha256::digest(blank.encode_to_vec()).to_vec()
}

fn checked_root(repo_root: &Path) -> Result<PathBuf, ResolveError> {
    reject_symlink_components(repo_root)?;
    let metadata = fs::symlink_metadata(repo_root).map_err(|source| ResolveError::Io {
        path: repo_root.to_path_buf(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(ResolveError::SymlinkRejected(repo_root.to_path_buf()));
    }
    if !metadata.is_dir() {
        return Err(ResolveError::RepoRootNotDirectory(repo_root.to_path_buf()));
    }
    fs::canonicalize(repo_root).map_err(|source| ResolveError::Io {
        path: repo_root.to_path_buf(),
        source,
    })
}

fn collect_maven(
    root: &Path,
    relative: &str,
    specs: &mut BTreeMap<String, ModuleSpec>,
    unresolved: &mut Vec<String>,
    visited: &mut BTreeSet<String>,
) -> Result<(), ResolveError> {
    let id = if relative.is_empty() { "root" } else { relative };
    if !relative.is_empty() && !visited.insert(id.to_string()) {
        return Err(ResolveError::DuplicateModule(id.to_string()));
    }
    let module_dir = root.join(relative);
    if !safe_is_dir(&module_dir)? {
        unresolved.push(format!("module-directory-missing:{id}"));
        return Ok(());
    }
    let pom = module_dir.join("pom.xml");
    if !pom.exists() {
        if relative.is_empty() {
            return Err(ResolveError::InvalidMetadata {
                system: "maven",
                path: pom,
                detail: "pom.xml is required for the Maven adapter".into(),
            });
        }
        if !relative.is_empty() {
            unresolved.push(format!("module-metadata-missing:{id}:pom.xml"));
        }
        return Ok(());
    }
    let text = read_regular_text(&pom)?;
    let packaging = extract_xml_values(&text, "packaging", &pom)?
        .into_iter()
        .next()
        .unwrap_or_else(|| "jar".into());
    // Maven aggregator POMs describe child modules but are not themselves
    // Java analysis units. Keep their children in the descriptor while
    // avoiding a false java-source-roots:none fact for the aggregator.
    if packaging != "pom" && !relative.is_empty() && specs.insert(
        id.to_string(),
        ModuleSpec {
            id: id.to_string(),
            relative_path: Some(relative.to_string()),
        },
    ).is_some() {
        return Err(ResolveError::DuplicateModule(id.to_string()));
    }
    for child in extract_xml_values(&text, "module", &pom)? {
        let child_rel = join_relative(relative, &child)?;
        collect_maven(root, &child_rel, specs, unresolved, visited)?;
    }
    Ok(())
}

fn collect_gradle(
    root: &Path,
    specs: &mut BTreeMap<String, ModuleSpec>,
    unresolved: &mut Vec<String>,
) -> Result<(), ResolveError> {
    let settings = ["settings.gradle.kts", "settings.gradle"]
        .iter()
        .map(|name| root.join(name))
        .find(|path| path.is_file());
    if let Some(settings) = settings {
        let text = read_regular_text(&settings)?;
        let (includes, dynamic) = parse_gradle_includes(&text, &settings)?;
        if dynamic {
            unresolved.push("gradle:dynamic-include-unresolved".into());
        }
        for (id, relative) in includes {
            if specs.insert(
                id.clone(),
                ModuleSpec {
                    id: id.clone(),
                    relative_path: Some(relative),
                },
            ).is_some() {
                return Err(ResolveError::DuplicateModule(id));
            }
        }
        if text.contains("projectDir") {
            // We do not infer custom project directories from arbitrary
            // Gradle expressions. Conventional paths are no longer safe to
            // assume, so mark the module locations unresolved instead.
            unresolved.push("gradle:project-dir-override-unresolved".into());
            for spec in specs.values_mut() {
                spec.relative_path = None;
                unresolved.push(format!("module-path-unresolved:{}", spec.id));
            }
        }
    } else if !["build.gradle.kts", "build.gradle"]
        .iter()
        .any(|name| root.join(name).is_file())
    {
        return Err(ResolveError::InvalidMetadata {
            system: "gradle",
            path: root.to_path_buf(),
            detail: "no settings.gradle(.kts) or build.gradle(.kts) found".into(),
        });
    }
    Ok(())
}

fn add_root_if_present(root: &Path, specs: &mut BTreeMap<String, ModuleSpec>, bytecode_mode: bool) {
    if root.join("src/main/java").is_dir()
        || root.join("src/test/java").is_dir()
        || (bytecode_mode && (root.join("target/classes").is_dir() || root.join("build/classes/java/main").is_dir()))
    {
        specs.entry("root".into()).or_insert_with(|| ModuleSpec {
            id: "root".into(),
            relative_path: Some(String::new()),
        });
    }
}

fn build_module(
    root: &Path,
    spec: &ModuleSpec,
    unresolved: &mut Vec<String>,
    bytecode_mode: bool,
) -> Result<wb::ModuleDescriptor, ResolveError> {
    let mut source_roots = Vec::new();
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let Some(relative) = &spec.relative_path else {
        unresolved.push(format!("java-source-roots:not-determined:{}", spec.id));
        return Ok(wb::ModuleDescriptor {
            module_id: spec.id.clone(),
            language: "java".into(),
            source_roots,
            files,
            ..Default::default()
        });
    };
    let module_dir = root.join(relative);
    if !safe_is_dir(&module_dir)? {
        unresolved.push(format!("module-directory-missing:{}", spec.id));
        return Ok(wb::ModuleDescriptor {
            module_id: spec.id.clone(),
            language: "java".into(),
            source_roots,
            files,
            ..Default::default()
        });
    }
    if bytecode_mode {
        for suffix in ["target/classes", "build/classes/java/main"] {
            let classes = module_dir.join(suffix);
            if safe_is_dir(&classes)? {
                ensure_inside(root, &classes)?;
                source_roots.push(relative_path(root, &classes));
                collect_class_files(root, &classes, &mut files, &mut seen)?;
            }
        }
        if source_roots.is_empty() {
            unresolved.push(format!("bytecode-roots:none:{}", spec.id));
        }
    } else {
        for suffix in ["src/main/java", "src/test/java"] {
            let source = module_dir.join(suffix);
            if safe_is_dir(&source)? {
                ensure_inside(root, &source)?;
                source_roots.push(relative_path(root, &source));
                collect_java_files(root, &source, &mut files, &mut seen)?;
            } else if safe_is_dir(&module_dir.join("src/main"))? && suffix == "src/main/java" {
                unresolved.push(format!("source-root-missing:{}:{suffix}", spec.id));
            }
        }
        if source_roots.is_empty() {
            unresolved.push(format!("java-source-roots:none:{}", spec.id));
        }
    }
    source_roots.sort();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(wb::ModuleDescriptor {
        module_id: spec.id.clone(),
        language: "java".into(),
        source_roots,
        files,
        ..Default::default()
    })
}

fn has_dependency_declarations(
    root: &Path,
    system: BuildSystem,
    specs: &BTreeMap<String, ModuleSpec>,
) -> Result<bool, ResolveError> {
    let mut metadata_paths = vec![match system {
        BuildSystem::Maven => root.join("pom.xml"),
        BuildSystem::Gradle => root.join("build.gradle"),
    }];
    if system == BuildSystem::Gradle {
        metadata_paths.push(root.join("build.gradle.kts"));
    }
    for spec in specs.values() {
        let Some(relative) = &spec.relative_path else { continue };
        let module_dir = root.join(relative);
        match system {
            BuildSystem::Maven => metadata_paths.push(module_dir.join("pom.xml")),
            BuildSystem::Gradle => {
                metadata_paths.push(module_dir.join("build.gradle"));
                metadata_paths.push(module_dir.join("build.gradle.kts"));
            }
        }
    }
    for path in metadata_paths {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(ResolveError::Io { path, source }),
        };
        if metadata.file_type().is_symlink() {
            return Err(ResolveError::SymlinkRejected(path));
        }
        if !metadata.is_file() {
            continue;
        }
        let text = read_regular_text(&path)?;
        let has_dependency = match system {
            BuildSystem::Maven => contains_xml_element(&strip_xml_comments(&text, &path)?, "dependency"),
            BuildSystem::Gradle => text.contains("dependencies") || text.contains("implementation(") || text.contains("api("),
        };
        if has_dependency {
            return Ok(true);
        }
    }
    Ok(false)
}

fn contains_xml_element(text: &str, tag: &str) -> bool {
    let open = format!("<{tag}");
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find(&open) {
        let start = cursor + offset + open.len();
        if !text
            .as_bytes()
            .get(start)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        {
            return true;
        }
        cursor = start;
    }
    false
}

fn collect_java_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<wb::FileEntry>,
    seen: &mut BTreeSet<String>,
) -> Result<(), ResolveError> {
    let mut entries = fs::read_dir(directory)
        .map_err(|source| ResolveError::Io { path: directory.into(), source })?
        .map(|entry| entry.map_err(|source| ResolveError::Io { path: directory.into(), source }))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|source| ResolveError::Io { path: path.clone(), source })?;
        if metadata.file_type().is_symlink() {
            return Err(ResolveError::SymlinkRejected(path));
        }
        if metadata.is_dir() {
            collect_java_files(root, &path, files, seen)?;
        } else if metadata.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("java") {
            ensure_inside(root, &path)?;
            let bytes = fs::read(&path)
                .map_err(|source| ResolveError::Io { path: path.clone(), source })?;
            let relative = relative_path(root, &path);
            if !seen.insert(relative.clone()) {
                return Err(ResolveError::DuplicateFile(relative));
            }
            files.push(wb::FileEntry {
                path: relative,
                blob_sha256: Sha256::digest(bytes).to_vec(),
            });
        }
    }
    Ok(())
}

fn collect_class_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<wb::FileEntry>,
    seen: &mut BTreeSet<String>,
) -> Result<(), ResolveError> {
    let mut entries = fs::read_dir(directory)
        .map_err(|source| ResolveError::Io { path: directory.into(), source })?
        .map(|entry| entry.map_err(|source| ResolveError::Io { path: directory.into(), source }))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|source| ResolveError::Io { path: path.clone(), source })?;
        if metadata.file_type().is_symlink() {
            return Err(ResolveError::SymlinkRejected(path));
        }
        if metadata.is_dir() {
            collect_class_files(root, &path, files, seen)?;
        } else if metadata.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("class") {
            ensure_inside(root, &path)?;
            let bytes = fs::read(&path)
                .map_err(|source| ResolveError::Io { path: path.clone(), source })?;
            let relative = relative_path(root, &path);
            if !seen.insert(relative.clone()) {
                return Err(ResolveError::DuplicateFile(relative));
            }
            files.push(wb::FileEntry {
                path: relative,
                blob_sha256: Sha256::digest(bytes).to_vec(),
            });
        }
    }
    Ok(())
}

fn read_regular_text(path: &Path) -> Result<String, ResolveError> {
    reject_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| ResolveError::Io { path: path.into(), source })?;
    if metadata.file_type().is_symlink() {
        return Err(ResolveError::SymlinkRejected(path.into()));
    }
    if !metadata.is_file() {
        return Err(ResolveError::Io {
            path: path.into(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "metadata is not a file"),
        });
    }
    let bytes = fs::read(path).map_err(|source| ResolveError::Io { path: path.into(), source })?;
    String::from_utf8(bytes).map_err(|_| ResolveError::InvalidMetadata {
        system: "build",
        path: path.into(),
        detail: "metadata is not UTF-8".into(),
    })
}

fn extract_xml_values(text: &str, tag: &str, path: &Path) -> Result<Vec<String>, ResolveError> {
    let text = strip_xml_comments(text, path)?;
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut values = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find(&open) {
        let start = cursor + offset;
        let after_name = start + open.len();
        if text.as_bytes().get(after_name).is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'-') {
            cursor = after_name;
            continue;
        }
        let end = text[after_name..].find('>').map(|i| after_name + i).ok_or_else(|| ResolveError::InvalidMetadata {
            system: "maven", path: path.into(), detail: format!("unterminated <{tag}> tag"),
        })?;
        if !text[after_name..end].trim().is_empty() {
            return Err(ResolveError::InvalidMetadata {
                system: "maven", path: path.into(), detail: format!("<{tag}> attributes are not supported"),
            });
        }
        let value_start = end + 1;
        let value_end = text[value_start..].find(&close).map(|i| value_start + i).ok_or_else(|| ResolveError::InvalidMetadata {
            system: "maven", path: path.into(), detail: format!("unterminated <{tag}> value"),
        })?;
        let value = text[value_start..value_end].trim();
        if value.is_empty() || value.contains('<') || value.contains('>') {
            return Err(ResolveError::InvalidMetadata {
                system: "maven", path: path.into(), detail: format!("invalid <{tag}> value"),
            });
        }
        values.push(value.to_string());
        cursor = value_end + close.len();
    }
    Ok(values)
}

fn strip_xml_comments(text: &str, path: &Path) -> Result<String, ResolveError> {
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find("<!--") {
        let start = cursor + offset;
        output.push_str(&text[cursor..start]);
        let end = text[start + 4..].find("-->").map(|i| start + 4 + i).ok_or_else(|| ResolveError::InvalidMetadata {
            system: "maven", path: path.into(), detail: "unterminated XML comment".into(),
        })?;
        cursor = end + 3;
    }
    output.push_str(&text[cursor..]);
    Ok(output)
}

fn parse_gradle_includes(text: &str, path: &Path) -> Result<(Vec<(String, String)>, bool), ResolveError> {
    let mut parser = GradleScanner::new(text, path);
    let mut includes = Vec::new();
    let mut dynamic = false;
    while let Some(position) = parser.next_include()? {
        match parser.arguments(position)? {
            Some(values) => {
                for value in values {
                    includes.push(normalize_gradle_project(&value)?);
                }
            }
            None => dynamic = true,
        }
    }
    includes.sort();
    Ok((includes, dynamic))
}

struct GradleScanner<'a> {
    text: &'a [u8],
    cursor: usize,
    path: PathBuf,
}

impl<'a> GradleScanner<'a> {
    fn new(text: &'a str, path: &Path) -> Self { Self { text: text.as_bytes(), cursor: 0, path: path.into() } }

    fn next_include(&mut self) -> Result<Option<usize>, ResolveError> {
        while self.cursor < self.text.len() {
            self.skip_space_and_comments()?;
            if self.cursor >= self.text.len() { return Ok(None); }
            if self.text[self.cursor] == b'\'' || self.text[self.cursor] == b'"' {
                self.skip_string()?;
                continue;
            }
            if is_identifier_start(self.text[self.cursor]) {
                let start = self.cursor;
                self.cursor += 1;
                while self.cursor < self.text.len() && is_identifier_part(self.text[self.cursor]) { self.cursor += 1; }
                if &self.text[start..self.cursor] == b"include" { return Ok(Some(self.cursor)); }
                continue;
            }
            self.cursor += 1;
        }
        Ok(None)
    }

    fn arguments(&mut self, after_name: usize) -> Result<Option<Vec<String>>, ResolveError> {
        self.cursor = after_name;
        self.skip_space_and_comments()?;
        let parenthesized = self.text.get(self.cursor) == Some(&b'(');
        if parenthesized { self.cursor += 1; self.skip_space_and_comments()?; }
        if self.text.get(self.cursor) != Some(&b'\'') && self.text.get(self.cursor) != Some(&b'"') {
            self.skip_to_expression_end(parenthesized);
            return Ok(None);
        }
        let mut values = Vec::new();
        loop {
            self.skip_space_and_comments()?;
            if self.text.get(self.cursor) != Some(&b'\'') && self.text.get(self.cursor) != Some(&b'"') { return Ok(None); }
            let (value, dynamic) = self.read_string()?;
            if dynamic { self.skip_to_expression_end(parenthesized); return Ok(None); }
            values.push(value);
            self.skip_space_and_comments()?;
            match self.text.get(self.cursor) {
                Some(b',') => { self.cursor += 1; }
                Some(b')') if parenthesized => { self.cursor += 1; break; }
                Some(b'\n') | Some(b';') | None if !parenthesized => { if self.cursor < self.text.len() { self.cursor += 1; } break; }
                _ => { self.skip_to_expression_end(parenthesized); return Ok(None); }
            }
        }
        Ok(Some(values))
    }

    fn skip_to_expression_end(&mut self, parenthesized: bool) {
        while self.cursor < self.text.len() {
            if parenthesized && self.text[self.cursor] == b')' { self.cursor += 1; break; }
            if !parenthesized && (self.text[self.cursor] == b'\n' || self.text[self.cursor] == b';') { self.cursor += 1; break; }
            self.cursor += 1;
        }
    }

    fn skip_space_and_comments(&mut self) -> Result<(), ResolveError> {
        loop {
            while self.cursor < self.text.len() && self.text[self.cursor].is_ascii_whitespace() { self.cursor += 1; }
            if self.text.get(self.cursor..self.cursor + 2) == Some(b"//") {
                self.cursor += 2;
                while self.cursor < self.text.len() && self.text[self.cursor] != b'\n' { self.cursor += 1; }
                continue;
            }
            if self.text.get(self.cursor..self.cursor + 2) == Some(b"/*") {
                self.cursor += 2;
                let Some(offset) = self.text[self.cursor..].windows(2).position(|w| w == b"*/") else {
                    return Err(ResolveError::InvalidMetadata { system: "gradle", path: self.path.clone(), detail: "unterminated block comment".into() });
                };
                self.cursor += offset + 2;
                continue;
            }
            break;
        }
        Ok(())
    }

    fn skip_string(&mut self) -> Result<(), ResolveError> { let _ = self.read_string()?; Ok(()) }

    fn read_string(&mut self) -> Result<(String, bool), ResolveError> {
        let quote = *self.text.get(self.cursor).ok_or_else(|| ResolveError::InvalidMetadata { system: "gradle", path: self.path.clone(), detail: "missing string".into() })?;
        self.cursor += 1;
        let start = self.cursor;
        let mut value = String::new();
        while self.cursor < self.text.len() {
            let byte = self.text[self.cursor];
            if byte == quote {
                self.cursor += 1;
                let dynamic = value.contains("${");
                return Ok((value, dynamic));
            }
            if byte == b'\\' {
                return Err(ResolveError::InvalidMetadata { system: "gradle", path: self.path.clone(), detail: "escaped strings are not supported".into() });
            }
            value.push(byte as char);
            self.cursor += 1;
        }
        let _ = start;
        Err(ResolveError::InvalidMetadata { system: "gradle", path: self.path.clone(), detail: "unterminated string".into() })
    }
}

fn normalize_gradle_project(value: &str) -> Result<(String, String), ResolveError> {
    if value.is_empty() { return Err(ResolveError::InvalidModulePath { value: value.into(), detail: "empty project path".into() }); }
    let id = value.strip_prefix(':').unwrap_or(value);
    let parts = id.split(':').collect::<Vec<_>>();
    if parts.iter().any(|part| part.is_empty() || *part == "." || *part == ".." || part.contains('/') || part.contains('\\')) {
        return Err(ResolveError::InvalidModulePath { value: value.into(), detail: "must contain non-empty project segments".into() });
    }
    Ok((parts.join(":"), parts.join("/")))
}

fn join_relative(parent: &str, child: &str) -> Result<String, ResolveError> {
    let clean = normalize_relative(child)?;
    if parent.is_empty() { Ok(clean) } else { Ok(format!("{parent}/{clean}")) }
}

fn normalize_relative(value: &str) -> Result<String, ResolveError> {
    if value.is_empty() || value.starts_with('/') || value.contains('\0') || value.contains("${") || (value.as_bytes().get(1) == Some(&b':')) {
        return Err(ResolveError::InvalidModulePath { value: value.into(), detail: "must be repository-relative".into() });
    }
    let normalized = value.replace('\\', "/");
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty() || part == "." { continue; }
        if part == ".." { return Err(ResolveError::InvalidModulePath { value: value.into(), detail: "parent traversal is forbidden".into() }); }
        if part.contains('\0') { return Err(ResolveError::InvalidModulePath { value: value.into(), detail: "NUL is forbidden".into() }); }
        parts.push(part);
    }
    if parts.is_empty() { return Err(ResolveError::InvalidModulePath { value: value.into(), detail: "empty path".into() }); }
    Ok(parts.join("/"))
}

fn ensure_inside(root: &Path, path: &Path) -> Result<(), ResolveError> {
    reject_symlink_components(path)?;
    let canonical = fs::canonicalize(path).map_err(|source| ResolveError::Io { path: path.into(), source })?;
    if !canonical.starts_with(root) { return Err(ResolveError::PathOutsideRepository(path.into())); }
    Ok(())
}

fn safe_is_dir(path: &Path) -> Result<bool, ResolveError> {
    reject_symlink_components(path)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(source) => return Err(ResolveError::Io { path: path.into(), source }),
    };
    if metadata.file_type().is_symlink() {
        return Err(ResolveError::SymlinkRejected(path.into()));
    }
    Ok(metadata.is_dir())
}

fn reject_symlink_components(path: &Path) -> Result<(), ResolveError> {
    for component in path.ancestors() {
        let metadata = match fs::symlink_metadata(component) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(ResolveError::Io { path: component.into(), source }),
        };
        if metadata.file_type().is_symlink() {
            return Err(ResolveError::SymlinkRejected(component.into()));
        }
    }
    Ok(())
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

fn is_identifier_start(byte: u8) -> bool { byte.is_ascii_alphabetic() || byte == b'_' }
fn is_identifier_part(byte: u8) -> bool { byte.is_ascii_alphanumeric() || byte == b'_' }

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn repo(name: &str) -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("erf-static-{name}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write(path: &Path, text: &str) { fs::create_dir_all(path.parent().unwrap()).unwrap(); fs::write(path, text).unwrap(); }

    #[test]
    fn maven_multi_module_is_static_and_repeatable() {
        let root = repo("maven");
        write(&root.join("pom.xml"), "<project><modules><module>app</module><module>lib</module></modules></project>");
        write(&root.join("app/src/main/java/a/App.java"), "class App {}\n");
        write(&root.join("lib/src/test/java/l/LibTest.java"), "class LibTest {}\n");
        write(
            &root.join("app/pom.xml"),
            "<project><dependencies><dependency><groupId>example</groupId><artifactId>api</artifactId></dependency></dependencies></project>",
        );
        write(&root.join("lib/pom.xml"), "<project/>");
        let first = resolve(&root, BuildSystem::Maven).unwrap();
        let second = resolve(&root, BuildSystem::Maven).unwrap();
        assert_eq!(first.encode_to_vec(), second.encode_to_vec());
        assert_eq!(first.modules.iter().map(|m| m.module_id.as_str()).collect::<Vec<_>>(), vec!["app", "lib"]);
        assert!(first.modules[0].source_roots.iter().any(|r| r == "app/src/main/java"));
        assert!(first.unresolved_facts.iter().any(|f| f == "dependency-classpath:not-determined-statically"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn maven_aggregator_pom_is_not_an_analysis_unit() {
        let root = repo("maven-aggregator");
        write(
            &root.join("pom.xml"),
            "<project><packaging>pom</packaging><modules><module>services</module></modules></project>",
        );
        write(
            &root.join("services/pom.xml"),
            "<project><packaging>pom</packaging><modules><module>api</module></modules></project>",
        );
        write(&root.join("services/api/pom.xml"), "<project/>");
        write(&root.join("services/api/src/main/java/example/App.java"), "class App {}\n");

        let descriptor = resolve(&root, BuildSystem::Maven).unwrap();
        assert_eq!(descriptor.modules.iter().map(|m| m.module_id.as_str()).collect::<Vec<_>>(), vec!["services/api"]);
        assert!(descriptor
            .unresolved_facts
            .iter()
            .all(|fact| !fact.contains("java-source-roots:none:services")));
        assert!(descriptor.modules[0].source_roots.iter().any(|root| root == "services/api/src/main/java"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spotbugs_class_output_and_empty_dependency_set_close_required_facts() {
        let root = repo("spotbugs-closed");
        write(&root.join("pom.xml"), "<project/>");
        write(&root.join("target/classes/example/Bad.class"), "class-bytes");
        let descriptor = resolve_with_backend(&root, BuildSystem::Maven, true).unwrap();
        assert!(descriptor
            .unresolved_facts
            .iter()
            .all(|fact| !fact.starts_with("build-output:") && !fact.starts_with("dependency-classpath:")));
        assert_eq!(descriptor.modules[0].files.len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn gradle_literal_include_does_not_execute_build_script() {
        let root = repo("gradle");
        write(&root.join("settings.gradle.kts"), "include(\":app\", \":lib\")\ninclude(*dynamicProjects)\n");
        write(&root.join("build.gradle.kts"), "throw RuntimeException(\"must never execute\")");
        write(&root.join("app/src/main/java/a/App.java"), "class App {}\n");
        let descriptor = resolve(&root, BuildSystem::Gradle).unwrap();
        assert_eq!(descriptor.resolver_id, "erf-gradle-static-resolver");
        assert_eq!(descriptor.modules[0].module_id, "app");
        assert!(descriptor.unresolved_facts.iter().any(|f| f == "gradle:dynamic-include-unresolved"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_maven_traversal_and_duplicate_are_rejected() {
        let root = repo("invalid");
        assert!(matches!(resolve(&root, BuildSystem::Maven), Err(ResolveError::InvalidMetadata { .. })));
        write(&root.join("pom.xml"), "<project><modules><module>../outside</module></modules></project>");
        assert!(matches!(resolve(&root, BuildSystem::Maven), Err(ResolveError::InvalidModulePath { .. })));
        write(&root.join("pom.xml"), "<project><modules><module>app</module><module>app</module></modules></project>");
        assert!(matches!(resolve(&root, BuildSystem::Maven), Err(ResolveError::DuplicateModule(_))));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_gradle_include_is_rejected() {
        let root = repo("gradle-duplicate");
        write(&root.join("settings.gradle"), "include ':app', ':app'\n");
        assert!(matches!(resolve(&root, BuildSystem::Gradle), Err(ResolveError::DuplicateModule(_))));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_source_is_rejected() {
        use std::os::unix::fs::symlink;
        let root = repo("symlink");
        write(&root.join("pom.xml"), "<project/>");
        fs::create_dir_all(root.join("src/main")).unwrap();
        symlink("/tmp", root.join("src/main/java")).unwrap();
        assert!(matches!(resolve(&root, BuildSystem::Maven), Err(ResolveError::SymlinkRejected(_))));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn spotbugs_mode_discovers_compiled_class_outputs() {
        let root = repo("spotbugs-output");
        write(&root.join("pom.xml"), "<project/>");
        fs::create_dir_all(root.join("target/classes/example")).unwrap();
        fs::write(root.join("target/classes/example/Bad.class"), b"cafebabe").unwrap();
        let descriptor = resolve_with_backend(&root, BuildSystem::Maven, true).unwrap();
        assert_eq!(descriptor.modules.len(), 1);
        assert_eq!(descriptor.modules[0].files[0].path, "target/classes/example/Bad.class");
        let _ = fs::remove_dir_all(root);
    }
}
