use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    extract::Request,
    http::{Method, StatusCode, header, request::Parts},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sweech_axum::{
    middleware::{AuthState, AuthValidator, auth_middleware},
    router::{AppState, AppletRouter, GuardObject},
};
use sweech_core::{
    AppletContext, AppletError, AppletResponse, AuthRequirement, UserClaims,
    error::Guard,
    handler::{Handler, HttpMethod},
};
use tower::ServiceExt;

// ─── Handler 1: Public ping ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct PingRequest {}

#[derive(Serialize)]
struct PingResponse {
    message: String,
}

struct PingHandler;

#[async_trait]
impl Handler for PingHandler {
    type Request = PingRequest;
    type Response = PingResponse;
    fn method() -> HttpMethod {
        HttpMethod::Get
    }
    fn auth() -> AuthRequirement {
        AuthRequirement::Public
    }
    async fn call(_req: PingRequest, _ctx: AppletContext) -> AppletResponse<PingResponse> {
        AppletResponse::ok(PingResponse {
            message: "pong".to_string(),
        })
    }
}

// ─── Handler 2: Requires auth ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct WhoAmIRequest {}

#[derive(Serialize)]
struct WhoAmIResponse {
    user_id: String,
}

struct WhoAmIHandler;

#[async_trait]
impl Handler for WhoAmIHandler {
    type Request = WhoAmIRequest;
    type Response = WhoAmIResponse;
    fn method() -> HttpMethod {
        HttpMethod::Get
    }
    // auth() defaults to Required
    async fn call(_req: WhoAmIRequest, ctx: AppletContext) -> AppletResponse<WhoAmIResponse> {
        let user_id = ctx.user.map(|u| u.user_id).unwrap_or_default();
        AppletResponse::ok(WhoAmIResponse { user_id })
    }
}

// ─── Handler 3: Requires auth + guard ────────────────────────────────────────

#[derive(Deserialize)]
struct AdminRequest {}

#[derive(Serialize)]
struct AdminResponse {
    secret: String,
}

struct AdminHandler;

#[async_trait]
impl Handler for AdminHandler {
    type Request = AdminRequest;
    type Response = AdminResponse;
    fn method() -> HttpMethod {
        HttpMethod::Get
    }
    fn guards() -> &'static [&'static str] {
        &["role:admin"]
    }
    async fn call(_req: AdminRequest, _ctx: AppletContext) -> AppletResponse<AdminResponse> {
        AppletResponse::ok(AdminResponse {
            secret: "admin_data".to_string(),
        })
    }
}

// ─── Guard: role:admin ────────────────────────────────────────────────────────

struct AdminGuard;

#[async_trait]
impl Guard for AdminGuard {
    fn name() -> &'static str {
        "role:admin"
    }
    fn instance_name(&self) -> &'static str {
        "role:admin"
    }
    async fn check(&self, ctx: &AppletContext) -> Result<(), AppletError> {
        match &ctx.user {
            Some(claims) if claims.has_role("admin") => Ok(()),
            _ => Err(AppletError::forbidden(
                "INSUFFICIENT_ROLE",
                "Admin role required",
            )),
        }
    }
}

// ─── AuthValidator ────────────────────────────────────────────────────────────
// Token format: "Bearer user_id:role1,role2"

struct TestAuthValidator;

#[async_trait]
impl AuthValidator for TestAuthValidator {
    async fn validate(&self, parts: &Parts) -> Option<UserClaims> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)?
            .to_str()
            .ok()?
            .strip_prefix("Bearer ")?;

        let mut parts = token.splitn(2, ':');
        let user_id = parts.next()?.to_string();
        let roles: Vec<String> = parts
            .next()
            .map(|r| {
                r.split(',')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        Some(UserClaims {
            user_id,
            tenant_id: None,
            roles,
            raw: serde_json::Value::Null,
        })
    }
}

// ─── App factory ──────────────────────────────────────────────────────────────

fn build_app() -> Router {
    let auth_state = AuthState {
        validator: Arc::new(TestAuthValidator),
    };

    let app_state = AppState {
        auth: auth_state.clone(),
        guards: Arc::new(vec![Arc::new(AdminGuard) as Arc<dyn GuardObject>]),
    };

    // Build routes, attach state
    let router = AppletRouter::new()
        .register::<PingHandler>("/ping")
        .register::<WhoAmIHandler>("/whoami")
        .register::<AdminHandler>("/admin")
        .build(app_state);

    // Wrap with auth middleware — router.layer() is the correct Axum pattern
    router.layer(axum::middleware::from_fn_with_state(
        auth_state,
        auth_middleware,
    ))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn public_endpoint_no_auth_needed() {
    let app = build_app();
    let resp = app.oneshot(get("/ping")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_string(resp).await;
    assert!(body.contains("pong"), "body: {body}");
}

#[tokio::test]
async fn required_auth_without_token_returns_401() {
    let app = build_app();
    let resp = app.oneshot(get("/whoami")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn required_auth_with_token_returns_200() {
    let app = build_app();
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/whoami")
                .header(header::AUTHORIZATION, "Bearer user-456:")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_string(resp).await;
    assert!(body.contains("user-456"), "body: {body}");
}

#[tokio::test]
async fn guard_blocks_non_admin() {
    let app = build_app();
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/admin")
                .header(header::AUTHORIZATION, "Bearer user-123:billing:active")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn guard_allows_admin() {
    let app = build_app();
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/admin")
                .header(header::AUTHORIZATION, "Bearer user-999:admin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_string(resp).await;
    assert!(body.contains("admin_data"), "body: {body}");
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

async fn to_string(resp: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}
