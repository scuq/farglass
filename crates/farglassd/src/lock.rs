//! The lock of a session: from the shell, after the idle time, and from logind through the agent.
//!
//! While the session of a user is locked, farglassd gives no channel of the
//! user to the agent. Each browser connection of the user stops, and a new
//! connection gets the lock screen. Only a login of the user unlocks the
//! session, or an unlock request of logind.

use farglass_login::audit::Event;
use farglass_proto::message::LinkMessage;

use crate::agents::{AgentEvent, Registry};
use crate::presence::Presence;

/// Locks the session of `uid`, stops each browser connection of the user, and sets the LockedHint of logind.
pub fn lock(presence: &Presence, registry: &Registry, uid: u32, cause: &str) {
    if presence.lock_user(uid) {
        Event {
            event: "lock",
            reason: cause,
            uid: Some(uid),
            ..Event::default()
        }
        .accepted();
        hint(registry, uid, true);
    }
}

/// Unlocks the session of `uid` and clears the LockedHint of logind.
pub fn unlock(presence: &Presence, registry: &Registry, uid: u32, cause: &str) {
    if presence.unlock_user(uid) {
        Event {
            event: "unlock",
            reason: cause,
            uid: Some(uid),
            ..Event::default()
        }
        .accepted();
        hint(registry, uid, false);
    }
}

fn hint(registry: &Registry, uid: u32, locked: bool) {
    if let Some(agent) = registry.get(uid) {
        agent.tell(&LinkMessage::LockedHint { locked });
    }
}

/// Follows the requests of the agents: the lock and unlock signals of logind.
/// A new agent gets the lock state of its session.
pub async fn agent_loop(presence: std::sync::Arc<Presence>, registry: std::sync::Arc<Registry>) {
    let mut events = registry.subscribe();
    loop {
        match events.recv().await {
            Ok(AgentEvent::Connected(uid)) => hint(&registry, uid, presence.is_locked(uid)),
            Ok(AgentEvent::Lock(uid)) => lock(&presence, &registry, uid, "logind"),
            Ok(AgentEvent::Unlock(uid)) => unlock(&presence, &registry, uid, "logind"),
            Ok(AgentEvent::Gone(_)) => {}
            Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                tracing::warn!(count, "lock requests of agents were lost");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        }
    }
}
