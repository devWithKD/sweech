use crate::auth::UserClaims;

// ─── What is this file? ───────────────────────────────────────────────────────
//
// AppletContext is the single struct injected into every handler call.
// The developer never constructs it — the framework builds it per-request.
//
// It gives handlers access to:
//   - The database connection pool
//   - The message queue
//   - File storage
//   - Cache
//   - The incoming request metadata (method, path, headers)
//   - The verified user identity (populated by auth middleware)
//
// ─── Current state ───────────────────────────────────────────────────────────
//
// We're building sweech-core in isolation right now — no Axum, no database,
// no actual plugins yet. So the plugin contexts (db, queue, storage, cache)
// are placeholder structs. They compile and have the right shape but don't
// do anything yet.
//
// This is intentional. We want the Handler trait and the full type system
// to be correct before we wire up real infrastructure. The ERP project
// will drive what gets filled in first.
//
// ─── Rust concept: pub struct ────────────────────────────────────────────────
//
// `pub` makes the struct visible outside this module/crate.
// Without `pub`, it's private — only usable within the same file/module.
// Same applies to fields: `pub field: Type` vs private `field: Type`.

/// Injected into every handler by the framework. Never construct this yourself.
///
/// All fields are `pub` so handlers can access them directly:
///   `ctx.db`, `ctx.user`, `ctx.request` etc.
#[derive(Debug)]
pub struct AppletContext {
    /// Database access. Use `ctx.db.query(...)` etc.
    /// Placeholder — real implementation comes when we wire Axum.
    pub db: DbContext,

    /// Message queue. Use `ctx.queue.publish(...)` etc.
    pub queue: QueueContext,

    /// File storage. Use `ctx.storage.put(...)` etc.
    pub storage: StorageContext,

    /// Cache. Use `ctx.cache.get(...)` etc.
    pub cache: CacheContext,

    /// Metadata about the incoming request.
    pub request: RequestInfo,

    /// The verified user identity, populated by auth middleware.
    ///
    /// `Option<UserClaims>` because:
    ///   - Required handlers: always Some(claims) by the time the handler runs
    ///   - Public handlers:   always None
    ///   - Optional handlers: Some or None depending on whether token was provided
    ///
    /// You should never need to check this in a Required handler —
    /// the framework guarantees it's populated. But you CAN safely unwrap
    /// if you've declared Required, knowing the framework enforced it.
    pub user: Option<UserClaims>,
}

// ─── Plugin context placeholders ─────────────────────────────────────────────
//
// These are empty structs for now. An empty struct in Rust takes zero bytes.
// `struct Foo;` is the syntax for a zero-sized type (ZST).
//
// We define them here so:
//   1. AppletContext compiles with the right shape
//   2. Handlers can be written against the correct API (ctx.db.query etc.)
//   3. When we implement real database pooling, we just fill these in —
//      all handler code stays unchanged

/// Database access context — placeholder until real pool is wired.
#[derive(Debug, Default)]
pub struct DbContext;

/// Message queue access — placeholder.
#[derive(Debug, Default)]
pub struct QueueContext;

/// File storage access — placeholder.
#[derive(Debug, Default)]
pub struct StorageContext;

/// Cache access — placeholder.
#[derive(Debug, Default)]
pub struct CacheContext;

// ─── RequestInfo ─────────────────────────────────────────────────────────────
//
// The framework extracts this from the raw HTTP request before the handler runs.
// The handler gets read-only access — it cannot mutate the incoming request.
//
// ─── Rust concept: HashMap ───────────────────────────────────────────────────
//
// std::collections::HashMap is Rust's hash map — like a JS object/Map.
// HashMap<String, String> maps string keys to string values.
// Used here for headers and path params.

use std::collections::HashMap;

/// Read-only metadata about the incoming HTTP request.
#[derive(Debug, Clone)]
pub struct RequestInfo {
    /// HTTP method as a string: "GET", "POST", "PUT", "DELETE", "PATCH"
    pub method: String,

    /// The full request path, e.g. "/products/abc-123"
    pub path: String,

    /// Parsed path parameters, e.g. { "productId": "abc-123" }
    /// Populated by the router from [param] folder names.
    pub params: HashMap<String, String>,

    /// Query string parameters, e.g. { "page": "2", "limit": "20" }
    pub query: HashMap<String, String>,

    /// Request headers as lowercase key → value.
    pub headers: HashMap<String, String>,

    /// Raw request body bytes. The framework deserializes this into
    /// the handler's Request type before calling the handler.
    pub body: Vec<u8>,
}

impl RequestInfo {
    /// Get a path parameter by name.
    ///
    /// Returns `Option<&str>` — None if the param wasn't in the path.
    ///
    /// # Example
    /// For route /products/[productId], calling `ctx.request.param("productId")`
    /// returns Some("abc-123") if the path was /products/abc-123.
    pub fn param(&self, name: &str) -> Option<&str> {
        // `.get()` on a HashMap returns Option<&V>
        // `.map(|s| s.as_str())` converts Option<&String> to Option<&str>
        self.params.get(name).map(|s| s.as_str())
    }

    /// Get a query parameter by name.
    pub fn query(&self, name: &str) -> Option<&str> {
        self.query.get(name).map(|s| s.as_str())
    }

    /// Get a header value by name (case-insensitive, stored lowercase).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_lowercase()).map(|s| s.as_str())
    }
}

impl AppletContext {
    /// Build a context for testing purposes.
    ///
    /// In production, the framework constructs AppletContext.
    /// This is only for unit tests inside handler files.
    #[cfg(test)]
    pub fn test_context(user: Option<UserClaims>) -> Self {
        Self {
            db: DbContext,
            queue: QueueContext,
            storage: StorageContext,
            cache: CacheContext,
            request: RequestInfo {
                method: "GET".to_string(),
                path: "/test".to_string(),
                params: HashMap::new(),
                query: HashMap::new(),
                headers: HashMap::new(),
                body: vec![],
            },
            user,
        }
    }
}
