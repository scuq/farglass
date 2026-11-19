//! The Origin check of the WebSocket endpoint and of `POST /session`.
//!
//! A page of a different site can open a WebSocket to farglassd, and the
//! browser sends the cookie with it. Thus farglassd accepts a request only if
//! the `Origin` header names the host of the request, or an origin of the
//! configuration.

use axum::http::header::{HOST, ORIGIN};
use axum::http::{HeaderMap, Uri};

/// The rules of the Origin check.
#[derive(Clone, Debug, Default)]
pub struct OriginPolicy {
    /// More origins that farglassd accepts, for example `http://localhost:5173` for the Vite server.
    pub extra: Vec<String>,
    /// Reports whether the host must be a loopback name. `dev_user` mode sets it,
    /// thus a different site cannot use DNS rebinding to get a user with no login.
    pub loopback_only: bool,
}

impl OriginPolicy {
    /// Reports whether the request comes from the shell of this server.
    #[must_use]
    pub fn allows(&self, headers: &HeaderMap, uri: &Uri) -> bool {
        let Some(origin) = headers.get(ORIGIN).and_then(|value| value.to_str().ok()) else {
            return false;
        };
        // HTTP/2 gives the host in the URI (:authority), HTTP/1.1 in the Host header.
        let host = uri
            .authority()
            .map(|authority| authority.as_str().to_owned())
            .or_else(|| {
                headers
                    .get(HOST)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned)
            });
        let Some(host) = host else {
            return false;
        };
        if self.loopback_only && !is_loopback_host(&host) {
            return false;
        }
        origin.eq_ignore_ascii_case(&format!("https://{host}"))
            || self
                .extra
                .iter()
                .any(|extra| origin.eq_ignore_ascii_case(extra))
    }
}

/// Reports whether `host` (with an optional port) is a loopback name or address.
fn is_loopback_host(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        // An IPv6 address in brackets, with an optional port.
        Some(rest) => rest.split(']').next().unwrap_or_default(),
        None => host.rsplit_once(':').map_or(host, |(name, _)| name),
    };
    name.eq_ignore_ascii_case("localhost")
        || name
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn request(origin: Option<&str>, host: &str) -> (HeaderMap, Uri) {
        let mut headers = HeaderMap::new();
        if let Some(origin) = origin {
            headers.insert(ORIGIN, HeaderValue::from_str(origin).unwrap());
        }
        headers.insert(HOST, HeaderValue::from_str(host).unwrap());
        (headers, Uri::from_static("/ws"))
    }

    #[test]
    fn same_origin_is_accepted_and_others_are_refused() {
        let policy = OriginPolicy::default();
        let allows = |origin, host| {
            let (headers, uri) = request(origin, host);
            policy.allows(&headers, &uri)
        };
        assert!(allows(
            Some("https://desk.cyberdyne.example"),
            "desk.cyberdyne.example"
        ));
        assert!(allows(
            Some("https://desk.cyberdyne.example:8443"),
            "desk.cyberdyne.example:8443"
        ));
        assert!(allows(
            Some("https://Desk.Cyberdyne.Example"),
            "desk.cyberdyne.example"
        ));
        assert!(!allows(
            Some("https://evil.example"),
            "desk.cyberdyne.example"
        ));
        assert!(!allows(
            Some("http://desk.cyberdyne.example"),
            "desk.cyberdyne.example"
        ));
        assert!(!allows(
            Some("https://desk.cyberdyne.example:9999"),
            "desk.cyberdyne.example:8443"
        ));
        assert!(!allows(Some("null"), "desk.cyberdyne.example"));
        assert!(!allows(None, "desk.cyberdyne.example"));
    }

    #[test]
    fn http2_gives_the_host_in_the_uri() {
        let mut headers = HeaderMap::new();
        headers.insert(
            ORIGIN,
            HeaderValue::from_static("https://desk.cyberdyne.example"),
        );
        let uri = Uri::from_static("https://desk.cyberdyne.example/ws");
        assert!(OriginPolicy::default().allows(&headers, &uri));
    }

    #[test]
    fn extra_origin_is_accepted() {
        let policy = OriginPolicy {
            extra: vec!["http://localhost:5173".to_owned()],
            loopback_only: false,
        };
        let (headers, uri) = request(Some("http://localhost:5173"), "localhost:8443");
        assert!(policy.allows(&headers, &uri));
    }

    #[test]
    fn dev_mode_accepts_only_a_loopback_host() {
        let policy = OriginPolicy {
            extra: Vec::new(),
            loopback_only: true,
        };
        for host in [
            "localhost:8443",
            "127.0.0.1:8443",
            "[::1]:8443",
            "localhost",
        ] {
            let (headers, uri) = request(Some(&format!("https://{host}")), host);
            assert!(policy.allows(&headers, &uri), "{host}");
        }
        // DNS rebinding: a different name that resolves to 127.0.0.1.
        let (headers, uri) = request(
            Some("https://rebind.evil.example:8443"),
            "rebind.evil.example:8443",
        );
        assert!(!policy.allows(&headers, &uri));
    }
}
