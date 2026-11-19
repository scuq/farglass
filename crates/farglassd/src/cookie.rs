//! The session cookie: one-time tickets after a login, and the sessions that the cookies name.
//!
//! A WebSocket message cannot set a cookie. Thus farglassd gives the shell a
//! ticket in the success event of the login, and the shell gives the ticket to
//! `POST /session`. The answer sets the cookie. The browser sends the cookie
//! with each later WebSocket connection, and farglassd knows the user from it.
//!
//! The store keeps the SHA-256 hash of each ticket and token, not the value.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use axum::http::header::COOKIE;
use rustls::crypto::CryptoProvider;
use sha2::{Digest as _, Sha256};

use crate::login::Identity;

/// The name of the cookie. The prefix `__Host-` makes the browser refuse the
/// cookie unless it is Secure, has the path `/`, and has no domain.
pub const COOKIE_NAME: &str = "__Host-farglass";

/// The time in which the shell must give the ticket to `POST /session`.
pub const TICKET_LIFETIME: Duration = Duration::from_secs(60);

/// The time for which a session cookie is valid. F010 adds the idle logout.
pub const SESSION_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);

/// The length of a ticket and of a token in random bytes.
const SECRET_LEN: usize = 32;

type Hash = [u8; 32];

#[derive(Debug)]
struct Entry {
    identity: Identity,
    expires: Instant,
}

/// The tickets and the sessions of the cookies.
#[derive(Debug, Default)]
pub struct SessionStore {
    tickets: Mutex<HashMap<Hash, Entry>>,
    sessions: Mutex<HashMap<Hash, Entry>>,
}

impl SessionStore {
    /// Returns a new ticket for `identity`.
    pub fn ticket(&self, identity: Identity) -> String {
        let ticket = secret();
        let mut tickets = lock(&self.tickets);
        let now = Instant::now();
        tickets.retain(|_, entry| entry.expires > now);
        tickets.insert(
            hash(&ticket),
            Entry {
                identity,
                expires: now + TICKET_LIFETIME,
            },
        );
        ticket
    }

    /// Redeems a ticket one time. Returns a new session token, or `None` if the ticket is not valid.
    pub fn redeem(&self, ticket: &str) -> Option<String> {
        let entry = lock(&self.tickets).remove(&hash(ticket))?;
        let now = Instant::now();
        if entry.expires <= now {
            return None;
        }
        let token = secret();
        let mut sessions = lock(&self.sessions);
        sessions.retain(|_, session| session.expires > now);
        sessions.insert(
            hash(&token),
            Entry {
                identity: entry.identity,
                expires: now + SESSION_LIFETIME,
            },
        );
        Some(token)
    }

    /// Returns the user of the session that the cookie in `headers` names.
    #[must_use]
    pub fn user(&self, headers: &HeaderMap) -> Option<Identity> {
        let token = cookie_value(headers)?;
        let sessions = lock(&self.sessions);
        let entry = sessions.get(&hash(token))?;
        (entry.expires > Instant::now()).then(|| entry.identity.clone())
    }

    /// Removes the session that the cookie in `headers` names.
    pub fn remove(&self, headers: &HeaderMap) {
        if let Some(token) = cookie_value(headers) {
            lock(&self.sessions).remove(&hash(token));
        }
    }
}

/// Returns the value of the `Set-Cookie` header for `token`.
#[must_use]
pub fn set_cookie(token: &str) -> String {
    // No Max-Age: the browser removes the cookie when it closes. The server limits the time.
    format!("{COOKIE_NAME}={token}; Path=/; Secure; HttpOnly; SameSite=Strict")
}

/// Returns the value of the `Set-Cookie` header that removes the cookie.
#[must_use]
pub fn clear_cookie() -> String {
    format!("{COOKIE_NAME}=; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=0")
}

/// Returns the value of the session cookie in the `Cookie` headers.
fn cookie_value(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE_NAME)
        .map(|(_, value)| value)
        .filter(|value| !value.is_empty())
}

/// Returns 32 random bytes from the TLS library, as 64 hexadecimal characters.
fn secret() -> String {
    let mut bytes = [0_u8; SECRET_LEN];
    let provider: CryptoProvider = rustls::crypto::aws_lc_rs::default_provider();
    // The random source of aws-lc-rs fails only if the kernel gives no random bytes.
    provider
        .secure_random
        .fill(&mut bytes)
        .expect("the kernel gave no random bytes");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hash(secret: &str) -> Hash {
    Sha256::digest(secret.as_bytes()).into()
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn alice() -> Identity {
        Identity {
            uid: 1000,
            name: "alice".to_owned(),
        }
    }

    fn with_cookie(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn ticket_gives_one_session_one_time() {
        let store = SessionStore::default();
        let ticket = store.ticket(alice());
        assert_eq!(ticket.len(), 64);
        let token = store.redeem(&ticket).unwrap();
        assert_ne!(token, ticket);
        assert!(
            store.redeem(&ticket).is_none(),
            "a ticket works one time only"
        );
        assert!(store.redeem("0".repeat(64).as_str()).is_none());

        let headers = with_cookie(&format!("theme=dark; {COOKIE_NAME}={token}; other=1"));
        assert_eq!(store.user(&headers), Some(alice()));
        store.remove(&headers);
        assert_eq!(store.user(&headers), None);
    }

    #[test]
    fn unknown_or_empty_cookie_gives_no_user() {
        let store = SessionStore::default();
        assert_eq!(store.user(&HeaderMap::new()), None);
        assert_eq!(store.user(&with_cookie(&format!("{COOKIE_NAME}="))), None);
        assert_eq!(
            store.user(&with_cookie(&format!("{COOKIE_NAME}={}", "a".repeat(64)))),
            None
        );
        assert_eq!(store.user(&with_cookie("farglass=abc")), None);
    }

    #[test]
    fn expired_ticket_is_refused() {
        let store = SessionStore::default();
        let ticket = store.ticket(alice());
        lock(&store.tickets)
            .values_mut()
            .for_each(|entry| entry.expires = Instant::now());
        assert!(store.redeem(&ticket).is_none());
    }

    #[test]
    fn expired_session_gives_no_user() {
        let store = SessionStore::default();
        let token = store.redeem(&store.ticket(alice())).unwrap();
        lock(&store.sessions)
            .values_mut()
            .for_each(|entry| entry.expires = Instant::now());
        assert_eq!(
            store.user(&with_cookie(&format!("{COOKIE_NAME}={token}"))),
            None
        );
    }

    #[test]
    fn cookie_has_the_attributes_that_protect_it() {
        let value = set_cookie("abc");
        for attribute in [
            "__Host-farglass=abc",
            "Path=/",
            "Secure",
            "HttpOnly",
            "SameSite=Strict",
        ] {
            assert!(
                value.contains(attribute),
                "{attribute} is missing in {value}"
            );
        }
        assert!(!value.contains("Domain"));
    }
}
