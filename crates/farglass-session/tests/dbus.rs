//! Tests of the dbus channel with the real session bus and system bus.
//!
//! A test that needs a bus prints SKIP and passes when the bus is not available.

use std::collections::HashMap;
use std::time::Duration;

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{Bus, Close, CloseReason, DbusEvent, DbusRequest, LinkMessage, Open};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglass_session::dbus::{Buses, DbusOptions};
use farglass_session::link::Session;
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value as Json, json};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

const WAIT: Duration = Duration::from_secs(10);

fn ch(number: u32) -> ChannelId {
    ChannelId::new(number)
}

/// The farglassd side of a link to an agent with the allowlist `options`.
struct Link(Framed<UnixStream, FrameCodec>);

impl Link {
    async fn start(options: DbusOptions) -> Self {
        let (ours, theirs) = UnixStream::pair().unwrap();
        tokio::spawn(async move {
            let mut session = Session {
                buses: Buses::new(options),
                ..Session::default()
            };
            let _ = farglass_session::link::serve_with(theirs, &mut session).await;
        });
        let mut link = Self(Framed::new(ours, FrameCodec));
        link.recv().await;
        let hello = LinkMessage::Hello {
            version: PROTOCOL_VERSION,
        };
        link.0
            .send(Frame::json(ChannelId::CONNECTION, FrameType::Control, &hello).unwrap())
            .await
            .unwrap();
        link
    }

    async fn recv(&mut self) -> Frame {
        tokio::time::timeout(WAIT, self.0.next())
            .await
            .expect("no frame")
            .unwrap()
            .unwrap()
    }

    async fn open(&mut self, number: u32, bus: Bus) {
        self.0
            .send(Frame::open(ch(number), &Open::Dbus { bus }).unwrap())
            .await
            .unwrap();
        assert_eq!(self.recv().await.frame_type(), FrameType::Ready);
    }

    async fn request(&mut self, number: u32, request: &DbusRequest) {
        self.0
            .send(Frame::json(ch(number), FrameType::Control, request).unwrap())
            .await
            .unwrap();
    }

    /// Returns the next event on the channel `number`.
    async fn event(&mut self, number: u32) -> DbusEvent {
        loop {
            let frame = self.recv().await;
            if frame.channel() == ch(number) && frame.frame_type() == FrameType::Control {
                return frame.parse().unwrap();
            }
        }
    }

    async fn call(&mut self, number: u32, id: u32, call: [&str; 5], args: Vec<Json>) -> DbusEvent {
        let [destination, path, interface, method, signature] = call;
        let request = DbusRequest::Call {
            id,
            destination: destination.to_owned(),
            path: path.to_owned(),
            interface: interface.to_owned(),
            method: method.to_owned(),
            signature: signature.to_owned(),
            args,
            interactive: false,
        };
        self.request(number, &request).await;
        self.event(number).await
    }

    /// Reports whether no frame comes on the link for a short time.
    async fn is_quiet(&mut self) -> bool {
        tokio::time::timeout(Duration::from_millis(300), self.0.next())
            .await
            .is_err()
    }
}

fn has_session_bus() -> bool {
    std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
}

const DBUS: [&str; 3] = [
    "org.freedesktop.DBus",
    "/org/freedesktop/DBus",
    "org.freedesktop.DBus",
];

#[tokio::test]
async fn call_gives_the_reply_in_the_json_form() {
    if !has_session_bus() {
        eprintln!("SKIP: no session bus");
        return;
    }
    let mut link = Link::start(DbusOptions::default()).await;
    link.open(1, Bus::Session).await;
    let [d, p, i] = DBUS;
    let reply = link
        .call(
            1,
            7,
            [d, p, i, "NameHasOwner", "s"],
            vec![json!("org.freedesktop.DBus")],
        )
        .await;
    assert_eq!(
        reply,
        DbusEvent::Reply {
            id: 7,
            values: vec![json!(true)]
        }
    );
}

#[tokio::test]
async fn error_of_the_service_and_the_allowlist_come_back_by_name() {
    if !has_session_bus() {
        eprintln!("SKIP: no session bus");
        return;
    }
    let mut link = Link::start(DbusOptions::default()).await;
    link.open(1, Bus::Session).await;
    let [d, p, i] = DBUS;
    let missing = link
        .call(
            1,
            1,
            [d, p, i, "GetNameOwner", "s"],
            vec![json!("org.farglass.NoSuchName")],
        )
        .await;
    let DbusEvent::Error { id, name, .. } = missing else {
        panic!("{missing:?}");
    };
    assert_eq!(
        (id, name.as_str()),
        (1, "org.freedesktop.DBus.Error.NameHasNoOwner")
    );

    let refused = link
        .call(
            1,
            2,
            [
                "org.freedesktop.systemd1",
                "/org/freedesktop/systemd1",
                "org.freedesktop.systemd1.Manager",
                "PowerOff",
                "",
            ],
            vec![],
        )
        .await;
    let DbusEvent::Error { name, .. } = refused else {
        panic!("{refused:?}")
    };
    assert_eq!(name, "org.freedesktop.DBus.Error.AccessDenied");

    let wrong = link
        .call(1, 3, [d, p, i, "NameHasOwner", "s"], vec![json!(5)])
        .await;
    let DbusEvent::Error { name, .. } = wrong else {
        panic!("{wrong:?}")
    };
    assert_eq!(name, "org.freedesktop.DBus.Error.InvalidArgs");
}

#[tokio::test]
async fn property_of_a_system_service_comes_as_a_variant() {
    let mut link = Link::start(DbusOptions::default()).await;
    link.open(1, Bus::System).await;
    let reply = link
        .call(
            1,
            1,
            [
                "org.freedesktop.hostname1",
                "/org/freedesktop/hostname1",
                "org.freedesktop.DBus.Properties",
                "Get",
                "ss",
            ],
            vec![json!("org.freedesktop.hostname1"), json!("Hostname")],
        )
        .await;
    match reply {
        DbusEvent::Reply { values, .. } => {
            let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap();
            assert_eq!(values, vec![json!({"type": "s", "value": hostname.trim()})]);
        }
        DbusEvent::Error { name, .. } if name == "org.freedesktop.DBus.Error.NoServer" => {
            eprintln!("SKIP: no system bus");
        }
        other => panic!("{other:?}"),
    }
}

/// A test service that echoes a dictionary of variants.
struct Echo;

#[zbus::interface(name = "org.farglass.Test")]
impl Echo {
    fn echo(
        &self,
        values: HashMap<String, zbus::OwnedVariant>,
        count: u32,
    ) -> (HashMap<String, zbus::OwnedVariant>, u32) {
        (values, count + 1)
    }
}

/// Starts the test service on the session bus with a unique name. Returns its connection and name.
async fn test_service() -> (zbus::Connection, String) {
    let name = format!("org.farglass.Test.T{}", std::process::id() ^ rand_suffix());
    let connection = zbus::connection::Builder::session()
        .name(name.as_str())
        .serve_at("/org/farglass/Test", Echo)
        .build()
        .await
        .unwrap();
    (connection, name)
}

fn rand_suffix() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos()
}

fn test_options() -> DbusOptions {
    DbusOptions {
        session: vec![
            "org.freedesktop.DBus".to_owned(),
            "org.farglass.Test.*".to_owned(),
        ],
        ..DbusOptions::default()
    }
}

#[tokio::test]
async fn dictionary_of_variants_goes_to_a_service_and_back() {
    if !has_session_bus() {
        eprintln!("SKIP: no session bus");
        return;
    }
    let (_service, name) = test_service().await;
    let mut link = Link::start(test_options()).await;
    link.open(1, Bus::Session).await;
    let values = json!({
        "name": {"type": "s", "value": "farglass"},
        "list": {"type": "ai", "value": [1, -2, 3]},
        "nested": {"type": "v", "value": {"type": "b", "value": true}},
    });
    let reply = link
        .call(
            1,
            9,
            [
                &name,
                "/org/farglass/Test",
                "org.farglass.Test",
                "Echo",
                "a{sv}u",
            ],
            vec![values.clone(), json!(41)],
        )
        .await;
    assert_eq!(
        reply,
        DbusEvent::Reply {
            id: 9,
            values: vec![values, json!(42)]
        }
    );
}

#[tokio::test]
async fn signals_come_until_the_unsubscribe_and_the_end_of_the_channel() {
    if !has_session_bus() {
        eprintln!("SKIP: no session bus");
        return;
    }
    let (service, name) = test_service().await;
    let emit = |text: &'static str| {
        let service = service.clone();
        async move {
            service
                .emit_signal(
                    None::<()>,
                    "/org/farglass/Test",
                    "org.farglass.Test",
                    "Ping",
                    &(text, 7_u32),
                )
                .await
                .unwrap();
        }
    };
    let mut link = Link::start(test_options()).await;
    link.open(1, Bus::Session).await;
    let subscribe = DbusRequest::Subscribe {
        id: 5,
        sender: name.clone(),
        path: Some("/org/farglass/Test".to_owned()),
        interface: Some("org.farglass.Test".to_owned()),
        member: Some("Ping".to_owned()),
    };
    link.request(1, &subscribe).await;
    assert_eq!(
        link.event(1).await,
        DbusEvent::Reply {
            id: 5,
            values: vec![]
        }
    );

    emit("first").await;
    assert_eq!(
        link.event(1).await,
        DbusEvent::Signal {
            subscription: 5,
            path: "/org/farglass/Test".to_owned(),
            interface: "org.farglass.Test".to_owned(),
            member: "Ping".to_owned(),
            values: vec![json!("first"), json!(7)],
        }
    );

    link.request(1, &DbusRequest::Unsubscribe { id: 5 }).await;
    assert_eq!(
        link.event(1).await,
        DbusEvent::Reply {
            id: 5,
            values: vec![]
        }
    );
    emit("after unsubscribe").await;
    assert!(link.is_quiet().await, "a signal came after the unsubscribe");

    // A new subscription ends with its channel.
    link.request(1, &subscribe).await;
    link.event(1).await;
    link.0
        .send(Frame::close(ch(1), &Close::new(CloseReason::Done)).unwrap())
        .await
        .unwrap();
    assert_eq!(link.recv().await.frame_type(), FrameType::Close);
    emit("after close").await;
    assert!(
        link.is_quiet().await,
        "a signal came after the end of the channel"
    );
}

#[tokio::test]
async fn subscription_to_a_sender_outside_the_allowlist_is_refused() {
    let mut link = Link::start(DbusOptions::default()).await;
    link.open(1, Bus::Session).await;
    let subscribe = DbusRequest::Subscribe {
        id: 1,
        sender: "org.freedesktop.systemd1".to_owned(),
        path: None,
        interface: None,
        member: None,
    };
    link.request(1, &subscribe).await;
    let DbusEvent::Error { name, .. } = link.event(1).await else {
        panic!()
    };
    assert_eq!(name, "org.freedesktop.DBus.Error.AccessDenied");
}
