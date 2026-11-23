//! The browser connections and the activity of each user, the lock state of
//! each session, and the announcements of logouts, locks, and unlocks.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;

/// The default time with no browser connection before the idle logout (8 hours).
pub const IDLE_LOGOUT: Duration = Duration::from_secs(8 * 60 * 60);

/// The default time with no keyboard or pointer activity before the idle lock (48 minutes).
pub const IDLE_LOCK: Duration = Duration::from_secs(48 * 60);

#[derive(Debug)]
struct Entry {
    connections: usize,
    /// The time from which the user had no connection.
    idle_since: Instant,
    /// The last keyboard or pointer activity in a tab of the user.
    active_at: Instant,
    locked: bool,
}

impl Entry {
    const fn new(now: Instant) -> Self {
        Self {
            connections: 0,
            idle_since: now,
            active_at: now,
            locked: false,
        }
    }
}

/// A change of a user that each browser connection of the user must know.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserEvent {
    /// The user logged out.
    LoggedOut(u32),
    /// The session of the user is locked.
    Locked(u32),
    /// The session of the user is unlocked.
    Unlocked(u32),
}

/// Counts the browser connections of each user, keeps the lock state of each
/// session, and announces logouts, locks, and unlocks to the connections.
#[derive(Debug)]
pub struct Presence {
    users: Mutex<HashMap<u32, Entry>>,
    events: broadcast::Sender<UserEvent>,
}

impl Default for Presence {
    fn default() -> Self {
        Self {
            users: Mutex::new(HashMap::new()),
            events: broadcast::channel(64).0,
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
            .or_insert_with(|| Entry::new(Instant::now()))
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
                let entry = users.entry(*uid).or_insert_with(|| Entry::new(now));
                entry.connections == 0 && now.duration_since(entry.idle_since) >= timeout
            })
            .collect()
    }

    /// Returns the users of `sessions` that are not locked and had no activity for `timeout` at `now`.
    ///
    /// A user that farglassd did not see before starts its time now.
    pub fn inactive_users(&self, sessions: &[u32], timeout: Duration, now: Instant) -> Vec<u32> {
        let mut users = self.lock();
        sessions
            .iter()
            .copied()
            .filter(|uid| {
                let entry = users.entry(*uid).or_insert_with(|| Entry::new(now));
                !entry.locked && now.duration_since(entry.active_at) >= timeout
            })
            .collect()
    }

    /// Notes keyboard or pointer activity of `uid`. A locked session ignores it.
    pub fn activity(&self, uid: u32) {
        if let Some(entry) = self.lock().get_mut(&uid)
            && !entry.locked
        {
            entry.active_at = Instant::now();
        }
    }

    /// Reports whether the session of `uid` is locked.
    #[must_use]
    pub fn is_locked(&self, uid: u32) -> bool {
        self.lock().get(&uid).is_some_and(|entry| entry.locked)
    }

    /// Locks the session of `uid` and tells each browser connection of the user.
    /// Returns `false` if it was locked already.
    pub fn lock_user(&self, uid: u32) -> bool {
        {
            let mut users = self.lock();
            let entry = users
                .entry(uid)
                .or_insert_with(|| Entry::new(Instant::now()));
            if entry.locked {
                return false;
            }
            entry.locked = true;
        }
        let _ = self.events.send(UserEvent::Locked(uid));
        true
    }

    /// Unlocks the session of `uid`, restarts its activity time, and tells each
    /// browser connection of the user. Returns `false` if it was not locked.
    pub fn unlock_user(&self, uid: u32) -> bool {
        {
            let mut users = self.lock();
            let Some(entry) = users.get_mut(&uid).filter(|entry| entry.locked) else {
                return false;
            };
            entry.locked = false;
            entry.active_at = Instant::now();
        }
        let _ = self.events.send(UserEvent::Unlocked(uid));
        true
    }

    /// Returns a receiver that gets each logout, lock, and unlock.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<UserEvent> {
        self.events.subscribe()
    }

    /// Tells each browser connection of `uid` that the user logged out, and restarts the idle time.
    /// The session of a logout is not locked.
    pub fn announce_logout(&self, uid: u32) {
        if let Some(entry) = self.lock().get_mut(&uid) {
            let now = Instant::now();
            entry.idle_since = now;
            entry.active_at = now;
            entry.locked = false;
        }
        // An error means that no connection listens.
        let _ = self.events.send(UserEvent::LoggedOut(uid));
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
        assert_eq!(a.try_recv().unwrap(), UserEvent::LoggedOut(1000));
        assert_eq!(b.try_recv().unwrap(), UserEvent::LoggedOut(1000));
        assert!(
            presence
                .idle_users(&[1000], HOUR, Instant::now())
                .is_empty()
        );
    }

    #[test]
    fn lock_and_unlock_are_announced_one_time_and_a_logout_unlocks() {
        let presence = Arc::new(Presence::default());
        let mut events = presence.subscribe();
        assert!(presence.lock_user(1000));
        assert!(!presence.lock_user(1000));
        assert!(presence.is_locked(1000));
        assert!(presence.unlock_user(1000));
        assert!(!presence.unlock_user(1000));
        assert_eq!(events.try_recv().unwrap(), UserEvent::Locked(1000));
        assert_eq!(events.try_recv().unwrap(), UserEvent::Unlocked(1000));
        assert!(events.try_recv().is_err());
        presence.lock_user(1000);
        presence.announce_logout(1000);
        assert!(!presence.is_locked(1000));
    }

    #[test]
    fn inactive_users_skip_locked_sessions_and_activity_restarts_the_time() {
        let presence = Arc::new(Presence::default());
        let _tab = presence.enter(1000);
        let _other = presence.enter(1001);
        let later = Instant::now() + HOUR;
        assert_eq!(
            presence.inactive_users(&[1000, 1001], HOUR / 2, later),
            [1000, 1001]
        );
        presence.lock_user(1001);
        presence.activity(1000);
        assert!(
            presence
                .inactive_users(&[1000, 1001], HOUR / 2, Instant::now())
                .is_empty()
        );
        assert_eq!(
            presence.inactive_users(&[1000, 1001], HOUR / 2, later),
            [1000]
        );
    }
}
