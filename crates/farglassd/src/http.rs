//! The HTTP routes: the shell files, the WebSocket endpoint, and the session endpoint.

use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::header::{CACHE_CONTROL, SET_COOKIE, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse as _, Response};
use axum::routing::{any, post};
use serde::Deserialize;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;

use crate::connection::Context;
use crate::cookie;
use crate::ws;

/// The path of the WebSocket endpoint.
pub const WS_PATH: &str = "/ws";

/// The path of the session endpoint: POST redeems a ticket, DELETE removes the session.
pub const SESSION_PATH: &str = "/session";

/// The Content Security Policy of all responses.
///
/// Only files of this server can run. No inline script, no eval, no frames,
/// and no connection to a different host.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; \
font-src 'self'; connect-src 'self'; worker-src 'self'; object-src 'none'; base-uri 'none'; \
form-action 'self'; frame-ancestors 'none'";

/// The headers that all responses get, with their values.
const SECURITY_HEADERS: [(&str, &str); 6] = [
    ("content-security-policy", CSP),
    // The shell gets its own browsing context group and can use SharedArrayBuffer later.
    ("cross-origin-opener-policy", "same-origin"),
    ("cross-origin-embedder-policy", "require-corp"),
    ("cross-origin-resource-policy", "same-origin"),
    ("referrer-policy", "no-referrer"),
    ("x-frame-options", "DENY"),
];

const HSTS: HeaderValue = HeaderValue::from_static("max-age=31536000");

/// The directory of the files that have a hash of their contents in the filename.
/// Vite writes all scripts and style sheets there.
const HASHED_PREFIX: &str = "/assets/";

const CACHE_IMMUTABLE: HeaderValue =
    HeaderValue::from_static("public, max-age=31536000, immutable");
const CACHE_REVALIDATE: HeaderValue = HeaderValue::from_static("no-cache");
const CACHE_NEVER: HeaderValue = HeaderValue::from_static("no-store");
const NOSNIFF: HeaderValue = HeaderValue::from_static("nosniff");

/// Returns the router of farglassd. `web_root` is the directory with the shell files.
pub fn router(web_root: &Path, state: Arc<Context>) -> Router {
    let files = Router::new()
        .fallback_service(ServeDir::new(web_root))
        .layer(CompressionLayer::new())
        .layer(middleware::from_fn(cache_headers));

    Router::new()
        // `any` accepts GET for HTTP/1.1 and CONNECT for HTTP/2 (RFC 8441).
        .route(WS_PATH, any(ws::upgrade))
        .route(SESSION_PATH, post(create_session).delete(delete_session))
        .layer(DefaultBodyLimit::max(1024))
        .merge(files)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            security_headers,
        ))
        .with_state(state)
}

/// The body of `POST /session`.
#[derive(Debug, Deserialize)]
struct SessionRequest {
    ticket: String,
}

/// Redeems the ticket of a login and sets the session cookie.
async fn create_session(
    State(context): State<Arc<Context>>,
    headers: HeaderMap,
    uri: Uri,
    body: axum::body::Bytes,
) -> Response {
    if !context.origin.allows(&headers, &uri) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(request) = serde_json::from_slice::<SessionRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Some(token) = context.sessions.redeem(&request.ticket) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let mut response = StatusCode::NO_CONTENT.into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie::set_cookie(&token)) {
        response.headers_mut().insert(SET_COOKIE, value);
    }
    response
}

/// Removes the session of the cookie and the cookie. F010 also ends the logind session.
async fn delete_session(
    State(context): State<Arc<Context>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if !context.origin.allows(&headers, &uri) {
        return StatusCode::FORBIDDEN.into_response();
    }
    context.sessions.remove(&headers);
    let mut response = StatusCode::NO_CONTENT.into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie::clear_cookie()) {
        response.headers_mut().insert(SET_COOKIE, value);
    }
    response
}

/// Sets the security headers on each response.
async fn security_headers(
    State(context): State<Arc<Context>>,
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in SECURITY_HEADERS {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    if context.hsts {
        headers.insert(HeaderName::from_static("strict-transport-security"), HSTS);
    }
    response
}

/// Sets the cache headers of a shell file.
///
/// A file with a hash in its name never changes, thus the browser keeps it.
/// For all other files, the browser asks the server each time. The answer is
/// small if the file did not change.
async fn cache_headers(request: Request, next: Next) -> Response {
    let hashed = request.uri().path().starts_with(HASHED_PREFIX);
    let mut response = next.run(request).await;
    let cache = match response.status() {
        StatusCode::OK | StatusCode::PARTIAL_CONTENT | StatusCode::NOT_MODIFIED if hashed => {
            CACHE_IMMUTABLE
        }
        StatusCode::OK | StatusCode::PARTIAL_CONTENT | StatusCode::NOT_MODIFIED => CACHE_REVALIDATE,
        _ => CACHE_NEVER,
    };
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, cache);
    headers.insert(X_CONTENT_TYPE_OPTIONS, NOSNIFF);
    response
}
