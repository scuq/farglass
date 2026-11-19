//! The logind session of the agent: its lock and unlock signals, and its LockedHint.
//!
//! `loginctl lock-session` sends the signal Lock to the session object. The
//! agent gives it to farglassd, which locks the session in each tab. When
//! farglassd locks or unlocks the session, the agent sets LockedHint, thus
//! `loginctl` and other programs see the state.

use std::num::NonZeroUsize;
use std::sync::Arc;

use farglass_proto::message::Bus;
use futures_util::StreamExt as _;
use tokio::sync::{OnceCell, broadcast};
use zbus::OwnedObjectPath;

use crate::dbus::Buses;

const LOGIND: &str = "org.freedesktop.login1";
const SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";

/// The logind session of the agent.
#[derive(Debug)]
pub struct Logind {
    buses: Arc<Buses>,
    path: OnceCell<OwnedObjectPath>,
    requests: broadcast::Sender<bool>,
}

impl Logind {
    /// Returns the logind session that uses the system bus of `buses`. It knows its session after [`Logind::watch`].
    #[must_use]
    pub fn new(buses: Arc<Buses>) -> Arc<Self> {
        Arc::new(Self {
            buses,
            path: OnceCell::new(),
            requests: broadcast::channel(8).0,
        })
    }

    /// Returns a receiver of the requests of logind: `true` to lock, `false` to unlock.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<bool> {
        self.requests.subscribe()
    }

    /// Finds the session `session_id` and gives its signals Lock and Unlock to the subscribers, until the bus stops.
    ///
    /// # Errors
    ///
    /// A D-Bus error if logind does not know the session or refuses the match rule.
    pub async fn watch(self: Arc<Self>, session_id: &str) -> zbus::Result<()> {
        let connection = self.buses.connection(Bus::System).await?;
        let reply = connection
            .call_method(
                Some(LOGIND),
                "/org/freedesktop/login1",
                Some("org.freedesktop.login1.Manager"),
                "GetSession",
                &(session_id,),
            )
            .await?;
        let path: OwnedObjectPath = reply.body().deserialize()?;
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(LOGIND)
            .interface(SESSION_INTERFACE)
            .path(path.clone())
            .build()?;
        let mut stream =
            zbus::MessageStream::for_match_rule(rule, &connection, NonZeroUsize::new(8)).await?;
        let _ = self.path.set(path);
        tracing::info!(session = session_id, "following the lock signals of logind");
        while let Some(message) = stream.next().await {
            let message = message?;
            let lock = match message.header().member().map(|member| member.as_str()) {
                Some("Lock") => true,
                Some("Unlock") => false,
                _ => continue,
            };
            tracing::info!(lock, "logind asks to change the lock of the session");
            let _ = self.requests.send(lock);
        }
        Ok(())
    }

    /// Sets LockedHint of the session in a separate task. Does nothing before [`Logind::watch`] found the session.
    pub fn set_locked_hint(self: &Arc<Self>, locked: bool) {
        let Some(path) = self.path.get().cloned() else {
            return;
        };
        let buses = self.buses.clone();
        tokio::spawn(async move {
            let result = async {
                let connection = buses.connection(Bus::System).await?;
                connection
                    .call_method(
                        Some(LOGIND),
                        path,
                        Some(SESSION_INTERFACE),
                        "SetLockedHint",
                        &(locked,),
                    )
                    .await
                    .map(drop)
            }
            .await;
            if let Err(error) = result {
                tracing::warn!(%error, locked, "logind did not take the LockedHint");
            }
        });
    }
}
