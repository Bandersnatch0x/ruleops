//! Deterministic Java affected-scope closure over a frozen Git `ChangedSet`.
//!
//! This module owns the narrow seam between repository change facts and a
//! future incremental `ScanPlan`. It parses only a bounded Maven/Gradle
//! metadata subset, never executes a project build, and widens the scope when
//! the module graph or generated-source boundary cannot be proven safe.

use erf_git_scope::ChangedSet;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const AFFECTED_SCOPE_VERSION: &str = "affected-scope/v1";
pub const MAX_METADATA_BYTES: usize = 4 * 1024 * 1024;
const MAX_METADATA_VALUES: usize = 4096;
const MAX_METADATA_VALUE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum BuildSystem {
    Maven,
    Gradle,
}

impl BuildSystem {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Maven => "maven",
            Self::Gradle => "gradle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum AnalysisKind {
    PmdAst,
    PmdTypeResolution,
    SpotBugsBytecode,
}

impl AnalysisKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::PmdAst => "pmd-ast",
            Self::PmdTypeResolution => "pmd-type-resolution",
            Self::SpotBugsBytecode => "spotbugs-bytecode",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ScopeLevel {
    FileCandidate,
    Module,
    Full,
}

impl ScopeLevel {
    pub const fn code(self) -> &'static str {
        match self {
            Self::FileCandidate => "FILE_CANDIDATE",
            Self::Module => "MODULE",
            Self::Full => "FULL",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::FileCandidate => 1,
            Self::Module => 2,
            Self::Full => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ReasonCode {
    BuildSystemNotDetected,
    MultipleBuildSystems,
    BuildGraphIncomplete,
    DynamicBuildGraph,
    ProjectDirectoryOverride,
    GeneratedSourcesUncertain,
    CustomSourceSetUnresolved,
    SharedBuildConfiguration,
    LockfileChanged,
    PolicyConfigurationChanged,
    RuleConfigurationChanged,
    ResolverConfigurationChanged,
    JavaSourceChanged,
    ModuleDescriptorChanged,
    ModuleResourceChanged,
    CrossModuleChange,
    ReverseDependencyClosure,
    DeletedModuleUnresolved,
    UnmappedChange,
    SpotBugsBytecodeClosure,
}

impl ReasonCode {
    pub const fn code(self) -> &'static str {
        match self {
            Self::BuildSystemNotDetected => "build-system-not-detected",
            Self::MultipleBuildSystems => "multiple-build-systems",
            Self::BuildGraphIncomplete => "build-graph-incomplete",
            Self::DynamicBuildGraph => "dynamic-build-graph",
            Self::ProjectDirectoryOverride => "project-directory-override",
            Self::GeneratedSourcesUncertain => "generated-sources-uncertain",
            Self::CustomSourceSetUnresolved => "custom-source-set-unresolved",
            Self::SharedBuildConfiguration => "shared-build-configuration",
            Self::LockfileChanged => "lockfile-changed",
            Self::PolicyConfigurationChanged => "policy-configuration-changed",
            Self::RuleConfigurationChanged => "rule-configuration-changed",
            Self::ResolverConfigurationChanged => "resolver-configuration-changed",
            Self::JavaSourceChanged => "java-source-changed",
            Self::ModuleDescriptorChanged => "module-descriptor-changed",
            Self::ModuleResourceChanged => "module-resource-changed",
            Self::CrossModuleChange => "cross-module-change",
            Self::ReverseDependencyClosure => "reverse-dependency-closure",
            Self::DeletedModuleUnresolved => "deleted-module-unresolved",
            Self::UnmappedChange => "unmapped-change",
            Self::SpotBugsBytecodeClosure => "spotbugs-bytecode-closure",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ScopeReason {
    code: ReasonCode,
    detail: String,
}

impl ScopeReason {
    fn new(code: ReasonCode, detail: impl Into<String>) -> Self {
        Self { code, detail: detail.into() }
    }

    pub fn code(&self) -> ReasonCode {
        self.code
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

#[derive(Debug, Error)]
pub enum AffectedScopeError {
    #[error("repository root is not a directory: {0}")]
    RepositoryRootNotDirectory(PathBuf),
    #[error("cannot inspect {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("metadata parser budget exceeded for {path}: {detail}")]
    MetadataBudgetExceeded { path: PathBuf, detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BackendScope {
    kind: AnalysisKind,
    level: ScopeLevel,
    module_ids: Vec<String>,
    paths: Vec<String>,
    reasons: Vec<ScopeReason>,
}

impl BackendScope {
    pub fn kind(&self) -> AnalysisKind {
        self.kind
    }

    pub fn level(&self) -> ScopeLevel {
        self.level
    }

    pub fn module_ids(&self) -> &[String] {
        &self.module_ids
    }

    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    pub fn reasons(&self) -> &[ScopeReason] {
        &self.reasons
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AffectedScope {
    build_system: Option<BuildSystem>,
    changed_set_digest: String,
    workspace_graph_digest: String,
    changed_paths: Vec<String>,
    affected_module_ids: Vec<String>,
    classification: ScopeLevel,
    reasons: Vec<ScopeReason>,
    pmd_ast: BackendScope,
    pmd_type_resolution: BackendScope,
    spotbugs_bytecode: BackendScope,
    digest: String,
}

impl AffectedScope {
    pub fn build_system(&self) -> Option<BuildSystem> {
        self.build_system
    }

    pub fn changed_set_digest(&self) -> &str {
        &self.changed_set_digest
    }

    pub fn workspace_graph_digest(&self) -> &str {
        &self.workspace_graph_digest
    }

    pub fn changed_paths(&self) -> &[String] {
        &self.changed_paths
    }

    pub fn affected_module_ids(&self) -> &[String] {
        &self.affected_module_ids
    }

    pub fn classification(&self) -> ScopeLevel {
        self.classification
    }

    pub fn reasons(&self) -> &[ScopeReason] {
        &self.reasons
    }

    pub fn backend(&self, kind: AnalysisKind) -> &BackendScope {
        match kind {
            AnalysisKind::PmdAst => &self.pmd_ast,
            AnalysisKind::PmdTypeResolution => &self.pmd_type_resolution,
            AnalysisKind::SpotBugsBytecode => &self.spotbugs_bytecode,
        }
    }

    pub fn pmd_ast(&self) -> &BackendScope {
        &self.pmd_ast
    }

    pub fn pmd_type_resolution(&self) -> &BackendScope {
        &self.pmd_type_resolution
    }

    pub fn spotbugs_bytecode(&self) -> &BackendScope {
        &self.spotbugs_bytecode
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[derive(Debug, Clone)]
pub struct AffectedScopeResolver {
    repository_root: PathBuf,
}

impl AffectedScopeResolver {
    pub fn new(repository_root: impl Into<PathBuf>) -> Self {
        Self { repository_root: repository_root.into() }
    }

    /// Resolve changed paths into backend-specific minimum safe closures.
    ///
    /// The resolver reads only repository metadata and source-root layout. It
    /// does not execute Maven, Gradle, generated-source tools, SCM operations,
    /// or analysis backends. A graph fact that cannot be proven is represented
    /// as a `FULL` widening reason.
    pub fn resolve(&self, changed_set: &ChangedSet) -> Result<AffectedScope, AffectedScopeError> {
        let metadata = fs::metadata(&self.repository_root).map_err(|source| AffectedScopeError::Io {
            path: self.repository_root.clone(),
            source,
        })?;
        if !metadata.is_dir() {
            return Err(AffectedScopeError::RepositoryRootNotDirectory(self.repository_root.clone()));
        }

        let graph = ProjectGraph::discover(&self.repository_root)?;
        let workspace_graph_digest = graph.digest();
        let changed_path_set = collect_changed_paths(changed_set);
        let changed_paths: Vec<String> = changed_path_set.iter().cloned().collect();
        let mut reasons = graph.reasons.clone();
        let mut seed_modules = BTreeSet::new();
        let mut source_paths = BTreeSet::new();
        let mut full = !graph.reasons.is_empty();
        let mut file_candidate_only = true;

        for entry in changed_set.entries() {
            let mut entry_modules = BTreeSet::new();
            let mut entry_paths = vec![entry.path()];
            if let Some(old_path) = entry.old_path() {
                entry_paths.push(old_path);
            }
            let mut entry_full = false;
            for path in entry_paths.iter().copied() {
                let impact = graph.classify_path(path);
                entry_modules.extend(impact.module_ids);
                source_paths.extend(impact.source_paths);
                reasons.extend(impact.reasons);
                entry_full |= impact.full;
            }
            if entry.old_path().is_some() {
                let old_module = graph.module_for_path(entry.old_path().unwrap());
                let new_module = graph.module_for_path(entry.path());
                if old_module != new_module {
                    reasons.push(ScopeReason::new(ReasonCode::CrossModuleChange, entry.path()));
                    if old_module.is_some() && new_module.is_some() {
                        entry_full = false;
                    } else {
                        reasons.push(ScopeReason::new(ReasonCode::DeletedModuleUnresolved, entry.path()));
                        entry_full = true;
                    }
                }
            }
            if entry_modules.is_empty() && !entry_full {
                reasons.push(ScopeReason::new(ReasonCode::UnmappedChange, entry.path()));
                entry_full = true;
            }
            seed_modules.extend(entry_modules);
            if entry_full {
                full = true;
            }
            if entry_paths.iter().any(|path| {
                !graph.is_java_source(path)
                    || path.ends_with("/module-info.java")
                    || *path == "module-info.java"
            }) {
                file_candidate_only = false;
            }
        }

        let closure = graph.reverse_dependency_closure(&seed_modules);
        if closure.len() > seed_modules.len() && !full {
            reasons.push(ScopeReason::new(ReasonCode::ReverseDependencyClosure, join_ids(&closure)));
        }
        if source_paths.is_empty() {
            file_candidate_only = false;
        }

        let all_modules = graph.module_ids();
        let pmd_ast = make_backend_scope(
            AnalysisKind::PmdAst,
            if full {
                ScopeLevel::Full
            } else if file_candidate_only {
                ScopeLevel::FileCandidate
            } else {
                ScopeLevel::Module
            },
            if full { &all_modules } else if file_candidate_only { &seed_modules } else { &closure },
            if full || !file_candidate_only { &changed_path_set } else { &source_paths },
            &reasons,
        );
        let pmd_type_resolution = make_backend_scope(
            AnalysisKind::PmdTypeResolution,
            if full { ScopeLevel::Full } else { ScopeLevel::Module },
            if full { &all_modules } else { &closure },
            &changed_path_set,
            &reasons,
        );
        let mut spotbugs_reasons = reasons.clone();
        if !full {
            spotbugs_reasons.push(ScopeReason::new(
                ReasonCode::SpotBugsBytecodeClosure,
                "bytecode and classpath remain module-scoped",
            ));
        }
        let spotbugs_bytecode = make_backend_scope(
            AnalysisKind::SpotBugsBytecode,
            if full { ScopeLevel::Full } else { ScopeLevel::Module },
            if full { &all_modules } else { &closure },
            &changed_path_set,
            &spotbugs_reasons,
        );

        let mut affected_module_ids = BTreeSet::new();
        affected_module_ids.extend(pmd_ast.module_ids.iter().cloned());
        affected_module_ids.extend(pmd_type_resolution.module_ids.iter().cloned());
        affected_module_ids.extend(spotbugs_bytecode.module_ids.iter().cloned());
        let classification = [pmd_ast.level, pmd_type_resolution.level, spotbugs_bytecode.level]
            .into_iter()
            .max_by_key(|level| level.rank())
            .unwrap_or(ScopeLevel::Full);
        let reasons = normalize_reasons(reasons);
        let mut scope = AffectedScope {
            build_system: graph.system,
            changed_set_digest: changed_set.digest(),
            workspace_graph_digest,
            changed_paths,
            affected_module_ids: affected_module_ids.into_iter().collect(),
            classification,
            reasons: reasons.clone(),
            pmd_ast,
            pmd_type_resolution,
            spotbugs_bytecode,
            digest: String::new(),
        };
        scope.digest = hex_digest(&Sha256::digest(scope_canonical_bytes(&scope)));
        Ok(scope)
    }
}

#[derive(Debug, Clone)]
struct PathImpact {
    module_ids: BTreeSet<String>,
    source_paths: BTreeSet<String>,
    reasons: Vec<ScopeReason>,
    full: bool,
}

impl PathImpact {
    fn empty() -> Self {
        Self { module_ids: BTreeSet::new(), source_paths: BTreeSet::new(), reasons: Vec::new(), full: false }
    }
}

#[derive(Debug, Clone)]
struct ModuleSpec {
    id: String,
    relative_root: String,
    artifact: Option<String>,
    source_roots: BTreeSet<String>,
    generated_roots: BTreeSet<String>,
    build_files: BTreeSet<String>,
    dependency_artifacts: BTreeSet<String>,
    dependencies: BTreeSet<String>,
}

impl ModuleSpec {
    fn new(id: impl Into<String>, relative_root: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            relative_root: relative_root.into(),
            artifact: None,
            source_roots: BTreeSet::new(),
            generated_roots: BTreeSet::new(),
            build_files: BTreeSet::new(),
            dependency_artifacts: BTreeSet::new(),
            dependencies: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct ProjectGraph {
    system: Option<BuildSystem>,
    modules: BTreeMap<String, ModuleSpec>,
    reasons: Vec<ScopeReason>,
}

impl ProjectGraph {
    fn discover(root: &Path) -> Result<Self, AffectedScopeError> {
        let has_maven = root.join("pom.xml").is_file();
        let has_gradle = ["settings.gradle", "settings.gradle.kts", "build.gradle", "build.gradle.kts"]
            .iter()
            .any(|name| root.join(name).is_file());
        let mut graph = Self { system: None, modules: BTreeMap::new(), reasons: Vec::new() };
        match (has_maven, has_gradle) {
            (true, false) => {
                graph.system = Some(BuildSystem::Maven);
                graph.discover_maven(root)?;
            }
            (false, true) => {
                graph.system = Some(BuildSystem::Gradle);
                graph.discover_gradle(root)?;
            }
            (true, true) => {
                graph.reasons.push(ScopeReason::new(ReasonCode::MultipleBuildSystems, "pom.xml and Gradle metadata"));
            }
            (false, false) => {
                graph.reasons.push(ScopeReason::new(ReasonCode::BuildSystemNotDetected, root.display().to_string()));
                graph.add_inferred_root(root);
            }
        }
        graph.resolve_dependencies();
        Ok(graph)
    }

    fn discover_maven(&mut self, root: &Path) -> Result<(), AffectedScopeError> {
        let mut pending = vec![(String::new(), PathBuf::from("pom.xml"))];
        let mut seen = BTreeSet::new();
        while let Some((relative_root, pom_relative)) = pending.pop() {
            if seen.len() >= MAX_METADATA_VALUES {
                return Err(AffectedScopeError::MetadataBudgetExceeded {
                    path: root.join(&pom_relative),
                    detail: format!("module count exceeds {MAX_METADATA_VALUES}"),
                });
            }
            let id = if relative_root.is_empty() { "root".to_string() } else { relative_root.clone() };
            if !seen.insert(id.clone()) {
                self.reasons.push(ScopeReason::new(ReasonCode::BuildGraphIncomplete, format!("duplicate Maven module {id}")));
                continue;
            }
            let pom_path = root.join(&pom_relative);
            let Some(text) = read_metadata(&pom_path)? else {
                self.reasons.push(ScopeReason::new(ReasonCode::BuildGraphIncomplete, format!("missing {}", pom_relative.display())));
                continue;
            };
            let mut module = ModuleSpec::new(&id, &relative_root);
            module.build_files.insert(pom_relative.to_string_lossy().replace('\\', "/"));
            module.artifact = first_tag(&text, "artifactId");
            add_default_source_roots(&mut module, &relative_root);
            if maven_graph_is_context_dependent(&text) {
                self.reasons.push(ScopeReason::new(
                    ReasonCode::DynamicBuildGraph,
                    format!("{} contains profile/property/plugin graph inputs", pom_relative.display()),
                ));
            }
            add_maven_metadata(&mut module, &text, &relative_root, &id, &mut self.reasons);
            if self.modules.insert(id.clone(), module).is_some() {
                self.reasons.push(ScopeReason::new(ReasonCode::BuildGraphIncomplete, format!("duplicate Maven module {id}")));
            }
            for child in tag_values(&text, "module") {
                match join_relative(&relative_root, &child) {
                    Ok(child_root) => pending.push((child_root.clone(), PathBuf::from(format!("{child_root}/pom.xml")))),
                    Err(detail) => self.reasons.push(ScopeReason::new(ReasonCode::BuildGraphIncomplete, detail)),
                }
            }
        }
        if root.join("src/main/java").is_dir() || root.join("src/test/java").is_dir() {
            self.add_inferred_root(root);
        }
        Ok(())
    }

    fn discover_gradle(&mut self, root: &Path) -> Result<(), AffectedScopeError> {
        let settings = ["settings.gradle.kts", "settings.gradle"].iter().map(|name| root.join(name)).find(|path| path.is_file());
        let mut includes = Vec::new();
        if let Some(settings_path) = settings {
            let text = read_metadata(&settings_path)?.unwrap_or_default();
            let (values, dynamic) = parse_gradle_includes(&text);
            if values.len() > MAX_METADATA_VALUES {
                return Err(AffectedScopeError::MetadataBudgetExceeded {
                    path: settings_path,
                    detail: format!("module count exceeds {MAX_METADATA_VALUES}"),
                });
            }
            includes = values;
            if dynamic {
                self.reasons.push(ScopeReason::new(ReasonCode::DynamicBuildGraph, settings_path.display().to_string()));
            }
            if text.contains("projectDir") {
                self.reasons.push(ScopeReason::new(ReasonCode::ProjectDirectoryOverride, settings_path.display().to_string()));
            }
        }
        let had_includes = !includes.is_empty();
        for (id, relative_root) in includes {
            let mut module = ModuleSpec::new(&id, &relative_root);
            module.build_files.insert(format!("{relative_root}/build.gradle"));
            module.build_files.insert(format!("{relative_root}/build.gradle.kts"));
            add_default_source_roots(&mut module, &relative_root);
            if let Some(build) = gradle_build_file(root, &relative_root) {
                if let Some(text) = read_metadata(&build)? {
                    if gradle_graph_is_context_dependent(&text) {
                        self.reasons.push(ScopeReason::new(
                            ReasonCode::DynamicBuildGraph,
                            format!("{} contains context-dependent Gradle graph inputs", build.display()),
                        ));
                    }
                    add_gradle_metadata(&mut module, &text, &relative_root, &mut self.reasons);
                }
            }
            self.modules.insert(id, module);
        }
        let root_build = gradle_build_file(root, "");
        if let Some(build) = root_build {
            if let Some(text) = read_metadata(&build)? {
                if gradle_graph_is_context_dependent(&text) {
                    self.reasons.push(ScopeReason::new(
                        ReasonCode::DynamicBuildGraph,
                        format!("{} contains context-dependent Gradle graph inputs", build.display()),
                    ));
                }
                if text.contains("sourceSets") {
                    self.reasons.push(ScopeReason::new(ReasonCode::CustomSourceSetUnresolved, build.display().to_string()));
                }
                add_generated_marker_reason(&text, &build, &mut self.reasons);
            }
        }
        if !had_includes && (root.join("src/main/java").is_dir() || root.join("src/test/java").is_dir()) {
            self.add_inferred_root(root);
        }
        Ok(())
    }

    fn add_inferred_root(&mut self, root: &Path) {
        let mut module = ModuleSpec::new("root", "");
        add_default_source_roots(&mut module, "");
        if root.join("pom.xml").is_file() {
            module.build_files.insert("pom.xml".into());
        }
        if gradle_build_file(root, "").is_some() {
            module.build_files.insert("build.gradle".into());
            module.build_files.insert("build.gradle.kts".into());
        }
        self.modules.entry("root".into()).or_insert(module);
    }

    fn resolve_dependencies(&mut self) {
        let mut artifact_to_ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for module in self.modules.values() {
            if let Some(artifact) = &module.artifact {
                artifact_to_ids.entry(artifact.clone()).or_default().push(module.id.clone());
            }
        }
        let snapshot = self.modules.clone();
        for module in self.modules.values_mut() {
            for dependency in module.dependency_artifacts.clone() {
                if let Some(ids) = artifact_to_ids.get(&dependency) {
                    if ids.len() == 1 {
                        module.dependencies.insert(ids[0].clone());
                    } else {
                        self.reasons.push(ScopeReason::new(ReasonCode::BuildGraphIncomplete, format!("ambiguous dependency {dependency}")));
                    }
                } else if snapshot.contains_key(&dependency) {
                    module.dependencies.insert(dependency);
                }
            }
        }
    }

    fn module_ids(&self) -> BTreeSet<String> {
        self.modules.keys().cloned().collect()
    }

    fn digest(&self) -> String {
        hex_digest(&Sha256::digest(graph_canonical_bytes(self)))
    }

    fn module_for_path(&self, path: &str) -> Option<String> {
        self.modules
            .values()
            .filter(|module| {
                if module.relative_root.is_empty() {
                    module.source_roots.iter().any(|root| is_under(path, root)) || module.build_files.contains(path)
                } else {
                    is_under(path, &module.relative_root)
                }
            })
            .max_by_key(|module| module.relative_root.len())
            .map(|module| module.id.clone())
    }

    fn is_java_source(&self, path: &str) -> bool {
        self.modules.values().any(|module| module.source_roots.iter().any(|root| is_under(path, root)))
    }

    fn reverse_dependency_closure(&self, seeds: &BTreeSet<String>) -> BTreeSet<String> {
        let mut selected = seeds.clone();
        loop {
            let mut changed = false;
            for module in self.modules.values() {
                if module.dependencies.iter().any(|dependency| selected.contains(dependency)) && selected.insert(module.id.clone()) {
                    changed = true;
                }
            }
            if !changed { break; }
        }
        selected
    }

    fn classify_path(&self, path: &str) -> PathImpact {
        let mut impact = PathImpact::empty();
        let normalized = path.replace('\\', "/");
        let lower = normalized.to_ascii_lowercase();
        if is_policy_path(&lower) {
            impact.full = true;
            impact.reasons.push(ScopeReason::new(ReasonCode::PolicyConfigurationChanged, path));
            return impact;
        }
        if is_rule_path(&lower) {
            impact.full = true;
            impact.reasons.push(ScopeReason::new(ReasonCode::RuleConfigurationChanged, path));
            return impact;
        }
        if is_resolver_path(&lower) {
            impact.full = true;
            impact.reasons.push(ScopeReason::new(ReasonCode::ResolverConfigurationChanged, path));
            return impact;
        }
        if is_lockfile(&lower) {
            impact.full = true;
            impact.reasons.push(ScopeReason::new(ReasonCode::LockfileChanged, path));
            return impact;
        }
        if self.is_shared_build_path(&normalized) {
            impact.full = true;
            impact.reasons.push(ScopeReason::new(ReasonCode::SharedBuildConfiguration, path));
            return impact;
        }
        let module_id = self.module_for_path(&normalized);
        if let Some(module_id) = module_id {
            impact.module_ids.insert(module_id.clone());
            if self.is_generated_path(&normalized) {
                impact.full = true;
                impact.reasons.push(ScopeReason::new(ReasonCode::GeneratedSourcesUncertain, normalized));
            } else if self.is_java_source(&normalized) {
                impact.source_paths.insert(normalized.clone());
                if normalized.ends_with("/module-info.java") || normalized == "module-info.java" {
                    impact.reasons.push(ScopeReason::new(ReasonCode::ModuleDescriptorChanged, normalized));
                } else {
                    impact.reasons.push(ScopeReason::new(ReasonCode::JavaSourceChanged, normalized));
                }
            } else {
                impact.reasons.push(ScopeReason::new(ReasonCode::ModuleResourceChanged, normalized));
            }
        } else {
            impact.full = true;
            impact.reasons.push(ScopeReason::new(ReasonCode::UnmappedChange, normalized));
        }
        impact
    }

    fn is_shared_build_path(&self, path: &str) -> bool {
        match self.system {
            Some(BuildSystem::Maven) => path == "pom.xml" || path.starts_with(".mvn/") || path == "mvnw" || path == "mvnw.cmd",
            Some(BuildSystem::Gradle) => matches!(path, "settings.gradle" | "settings.gradle.kts" | "build.gradle" | "build.gradle.kts" | "gradlew" | "gradlew.bat") || path.starts_with("gradle/"),
            None => false,
        }
    }

    fn is_generated_path(&self, path: &str) -> bool {
        self.modules.values().any(|module| module.generated_roots.iter().any(|root| is_under(path, root)))
    }
}

fn make_backend_scope(
    kind: AnalysisKind,
    level: ScopeLevel,
    modules: &BTreeSet<String>,
    paths: &BTreeSet<String>,
    reasons: &[ScopeReason],
) -> BackendScope {
    BackendScope {
        kind,
        level,
        module_ids: modules.iter().cloned().collect(),
        paths: paths.iter().cloned().collect(),
        reasons: normalize_reasons(reasons.to_vec()),
    }
}

fn collect_changed_paths(changed_set: &ChangedSet) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for entry in changed_set.entries() {
        paths.insert(entry.path().to_string());
        if let Some(old_path) = entry.old_path() {
            paths.insert(old_path.to_string());
        }
    }
    paths
}

fn normalize_reasons(mut reasons: Vec<ScopeReason>) -> Vec<ScopeReason> {
    reasons.sort();
    reasons.dedup();
    reasons
}

fn join_ids(ids: &BTreeSet<String>) -> String {
    ids.iter().cloned().collect::<Vec<_>>().join(",")
}

fn scope_canonical_bytes(scope: &AffectedScope) -> Vec<u8> {
    let mut bytes = AFFECTED_SCOPE_VERSION.as_bytes().to_vec();
    bytes.push(0);
    append_string(&mut bytes, scope.changed_set_digest());
    append_string(&mut bytes, &scope.workspace_graph_digest);
    append_string(&mut bytes, scope.build_system.map(BuildSystem::code).unwrap_or("unknown"));
    append_strings(&mut bytes, &scope.changed_paths);
    append_strings(&mut bytes, &scope.affected_module_ids);
    append_reasons(&mut bytes, &scope.reasons);
    for backend in [&scope.pmd_ast, &scope.pmd_type_resolution, &scope.spotbugs_bytecode] {
        append_string(&mut bytes, backend.kind.code());
        append_string(&mut bytes, backend.level.code());
        append_strings(&mut bytes, &backend.module_ids);
        append_strings(&mut bytes, &backend.paths);
        append_reasons(&mut bytes, &backend.reasons);
    }
    bytes
}

fn graph_canonical_bytes(graph: &ProjectGraph) -> Vec<u8> {
    let mut bytes = b"workspace-graph/v1\0".to_vec();
    append_string(&mut bytes, graph.system.map(BuildSystem::code).unwrap_or("unknown"));
    append_reasons(&mut bytes, &normalize_reasons(graph.reasons.clone()));
    bytes.extend_from_slice(&(graph.modules.len() as u64).to_be_bytes());
    for module in graph.modules.values() {
        append_string(&mut bytes, &module.id);
        append_string(&mut bytes, &module.relative_root);
        append_string(&mut bytes, module.artifact.as_deref().unwrap_or(""));
        append_strings(&mut bytes, &module.source_roots.iter().cloned().collect::<Vec<_>>());
        append_strings(&mut bytes, &module.generated_roots.iter().cloned().collect::<Vec<_>>());
        append_strings(&mut bytes, &module.build_files.iter().cloned().collect::<Vec<_>>());
        append_strings(&mut bytes, &module.dependency_artifacts.iter().cloned().collect::<Vec<_>>());
        append_strings(&mut bytes, &module.dependencies.iter().cloned().collect::<Vec<_>>());
    }
    bytes
}

fn append_reasons(bytes: &mut Vec<u8>, reasons: &[ScopeReason]) {
    bytes.extend_from_slice(&(reasons.len() as u64).to_be_bytes());
    for reason in reasons {
        append_string(bytes, reason.code.code());
        append_string(bytes, &reason.detail);
    }
}

fn append_strings(bytes: &mut Vec<u8>, values: &[String]) {
    bytes.extend_from_slice(&(values.len() as u64).to_be_bytes());
    for value in values { append_string(bytes, value); }
}

fn append_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn add_default_source_roots(module: &mut ModuleSpec, relative_root: &str) {
    for suffix in ["src/main/java", "src/test/java"] {
        module.source_roots.insert(join_path(relative_root, suffix));
    }
    for suffix in ["target/generated-sources", "build/generated", "src/generated/java"] {
        module.generated_roots.insert(join_path(relative_root, suffix));
    }
}

fn add_maven_metadata(module: &mut ModuleSpec, text: &str, relative_root: &str, id: &str, reasons: &mut Vec<ScopeReason>) {
    for tag in ["sourceDirectory", "testSourceDirectory"] {
        if let Some(value) = first_tag(text, tag) {
            if value.contains("${") {
                reasons.push(ScopeReason::new(ReasonCode::CustomSourceSetUnresolved, format!("{id}:{tag}")));
            } else if let Ok(path) = join_relative(relative_root, &value) {
                module.source_roots.insert(path);
            }
        }
    }
    for block in tag_blocks(text, "dependency") {
        if let Some(value) = first_tag(block, "artifactId") {
            if value.contains("${") { reasons.push(ScopeReason::new(ReasonCode::BuildGraphIncomplete, format!("{id}:dependency"))); }
            else { module.dependency_artifacts.insert(value); }
        }
    }
    let lowered = text.to_ascii_lowercase();
    if lowered.contains("<sourcedirectory>") || lowered.contains("<testsourcedirectory>") {
        // Literal roots were added above; this marker is only a reason when a
        // property prevented us from resolving the root.
    }
    if has_generated_marker(&lowered) {
        add_generated_marker_reason(text, Path::new(id), reasons);
    }
}

fn maven_graph_is_context_dependent(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    [
        "<profiles>",
        "<properties>",
        "<dependencymanagement>",
        "<pluginmanagement>",
        "<plugin>",
        "<activation>",
        "build-helper-maven-plugin",
        "flatten-maven-plugin",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn add_gradle_metadata(module: &mut ModuleSpec, text: &str, relative_root: &str, reasons: &mut Vec<ScopeReason>) {
    let (dependencies, dynamic) = parse_gradle_project_refs(text);
    module.dependency_artifacts.extend(dependencies);
    if dynamic {
        reasons.push(ScopeReason::new(ReasonCode::DynamicBuildGraph, join_path(relative_root, "build.gradle")));
    }
    if text.contains("sourceSets") {
        reasons.push(ScopeReason::new(ReasonCode::CustomSourceSetUnresolved, join_path(relative_root, "sourceSets")));
    }
    add_generated_marker_reason(text, Path::new(&join_path(relative_root, "build.gradle")), reasons);
}

fn gradle_graph_is_context_dependent(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    [
        "includebuild",
        "dependencysubstitution",
        "versioncatalogs",
        "libs.",
        "findproperty(",
        "providers.",
        "project.ext",
        "apply from:",
        "settingsevaluated",
        // Root/global Gradle configuration can establish dependencies or
        // source/build inputs for every project without appearing in a
        // module-local `dependencies` block. We cannot safely evaluate that
        // closure in SAFE_STATIC, so widen to FULL.
        "subprojects",
        "allprojects",
        "evaluationdependson",
        "gradle.beforeproject",
        "gradle.afterproject",
        "plugins.withid",
        "configurations.all",
        "tasks.withtype",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

fn add_generated_marker_reason(text: &str, path: &Path, reasons: &mut Vec<ScopeReason>) {
    if has_generated_marker(&text.to_ascii_lowercase()) {
        reasons.push(ScopeReason::new(ReasonCode::GeneratedSourcesUncertain, path.display().to_string()));
    }
}

fn has_generated_marker(lowered: &str) -> bool {
    ["generated", "generate", "annotationprocessor", "annotation_processor", "protobuf", "openapi", "jooq", "kapt", "codegen"]
        .iter()
        .any(|marker| lowered.contains(marker))
}

fn read_metadata(path: &Path) -> Result<Option<String>, AffectedScopeError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(AffectedScopeError::Io { path: path.to_path_buf(), source }),
    };
    if metadata.len() > MAX_METADATA_BYTES as u64 {
        return Err(AffectedScopeError::MetadataBudgetExceeded {
            path: path.to_path_buf(),
            detail: format!("file exceeds {MAX_METADATA_BYTES} bytes"),
        });
    }
    let file = fs::File::open(path).map_err(|source| AffectedScopeError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_METADATA_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| AffectedScopeError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() > MAX_METADATA_BYTES {
        return Err(AffectedScopeError::MetadataBudgetExceeded {
            path: path.to_path_buf(),
            detail: format!("file exceeds {MAX_METADATA_BYTES} bytes"),
        });
    }
    let text = String::from_utf8(bytes).map_err(|error| AffectedScopeError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, error),
    })?;
    if metadata_values_exceed_budget(&text) {
        return Err(AffectedScopeError::MetadataBudgetExceeded {
            path: path.to_path_buf(),
            detail: format!(
                "metadata contains more than {MAX_METADATA_VALUES} values or a value larger than {MAX_METADATA_VALUE_BYTES} bytes"
            ),
        });
    }
    Ok(Some(text))
}

fn metadata_values_exceed_budget(text: &str) -> bool {
    let xml_values = ["module", "dependency", "artifactId", "groupId", "artifact"]
        .into_iter()
        .flat_map(|tag| tag_blocks(text, tag));
    let mut count = 0usize;
    for value in xml_values {
        count += 1;
        if count > MAX_METADATA_VALUES || value.trim().len() > MAX_METADATA_VALUE_BYTES {
            return true;
        }
    }
    text.lines().any(|line| line.len() > MAX_METADATA_VALUE_BYTES)
}

fn gradle_build_file(root: &Path, relative_root: &str) -> Option<PathBuf> {
    ["build.gradle.kts", "build.gradle"].iter().map(|name| {
        if relative_root.is_empty() { root.join(name) } else { root.join(relative_root).join(name) }
    }).find(|path| path.is_file())
}

fn first_tag(text: &str, tag: &str) -> Option<String> {
    tag_values(text, tag).into_iter().next()
}

fn tag_values(text: &str, tag: &str) -> Vec<String> {
    tag_blocks(text, tag)
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains('<') && !value.contains('>'))
        .map(ToOwned::to_owned)
        .collect()
}

fn tag_blocks<'a>(text: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut values = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find(&open) {
        let start = cursor + offset;
        let after = start + open.len();
        if text.as_bytes().get(after).is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'-') {
            cursor = after;
            continue;
        }
        let Some(end_offset) = text[after..].find('>') else { break; };
        let value_start = after + end_offset + 1;
        let Some(value_offset) = text[value_start..].find(&close) else { break; };
        let value_end = value_start + value_offset;
        values.push(&text[value_start..value_end]);
        cursor = value_end + close.len();
    }
    values
}

fn join_relative(parent: &str, child: &str) -> Result<String, String> {
    let normalized = child.replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with('/') || normalized.contains("${") || normalized.split('/').any(|part| part == ".." || part.is_empty()) {
        return Err(format!("invalid module path {child:?}"));
    }
    Ok(join_path(parent, &normalized))
}

fn join_path(parent: &str, child: &str) -> String {
    if parent.is_empty() { child.trim_matches('/').to_string() } else { format!("{}/{}", parent.trim_matches('/'), child.trim_matches('/')) }
}

fn normalize_gradle_project(value: &str) -> Option<(String, String)> {
    let value = value.strip_prefix(':').unwrap_or(value);
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.iter().any(|part| part.is_empty() || *part == "." || *part == ".." || part.contains('/') || part.contains('\\')) { return None; }
    Some((parts.join(":"), parts.join("/")))
}

fn parse_gradle_includes(text: &str) -> (Vec<(String, String)>, bool) {
    let mut values = Vec::new();
    let mut dynamic = false;
    for position in keyword_positions(text, "include") {
        let (literals, uncertain) = parse_quoted_arguments(text, position + "include".len());
        if uncertain { dynamic = true; }
        for literal in literals {
            if let Some(value) = normalize_gradle_project(&literal) { values.push(value); } else { dynamic = true; }
        }
    }
    values.sort();
    values.dedup();
    (values, dynamic)
}

fn parse_gradle_project_refs(text: &str) -> (BTreeSet<String>, bool) {
    let mut values = BTreeSet::new();
    let mut dynamic = false;
    for position in keyword_positions(text, "project") {
        let end = text[position..].find(['\n', ';', ')']).map(|offset| position + offset).unwrap_or(text.len());
        let fragment = &text[position..end];
        let mut found = false;
        let mut cursor = 0;
        while let Some(offset) = fragment[cursor..].find(['\'', '"']) {
            let start = cursor + offset;
            let quote = fragment.as_bytes()[start];
            let rest = &fragment[start + 1..];
            let Some(close) = rest.find(quote as char) else { break; };
            let value = &rest[..close];
            if value.starts_with(':') {
                if let Some((id, _)) = normalize_gradle_project(value) { values.insert(id); found = true; }
            }
            cursor = start + close + 2;
        }
        if !found { dynamic = true; }
    }
    (values, dynamic)
}

fn keyword_positions(text: &str, keyword: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut positions = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] == b'/' && bytes.get(cursor + 1) == Some(&b'/') {
            cursor += 2;
            while cursor < bytes.len() && bytes[cursor] != b'\n' { cursor += 1; }
            continue;
        }
        if bytes[cursor] == b'/' && bytes.get(cursor + 1) == Some(&b'*') {
            cursor += 2;
            while cursor + 1 < bytes.len() && !(bytes[cursor] == b'*' && bytes[cursor + 1] == b'/') { cursor += 1; }
            cursor = (cursor + 2).min(bytes.len());
            continue;
        }
        if bytes[cursor] == b'\'' || bytes[cursor] == b'"' {
            let quote = bytes[cursor];
            cursor += 1;
            while cursor < bytes.len() {
                if bytes[cursor] == b'\\' { cursor = (cursor + 2).min(bytes.len()); continue; }
                if bytes[cursor] == quote { cursor += 1; break; }
                cursor += 1;
            }
            continue;
        }
        if bytes.get(cursor..cursor + keyword.len()) == Some(keyword.as_bytes())
            && (cursor == 0 || !is_identifier_byte(bytes[cursor - 1]))
            && (cursor + keyword.len() == bytes.len() || !is_identifier_byte(bytes[cursor + keyword.len()]))
        {
            positions.push(cursor);
            cursor += keyword.len();
        } else {
            cursor += 1;
        }
    }
    positions
}

fn parse_quoted_arguments(text: &str, start: usize) -> (Vec<String>, bool) {
    let bytes = text.as_bytes();
    let mut cursor = start;
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() { cursor += 1; }
    let parenthesized = bytes.get(cursor) == Some(&b'(');
    if parenthesized { cursor += 1; }
    let mut values = Vec::new();
    let mut uncertain = false;
    loop {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() { cursor += 1; }
        if (!parenthesized && (cursor >= bytes.len() || bytes[cursor] == b'\n' || bytes[cursor] == b';')) || (parenthesized && bytes.get(cursor) == Some(&b')')) { break; }
        if bytes.get(cursor) != Some(&b'\'') && bytes.get(cursor) != Some(&b'"') {
            uncertain = true;
            while cursor < bytes.len() && bytes[cursor] != b'\n' && (!parenthesized || bytes[cursor] != b')') { cursor += 1; }
            break;
        }
        let quote = bytes[cursor];
        cursor += 1;
        let mut value = String::new();
        let mut closed = false;
        while cursor < bytes.len() {
            if bytes[cursor] == b'\\' { uncertain = true; cursor += 1; if cursor < bytes.len() { cursor += 1; } continue; }
            if bytes[cursor] == quote { cursor += 1; closed = true; break; }
            value.push(bytes[cursor] as char);
            cursor += 1;
        }
        if !closed { uncertain = true; break; }
        values.push(value);
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() { cursor += 1; }
        if bytes.get(cursor) == Some(&b',') { cursor += 1; continue; }
        if parenthesized && bytes.get(cursor) == Some(&b')') { break; }
        if !parenthesized && (cursor >= bytes.len() || bytes[cursor] == b'\n' || bytes[cursor] == b';') { break; }
        uncertain = true;
        break;
    }
    (values, uncertain)
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

fn is_under(path: &str, root: &str) -> bool {
    if root.is_empty() { return true; }
    path == root || path.starts_with(&format!("{root}/"))
}

fn is_policy_path(path: &str) -> bool {
    path.starts_with(".ruleops/") || path.starts_with(".rf/") || path.starts_with("ruleops/") || path.starts_with("policy/") || path.ends_with("/policy.yaml") || path.ends_with("/policy.yml") || path == "policy.yaml" || path == "policy.yml"
}

fn is_rule_path(path: &str) -> bool {
    path.starts_with("rules/") || path.starts_with("rule-intent/") || path.starts_with("specs/rule-") || path.starts_with("standards/") || path.ends_with("/rules.yaml") || path == "rules.yaml"
}

fn is_resolver_path(path: &str) -> bool {
    path.starts_with("resolver/") || path.ends_with("/resolver.yaml") || path == "resolver.yaml"
}

fn is_lockfile(path: &str) -> bool {
    path == "gradle.lockfile" || path.ends_with("/gradle.lockfile") || path.ends_with(".lockfile") || path.ends_with(".lock") || path == "package-lock.json" || path.ends_with("/package-lock.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use erf_git_scope::GitChangeResolver;
    use std::process::{Command, Output};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo(label: &str) -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("erf-affected-scope-{label}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        git(&path, &["init", "--quiet"]);
        git(&path, &["config", "user.email", "erf-tests@example.invalid"]);
        git(&path, &["config", "user.name", "ERF Tests"]);
        path
    }

    fn git(repo: &Path, args: &[&str]) -> Output {
        let output = Command::new("git").arg("-C").arg(repo).args(args).output().unwrap();
        assert!(output.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&output.stderr));
        output
    }

    fn git_text(repo: &Path, args: &[&str]) -> String {
        String::from_utf8(git(repo, args).stdout).unwrap().trim().to_string()
    }

    fn write(repo: &Path, relative: &str, text: &str) {
        let path = repo.join(relative);
        if let Some(parent) = path.parent() { fs::create_dir_all(parent).unwrap(); }
        fs::write(path, text).unwrap();
    }

    fn commit(repo: &Path, message: &str) -> String {
        git(repo, &["add", "."]);
        git(repo, &["commit", "--quiet", "-m", message]);
        git_text(repo, &["rev-parse", "HEAD"])
    }

    fn changed(repo: &Path, base: &str, head: &str) -> ChangedSet {
        let range = erf_git_scope::GitChangeRange::new(base, head).unwrap();
        GitChangeResolver::new(repo).resolve(&range).unwrap()
    }

    #[test]
    fn maven_source_change_closes_reverse_dependents_and_keeps_pmd_file_candidates() {
        let repo = temp_repo("maven");
        write(&repo, "pom.xml", "<project><artifactId>root</artifactId><modules><module>lib</module><module>app</module></modules></project>");
        write(&repo, "lib/pom.xml", "<project><artifactId>lib</artifactId></project>");
        write(&repo, "app/pom.xml", "<project><artifactId>app</artifactId><dependencies><dependency><artifactId>lib</artifactId></dependency></dependencies></project>");
        write(&repo, "lib/src/main/java/lib/Lib.java", "class Lib {}\n");
        write(&repo, "app/src/main/java/app/App.java", "class App {}\n");
        let base = commit(&repo, "base");
        write(&repo, "lib/src/main/java/lib/Lib.java", "class Lib { int changed; }\n");
        let head = commit(&repo, "change lib");
        let set = changed(&repo, &base, &head);
        let scope = AffectedScopeResolver::new(&repo).resolve(&set).unwrap();
        assert_eq!(scope.build_system(), Some(BuildSystem::Maven));
        assert_eq!(scope.pmd_ast().level(), ScopeLevel::FileCandidate);
        assert_eq!(scope.pmd_type_resolution().level(), ScopeLevel::Module);
        assert_eq!(scope.pmd_type_resolution().module_ids(), &["app".to_string(), "lib".to_string()]);
        assert_eq!(scope.spotbugs_bytecode().module_ids(), &["app".to_string(), "lib".to_string()]);
        assert!(scope.pmd_ast().paths().contains(&"lib/src/main/java/lib/Lib.java".to_string()));
        assert!(!scope.digest().is_empty());
        assert!(!scope.workspace_graph_digest().is_empty());
        let repeated = AffectedScopeResolver::new(&repo).resolve(&set).unwrap();
        assert_eq!(scope, repeated);
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn gradle_literal_projects_are_resolved_without_executing_build_logic() {
        let repo = temp_repo("gradle");
        write(&repo, "settings.gradle", "// 中文注释 include ':fake'\nval text = \"include ':fake'\"\ninclude ':lib', ':app'\n");
        write(&repo, "lib/build.gradle", "plugins { id 'java' }\n");
        write(&repo, "app/build.gradle", "dependencies { implementation project(':lib') }\n");
        write(&repo, "lib/src/main/java/lib/Lib.java", "class Lib {}\n");
        write(&repo, "app/src/main/java/app/App.java", "class App {}\n");
        let base = commit(&repo, "base");
        write(&repo, "lib/src/main/java/lib/Lib.java", "class Lib { int changed; }\n");
        let head = commit(&repo, "change lib");
        let scope = AffectedScopeResolver::new(&repo).resolve(&changed(&repo, &base, &head)).unwrap();
        assert_eq!(scope.build_system(), Some(BuildSystem::Gradle));
        assert_eq!(scope.pmd_type_resolution().module_ids(), &["app".to_string(), "lib".to_string()]);
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn maven_and_gradle_single_module_layouts_are_resolved() {
        let maven = temp_repo("single-maven");
        write(&maven, "pom.xml", "<project><artifactId>single</artifactId></project>");
        write(&maven, "src/main/java/App.java", "class App {}\n");
        let maven_base = commit(&maven, "base");
        write(&maven, "src/main/java/App.java", "class App { int changed; }\n");
        let maven_head = commit(&maven, "change");
        let maven_scope = AffectedScopeResolver::new(&maven).resolve(&changed(&maven, &maven_base, &maven_head)).unwrap();
        assert_eq!(maven_scope.build_system(), Some(BuildSystem::Maven));
        assert_eq!(maven_scope.pmd_ast().level(), ScopeLevel::FileCandidate);
        assert_eq!(maven_scope.affected_module_ids(), &["root".to_string()]);
        fs::remove_dir_all(maven).unwrap();

        let gradle = temp_repo("single-gradle");
        write(&gradle, "build.gradle", "plugins { id 'java' }\n");
        write(&gradle, "src/main/java/App.java", "class App {}\n");
        let gradle_base = commit(&gradle, "base");
        write(&gradle, "src/main/java/App.java", "class App { int changed; }\n");
        let gradle_head = commit(&gradle, "change");
        let gradle_scope = AffectedScopeResolver::new(&gradle).resolve(&changed(&gradle, &gradle_base, &gradle_head)).unwrap();
        assert_eq!(gradle_scope.build_system(), Some(BuildSystem::Gradle));
        assert_eq!(gradle_scope.pmd_ast().level(), ScopeLevel::FileCandidate);
        assert_eq!(gradle_scope.affected_module_ids(), &["root".to_string()]);
        fs::remove_dir_all(gradle).unwrap();
    }

    #[test]
    fn shared_config_lock_generated_and_unmapped_changes_fail_closed_to_full() {
        let repo = temp_repo("full");
        write(&repo, "pom.xml", "<project><artifactId>root</artifactId><modules><module>app</module></modules></project>");
        write(&repo, "app/pom.xml", "<project><artifactId>app</artifactId></project>");
        write(&repo, "app/src/main/java/App.java", "class App {}\n");
        let base = commit(&repo, "base");
        write(&repo, "pom.xml", "<project><artifactId>root</artifactId><modules><module>app</module></modules><properties><x>2</x></properties></project>");
        write(&repo, "gradle.lockfile", "lock\n");
        write(&repo, "policy.yaml", "policy: changed\n");
        write(&repo, "unmapped.txt", "unknown\n");
        let head = commit(&repo, "broad changes");
        let scope = AffectedScopeResolver::new(&repo).resolve(&changed(&repo, &base, &head)).unwrap();
        assert_eq!(scope.classification(), ScopeLevel::Full);
        assert_eq!(scope.pmd_ast().level(), ScopeLevel::Full);
        assert!(scope.reasons().iter().any(|reason| reason.code() == ReasonCode::SharedBuildConfiguration));
        assert!(scope.reasons().iter().any(|reason| reason.code() == ReasonCode::LockfileChanged));
        assert!(scope.reasons().iter().any(|reason| reason.code() == ReasonCode::PolicyConfigurationChanged));
        assert!(scope.reasons().iter().any(|reason| reason.code() == ReasonCode::UnmappedChange));
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn dynamic_gradle_and_generated_source_metadata_widen_scope() {
        let repo = temp_repo("uncertain");
        write(&repo, "settings.gradle.kts", "include(*projects)\n");
        write(&repo, "build.gradle.kts", "plugins { id(\"java\") }\n task generateSources {}\n");
        write(&repo, "src/main/java/App.java", "class App {}\n");
        let base = commit(&repo, "base");
        write(&repo, "src/main/java/App.java", "class App { int changed; }\n");
        let head = commit(&repo, "change");
        let scope = AffectedScopeResolver::new(&repo).resolve(&changed(&repo, &base, &head)).unwrap();
        assert_eq!(scope.classification(), ScopeLevel::Full);
        assert!(scope.reasons().iter().any(|reason| reason.code() == ReasonCode::DynamicBuildGraph));
        assert!(scope.reasons().iter().any(|reason| reason.code() == ReasonCode::GeneratedSourcesUncertain));
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn unsupported_gradle_and_maven_graph_inputs_widen_scope() {
        let gradle = temp_repo("context-dependent-gradle");
        write(&gradle, "settings.gradle", "include ':app'\nincludeBuild('../shared-build')\n");
        write(&gradle, "build.gradle", "configurations.all { resolutionStrategy.dependencySubstitution {} }\n");
        write(&gradle, "app/build.gradle", "dependencies { implementation(libs.guava) }\n");
        write(&gradle, "app/src/main/java/App.java", "class App {}\n");
        let base = commit(&gradle, "base");
        write(&gradle, "app/src/main/java/App.java", "class App { int changed; }\n");
        let head = commit(&gradle, "change");
        let scope = AffectedScopeResolver::new(&gradle).resolve(&changed(&gradle, &base, &head)).unwrap();
        assert_eq!(scope.classification(), ScopeLevel::Full);
        assert!(scope.reasons().iter().any(|reason| reason.code() == ReasonCode::DynamicBuildGraph));
        fs::remove_dir_all(gradle).unwrap();

        let global = temp_repo("global-gradle-configuration");
        write(&global, "settings.gradle", "include ':app'\n");
        write(&global, "build.gradle", "subprojects { evaluationDependsOn(':app') }\n");
        write(&global, "app/build.gradle", "plugins { id 'java' }\n");
        write(&global, "app/src/main/java/App.java", "class App {}\n");
        let base = commit(&global, "base");
        write(&global, "app/src/main/java/App.java", "class App { int changed; }\n");
        let head = commit(&global, "change");
        let scope = AffectedScopeResolver::new(&global).resolve(&changed(&global, &base, &head)).unwrap();
        assert_eq!(scope.classification(), ScopeLevel::Full);
        assert!(scope.reasons().iter().any(|reason| reason.code() == ReasonCode::DynamicBuildGraph));
        fs::remove_dir_all(global).unwrap();

        let maven = temp_repo("context-dependent-maven");
        write(&maven, "pom.xml", "<project><artifactId>app</artifactId><profiles><profile><id>ci</id></profile></profiles><properties><module.name>app</module.name></properties></project>");
        write(&maven, "src/main/java/App.java", "class App {}\n");
        let base = commit(&maven, "base");
        write(&maven, "src/main/java/App.java", "class App { int changed; }\n");
        let head = commit(&maven, "change");
        let scope = AffectedScopeResolver::new(&maven).resolve(&changed(&maven, &base, &head)).unwrap();
        assert_eq!(scope.classification(), ScopeLevel::Full);
        assert!(scope.reasons().iter().any(|reason| reason.code() == ReasonCode::DynamicBuildGraph));
        fs::remove_dir_all(maven).unwrap();
    }

    #[test]
    fn rename_preserves_old_and_new_paths_and_deleted_module_widens() {
        let repo = temp_repo("rename");
        write(&repo, "pom.xml", "<project><artifactId>root</artifactId><modules><module>app</module></modules></project>");
        write(&repo, "app/pom.xml", "<project><artifactId>app</artifactId></project>");
        write(&repo, "app/src/main/java/Old.java", "class Old {}\n");
        let base = commit(&repo, "base");
        git(&repo, &["mv", "app/src/main/java/Old.java", "app/src/main/java/New.java"]);
        let head = commit(&repo, "rename");
        let scope = AffectedScopeResolver::new(&repo).resolve(&changed(&repo, &base, &head)).unwrap();
        assert!(scope.changed_paths().contains(&"app/src/main/java/Old.java".to_string()));
        assert!(scope.changed_paths().contains(&"app/src/main/java/New.java".to_string()));
        assert_eq!(scope.classification(), ScopeLevel::Module);
        fs::remove_file(repo.join("app/src/main/java/New.java")).unwrap();
        let deleted_head = commit(&repo, "delete");
        let deleted_scope = AffectedScopeResolver::new(&repo).resolve(&changed(&repo, &head, &deleted_head)).unwrap();
        assert!(deleted_scope.changed_paths().contains(&"app/src/main/java/New.java".to_string()));
        assert_eq!(deleted_scope.classification(), ScopeLevel::Module);
        fs::remove_dir_all(repo).unwrap();
    }
}
