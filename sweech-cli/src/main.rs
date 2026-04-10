mod commands;
mod config;
mod manifest;
mod scanner;
mod templates;
mod type_gen;
mod validator;

use clap::{Parser, Subcommand};

/// Sweech — Switch architectures. Not codebases.
#[derive(Parser)]
#[command(
    name = "sweech",
    version,
    about = "Switch architectures. Not codebases.",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold a new Sweech project
    Init {
        /// Project name (becomes the directory name)
        name: Option<String>,

        /// Path to your local sweech workspace (contains sweech-core/ and sweech-axum/).
        /// Auto-detected if not provided. Required until sweech is on crates.io.
        ///
        /// Example: sweech init my-app --path /home/user/sweech
        #[arg(long)]
        path: Option<String>,
    },

    /// Start development mode with hot reload
    ///
    /// Spawns cargo watch for the backend and dev commands for all
    /// declared frontends. Watches route.rs files and hot-reloads
    /// packages/types on changes.
    Dev,

    /// Run pre-flight validation without building
    Check,

    /// Manage sweech global configuration
    ///
    /// Config lives at ~/.config/sweech/config.toml
    Config {
        #[command(subcommand)]
        what: ConfigCommands,
    },

    /// Build artifacts for a deployment topology
    Build {
        /// Deployment mode: monolith | microservices | serverless
        #[arg(long, default_value = "monolith")]
        mode: String,
    },

    /// Add scaffolding to an existing project
    Add {
        #[command(subcommand)]
        what: AddCommands,
    },

    /// Generate project artifacts
    Generate {
        #[command(subcommand)]
        what: GenerateCommands,
    },
}

#[derive(Subcommand)]
enum AddCommands {
    /// Scaffold a new applet directory and root route handler
    ///
    /// Example: sweech add applet inventory
    Applet {
        /// Applet name (becomes the directory name and URL prefix)
        name: String,

        /// Default auth requirement for handlers in this applet
        #[arg(long, default_value = "required")]
        auth: Option<String>,
    },

    /// Scaffold a new route handler inside an existing applet
    ///
    /// Example: sweech add handler inventory/items
    ///          sweech add handler inventory/items/[itemId]
    Handler {
        /// <applet>/<route-path> — e.g. inventory/items or auth/login
        path: String,

        /// HTTP method: GET | POST | PUT | PATCH | DELETE
        #[arg(long)]
        method: Option<String>,

        /// Auth requirement: required | public | optional
        #[arg(long)]
        auth: Option<String>,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Set a config value
    ///
    /// Keys: source-url, source-path, source-version
    ///
    /// Examples:
    ///   sweech config set source-url https://github.com/devWithKD/sweech
    ///   sweech config set source-path /home/user/sweech
    ///   sweech config set source-version 0.1
    Set { key: String, value: String },
    /// Remove a config value
    Unset { key: String },
}

#[derive(Subcommand)]
enum GenerateCommands {
    /// Generate Dockerfile(s) for the current build mode
    ///
    /// Monolith: single multi-stage Dockerfile (embedded or standalone frontend)
    Dockerfile,

    /// Generate docker-compose.yml for local/production use
    Compose,

    /// Generate packages/types/index.ts from handler contracts
    ///
    /// Also runs automatically during `sweech dev` on file changes.
    Types,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { name, path } => commands::init::run(name, path),
        Commands::Dev => commands::dev::run(),
        Commands::Check => commands::check::run(),
        Commands::Build { mode } => commands::build::run(mode),
        Commands::Add { what } => match what {
            AddCommands::Applet { name, auth } => commands::add::add_applet(name, auth),
            AddCommands::Handler { path, method, auth } => {
                commands::add::add_handler(path, method, auth)
            }
        },
        Commands::Config { what } => match what {
            ConfigCommands::Show => commands::config::show(),
            ConfigCommands::Set { key, value } => commands::config::set(key, value),
            ConfigCommands::Unset { key } => commands::config::unset(key),
        },
        Commands::Generate { what } => match what {
            GenerateCommands::Dockerfile => commands::generate::run_dockerfile(),
            GenerateCommands::Compose => commands::generate::run_compose(),
            GenerateCommands::Types => commands::generate::run_types(),
        },
    };

    if let Err(e) = result {
        eprintln!("{}", format!("error: {e}").as_str());
        for cause in e.chain().skip(1) {
            eprintln!("       {cause}");
        }
        std::process::exit(1);
    }
}
