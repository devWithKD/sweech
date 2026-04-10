use crate::{config as cfg, templates};
use anyhow::{Context, Result, bail};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

pub fn run(name: Option<String>, sweech_path: Option<String>) -> Result<()> {
    let project_name = name.unwrap_or_else(|| "my-sweech-app".to_string());

    if !project_name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        bail!(
            "Project name '{}' contains invalid characters.\n\
             Use lowercase letters, numbers, hyphens, or underscores.",
            project_name
        );
    }

    let target = Path::new(&project_name);

    if target.exists() {
        bail!(
            "Directory '{}' already exists. Remove it or choose a different name.",
            project_name
        );
    }

    // ── Resolve sweech dependency strings ────────────────────────────────────
    //
    // Priority:
    //   1. --path flag          explicit local path (overrides everything)
    //   2. Config file          ~/.config/sweech/config.toml source setting
    //   3. Auto-detect          walk up from binary looking for workspace dirs
    //   4. Fallback placeholder TODO comment, user must fix manually

    let (core_dep, axum_dep, dep_note) = resolve_sweech_deps(sweech_path)?;

    println!("{} {}", "sweech init".bold(), project_name.cyan().bold());
    println!();

    fs::create_dir_all(target)
        .with_context(|| format!("Could not create directory '{}'", project_name))?;

    write_file(
        target.join("sweech.manifest.toml"),
        &templates::manifest_toml(&project_name),
    )?;
    printed("sweech.manifest.toml");

    // Single Cargo.toml at the workspace root — workspace + package combined.
    // No separate app crate. sweech.app.rs sits next to .applet dirs so that
    // #[path = "./hello.applet/route.rs"] resolves correctly.
    write_file(
        target.join("Cargo.toml"),
        &templates::root_cargo_toml(&project_name, &core_dep, &axum_dep),
    )?;
    printed("Cargo.toml");

    write_file(
        target.join("sweech.app.rs"),
        &templates::sweech_app_rs(&project_name),
    )?;
    printed("sweech.app.rs");

    let applet_dir = target.join("hello.applet");
    fs::create_dir_all(&applet_dir)?;
    write_file(applet_dir.join("route.rs"), &templates::hello_route_rs())?;
    printed("hello.applet/route.rs");

    write_file(target.join(".gitignore"), templates::gitignore())?;
    printed(".gitignore");

    println!();
    println!("{}", "Done!".bold());
    println!();
    println!("  {}  {}", "cd".dimmed(), project_name.cyan());
    println!("  {}  {}", "  ".dimmed(), "sweech dev".green().bold());
    println!();
    println!("  {}", dep_note.dimmed());
    println!();

    Ok(())
}

// ── Dependency resolution ─────────────────────────────────────────────────────

fn resolve_sweech_deps(explicit_path: Option<String>) -> Result<(String, String, String)> {
    // 1. --path flag
    if let Some(p) = explicit_path {
        return build_path_deps(&PathBuf::from(&p), &p);
    }

    // 2. Config file
    let config = cfg::load();
    if config.source.r#type != cfg::SourceType::Unset {
        if let Some((core, axum)) = cfg::resolve_dep_strings(&config.source) {
            let note = match config.source.r#type {
                cfg::SourceType::Git => format!(
                    "Source: git ({})",
                    config.source.url.as_deref().unwrap_or("")
                ),
                cfg::SourceType::Path => format!(
                    "Source: path ({})",
                    config.source.path.as_deref().unwrap_or("")
                ),
                cfg::SourceType::Crates => format!(
                    "Source: crates.io ({})",
                    config.source.version.as_deref().unwrap_or("*")
                ),
                cfg::SourceType::Unset => String::new(),
            };
            return Ok((core, axum, note));
        }
    }

    // 3. Auto-detect from binary location
    if let Some(detected) = auto_detect_sweech_root() {
        if let Ok(result) = build_path_deps(&detected, &detected.display().to_string()) {
            return Ok(result);
        }
    }

    // 4. Fallback placeholder
    let core =
        "{ path = \"../sweech/sweech-core\" }  # TODO: run `sweech config set source-url <url>`"
            .to_string();
    let axum =
        "{ path = \"../sweech/sweech-axum\" }  # TODO: run `sweech config set source-url <url>`"
            .to_string();
    let note = "No sweech source configured.\n  \
                Run: sweech config set source-url https://github.com/devWithKD/sweech\n  \
                 or: sweech config set source-path /path/to/sweech"
        .to_string();
    Ok((core, axum, note))
}

fn auto_detect_sweech_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent()?.to_path_buf();
    for _ in 0..4 {
        if dir.join("sweech-core").join("Cargo.toml").exists()
            && dir.join("sweech-axum").join("Cargo.toml").exists()
        {
            return Some(dir);
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => break,
        }
    }
    None
}

fn build_path_deps(base: &Path, display: &str) -> Result<(String, String, String)> {
    let core_path = base.join("sweech-core");
    let axum_path = base.join("sweech-axum");

    if !core_path.join("Cargo.toml").exists() {
        bail!(
            "sweech-core not found at '{}'.\n\
             Make sure the path points to the sweech workspace root\n\
             (the directory containing sweech-core/ and sweech-axum/).",
            core_path.display()
        );
    }
    if !axum_path.join("Cargo.toml").exists() {
        bail!("sweech-axum not found at '{}'.", axum_path.display());
    }

    let core_abs = core_path.canonicalize().unwrap_or(core_path);
    let axum_abs = axum_path.canonicalize().unwrap_or(axum_path);

    let core_dep = format!("{{ path = \"{}\" }}", core_abs.display());
    let axum_dep = format!("{{ path = \"{}\" }}", axum_abs.display());
    let note = format!("Source: path ({})", display);

    Ok((core_dep, axum_dep, note))
}

fn write_file(path: impl AsRef<Path>, content: &str) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn printed(name: &str) {
    println!("  {} {}", "+".green(), name.white());
}
