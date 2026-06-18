use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct ModuleInfo {
    name: String,
    items: Vec<String>,
    submodules: Vec<String>,
    uses: Vec<String>,
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
    
    // Create output directory
    fs::create_dir_all(&output_dir).context("Failed to create output-md directory")?;

    // Run cargo arc to generate SVG
    println!("Running cargo arc -o deps.svg...");
    let arc_result = std::process::Command::new("cargo")
        .args(["arc", "-o", "deps.svg"])
        .current_dir(project_path)
        .status();
    
    match arc_result {
        Ok(status) if status.success() => println!("✓ Generated deps.svg"),
        _ => eprintln!("⚠ cargo arc failed or not installed, continuing with metadata analysis..."),
    }

    // Analyze workspace
    let manifest_path = project_path.join("Cargo.toml");
    let metadata = MetadataCommand::new()
        .manifest_path(&manifest_path)
        .exec()
        .context("Failed to run cargo metadata")?;

    // Check if it's a workspace
    let is_workspace = metadata.workspace_members.len() > 1 || metadata.workspace_packages().len() > 1;
    
    if is_workspace {
        println!("Detected workspace with {} members", metadata.workspace_packages().len());
        process_workspace(project_path, &metadata, &output_dir)?;
    } else {
        println!("Detected single crate project");
        process_single_crate(project_path, &metadata, &output_dir)?;
    }

    println!("\n✓ Documentation generated successfully in output-md/");
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
    // Get workspace members
    let workspace_members: Vec<(String, PathBuf)> = metadata
        .workspace_packages()
        .iter()
        .map(|p| (p.name.to_string(), PathBuf::from(p.manifest_path.parent().unwrap().as_std_path())))
        .collect();

    // Build dependency map
    let dependency_map = build_dependency_map(metadata);

    // Get all crates
    let all_crates: Vec<String> = workspace_members.iter()
        .map(|(name, _)| normalize_crate_name(name))
        .collect();

    // Generate ARCHITECTURE_DIAGRAMS.md
    generate_architecture_diagrams(project_path, workspace_members.as_slice(), &dependency_map, &all_crates)?;

    // Generate README.md
    generate_readme(project_path, workspace_members.as_slice())?;

    // Generate individual crate markdown files
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
    let workspace_members = vec![(pkg.name.to_string(), PathBuf::from(pkg.manifest_path.parent().unwrap().as_std_path()))];
    let dependency_map = HashMap::new();
    let all_crates = vec![normalize_crate_name(pkg.name.as_str())];

    generate_architecture_diagrams(project_path, workspace_members.as_slice(), &dependency_map, &all_crates)?;
    generate_readme(project_path, workspace_members.as_slice())?;
    generate_crate_markdown(output_dir, pkg.name.as_str(), pkg.manifest_path.parent().unwrap().as_std_path(), &dependency_map)?;

    Ok(())
}

fn build_dependency_map(metadata: &cargo_metadata::Metadata) -> HashMap<String, Vec<String>> {
    let mut dependency_map: HashMap<String, Vec<String>> = HashMap::new();
    let workspace_names: HashSet<String> = metadata.workspace_packages()
        .iter()
        .map(|p| normalize_crate_name(p.name.as_str()))
        .collect();

    for pkg in &metadata.workspace_packages() {
        let normalized_name = normalize_crate_name(pkg.name.as_str());
        let mut deps = Vec::new();
        
        if let Some(resolve) = &metadata.resolve {
            for node in &resolve.nodes {
                if node.id.repr.contains(pkg.name.as_str()) {
                    for dep in &node.deps {
                        let dep_normalized = normalize_crate_name(dep.name.as_str());
                        if workspace_names.contains(&dep_normalized) {
                            deps.push(dep_normalized);
                        }
                    }
                }
            }
        }
        dependency_map.insert(normalized_name, deps);
    }

    dependency_map
}

fn generate_architecture_diagrams(
    project_path: &Path,
    workspace_members: &[(String, PathBuf)],
    dependency_map: &HashMap<String, Vec<String>>,
    all_crates: &[String],
) -> Result<()> {
    let output_path = project_path.join("ARCHITECTURE_DIAGRAMS.md");
    
    let mut content = String::new();
    
    // YAML Frontmatter
    content.push_str("# Architecture Diagrams\n\n");
    content.push_str("AI-optimized documentation for workspace dependency visualization.\n\n");
    content.push_str("---\n");
    content.push_str("```yaml\n");
    content.push_str("crates:\n");
    
    for (name, path) in workspace_members {
        let relative_path = path.strip_prefix(project_path).unwrap_or(path);
        let crate_type = if path.join("src/main.rs").exists() { "binary" } else { "library" };
        content.push_str(&format!("  - name: {}\n", name));
        content.push_str(&format!("    path: {}\n", relative_path.display()));
        content.push_str(&format!("    type: {}\n", crate_type));
    }
    
    content.push_str("\ndependencies:\n");
    for (crate_name, deps) in dependency_map {
        for dep in deps {
            content.push_str(&format!("  - {} -> {}\n", crate_name, dep));
        }
    }
    
    content.push_str("```\n");
    content.push_str("---\n\n");
    
    // Flowchart Diagram
    content.push_str("## Flowchart Diagram\n\n");
    content.push_str("```mermaid\n");
    content.push_str("flowchart TD\n");
    content.push_str("    %% Workspace structure\n");
    
    for crate_name in all_crates {
        content.push_str(&format!("    {}[\"{}\"]\n", crate_name, crate_name.replace('_', "-")));
    }
    
    content.push_str("\n    %% Dependencies\n");
    for (crate_name, deps) in dependency_map {
        for dep in deps {
            content.push_str(&format!("    {} --> {}\n", crate_name, dep));
        }
    }
    
    content.push_str("```\n\n");
    
    // Sequence Diagram
    content.push_str("## Sequence Diagram\n\n");
    content.push_str("```mermaid\n");
    content.push_str("sequenceDiagram\n");
    content.push_str("    %% Runtime flow between components\n");
    content.push_str("    participant User\n");
    
    for (i, crate_name) in all_crates.iter().take(5).enumerate() {
        content.push_str(&format!("    participant C{} as {}\n", i, crate_name.replace('_', "-")));
    }
    
    if !all_crates.is_empty() {
        content.push_str(&format!("    User->>C0: Initialize\n"));
        for i in 0..all_crates.len().min(4) {
            content.push_str(&format!("    C{}->>C{}: Request/Response\n", i, i + 1));
        }
    }
    
    content.push_str("```\n\n");
    
    // Module Relationships section
    content.push_str("## Module Relationships\n\n");
    content.push_str("Detailed module relationships are documented in individual crate files in the `output-md/` directory.\n\n");
    
    for (name, _) in workspace_members {
        let normalized = normalize_crate_name(name);
        content.push_str(&format!("- [{}]({}.md)\n", name, normalized));
    }
    
    content.push_str("\n");
    
    // Summary
    content.push_str("## Summary and Key Insights\n\n");
    content.push_str(&format!("This workspace contains {} crates.\n\n", workspace_members.len()));
    content.push_str("### Architecture\n");
    content.push_str("- Modular workspace with clear separation of concerns\n");
    content.push_str("- Dependency graph shows inter-crate relationships\n");
    content.push_str("- Each crate has dedicated documentation in output-md/\n\n");
    content.push_str("### AI-Readability Features\n");
    content.push_str("- Fully qualified node IDs (e.g., `crate_name`) to avoid collisions\n");
    content.push_str("- Explicit dependency pairs in YAML metadata for programmatic parsing\n");
    content.push_str("- Structured crate metadata with types and paths\n");
    content.push_str("- Mermaid comments explaining component purposes\n");
    
    fs::write(&output_path, content).context("Failed to write ARCHITECTURE_DIAGRAMS.md")?;
    println!("✓ Generated: {}", output_path.display());
    
    Ok(())
}

fn generate_readme(
    project_path: &Path,
    workspace_members: &[(String, PathBuf)],
) -> Result<()> {
    let output_path = project_path.join("README.md");
    
    let mut content = String::new();
    content.push_str("# Workspace Documentation\n\n");
    content.push_str("This directory contains AI-optimized documentation for the workspace.\n\n");
    content.push_str("## Generated Files\n\n");
    content.push_str("- `ARCHITECTURE_DIAGRAMS.md`: Complete architecture documentation with Mermaid diagrams\n");
    content.push_str("- `deps.svg`: Interactive SVG dependency visualization (generated by cargo-arc)\n");
    content.push_str("- `output-md/`: Individual crate documentation\n\n");
    content.push_str("## Workspace Members\n\n");
    
    for (name, path) in workspace_members {
        let relative_path = path.strip_prefix(project_path).unwrap_or(path);
        content.push_str(&format!("- **{}**: `{}`\n", name, relative_path.display()));
    }
    
    content.push_str("\n## Regenerating Documentation\n\n");
    content.push_str("Run the generation script:\n");
    content.push_str("- Bash: `./generate-docs.sh`\n");
    content.push_str("- PowerShell: `./generate-docs.ps1`\n");
    
    fs::write(&output_path, content).context("Failed to write README.md")?;
    println!("✓ Generated: {}", output_path.display());
    
    Ok(())
}

fn generate_crate_markdown(
    output_dir: &Path,
    crate_name: &str,
    crate_path: &Path,
    dependency_map: &HashMap<String, Vec<String>>,
) -> Result<()> {
    let normalized_name = normalize_crate_name(crate_name);
    let output_path = output_dir.join(format!("{}.md", normalized_name));
    
    // Parse Cargo.toml for metadata
    let cargo_toml_path = crate_path.join("Cargo.toml");
    let (version, description, deps_list) = if cargo_toml_path.exists() {
        parse_cargo_toml(&cargo_toml_path)?
    } else {
        ("0.0.0".to_string(), "Unknown".to_string(), Vec::new())
    };
    
    // Determine crate type
    let crate_type = if crate_path.join("src/main.rs").exists() {
        "binary"
    } else if crate_path.join("src/lib.rs").exists() {
        "library"
    } else {
        "unknown"
    };
    
    // Parse source files for module structure
    let modules = parse_crate_modules(crate_path)?;
    
    // Get workspace dependencies
    let workspace_deps = dependency_map.get(&normalized_name).cloned().unwrap_or_default();
    
    // Get dependents
    let dependents: Vec<String> = dependency_map
        .iter()
        .filter(|(_, deps)| deps.contains(&normalized_name))
        .map(|(name, _)| name.clone())
        .collect();
    
    let crate_info = CrateInfo {
        name: crate_name.to_string(),
        version,
        description,
        crate_type: crate_type.to_string(),
        dependencies: workspace_deps.clone(),
        dependents,
        modules,
    };
    
    let mut content = String::new();
    
    // Title and description
    content.push_str(&format!("# {}\n\n", crate_name));
    content.push_str(&format!("{}\n\n", crate_info.description));
    
    // YAML Frontmatter
    content.push_str("---\n");
    content.push_str("```yaml\n");
    content.push_str(&format!("crate:\n"));
    content.push_str(&format!("  name: {}\n", crate_name));
    let relative_path = crate_path.file_name().unwrap_or_else(|| crate_path.as_os_str());
    content.push_str(&format!("  path: {}\n", relative_path.to_string_lossy()));
    content.push_str(&format!("  version: {}\n", crate_info.version));
    content.push_str(&format!("  type: {}\n", crate_info.crate_type));
    content.push_str(&format!("  description: {}\n", crate_info.description));
    
    if !deps_list.is_empty() {
        content.push_str("\ndependencies:\n");
        for dep in &deps_list {
            content.push_str(&format!("  - {}\n", dep));
        }
    }
    
    if !workspace_deps.is_empty() {
        content.push_str("\nworkspace_dependencies:\n");
        for dep in &workspace_deps {
            content.push_str(&format!("  - {}\n", dep.replace('_', "-")));
        }
    }
    
    if !crate_info.dependents.is_empty() {
        content.push_str("\ndependents:\n");
        for dep in &crate_info.dependents {
            content.push_str(&format!("  - {}\n", dep.replace('_', "-")));
        }
    }
    
    content.push_str("```\n");
    content.push_str("---\n\n");
    
    // Flowchart Diagram
    content.push_str("## Flowchart Diagram\n\n");
    content.push_str("```mermaid\n");
    content.push_str("flowchart TD\n");
    content.push_str(&format!("    subgraph {}[\"{}\"]\n", normalized_name, crate_name));
    
    for module in &crate_info.modules {
        let module_id = format!("{}__{}", normalized_name, normalize_crate_name(&module.name));
        content.push_str(&format!("        {}[\"{}\"]\n", module_id, module.name));
    }
    
    content.push_str("    end\n\n");
    
    if !workspace_deps.is_empty() {
        content.push_str("    subgraph dependencies[\"Dependencies\"]\n");
        for dep in &workspace_deps {
            content.push_str(&format!("        {}[\"{}\"]\n", dep, dep.replace('_', "-")));
        }
        content.push_str("    end\n\n");
        
        for dep in &workspace_deps {
            content.push_str(&format!("    {} --> {}\n", normalized_name, dep));
        }
    }
    
    if !crate_info.dependents.is_empty() {
        content.push_str("\n    subgraph dependents[\"Dependents\"]\n");
        for dep in &crate_info.dependents {
            content.push_str(&format!("        {}[\"{}\"]\n", dep, dep.replace('_', "-")));
        }
        content.push_str("    end\n\n");
        
        for dep in &crate_info.dependents {
            content.push_str(&format!("    {} --> {}\n", dep, normalized_name));
        }
    }
    
    content.push_str("```\n\n");
    
    // Sequence Diagram
    content.push_str("## Sequence Diagram\n\n");
    content.push_str("```mermaid\n");
    content.push_str("sequenceDiagram\n");
    
    let participants = generate_sequence_participants(&crate_info);
    for (i, participant) in participants.iter().enumerate() {
        content.push_str(&format!("    participant P{} as {}\n", i, participant));
    }
    
    content.push_str("\n");
    let sequence_flow = generate_sequence_flow(&crate_info);
    content.push_str(&sequence_flow);
    
    content.push_str("```\n\n");
    
    // Summary and Key Insights
    content.push_str("## Summary and Key Insights\n\n");
    content.push_str("### Purpose\n");
    content.push_str(&format!("{}\n\n", crate_info.description));
    
    content.push_str("### Key Components\n");
    if !crate_info.modules.is_empty() {
        for module in &crate_info.modules {
            content.push_str(&format!("- **{}", module.name));
            if !module.items.is_empty() {
                content.push_str(&format!(": Contains {} items", module.items.len()));
            }
            content.push_str("\n");
        }
    } else {
        content.push_str("- No module structure detected\n");
    }
    content.push_str("\n");
    
    content.push_str("### Dependency Role\n");
    if workspace_deps.is_empty() && crate_info.dependents.is_empty() {
        content.push_str("Standalone crate with no workspace dependencies or dependents.\n");
    } else if workspace_deps.is_empty() {
        content.push_str(&format!("Leaf node - depended upon by {} other crates but has no workspace dependencies.\n", crate_info.dependents.len()));
    } else if crate_info.dependents.is_empty() {
        content.push_str(&format!("Leaf node - depends on {} other workspace crates but nothing depends on it.\n", workspace_deps.len()));
    } else {
        content.push_str(&format!("Intermediate node - depends on {} crates and is depended upon by {} crates.\n", workspace_deps.len(), crate_info.dependents.len()));
    }
    
    fs::write(&output_path, content).context("Failed to write crate markdown")?;
    println!("✓ Generated: {}", output_path.display());
    
    Ok(())
}

fn parse_cargo_toml(path: &Path) -> Result<(String, String, Vec<String>)> {
    let content = fs::read_to_string(path).context("Failed to read Cargo.toml")?;
    
    let mut version = "0.0.0".to_string();
    let mut description = "Unknown".to_string();
    let mut dependencies = Vec::new();
    
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("version =") {
            version = line.split('=').nth(1).unwrap_or("\"0.0.0\"").trim().trim_matches('"').to_string();
        } else if line.starts_with("description =") {
            description = line.split('=').nth(1).unwrap_or("\"Unknown\"").trim().trim_matches('"').to_string();
        }
    }
    
    Ok((version, description, dependencies))
}

fn parse_crate_modules(crate_path: &Path) -> Result<Vec<ModuleInfo>> {
    let mut modules = Vec::new();
    let src_path = crate_path.join("src");
    
    if !src_path.exists() {
        return Ok(modules);
    }
    
    // Parse main source files
    let main_rs = src_path.join("main.rs");
    let lib_rs = src_path.join("lib.rs");
    
    if main_rs.exists() {
        if let Ok(module) = parse_rust_file(&main_rs, "main") {
            modules.push(module);
        }
    }
    
    if lib_rs.exists() {
        if let Ok(module) = parse_rust_file(&lib_rs, "lib") {
            modules.push(module);
        }
    }
    
    // Parse other .rs files as modules
    if let Ok(entries) = fs::read_dir(&src_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "rs") {
                let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
                if file_name != "main" && file_name != "lib" && file_name != "mod" {
                    if let Ok(module) = parse_rust_file(&path, &file_name) {
                        modules.push(module);
                    }
                }
            }
        }
    }
    
    Ok(modules)
}

fn parse_rust_file(path: &Path, module_name: &str) -> Result<ModuleInfo> {
    let content = fs::read_to_string(path).context("Failed to read Rust file")?;
    
    let mut items = Vec::new();
    let mut submodules = Vec::new();
    let mut uses = Vec::new();
    
    // Simple parsing for common patterns
    for line in content.lines() {
        let line = line.trim();
        
        // Detect pub fn, pub struct, pub enum, pub trait
        if line.starts_with("pub fn ") {
            if let Some(name) = line.strip_prefix("pub fn ") {
                let func_name = name.split('(').next().unwrap_or(name);
                items.push(format!("fn {}", func_name));
            }
        } else if line.starts_with("pub struct ") {
            if let Some(name) = line.strip_prefix("pub struct ") {
                let struct_name = name.split('<').next().unwrap_or(name).split('{').next().unwrap_or(name);
                items.push(format!("struct {}", struct_name.trim()));
            }
        } else if line.starts_with("pub enum ") {
            if let Some(name) = line.strip_prefix("pub enum ") {
                let enum_name = name.split('<').next().unwrap_or(name).split('{').next().unwrap_or(name);
                items.push(format!("enum {}", enum_name.trim()));
            }
        } else if line.starts_with("pub trait ") {
            if let Some(name) = line.strip_prefix("pub trait ") {
                let trait_name = name.split('<').next().unwrap_or(name).split('{').next().unwrap_or(name);
                items.push(format!("trait {}", trait_name.trim()));
            }
        } else if line.starts_with("pub mod ") {
            if let Some(name) = line.strip_prefix("pub mod ") {
                let mod_name = name.split(';').next().unwrap_or(name).split('{').next().unwrap_or(name);
                submodules.push(mod_name.trim().to_string());
            }
        } else if line.starts_with("use ") {
            if let Some(use_path) = line.strip_prefix("use ") {
                let use_clean = use_path.trim_end_matches(';').trim();
                uses.push(use_clean.to_string());
            }
        }
    }
    
    Ok(ModuleInfo {
        name: module_name.to_string(),
        items,
        submodules,
        uses,
    })
}

fn generate_sequence_participants(crate_info: &CrateInfo) -> Vec<String> {
    let mut participants = vec![crate_info.name.clone()];
    
    for dep in &crate_info.dependencies {
        participants.push(dep.replace('_', "-"));
    }
    
    for dep in &crate_info.dependents {
        if !participants.contains(&dep.replace('_', "-")) {
            participants.push(dep.replace('_', "-"));
        }
    }
    
    participants.truncate(6); // Limit to 6 participants
    participants
}

fn generate_sequence_flow(crate_info: &CrateInfo) -> String {
    let mut flow = String::new();
    let participants = generate_sequence_participants(crate_info);
    
    if participants.is_empty() {
        return "    Note over Self: No external dependencies\n".to_string();
    }
    
    let crate_name = &crate_info.name;
    let normalized_name = normalize_crate_name(crate_name);
    
    // Generate context-aware sequence diagrams based on crate patterns
    if normalized_name.contains("agent") || normalized_name.contains("beacon") {
        // Agent/beacon pattern: check-in, task execution, result reporting
        flow.push_str(&format!("    Note over {}: Initial check-in and key exchange\n", crate_name));
        if !crate_info.dependencies.is_empty() {
            flow.push_str(&format!("    {}->>{}: Generate cryptographic keys\n", crate_name, crate_info.dependencies[0].replace('_', "-")));
            flow.push_str(&format!("    {}-->>{}: Key exchange complete\n", crate_info.dependencies[0].replace('_', "-"), crate_name));
        }
        
        flow.push_str(&format!("\n    Note over {}: Encrypted communication\n", crate_name));
        flow.push_str(&format!("    {}->>{}: Send encrypted beacon/check-in\n", crate_name, participants.get(1).unwrap_or(&crate_name.to_string())));
        flow.push_str(&format!("    {}-->>{}: Return tasks (encrypted)\n", participants.get(1).unwrap_or(&crate_name.to_string()), crate_name));
        
        flow.push_str(&format!("\n    loop Task execution\n"));
        flow.push_str(&format!("        {}->>{}: Decrypt and execute task\n", crate_name, crate_name));
        flow.push_str(&format!("        {}->>{}: Encrypt result\n", crate_name, crate_name));
        flow.push_str(&format!("        {}->>{}: Submit result\n", crate_name, participants.get(1).unwrap_or(&crate_name.to_string())));
        flow.push_str(&format!("    end\n"));
        
        flow.push_str(&format!("\n    Note over {}: Memory cleanup\n", crate_name));
        flow.push_str(&format!("    {}->>{}: Zeroize sensitive data\n", crate_name, crate_name));
        
    } else if normalized_name.contains("core") || normalized_name.contains("server") {
        // Core/server pattern: API handling, storage, task management
        flow.push_str(&format!("    Note over {}: API request handling\n", crate_name));
        if !crate_info.dependents.is_empty() {
            flow.push_str(&format!("    {}->>{}: API request\n", crate_info.dependents[0].replace('_', "-"), crate_name));
            flow.push_str(&format!("    {}->>{}: Validate request\n", crate_name, crate_name));
        }
        
        if !crate_info.dependencies.is_empty() {
            flow.push_str(&format!("\n    Note over {}: Storage operations\n", crate_name));
            flow.push_str(&format!("    {}->>{}: Query/Persist data\n", crate_name, crate_info.dependencies[0].replace('_', "-")));
            flow.push_str(&format!("    {}-->>{}: Data result\n", crate_info.dependencies[0].replace('_', "-"), crate_name));
        }
        
        flow.push_str(&format!("\n    Note over {}: Response\n", crate_name));
        if !crate_info.dependents.is_empty() {
            flow.push_str(&format!("    {}-->>{}: API response\n", crate_name, crate_info.dependents[0].replace('_', "-")));
        }
        
    } else if normalized_name.contains("storage") || normalized_name.contains("db") {
        // Storage pattern: cache, persistence, queries
        flow.push_str(&format!("    Note over {}: Cache check\n", crate_name));
        flow.push_str(&format!("    {}->>{}: Check in-memory cache\n", participants.get(1).unwrap_or(&crate_name.to_string()), crate_name));
        
        flow.push_str(&format!("\n    alt Cache hit\n"));
        flow.push_str(&format!("        {}-->>{}: Return cached data\n", crate_name, participants.get(1).unwrap_or(&crate_name.to_string())));
        flow.push_str(&format!("    else Cache miss\n"));
        if !crate_info.dependencies.is_empty() {
            flow.push_str(&format!("        {}->>{}: Query database\n", crate_name, crate_info.dependencies[0].replace('_', "-")));
            flow.push_str(&format!("        {}-->>{}: Database result\n", crate_info.dependencies[0].replace('_', "-"), crate_name));
            flow.push_str(&format!("        {}->>{}: Update cache\n", crate_name, crate_name));
        }
        flow.push_str(&format!("        {}-->>{}: Return data\n", crate_name, participants.get(1).unwrap_or(&crate_name.to_string())));
        flow.push_str(&format!("    end\n"));
        
    } else if normalized_name.contains("forge") || normalized_name.contains("generator") {
        // Generator pattern: configuration, generation, output
        flow.push_str(&format!("    Note over {}: Configuration\n", crate_name));
        if !crate_info.dependents.is_empty() {
            flow.push_str(&format!("    {}->>{}: Provide generation config\n", crate_info.dependents[0].replace('_', "-"), crate_name));
        }
        flow.push_str(&format!("    {}->>{}: Parse and validate config\n", crate_name, crate_name));
        
        if !crate_info.dependencies.is_empty() {
            flow.push_str(&format!("\n    Note over {}: Generation process\n", crate_name));
            for dep in crate_info.dependencies.iter().take(2) {
                flow.push_str(&format!("    {}->>{}: Request generation component\n", crate_name, dep.replace('_', "-")));
                flow.push_str(&format!("    {}-->>{}: Component ready\n", dep.replace('_', "-"), crate_name));
            }
        }
        
        flow.push_str(&format!("\n    Note over {}: Final output\n", crate_name));
        flow.push_str(&format!("    {}->>{}: Assemble final artifact\n", crate_name, crate_name));
        if !crate_info.dependents.is_empty() {
            flow.push_str(&format!("    {}-->>{}: Return generated artifact\n", crate_name, crate_info.dependents[0].replace('_', "-")));
        }
        
    } else if normalized_name.contains("crypto") || normalized_name.contains("encryption") {
        // Crypto pattern: key generation, encryption, decryption
        flow.push_str(&format!("    Note over {}: Key operations\n", crate_name));
        flow.push_str(&format!("    {}->>{}: Generate keypair\n", crate_name, crate_name));
        flow.push_str(&format!("    {}->>{}: Derive shared secret\n", crate_name, crate_name));
        
        flow.push_str(&format!("\n    Note over {}: Encryption\n", crate_name));
        if !crate_info.dependents.is_empty() {
            flow.push_str(&format!("    {}->>{}: Request encryption\n", crate_info.dependents[0].replace('_', "-"), crate_name));
        }
        flow.push_str(&format!("    {}->>{}: Encrypt data (ChaCha20Poly1305/AES-GCM)\n", crate_name, crate_name));
        if !crate_info.dependents.is_empty() {
            flow.push_str(&format!("    {}-->>{}: Encrypted data\n", crate_name, crate_info.dependents[0].replace('_', "-")));
        }
        
        flow.push_str(&format!("\n    Note over {}: Decryption\n", crate_name));
        if !crate_info.dependents.is_empty() {
            flow.push_str(&format!("    {}->>{}: Request decryption\n", crate_info.dependents[0].replace('_', "-"), crate_name));
        }
        flow.push_str(&format!("    {}->>{}: Decrypt data\n", crate_name, crate_name));
        if !crate_info.dependents.is_empty() {
            flow.push_str(&format!("    {}-->>{}: Decrypted data\n", crate_name, crate_info.dependents[0].replace('_', "-")));
        }
        
        flow.push_str(&format!("\n    Note over {}: Memory safety\n", crate_name));
        flow.push_str(&format!("    {}->>{}: Zeroize sensitive data\n", crate_name, crate_name));
        
    } else if normalized_name.contains("operator") || normalized_name.contains("client") {
        // Operator/client pattern: UI interaction, API calls, WebSocket
        flow.push_str(&format!("    Note over {}: User interaction\n", crate_name));
        flow.push_str(&format!("    {}->>{}: User action/input\n", "User", crate_name));
        
        if !crate_info.dependencies.is_empty() {
            flow.push_str(&format!("\n    Note over {}: API communication\n", crate_name));
            flow.push_str(&format!("    {}->>{}: API request\n", crate_name, crate_info.dependencies[0].replace('_', "-")));
            flow.push_str(&format!("    {}-->>{}: API response\n", crate_info.dependencies[0].replace('_', "-"), crate_name));
        }
        
        flow.push_str(&format!("\n    Note over {}: Real-time updates\n", crate_name));
        flow.push_str(&format!("    {}->>{}: WebSocket connect\n", crate_name, crate_info.dependencies.get(0).unwrap_or(&crate_name.to_string()).replace('_', "-")));
        flow.push_str(&format!("    {}-->>{}: Live data stream\n", crate_info.dependencies.get(0).unwrap_or(&crate_name.to_string()).replace('_', "-"), crate_name));
        
        flow.push_str(&format!("\n    Note over {}: UI update\n", crate_name));
        flow.push_str(&format!("    {}->>{}: Update UI state\n", crate_name, "User"));
        
    } else {
        // Generic pattern
        if !crate_info.dependencies.is_empty() {
            flow.push_str(&format!("    Note over {}, {}: Dependency initialization\n", crate_name, crate_info.dependencies[0].replace('_', "-")));
            flow.push_str(&format!("    {}->>{}: Request/Invoke\n", crate_name, crate_info.dependencies[0].replace('_', "-")));
            flow.push_str(&format!("    {}-->>{}: Response/Return\n", crate_info.dependencies[0].replace('_', "-"), crate_name));
        }
        
        if !crate_info.dependents.is_empty() {
            flow.push_str(&format!("\n    Note over {}, {}: Client interaction\n", crate_info.dependents[0].replace('_', "-"), crate_name));
            flow.push_str(&format!("    {}->>{}: API call/Request\n", crate_info.dependents[0].replace('_', "-"), crate_name));
            flow.push_str(&format!("    {}-->>{}: Response\n", crate_name, crate_info.dependents[0].replace('_', "-")));
        }
        
        if crate_info.dependencies.is_empty() && crate_info.dependents.is_empty() {
            flow.push_str(&format!("    Note over {}: Internal operations\n", crate_name));
            flow.push_str(&format!("    {}->>{}: Process data\n", crate_name, crate_name));
        }
    }
    
    flow
}
