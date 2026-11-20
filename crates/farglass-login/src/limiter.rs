//! A rate limiter for failed logins, for each address and each user.
//!
//! The code comes from `rcd/crates/rcdd/src/auth.rs`.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// The failures with no block, for each address and each user.
pub const FREE_FAILURES: u32 = 5;
/// The first block. Each more failure doubles it, up to [`MAX_BLOCK`].
pub const FIRST_BLOCK: Duration = Duration::from_secs(30);
/// The longest block.
pub const MAX_BLOCK: Duration = Duration::from_secs(15 * 60);
/// The limiter removes a counter that had no failure for this time.
pub const FORGET: Duration = Duration::from_secs(60 * 60);
/// The largest number of counters. A larger number removes the oldest counter
/// with no active block. Thus random usernames cannot fill the memory.
/// If each counter has an active block, a new key counts as blocked (ADR-042).
pub const MAX_ENTRIES: usize = 10_000;

/// The thing that the limiter counts.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// An address of a browser.
    Address(String),
    /// A username.
    User(String),
}

#[derive(Debug)]
struct Entry {
    failures: u32,
    last: Instant,
    blocked_until: Option<Instant>,
}

/// Counts failed logins for each address and each username.
#[derive(Debug, Default)]
pub struct Limiter {
    entries: Mutex<HashMap<Key, Entry>>,
}

impl Limiter {
    /// Returns the time until `key` is free, or `None` if it is not blocked.
    pub fn blocked(&self, key: &Key) -> Option<Duration> {
        self.blocked_at(key, Instant::now())
    }

    /// Records a failed login for `key`.
    pub fn failed(&self, key: Key) {
        self.failed_at(key, Instant::now());
    }

    /// Removes the counter of `key` after a login succeeded.
    pub fn succeeded(&self, key: &Key) {
        self.lock().remove(key);
    }

    fn blocked_at(&self, key: &Key, now: Instant) -> Option<Duration> {
        let mut entries = self.lock();
        entries.retain(|_, entry| now.duration_since(entry.last) < FORGET);
        let Some(entry) = entries.get(key) else {
            // A full table of active blocks has no place to count a new key: refuse it.
            return (entries.len() >= MAX_ENTRIES && evictable(&entries, now).is_none())
                .then_some(FIRST_BLOCK);
        };
        let until = entry.blocked_until?;
        until
            .checked_duration_since(now)
            .filter(|left| !left.is_zero())
    }

    fn failed_at(&self, key: Key, now: Instant) {
        let mut entries = self.lock();
        if entries.len() >= MAX_ENTRIES && !entries.contains_key(&key) {
            // Never remove an active block (ADR-042). With no free place, the key stays
            // uncounted, and `blocked` refuses it.
            match evictable(&entries, now) {
                Some(oldest) => {
                    entries.remove(&oldest);
                }
                None => return,
            }
        }
        let entry = entries.entry(key).or_insert(Entry {
            failures: 0,
            last: now,
            blocked_until: None,
        });
        entry.failures += 1;
        entry.last = now;
        if entry.failures >= FREE_FAILURES {
            let doublings = (entry.failures - FREE_FAILURES).min(10);
            let block = FIRST_BLOCK.saturating_mul(1 << doublings).min(MAX_BLOCK);
            entry.blocked_until = Some(now + block);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Key, Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Returns the oldest key with no active block.
fn evictable(entries: &HashMap<Key, Entry>, now: Instant) -> Option<Key> {
    entries
        .iter()
        .filter(|(_, entry)| entry.blocked_until.is_none_or(|until| until <= now))
        .min_by_key(|(_, entry)| entry.last)
        .map(|(key, _)| key.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(name: &str) -> Key {
        Key::User(name.to_owned())
    }

    #[test]
    fn blocks_after_the_free_failures_and_a_success_clears_the_counter() {
        let limiter = Limiter::default();
        let now = Instant::now();
        for _ in 0..FREE_FAILURES - 1 {
            limiter.failed_at(user("alice"), now);
        }
        assert_eq!(limiter.blocked_at(&user("alice"), now), None);
        limiter.failed_at(user("alice"), now);
        assert_eq!(limiter.blocked_at(&user("alice"), now), Some(FIRST_BLOCK));
        assert_eq!(limiter.blocked_at(&user("bob"), now), None);
        limiter.succeeded(&user("alice"));
        assert_eq!(limiter.blocked_at(&user("alice"), now), None);
    }

    #[test]
    fn each_more_failure_doubles_the_block_up_to_the_limit() {
        let limiter = Limiter::default();
        let now = Instant::now();
        for _ in 0..FREE_FAILURES + 1 {
            limiter.failed_at(user("alice"), now);
        }
        assert_eq!(
            limiter.blocked_at(&user("alice"), now),
            Some(FIRST_BLOCK * 2)
        );
        for _ in 0..20 {
            limiter.failed_at(user("alice"), now);
        }
        assert_eq!(limiter.blocked_at(&user("alice"), now), Some(MAX_BLOCK));
    }

    #[test]
    fn block_ends_and_the_counter_goes_after_an_hour() {
        let limiter = Limiter::default();
        let now = Instant::now();
        for _ in 0..FREE_FAILURES {
            limiter.failed_at(user("alice"), now);
        }
        assert_eq!(limiter.blocked_at(&user("alice"), now + FIRST_BLOCK), None);
        limiter.blocked_at(&user("alice"), now + FORGET);
        assert!(limiter.lock().is_empty());
    }

    #[test]
    fn address_and_user_are_separate_counters() {
        let limiter = Limiter::default();
        let now = Instant::now();
        for _ in 0..FREE_FAILURES {
            limiter.failed_at(Key::Address("192.0.2.10".to_owned()), now);
        }
        assert!(
            limiter
                .blocked_at(&Key::Address("192.0.2.10".to_owned()), now)
                .is_some()
        );
        assert_eq!(
            limiter.blocked_at(&Key::User("192.0.2.10".to_owned()), now),
            None
        );
    }

    #[test]
    fn number_of_counters_has_a_limit() {
        let limiter = Limiter::default();
        let now = Instant::now();
        for i in 0..MAX_ENTRIES + 10 {
            limiter.failed_at(
                user(&format!("user{i}")),
                now + Duration::from_millis(i as u64),
            );
        }
        assert_eq!(limiter.lock().len(), MAX_ENTRIES);
        assert!(!limiter.lock().contains_key(&user("user0")));
    }

    #[test]
    fn a_full_table_keeps_each_active_block_and_refuses_a_new_key() {
        let limiter = Limiter::default();
        let now = Instant::now();
        {
            let mut entries = limiter.lock();
            for i in 0..MAX_ENTRIES {
                entries.insert(
                    user(&format!("user{i}")),
                    Entry {
                        failures: FREE_FAILURES,
                        last: now,
                        blocked_until: Some(now + MAX_BLOCK),
                    },
                );
            }
        }
        limiter.failed_at(user("new"), now);
        assert_eq!(limiter.lock().len(), MAX_ENTRIES);
        assert!(limiter.blocked_at(&user("user0"), now).is_some());
        assert_eq!(limiter.blocked_at(&user("new"), now), Some(FIRST_BLOCK));
    }
}
