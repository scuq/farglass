//! Tests of the desktop notifications: the notify channel of the link and the D-Bus service.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::Duration;

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{LinkMessage, NotifyEvent, NotifyReply, Open, Urgency};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglass_session::link::Session;
use farglass_session::notify::{self, Incoming, Notifications, Signal};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

const WAIT: Duration = Duration::from_secs(10);

async fn next_event(link: &mut Framed<UnixStream, FrameCodec>) -> NotifyEvent {
    loop {
        let frame = tokio::time::timeout(WAIT, link.next())
            .await
            .expect("no frame")
            .unwrap()
            .unwrap();
        if let Ok(event) = frame.parse::<NotifyEvent>() {
            return event;
        }
    }
}

#[tokio::test]
async fn a_notification_waits_for_the_shell_and_the_action_goes_back() {
    let notifications = Notifications::new();
    let mut signals = notifications.take_signals().unwrap();
    // The notification comes before a shell connects.
    let id = notifications.notify(Incoming {
        app: "mail".to_owned(),
        summary: "New mail".to_owned(),
        body: "From Alice".to_owned(),
        actions: vec!["default".to_owned(), "Open".to_owned()],
        expire_timeout: -1,
        ..Incoming::default()
    });

    let agent = notifications.clone();
    let (ours, theirs) = UnixStream::pair().unwrap();
    tokio::spawn(async move {
        let mut session = Session {
            notifications: agent,
            ..Session::default()
        };
        let _ = farglass_session::link::serve_with(theirs, &mut session).await;
    });
    let mut link = Framed::new(ours, FrameCodec);
    link.next().await.unwrap().unwrap();
    let hello = LinkMessage::Hello {
        version: PROTOCOL_VERSION,
    };
    link.send(Frame::json(ChannelId::CONNECTION, FrameType::Control, &hello).unwrap())
        .await
        .unwrap();
    link.send(Frame::open(ChannelId::new(1), &Open::Notify).unwrap())
        .await
        .unwrap();
    assert_eq!(
        link.next().await.unwrap().unwrap().frame_type(),
        FrameType::Ready
    );

    let NotifyEvent::Show { notification } = next_event(&mut link).await else {
        panic!("the open notification did not come first");
    };
    assert_eq!(notification.id, id);
    assert_eq!(notification.summary, "New mail");
    assert_eq!(notification.urgency, Urgency::Normal);
    assert_eq!(notification.actions[0].label, "Open");

    let reply = NotifyReply::Action {
        id,
        key: "default".to_owned(),
    };
    link.send(Frame::json(ChannelId::new(1), FrameType::Control, &reply).unwrap())
        .await
        .unwrap();
    assert_eq!(next_event(&mut link).await, NotifyEvent::Close { id });
    assert_eq!(
        signals.recv().await.unwrap(),
        Signal::Action {
            id,
            key: "default".to_owned()
        }
    );
    assert_eq!(
        signals.recv().await.unwrap(),
        Signal::Closed {
            id,
            reason: notify::reason::DISMISSED
        }
    );
    assert!(notifications.is_empty());
}

#[tokio::test]
async fn the_dbus_service_takes_notify_and_sends_the_signals() {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        eprintln!("SKIP: no session bus");
        return;
    }
    let name = format!("org.farglass.test.Notifications{}", std::process::id());
    let notifications = Notifications::new();
    let server = zbus::Connection::session().await.unwrap();
    notify::serve(notifications.clone(), &server, &name)
        .await
        .unwrap();

    let program = zbus::Connection::session().await.unwrap();
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.Notifications")
        .path(notify::NOTIFY_PATH)
        .build()
        .unwrap();
    let mut stream = zbus::MessageStream::for_match_rule(rule, &program, NonZeroUsize::new(16))
        .await
        .unwrap();

    let info = program
        .call_method(
            Some(name.as_str()),
            notify::NOTIFY_PATH,
            Some("org.freedesktop.Notifications"),
            "GetServerInformation",
            &(),
        )
        .await
        .unwrap();
    let (server_name, _, _, spec): (String, String, String, String) =
        info.body().deserialize().unwrap();
    assert_eq!((server_name.as_str(), spec.as_str()), ("farglass", "1.2"));

    let hints: HashMap<&str, zbus::Variant<'_>> =
        HashMap::from([("urgency", zbus::Variant::from(2_u8))]);
    let reply = program
        .call_method(
            Some(name.as_str()),
            notify::NOTIFY_PATH,
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "backup",
                0_u32,
                "",
                "Backup failed",
                "The disk is full.",
                vec!["retry", "Retry"],
                hints,
                -1_i32,
            ),
        )
        .await
        .unwrap();
    let id: u32 = reply.body().deserialize().unwrap();

    let (send, mut shown) = tokio::sync::mpsc::unbounded_channel();
    let _viewer = notifications.add_viewer(move |event| {
        let _ = send.send(event);
    });
    let Some(NotifyEvent::Show { notification }) = shown.recv().await else {
        panic!("no notification");
    };
    assert_eq!(notification.urgency, Urgency::Critical);
    assert_eq!(notification.timeout_ms, None);

    notifications.reply(NotifyReply::Action {
        id,
        key: "retry".to_owned(),
    });
    let mut seen = Vec::new();
    while seen.len() < 2 {
        let message = tokio::time::timeout(WAIT, stream.next())
            .await
            .expect("no signal")
            .unwrap()
            .unwrap();
        let header = message.header();
        match header.member().map(|member| member.as_str()) {
            Some("ActionInvoked") => {
                let (signal_id, key): (u32, String) = message.body().deserialize().unwrap();
                seen.push(format!("action {signal_id} {key}"));
            }
            Some("NotificationClosed") => {
                let (signal_id, reason): (u32, u32) = message.body().deserialize().unwrap();
                seen.push(format!("closed {signal_id} {reason}"));
            }
            _ => {}
        }
    }
    assert_eq!(
        seen,
        [format!("action {id} retry"), format!("closed {id} 2")]
    );
}
