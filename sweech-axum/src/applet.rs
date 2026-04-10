use axum::Router;
use std::sync::Arc;
use sweech_core::auth::AuthRequirement;

use crate::router::{AppState, AppletRouter, GuardObject};

// ─── What is this file? ───────────────────────────────────────────────────────
//
// An "applet" in Sweech is one deployable domain — a folder ending in .applet.
// In monolith mode, all applets get merged into one Axum Router.
// This file defines:
//
//   AppletConfig  — the per-applet settings from the manifest
//                   (default auth, default guards, stateful flag)
//
//   Applet        — one assembled applet: config + its routes as a Router
//
//   SweechApp     — the top-level assembler that merges all applets
//                   into a single monolith router
//
// ─── Why applet-level defaults matter ────────────────────────────────────────
//
// The manifest can declare:
//   [[applet]]
//   name = "products"
//   auth = "required"
//   guards = ["billing:active"]
//
// Every handler in that applet inherits those defaults UNLESS the handler
// explicitly overrides them. Overriding REPLACES — never merges.
//
// But here's the thing: handler defaults are baked into the Handler trait
// at compile time (fn auth(), fn guards()). So applet-level defaults
// are effectively the compile-time defaults — Required auth, no guards.
//
// The manifest override path (reading TOML and changing the default at runtime)
// is a CLI concern — the scanner reads the manifest and generates code that
// declares the right auth/guards on each handler. The framework itself
// just executes whatever the handlers declare.
//
// AppletConfig here captures what the assembler needs to know:
//   - The route prefix (derived from the folder name)
//   - Whether this applet is stateful (affects serverless mode later)

/// Per-applet configuration, derived from the manifest.
#[derive(Debug, Clone)]
pub struct AppletConfig {
    /// The applet's name, e.g. "auth", "products".
    /// Becomes the route prefix: /auth, /products.
    pub name: String,

    /// Whether this applet is stateful (runs as a persistent container
    /// in serverless mode instead of a function).
    pub stateful: bool,

    /// Default auth requirement for handlers in this applet.
    /// Handlers that don't declare auth() inherit this.
    /// Currently enforced at code-gen time by the CLI scanner.
    pub default_auth: AuthRequirement,

    /// Default guards for handlers in this applet.
    /// Handlers that don't declare guards() inherit these.
    /// Currently enforced at code-gen time by the CLI scanner.
    pub default_guards: Vec<String>,
}

impl AppletConfig {
    /// Create a config with sensible defaults.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            stateful: false,
            default_auth: AuthRequirement::Required,
            default_guards: vec![],
        }
    }

    pub fn stateful(mut self, stateful: bool) -> Self {
        self.stateful = stateful;
        self
    }

    pub fn default_auth(mut self, auth: AuthRequirement) -> Self {
        self.default_auth = auth;
        self
    }

    pub fn default_guards(mut self, guards: Vec<String>) -> Self {
        self.default_guards = guards;
        self
    }
}

/// One assembled applet — its config and its routes as a built Router.
///
/// The Router inside already has AppState attached (via `.build(state)`).
/// It's ready to be nested into the monolith at its prefix path.
pub struct Applet {
    pub config: AppletConfig,
    /// The routes for this applet, fully wired with state.
    pub router: Router,
}

impl Applet {
    /// Create an applet from its config and a populated AppletRouter.
    ///
    /// # Example
    /// ```rust,ignore
    /// let auth_applet = Applet::new(
    ///     AppletConfig::new("auth").default_auth(AuthRequirement::Public),
    ///     AppletRouter::new()
    ///         .register::<LoginHandler>("/login")
    ///         .register::<RegisterHandler>("/register"),
    ///     state.clone(),
    /// );
    /// ```
    pub fn new(config: AppletConfig, router: AppletRouter, state: AppState) -> Self {
        Self {
            router: router.build(state),
            config,
        }
    }

    /// The URL prefix this applet mounts at.
    /// "auth" → "/auth", "products" → "/products"
    pub fn prefix(&self) -> String {
        format!("/{}", self.config.name)
    }
}

// ─── SweechApp — the monolith assembler ──────────────────────────────────────
//
// SweechApp takes multiple Applets and merges them into one Axum Router.
// This is "monolith mode" — one binary, one port, all applets inside.
//
// ─── Rust concept: builder pattern ───────────────────────────────────────────
//
// Builder pattern in Rust:
//   - Methods take `self` (not `&mut self`) and return `Self`
//   - This means each call CONSUMES the builder and returns a new one
//   - Forces you to use the returned value — you can't accidentally
//     discard the updated builder
//   - The chain reads naturally: SweechApp::new().applet(a).applet(b).build()
//
// Compare with `&mut self` which would be:
//   let mut app = SweechApp::new();
//   app.applet(a);   ← easy to forget to use the return value
//   app.build()
//
// The `self` version makes mistakes impossible at compile time.

/// Assembles multiple applets into a single monolith Router.
///
/// # Usage
/// ```rust,ignore
/// let app = SweechApp::new()
///     .applet(auth_applet)
///     .applet(products_applet)
///     .build();
///
/// axum::serve(listener, app).await?;
/// ```
pub struct SweechApp {
    applets: Vec<Applet>,
}

impl SweechApp {
    pub fn new() -> Self {
        Self { applets: vec![] }
    }

    /// Add an applet to the monolith.
    pub fn applet(mut self, applet: Applet) -> Self {
        self.applets.push(applet);
        self
    }

    /// Merge all applets into one Router, each nested at its prefix.
    ///
    /// ─── Rust concept: fold ───────────────────────────────────────────────
    ///
    /// `Iterator::fold(initial, |acc, item| ...)` is reduce/accumulate.
    /// We start with an empty Router and fold each applet in, nesting it
    /// at its prefix path using Axum's `.nest()`.
    ///
    /// `.nest("/auth", auth_router)` means all routes inside auth_router
    /// get the "/auth" prefix prepended. A route registered as "/login"
    /// inside becomes "/auth/login" from the outside.
    ///
    /// This is exactly what we want — handlers register their own path
    /// segments, the applet prefix is added by the assembler.
    pub fn build(self) -> Router {
        self.applets
            .into_iter()
            .fold(Router::new(), |merged, applet| {
                let prefix = applet.prefix();
                merged.nest(&prefix, applet.router)
            })
    }
}

impl Default for SweechApp {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applet_prefix_from_name() {
        let config = AppletConfig::new("products");
        let applet_stub = Applet {
            config,
            router: Router::new(),
        };
        assert_eq!(applet_stub.prefix(), "/products");
    }

    #[test]
    fn applet_config_builder() {
        let config = AppletConfig::new("scheduler")
            .stateful(true)
            .default_auth(AuthRequirement::Public);

        assert_eq!(config.name, "scheduler");
        assert!(config.stateful);
        assert_eq!(config.default_auth, AuthRequirement::Public);
    }
}
