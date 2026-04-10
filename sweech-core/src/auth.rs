use serde::{Deserialize, Serialize};

// ─── What is this file? ───────────────────────────────────────────────────────
//
// Two things live here:
//   1. AuthRequirement — what a handler declares about who can call it
//   2. UserClaims — the parsed, verified identity that auth middleware injects
//
// Sweech's two-layer access model:
//   Layer 1 (this file) — AuthRequirement: "who are you?" — stateless, no DB
//   Layer 2 (guard.rs)  — Guards:          "are you allowed?" — stateful, DB OK
//
// ─── Rust concept: enum ──────────────────────────────────────────────────────
//
// Rust enums are much more powerful than C/Java enums.
// Each variant can carry data (we'll see that later with AppletError).
// Here they're simple — just named states, like a type-safe string constant.
//
// ─── Rust concept: #[derive] attributes ──────────────────────────────────────
//
// We derive several traits here:
//   Debug   — lets you print with {:?} for logging/debugging
//   Clone   — lets you call .clone() to make a copy
//   PartialEq — lets you compare with == and !=
//   Default — lets you call AuthRequirement::default() to get a default value
//             (we set Required as default — opt DOWN, not up)
//
// These are all zero-cost — the compiler generates the implementations,
// there's no runtime overhead.

/// Declares who is allowed to call a handler.
///
/// Default is `Required` — every handler requires auth unless explicitly
/// opted down. You never accidentally expose a route publicly.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum AuthRequirement {
    /// A valid, verified token must be present.
    /// `ctx.user` will be `Some(claims)`.
    /// If missing or invalid → 401 Unauthorized, handler never runs.
    #[default] // AuthRequirement::default() returns this variant
    Required,

    /// No token needed. `ctx.user` will always be `None`.
    /// Use for login, register, public product listings, etc.
    Public,

    /// Token is optional. `ctx.user` is `Some` if present, `None` if not.
    /// Use for "show more details if logged in" type endpoints.
    Optional,
}

// ─── UserClaims ───────────────────────────────────────────────────────────────
//
// This is what auth middleware puts into `ctx.user` after validating a JWT.
// It contains only the information baked into the token — no DB lookup.
// That's what makes Layer 1 stateless.
//
// ─── Rust concept: String vs &str ────────────────────────────────────────────
//
// `String` is an owned, heap-allocated string. You own it, you can modify it.
// `&str` is a borrowed reference to string data (could be from a String, or
// from a string literal "like this" which lives in the binary).
//
// In structs, you almost always use `String` for owned data — the struct
// owns the string and it lives as long as the struct does.
//
// ─── Rust concept: Option<T> ─────────────────────────────────────────────────
//
// Rust has no null. Instead it has Option<T>:
//   Some(value)  — there is a value
//   None         — there is no value
//
// The compiler forces you to handle both cases before you can use the value.
// This eliminates null pointer exceptions at compile time.

/// The verified identity extracted from a JWT by auth middleware.
///
/// Injected into `AppletContext.user` before the handler runs.
/// If `AuthRequirement::Required` and no valid token → 401, this is never populated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserClaims {
    /// The user's unique identifier (from your JWT `sub` claim).
    pub user_id: String,

    /// The tenant this user belongs to.
    /// Multi-tenancy is the developer's concern — Sweech just carries the claim.
    pub tenant_id: Option<String>,

    /// Roles assigned to this user, e.g. ["admin", "billing:active"].
    /// Guards can inspect these to make authorization decisions.
    pub roles: Vec<String>,

    /// Raw JWT claims as JSON — lets guards access any custom claim
    /// without Sweech needing to know about them upfront.
    pub raw: serde_json::Value,
}

impl UserClaims {
    /// Check if this user has a specific role.
    ///
    /// Used by guard implementations — e.g. `ctx.user?.has_role("admin")`.
    pub fn has_role(&self, role: &str) -> bool {
        // `.iter()` gives us an iterator over &String references
        // `.any(|r| r == role)` returns true if any element matches
        // This is like .some() in JavaScript
        self.roles.iter().any(|r| r == role)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_auth_is_required() {
        // AuthRequirement::default() should give us Required
        assert_eq!(AuthRequirement::default(), AuthRequirement::Required);
    }

    #[test]
    fn has_role_works() {
        let claims = UserClaims {
            user_id: "user-123".to_string(),
            tenant_id: Some("tenant-abc".to_string()),
            roles: vec!["admin".to_string(), "billing:active".to_string()],
            raw: serde_json::Value::Null,
        };

        assert!(claims.has_role("admin"));
        assert!(claims.has_role("billing:active"));
        assert!(!claims.has_role("superuser"));
    }
}
