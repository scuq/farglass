//! The machine PIN (F059): an optional filter before the web login.
//!
//! When the file `STATE_DIR/pin` exists, a browser connection with no user
//! must give the PIN before it can open a login channel. The PIN is not a
//! factor: the password and the passkey stay necessary.
//!
//! The file holds `v1:SALT:HASH`: a random salt and an iterated SHA-256 of the
//! salt and the PIN, in hexadecimal. `farglassd --set-pin` writes it.
//!
//! Faults close the gate (ADR-042): a file that cannot be read or parsed
//! requires the PIN and refuses each PIN. Only a missing file turns the PIN off.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write as _};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use sha2::{Digest as _, Sha256};

/// The wrong PINs from one address group before a block.
pub const FREE_FAILURES: u32 = 5;
/// The block after [`FREE_FAILURES`] wrong PINs.
pub const BLOCK: Duration = Duration::from_secs(15 * 60);
/// The wait after a wrong PIN, before the answer.
pub const FAIL_DELAY: Duration = Duration::from_secs(1);
/// A counter with no failure for this time goes away.
const FORGET: Duration = Duration::from_secs(60 * 60);
/// The largest number of counters.
const MAX_ENTRIES: usize = 10_000;
/// The rounds of SHA-256 for one PIN.
const ROUNDS: u32 = 20_000;

/// The state of the PIN file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PinState {
    /// No PIN file: the gate is open.
    Off,
    /// The salt and the hash of the PIN.
    On {
        /// The salt.
        salt: Vec<u8>,
        /// The hash.
        hash: [u8; 32],
    },
    /// The file exists but cannot be read or parsed: the gate is closed (ADR-042).
    Fault(String),
}

/// The result of a PIN check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinCheck {
    /// The PIN is correct, or the gate is open.
    Accepted,
    /// The PIN is wrong.
    Wrong,
    /// The address group has too many wrong PINs. The value is the time left.
    Blocked(Duration),
    /// A fault of the host stopped the check: the PIN is refused.
    Fault,
}

#[derive(Debug)]
struct Entry {
    failures: u32,
    last: Instant,
    blocked_until: Option<Instant>,
}

/// The PIN file and the counters of wrong PINs.
#[derive(Debug, Default)]
pub struct PinStore {
    path: Option<PathBuf>,
    entries: Mutex<HashMap<String, Entry>>,
}

impl PinStore {
    /// Returns a store for the PIN file at `path`. `None` turns the PIN off, for tests.
    #[must_use]
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            entries: Mutex::default(),
        }
    }

    /// Returns the state of the PIN file. The file is read each time: a new PIN applies at once.
    #[must_use]
    pub fn state(&self) -> PinState {
        let Some(path) = &self.path else {
            return PinState::Off;
        };
        match fs::read_to_string(path) {
            Ok(text) => parse(text.trim()).map_or_else(
                || PinState::Fault(format!("{}: the content is not valid", path.display())),
                |(salt, hash)| PinState::On { salt, hash },
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => PinState::Off,
            Err(error) => PinState::Fault(format!("{}: {error}", path.display())),
        }
    }

    /// Reports whether a connection with no user must give the PIN.
    #[must_use]
    pub fn required(&self) -> bool {
        self.state() != PinState::Off
    }

    /// Checks `pin` for the address group `group`.
    #[must_use]
    pub fn check(&self, group: &str, pin: &str) -> PinCheck {
        self.check_at(group, pin, Instant::now())
    }

    fn check_at(&self, group: &str, pin: &str, now: Instant) -> PinCheck {
        if let Some(left) = self.blocked(group, now) {
            return PinCheck::Blocked(left);
        }
        match self.state() {
            PinState::Off => PinCheck::Accepted,
            PinState::Fault(_) => PinCheck::Fault,
            PinState::On { salt, hash } => {
                if equal(&derive(&salt, pin), &hash) {
                    self.lock().remove(group);
                    PinCheck::Accepted
                } else {
                    self.failed(group, now);
                    PinCheck::Wrong
                }
            }
        }
    }

    fn blocked(&self, group: &str, now: Instant) -> Option<Duration> {
        let mut entries = self.lock();
        entries.retain(|_, entry| now.duration_since(entry.last) < FORGET);
        let Some(entry) = entries.get(group) else {
            // A full table of active blocks has no place for a new group: refuse it (ADR-042).
            return (entries.len() >= MAX_ENTRIES && evictable(&entries, now).is_none())
                .then_some(BLOCK);
        };
        entry
            .blocked_until?
            .checked_duration_since(now)
            .filter(|left| !left.is_zero())
    }

    fn failed(&self, group: &str, now: Instant) {
        let mut entries = self.lock();
        if entries.len() >= MAX_ENTRIES && !entries.contains_key(group) {
            match evictable(&entries, now) {
                Some(oldest) => {
                    entries.remove(&oldest);
                }
                None => return,
            }
        }
        let entry = entries.entry(group.to_owned()).or_insert(Entry {
            failures: 0,
            last: now,
            blocked_until: None,
        });
        entry.failures += 1;
        entry.last = now;
        if entry.failures >= FREE_FAILURES {
            entry.blocked_until = Some(now + BLOCK);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Returns the oldest group with no active block.
fn evictable(entries: &HashMap<String, Entry>, now: Instant) -> Option<String> {
    entries
        .iter()
        .filter(|(_, entry)| entry.blocked_until.is_none_or(|until| until <= now))
        .min_by_key(|(_, entry)| entry.last)
        .map(|(group, _)| group.clone())
}

/// Reports whether `pin` has the form of a PIN: 4 to 6 digits.
#[must_use]
pub fn valid_form(pin: &str) -> bool {
    (4..=6).contains(&pin.len()) && pin.bytes().all(|byte| byte.is_ascii_digit())
}

/// Writes the PIN file at `path` with a new salt, mode 0600, and the owner of its directory.
///
/// The directory owner is the user of farglassd. Thus root can set the PIN, and farglassd can read it.
///
/// # Errors
///
/// An error if `pin` is not 4 to 6 digits, or if the file cannot be written.
pub fn write_pin(path: &Path, pin: &str) -> io::Result<()> {
    if !valid_form(pin) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the PIN must have 4 to 6 digits",
        ));
    }
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let salt = crate::cookie::secret();
    let salt = salt.get(..32).unwrap_or(&salt).as_bytes().to_vec();
    let line = format!(
        "v1:{}:{}\n",
        String::from_utf8_lossy(&salt),
        hex(&derive(&salt, pin))
    );
    let temporary = dir.join(".pin.new");
    let _ = fs::remove_file(&temporary);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(line.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    let meta = fs::metadata(dir)?;
    {
        use std::os::unix::fs::MetadataExt as _;
        std::os::unix::fs::chown(&temporary, Some(meta.uid()), Some(meta.gid()))?;
    }
    fs::rename(&temporary, path)
}

/// Removes the PIN file at `path`. A missing file is not an error.
///
/// # Errors
///
/// An error if the file exists and cannot be removed.
pub fn clear_pin(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn parse(text: &str) -> Option<(Vec<u8>, [u8; 32])> {
    let mut parts = text.split(':');
    if parts.next()? != "v1" {
        return None;
    }
    let salt = parts.next()?;
    let hash = parts.next()?;
    if parts.next().is_some() || salt.is_empty() || hash.len() != 64 {
        return None;
    }
    let mut out = [0_u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hash.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some((salt.as_bytes().to_vec(), out))
}

fn derive(salt: &[u8], pin: &str) -> [u8; 32] {
    let mut digest: [u8; 32] = Sha256::new()
        .chain_update(salt)
        .chain_update(pin.as_bytes())
        .finalize()
        .into();
    for _ in 1..ROUNDS {
        digest = Sha256::new()
            .chain_update(salt)
            .chain_update(digest)
            .finalize()
            .into();
    }
    digest
}

/// Compares in constant time.
fn equal(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0_u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(pin: &str) -> (tempfile::TempDir, PinStore) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pin");
        write_pin(&path, pin).unwrap();
        (dir, PinStore::new(Some(path)))
    }

    #[test]
    fn the_form_is_4_to_6_digits() {
        for good in ["1234", "12345", "123456"] {
            assert!(valid_form(good));
        }
        for bad in ["123", "1234567", "12a4", "", " 1234"] {
            assert!(!valid_form(bad), "{bad}");
        }
        let dir = tempfile::tempdir().unwrap();
        assert!(write_pin(&dir.path().join("pin"), "12").is_err());
    }

    #[test]
    fn the_file_has_mode_0600_and_no_pin_in_plain_text() {
        let (dir, store) = store_with("482913");
        let path = dir.path().join("pin");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!fs::read_to_string(&path).unwrap().contains("482913"));
        assert!(store.required());
    }

    #[test]
    fn a_correct_pin_passes_and_a_wrong_one_does_not() {
        let (_dir, store) = store_with("482913");
        assert_eq!(store.check("203.0.113.7", "482913"), PinCheck::Accepted);
        assert_eq!(store.check("203.0.113.7", "000000"), PinCheck::Wrong);
    }

    #[test]
    fn five_wrong_pins_block_the_address_group_also_for_the_correct_pin() {
        let (_dir, store) = store_with("4829");
        let now = Instant::now();
        for _ in 0..FREE_FAILURES {
            assert_eq!(store.check_at("g", "0000", now), PinCheck::Wrong);
        }
        assert_eq!(store.check_at("g", "4829", now), PinCheck::Blocked(BLOCK));
        assert_eq!(store.check_at("other", "4829", now), PinCheck::Accepted);
        assert_eq!(store.check_at("g", "4829", now + BLOCK), PinCheck::Accepted);
    }

    #[test]
    fn no_file_turns_the_pin_off() {
        let dir = tempfile::tempdir().unwrap();
        let store = PinStore::new(Some(dir.path().join("pin")));
        assert!(!store.required());
        assert_eq!(store.check("g", "anything"), PinCheck::Accepted);
        assert!(!PinStore::new(None).required());
    }

    #[test]
    fn a_damaged_or_unreadable_file_requires_the_pin_and_refuses_each_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pin");
        fs::write(&path, "damaged").unwrap();
        let store = PinStore::new(Some(path.clone()));
        assert!(store.required());
        assert_eq!(store.check("g", "1234"), PinCheck::Fault);
        // A directory in place of the file gives a read error that is not NotFound.
        let dir_path = dir.path().join("pin-dir");
        fs::create_dir(&dir_path).unwrap();
        let store = PinStore::new(Some(dir_path));
        assert!(store.required());
        assert!(matches!(store.state(), PinState::Fault(_)));
        assert_eq!(store.check("g", "1234"), PinCheck::Fault);
    }

    #[test]
    fn a_full_table_keeps_each_block_and_refuses_a_new_group() {
        let (_dir, store) = store_with("4829");
        let now = Instant::now();
        {
            let mut entries = store.lock();
            for i in 0..MAX_ENTRIES {
                entries.insert(
                    format!("g{i}"),
                    Entry {
                        failures: FREE_FAILURES,
                        last: now,
                        blocked_until: Some(now + BLOCK),
                    },
                );
            }
        }
        assert_eq!(store.check_at("new", "4829", now), PinCheck::Blocked(BLOCK));
        assert!(matches!(
            store.check_at("g0", "4829", now),
            PinCheck::Blocked(_)
        ));
    }

    #[test]
    fn clear_removes_the_file() {
        let (dir, store) = store_with("4829");
        clear_pin(&dir.path().join("pin")).unwrap();
        clear_pin(&dir.path().join("pin")).unwrap();
        assert!(!store.required());
    }
}
