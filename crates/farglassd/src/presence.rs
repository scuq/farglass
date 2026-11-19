//! The browser connections of each user, for the idle logout, and the logout announcement.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;

/// The default time with no browser connection before the idle logout (8 hours).
pub const IDLE_LOGOUT: Duration = Duration::from_secs(8 * 60 * 60);

#[derive(Debug)]
struct Entry {
    connections: usize,
    /// The time from which the user had no connection.
    idle_since: Instant,
}

/// Counts the browser connections of each user and announces logouts to them.
#[derive(Debug)]
pub struct Presence {
    users: Mutex<HashMap<u32, Entry>>,
    logouts: broadcast::Sender<u32>,
}

impl Default for Presence {
    fn default() -> Self {
        Self {
            users: Mutex::new(HashMap::new()),
            logouts: broadcast::channel(64).0,
        }
    }
}

/// One browser connection of a user. Dropping it ends the count of the connection.
#[derive(Debug)]
pub struct PresenceGuard {
    presence: Arc<Presence>,
    uid: u32,
}

impl Drop for PresenceGuard {
    fn drop(&mut self) {
        let mut users = self.presence.lock();
        if let Some(entry) = users.get_mut(&self.uid) {
            entry.connections = entry.connections.saturating_sub(1);
            if entry.connections == 0 {
                entry.idle_since = Instant::now();
            }
        }
    }
}

impl Presence {
    /// Counts a new browser connection of `uid` until the guard drops.
    #[must_use]
    pub fn enter(self: &Arc<Self>, uid: u32) -> PresenceGuard {
        self.lock()
            .entry(uid)
            .or_insert(Entry {
                connections: 0,
                idle_since: Instant::now(),
            })
            .connections += 1;
        PresenceGuard {
            presence: self.clone(),
            uid,
        }
    }

    /// Returns the users of `sessions` that had no browser connection for `timeout` at `now`.
    ///
    /// A user that farglassd did not see before starts its idle time now, for
    /// example an agent that connected again after a restart of farglassd.
    pub fn idle_users(&self, sessions: &[u32], timeout: Duration, now: Instant) -> Vec<u32> {
        let mut users = self.lock();
        sessions
            .iter()
            .copied()
            .filter(|uid| {
                let entry = users.entry(*uid).or_insert(Entry {
                    connections: 0,
                    idle_since: now,
                });
                entry.connections == 0 && now.duration_since(entry.idle_since) >= timeout
            })
            .collect()
    }

    /// Returns a receiver that gets the uid of each logout.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<u32> {
        self.logouts.subscribe()
    }

    /// Tells each browser connection of `uid` that the user logged out, and restarts the idle time.
    pub fn announce_logout(&self, uid: u32) {
        if let Some(entry) = self.lock().get_mut(&uid) {
            entry.idle_since = Instant::now();
        }
        // An error means that no connection listens.
        let _ = self.logouts.send(uid);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Entry>> {
        self.users.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: Duration = Duration::from_secs(3600);

    #[test]
    fn user_with_a_connection_is_never_idle() {
        let presence = Arc::new(Presence::default());
        let now = Instant::now();
        let _tab = presence.enter(1000);
        assert!(
            presence
                .idle_users(&[1000], HOUR, now + 10 * HOUR)
                .is_empty()
        );
    }

    #[test]
    fn idle_time_starts_when_the_last_connection_closes() {
        let presence = Arc::new(Presence::default());
        let first = presence.enter(1000);
        let second = presence.enter(1000);
        drop(first);
        assert!(
            presence
                .idle_users(&[1000], Duration::ZERO, Instant::now())
                .is_empty()
        );
        drop(second);
        let closed = Instant::now();
        assert!(
            presence
                .idle_users(&[1000], HOUR, closed + HOUR / 2)
                .is_empty()
        );
        assert_eq!(
            presence.idle_users(&[1000], HOUR, closed + 2 * HOUR),
            [1000]
        );
    }

    #[test]
    fn session_with_no_known_connection_starts_its_idle_time_at_the_first_look() {
        let presence = Presence::default();
        let now = Instant::now();
        assert!(presence.idle_users(&[1001], HOUR, now).is_empty());
        assert_eq!(presence.idle_users(&[1001], HOUR, now + HOUR), [1001]);
    }

    #[test]
    fn logout_reaches_each_subscriber_and_restarts_the_idle_time() {
        let presence = Arc::new(Presence::default());
        let mut a = presence.subscribe();
        let mut b = presence.subscribe();
        drop(presence.enter(1000));
        presence.announce_logout(1000);
        assert_eq!(a.try_recv().unwrap(), 1000);
        assert_eq!(b.try_recv().unwrap(), 1000);
        assert!(
            presence
                .idle_users(&[1000], HOUR, Instant::now())
                .is_empty()
        );
    }
}
