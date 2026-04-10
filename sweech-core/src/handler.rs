use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    auth::AuthRequirement, context::AppletContext, error::AppletError, response::AppletResponse,
};

// ─── What is this file? ───────────────────────────────────────────────────────
//
// The Handler trait is THE core contract of Sweech.
// Every route file exports exactly one type that implements this trait.
//
// What the trait defines:
//   - Request type: what shape of data this handler accepts (deserialized from body)
//   - Response type: what shape of data it returns
//   - method(): which HTTP verb (GET, POST, etc.)
//   - auth(): who is allowed to call it (defaults to Required)
//   - guards(): extra authorization checks to run (defaults to none)
//   - call(): the actual business logic
//
// The framework (not the developer) handles:
//   - Deserializing the request body into Request type
//   - Running auth middleware
//   - Running guards
//   - Serializing the Response into JSON (or whatever the mode needs)
//
// ─── Rust concept: associated types ──────────────────────────────────────────
//
// `type Request` and `type Response` inside a trait are "associated types".
// When you implement the trait, you declare what these types actually are.
//
// Why not just use generics like Handler<Req, Res>?
// Because with associated types, there can only be ONE implementation of
// Handler per struct — you can't accidentally implement it twice with
// different type params. This matches our rule: one handler per route file.
//
// ─── Rust concept: trait bounds ──────────────────────────────────────────────
//
// `type Request: DeserializeOwned + Send`
//
// This says: whatever type you choose for Request, it must:
//   - DeserializeOwned: be deserializable from JSON (owned, not borrowed)
//   - Send: be safe to move across threads (required for async)
//
// `type Response: Serialize + Send`
//   - Serialize: be convertible to JSON
//   - Send: thread-safe

/// The contract every Sweech route must implement.
///
/// One struct, one impl, one route. The framework discovers these via
/// the folder scanner and wires them into the router automatically.
///
/// # Minimal example (GET /products)
/// ```rust
/// use sweech_core::handler::{Handler, HttpMethod};
/// use sweech_core::response::AppletResponse;
/// use sweech_core::context::AppletContext;
/// use sweech_core::auth::AuthRequirement;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Deserialize)]
/// pub struct GetProductsRequest {}   // no body for GET
///
/// #[derive(Serialize)]
/// pub struct GetProductsResponse {
///     pub products: Vec<String>,
/// }
///
/// pub struct GetProducts;
///
/// #[async_trait::async_trait]
/// impl Handler for GetProducts {
///     type Request = GetProductsRequest;
///     type Response = GetProductsResponse;
///
///     fn method() -> HttpMethod { HttpMethod::Get }
///     fn auth() -> AuthRequirement { AuthRequirement::Public }
///
///     async fn call(
///         _req: Self::Request,
///         _ctx: AppletContext,
///     ) -> AppletResponse<Self::Response> {
///         AppletResponse::ok(GetProductsResponse { products: vec![] })
///     }
/// }
/// ```
#[async_trait]
pub trait Handler: Send + Sync {
    /// The request payload type. Deserialized from the request body.
    /// Use an empty struct `{}` for GET requests with no body.
    type Request: DeserializeOwned + Send;

    /// The response payload type. Serialized to JSON in the response body.
    type Response: Serialize + Send;

    /// Which HTTP method this handler responds to.
    fn method() -> HttpMethod
    where
        Self: Sized;

    /// Who is allowed to call this handler.
    ///
    /// Default is `Required` — you must explicitly opt down to Public or Optional.
    /// This is intentional: secure by default, opt down, never opt up.
    fn auth() -> AuthRequirement
    where
        Self: Sized,
    {
        AuthRequirement::Required
    }

    /// Named guards that must pass before this handler runs.
    ///
    /// These names correspond to guards registered in the app entry point.
    /// e.g. `&["billing:active", "role:admin"]`
    ///
    /// IMPORTANT: Declaring guards() on a handler REPLACES the applet-level
    /// default — it does NOT merge. If your applet default is ["billing:active"]
    /// and you declare guards() = &["role:admin"], only "role:admin" runs.
    /// If you want both, list both explicitly.
    fn guards() -> &'static [&'static str]
    where
        Self: Sized,
    {
        &[]
    }

    /// The handler's business logic.
    ///
    /// By the time this is called:
    ///   - The request body has been deserialized into `req`
    ///   - Auth middleware has run (ctx.user is populated if Required/Optional)
    ///   - All guards have passed
    ///
    /// This function should ONLY contain business logic.
    /// Never check auth here. Never check guards here. The framework did it.
    async fn call(req: Self::Request, ctx: AppletContext) -> AppletResponse<Self::Response>;
}

// ─── HttpMethod ───────────────────────────────────────────────────────────────
//
// A simple enum for HTTP verbs. We don't use the `http` crate's Method here
// because we want our own type that the scanner and manifest system can work with
// (serialize/deserialize, match against strings from folder names, etc.)

/// HTTP method for a handler.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")] // serializes as "GET", "POST" etc.
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    /// Parse from a string — used by the CLI scanner.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "PATCH" => Some(Self::Patch),
            "DELETE" => Some(Self::Delete),
            _ => None,
        }
    }

    /// Convert to the string representation used in generated code.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

// ─── AppletError → Result shorthand ──────────────────────────────────────────
//
// Throughout handler and guard code, operations that can fail return
// `Result<T, AppletError>`. This type alias makes that less verbose.
//
// `type SweechResult<T> = Result<T, AppletError>` means you can write
// `SweechResult<Product>` instead of `Result<Product, AppletError>`.
// It's purely cosmetic — the underlying type is the same.

/// Shorthand for `Result<T, AppletError>`. Use this in handler/guard logic.
pub type SweechResult<T> = Result<T, AppletError>;

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_method_parse() {
        assert_eq!(HttpMethod::from_str("GET"), Some(HttpMethod::Get));
        assert_eq!(HttpMethod::from_str("post"), Some(HttpMethod::Post));
        assert_eq!(HttpMethod::from_str("PATCH"), Some(HttpMethod::Patch));
        assert_eq!(HttpMethod::from_str("bad"), None);
    }

    #[test]
    fn http_method_as_str() {
        assert_eq!(HttpMethod::Delete.as_str(), "DELETE");
        assert_eq!(HttpMethod::Put.as_str(), "PUT");
    }
}
