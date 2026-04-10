// ─── What is this file? ───────────────────────────────────────────────────────
//
// Global sweech config stored at ~/.config/sweech/config.toml
//
// The most important field right now is `source` — how to resolve sweech-core
// and sweech-axum when generating a new project's Cargo.toml.
//
// Source types:
//
//   [source]
//   type = "git"
//   url  = "https://github.com/devWithKD/sweech"
//
//   [source]
//   type = "path"
//   path = "/home/kedar/Github/sweech"
//
//   [source]
//   type = "crates"
//   version = "0.1"          # once published
//
// Priority when resolving deps for `sweech init`:
//   1. --path flag on the command line  (overrides everything)
//   2. Config file source               (whatever was set via `sweech config set`)
//   3. Auto-detect from binary location (walks up from exe looking for workspace)
//   4. Fallback placeholder             (TODO comment in generated Cargo.toml)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ─── Config schema ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct SweechConfig {
    #[serde(default)]
    pub source: SourceConfig,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct SourceConfig {
    /// "git" | "path" | "crates"
    #[serde(default)]
    pub r#type: SourceType,

    /// Used when type = "git"
    #[serde(default)]
    pub url: Option<String>,

    /// Used when type = "path"
    #[serde(default)]
    pub path: Option<String>,

    /// Used when type = "crates"
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    #[default]
    Unset,
    Git,
    Path,
    Crates,
}

// ─── Config file location ─────────────────────────────────────────────────────

pub fn config_path() -> Result<PathBuf> {
    // ~/.config/sweech/config.toml
    let base = dirs_next::config_dir()
        .or_else(|| {
            // Fallback: ~/.sweech/config.toml if dirs_next not available
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".sweech"))
        })
        .context("Could not determine config directory")?;

    Ok(base.join("sweech").join("config.toml"))
}

// ─── Load / save ─────────────────────────────────────────────────────────────

pub fn load() -> SweechConfig {
    // Config is optional — missing file = default config, not an error
    let path = match config_path() {
        Ok(p) => p,
        Err(_) => return SweechConfig::default(),
    };

    if !path.exists() {
        return SweechConfig::default();
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return SweechConfig::default(),
    };

    toml::from_str(&content).unwrap_or_default()
}

pub fn save(config: &SweechConfig) -> Result<()> {
    let path = config_path()?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Could not create config directory: {}", parent.display()))?;
    }

    let content = toml::to_string_pretty(config).context("Failed to serialize config")?;

    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write config to {}", path.display()))?;

    Ok(())
}

// ─── Dep string resolution ────────────────────────────────────────────────────
//
// Returns (core_dep, axum_dep) — the strings that go into Cargo.toml:
//
//   sweech-core = { git = "https://github.com/devWithKD/sweech" }
//   sweech-core = { path = "/home/kedar/Github/sweech/sweech-core" }
//   sweech-core = "0.1"

pub fn resolve_dep_strings(source: &SourceConfig) -> Option<(String, String)> {
    match source.r#type {
        SourceType::Git => {
            let url = source.url.as_ref()?;
            let core = format!("{{ git = \"{url}\", package = \"sweech-core\" }}");
            let axum = format!("{{ git = \"{url}\", package = \"sweech-axum\" }}");
            Some((core, axum))
        }

        SourceType::Path => {
            let base = source.path.as_ref()?;
            let base_path = PathBuf::from(base);
            let core_path = base_path.join("sweech-core");
            let axum_path = base_path.join("sweech-axum");

            // Canonicalize so the path is absolute and clean
            let core_abs = core_path.canonicalize().unwrap_or(core_path);
            let axum_abs = axum_path.canonicalize().unwrap_or(axum_path);

            let core = format!("{{ path = \"{}\" }}", core_abs.display());
            let axum = format!("{{ path = \"{}\" }}", axum_abs.display());
            Some((core, axum))
        }

        SourceType::Crates => {
            let version = source.version.as_deref().unwrap_or("*");
            let core = format!("\"{}\"", version);
            let axum = format!("\"{}\"", version);
            Some((core, axum))
        }

        SourceType::Unset => None,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_source_produces_correct_deps() {
        let source = SourceConfig {
            r#type: SourceType::Git,
            url: Some("https://github.com/devWithKD/sweech".to_string()),
            ..Default::default()
        };
        let (core, axum) = resolve_dep_strings(&source).unwrap();
        assert!(core.contains("git = \"https://github.com/devWithKD/sweech\""));
        assert!(core.contains("package = \"sweech-core\""));
        assert!(axum.contains("package = \"sweech-axum\""));
    }

    #[test]
    fn crates_source_produces_version_string() {
        let source = SourceConfig {
            r#type: SourceType::Crates,
            version: Some("0.2".to_string()),
            ..Default::default()
        };
        let (core, axum) = resolve_dep_strings(&source).unwrap();
        assert_eq!(core, "\"0.2\"");
        assert_eq!(axum, "\"0.2\"");
    }

    #[test]
    fn unset_source_returns_none() {
        let source = SourceConfig::default();
        assert!(resolve_dep_strings(&source).is_none());
    }

    #[test]
    fn config_roundtrips_through_toml() {
        let mut config = SweechConfig::default();
        config.source.r#type = SourceType::Git;
        config.source.url = Some("https://github.com/devWithKD/sweech".to_string());

        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: SweechConfig = toml::from_str(&serialized).unwrap();

        assert_eq!(deserialized.source.r#type, SourceType::Git);
        assert_eq!(
            deserialized.source.url.as_deref(),
            Some("https://github.com/devWithKD/sweech")
        );
    }
}
