use crate::{
    manifest::Manifest,
    scanner,
    validator::{self, Severity},
};
use anyhow::Result;
use colored::Colorize;

pub fn run() -> Result<()> {
    let root = Manifest::find_root()?;
    let manifest = Manifest::load(&root)?;
    let project = scanner::scan(&root)?;

    println!("{}", "sweech check".bold());
    println!("Project: {}", manifest.project.name.cyan());
    println!("Mode:    {:?}", manifest.build.mode);
    println!();

    // Print what was found
    println!("{}", "Discovered:".bold());
    for applet in &project.applets {
        println!("  {} {}", "▸".dimmed(), applet.name.cyan());
        for route in &applet.routes {
            println!("    {} {}", "·".dimmed(), route.axum_path);
        }
    }
    println!();

    // Run validation
    let issues = validator::validate(&manifest, &project);

    if issues.is_empty() {
        println!("{}", "✓ No issues found.".green().bold());
        return Ok(());
    }

    // Print issues grouped by severity
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    let warnings: Vec<_> = issues
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .collect();

    for issue in &warnings {
        println!(
            "{} [{}] {}",
            "warn".yellow().bold(),
            issue.code.yellow(),
            issue.message
        );
    }
    for issue in &errors {
        println!(
            "{} [{}] {}",
            "error".red().bold(),
            issue.code.red(),
            issue.message
        );
    }

    println!();

    if validator::has_errors(&issues) {
        println!(
            "{}",
            format!("✗ {} error(s), {} warning(s)", errors.len(), warnings.len())
                .red()
                .bold()
        );
        // Exit with non-zero so CI pipelines fail correctly
        std::process::exit(1);
    } else {
        println!(
            "{}",
            format!("⚠ {} warning(s)", warnings.len()).yellow().bold()
        );
    }

    Ok(())
}
