//! The desktop notifications of the session.
//!
//! The agent owns `org.freedesktop.Notifications` on the session bus and
//! keeps each notification until the user closes it. A notify channel of the
//! shell gets each notification that is open. Thus a notification waits while
//! no client is connected, and its time to show starts in the shell.
//! The actions and the close events go back to the program as signals.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use farglass_proto::message::{Notification, NotifyAction, NotifyEvent, NotifyReply, Urgency};
use tokio::sync::mpsc;
use zbus::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::object_server::SignalEmitter;

/// The bus name of the notification service.
pub const NOTIFY_NAME: &str = "org.freedesktop.Notifications";

/// The object path of the notification service.
pub const NOTIFY_PATH: &str = "/org/freedesktop/Notifications";

/// The largest number of open notifications. A new one closes the oldest.
pub const MAX_NOTIFICATIONS: usize = 100;

/// The time to show a notification of normal urgency, when the program gives none.
const NORMAL_TIMEOUT_MS: u32 = 6_000;

/// The time to show a notification of low urgency, when the program gives none.
const LOW_TIMEOUT_MS: u32 = 4_000;

/// The reasons of the signal NotificationClosed, from the specification.
pub mod reason {
    /// The time ended.
    pub const EXPIRED: u32 = 1;
    /// The user closed it.
    pub const DISMISSED: u32 = 2;
    /// The program closed it with CloseNotification.
    pub const CLOSED: u32 = 3;
    /// A different reason, for example the limit of open notifications.
    pub const UNDEFINED: u32 = 4;
}

/// A signal for the program that sent a notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signal {
    /// NotificationClosed.
    Closed {
        /// The id of the notification.
        id: u32,
        /// One of [`reason`].
        reason: u32,
    },
    /// ActionInvoked.
    Action {
        /// The id of the notification.
        id: u32,
        /// The key of the action.
        key: String,
    },
}

/// A notification as a program sends it with Notify.
#[derive(Clone, Debug, Default)]
pub struct Incoming {
    /// The name of the program.
    pub app: String,
    /// The id of a notification that this one replaces, or 0.
    pub replaces_id: u32,
    /// The summary.
    pub summary: String,
    /// The body.
    pub body: String,
    /// The actions as pairs of key and label, flat.
    pub actions: Vec<String>,
    /// The hint `urgency`: 0 low, 1 normal, 2 critical.
    pub urgency: Option<u8>,
    /// The hint `resident`: the notification stays after an action.
    pub resident: bool,
    /// The time in milliseconds: -1 for the default of the server, 0 for no end.
    pub expire_timeout: i32,
}

type EventSink = Arc<dyn Fn(NotifyEvent) + Send + Sync>;

#[derive(Default)]
struct Inner {
    items: BTreeMap<u32, (Notification, bool)>,
    next_id: u32,
    viewers: HashMap<u64, EventSink>,
}

/// The open notifications and the notify channels of the shell.
pub struct Notifications {
    inner: Mutex<Inner>,
    next_viewer: AtomicU64,
    signals: mpsc::UnboundedSender<Signal>,
    receiver: Mutex<Option<mpsc::UnboundedReceiver<Signal>>>,
}

impl std::fmt::Debug for Notifications {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Notifications").finish_non_exhaustive()
    }
}

/// One open notify channel. Dropping it removes the channel.
pub struct ViewerGuard {
    notifications: Arc<Notifications>,
    id: u64,
}

impl std::fmt::Debug for ViewerGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewerGuard").field("id", &self.id).finish()
    }
}

impl Drop for ViewerGuard {
    fn drop(&mut self) {
        self.notifications.lock().viewers.remove(&self.id);
    }
}

impl Notifications {
    /// Returns an empty set of notifications.
    #[must_use]
    pub fn new() -> Arc<Self> {
        let (signals, receiver) = mpsc::unbounded_channel();
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            next_viewer: AtomicU64::new(1),
            signals,
            receiver: Mutex::new(Some(receiver)),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes the queue of the signals for the programs. The D-Bus service takes it one time.
    pub fn take_signals(&self) -> Option<mpsc::UnboundedReceiver<Signal>> {
        self.receiver
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    fn broadcast(&self, event: &NotifyEvent) {
        let viewers: Vec<EventSink> = self.lock().viewers.values().cloned().collect();
        for send in viewers {
            send(event.clone());
        }
    }

    /// Adds a notify channel. It gets each open notification at once, then each change.
    pub fn add_viewer(
        self: &Arc<Self>,
        send: impl Fn(NotifyEvent) + Send + Sync + 'static,
    ) -> ViewerGuard {
        let id = self.next_viewer.fetch_add(1, Ordering::Relaxed);
        let send: EventSink = Arc::new(send);
        let open: Vec<Notification> = {
            let mut inner = self.lock();
            inner.viewers.insert(id, send.clone());
            inner
                .items
                .values()
                .map(|(notification, _)| notification.clone())
                .collect()
        };
        for notification in open {
            send(NotifyEvent::Show { notification });
        }
        ViewerGuard {
            notifications: self.clone(),
            id,
        }
    }

    /// Adds or replaces a notification. Returns its id.
    pub fn notify(&self, incoming: Incoming) -> u32 {
        let urgency = match incoming.urgency {
            Some(0) => Urgency::Low,
            Some(2) => Urgency::Critical,
            _ => Urgency::Normal,
        };
        let timeout_ms = match (incoming.expire_timeout, urgency) {
            (0, _) | (_, Urgency::Critical) => None,
            (ms, _) if ms > 0 => Some(ms.unsigned_abs()),
            (_, Urgency::Low) => Some(LOW_TIMEOUT_MS),
            (_, Urgency::Normal) => Some(NORMAL_TIMEOUT_MS),
        };
        let actions = incoming
            .actions
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[key, label]| NotifyAction {
                key: key.clone(),
                label: label.clone(),
            })
            .collect();
        let (notification, dropped) = {
            let mut inner = self.lock();
            let id = if incoming.replaces_id != 0 && inner.items.contains_key(&incoming.replaces_id)
            {
                incoming.replaces_id
            } else {
                inner.next_id = inner.next_id.wrapping_add(1).max(1);
                inner.next_id
            };
            let notification = Notification {
                id,
                app: incoming.app,
                summary: incoming.summary,
                body: incoming.body,
                actions,
                urgency,
                timeout_ms,
            };
            inner
                .items
                .insert(id, (notification.clone(), incoming.resident));
            let dropped = if inner.items.len() > MAX_NOTIFICATIONS {
                inner.items.pop_first().map(|(old, _)| old)
            } else {
                None
            };
            (notification, dropped)
        };
        if let Some(old) = dropped {
            self.broadcast(&NotifyEvent::Close { id: old });
            let _ = self.signals.send(Signal::Closed {
                id: old,
                reason: reason::UNDEFINED,
            });
        }
        let id = notification.id;
        self.broadcast(&NotifyEvent::Show { notification });
        id
    }

    /// Closes a notification with `reason`. Returns `false` if it is not open.
    pub fn close(&self, id: u32, reason: u32) -> bool {
        if self.lock().items.remove(&id).is_none() {
            return false;
        }
        self.broadcast(&NotifyEvent::Close { id });
        let _ = self.signals.send(Signal::Closed { id, reason });
        true
    }

    /// Handles a reply of the shell. After an action, the notification closes, unless it is resident.
    pub fn reply(&self, reply: NotifyReply) {
        match reply {
            NotifyReply::Dismiss { id } => {
                self.close(id, reason::DISMISSED);
            }
            NotifyReply::Expire { id } => {
                self.close(id, reason::EXPIRED);
            }
            NotifyReply::Action { id, key } => {
                let Some(resident) = self.lock().items.get(&id).map(|(_, resident)| *resident)
                else {
                    return;
                };
                let _ = self.signals.send(Signal::Action { id, key });
                if !resident {
                    self.close(id, reason::DISMISSED);
                }
            }
        }
    }

    /// Returns the number of open notifications.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().items.len()
    }

    /// Reports whether no notification is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The object `org.freedesktop.Notifications` on the session bus.
struct NotifyObject {
    notifications: Arc<Notifications>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl NotifyObject {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        _app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, zbus::OwnedVariant>,
        expire_timeout: i32,
    ) -> u32 {
        let urgency = hints
            .get("urgency")
            .and_then(|value| u8::try_from(&**value).ok());
        let resident = hints
            .get("resident")
            .and_then(|value| bool::try_from(&**value).ok())
            .unwrap_or(false);
        self.notifications.notify(Incoming {
            app: app_name,
            replaces_id,
            summary,
            body,
            actions,
            urgency,
            resident,
            expire_timeout,
        })
    }

    fn close_notification(&self, id: u32) {
        self.notifications.close(id, reason::CLOSED);
    }

    /// The shell shows the body as plain text and gives action buttons.
    fn get_capabilities(&self) -> Vec<&'static str> {
        vec!["actions", "body", "persistence"]
    }

    fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        ("farglass", "farglass", env!("CARGO_PKG_VERSION"), "1.2")
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

/// Serves the notifications on `connection` under the bus name `name`, and sends the signals.
///
/// The agent does not take the name from a different owner, for example the
/// notification service of a local desktop. It waits in the queue of the bus
/// and gets the name when the other owner gives it up.
///
/// # Errors
///
/// A D-Bus error if the bus refuses the object or the request for the name.
pub async fn serve(
    notifications: Arc<Notifications>,
    connection: &Connection,
    name: &str,
) -> zbus::Result<()> {
    let Some(mut signals) = notifications.take_signals() else {
        return Ok(());
    };
    connection
        .object_server()
        .at(NOTIFY_PATH, NotifyObject { notifications })
        .await?;
    match connection
        .request_name_with_flags(name, RequestNameFlags::empty())
        .await?
    {
        RequestNameReply::InQueue => {
            tracing::warn!(
                name,
                "a different program owns the notification name, the agent waits for it"
            );
        }
        _ => tracing::info!(name, "serving the desktop notifications"),
    }
    let emitter = SignalEmitter::new(connection, NOTIFY_PATH)?.into_owned();
    tokio::spawn(async move {
        while let Some(signal) = signals.recv().await {
            let sent = match signal {
                Signal::Closed { id, reason } => {
                    NotifyObject::notification_closed(&emitter, id, reason).await
                }
                Signal::Action { id, key } => {
                    NotifyObject::action_invoked(&emitter, id, &key).await
                }
            };
            if let Err(error) = sent {
                tracing::warn!(%error, "a notification signal did not go");
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn incoming(summary: &str) -> Incoming {
        Incoming {
            app: "test".to_owned(),
            summary: summary.to_owned(),
            expire_timeout: -1,
            ..Incoming::default()
        }
    }

    #[test]
    fn timeouts_follow_the_urgency_and_the_program() {
        let notifications = Notifications::new();
        let shown = |incoming: Incoming| {
            let id = notifications.notify(incoming);
            notifications.lock().items[&id].0.timeout_ms
        };
        assert_eq!(shown(incoming("a")), Some(NORMAL_TIMEOUT_MS));
        assert_eq!(
            shown(Incoming {
                urgency: Some(0),
                ..incoming("b")
            }),
            Some(LOW_TIMEOUT_MS)
        );
        assert_eq!(
            shown(Incoming {
                urgency: Some(2),
                ..incoming("c")
            }),
            None
        );
        assert_eq!(
            shown(Incoming {
                expire_timeout: 0,
                ..incoming("d")
            }),
            None
        );
        assert_eq!(
            shown(Incoming {
                expire_timeout: 1500,
                ..incoming("e")
            }),
            Some(1500)
        );
    }

    #[test]
    fn replaces_id_keeps_the_id_and_an_unknown_one_gives_a_new_id() {
        let notifications = Notifications::new();
        let first = notifications.notify(incoming("one"));
        let same = notifications.notify(Incoming {
            replaces_id: first,
            ..incoming("two")
        });
        assert_eq!(same, first);
        assert_eq!(notifications.len(), 1);
        let other = notifications.notify(Incoming {
            replaces_id: 999,
            ..incoming("three")
        });
        assert_ne!(other, first);
    }

    #[test]
    fn actions_come_as_pairs() {
        let notifications = Notifications::new();
        let id = notifications.notify(Incoming {
            actions: vec![
                "default".into(),
                "Open".into(),
                "later".into(),
                "Later".into(),
                "odd".into(),
            ],
            ..incoming("a")
        });
        let actions = notifications.lock().items[&id].0.actions.clone();
        assert_eq!(
            actions.iter().map(|a| a.key.as_str()).collect::<Vec<_>>(),
            ["default", "later"]
        );
    }

    #[test]
    fn the_oldest_goes_at_the_limit() {
        let notifications = Notifications::new();
        let mut signals = notifications.take_signals().unwrap();
        let first = notifications.notify(incoming("first"));
        for i in 0..MAX_NOTIFICATIONS {
            notifications.notify(incoming(&format!("n{i}")));
        }
        assert_eq!(notifications.len(), MAX_NOTIFICATIONS);
        assert_eq!(
            signals.try_recv().unwrap(),
            Signal::Closed {
                id: first,
                reason: reason::UNDEFINED
            }
        );
    }
}
