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

    /// Axum-style route path, e.g. "/login" or "/:userId"
    /// Relative to the applet — prefix NOT included.
    pub axum_path: String,

    /// Extracted handler type info for `sweech generate types`.
    /// None if the file couldn't be parsed (no compile-time error — just skipped).
    pub handler_info: Option<HandlerInfo>,
}

/// Type information extracted from a route.rs file by static text analysis.
/// We don't compile the file — we scan for the Handler trait impl pattern.
#[derive(Debug, Clone)]
pub struct HandlerInfo {
    /// The handler struct name, e.g. "GetProducts"
    pub handler_name: String,
    /// The Request type associated type, e.g. "GetProductsRequest"
    pub request_type: String,
    /// The Response type associated type, e.g. "GetProductsResponse"
    pub response_type: String,
    /// HTTP method extracted from `fn method()` body, e.g. "GET"
    pub method: String,
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

    /// Full route path including the applet prefix, e.g. "/products/:id"
    pub fn full_path(&self, applet_name: &str) -> String {
        if self.axum_path == "/" {
            format!("/{}", applet_name)
        } else {
            format!("/{}{}", applet_name, self.axum_path)
        }
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
    #[allow(dead_code)]
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

        let rel = path.strip_prefix(applet_path).unwrap();

        // Segments are all components except the final "route.rs"
        let path_segments: Vec<String> = rel
            .components()
            .filter_map(|c| {
                let s = c.as_os_str().to_str()?;
                if s == "route.rs" {
                    None
                } else {
                    // [param] → :param
                    Some(convert_segment(s))
                }
            })
            .collect();

        let axum_path = ScannedRoute::build_axum_path(&path_segments);

        // Try to extract handler info from the file source
        let handler_info = extract_handler_info(path).ok().flatten();

        routes.push(ScannedRoute {
            path_segments,
            file: path.to_path_buf(),
            axum_path,
            handler_info,
        });
    }

    routes.sort_by(|a, b| a.axum_path.cmp(&b.axum_path));
    Ok(routes)
}

/// Convert Next.js-style folder names to Axum path segments.
///   [userId]  → :userId
///   [...slug] → *slug
///   items     → items
fn convert_segment(s: &str) -> String {
    if let Some(inner) = s.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        if let Some(slug) = inner.strip_prefix("...") {
            format!("*{}", slug)
        } else {
            format!(":{}", inner)
        }
    } else {
        s.to_string()
    }
}

// ─── Handler info extraction (static text analysis) ──────────────────────────
//
// We scan route.rs files with simple string matching to extract the
// handler struct name, Request/Response associated types, and HTTP method.
//
// We do NOT compile the file or use a real parser. The patterns are
// stable enough that regex-free line scanning works reliably.
//
// Pattern we look for:
//
//   impl Handler for MyHandler {
//       type Request = MyRequest;
//       type Response = MyResponse;
//       fn method() -> HttpMethod { HttpMethod::Get }
//
// All of these are on their own lines with consistent formatting
// because the scaffolding generates them that way.

fn extract_handler_info(path: &Path) -> Result<Option<HandlerInfo>> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_handler_info(&content))
}

pub fn parse_handler_info(source: &str) -> Option<HandlerInfo> {
    let mut handler_name = None::<String>;
    let mut request_type = None::<String>;
    let mut response_type = None::<String>;
    let mut method = None::<String>;

    for line in source.lines() {
        let trimmed = line.trim();

        // impl Handler for XxxHandler {
        if trimmed.starts_with("impl Handler for ") {
            if let Some(rest) = trimmed.strip_prefix("impl Handler for ") {
                let name = rest.trim_end_matches('{').trim().to_string();
                if !name.contains('<') && !name.contains(' ') {
                    handler_name = Some(name);
                }
            }
        }

        // type Request = XxxRequest;
        if trimmed.starts_with("type Request = ") {
            if let Some(rest) = trimmed.strip_prefix("type Request = ") {
                let t = rest.trim_end_matches(';').trim().to_string();
                request_type = Some(t);
            }
        }

        // type Response = XxxResponse;
        if trimmed.starts_with("type Response = ") {
            if let Some(rest) = trimmed.strip_prefix("type Response = ") {
                let t = rest.trim_end_matches(';').trim().to_string();
                response_type = Some(t);
            }
        }

        // fn method() -> HttpMethod { HttpMethod::Get }
        // fn method() -> HttpMethod {
        //     HttpMethod::Post
        // }
        if trimmed.contains("HttpMethod::") {
            let verb = if trimmed.contains("HttpMethod::Get") {
                Some("GET")
            } else if trimmed.contains("HttpMethod::Post") {
                Some("POST")
            } else if trimmed.contains("HttpMethod::Put") {
                Some("PUT")
            } else if trimmed.contains("HttpMethod::Patch") {
                Some("PATCH")
            } else if trimmed.contains("HttpMethod::Delete") {
                Some("DELETE")
            } else {
                None
            };
            if let Some(v) = verb {
                method = Some(v.to_string());
            }
        }
    }

    match (handler_name, request_type, response_type, method) {
        (Some(n), Some(req), Some(res), Some(m)) => Some(HandlerInfo {
            handler_name: n,
            request_type: req,
            response_type: res,
            method: m,
        }),
        _ => None,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_conversion() {
        assert_eq!(convert_segment("[userId]"), ":userId");
        assert_eq!(convert_segment("[...slug]"), "*slug");
        assert_eq!(convert_segment("items"), "items");
    }

    #[test]
    fn axum_path_construction() {
        assert_eq!(ScannedRoute::build_axum_path(&[]), "/");
        assert_eq!(
            ScannedRoute::build_axum_path(&["login".to_string()]),
            "/login"
        );
        assert_eq!(
            ScannedRoute::build_axum_path(&[":userId".to_string(), "posts".to_string()]),
            "/:userId/posts"
        );
    }

    #[test]
    fn parse_handler_info_from_source() {
        let source = r#"
use sweech_core::prelude::*;

#[derive(Deserialize)]
pub struct GetProductsRequest {}

#[derive(Serialize)]
pub struct GetProductsResponse {
    pub products: Vec<String>,
}

pub struct GetProducts;

#[async_trait]
impl Handler for GetProducts {
    type Request = GetProductsRequest;
    type Response = GetProductsResponse;

    fn method() -> HttpMethod { HttpMethod::Get }
    fn auth() -> AuthRequirement { AuthRequirement::Public }

    async fn call(_req: Self::Request, _ctx: AppletContext) -> AppletResponse<Self::Response> {
        AppletResponse::ok(GetProductsResponse { products: vec![] })
    }
}
"#;
        let info = parse_handler_info(source).unwrap();
        assert_eq!(info.handler_name, "GetProducts");
        assert_eq!(info.request_type, "GetProductsRequest");
        assert_eq!(info.response_type, "GetProductsResponse");
        assert_eq!(info.method, "GET");
    }

    #[test]
    fn parse_handler_info_post() {
        let source = r#"
#[async_trait]
impl Handler for CreateItem {
    type Request = CreateItemRequest;
    type Response = CreateItemResponse;
    fn method() -> HttpMethod {
        HttpMethod::Post
    }
    async fn call(req: Self::Request, _ctx: AppletContext) -> AppletResponse<Self::Response> {
        AppletResponse::created(CreateItemResponse { id: "1".to_string() })
    }
}
"#;
        let info = parse_handler_info(source).unwrap();
        assert_eq!(info.method, "POST");
        assert_eq!(info.handler_name, "CreateItem");
    }

    #[test]
    fn parse_handler_info_missing_fields_returns_none() {
        let source = r#"
// A file with no handler impl
pub fn helper() {}
"#;
        assert!(parse_handler_info(source).is_none());
    }
}
