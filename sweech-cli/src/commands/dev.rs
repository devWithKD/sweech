use crate::{manifest::Manifest, scanner};
use anyhow::Result;
use colored::Colorize;

pub fn run() -> Result<()> {
    let root = Manifest::find_root()?;
    let manifest = Manifest::load(&root)?;
    let project = scanner::scan(&root)?;

    println!("{}", "sweech dev".bold());
    println!("Project: {}", manifest.project.name.cyan());
    println!("Mode:    {:?}", manifest.build.mode);
    println!();

    if project.applets.is_empty() {
        println!(
            "{}",
            "No applets found. Create a folder ending in .applet to get started.".yellow()
        );
        return Ok(());
    }

    println!("{}", "Applets:".bold());
    for applet in &project.applets {
        println!(
            "  {} {} {} routes",
            "▸".green(),
            applet.name.cyan().bold(),
            applet.routes.len().to_string().dimmed(),
        );
        for route in &applet.routes {
            println!(
                "    {} {}{}",
                "·".dimmed(),
                applet.prefix().dimmed(),
                route.axum_path.white()
            );
        }
    }

    println!();
    println!("{}", "─────────────────────────────────────────".dimmed());
    println!(
        "{}  {} routes across {} applet(s)",
        "→".green().bold(),
        project.route_count().to_string().white().bold(),
        project.applets.len().to_string().white().bold(),
    );
    println!();

    // Phase 1: sweech dev prints the discovered structure and validates.
    // Full hot-reload process spawning comes in Phase 2.
    println!(
        "{}",
        "Note: Full hot-reload dev server coming in CLI Phase 2.".dimmed()
    );
    println!(
        "{}",
        "      sweech dev currently validates and prints the route map.".dimmed()
    );

    Ok(())
}
