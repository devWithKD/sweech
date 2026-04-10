// ─── Monolith assembly test ───────────────────────────────────────────────────
//
// This test proves that SweechApp correctly merges multiple applets into
// one router, each mounted at its prefix, all sharing the same state.
//
// Structure being simulated:
//
//   auth.applet/
//       login/route.rs     → POST /auth/login
//       me/route.rs        → GET  /auth/me       (requires auth)
//
//   products.applet/
//       route.rs           → GET  /products      (public)
//       route.rs           → POST /products      (requires auth)

use async_trait::async_trait;
use axum::{
    body::Body,
    extract::Request,
    http::{Method, StatusCode, header, request::Parts},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sweech_axum::{
    Applet, AppletConfig, SweechApp,
    middleware::{AuthState, AuthValidator, auth_middleware},
    router::{AppState, AppletRouter},
};
use sweech_core::{
    AppletContext, AppletResponse, AuthRequirement, UserClaims,
    handler::{Handler, HttpMethod},
};
use tower::ServiceExt;

// ─── Shared auth setup ────────────────────────────────────────────────────────

struct TestAuth;

#[async_trait]
impl AuthValidator for TestAuth {
    async fn validate(&self, parts: &Parts) -> Option<UserClaims> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)?
            .to_str()
            .ok()?
            .strip_prefix("Bearer ")?;

        let mut split = token.splitn(2, ':');
        let user_id = split.next()?.to_string();
        let roles: Vec<String> = split
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

// ─── auth.applet handlers ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
}

struct LoginHandler;

#[async_trait]
impl Handler for LoginHandler {
    type Request = LoginRequest;
    type Response = LoginResponse;
    fn method() -> HttpMethod {
        HttpMethod::Post
    }
    fn auth() -> AuthRequirement {
        AuthRequirement::Public
    }
    async fn call(req: LoginRequest, _ctx: AppletContext) -> AppletResponse<LoginResponse> {
        // In real life: verify credentials, issue token
        AppletResponse::ok(LoginResponse {
            token: format!("token-for-{}", req.username),
        })
    }
}

#[derive(Deserialize)]
struct MeRequest {}

#[derive(Serialize)]
struct MeResponse {
    user_id: String,
}

struct MeHandler;

#[async_trait]
impl Handler for MeHandler {
    type Request = MeRequest;
    type Response = MeResponse;
    fn method() -> HttpMethod {
        HttpMethod::Get
    }
    // auth defaults to Required
    async fn call(_req: MeRequest, ctx: AppletContext) -> AppletResponse<MeResponse> {
        let user_id = ctx.user.map(|u| u.user_id).unwrap_or_default();
        AppletResponse::ok(MeResponse { user_id })
    }
}

// ─── products.applet handlers ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListProductsRequest {}

#[derive(Serialize)]
struct ProductsResponse {
    products: Vec<String>,
}

struct ListProductsHandler;

#[async_trait]
impl Handler for ListProductsHandler {
    type Request = ListProductsRequest;
    type Response = ProductsResponse;
    fn method() -> HttpMethod {
        HttpMethod::Get
    }
    fn auth() -> AuthRequirement {
        AuthRequirement::Public
    }
    async fn call(
        _req: ListProductsRequest,
        _ctx: AppletContext,
    ) -> AppletResponse<ProductsResponse> {
        AppletResponse::ok(ProductsResponse {
            products: vec!["widget".to_string(), "gadget".to_string()],
        })
    }
}

#[derive(Deserialize)]
struct CreateProductRequest {
    name: String,
}

#[derive(Serialize)]
struct CreateProductResponse {
    id: String,
    name: String,
}

struct CreateProductHandler;

#[async_trait]
impl Handler for CreateProductHandler {
    type Request = CreateProductRequest;
    type Response = CreateProductResponse;
    fn method() -> HttpMethod {
        HttpMethod::Post
    }
    // auth defaults to Required
    async fn call(
        req: CreateProductRequest,
        _ctx: AppletContext,
    ) -> AppletResponse<CreateProductResponse> {
        AppletResponse::created(CreateProductResponse {
            id: "prod-001".to_string(),
            name: req.name,
        })
    }
}

// ─── App factory ──────────────────────────────────────────────────────────────

fn build_monolith() -> axum::Router {
    let auth_state = AuthState {
        validator: Arc::new(TestAuth),
    };
    let app_state = AppState {
        auth: auth_state.clone(),
        guards: Arc::new(vec![]),
    };

    // auth.applet — /auth/login, /auth/me
    let auth_applet = Applet::new(
        AppletConfig::new("auth"),
        AppletRouter::new()
            .register::<LoginHandler>("/login")
            .register::<MeHandler>("/me"),
        app_state.clone(),
    );

    // products.applet — /products (GET + POST)
    let products_applet = Applet::new(
        AppletConfig::new("products"),
        AppletRouter::new()
            .register::<ListProductsHandler>("/")
            .register::<CreateProductHandler>("/"),
        app_state.clone(),
    );

    // Assemble monolith and wrap with auth middleware
    let router = SweechApp::new()
        .applet(auth_applet)
        .applet(products_applet)
        .build();

    router.layer(axum::middleware::from_fn_with_state(
        auth_state,
        auth_middleware,
    ))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn auth_login_is_public_and_reachable() {
    let app = build_monolith();

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"username":"alice"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_string(resp).await;
    assert!(body.contains("token-for-alice"), "body: {body}");
}

#[tokio::test]
async fn auth_me_requires_token() {
    let app = build_monolith();

    let resp = app.oneshot(get("/auth/me")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_me_with_token_returns_user() {
    let app = build_monolith();

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/me")
                .header(header::AUTHORIZATION, "Bearer user-bob:")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_string(resp).await;
    assert!(body.contains("user-bob"), "body: {body}");
}

#[tokio::test]
async fn products_list_is_public() {
    let app = build_monolith();

    let resp = app.oneshot(get("/products")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_string(resp).await;
    assert!(body.contains("widget"), "body: {body}");
}

#[tokio::test]
async fn products_create_requires_auth() {
    let app = build_monolith();

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/products")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"new-widget"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn products_create_with_auth_returns_201() {
    let app = build_monolith();

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/products")
                .header(header::AUTHORIZATION, "Bearer user-alice:")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"new-widget"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = to_string(resp).await;
    assert!(body.contains("new-widget"), "body: {body}");
}

#[tokio::test]
async fn applets_dont_bleed_into_each_other() {
    let app = build_monolith();

    // /auth/products should 404 — products is its own applet at /products
    let resp = app.oneshot(get("/auth/products")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
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
    let b = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(b.to_vec()).unwrap()
}
