// ─── Scaffolding integration tests ───────────────────────────────────────────
//
// These tests invoke the real `sweech` binary via std::process::Command.
// This tests the full stack: argument parsing, command dispatch, file creation.
//
// Run with: cargo test --test scaffolding
// (cargo test builds the binary first, then runs the integration tests)

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

// ── Binary path helper ────────────────────────────────────────────────────────

fn sweech_bin() -> std::path::PathBuf {
    // Integration test exe is at: target/debug/deps/scaffolding-<hash>
    // The binary is at:           target/debug/sweech
    let exe = std::env::current_exe().unwrap();
    let deps_dir = exe.parent().unwrap();
    let debug_dir = deps_dir.parent().unwrap();
    debug_dir.join("sweech")
}

fn sweech(args: &[&str], cwd: &Path) -> std::process::Output {
    Command::new(sweech_bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "failed to spawn sweech binary: {e}\nExpected at: {:?}",
                sweech_bin()
            )
        })
}

fn ok(out: &std::process::Output) {
    if !out.status.success() {
        panic!(
            "sweech exited {}\nstdout:\n{}\nstderr:\n{}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn make_minimal_project(tmp: &TempDir) {
    fs::write(
        tmp.path().join("sweech.manifest.toml"),
        "[project]\nname = \"testapp\"\n\n[build]\nmode = \"monolith\"\n",
    )
    .unwrap();
}

fn make_project_with_handler(tmp: &TempDir) {
    fs::write(
        tmp.path().join("sweech.manifest.toml"),
        r#"[project]
name = "testapp"

[build]
mode = "monolith"

[[applet]]
name = "products"
path = "products.applet"
"#,
    )
    .unwrap();

    let dir = tmp.path().join("products.applet").join("items");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("route.rs"),
        r#"use sweech_core::prelude::*;

#[derive(Deserialize)]
pub struct GetItemsRequest {}

#[derive(Serialize)]
pub struct GetItemsResponse { pub items: Vec<String> }

pub struct GetItems;

#[async_trait]
impl Handler for GetItems {
    type Request = GetItemsRequest;
    type Response = GetItemsResponse;
    fn method() -> HttpMethod { HttpMethod::Get }
    fn auth() -> AuthRequirement { AuthRequirement::Public }
    async fn call(_req: Self::Request, _ctx: AppletContext) -> AppletResponse<Self::Response> {
        AppletResponse::ok(GetItemsResponse { items: vec![] })
    }
}
"#,
    )
    .unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// sweech init
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn init_creates_expected_files() {
    let tmp = TempDir::new().unwrap();
    ok(&sweech(&["init", "my-app"], tmp.path()));

    let root = tmp.path().join("my-app");
    assert!(root.join("sweech.manifest.toml").exists());
    assert!(root.join("Cargo.toml").exists());
    assert!(root.join("sweech.app.rs").exists());
    assert!(root.join("hello.applet").join("route.rs").exists());
    assert!(root.join(".gitignore").exists());

    // No separate app crate — single merged Cargo.toml at root
    assert!(
        !root.join("my-app-app").exists(),
        "app crate dir should not exist"
    );

    let cargo = read(&root.join("Cargo.toml"));
    assert!(
        cargo.contains("[workspace]"),
        "Cargo.toml must have [workspace]"
    );
    assert!(
        cargo.contains("[package]"),
        "Cargo.toml must have [package]"
    );
    assert!(
        cargo.contains("path = \"sweech.app.rs\""),
        "[[bin]] path must point at sweech.app.rs"
    );
    assert!(cargo.contains("resolver = \"3\""));

    let manifest = read(&root.join("sweech.manifest.toml"));
    assert!(manifest.contains("name = \"my-app\""));
    assert!(manifest.contains("mode = \"monolith\""));
    assert!(manifest.contains("name = \"hello\""));

    let app = read(&root.join("sweech.app.rs"));
    assert!(app.contains("#[path = \"./hello.applet/route.rs\"]"));
    assert!(app.contains("MyAppAuth"));

    let hello = read(&root.join("hello.applet").join("route.rs"));
    assert!(hello.contains("impl Handler for Hello"));
    assert!(hello.contains("AuthRequirement::Public"));
}

#[test]
fn init_rejects_existing_directory() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("clash")).unwrap();
    let out = sweech(&["init", "clash"], tmp.path());
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("already exists"));
}

// ─────────────────────────────────────────────────────────────────────────────
// sweech add applet
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn add_applet_creates_directory_and_updates_manifest() {
    let tmp = TempDir::new().unwrap();
    make_minimal_project(&tmp);
    ok(&sweech(&["add", "applet", "inventory"], tmp.path()));

    let root = tmp.path();
    assert!(root.join("inventory.applet").join("route.rs").exists());

    let src = read(&root.join("inventory.applet").join("route.rs"));
    assert!(src.contains("impl Handler for Inventory"));
    assert!(src.contains("type Request = InventoryRequest"));
    assert!(src.contains("AuthRequirement::Required"));

    let manifest = read(&root.join("sweech.manifest.toml"));
    assert!(manifest.contains("name = \"inventory\""));
    assert!(manifest.contains("path = \"inventory.applet\""));
}

#[test]
fn add_applet_public_auth() {
    let tmp = TempDir::new().unwrap();
    make_minimal_project(&tmp);
    ok(&sweech(
        &["add", "applet", "catalog", "--auth", "public"],
        tmp.path(),
    ));

    let src = read(&tmp.path().join("catalog.applet").join("route.rs"));
    assert!(src.contains("AuthRequirement::Public"));
}

#[test]
fn add_applet_rejects_duplicate() {
    let tmp = TempDir::new().unwrap();
    make_minimal_project(&tmp);
    ok(&sweech(&["add", "applet", "auth"], tmp.path()));
    let out = sweech(&["add", "applet", "auth"], tmp.path());
    assert!(!out.status.success());
}

// ─────────────────────────────────────────────────────────────────────────────
// sweech add handler
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn add_handler_nested() {
    let tmp = TempDir::new().unwrap();
    make_minimal_project(&tmp);
    fs::create_dir(tmp.path().join("inventory.applet")).unwrap();

    ok(&sweech(
        &["add", "handler", "inventory/items", "--method", "GET"],
        tmp.path(),
    ));

    let route = tmp
        .path()
        .join("inventory.applet")
        .join("items")
        .join("route.rs");
    assert!(route.exists());
    assert!(read(&route).contains("HttpMethod::Get"));
}

#[test]
fn add_handler_dynamic_segment() {
    let tmp = TempDir::new().unwrap();
    make_minimal_project(&tmp);
    fs::create_dir_all(tmp.path().join("inventory.applet").join("items")).unwrap();

    ok(&sweech(
        &[
            "add",
            "handler",
            "inventory/items/[itemId]",
            "--method",
            "DELETE",
        ],
        tmp.path(),
    ));

    let route = tmp
        .path()
        .join("inventory.applet")
        .join("items")
        .join("[itemId]")
        .join("route.rs");
    assert!(route.exists());
    assert!(read(&route).contains("HttpMethod::Delete"));
}

#[test]
fn add_handler_fails_no_applet_dir() {
    let tmp = TempDir::new().unwrap();
    make_minimal_project(&tmp);
    let out = sweech(&["add", "handler", "ghost/items"], tmp.path());
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not found"));
}

// ─────────────────────────────────────────────────────────────────────────────
// sweech generate types
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn generate_types_creates_index_ts() {
    let tmp = TempDir::new().unwrap();
    make_project_with_handler(&tmp);
    ok(&sweech(&["generate", "types"], tmp.path()));

    let ts = tmp.path().join("packages").join("types").join("index.ts");
    assert!(ts.exists());
    let src = read(&ts);

    assert!(src.contains("export interface GetItemsRequest"));
    assert!(src.contains("export interface GetItemsResponse"));
    assert!(src.contains("export const API"));
    assert!(src.contains("products:"));
    assert!(src.contains("getItems:"));
    assert!(src.contains("method: \"GET\" as const"));
    assert!(src.contains("path: \"/products/items\" as const"));
    assert!(src.contains("satisfies ApiRegistry"));
    assert!(src.contains("export type RequestOf<"));
    assert!(src.contains("export type ResponseOf<"));
}

#[test]
fn generate_types_preserves_edited_interfaces() {
    let tmp = TempDir::new().unwrap();
    make_project_with_handler(&tmp);
    ok(&sweech(&["generate", "types"], tmp.path()));

    let ts = tmp.path().join("packages").join("types").join("index.ts");
    let first = read(&ts);
    let edited = first.replace(
        "export interface GetItemsResponse {\n  // TODO: Add response fields\n}",
        "export interface GetItemsResponse {\n  items: string[];\n  total: number;\n}",
    );
    fs::write(&ts, &edited).unwrap();

    ok(&sweech(&["generate", "types"], tmp.path()));

    let second = read(&ts);
    assert!(second.contains("items: string[];"), "edited field lost");
    assert!(second.contains("total: number;"), "edited field lost");
    assert!(second.contains("export const API"));
}

// ─────────────────────────────────────────────────────────────────────────────
// sweech generate dockerfile
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn generate_dockerfile_rust_only() {
    let tmp = TempDir::new().unwrap();
    make_minimal_project(&tmp);
    ok(&sweech(&["generate", "dockerfile"], tmp.path()));

    let src = read(&tmp.path().join("Dockerfile"));
    assert!(src.contains("FROM rust:"));
    assert!(src.contains("FROM debian:"));
    assert!(src.contains("cargo build --release"));
    assert!(tmp.path().join(".dockerignore").exists());
}

#[test]
fn generate_dockerfile_embedded_frontend() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("sweech.manifest.toml"),
        r#"[project]
name = "testapp"
[build]
mode = "monolith"
port = 3000
[[frontend]]
name = "web"
path = "apps/web"
framework = "next"
deploy_target = "^build"
serve = "embedded"
build_command = "npm run build"
"#,
    )
    .unwrap();

    ok(&sweech(&["generate", "dockerfile"], tmp.path()));

    let src = read(&tmp.path().join("Dockerfile"));
    assert!(src.contains("FROM node:"));
    assert!(src.contains("npm run build"));
    assert!(src.contains("FROM rust:"));
    assert!(src.contains("--from=frontend-builder"));
}

// ─────────────────────────────────────────────────────────────────────────────
// sweech generate compose
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn generate_compose_valid() {
    let tmp = TempDir::new().unwrap();
    make_minimal_project(&tmp);
    ok(&sweech(&["generate", "compose"], tmp.path()));

    let src = read(&tmp.path().join("docker-compose.yml"));
    assert!(src.contains("services:"));
    assert!(src.contains("testapp:"));
    assert!(src.contains("build: ."));
    assert!(src.contains("3000:3000"));
    assert!(src.contains("DATABASE_URL"));
}

// ─────────────────────────────────────────────────────────────────────────────
// sweech check
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn check_clean_exits_zero() {
    let tmp = TempDir::new().unwrap();
    make_minimal_project(&tmp);
    ok(&sweech(&["check"], tmp.path()));
}

#[test]
fn check_error_exits_nonzero() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("sweech.manifest.toml"),
        r#"[project]
name = "testapp"
[build]
mode = "monolith"
[[frontend]]
name = "mobile"
path = "apps/mobile"
framework = "expo"
deploy_target = "eas"
serve = "embedded"
"#,
    )
    .unwrap();

    let out = sweech(&["check"], tmp.path());
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("EMBEDDED_ON_MOBILE"));
}
