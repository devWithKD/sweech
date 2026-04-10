use axum::{
    extract::Request,
    http::request::Parts,
    middleware::Next,
    response::{IntoResponse, Response},
    http::StatusCode,
    Json,
};
use serde_json::json;
use std::sync::Arc;
use sweech_core::auth::{AuthRequirement, UserClaims};

// ─── AuthValidator ────────────────────────────────────────────────────────────
//
// The one trait the developer implements to plug auth into Sweech.
// Receives request Parts (headers, uri, method — no body) because:
//   - Tokens live in headers/cookies, never the body
//   - Parts is Sync, full Request<Body> is not (body stream isn't Sync)
//   - We preserve the body for the handler
//
// Return Some(UserClaims) if authenticated, None if not.
// Never return an error — Sweech handles 401 if Required and claims are None.

#[async_trait::async_trait]
pub trait AuthValidator: Send + Sync {
    async fn validate(&self, parts: &Parts) -> Option<UserClaims>;
}

/// Shared auth state — cloned cheaply via Arc for every request.
#[derive(Clone)]
pub struct AuthState {
    pub validator: Arc<dyn AuthValidator>,
}

/// Axum middleware: runs AuthValidator and injects UserClaims into extensions.
///
/// Runs on every request. Does NOT enforce AuthRequirement — that happens
/// in route_adapter where we know the specific handler's requirement.
pub async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<AuthState>,
    req: Request,
    next: Next,
) -> Response {
    // Split the request into parts (headers/uri/method) + body.
    // Parts is Sync so we can safely hold a reference across the await.
    // Body is a stream — we put it back after the await.
    let (parts, body) = req.into_parts();

    let claims = state.validator.validate(&parts).await;

    // Reassemble the request
    let mut req = Request::from_parts(parts, body);

    // Inject claims into extensions if we got them
    if let Some(c) = claims {
        req.extensions_mut().insert(c);
    }

    next.run(req).await
}

/// Enforce AuthRequirement for a specific handler.
/// Called in route_adapter after routing but before the handler runs.
pub fn enforce_auth(
    req: &Request,
    requirement: AuthRequirement,
) -> Result<Option<UserClaims>, Response> {
    let claims = req.extensions().get::<UserClaims>().cloned();

    match requirement {
        AuthRequirement::Required => match claims {
            Some(c) => Ok(Some(c)),
            None => Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error_code": "AUTH_REQUIRED",
                    "error_message": "Authentication required"
                })),
            ).into_response()),
        },
        AuthRequirement::Optional => Ok(claims),
        AuthRequirement::Public   => Ok(None),
    }
}

pub fn forbidden_response(code: &str, message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error_code": code, "error_message": message })),
    ).into_response()
}
