use http::StatusCode;
use serde::Serialize;

// ─── What is this file? ───────────────────────────────────────────────────────
//
// Every handler in Sweech must return AppletResponse<T>.
// This is the one return type the framework owns.
// The developer never constructs a raw HTTP response — they call helpers like
// AppletResponse::ok(data) or AppletResponse::not_found("message") and Sweech
// maps that to the correct wire format for the deployment mode.
//
// ─── Rust concept: Generics (<T>) ────────────────────────────────────────────
//
// AppletResponse<T> means "an AppletResponse that carries some data of type T".
// T is a placeholder — the actual type is filled in by whoever uses it.
// Example:
//   AppletResponse<ProductResponse>   — carries a ProductResponse in the body
//   AppletResponse<()>                — carries nothing (unit type, like void)
//
// The constraint `T: Serialize` means "T must be serializable to JSON".
// Rust enforces this at compile time — if you try to return a type that
// can't be serialized, the code won't compile.
//
// ─── Rust concept: #[derive(...)] ────────────────────────────────────────────
//
// This is a macro that auto-generates code for you.
// `#[derive(Debug)]` gives you the ability to print the struct with {:?}.
// Instead of writing `fn fmt(...)` by hand, the compiler writes it for you.

/// The single return type for every Sweech handler.
///
/// `T` is the type of the response body. It must implement `Serialize`
/// so Sweech can turn it into JSON (or whatever the deployment mode needs).
#[derive(Debug)]
pub struct AppletResponse<T: Serialize> {
    /// HTTP status code — 200, 201, 400, etc.
    pub status: StatusCode,

    /// The actual body payload. `Option<T>` means it can be Some(data) or None.
    /// None is used for 204 No Content responses.
    pub body: Option<T>,

    /// Optional machine-readable error code, e.g. "PRODUCT_NOT_FOUND".
    /// Only present on error responses.
    pub error_code: Option<String>,

    /// Optional human-readable error message.
    /// Only present on error responses.
    pub error_message: Option<String>,
}

// ─── Rust concept: impl block ────────────────────────────────────────────────
//
// `impl AppletResponse<T>` is where you attach functions (methods) to a type.
// It's similar to a class body in other languages, but Rust separates
// the struct definition (the shape) from the impl (the behaviour).
//
// The `where T: Serialize` line is a "where clause" — same as <T: Serialize>
// but written separately for readability when there are multiple constraints.

impl<T> AppletResponse<T>
where
    T: Serialize,
{
    // ── Success variants ──────────────────────────────────────────────────

    /// 200 OK — standard success with a body.
    pub fn ok(data: T) -> Self {
        Self {
            status: StatusCode::OK, // 200
            body: Some(data),
            error_code: None,
            error_message: None,
        }
    }

    /// 201 Created — resource was successfully created.
    pub fn created(data: T) -> Self {
        Self {
            status: StatusCode::CREATED, // 201
            body: Some(data),
            error_code: None,
            error_message: None,
        }
    }

    // ── Error variants ────────────────────────────────────────────────────
    //
    // Error variants take `code` and `message` instead of data.
    // They use AppletResponse<T> with body: None.
    //
    // `impl` (with lowercase i) here means "any type that implements
    // the Into<String> trait" — so you can pass a &str or a String,
    // and Rust will convert it automatically. No overloading needed.

    /// 400 Bad Request — client sent invalid data.
    pub fn bad_request(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::error(StatusCode::BAD_REQUEST, code, message)
    }

    /// 401 Unauthorized — no valid authentication token.
    pub fn unauthorized(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::error(StatusCode::UNAUTHORIZED, code, message)
    }

    /// 403 Forbidden — authenticated but not allowed.
    pub fn forbidden(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::error(StatusCode::FORBIDDEN, code, message)
    }

    /// 404 Not Found — resource doesn't exist.
    pub fn not_found(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::error(StatusCode::NOT_FOUND, code, message)
    }

    /// 409 Conflict — state conflict (e.g. duplicate resource).
    pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::error(StatusCode::CONFLICT, code, message)
    }

    /// 500 Internal Server Error — something broke on our side.
    pub fn internal(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::error(StatusCode::INTERNAL_SERVER_ERROR, code, message)
    }

    // ── Private helper ────────────────────────────────────────────────────
    //
    // `fn error` is `pub(self)` by default (private to this module).
    // We don't want callers constructing arbitrary status codes directly.
    // They must use the named helpers above.

    fn error(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            body: None,
            error_code: Some(code.into()),
            error_message: Some(message.into()),
        }
    }
}

// ─── Special case: No Content (204) ──────────────────────────────────────────
//
// 204 has no body by definition, so we implement it separately on
// AppletResponse<()> — the unit type () means "nothing".
// This way the type system enforces that you can't accidentally put
// data in a no-content response.

impl AppletResponse<()> {
    /// 204 No Content — success, nothing to return.
    pub fn no_content() -> Self {
        Self {
            status: StatusCode::NO_CONTENT, // 204
            body: None,
            error_code: None,
            error_message: None,
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────
//
// `#[cfg(test)]` means this module only compiles when running `cargo test`.
// It won't be included in the final binary. This is standard Rust practice —
// tests live right next to the code they test.

#[cfg(test)]
mod tests {
    use super::*; // bring everything from this file into scope

    // `#[test]` marks a function as a test case.
    // `cargo test` finds and runs all of them automatically.

    #[test]
    fn ok_response_has_correct_status() {
        let resp = AppletResponse::ok(42u32);
        assert_eq!(resp.status, StatusCode::OK);
        assert!(resp.body.is_some());
        assert!(resp.error_code.is_none());
    }

    #[test]
    fn not_found_has_no_body() {
        let resp = AppletResponse::<()>::not_found("USER_NOT_FOUND", "User does not exist");
        assert_eq!(resp.status, StatusCode::NOT_FOUND);
        assert!(resp.body.is_none());
        assert_eq!(resp.error_code.as_deref(), Some("USER_NOT_FOUND"));
    }

    #[test]
    fn no_content_is_204() {
        let resp = AppletResponse::no_content();
        assert_eq!(resp.status, StatusCode::NO_CONTENT);
        assert!(resp.body.is_none());
    }
}
