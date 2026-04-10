use crate::{manifest::Manifest, scanner, type_gen};
use anyhow::Result;
use colored::Colorize;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

// ─── sweech dev ───────────────────────────────────────────────────────────────
//
// What this does:
//   1. Validates the project (sweech check equivalent)
//   2. Prints the route map
//   3. Runs type generation (packages/types)
//   4. Spawns the backend: cargo watch -x run (or cargo run if watch not installed)
//   5. Spawns each frontend's dev_command in parallel
//   6. Watches .applet/**/route.rs files for changes → re-runs type generation
//   7. Ctrl+C kills all children cleanly

pub fn run() -> Result<()> {
    let root = Manifest::find_root()?;
    let manifest = Manifest::load(&root)?;
    let project = scanner::scan(&root)?;

    // ── Header ────────────────────────────────────────────────────────────────
    println!("{}", "sweech dev".bold().green());
    println!(
        "  {} {}  {}",
        "project".dimmed(),
        manifest.project.name.cyan().bold(),
        format!("({})", format!("{:?}", manifest.build.mode).to_lowercase()).dimmed()
    );
    println!();

    if project.applets.is_empty() {
        println!(
            "{}",
            "No applets found. Run `sweech add applet <name>` to get started.".yellow()
        );
        return Ok(());
    }

    // ── Route map ─────────────────────────────────────────────────────────────
    println!("{}", "Routes:".bold());
    for applet in &project.applets {
        println!(
            "  {} {}  {}",
            "▸".green(),
            applet.name.cyan().bold(),
            format!("({} routes)", applet.routes.len()).dimmed()
        );
        for route in &applet.routes {
            let method = route
                .handler_info
                .as_ref()
                .map(|h| h.method.as_str())
                .unwrap_or("???");
            println!(
                "    {} {}  {}{}",
                "·".dimmed(),
                pad_method(method),
                applet.prefix().dimmed(),
                route.axum_path.white()
            );
        }
    }
    println!();

    // ── Initial type generation ───────────────────────────────────────────────
    if manifest.frontends.iter().any(|f| f.framework.is_ts_based()) {
        match type_gen::generate(&root, &manifest, &project) {
            Ok(path) => println!(
                "  {} types → {}",
                "✓".green(),
                path.display().to_string().dimmed()
            ),
            Err(e) => println!("  {} type generation skipped: {}", "⚠".yellow(), e),
        }
        println!();
    }

    // ── Ctrl+C handler ────────────────────────────────────────────────────────
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Could not set Ctrl+C handler");

    // ── Spawn backend ─────────────────────────────────────────────────────────
    let backend_cmd = backend_command(&root);
    println!(
        "  {} {}  {}",
        "▸".green(),
        "backend".cyan().bold(),
        backend_cmd.join(" ").dimmed()
    );
    let mut backend = spawn_prefixed(&backend_cmd, &root, "backend")?;

    // ── Spawn frontends ───────────────────────────────────────────────────────
    let mut frontends: Vec<Child> = Vec::new();
    for fe in &manifest.frontends {
        let dev_cmd_str = fe
            .dev_command
            .clone()
            .unwrap_or_else(|| fe.framework.default_dev_command().to_string());

        let fe_path = root.join(&fe.path);
        let cmd_parts = shell_words(&dev_cmd_str);

        println!(
            "  {} {}  {}",
            "▸".green(),
            fe.name.cyan().bold(),
            dev_cmd_str.dimmed()
        );

        match spawn_prefixed(&cmd_parts, &fe_path, &fe.name) {
            Ok(child) => frontends.push(child),
            Err(e) => println!("  {} failed to start {}: {}", "✗".red(), fe.name, e),
        }
    }

    println!();
    println!(
        "{}",
        format!("  Listening on http://localhost:{}", manifest.build.port)
            .green()
            .bold()
    );
    println!("{}", "  Press Ctrl+C to stop.".dimmed());
    println!();

    // ── File watcher for hot-reload types ─────────────────────────────────────
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

    // Watch all .applet directories for route.rs changes
    for applet in &project.applets {
        let _ = watcher.watch(&applet.path, RecursiveMode::Recursive);
    }

    let mut last_regen = Instant::now();

    // ── Main loop ─────────────────────────────────────────────────────────────
    while running.load(Ordering::SeqCst) {
        // Check if any child exited unexpectedly
        if let Ok(Some(status)) = backend.try_wait() {
            if !status.success() {
                println!(
                    "  {} Backend exited with status {}. Restart with {}.",
                    "✗".red(),
                    status,
                    "sweech dev".green()
                );
            }
            break;
        }

        // Check for file system events (type regen)
        if let Ok(Ok(event)) = rx.try_recv() {
            let is_route_change = event
                .paths
                .iter()
                .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("route.rs"));

            // Debounce: only regen if >500ms since last regen
            if is_route_change && last_regen.elapsed() > Duration::from_millis(500) {
                last_regen = Instant::now();

                // Re-scan and re-generate
                if let Ok(updated) = scanner::scan(&root) {
                    match type_gen::generate(&root, &manifest, &updated) {
                        Ok(_) => println!("  {} types regenerated", "↺".cyan()),
                        Err(e) => println!("  {} type regen failed: {}", "⚠".yellow(), e),
                    }
                }
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    // ── Cleanup ───────────────────────────────────────────────────────────────
    println!();
    println!("{}", "Stopping...".dimmed());

    let _ = backend.kill();
    for mut child in frontends {
        let _ = child.kill();
    }

    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Decide what command to use for the backend.
/// Uses `cargo watch` if available, falls back to `cargo run`.
fn backend_command(root: &PathBuf) -> Vec<String> {
    // Check if cargo-watch is available
    let watch_available = Command::new("cargo")
        .args(["watch", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if watch_available {
        // cargo watch -q -c -x run
        // -q: quiet (less noise)
        // -c: clear screen on rebuild
        // -x run: run `cargo run` after rebuild
        vec![
            "cargo".to_string(),
            "watch".to_string(),
            "-q".to_string(),
            "-c".to_string(),
            "-x".to_string(),
            "run".to_string(),
        ]
    } else {
        // Install cargo-watch automatically on first use
        println!(
            "  {} cargo-watch not found — installing it for hot reload...",
            "→".yellow()
        );
        let install = Command::new("cargo")
            .args(["install", "cargo-watch"])
            .current_dir(root)
            .status();

        match install {
            Ok(s) if s.success() => vec![
                "cargo".to_string(),
                "watch".to_string(),
                "-q".to_string(),
                "-c".to_string(),
                "-x".to_string(),
                "run".to_string(),
            ],
            _ => {
                println!(
                    "  {} cargo-watch install failed. Using cargo run (no hot reload).",
                    "⚠".yellow()
                );
                vec!["cargo".to_string(), "run".to_string()]
            }
        }
    }
}

/// Spawn a child process with a colored prefix on each output line.
fn spawn_prefixed(cmd: &[String], cwd: &PathBuf, label: &str) -> Result<Child> {
    if cmd.is_empty() {
        anyhow::bail!("Empty command for '{}'", label);
    }

    let child = Command::new(&cmd[0])
        .args(&cmd[1..])
        .current_dir(cwd)
        // Inherit stdio — child output goes straight to terminal.
        // For prefixed output we'd need to pipe and spawn threads;
        // for now direct inheritance is cleaner and avoids buffering issues.
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to spawn '{}' for '{}': {}\n  Make sure the command is installed.",
                cmd[0],
                label,
                e
            )
        })?;

    Ok(child)
}

fn pad_method(m: &str) -> String {
    format!("{:<6}", m).bold().to_string()
}

/// Split a shell command string into parts, respecting quoted strings.
/// "npm run dev"    → ["npm", "run", "dev"]
/// "pnpm dev"       → ["pnpm", "dev"]
/// Does NOT handle shell pipes or redirects — just simple space-separated args.
fn shell_words(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';

    for ch in s.chars() {
        match ch {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = ch;
            }
            c if in_quotes && c == quote_char => {
                in_quotes = false;
            }
            ' ' if !in_quotes => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_words_basic() {
        assert_eq!(shell_words("npm run dev"), vec!["npm", "run", "dev"]);
        assert_eq!(
            shell_words("cargo watch -x run"),
            vec!["cargo", "watch", "-x", "run"]
        );
    }

    #[test]
    fn shell_words_quoted() {
        assert_eq!(
            shell_words(r#"echo "hello world""#),
            vec!["echo", "hello world"]
        );
    }
}
