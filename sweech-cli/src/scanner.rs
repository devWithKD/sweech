use anyhow::Result;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// ─── What is this file? ───────────────────────────────────────────────────────
//
// The scanner walks the project filesystem and discovers the structure
// Sweech cares about:
//
//   1. Applet directories — folders ending in `.applet`
//   2. Route files — `route.rs` files inside applet directories
//   3. The path segments between the applet root and each route.rs
//      become URL path segments
//
// The output is a `ScannedProject` — a structured description of what
// was found, which the CLI uses to:
//   - Run validation (sweech check)
//   - Start processes (sweech dev)
//   - Generate build artifacts (sweech build)
//
// ─── Example: what scanning produces ─────────────────────────────────────────
//
// Given this tree:
//   auth.applet/
//     login/route.rs
//     register/route.rs
//     [userId]/route.rs
//   products.applet/
//     route.rs
//     [productId]/route.rs
//
// Scanner produces:
//   ScannedApplet { name: "auth", routes: [
//     ScannedRoute { path_segments: ["login"],            file: "auth.applet/login/route.rs" }
//     ScannedRoute { path_segments: ["register"],         file: "auth.applet/register/route.rs" }
//     ScannedRoute { path_segments: [":userId"],          file: "auth.applet/[userId]/route.rs" }
//   ]}
//   ScannedApplet { name: "products", routes: [
//     ScannedRoute { path_segments: [],                   file: "products.applet/route.rs" }
//     ScannedRoute { path_segments: [":productId"],       file: "products.applet/[productId]/route.rs" }
//   ]}
//
// Note: [userId] folder → :userId path segment (Next.js convention → Express convention)

/// A route file found inside an applet.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ScannedRoute {
    /// Path segments between the applet root and this route.rs.
    /// Empty vec = the applet root route (products.applet/route.rs → GET /products)
    /// ["login"] = auth.applet/login/route.rs → /auth/login
    /// [":userId"] = auth.applet/[userId]/route.rs → /auth/:userId
    pub path_segments: Vec<String>,

    /// Absolute path to the route.rs file
    pub file: PathBuf,

    /// The Axum-style route path for this file, e.g. "/login" or "/:userId"
    /// Relative to the applet — the applet prefix is NOT included.
    pub axum_path: String,
}

impl ScannedRoute {
    /// Build the axum_path from path_segments.
    /// Empty segments → "/" (the applet root)
    /// ["login"] → "/login"
    /// [":userId"] → "/:userId"
    /// ["orders", ":orderId"] → "/orders/:orderId"
    pub fn build_axum_path(segments: &[String]) -> String {
        if segments.is_empty() {
            return "/".to_string();
        }
        format!("/{}", segments.join("/"))
    }
}

/// An applet found in the project.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ScannedApplet {
    /// The applet name, e.g. "auth" (derived from "auth.applet" folder)
    pub name: String,

    /// Absolute path to the .applet directory
    pub path: PathBuf,

    /// All route.rs files found inside this applet
    pub routes: Vec<ScannedRoute>,
}

impl ScannedApplet {
    /// The URL prefix this applet mounts at.
    pub fn prefix(&self) -> String {
        format!("/{}", self.name)
    }
}

/// The full result of scanning a project.
#[allow(dead_code)]
#[derive(Debug)]
pub struct ScannedProject {
    /// Absolute path to the project root (where sweech.manifest.toml lives)
    pub root: PathBuf,

    /// All applets found, in filesystem order
    pub applets: Vec<ScannedApplet>,
}

impl ScannedProject {
    /// Total number of route files across all applets
    pub fn route_count(&self) -> usize {
        self.applets.iter().map(|a| a.routes.len()).sum()
    }
}

// ─── Scanner ──────────────────────────────────────────────────────────────────

/// Scan a project root and return the discovered structure.
pub fn scan(project_root: &Path) -> Result<ScannedProject> {
    let mut applets = Vec::new();

    // Walk the project root (non-recursive at this level — we handle recursion
    // inside each applet ourselves)
    //
    // ─── Rust concept: WalkDir ────────────────────────────────────────────
    //
    // WalkDir gives us an iterator over directory entries.
    // `.max_depth(1)` means only look one level deep — direct children
    // of the project root. We don't want to recurse into node_modules
    // or target directories here. We recurse into applets separately.
    //
    // `.into_iter()` turns it into a standard iterator we can chain.
    // `.filter_map(|e| e.ok())` skips entries we can't read (permission errors etc.)

    for entry in WalkDir::new(project_root)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // ─── Rust concept: if let + method chaining ───────────────────────
        //
        // `path.file_name()` → Option<&OsStr> (the last component of the path)
        // `.and_then(|n| n.to_str())` → Option<&str> (convert OsStr to str)
        // `.map(|n| n.ends_with(".applet"))` → Option<bool>
        // `.unwrap_or(false)` → bool (false if any step returned None)
        //
        // This chain safely handles all the Option unwrapping without panicking.

        let is_applet_dir = path.is_dir()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(".applet"))
                .unwrap_or(false);

        if !is_applet_dir {
            continue;
        }

        let applet_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap()
            .trim_end_matches(".applet")
            .to_string();

        let routes = scan_applet_routes(path)?;

        applets.push(ScannedApplet {
            name: applet_name,
            path: path.to_path_buf(),
            routes,
        });
    }

    // Sort applets alphabetically for deterministic output
    applets.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ScannedProject {
        root: project_root.to_path_buf(),
        applets,
    })
}

/// Recursively find all route.rs files inside an applet directory.
fn scan_applet_routes(applet_path: &Path) -> Result<Vec<ScannedRoute>> {
    let mut routes = Vec::new();

    // Walk the entire applet subtree looking for route.rs files
    for entry in WalkDir::new(applet_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        // Only care about files named "route.rs"
        let is_route_file =
            path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some("route.rs");

        if !is_route_file {
            continue;
        }

        // Compute path segments: the directories between the applet root
        // and this route.rs file.
        //
        // ─── Rust concept: strip_prefix ───────────────────────────────────
        //
        // `path.strip_prefix(applet_path)` removes the applet root from
        // the beginning of the path, giving us the relative path.
        // Returns Result — fails if the path doesn't start with the prefix.
        //
        // Example:
        //   applet_path = /project/auth.applet
        //   path        = /project/auth.applet/login/route.rs
        //   relative    = login/route.rs
        //   parent      = login
        //   segments    = ["login"]

        let relative = path.strip_prefix(applet_path)?;
        let segments = relative
            .parent() // remove "route.rs" filename
            .map(|p| {
                p.components()
                    .filter_map(|c| {
                        // ─── Rust concept: pattern matching on enum variants ──
                        //
                        // `std::path::Component` is an enum. We only want
                        // Normal components (actual folder names), not
                        // RootDir, CurDir, ParentDir, or Prefix.
                        use std::path::Component;
                        match c {
                            Component::Normal(os) => os.to_str().map(convert_segment),
                            _ => None,
                        }
                    })
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        let axum_path = ScannedRoute::build_axum_path(&segments);

        routes.push(ScannedRoute {
            path_segments: segments,
            file: path.to_path_buf(),
            axum_path,
        });
    }

    // Sort routes for deterministic output
    routes.sort_by(|a, b| a.axum_path.cmp(&b.axum_path));

    Ok(routes)
}

/// Convert a folder name segment to a URL path segment.
///
/// Sweech uses Next.js App Router conventions:
///   [param]    → :param      (dynamic segment)
///   [...slug]  → *slug       (catch-all) — future
///   (group)    → ""          (grouping folder, skipped) — future
///   normal     → normal      (static segment)
fn convert_segment(folder_name: &str) -> String {
    if folder_name.starts_with('[') && folder_name.ends_with(']') {
        // Dynamic segment: [userId] → :userId
        let inner = &folder_name[1..folder_name.len() - 1];
        format!(":{}", inner)
    } else {
        // Static segment — use as-is
        folder_name.to_string()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir; // we'll add this as dev-dependency

    fn make_route(tmp: &Path, rel_path: &str) {
        let full = tmp.join(rel_path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(&full, "// route").unwrap();
    }

    #[test]
    fn convert_segment_handles_dynamic() {
        assert_eq!(convert_segment("[userId]"), ":userId");
        assert_eq!(convert_segment("[productId]"), ":productId");
        assert_eq!(convert_segment("login"), "login");
        assert_eq!(convert_segment("orders"), "orders");
    }

    #[test]
    fn axum_path_from_segments() {
        assert_eq!(ScannedRoute::build_axum_path(&[]), "/");
        assert_eq!(
            ScannedRoute::build_axum_path(&["login".to_string()]),
            "/login"
        );
        assert_eq!(
            ScannedRoute::build_axum_path(&[":userId".to_string()]),
            "/:userId"
        );
        assert_eq!(
            ScannedRoute::build_axum_path(&["orders".to_string(), ":orderId".to_string()]),
            "/orders/:orderId"
        );
    }

    #[test]
    fn scan_finds_applets_and_routes() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // auth.applet/login/route.rs
        make_route(root, "auth.applet/login/route.rs");
        // auth.applet/register/route.rs
        make_route(root, "auth.applet/register/route.rs");
        // products.applet/route.rs
        make_route(root, "products.applet/route.rs");
        // products.applet/[productId]/route.rs
        make_route(root, "products.applet/[productId]/route.rs");

        let project = scan(root).unwrap();

        assert_eq!(project.applets.len(), 2);
        assert_eq!(project.route_count(), 4);

        let auth = project.applets.iter().find(|a| a.name == "auth").unwrap();
        assert_eq!(auth.routes.len(), 2);

        let products = project
            .applets
            .iter()
            .find(|a| a.name == "products")
            .unwrap();
        // route.rs at root + [productId]/route.rs
        assert_eq!(products.routes.len(), 2);

        // Check dynamic segment conversion
        let dynamic = products
            .routes
            .iter()
            .find(|r| r.axum_path.contains(":productId"));
        assert!(dynamic.is_some(), "Expected :productId route");
    }

    #[test]
    fn non_applet_dirs_are_ignored() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        make_route(root, "auth.applet/login/route.rs");
        make_route(root, "apps/web/pages/index.rs"); // not an applet
        make_route(root, "packages/types/src/lib.rs"); // not an applet

        let project = scan(root).unwrap();
        assert_eq!(project.applets.len(), 1);
        assert_eq!(project.applets[0].name, "auth");
    }
}
