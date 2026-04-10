mod commands;
mod manifest;
mod scanner;
mod validator;

use clap::{Parser, Subcommand};

/// Sweech — Switch architectures. Not codebases.
#[derive(Parser)]
#[command(name = "sweech", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start development mode — discovers applets and prints the route map
    Dev,

    /// Run pre-flight validation without building
    Check,

    /// Build artifacts for a deployment topology
    Build {
        #[arg(long, default_value = "monolith")]
        mode: String,
    },

    /// Scaffold a new Sweech project
    Init { name: Option<String> },

    /// Add a new applet or handler to the project
    Add {
        #[command(subcommand)]
        what: AddCommands,
    },
}

#[derive(Subcommand)]
enum AddCommands {
    /// Scaffold a new applet directory
    Applet { name: String },
    /// Scaffold a new route handler
    Handler { path: String },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Dev => commands::dev::run(),
        Commands::Check => commands::check::run(),
        Commands::Build { mode } => {
            println!("sweech build --mode {mode}");
            println!("Build system coming in next phase.");
            Ok(())
        }
        Commands::Init { name } => {
            let project_name = name.unwrap_or_else(|| "my-sweech-app".to_string());
            println!("sweech init {project_name}");
            println!("Project scaffolding coming in next phase.");
            Ok(())
        }
        Commands::Add { what } => match what {
            AddCommands::Applet { name } => {
                println!("sweech add applet {name}");
                println!("Scaffolding coming in next phase.");
                Ok(())
            }
            AddCommands::Handler { path } => {
                println!("sweech add handler {path}");
                println!("Scaffolding coming in next phase.");
                Ok(())
            }
        },
    };

    // Print errors cleanly — no stack trace noise in normal usage
    if let Err(e) = result {
        eprintln!("error: {e}");
        // Chain of causes, indented
        for cause in e.chain().skip(1) {
            eprintln!("       {cause}");
        }
        std::process::exit(1);
    }
}
