use crate::{
    manifest::{Manifest, ServeMode},
    scanner, templates, type_gen, validator,
};
use anyhow::{Result, bail};
use colored::Colorize;
use std::fs;
use std::path::Path;

pub fn run_dockerfile() -> Result<()> {
    let root = Manifest::find_root()?;
    let manifest = Manifest::load(&root)?;
    let project = scanner::scan(&root)?;

    println!("{}", "sweech generate dockerfile".bold());
    println!(
        "  {} {}  mode: {}",
        "project".dimmed(),
        manifest.project.name.cyan().bold(),
        format!("{:?}", manifest.build.mode).to_lowercase().green()
    );
    println!();

    // Pre-flight
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
        bail!("Fix errors before generating Dockerfile. Run `sweech check` for details.");
    }

    let port = manifest.build.port;
    let project_name = &manifest.project.name;

    // Find an embedded web frontend, if any
    let embedded_frontend = manifest
        .frontends
        .iter()
        .find(|f| !f.framework.is_mobile() && f.serve == Some(ServeMode::Embedded));

    let (dockerfile_content, note) = if let Some(fe) = embedded_frontend {
        let build_cmd = fe
            .build_command
            .clone()
            .unwrap_or_else(|| fe.framework.default_build_command().to_string());

        (
            templates::dockerfile_monolith_embedded_frontend(
                project_name,
                port,
                &fe.path,
                &build_cmd,
            ),
            format!(
                "embedded frontend: {} ({})",
                fe.name.cyan(),
                fe.path.dimmed()
            ),
        )
    } else {
        (
            templates::dockerfile_monolith_rust_only(project_name, port),
            "backend only".to_string(),
        )
    };

    // Also generate standalone frontend Dockerfiles if any
    let standalone_frontends: Vec<_> = manifest
        .frontends
        .iter()
        .filter(|f| !f.framework.is_mobile() && f.serve == Some(ServeMode::Standalone))
        .collect();

    // Write main Dockerfile
    write_file(&root, "Dockerfile", &dockerfile_content)?;
    println!("  {} Dockerfile  ({})", "+".green(), note);

    // Write .dockerignore
    write_file(&root, ".dockerignore", templates::dockerignore())?;
    println!("  {} .dockerignore", "+".green());

    // Standalone frontend Dockerfiles
    for fe in &standalone_frontends {
        let build_cmd = fe
            .build_command
            .clone()
            .unwrap_or_else(|| fe.framework.default_build_command().to_string());

        let content = standalone_frontend_dockerfile(&fe.path, &build_cmd);
        let filename = format!("Dockerfile.{}", fe.name);
        write_file(&root, &filename, &content)?;
        println!(
            "  {} {}  (standalone frontend)",
            "+".green(),
            filename.white()
        );
    }

    println!();
    println!("{}", "Done!".bold());
    println!();
    println!("  {}", "Build and run:".dimmed());
    println!(
        "    {}",
        format!("docker build -t {} .", project_name).cyan()
    );
    println!(
        "    {}",
        format!("docker run -p {}:{} {}", port, port, project_name).cyan()
    );

    Ok(())
}

pub fn run_compose() -> Result<()> {
    let root = Manifest::find_root()?;
    let manifest = Manifest::load(&root)?;

    println!("{}", "sweech generate compose".bold());
    println!();

    let content = templates::docker_compose_monolith(&manifest.project.name, manifest.build.port);
    write_file(&root, "docker-compose.yml", &content)?;
    println!("  {} docker-compose.yml", "+".green());

    println!();
    println!("{}", "Done!".bold());
    println!();
    println!("  {}", "Start:".dimmed());
    println!("    {}", "docker compose up --build".cyan());

    Ok(())
}

pub fn run_types() -> Result<()> {
    let root = Manifest::find_root()?;
    let manifest = Manifest::load(&root)?;
    let project = scanner::scan(&root)?;

    println!("{}", "sweech generate types".bold());
    println!();

    match type_gen::generate(&root, &manifest, &project) {
        Ok(out_path) => {
            println!(
                "  {} {}",
                "✓".green(),
                out_path.display().to_string().cyan()
            );
            println!();
            println!("{}", "Done!".bold());
            println!();
            println!("  Import in your frontend:");
            println!(
                "    {}",
                r#"import { API } from "../../packages/types";"#.cyan()
            );
        }
        Err(e) => {
            bail!("Type generation failed: {}", e);
        }
    }

    Ok(())
}

// ─── Standalone frontend Dockerfile ──────────────────────────────────────────

fn standalone_frontend_dockerfile(frontend_path: &str, build_command: &str) -> String {
    format!(
        r#"# syntax=docker/dockerfile:1
# Frontend — standalone static server
FROM node:20-slim AS builder
WORKDIR /app
COPY {frontend_path}/package*.json ./
RUN npm ci
COPY {frontend_path}/ ./
RUN {build_command}

FROM nginx:alpine AS runtime
COPY --from=builder /app/out /usr/share/nginx/html
COPY --from=builder /app/.next/static /usr/share/nginx/html/_next/static
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
"#
    )
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn write_file(root: &Path, name: &str, content: &str) -> Result<()> {
    let path = root.join(name);
    fs::write(&path, content)
        .map_err(|e| anyhow::anyhow!("Failed to write {}: {}", path.display(), e))?;
    Ok(())
}
