//! The device cookie (F051): a long-lived mark of a browser that logged in before.
//!
//! When the login places are full, a browser with a valid device cookie can
//! still use the reserved places. The cookie gives no access: the user must
//! log in as before.
//!
//! The cookie value is `EXPIRY.MAC`: the end of its validity in Unix seconds,
//! and an HMAC-SHA256 of it with a key of farglassd. The key is in
//! `STATE_DIR/device.key`, mode 0600. farglassd keeps no table of devices.
//!
//! If the key file cannot be read or created, no device cookie is valid
//! (ADR-042): each browser uses the open places only.

use std::fs;
use std::io::{self, Write as _};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::http::HeaderMap;
use axum::http::header::COOKIE;
use farglass_login::audit::Event;
use sha2::{Digest as _, Sha256};

/// The name of the device cookie.
pub const DEVICE_COOKIE: &str = "__Host-farglass-device";

/// The validity of a device cookie.
pub const DEVICE_LIFETIME: Duration = Duration::from_secs(90 * 24 * 60 * 60);

/// The key of the device cookies.
#[derive(Debug, Default)]
pub struct DeviceKeys {
    key: Option<[u8; 32]>,
}

impl DeviceKeys {
    /// Loads the key from `path`, or creates it if the file does not exist.
    ///
    /// Each other error gives a store with no key, and the journal gets the fault.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        match read_or_create(path) {
            Ok(key) => Self { key: Some(key) },
            Err(error) => {
                Event {
                    event: "device-key",
                    reason: "read-error",
                    detail: Some(&format!("{}: {error}", path.display())),
                    ..Event::default()
                }
                .fault();
                Self { key: None }
            }
        }
    }

    /// Returns a store with the key `key`, for tests.
    #[must_use]
    pub fn with_key(key: [u8; 32]) -> Self {
        Self { key: Some(key) }
    }

    /// Returns a new cookie value, or `None` if farglassd has no key.
    #[must_use]
    pub fn issue(&self) -> Option<String> {
        let key = self.key.as_ref()?;
        let expiry = now_secs().checked_add(DEVICE_LIFETIME.as_secs())?;
        Some(format!("{expiry}.{}", hex(&mac(key, expiry))))
    }

    /// Reports whether the cookie headers in `headers` hold a valid device cookie.
    #[must_use]
    pub fn valid(&self, headers: &HeaderMap) -> bool {
        cookie_value(headers).is_some_and(|value| self.check(value))
    }

    fn check(&self, value: &str) -> bool {
        let Some(key) = self.key.as_ref() else {
            return false;
        };
        let Some((expiry, given)) = value.split_once('.') else {
            return false;
        };
        let Ok(expiry) = expiry.parse::<u64>() else {
            return false;
        };
        if expiry <= now_secs() {
            return false;
        }
        let expected = hex(&mac(key, expiry));
        // Compare in constant time.
        expected.len() == given.len()
            && expected
                .bytes()
                .zip(given.bytes())
                .fold(0_u8, |acc, (a, b)| acc | (a ^ b))
                == 0
    }
}

/// Returns the value of the `Set-Cookie` header for the device cookie `value`.
#[must_use]
pub fn set_cookie(value: &str) -> String {
    format!(
        "{DEVICE_COOKIE}={value}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age={}",
        DEVICE_LIFETIME.as_secs()
    )
}

fn read_or_create(path: &Path) -> io::Result<[u8; 32]> {
    match fs::read_to_string(path) {
        Ok(text) => parse_key(text.trim())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "the key is not valid")),
        // Only a missing file gives a new key. A damaged file stays (ADR-042).
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let text = crate::cookie::secret();
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            parse_key(&text).ok_or_else(|| io::Error::other("the new key is not valid"))
        }
        Err(error) => Err(error),
    }
}

fn parse_key(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut key = [0_u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(key)
}

/// Returns the HMAC-SHA256 (RFC 2104) of the expiry with `key`.
fn mac(key: &[u8; 32], expiry: u64) -> [u8; 32] {
    let mut inner = [0x36_u8; 64];
    let mut outer = [0x5c_u8; 64];
    for (i, byte) in key.iter().enumerate() {
        inner[i] ^= byte;
        outer[i] ^= byte;
    }
    let inner_hash = Sha256::new()
        .chain_update(inner)
        .chain_update(b"farglass-device:")
        .chain_update(expiry.to_string())
        .finalize();
    Sha256::new()
        .chain_update(outer)
        .chain_update(inner_hash)
        .finalize()
        .into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs())
}

fn cookie_value(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == DEVICE_COOKIE)
        .map(|(_, value)| value)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use axum::http::HeaderValue;

    use super::*;

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_str(&format!("a=1; {DEVICE_COOKIE}={value}")).unwrap(),
        );
        headers
    }

    #[test]
    fn an_issued_cookie_is_valid_and_a_changed_one_is_not() {
        let keys = DeviceKeys::with_key([7; 32]);
        let value = keys.issue().unwrap();
        assert!(keys.valid(&headers(&value)));
        let (expiry, mac) = value.split_once('.').unwrap();
        let later = format!("{}.{mac}", expiry.parse::<u64>().unwrap() + 1);
        assert!(!keys.valid(&headers(&later)), "the MAC covers the expiry");
        assert!(!DeviceKeys::with_key([8; 32]).valid(&headers(&value)));
        assert!(!keys.valid(&HeaderMap::new()));
        assert!(!keys.valid(&headers("garbage")));
    }

    #[test]
    fn an_expired_cookie_is_not_valid() {
        let keys = DeviceKeys::with_key([7; 32]);
        let expiry = now_secs() - 1;
        let value = format!("{expiry}.{}", hex(&mac(&[7; 32], expiry)));
        assert!(!keys.valid(&headers(&value)));
    }

    #[test]
    fn the_key_file_is_created_once_with_mode_0600() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device.key");
        let first = DeviceKeys::load(&path);
        let value = first.issue().unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert!(DeviceKeys::load(&path).valid(&headers(&value)));
    }

    #[test]
    fn a_damaged_or_unreadable_key_file_gives_no_valid_cookie_and_stays() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("device.key");
        fs::write(&path, "damaged").unwrap();
        let keys = DeviceKeys::load(&path);
        assert!(keys.issue().is_none());
        assert_eq!(fs::read_to_string(&path).unwrap(), "damaged");
        // A directory in place of the file gives a read error that is not NotFound.
        let dir_path = dir.path().join("dir.key");
        fs::create_dir(&dir_path).unwrap();
        assert!(DeviceKeys::load(&dir_path).issue().is_none());
    }
}
