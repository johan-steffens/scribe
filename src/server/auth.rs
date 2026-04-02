// Rust guideline compliant 2026-02-21
//! Bearer token authentication middleware for the REST sync server.
//!
//! Provides [`require_bearer`], an axum middleware layer that validates every
//! incoming request against a shared secret stored in axum's request extensions.
//!
//! The expected secret is inserted into the extension map by the router setup in
//! [`super::start_server`] before any handler runs. Requests missing or
//! presenting an incorrect Bearer token are rejected with `401 Unauthorized`.

// DOCUMENTED-MAGIC: Dead code until the daemon wires the server in a later task.

use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};

/// Validates `Authorization: Bearer <secret>` on every request.
///
/// The expected secret is read from the request's axum extension map
/// (inserted by the router before this middleware runs).  Returns
/// `401 Unauthorized` if the header is absent, malformed, or incorrect.
///
/// # Errors
///
/// Returns [`StatusCode::UNAUTHORIZED`] when the provided token does not
/// match the expected secret.
pub async fn require_bearer(
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // DOCUMENTED-MAGIC: The expected secret is threaded through axum's
    // extension map rather than captured directly so the middleware function
    // can be constructed with `middleware::from_fn` without closing over
    // a secret-bearing `Arc`. The calling site in `start_server` inserts
    // the secret before this middleware executes.
    let expected = request
        .extensions()
        .get::<String>()
        .cloned()
        .unwrap_or_default();

    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if provided == expected {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
