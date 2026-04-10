use crate::{
    manifest::{AuthSetting, Manifest},
    templates,
};
use anyhow::{Context, Result, bail};
use colored::Colorize;
use std::fs;

// ─── sweech add applet <name> ─────────────────────────────────────────────────

pub fn add_applet(name: String, auth: Option<String>) -> Result<()> {
    let root = Manifest::find_root()?;
    let manifest = Manifest::load(&root)?;

    // Validate name
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        bail!(
            "Applet name '{}' contains invalid characters.\n\
             Use lowercase letters, numbers, hyphens, or underscores.",
            name
        );
    }

    // Check not already declared in manifest
    if manifest.applets.iter().any(|a| a.name == name) {
        bail!(
            "Applet '{}' is already declared in sweech.manifest.toml.",
            name
        );
    }

    let auth_setting = parse_auth(auth.as_deref())?;
    let applet_dir_name = format!("{}.applet", name);
    let applet_path = root.join(&applet_dir_name);

    if applet_path.exists() {
        bail!("Directory '{}' already exists.", applet_path.display());
    }

    println!("{} {}", "sweech add applet".bold(), name.cyan().bold());
    println!();

    // Create the applet directory and root route.rs
    fs::create_dir_all(&applet_path)
        .with_context(|| format!("Could not create '{}'", applet_path.display()))?;

    let route_path = applet_path.join("route.rs");
    fs::write(
        &route_path,
        templates::applet_route_rs(&name, &auth_setting),
    )
    .with_context(|| format!("Could not write {}", route_path.display()))?;
    println!(
        "  {} {}",
        "+".green(),
        format!("{}/route.rs", applet_dir_name).white()
    );

    // Append to manifest (raw text append — preserves comments, avoids serializer issues)
    let new_applet = crate::manifest::AppletManifest {
        name: name.clone(),
        path: applet_dir_name.clone(),
        auth: auth_setting,
        guards: vec![],
        stateful: false,
        runtime: None,
    };
    Manifest::append_applet(&root, &new_applet)?;
    println!(
        "  {} {}",
        "~".yellow(),
        "sweech.manifest.toml (applet entry added)".white()
    );

    println!();
    println!("{}", "Done!".bold());
    println!();
    println!(
        "  Route file: {}",
        format!("{}/route.rs", applet_dir_name).cyan()
    );
    println!("  URL prefix: {}", format!("/{}", name).green());
    println!();
    println!(
        "{}",
        "  Remember to register the applet in sweech.app.rs.".dimmed()
    );
    println!();

    Ok(())
}

// ─── sweech add handler <applet>/<path> ───────────────────────────────────────
//
// Examples:
//   sweech add handler inventory/items          → inventory.applet/items/route.rs
//   sweech add handler inventory/items/[itemId] → inventory.applet/items/[itemId]/route.rs
//   sweech add handler auth/login               → auth.applet/login/route.rs  (POST by convention)

pub fn add_handler(path: String, method: Option<String>, auth: Option<String>) -> Result<()> {
    let root = Manifest::find_root()?;

    // path format: "<applet>/<route-path>"
    // e.g. "inventory/items" or "inventory/items/[itemId]"
    let parts: Vec<&str> = path.splitn(2, '/').collect();
    if parts.len() != 2 {
        bail!(
            "Path must be in format <applet>/<route-path>.\n\
             Example: sweech add handler inventory/items"
        );
    }

    let applet_name = parts[0];
    let route_subpath = parts[1]; // e.g. "items" or "items/[itemId]"

    // Infer a handler name from the last path segment
    let last_segment = route_subpath.split('/').last().unwrap_or(route_subpath);
    let clean_segment = last_segment
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_start_matches("...");

    let inferred_method = method.as_deref().unwrap_or_else(|| {
        // Convention: if last segment is a param, it's probably a GET/PUT/DELETE on a resource
        // Otherwise default to GET. User can always change it.
        "GET"
    });

    // Build handler name: e.g. "items" + GET → "GetItems", [itemId] + GET → "GetItem"
    let handler_name = format!("{}_{}", inferred_method.to_lowercase(), clean_segment);

    let auth_setting = parse_auth(auth.as_deref())?;

    let applet_dir = root.join(format!("{}.applet", applet_name));
    if !applet_dir.exists() {
        bail!(
            "Applet directory '{}.applet' not found.\n\
             Run `sweech add applet {}` first.",
            applet_name,
            applet_name
        );
    }

    let handler_dir = applet_dir.join(route_subpath);
    let route_file = handler_dir.join("route.rs");

    if route_file.exists() {
        bail!("Route file '{}' already exists.", route_file.display());
    }

    println!(
        "{} {}/{}",
        "sweech add handler".bold(),
        applet_name.cyan(),
        route_subpath.cyan()
    );
    println!();

    fs::create_dir_all(&handler_dir)
        .with_context(|| format!("Could not create '{}'", handler_dir.display()))?;

    fs::write(
        &route_file,
        templates::handler_route_rs(&handler_name, inferred_method, &auth_setting),
    )
    .with_context(|| format!("Could not write {}", route_file.display()))?;

    let display_path = format!("{}.applet/{}/route.rs", applet_name, route_subpath);
    println!("  {} {}", "+".green(), display_path.white());

    // Convert route path to Axum URL for display
    let url_segments: Vec<String> = route_subpath
        .split('/')
        .map(|s| {
            if let Some(inner) = s.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                format!(":{}", inner)
            } else {
                s.to_string()
            }
        })
        .collect();
    let url = format!("/{}/{}", applet_name, url_segments.join("/"));

    println!();
    println!("{}", "Done!".bold());
    println!();
    println!("  Method:  {}", inferred_method.green().bold());
    println!("  URL:     {}", url.cyan());
    println!(
        "  Handler: {}",
        templates::to_pascal_case(&handler_name).white()
    );
    println!();

    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn parse_auth(s: Option<&str>) -> Result<AuthSetting> {
    Ok(match s {
        None | Some("required") => AuthSetting::Required,
        Some("public") => AuthSetting::Public,
        Some("optional") => AuthSetting::Optional,
        Some(other) => bail!(
            "Unknown auth value '{}'. Use: required, public, optional.",
            other
        ),
    })
}
