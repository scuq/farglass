//! The HTTP routes: the shell files and the WebSocket endpoint.

use std::path::Path;

use axum::Router;
use axum::extract::Request;
use axum::http::header::{CACHE_CONTROL, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::any;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;

use crate::ws;

/// The path of the WebSocket endpoint.
pub const WS_PATH: &str = "/ws";

/// The directory of the files that have a hash of their contents in the filename.
/// Vite writes all scripts and style sheets there.
const HASHED_PREFIX: &str = "/assets/";

const CACHE_IMMUTABLE: HeaderValue =
    HeaderValue::from_static("public, max-age=31536000, immutable");
const CACHE_REVALIDATE: HeaderValue = HeaderValue::from_static("no-cache");
const CACHE_NEVER: HeaderValue = HeaderValue::from_static("no-store");
const NOSNIFF: HeaderValue = HeaderValue::from_static("nosniff");

/// Returns the router of farglassd. `web_root` is the directory with the shell files.
pub fn router(web_root: &Path) -> Router {
    let files = Router::new()
        .fallback_service(ServeDir::new(web_root))
        .layer(CompressionLayer::new())
        .layer(middleware::from_fn(cache_headers));

    Router::new()
        // `any` accepts GET for HTTP/1.1 and CONNECT for HTTP/2 (RFC 8441).
        .route(WS_PATH, any(ws::upgrade))
        .merge(files)
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
