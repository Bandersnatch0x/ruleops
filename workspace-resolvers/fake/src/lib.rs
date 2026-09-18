//! Canned resolution logic of the reference fake Workspace Resolver, shared
//! between the binary and its contract tests so they can never drift.
//!
//! Resolution is SAFE_STATIC by design: the resolver walks the repository
//! layout (no build scripts executed), treats every directory holding a
//! `src/main` subtree as a module source root, and binds every file to its
//! sha256 content digest. Anything it cannot determine statically goes to
//! `unresolved_facts` — never to local guesses (spec §21).

use erf_contracts::workspace::v0 as wb;
use prost::Message as _;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Capability facts of the reference resolver.
pub fn facts() -> HashMap<String, bool> {
    HashMap::from([
        ("ecosystem.static-layout".to_string(), true),
        ("ecosystem.maven.safe-static".to_string(), true),
        ("ecosystem.gradle.safe-static".to_string(), true),
    ])
}

/// Resolves the layout under `repo_root` into a descriptor.
pub fn resolve(repo_root: &std::path::Path) -> std::io::Result<wb::WorkspaceDescriptor> {
    if !repo_root.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("repo root {} is not a directory", repo_root.display()),
        ));
    }
    let mut modules: Vec<wb::ModuleDescriptor> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();

    // Build-system identification is deliberately metadata-only. The adapter
    // records what it saw but never evaluates a Maven/Gradle build or invents
    // a dependency graph from incomplete static information.
    let has_maven = ["pom.xml", "settings.xml"]
        .iter()
        .any(|name| repo_root.join(name).is_file());
    let has_gradle = ["settings.gradle", "settings.gradle.kts", "build.gradle", "build.gradle.kts"]
        .iter()
        .any(|name| repo_root.join(name).is_file());
    if has_maven { unresolved.push("build-system:maven".to_string()); }
    if has_gradle { unresolved.push("build-system:gradle".to_string()); }

    // v0 static convention: one module per direct child directory that
    // carries a `src/main` source root; a root-level `src` is the single
    // default module.
    let default_root = repo_root.join("src");
    let mut roots: Vec<(String, std::path::PathBuf)> = Vec::new();
    if default_root.join("main").is_dir() {
        roots.push(("default".to_string(), default_root));
    }
    for entry in std::fs::read_dir(repo_root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && path != repo_root && path.join("src/main").is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                roots.push((name.to_string(), path.join("src")));
            }
        }
    }

    if roots.is_empty() {
        unresolved.push("module-structure:not-determined-statically".to_string());
    }
    // Static resolution never executes builds or reads dependency graphs
    // that would require them; both are structured as unresolved facts
    // (spec §21: never local guesses).
    unresolved.push("build-output:not-available-in-safe-static".to_string());
    unresolved.push("dependency-classpath:not-determined-statically".to_string());

    for (module_id, src_root) in roots {
        let mut files: Vec<wb::FileEntry> = Vec::new();
        collect_files(repo_root, &src_root, &mut files)?;
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let relative_root = src_root
            .strip_prefix(repo_root)
            .unwrap_or(&src_root)
            .to_string_lossy()
            .replace('\\', "/");
        modules.push(wb::ModuleDescriptor {
            module_id,
            language: "java".to_string(),
            source_roots: vec![relative_root],
            files,
        });
    }

    let mut descriptor = wb::WorkspaceDescriptor {
        mode: wb::WorkspaceResolutionMode::SafeStatic as i32,
        resolver_id: "erf-fake-resolver".to_string(),
        resolver_version: env!("CARGO_PKG_VERSION").to_string(),
        modules,
        unresolved_facts: unresolved,
        descriptor_sha256: vec![],
    };
    descriptor.descriptor_sha256 = descriptor_digest(&descriptor);
    Ok(descriptor)
}

/// sha256 over the canonical descriptor encoding with the digest field blank.
pub fn descriptor_digest(descriptor: &wb::WorkspaceDescriptor) -> Vec<u8> {
    let mut blank = descriptor.clone();
    blank.descriptor_sha256 = vec![];
    Sha256::digest(blank.encode_to_vec()).to_vec()
}

fn collect_files(
    repo_root: &std::path::Path,
    dir: &std::path::Path,
    files: &mut Vec<wb::FileEntry>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(repo_root, &path, files)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("java") {
            let bytes = std::fs::read(&path)?;
            let relative = path
                .strip_prefix(repo_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            files.push(wb::FileEntry {
                path: relative,
                blob_sha256: Sha256::digest(&bytes).to_vec(),
            });
        }
    }
    Ok(())
}
