//! Tests of the apps channel: the desktop entries of the host, their icons, and the start of an app (F031).

use std::time::Duration;

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{AppsEvent, AppsRequest, LinkMessage, Open};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglass_session::launch::{LaunchOptions, Launcher};
use farglass_session::link::Session;
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

const WAIT: Duration = Duration::from_secs(20);
const APPS: ChannelId = ChannelId::new(1);

async fn next_frame(link: &mut Framed<UnixStream, FrameCodec>) -> Frame {
    tokio::time::timeout(WAIT, async {
        loop {
            let frame = link.next().await.unwrap().unwrap();
            if frame.channel() == APPS {
                return frame;
            }
        }
    })
    .await
    .expect("no frame on the apps channel")
}

async fn next_event(link: &mut Framed<UnixStream, FrameCodec>) -> AppsEvent {
    next_frame(link).await.parse().unwrap()
}

#[tokio::test]
async fn the_channel_lists_the_apps_gives_an_icon_and_starts_an_app() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("share");
    let marker = dir.path().join("started");
    let applications = data.join("applications");
    std::fs::create_dir_all(&applications).unwrap();
    std::fs::write(
        applications.join("org.example.Marker.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=Marker\nComment=Writes a file\nIcon=marker\nExec=sh -c \"echo %c > {}\" %U\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::write(
        applications.join("hidden.desktop"),
        "[Desktop Entry]\nType=Application\nName=Hidden\nExec=true\nNoDisplay=true\n",
    )
    .unwrap();
    let icon = data.join("icons/hicolor/scalable/apps/marker.svg");
    std::fs::create_dir_all(icon.parent().unwrap()).unwrap();
    std::fs::write(&icon, "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();

    let (ours, theirs) = UnixStream::pair().unwrap();
    tokio::spawn(async move {
        let mut session = Session {
            launcher: Launcher::new(LaunchOptions {
                scope: false,
                wayland: None,
                dirs: vec![data],
                languages: Vec::new(),
            }),
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
    link.send(Frame::open(APPS, &Open::Apps).unwrap())
        .await
        .unwrap();
    assert_eq!(next_frame(&mut link).await.frame_type(), FrameType::Ready);

    let AppsEvent::List { apps } = next_event(&mut link).await else {
        panic!("no list")
    };
    assert_eq!(apps.len(), 1, "an entry with NoDisplay does not show");
    assert_eq!(apps[0].id, "org.example.Marker");
    assert_eq!(apps[0].comment.as_deref(), Some("Writes a file"));
    assert!(apps[0].icon);
    assert_eq!(apps[0].names, ["org.example.marker", "sh"]);

    let send = |request: AppsRequest| Frame::json(APPS, FrameType::Control, &request).unwrap();
    link.send(send(AppsRequest::Icon {
        id: "org.example.Marker".into(),
    }))
    .await
    .unwrap();
    assert_eq!(
        next_event(&mut link).await,
        AppsEvent::Icon {
            id: "org.example.Marker".into(),
            mime: Some("image/svg+xml".into())
        }
    );
    let bytes = next_frame(&mut link).await;
    assert_eq!(bytes.frame_type(), FrameType::Data);
    assert!(bytes.payload().starts_with(b"<svg"));

    link.send(send(AppsRequest::Launch {
        id: "org.example.Marker".into(),
    }))
    .await
    .unwrap();
    assert_eq!(
        next_event(&mut link).await,
        AppsEvent::Launched {
            id: "org.example.Marker".into()
        }
    );
    tokio::time::timeout(WAIT, async {
        while !marker.exists() || std::fs::read_to_string(&marker).unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the app did not run");
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "Marker\n");

    link.send(send(AppsRequest::Launch {
        id: "nothing".into(),
    }))
    .await
    .unwrap();
    assert!(matches!(
        next_event(&mut link).await,
        AppsEvent::Failed { id, .. } if id == "nothing"
    ));
}
