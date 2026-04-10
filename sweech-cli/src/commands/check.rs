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
    println!(
        "  {} {}  mode: {}",
        "project".dimmed(),
        manifest.project.name.cyan().bold(),
        format!("{:?}", manifest.build.mode).to_lowercase().green()
    );
    println!();

    println!("{}", "Discovered:".bold());
    for applet in &project.applets {
        println!("  {} {}", "▸".dimmed(), applet.name.cyan());
        for route in &applet.routes {
            let method = route
                .handler_info
                .as_ref()
                .map(|h| h.method.as_str())
                .unwrap_or("???");
            println!(
                "    {} {:6} {}{}",
                "·".dimmed(),
                method,
                applet.prefix().dimmed(),
                route.axum_path.white()
            );
        }
    }
    println!();

    let issues = validator::validate(&manifest, &project);

    if issues.is_empty() {
        println!("{}", "✓ No issues found.".green().bold());
        return Ok(());
    }

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
        std::process::exit(1);
    } else {
        println!(
            "{}",
            format!("⚠ {} warning(s)", warnings.len()).yellow().bold()
        );
    }

    Ok(())
}
