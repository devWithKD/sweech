use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ─── What is this file? ───────────────────────────────────────────────────────
//
// The sweech.manifest.toml schema, fully defined from day 1.
// Sections that aren't acted on yet are parsed and stored — they won't
// cause errors, they just won't do anything until we implement them.
//
// This means the ERP project can write a complete manifest today
// and it will stay valid as Sweech grows.
//
// ─── Rust concept: #[serde(default)] ─────────────────────────────────────────
//
// When deserializing TOML, if a field is missing from the file,
// `#[serde(default)]` fills it in with Default::default().
// For Option<T> that's None. For Vec<T> that's []. For bool that's false.
// This makes every field in the manifest optional from the user's perspective —
// they only declare what they need to configure.
//
// ─── Rust concept: #[serde(rename_all = "snake_case")] ───────────────────────
//
// TOML uses snake_case keys. Rust structs use snake_case too, so this
// is actually a no-op here — but it's explicit and documents intent.

/// The root manifest — parsed from sweech.manifest.toml
#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest {
    pub project: ProjectConfig,

    pub build: BuildConfig,

    /// One entry per applet directory
    #[serde(default, rename = "applet")]
    pub applets: Vec<AppletManifest>,

    /// One entry per frontend (web, mobile)
    #[serde(default, rename = "frontend")]
    pub frontends: Vec<FrontendManifest>,

    /// Shared packages (types, utils)
    #[serde(default, rename = "package")]
    pub packages: Vec<PackageManifest>,

    #[serde(default)]
    pub http: HttpConfig,

    #[serde(default)]
    pub plugins: PluginsConfig,

    // ── Placeholders ──────────────────────────────────────────────────────
    //
    // These are parsed and stored but not yet acted on.
    // They're here so the manifest schema is stable — users can write them
    // today and they'll work when we implement them.
    /// Future: arbitrary task orchestration (test, lint, etc.)
    #[serde(default)]
    pub tasks: std::collections::HashMap<String, TaskConfig>,

    /// Future: per-workspace environment variable management
    #[serde(default)]
    pub env: EnvConfig,
}

// ─── [project] ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub name: String,

    #[serde(default = "default_version")]
    pub version: String,

    #[serde(default)]
    pub description: Option<String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

// ─── [build] ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct BuildConfig {
    pub mode: BuildMode,

    #[serde(default)]
    pub provider: Option<CloudProvider>,

    /// Runtime for the whole project in monolith mode.
    /// Per-applet runtime is only allowed in microservices/serverless.
    #[serde(default = "default_runtime")]
    pub runtime: Runtime,
}

fn default_runtime() -> Runtime {
    Runtime::Rust
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum BuildMode {
    Monolith,
    Microservices,
    Serverless,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum CloudProvider {
    Aws,
    Gcp,
    Vercel,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Rust,
    // TypeScript and Python are placeholders — not yet implemented
    #[serde(rename = "typescript")]
    TypeScript,
    Python,
}

// ─── [[applet]] ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct AppletManifest {
    pub name: String,
    pub path: String,

    /// Default auth for all handlers in this applet.
    /// Handlers that don't declare auth() inherit this.
    #[serde(default = "default_auth")]
    pub auth: AuthSetting,

    /// Default guards for all handlers in this applet.
    #[serde(default)]
    pub guards: Vec<String>,

    /// If true, this applet runs as a persistent container in serverless mode.
    #[serde(default)]
    pub stateful: bool,

    /// Per-applet runtime override — only valid in microservices/serverless mode.
    /// In monolith mode, must match [build].runtime or be omitted.
    #[serde(default)]
    pub runtime: Option<Runtime>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum AuthSetting {
    Required,
    Public,
    Optional,
}

fn default_auth() -> AuthSetting {
    AuthSetting::Required
}

// ─── [[frontend]] ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct FrontendManifest {
    pub name: String,
    pub path: String,
    pub framework: FrontendFramework,
    pub deploy_target: DeployTarget,

    /// How the frontend is served relative to the backend.
    /// Required when deploy_target = "^build" on a web frontend.
    #[serde(default)]
    pub serve: Option<ServeMode>,

    /// API prefix when serve = "embedded". Default: "/api"
    #[serde(default = "default_api_prefix")]
    pub api_prefix: String,
}

fn default_api_prefix() -> String {
    "/api".to_string()
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum FrontendFramework {
    Next,
    Nuxt,
    Vite,
    Sveltekit, // web
    Expo,
    Ionic, // mobile
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum DeployTarget {
    #[serde(rename = "^build")]
    Build, // inherits from [build] mode
    Vercel,
    Eas,
    Capacitor,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum ServeMode {
    Embedded,
    Standalone,
}

// ─── [[package]] ─────────────────────────────────────────────────────────────
// Placeholder — parsed but not yet acted on

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct PackageManifest {
    pub name: String,
    pub path: String,

    /// Future: build order graph
    #[serde(default)]
    pub depends_on: Vec<String>,

    /// Future: custom build command
    #[serde(default)]
    pub built_by: Option<String>,
}

// ─── [http] ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct HttpConfig {
    #[serde(default)]
    pub cors: CorsConfig,

    /// Request timeout in milliseconds
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

fn default_timeout() -> u64 {
    30_000
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            cors: CorsConfig::default(),
            timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct CorsConfig {
    #[serde(default)]
    pub origins: Vec<String>,
}

// ─── [plugins] ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct PluginsConfig {
    #[serde(default)]
    pub queue: Option<PluginSetting>,

    #[serde(default)]
    pub storage: Option<PluginSetting>,

    #[serde(default)]
    pub cache: Option<PluginSetting>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum PluginSetting {
    Auto,
    Disabled,
}

// ─── [tasks] — placeholder ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct TaskConfig {
    /// Future: command to run for this task
    #[serde(default)]
    pub run: Option<String>,

    /// Future: tasks that must complete before this one
    #[serde(default)]
    pub depends_on: Vec<String>,
}

// ─── [env] — placeholder ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct EnvConfig {
    /// Future: env vars shared across all workspaces
    #[serde(default)]
    pub shared: Vec<String>,

    /// Future: env vars only for the API/backend
    #[serde(default)]
    pub api: Vec<String>,

    /// Future: env vars only for web frontends
    #[serde(default)]
    pub web: Vec<String>,
}

// ─── Parsing ──────────────────────────────────────────────────────────────────

impl Manifest {
    /// Load and parse a sweech.manifest.toml from a project root path.
    pub fn load(project_root: &Path) -> Result<Self> {
        let manifest_path = project_root.join("sweech.manifest.toml");

        // ─── Rust concept: ? operator ─────────────────────────────────────
        //
        // `?` at the end of a Result expression means:
        //   - If Ok(value): unwrap and continue, value is bound
        //   - If Err(e): return Err(e) from the current function immediately
        //
        // `.context("message")` from anyhow wraps the error with extra info
        // so if something fails you know WHERE it failed and WHY.
        // Think of it like: throw new Error("message: " + originalError)

        let content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Could not read {}", manifest_path.display()))?;

        let manifest: Manifest = toml::from_str(&content)
            .with_context(|| format!("Invalid TOML in {}", manifest_path.display()))?;

        Ok(manifest)
    }

    /// Find the project root by walking up from the current directory,
    /// looking for sweech.manifest.toml. Same pattern as Cargo/Git.
    pub fn find_root() -> Result<std::path::PathBuf> {
        let mut dir = std::env::current_dir().context("Could not get current directory")?;

        loop {
            if dir.join("sweech.manifest.toml").exists() {
                return Ok(dir);
            }
            // Move up one level
            // ─── Rust concept: match on Option ───────────────────────────
            // `.parent()` returns Option<&Path> — None if we're at filesystem root
            match dir.parent() {
                Some(parent) => dir = parent.to_path_buf(),
                None => anyhow::bail!(
                    "Could not find sweech.manifest.toml in current or any parent directory.\n\
                     Run `sweech init` to create a new project."
                ),
            }
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml: &str) -> Manifest {
        toml::from_str(toml).expect("failed to parse manifest")
    }

    #[test]
    fn minimal_manifest_parses() {
        let m = parse(
            r#"
            [project]
            name = "myapp"

            [build]
            mode = "monolith"
        "#,
        );

        assert_eq!(m.project.name, "myapp");
        assert_eq!(m.build.mode, BuildMode::Monolith);
        assert_eq!(m.build.runtime, Runtime::Rust); // default
    }

    #[test]
    fn applets_parse_correctly() {
        let m = parse(
            r#"
            [project]
            name = "myapp"

            [build]
            mode = "monolith"

            [[applet]]
            name = "auth"
            path = "auth.applet"
            auth = "public"

            [[applet]]
            name = "products"
            path = "products.applet"
            guards = ["billing:active"]
            stateful = false
        "#,
        );

        assert_eq!(m.applets.len(), 2);
        assert_eq!(m.applets[0].auth, AuthSetting::Public);
        assert_eq!(m.applets[1].guards, vec!["billing:active"]);
    }

    #[test]
    fn placeholders_dont_break_parsing() {
        let m = parse(
            r#"
            [project]
            name = "myapp"

            [build]
            mode = "monolith"

            [[package]]
            name = "types"
            path = "packages/types"
            depends_on = []

            [tasks.test]
            run = "cargo test"

            [tasks.lint]
            run = "cargo clippy"

            [env]
            shared = ["DATABASE_URL"]
            api = ["JWT_SECRET"]
        "#,
        );

        assert_eq!(m.packages[0].name, "types");
        assert_eq!(m.tasks["test"].run.as_deref(), Some("cargo test"));
        assert_eq!(m.env.shared, vec!["DATABASE_URL"]);
    }

    #[test]
    fn http_config_has_defaults() {
        let m = parse(
            r#"
            [project]
            name = "myapp"
            [build]
            mode = "monolith"
        "#,
        );

        assert_eq!(m.http.timeout_ms, 30_000);
        assert_eq!(m.http.cors.origins, Vec::<String>::new());
    }
}
