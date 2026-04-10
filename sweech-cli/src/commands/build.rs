use crate::{manifest::Manifest, scanner, validator};
use anyhow::{Result, bail};
use colored::Colorize;
use std::process::Command;

pub fn run(mode: String) -> Result<()> {
    let root = Manifest::find_root()?;
    let manifest = Manifest::load(&root)?;
    let project = scanner::scan(&root)?;

    println!("{}", "sweech build".bold());
    println!(
        "  {} {}  mode: {}",
        "project".dimmed(),
        manifest.project.name.cyan().bold(),
        mode.green()
    );
    println!();

    // Pre-flight check
    let issues = validator::validate(&manifest, &project);
    if validator::has_errors(&issues) {
        for issue in &issues {
            if issue.severity == validator::Severity::Error {
                println!(
                    "  {} [{}] {}",
                    "error".red().bold(),
                    issue.code.red(),
                    issue.message
                );
            }
        }
        println!();
        bail!("Build aborted — fix errors above first. Run `sweech check` for details.");
    }
    for issue in &issues {
        if issue.severity == validator::Severity::Warning {
            println!(
                "  {} [{}] {}",
                "warn".yellow().bold(),
                issue.code.yellow(),
                issue.message
            );
        }
    }

    match mode.as_str() {
        "monolith" => build_monolith(&root, &manifest),
        other => bail!(
            "Unknown build mode '{}'. Currently supported: monolith",
            other
        ),
    }
}

fn build_monolith(root: &std::path::Path, manifest: &Manifest) -> Result<()> {
    println!("{}", "Building backend (cargo build --release)...".bold());
    println!();

    let status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(root)
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to invoke cargo: {}", e))?;

    if !status.success() {
        bail!("cargo build --release failed.");
    }

    println!();
    println!("  {} Backend build successful.", "✓".green().bold());

    // Build frontends if any
    for fe in &manifest.frontends {
        let build_cmd = fe
            .build_command
            .clone()
            .unwrap_or_else(|| fe.framework.default_build_command().to_string());

        println!();
        println!(
            "  {} Building frontend {}  {}",
            "▸".green(),
            fe.name.cyan().bold(),
            build_cmd.dimmed()
        );

        let fe_path = root.join(&fe.path);
        if !fe_path.exists() {
            println!(
                "  {} Frontend path '{}' not found — skipping.",
                "⚠".yellow(),
                fe_path.display()
            );
            continue;
        }

        let parts: Vec<&str> = build_cmd.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let status = Command::new(parts[0])
            .args(&parts[1..])
            .current_dir(&fe_path)
            .status()
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to run build command for frontend '{}': {}",
                    fe.name,
                    e
                )
            })?;

        if !status.success() {
            bail!(
                "Frontend build for '{}' failed (command: {})",
                fe.name,
                build_cmd
            );
        }

        println!(
            "  {} Frontend '{}' build successful.",
            "✓".green().bold(),
            fe.name.cyan()
        );
    }

    println!();
    println!("{}", "Build complete.".green().bold());
    println!();
    println!(
        "  Binary: {}",
        format!("target/release/{}", manifest.project.name).cyan()
    );
    println!();
    println!("{}", "  Next steps:".dimmed());
    println!(
        "{}",
        "    sweech generate dockerfile   → generate Dockerfile".dimmed()
    );
    println!(
        "{}",
        "    sweech generate compose      → generate docker-compose.yml".dimmed()
    );

    Ok(())
}
