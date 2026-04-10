use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use serde_json::json;
use sweech_core::{
    error::Guard,
    handler::{Handler, HttpMethod},
};

use crate::{
    extractor::{build_context, IncomingRequest},
    middleware::{enforce_auth, AuthState},
};

/// Shared state injected into every route handler.
#[derive(Clone)]
pub struct AppState {
    pub auth: AuthState,
    pub guards: Arc<Vec<Arc<dyn GuardObject>>>,
}

// ─── GuardObject ──────────────────────────────────────────────────────────────
//
// Guard::name() is a static method — can't be called on a trait object.
// GuardObject is an object-safe wrapper that adds name_str(&self).
// Blanket impl means: implement Guard → get GuardObject for free.

#[async_trait::async_trait]
pub trait GuardObject: Send + Sync {
    fn name_str(&self) -> &'static str;
    async fn check(&self, ctx: &sweech_core::AppletContext) -> Result<(), sweech_core::AppletError>;
}

#[async_trait::async_trait]
impl<G> GuardObject for G
where
    G: Guard + 'static,
{
    fn name_str(&self) -> &'static str {
        G::name()
    }

    async fn check(&self, ctx: &sweech_core::AppletContext) -> Result<(), sweech_core::AppletError> {
        Guard::check(self, ctx).await
    }
}

// ─── route_adapter ────────────────────────────────────────────────────────────

async fn route_adapter<H>(State(state): State<AppState>, req: Request) -> Response
where
    H: Handler + Send + Sync + 'static,
{
    // 1. Enforce auth
    let user = match enforce_auth(&req, H::auth()) {
        Ok(u) => u,
        Err(resp) => return resp,
    };

    // 2. Extract request parts before consuming body
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();
    let path = uri.path().to_string();

    let query: HashMap<String, String> = uri
        .query()
        .map(|q| {
            q.split('&')
                .filter_map(|pair| {
                    let mut parts = pair.splitn(2, '=');
                    let key = parts.next()?.to_string();
                    let val = parts.next().unwrap_or("").to_string();
                    if key.is_empty() { None } else { Some((key, val)) }
                })
                .collect()
        })
        .unwrap_or_default();

    let body: Bytes = match axum::body::to_bytes(req.into_body(), 4 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, Json(json!({
                "error_code": "BODY_READ_ERROR",
                "error_message": "Failed to read request body"
            }))).into_response();
        }
    };

    // 3. Build AppletContext
    let incoming = IncomingRequest {
        method,
        path,
        params: HashMap::new(), // path params: future — needs axum Path extractor integration
        query,
        headers,
        body: body.clone(),
        user,
    };
    let ctx = build_context(incoming);

    // 4. Deserialize body into H::Request
    let slice: &[u8] = if body.is_empty() { b"{}" } else { &body };
    let request_data: H::Request = match serde_json::from_slice(slice) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(json!({
                "error_code": "INVALID_REQUEST_BODY",
                "error_message": format!("Failed to parse request body: {}", e)
            }))).into_response();
        }
    };

    // 5. Run guards
    for &guard_name in H::guards() {
        match state.guards.iter().find(|g| g.name_str() == guard_name) {
            None => {
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
                    "error_code": "GUARD_NOT_REGISTERED",
                    "error_message": format!("Guard '{}' declared on handler but not registered", guard_name)
                }))).into_response();
            }
            Some(guard) => {
                if let Err(err) = guard.check(&ctx).await {
                    return applet_error_to_response(err);
                }
            }
        }
    }

    // 6. Call handler and convert response
    let response = H::call(request_data, ctx).await;
    applet_response_to_axum(response)
}

// ─── Response helpers ─────────────────────────────────────────────────────────

fn applet_response_to_axum<T: serde::Serialize>(resp: sweech_core::AppletResponse<T>) -> Response {
    let status = StatusCode::from_u16(resp.status.as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    if resp.status.is_success() {
        match resp.body {
            Some(data) => {
                let body = json!({ "data": serde_json::to_value(data).unwrap_or(json!(null)) });
                (status, Json(body)).into_response()
            }
            None => status.into_response(),
        }
    } else {
        (status, Json(json!({
            "error_code": resp.error_code,
            "error_message": resp.error_message,
        }))).into_response()
    }
}

fn applet_error_to_response(err: sweech_core::AppletError) -> Response {
    applet_response_to_axum(err.into_response::<()>())
}

// ─── AppletRouter ─────────────────────────────────────────────────────────────

pub struct AppletRouter {
    inner: Router<AppState>,
}

impl AppletRouter {
    pub fn new() -> Self {
        Self { inner: Router::new() }
    }

    pub fn register<H>(self, path: &str) -> Self
    where
        H: Handler + Send + Sync + 'static,
    {
        let handler = |state: State<AppState>, req: Request| async move {
            route_adapter::<H>(state, req).await
        };

        let route = match H::method() {
            HttpMethod::Get    => self.inner.route(path, get(handler)),
            HttpMethod::Post   => self.inner.route(path, post(handler)),
            HttpMethod::Put    => self.inner.route(path, put(handler)),
            HttpMethod::Patch  => self.inner.route(path, patch(handler)),
            HttpMethod::Delete => self.inner.route(path, delete(handler)),
        };

        Self { inner: route }
    }

    pub fn build(self, state: AppState) -> Router {
        self.inner.with_state(state)
    }
}

impl Default for AppletRouter {
    fn default() -> Self { Self::new() }
}
