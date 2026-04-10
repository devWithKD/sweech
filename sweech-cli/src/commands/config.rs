use crate::config::{self as cfg, SourceType};
use anyhow::{Result, bail};
use colored::Colorize;

// ─── sweech config show ───────────────────────────────────────────────────────

pub fn show() -> Result<()> {
    let config = cfg::load();
    let path = cfg::config_path().unwrap_or_default();

    println!("{}", "sweech config".bold());
    println!("  {}", path.display().to_string().dimmed());
    println!();

    match config.source.r#type {
        SourceType::Unset => {
            println!("  {} source   {}", "·".dimmed(), "not set".yellow());
            println!();
            println!(
                "{}",
                "Set a source so `sweech init` knows where to find sweech:".dimmed()
            );
            println!();
            println!(
                "  {}",
                "sweech config set source-url https://github.com/devWithKD/sweech".cyan()
            );
            println!(
                "  {}",
                "sweech config set source-path /path/to/sweech".cyan()
            );
            println!(
                "  {}",
                "sweech config set source-version 0.1  # once on crates.io".cyan()
            );
        }
        SourceType::Git => {
            println!("  {} source   {}", "·".dimmed(), "git".green().bold());
            println!(
                "  {} url      {}",
                "·".dimmed(),
                config.source.url.as_deref().unwrap_or("(not set)").cyan()
            );
        }
        SourceType::Path => {
            println!("  {} source   {}", "·".dimmed(), "path".green().bold());
            println!(
                "  {} path     {}",
                "·".dimmed(),
                config.source.path.as_deref().unwrap_or("(not set)").cyan()
            );
        }
        SourceType::Crates => {
            println!("  {} source   {}", "·".dimmed(), "crates.io".green().bold());
            println!(
                "  {} version  {}",
                "·".dimmed(),
                config.source.version.as_deref().unwrap_or("*").cyan()
            );
        }
    }

    println!();
    Ok(())
}

// ─── sweech config set ────────────────────────────────────────────────────────

pub fn set(key: String, value: String) -> Result<()> {
    let mut config = cfg::load();

    match key.as_str() {
        // git URL — most common for pre-crates.io development
        // sweech config set source-url https://github.com/devWithKD/sweech
        "source-url" | "source.url" => {
            config.source.r#type = SourceType::Git;
            config.source.url = Some(value.clone());
            config.source.path = None;
            config.source.version = None;
        }

        // local path — for contributors / local dev
        // sweech config set source-path /home/kedar/Github/sweech
        "source-path" | "source.path" => {
            let expanded = expand_tilde(&value);
            let path = std::path::PathBuf::from(&expanded);

            // Validate the path actually contains sweech-core and sweech-axum
            if !path.join("sweech-core").join("Cargo.toml").exists() {
                bail!(
                    "sweech-core not found at '{}'.\n\
                     Make sure the path points to the sweech workspace root\n\
                     (the directory containing sweech-core/ and sweech-axum/).",
                    path.display()
                );
            }
            if !path.join("sweech-axum").join("Cargo.toml").exists() {
                bail!("sweech-axum not found at '{}'.", path.display());
            }

            config.source.r#type = SourceType::Path;
            config.source.path = Some(expanded);
            config.source.url = None;
            config.source.version = None;
        }

        // crates.io version — for after publishing
        // sweech config set source-version 0.1
        "source-version" | "source.version" => {
            config.source.r#type = SourceType::Crates;
            config.source.version = Some(value.clone());
            config.source.url = None;
            config.source.path = None;
        }

        other => bail!(
            "Unknown config key '{}'.\n\
             Valid keys: source-url, source-path, source-version",
            other
        ),
    }

    cfg::save(&config)?;

    let path = cfg::config_path().unwrap_or_default();
    println!("{}", "sweech config".bold());
    println!("  {} {} = {}", "✓".green(), key.cyan(), value.white());
    println!("  {}", path.display().to_string().dimmed());
    println!();

    Ok(())
}

// ─── sweech config unset ─────────────────────────────────────────────────────

pub fn unset(key: String) -> Result<()> {
    let mut config = cfg::load();

    match key.as_str() {
        "source-url" | "source.url" | "source-path" | "source.path" | "source-version"
        | "source.version" | "source" => {
            config.source = cfg::SourceConfig::default();
        }
        other => bail!("Unknown config key '{}'.", other),
    }

    cfg::save(&config)?;

    println!("{} {} unset", "✓".green(), key.cyan());
    println!();
    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}/{}", home, &path[2..]);
        }
    }
    path.to_string()
}
