use axum::{
    body::Bytes,
    http::{HeaderMap, Method},
};
use std::collections::HashMap;
use sweech_core::auth::UserClaims;
use sweech_core::context::{
    AppletContext, CacheContext, DbContext, QueueContext, RequestInfo, StorageContext,
};

// ─── What is this file? ───────────────────────────────────────────────────────
//
// When an HTTP request comes in, Axum has its own internal representation of it.
// Sweech needs to turn that into our AppletContext so handlers only ever
// see Sweech's types — never raw Axum types.
//
// This file owns that translation.
//
// ─── Rust concept: From / Into traits ────────────────────────────────────────
//
// `From<A>` for `B` means: "B can be constructed from A".
// Implementing From automatically gives you Into for free — Rust does that.
//
// We implement `From<IncomingRequest>` for `RequestInfo` here.
// That means anywhere in the code you can write:
//   let info = RequestInfo::from(incoming);
// or equivalently:
//   let info: RequestInfo = incoming.into();
//
// ─── Rust concept: Axum extractors ───────────────────────────────────────────
//
// Axum uses a pattern called "extractors" — types that know how to pull
// specific data out of an HTTP request. For example:
//   Path(params)  — extracts URL path parameters
//   Query(map)    — extracts query string parameters
//   State(s)      — extracts shared application state
//   Bytes         — extracts the raw body as bytes
//
// When you list these as function arguments in an Axum handler, Axum
// automatically calls their extraction logic before your function runs.
// We use the same mechanism to build AppletContext.

/// Raw pieces extracted from an Axum request before we build AppletContext.
/// This is an intermediate struct — it doesn't live beyond the extraction step.
pub struct IncomingRequest {
    pub method: Method,
    pub path: String,
    pub params: HashMap<String, String>,
    pub query: HashMap<String, String>,
    pub headers: HeaderMap,
    pub body: Bytes,
    /// The verified user identity, injected by auth middleware before we get here.
    /// None if the route is Public or no token was provided on an Optional route.
    pub user: Option<UserClaims>,
}

impl IncomingRequest {
    /// Convert into the Sweech RequestInfo + optional UserClaims.
    /// Called by build_context() below.
    pub fn into_request_info(self) -> (RequestInfo, Option<UserClaims>) {
        // Convert HeaderMap (Axum's type) into HashMap<String, String> (our type).
        //
        // ─── Rust concept: iterator chains ───────────────────────────────────
        //
        // `.iter()` gives us an iterator over (key, value) pairs.
        // `.filter_map(|(k, v)| ...)` maps each pair, skipping any that return None.
        //   (v.to_str() can fail if the header contains non-UTF8 bytes — we skip those)
        // `.collect()` gathers everything into the target collection type.
        //   Rust infers the target type from the variable's declared type.
        //
        // This is the Rust equivalent of:
        //   Object.fromEntries(headers.entries().filter(([k,v]) => isValidStr(v)))

        let headers: HashMap<String, String> = self
            .headers
            .iter()
            .filter_map(|(k, v)| {
                v.to_str()
                    .ok()
                    .map(|v_str| (k.as_str().to_lowercase(), v_str.to_string()))
            })
            .collect();

        let info = RequestInfo {
            method: self.method.as_str().to_string(),
            path: self.path,
            params: self.params,
            query: self.query,
            headers,
            body: self.body.to_vec(),
        };

        (info, self.user)
    }
}

/// Build the full AppletContext from an IncomingRequest.
///
/// Called by the route adapter right before invoking the handler.
/// Plugin contexts (db, queue, etc.) will be populated from AppState later —
/// for now they're placeholders.
pub fn build_context(incoming: IncomingRequest) -> AppletContext {
    let (request, user) = incoming.into_request_info();

    AppletContext {
        db: DbContext,
        queue: QueueContext,
        storage: StorageContext,
        cache: CacheContext,
        request,
        user,
    }
}

// ─── UserClaims extension key ─────────────────────────────────────────────────
//
// Axum has a concept called "request extensions" — a type-safe bag of arbitrary
// data you can attach to a request as it flows through middleware.
//
// Auth middleware validates the token and inserts UserClaims into the extensions.
// The route adapter then reads it back out here.
//
// The type itself IS the key — Axum uses TypeId internally. So to store
// UserClaims in extensions you do:  extensions.insert(claims)
// And to read it back:              extensions.get::<UserClaims>()
//
// This is why UserClaims derives Clone — extensions.get() returns a reference,
// so we clone to take ownership.
//
// We don't need any extra code here — it's built into Axum's Extension system.
// Just documenting the pattern so it's clear when you read the middleware file.
