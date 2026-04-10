use crate::response::AppletResponse;

// ─── What is this file? ───────────────────────────────────────────────────────
//
// Two things:
//   1. AppletError — the error type guards and handlers return when something fails
//   2. Guard trait — the contract every authorization check must implement
//
// ─── Rust concept: enum with data ────────────────────────────────────────────
//
// Unlike simple enums (Required/Public/Optional), AppletError variants
// carry data. Each variant is essentially its own mini-struct.
//
// This is one of Rust's most powerful features. Instead of:
//   throw new Error({ code: "...", message: "..." })
//
// You write:
//   return Err(AppletError::Forbidden { code: "...", message: "..." })
//
// And the type system knows EXACTLY which kind of error it is.
// No instanceof checks. No duck typing. Pattern matching handles it perfectly.

/// An error that can be returned from a guard or handler.
///
/// Sweech maps this to the correct AppletResponse automatically.
/// You never construct a raw HTTP response in error paths.
#[derive(Debug)]
pub enum AppletError {
    /// The request is malformed or invalid — 400.
    BadRequest {
        code: String,
        message: String,
    },

    /// Authentication required but missing/invalid — 401.
    Unauthorized {
        code: String,
        message: String,
    },

    /// Authenticated but not permitted — 403.
    Forbidden {
        code: String,
        message: String,
    },

    /// Resource not found — 404.
    NotFound {
        code: String,
        message: String,
    },

    /// State conflict — 409.
    Conflict {
        code: String,
        message: String,
    },

    /// Something broke on our side — 500.
    Internal {
        code: String,
        message: String,
    },
}

impl AppletError {
    // ── Convenience constructors ──────────────────────────────────────────
    //
    // These mirror AppletResponse's helpers so the call sites are consistent.
    // `impl Into<String>` — explained in response.rs — accepts &str or String.

    pub fn not_found(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::NotFound { code: code.into(), message: message.into() }
    }

    pub fn forbidden(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Forbidden { code: code.into(), message: message.into() }
    }

    pub fn unauthorized(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Unauthorized { code: code.into(), message: message.into() }
    }

    pub fn bad_request(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::BadRequest { code: code.into(), message: message.into() }
    }

    pub fn internal(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Internal { code: code.into(), message: message.into() }
    }

    /// Convert this error into an AppletResponse<T>.
    ///
    /// Called by the framework when a guard returns Err(AppletError).
    /// The handler never sees this — the pipeline short-circuits before call().
    ///
    /// `T: serde::Serialize` is needed because AppletResponse<T> requires it,
    /// even though error responses have no body (body: None).
    pub fn into_response<T: serde::Serialize>(self) -> AppletResponse<T> {
        // ─── Rust concept: match ──────────────────────────────────────────
        //
        // `match` is like switch/case but exhaustive — Rust forces you to
        // handle every variant. If you add a new AppletError variant and
        // forget to handle it here, the code won't compile.
        //
        // The `{ code, message }` syntax is destructuring — we pull the
        // fields out of the enum variant directly into local variables.

        match self {
            Self::BadRequest   { code, message } => AppletResponse::bad_request(code, message),
            Self::Unauthorized { code, message } => AppletResponse::unauthorized(code, message),
            Self::Forbidden    { code, message } => AppletResponse::forbidden(code, message),
            Self::NotFound     { code, message } => AppletResponse::not_found(code, message),
            Self::Conflict     { code, message } => AppletResponse::conflict(code, message),
            Self::Internal     { code, message } => AppletResponse::internal(code, message),
        }
    }
}

// ─── Guard trait ─────────────────────────────────────────────────────────────
//
// ─── Rust concept: trait ─────────────────────────────────────────────────────
//
// A trait is a contract — it defines what a type CAN DO, not what it IS.
// Similar to an interface in TypeScript/Java, but more powerful.
//
// Any struct that `impl Guard for MyGuard` promises to:
//   1. Have a `name()` function returning a &'static str
//   2. Have an async `check()` function that returns Ok(()) or Err(AppletError)
//
// The framework calls these at runtime without knowing the concrete type.
//
// ─── Rust concept: &'static str ──────────────────────────────────────────────
//
// `'static` is a lifetime annotation — it means "this string lives forever
// (for the entire duration of the program)". String literals like "admin"
// are always 'static. This is just Rust saying "give me a constant name,
// not something that might be dropped mid-execution".
//
// ─── Rust concept: async-trait ───────────────────────────────────────────────
//
// Rust's async support in traits is not yet fully stable without a helper.
// The `#[async_trait]` macro from the async-trait crate rewrites async fn
// in traits into something the compiler accepts. It's a temporary workaround
// that is transparent to you — you just write async fn as normal.
//
// ─── Rust concept: Result<T, E> ──────────────────────────────────────────────
//
// Like Option but for operations that can fail with a reason.
//   Ok(value)  — success, here's the value
//   Err(error) — failure, here's why
//
// `Result<(), AppletError>` means:
//   Ok(())           — guard passed, nothing to return
//   Err(AppletError) — guard failed, here's the error to send back
//
// The `?` operator (you'll see it in handler code) is sugar for:
//   "if this is Err, return the error immediately; if Ok, unwrap the value"

use async_trait::async_trait;

#[async_trait]
pub trait Guard: Send + Sync {
    // ─── Rust concept: Send + Sync ────────────────────────────────────────
    //
    // These are marker traits that tell the compiler this type is safe to
    // use across threads.
    //   Send  — the type can be moved to another thread
    //   Sync  — the type can be shared between threads via a reference
    //
    // We need these because Sweech is async and handlers run concurrently.
    // Any guard the framework holds onto must be thread-safe.
    // Rust enforces this at compile time — no data races, ever.

    /// The name this guard is registered under in the manifest.
    /// e.g. "billing:active", "role:admin"
    ///
    /// Static method — used at registration time.
    fn name() -> &'static str where Self: Sized;

    /// Instance version of name() — callable on trait objects (Box<dyn Guard>).
    ///
    /// When you implement Guard, write:
    ///   fn instance_name(&self) -> &'static str { Self::name() }
    fn instance_name(&self) -> &'static str;

    /// Run the authorization check.
    ///
    /// Receives the full AppletContext (user is already populated).
    /// DB access is fine here — this is Layer 2 (stateful).
    ///
    /// Return `Ok(())` to allow the request through.
    /// Return `Err(AppletError)` to reject — the framework sends the error response.
    async fn check(&self, ctx: &crate::context::AppletContext) -> Result<(), AppletError>;
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    #[test]
    fn error_converts_to_correct_status() {
        let err = AppletError::not_found("ITEM_NOT_FOUND", "Item does not exist");
        let resp = err.into_response::<()>();
        assert_eq!(resp.status, StatusCode::NOT_FOUND);
        assert_eq!(resp.error_code.as_deref(), Some("ITEM_NOT_FOUND"));
    }

    #[test]
    fn forbidden_error_converts() {
        let err = AppletError::forbidden("INSUFFICIENT_ROLE", "Admin access required");
        let resp = err.into_response::<()>();
        assert_eq!(resp.status, StatusCode::FORBIDDEN);
    }
}
