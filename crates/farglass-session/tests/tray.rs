//! Tests of the system tray: the tray channel of the link and the D-Bus watcher.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{LinkMessage, Open, TrayEvent, TrayItem, TrayRequest, TrayStatus};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglass_session::link::Session;
use farglass_session::tray::{self, Properties, Tray};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_util::codec::Framed;
use zbus::OwnedObjectPath;
use zbus::object_server::SignalEmitter;

const WAIT: Duration = Duration::from_secs(10);

async fn next_event(link: &mut Framed<UnixStream, FrameCodec>) -> TrayEvent {
    loop {
        let frame = tokio::time::timeout(WAIT, link.next())
            .await
            .expect("no frame")
            .unwrap()
            .unwrap();
        if let Ok(event) = frame.parse::<TrayEvent>() {
            return event;
        }
    }
}

#[tokio::test]
async fn the_shell_gets_the_items_and_its_click_goes_to_the_queue() {
    let tray = Tray::new();
    let mut requests = tray.take_requests().unwrap();
    // The item registers before a shell connects.
    let id = tray.register(":1.5", "/StatusNotifierItem").unwrap();
    let properties = Properties {
        id: "bitwarden".to_owned(),
        title: "Bitwarden".to_owned(),
        status: "Passive".to_owned(),
        ..Properties::default()
    };
    tray.update(id, tray::item_from(id, &properties, &[]));

    let agent = tray.clone();
    let (ours, theirs) = UnixStream::pair().unwrap();
    tokio::spawn(async move {
        let mut session = Session {
            tray: agent,
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
    link.send(Frame::open(ChannelId::new(1), &Open::Tray).unwrap())
        .await
        .unwrap();
    assert_eq!(
        link.next().await.unwrap().unwrap().frame_type(),
        FrameType::Ready
    );

    let TrayEvent::Items { items } = next_event(&mut link).await else {
        panic!("the items did not come first");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Bitwarden");
    assert_eq!(items[0].status, TrayStatus::Passive);

    let request = TrayRequest::Activate { id, x: 900, y: 12 };
    link.send(Frame::json(ChannelId::new(1), FrameType::Control, &request).unwrap())
        .await
        .unwrap();
    assert_eq!(
        tokio::time::timeout(WAIT, requests.recv()).await.unwrap(),
        Some(request)
    );

    assert_eq!(tray.remove_owner(":1.5"), [":1.5/StatusNotifierItem"]);
    assert_eq!(next_event(&mut link).await, TrayEvent::Removed { id });
}

/// What the fake program got from the agent.
#[derive(Debug, PartialEq, Eq)]
enum Call {
    Activate(i32, i32),
    SecondaryActivate(i32, i32),
    Scroll(i32, String),
}

/// A status notifier item as a program exports it.
struct FakeItem {
    id: &'static str,
    title: &'static str,
    pixel: Arc<Mutex<[u8; 4]>>,
    calls: mpsc::UnboundedSender<Call>,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl FakeItem {
    #[zbus(property)]
    fn id(&self) -> String {
        self.id.to_owned()
    }

    #[zbus(property)]
    fn title(&self) -> String {
        self.title.to_owned()
    }

    #[zbus(property)]
    fn status(&self) -> String {
        "Active".to_owned()
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        vec![(1, 1, self.pixel.lock().unwrap().to_vec())]
    }

    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/MenuBar").unwrap()
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    fn activate(&self, x: i32, y: i32) {
        let _ = self.calls.send(Call::Activate(x, y));
    }

    fn secondary_activate(&self, x: i32, y: i32) {
        let _ = self.calls.send(Call::SecondaryActivate(x, y));
    }

    fn scroll(&self, delta: i32, orientation: String) {
        let _ = self.calls.send(Call::Scroll(delta, orientation));
    }

    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

async fn item_event(events: &mut mpsc::UnboundedReceiver<TrayEvent>) -> TrayItem {
    match tokio::time::timeout(WAIT, events.recv()).await {
        Ok(Some(TrayEvent::Item { item })) => item,
        other => panic!("no item: {other:?}"),
    }
}

async fn register(program: &zbus::Connection, watcher: &str, service: &str) {
    program
        .call_method(
            Some(watcher),
            tray::WATCHER_PATH,
            Some("org.kde.StatusNotifierWatcher"),
            "RegisterStatusNotifierItem",
            &(service,),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn the_watcher_takes_items_by_name_and_by_path_and_sends_the_clicks() {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        eprintln!("SKIP: no session bus");
        return;
    }
    let watcher = format!(
        "org.farglass.test.StatusNotifierWatcher{}",
        std::process::id()
    );
    let tray = Tray::new();
    let server = zbus::Connection::session().await.unwrap();
    tray::serve(tray.clone(), &server, &watcher).await.unwrap();
    let (send, mut events) = mpsc::unbounded_channel();
    let _viewer = tray.add_viewer(move |event| {
        let _ = send.send(event);
    });
    assert_eq!(
        events.recv().await,
        Some(TrayEvent::Items { items: Vec::new() })
    );

    // A program that registers with a bus name, as Qt does: the item is at /StatusNotifierItem.
    let (calls, mut called) = mpsc::unbounded_channel();
    let pixel = Arc::new(Mutex::new([255, 255, 0, 0]));
    let named = zbus::Connection::session().await.unwrap();
    named
        .object_server()
        .at(
            "/StatusNotifierItem",
            FakeItem {
                id: "seafile",
                title: "Seafile",
                pixel: pixel.clone(),
                calls: calls.clone(),
            },
        )
        .await
        .unwrap();
    let name = format!("org.farglass.test.Item{}", std::process::id());
    named.request_name(name.as_str()).await.unwrap();
    register(&named, &watcher, &name).await;
    let seafile = item_event(&mut events).await;
    assert_eq!(
        (seafile.name.as_str(), seafile.title.as_str()),
        ("seafile", "Seafile")
    );
    assert_eq!(seafile.status, TrayStatus::Active);
    assert_eq!(seafile.icon_mime.as_deref(), Some("image/png"));
    assert!(seafile.menu);
    assert!(!seafile.menu_only);
    // The same item again is not a second item.
    register(&named, &watcher, &name).await;

    // A program that registers with an object path, as Electron does.
    let pathed = zbus::Connection::session().await.unwrap();
    pathed
        .object_server()
        .at(
            "/org/ayatana/NotificationItem/bitwarden",
            FakeItem {
                id: "bitwarden",
                title: "Bitwarden",
                pixel: Arc::new(Mutex::new([255, 0, 0, 255])),
                calls,
            },
        )
        .await
        .unwrap();
    register(&pathed, &watcher, "/org/ayatana/NotificationItem/bitwarden").await;
    let bitwarden = item_event(&mut events).await;
    assert_eq!(bitwarden.title, "Bitwarden");
    assert_ne!(bitwarden.id, seafile.id);
    assert_ne!(bitwarden.icon, seafile.icon);

    let listed: zbus::OwnedVariant = named
        .call_method(
            Some(watcher.as_str()),
            tray::WATCHER_PATH,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(
                "org.kde.StatusNotifierWatcher",
                "RegisteredStatusNotifierItems",
            ),
        )
        .await
        .unwrap()
        .body()
        .deserialize()
        .unwrap();
    let listed = Vec::<String>::try_from(listed).unwrap();
    assert_eq!(
        listed,
        [
            format!("{}/StatusNotifierItem", named.unique_name().unwrap()),
            format!(
                "{}/org/ayatana/NotificationItem/bitwarden",
                pathed.unique_name().unwrap()
            ),
        ]
    );

    // A new icon: the program sends NewIcon, and the agent reads the item again.
    *pixel.lock().unwrap() = [255, 0, 255, 0];
    let emitter = SignalEmitter::new(&named, "/StatusNotifierItem").unwrap();
    FakeItem::new_icon(&emitter).await.unwrap();
    let changed = item_event(&mut events).await;
    assert_eq!(changed.id, seafile.id);
    assert_ne!(changed.icon, seafile.icon);

    // The requests of the shell are method calls on the item.
    tray.request(TrayRequest::Activate {
        id: bitwarden.id,
        x: 900,
        y: 12,
    });
    assert_eq!(called.recv().await, Some(Call::Activate(900, 12)));
    tray.request(TrayRequest::SecondaryActivate {
        id: seafile.id,
        x: 1,
        y: 2,
    });
    assert_eq!(called.recv().await, Some(Call::SecondaryActivate(1, 2)));
    tray.request(TrayRequest::Scroll {
        id: seafile.id,
        delta: -120,
        horizontal: false,
    });
    assert_eq!(
        called.recv().await,
        Some(Call::Scroll(-120, "vertical".to_owned()))
    );

    // A program that leaves the bus takes its item with it.
    pathed.close().await.unwrap();
    match tokio::time::timeout(WAIT, events.recv()).await {
        Ok(Some(TrayEvent::Removed { id })) => assert_eq!(id, bitwarden.id),
        other => panic!("no removal: {other:?}"),
    }
    assert_eq!(tray.items().len(), 1);
}
