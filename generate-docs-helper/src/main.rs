use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct ModuleInfo {
    /// Relative path from src/ (e.g. "builder", "http_api/handlers")
    path: String,
    /// Display name in docs (dir gets trailing /, file is plain)
    display: String,
    /// Counted public items
    item_count: usize,
    /// For directory modules: list of child .rs filenames (without .rs)
    children: Vec<String>,
    /// True if this entry represents a directory
    is_dir: bool,
}

#[derive(Debug, Clone)]
struct CrateInfo {
    name: String,
    version: String,
    description: String,
    crate_type: String,
    dependencies: Vec<String>,
    dependents: Vec<String>,
    modules: Vec<ModuleInfo>,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <project-path>", args[0]);
        std::process::exit(1);
    }

    let project_path = Path::new(&args[1]);
    let output_dir = project_path.join("output-md");
    fs::create_dir_all(&output_dir).context("Failed to create output-md directory")?;

    // Optional: cargo arc for SVG
    let arc_result = std::process::Command::new("cargo")
        .args(["arc", "-o", "deps.svg"])
        .current_dir(project_path)
        .status();
    match arc_result {
        Ok(s) if s.success() => println!("✓ Generated deps.svg"),
        _ => eprintln!("⚠ cargo arc failed, continuing..."),
    }

    let manifest_path = project_path.join("Cargo.toml");
    let metadata = MetadataCommand::new()
        .manifest_path(&manifest_path)
        .exec()
        .context("Failed to run cargo metadata")?;

    let is_workspace = metadata.workspace_members.len() > 1
        || metadata.workspace_packages().len() > 1;

    if is_workspace {
        println!("Workspace: {} members", metadata.workspace_packages().len());
        process_workspace(project_path, &metadata, &output_dir)?;
    } else {
        println!("Single crate");
        process_single_crate(project_path, &metadata, &output_dir)?;
    }

    println!("\n✓ Done — output-md/");
    Ok(())
}

fn normalize_crate_name(name: &str) -> String {
    name.replace('-', "_")
}

fn process_workspace(
    project_path: &Path,
    metadata: &cargo_metadata::Metadata,
    output_dir: &Path,
) -> Result<()> {
    let workspace_members: Vec<(String, PathBuf)> = metadata
        .workspace_packages()
        .iter()
        .map(|p| (
            p.name.to_string(),
            PathBuf::from(p.manifest_path.parent().unwrap().as_std_path()),
        ))
        .collect();

    let dependency_map = build_dependency_map(metadata);
    let all_crates: Vec<String> = workspace_members.iter()
        .map(|(n, _)| normalize_crate_name(n))
        .collect();

    generate_architecture_diagrams(project_path, &workspace_members, &dependency_map, &all_crates)?;
    generate_readme(project_path, &workspace_members)?;

    for (name, path) in &workspace_members {
        generate_crate_markdown(output_dir, name, path, &dependency_map)?;
    }
    Ok(())
}

fn process_single_crate(
    project_path: &Path,
    metadata: &cargo_metadata::Metadata,
    output_dir: &Path,
) -> Result<()> {
    let pkg = &metadata.packages[0];
    let path = PathBuf::from(pkg.manifest_path.parent().unwrap().as_std_path());
    let dependency_map = HashMap::new();
    generate_architecture_diagrams(project_path, &[(pkg.name.to_string(), path.clone())], &dependency_map, &[normalize_crate_name(pkg.name.as_str())])?;
    generate_readme(project_path, &[(pkg.name.to_string(), path.clone())])?;
    generate_crate_markdown(output_dir, pkg.name.as_str(), &path, &dependency_map)?;
    Ok(())
}

fn build_dependency_map(metadata: &cargo_metadata::Metadata) -> HashMap<String, Vec<String>> {
    let workspace_names: HashSet<String> = metadata.workspace_packages()
        .iter()
        .map(|p| normalize_crate_name(p.name.as_str()))
        .collect();

    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for pkg in metadata.workspace_packages() {
        let me = normalize_crate_name(pkg.name.as_str());
        let mut deps = Vec::new();
        if let Some(resolve) = &metadata.resolve {
            for node in &resolve.nodes {
                if node.id.repr.contains(pkg.name.as_str()) {
                    for dep in &node.deps {
                        let d = normalize_crate_name(dep.name.as_str());
                        if workspace_names.contains(&d) {
                            deps.push(d);
                        }
                    }
                }
            }
        }
        map.insert(me, deps);
    }
    map
}

// ── Module parsing ────────────────────────────────────────────────────────────

/// Walk src/ recursively. Each subdirectory becomes a directory module entry.
/// Each top-level .rs file (excluding mod.rs) becomes a flat module entry.
fn parse_crate_modules(crate_path: &Path) -> Result<Vec<ModuleInfo>> {
    let mut modules = Vec::new();

    // Some crates put lib.rs at root (not src/)
    let src = crate_path.join("src");
    let has_src = src.exists();

    let roots = if has_src {
        vec![src.clone()]
    } else {
        vec![crate_path.to_path_buf()]
    };

    let base = roots[0].clone();

    // Top-level .rs files (main.rs, lib.rs, and siblings)
    if let Ok(entries) = fs::read_dir(&base) {
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |e| e == "rs"))
            .collect();
        files.sort();
        for path in files {
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            if stem == "mod" { continue; }
            let count = count_public_items(&path);
            modules.push(ModuleInfo {
                path: stem.clone(),
                display: stem,
                item_count: count,
                children: Vec::new(),
                is_dir: false,
            });
        }
    }

    // Subdirectories → directory modules
    if let Ok(entries) = fs::read_dir(&base) {
        let mut dirs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            let dir_name = dir.file_name().unwrap().to_string_lossy().to_string();
            let (total_items, children) = walk_dir_items(&dir);
            modules.push(ModuleInfo {
                path: format!("{}/", dir_name),
                display: format!("{}/", dir_name),
                item_count: total_items,
                children,
                is_dir: true,
            });
        }
    }

    Ok(modules)
}

/// Recursively count public items and list child .rs filenames under a directory.
fn walk_dir_items(dir: &Path) -> (usize, Vec<String>) {
    let mut total = 0;
    let mut children = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                let (sub_count, _) = walk_dir_items(&path);
                total += sub_count;
                let sub_name = path.file_name().unwrap().to_string_lossy().to_string();
                children.push(format!("{}/", sub_name));
            } else if path.extension().map_or(false, |e| e == "rs") {
                let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                total += count_public_items(&path);
                if stem != "mod" {
                    children.push(stem);
                }
            }
        }
    }
    (total, children)
}

/// Count public API surface: pub fn, pub async fn, pub struct, pub enum,
/// pub trait, pub type, pub const, pub static, pub(crate) variants.
fn count_public_items(path: &Path) -> usize {
    let Ok(content) = fs::read_to_string(path) else { return 0 };
    let mut count = 0;
    for line in content.lines() {
        let t = line.trim();
        // pub fn / pub async fn / pub(crate) fn / pub(super) fn
        if (t.starts_with("pub fn ")
            || t.starts_with("pub async fn ")
            || t.starts_with("pub(crate) fn ")
            || t.starts_with("pub(super) fn ")
            || t.starts_with("pub(crate) async fn "))
            && !t.contains("//")
        {
            count += 1;
        } else if (t.starts_with("pub struct ")
            || t.starts_with("pub(crate) struct "))
            && !t.contains("//")
        {
            count += 1;
        } else if (t.starts_with("pub enum ")
            || t.starts_with("pub(crate) enum "))
            && !t.contains("//")
        {
            count += 1;
        } else if (t.starts_with("pub trait ")
            || t.starts_with("pub(crate) trait "))
            && !t.contains("//")
        {
            count += 1;
        } else if (t.starts_with("pub type ")
            || t.starts_with("pub(crate) type "))
            && !t.contains("//")
        {
            count += 1;
        } else if (t.starts_with("pub const ")
            || t.starts_with("pub(crate) const "))
            && !t.contains("//")
        {
            count += 1;
        } else if (t.starts_with("pub static ")
            || t.starts_with("pub(crate) static "))
            && !t.contains("//")
        {
            count += 1;
        }
    }
    count
}

// ── Markdown generation ───────────────────────────────────────────────────────

fn generate_crate_markdown(
    output_dir: &Path,
    crate_name: &str,
    crate_path: &Path,
    dependency_map: &HashMap<String, Vec<String>>,
) -> Result<()> {
    let normalized_name = normalize_crate_name(crate_name);
    let output_path = output_dir.join(format!("{}.md", normalized_name));

    let cargo_toml = crate_path.join("Cargo.toml");
    let (version, description) = if cargo_toml.exists() {
        parse_cargo_toml(&cargo_toml)?
    } else {
        ("0.0.0".to_string(), String::new())
    };

    let crate_type = detect_crate_type(crate_path);
    let modules = parse_crate_modules(crate_path)?;

    let workspace_deps = dependency_map.get(&normalized_name).cloned().unwrap_or_default();
    let dependents: Vec<String> = dependency_map
        .iter()
        .filter(|(_, deps)| deps.contains(&normalized_name))
        .map(|(n, _)| n.clone())
        .collect();

    let info = CrateInfo {
        name: crate_name.to_string(),
        version,
        description,
        crate_type,
        dependencies: workspace_deps.clone(),
        dependents,
        modules,
    };

    let mut out = String::new();

    // Title
    out.push_str(&format!("# {}\n\n", crate_name));
    out.push_str(&format!("{}\n\n", info.description));

    // YAML frontmatter
    out.push_str("---\n```yaml\ncrate:\n");
    out.push_str(&format!("  name: {}\n", crate_name));
    let rel = crate_path.file_name().unwrap_or(crate_path.as_os_str()).to_string_lossy();
    out.push_str(&format!("  path: {}\n", rel));
    out.push_str(&format!("  version: {}\n", info.version));
    out.push_str(&format!("  type: {}\n", info.crate_type));
    out.push_str(&format!("  description: {}\n", info.description));

    if !workspace_deps.is_empty() {
        out.push_str("\nworkspace_dependencies:\n");
        for d in &workspace_deps {
            out.push_str(&format!("  - {}\n", d.replace('_', "-")));
        }
    }
    if !info.dependents.is_empty() {
        out.push_str("\ndependents:\n");
        for d in &info.dependents {
            out.push_str(&format!("  - {}\n", d.replace('_', "-")));
        }
    }
    out.push_str("```\n---\n\n");

    // Flowchart
    out.push_str("## Flowchart Diagram\n\n```mermaid\nflowchart TD\n");
    out.push_str(&format!("    subgraph {}[\"{}\"]\n", normalized_name, crate_name));
    for m in &info.modules {
        let mid = format!("{}__{}", normalized_name, normalize_crate_name(&m.path));
        out.push_str(&format!("        {}[\"{}\"]\n", mid, m.display));
    }
    out.push_str("    end\n");

    if !workspace_deps.is_empty() {
        out.push_str("\n    subgraph dependencies[\"Dependencies\"]\n");
        for d in &workspace_deps {
            out.push_str(&format!("        {}[\"{}\"]\n", d, d.replace('_', "-")));
        }
        out.push_str("    end\n");
        for d in &workspace_deps {
            out.push_str(&format!("    {} --> {}\n", normalized_name, d));
        }
    }
    if !info.dependents.is_empty() {
        out.push_str("\n    subgraph dependents[\"Dependents\"]\n");
        for d in &info.dependents {
            out.push_str(&format!("        {}[\"{}\"]\n", d, d.replace('_', "-")));
        }
        out.push_str("    end\n");
        for d in &info.dependents {
            out.push_str(&format!("    {} --> {}\n", d, normalized_name));
        }
    }
    out.push_str("```\n\n");

    // Sequence diagram
    out.push_str("## Sequence Diagram\n\n```mermaid\nsequenceDiagram\n");
    let participants = build_participants(&info);
    for (i, p) in participants.iter().enumerate() {
        out.push_str(&format!("    participant P{} as {}\n", i, p));
    }
    out.push('\n');
    out.push_str(&generate_sequence_flow(&info));
    out.push_str("```\n\n");

    // Summary
    out.push_str("## Summary and Key Insights\n\n### Purpose\n");
    out.push_str(&format!("{}\n\n", info.description));

    out.push_str("### Key Components\n");
    if info.modules.is_empty() {
        out.push_str("- No module structure detected\n");
    } else {
        for m in &info.modules {
            if m.is_dir {
                if m.children.is_empty() {
                    out.push_str(&format!("- **{}** ({} items)\n", m.display, m.item_count));
                } else {
                    let child_list = m.children.join(", ");
                    out.push_str(&format!("- **{}** ({} items) — {}\n",
                        m.display, m.item_count, child_list));
                }
            } else if m.item_count > 0 {
                out.push_str(&format!("- **{}** ({} items)\n", m.display, m.item_count));
            } else {
                out.push_str(&format!("- **{}**\n", m.display));
            }
        }
    }

    out.push_str("\n### Dependency Role\n");
    match (workspace_deps.is_empty(), info.dependents.is_empty()) {
        (true, true) => out.push_str("Standalone crate — no workspace dependencies or dependents.\n"),
        (true, false) => out.push_str(&format!(
            "Foundation node — depended upon by {} crates, no workspace dependencies.\n",
            info.dependents.len()
        )),
        (false, true) => out.push_str(&format!(
            "Leaf node — depends on {} workspace crates, nothing depends on it.\n",
            workspace_deps.len()
        )),
        (false, false) => out.push_str(&format!(
            "Intermediate node — depends on {} crates, depended upon by {} crates.\n",
            workspace_deps.len(), info.dependents.len()
        )),
    }

    fs::write(&output_path, out).context("Failed to write crate markdown")?;
    println!("✓ {}", output_path.display());
    Ok(())
}

fn detect_crate_type(crate_path: &Path) -> String {
    if crate_path.join("src/main.rs").exists() {
        "binary".to_string()
    } else if crate_path.join("src/lib.rs").exists() || crate_path.join("lib.rs").exists() {
        "library".to_string()
    } else {
        // Check for [[bin]] in Cargo.toml
        "unknown".to_string()
    }
}

/// Parse description and version from Cargo.toml. Handles:
/// - `description = "..."` (single-line)
/// - `description = """..."""` (multi-line)
/// - workspace inheritance markers
fn parse_cargo_toml(path: &Path) -> Result<(String, String)> {
    let content = fs::read_to_string(path).context("read Cargo.toml")?;
    let mut version = "0.0.0".to_string();
    let mut description = String::new();
    let mut in_package = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_package = false;
        }
        if !in_package { continue; }

        if let Some(rest) = trimmed.strip_prefix("version") {
            let rest = rest.trim_start_matches([' ', '=']).trim();
            if rest.starts_with('"') {
                version = rest.trim_matches('"').to_string();
            }
        }
        if let Some(rest) = trimmed.strip_prefix("description") {
            let rest = rest.trim_start_matches([' ', '=']).trim();
            // Strip quotes and trailing comments
            let d = rest.trim_matches('"').trim_matches('\'');
            if !d.is_empty() && d != "workspace" && d != "true" {
                description = d.to_string();
            }
        }
    }

    Ok((version, description))
}

// ── Sequence diagram helpers ──────────────────────────────────────────────────

fn build_participants(info: &CrateInfo) -> Vec<String> {
    let mut p = vec![info.name.clone()];
    for d in &info.dependencies {
        p.push(d.replace('_', "-"));
    }
    for d in &info.dependents {
        let nd = d.replace('_', "-");
        if !p.contains(&nd) { p.push(nd); }
    }
    p.truncate(6);
    p
}

fn generate_sequence_flow(info: &CrateInfo) -> String {
    let mut f = String::new();
    let n = &info.name;
    let nn = normalize_crate_name(n);
    let deps = &info.dependencies;
    let dependents = &info.dependents;
    let dep0 = deps.first().map(|s| s.replace('_', "-")).unwrap_or_else(|| n.clone());
    let dep_t0 = dependents.first().map(|s| s.replace('_', "-")).unwrap_or_else(|| n.clone());

    if nn.contains("agent") || nn.contains("beacon") || nn.contains("implant") {
        f.push_str(&format!("    Note over {n}: Initial check-in and key exchange\n"));
        if !deps.is_empty() {
            f.push_str(&format!("    {n}->>{dep0}: Generate cryptographic keys\n"));
            f.push_str(&format!("    {dep0}-->>{n}: Key exchange complete\n"));
        }
        f.push_str(&format!("\n    Note over {n}: Encrypted communication\n"));
        f.push_str(&format!("    {n}->>{dep0}: Send encrypted beacon/check-in\n"));
        f.push_str(&format!("    {dep0}-->>{n}: Return tasks (encrypted)\n"));
        f.push_str("\n    loop Task execution\n");
        f.push_str(&format!("        {n}->>{n}: Decrypt and execute task\n"));
        f.push_str(&format!("        {n}->>{dep0}: Submit result\n"));
        f.push_str("    end\n");
    } else if nn.contains("core") || nn.contains("server") || nn.contains("kernel") {
        if !dependents.is_empty() {
            f.push_str(&format!("    Note over {n}: Request handling\n"));
            f.push_str(&format!("    {dep_t0}->>{n}: API request\n"));
            f.push_str(&format!("    {n}->>{n}: Validate + route\n"));
        }
        if !deps.is_empty() {
            f.push_str(&format!("\n    Note over {n}: Storage operations\n"));
            f.push_str(&format!("    {n}->>{dep0}: Query/Persist data\n"));
            f.push_str(&format!("    {dep0}-->>{n}: Data result\n"));
        }
        if !dependents.is_empty() {
            f.push_str(&format!("\n    {n}-->>{dep_t0}: Response\n"));
        }
    } else if nn.contains("storage") || nn.contains("db") || nn.contains("repo") {
        f.push_str(&format!("    Note over {n}: Cache check\n"));
        f.push_str(&format!("    {dep_t0}->>{n}: Query request\n"));
        f.push_str("\n    alt Cache hit\n");
        f.push_str(&format!("        {n}-->>{dep_t0}: Return cached\n"));
        f.push_str("    else Cache miss\n");
        if !deps.is_empty() {
            f.push_str(&format!("        {n}->>{dep0}: SQL query\n"));
            f.push_str(&format!("        {dep0}-->>{n}: Rows\n"));
            f.push_str(&format!("        {n}->>{n}: Update cache\n"));
        }
        f.push_str(&format!("        {n}-->>{dep_t0}: Return data\n"));
        f.push_str("    end\n");
    } else if nn.contains("forge") || nn.contains("generat") || nn.contains("build") {
        if !dependents.is_empty() {
            f.push_str(&format!("    Note over {n}: Configuration\n"));
            f.push_str(&format!("    {dep_t0}->>{n}: Provide config\n"));
        }
        f.push_str(&format!("    {n}->>{n}: Parse and validate\n"));
        if !deps.is_empty() {
            f.push_str(&format!("\n    Note over {n}: Generation\n"));
            for d in deps.iter().take(2) {
                let dr = d.replace('_', "-");
                f.push_str(&format!("    {n}->>{dr}: Request component\n"));
                f.push_str(&format!("    {dr}-->>{n}: Component ready\n"));
            }
        }
        f.push_str(&format!("    {n}->>{n}: Assemble artifact\n"));
        if !dependents.is_empty() {
            f.push_str(&format!("    {n}-->>{dep_t0}: Return artifact\n"));
        }
    } else if nn.contains("operator") || nn.contains("client") || nn.contains("cli") || nn.contains("tui") {
        f.push_str(&format!("    Note over {n}: User interaction\n"));
        f.push_str(&format!("    User->>{n}: User action/input\n"));
        if !deps.is_empty() {
            f.push_str(&format!("\n    Note over {n}: API communication\n"));
            f.push_str(&format!("    {n}->>{dep0}: API request\n"));
            f.push_str(&format!("    {dep0}-->>{n}: API response\n"));
        }
        f.push_str(&format!("\n    {n}->>User: Update UI state\n"));
    } else {
        if !deps.is_empty() {
            f.push_str(&format!("    Note over {n}, {dep0}: Dependency initialization\n"));
            f.push_str(&format!("    {n}->>{dep0}: Request/Invoke\n"));
            f.push_str(&format!("    {dep0}-->>{n}: Response/Return\n"));
        }
        if !dependents.is_empty() {
            f.push_str(&format!("\n    Note over {dep_t0}, {n}: Client interaction\n"));
            f.push_str(&format!("    {dep_t0}->>{n}: API call/Request\n"));
            f.push_str(&format!("    {n}-->>{dep_t0}: Response\n"));
        }
        if deps.is_empty() && dependents.is_empty() {
            f.push_str(&format!("    Note over {n}: Internal operations\n"));
        }
    }
    f
}

// ── Architecture / README (unchanged structure, minor cleanup) ────────────────

fn generate_architecture_diagrams(
    project_path: &Path,
    workspace_members: &[(String, PathBuf)],
    dependency_map: &HashMap<String, Vec<String>>,
    all_crates: &[String],
) -> Result<()> {
    let output_path = project_path.join("ARCHITECTURE_DIAGRAMS.md");
    let mut out = String::new();

    out.push_str("# Architecture Diagrams\n\nAI-optimized dependency visualization.\n\n---\n```yaml\ncrates:\n");
    for (name, path) in workspace_members {
        let rel = path.strip_prefix(project_path).unwrap_or(path);
        let ct = detect_crate_type(path);
        out.push_str(&format!("  - name: {}\n    path: {}\n    type: {}\n", name, rel.display(), ct));
    }
    out.push_str("\ndependencies:\n");
    for (cr, deps) in dependency_map {
        for d in deps { out.push_str(&format!("  - {} -> {}\n", cr, d)); }
    }
    out.push_str("```\n---\n\n## Flowchart Diagram\n\n```mermaid\nflowchart TD\n");
    for c in all_crates {
        out.push_str(&format!("    {}[\"{}\"]\n", c, c.replace('_', "-")));
    }
    out.push_str("\n");
    for (c, deps) in dependency_map {
        for d in deps { out.push_str(&format!("    {} --> {}\n", c, d)); }
    }
    out.push_str("```\n\n## Module Relationships\n\n");
    for (name, _) in workspace_members {
        let norm = normalize_crate_name(name);
        out.push_str(&format!("- [{}]({}.md)\n", name, norm));
    }
    out.push_str(&format!("\n## Summary\n\nWorkspace with {} crates. See `output-md/` for details.\n", workspace_members.len()));

    fs::write(&output_path, out).context("Failed to write ARCHITECTURE_DIAGRAMS.md")?;
    println!("✓ {}", output_path.display());
    Ok(())
}

fn generate_readme(project_path: &Path, workspace_members: &[(String, PathBuf)]) -> Result<()> {
    let output_path = project_path.join("README.md");
    let mut out = String::new();
    out.push_str("# Workspace Documentation\n\n## Generated Files\n\n");
    out.push_str("- `ARCHITECTURE_DIAGRAMS.md` — Mermaid architecture diagrams\n");
    out.push_str("- `deps.svg` — SVG dependency graph (cargo-arc)\n");
    out.push_str("- `output-md/` — Per-crate documentation\n\n## Workspace Members\n\n");
    for (name, path) in workspace_members {
        let rel = path.strip_prefix(project_path).unwrap_or(path);
        out.push_str(&format!("- **{}**: `{}`\n", name, rel.display()));
    }
    out.push_str("\n## Regenerate\n\n- Bash: `./generate-docs.sh`\n- PowerShell: `./generate-docs.ps1`\n");
    fs::write(&output_path, out).context("Failed to write README.md")?;
    println!("✓ {}", output_path.display());
    Ok(())
}
