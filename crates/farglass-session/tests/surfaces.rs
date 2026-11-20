//! Tests of the surfaces channel: the windows of the Wayland compositor of the session.

use std::time::Duration;

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{LinkMessage, Open, SurfaceEvent, SurfaceRequest};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglass_session::link::Session;
use farglass_session::surfaces::Surfaces;
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

const WAIT: Duration = Duration::from_secs(20);

/// Starts a link with `surfaces` and opens a surfaces channel on it.
async fn link(surfaces: std::sync::Arc<Surfaces>) -> Framed<UnixStream, FrameCodec> {
    let (ours, theirs) = UnixStream::pair().unwrap();
    tokio::spawn(async move {
        let mut session = Session {
            surfaces,
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
    link.send(Frame::open(ChannelId::new(1), &Open::Surfaces).unwrap())
        .await
        .unwrap();
    assert_eq!(
        link.next().await.unwrap().unwrap().frame_type(),
        FrameType::Ready
    );
    link
}

async fn next_event(
    link: &mut Framed<UnixStream, FrameCodec>,
    wanted: impl Fn(&SurfaceEvent) -> bool,
) -> SurfaceEvent {
    tokio::time::timeout(WAIT, async {
        loop {
            let frame = link.next().await.unwrap().unwrap();
            if let Ok(event) = frame.parse::<SurfaceEvent>()
                && wanted(&event)
            {
                return event;
            }
        }
    })
    .await
    .expect("no matching surface event")
}

#[tokio::test]
async fn the_channel_gets_the_windows_and_its_requests_go_to_the_compositor() {
    let surfaces = Surfaces::new();
    let commands = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = commands.clone();
    surfaces.set_commands(move |command| sink.lock().unwrap().push(command));
    surfaces.handle(farglass_surface::Event::ToplevelNew { id: 1, pid: None });
    let mut link = link(surfaces.clone()).await;
    assert_eq!(
        next_event(&mut link, |_| true).await,
        SurfaceEvent::Toplevel {
            id: 1,
            title: None,
            app_id: None,
            parent: None
        }
    );
    assert_eq!(next_event(&mut link, |_| true).await, SurfaceEvent::Synced);
    surfaces.handle(farglass_surface::Event::ToplevelClosed { id: 1 });
    assert_eq!(
        next_event(&mut link, |_| true).await,
        SurfaceEvent::Closed { id: 1 }
    );

    let request = SurfaceRequest::Configure {
        id: 1,
        width: 640,
        height: 480,
    };
    link.send(Frame::json(ChannelId::new(1), FrameType::Control, &request).unwrap())
        .await
        .unwrap();
    tokio::time::timeout(WAIT, async {
        while commands.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        *commands.lock().unwrap(),
        [farglass_surface::Command::Configure {
            id: 1,
            width: 640,
            height: 480
        }]
    );
}

/// A real GTK 4 app on the compositor: its window, its commits, and the close from the shell.
#[tokio::test]
async fn a_gtk_app_shows_its_window_and_closes_on_request() {
    let app = std::path::Path::new("/usr/bin/gnome-calculator");
    if !app.exists() {
        eprintln!("SKIP: no gnome-calculator");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("wayland-test");
    let surfaces = Surfaces::new();
    let _compositor = surfaces.start(&socket).unwrap();
    let mut link = link(surfaces).await;
    let mut child = tokio::process::Command::new(app)
        .env("WAYLAND_DISPLAY", &socket)
        .env("GDK_BACKEND", "wayland")
        // A separate instance: with no session bus, GApplication does not give the work to a running calculator.
        .env("DBUS_SESSION_BUS_ADDRESS", "disabled:")
        .env_remove("DISPLAY")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    // With no session bus, GTK takes the program name as the app id.
    let SurfaceEvent::Toplevel { id, .. } =
        next_event(&mut link, |e| matches!(e, SurfaceEvent::Toplevel { app_id: Some(app), .. } if app.to_ascii_lowercase().contains("calculator"))).await
    else {
        unreachable!()
    };
    let SurfaceEvent::Commit {
        width,
        height,
        damage,
        ..
    } = next_event(
        &mut link,
        |e| matches!(e, SurfaceEvent::Commit { id: c, .. } if *c == id),
    )
    .await
    else {
        unreachable!()
    };
    assert!(width > 100 && height > 100, "a window of {width}x{height}");
    assert!(!damage.is_empty());

    let close = SurfaceRequest::Close { id };
    link.send(Frame::json(ChannelId::new(1), FrameType::Control, &close).unwrap())
        .await
        .unwrap();
    assert_eq!(
        next_event(&mut link, |e| matches!(e, SurfaceEvent::Closed { .. })).await,
        SurfaceEvent::Closed { id }
    );
    let status = tokio::time::timeout(WAIT, child.wait())
        .await
        .expect("the app did not stop")
        .unwrap();
    assert!(status.success(), "the app ended with {status}");
}

/// A commit of a surface of 64 x 64 pixels with one color.
fn commit(id: u32, shade: u8) -> farglass_surface::Event {
    farglass_surface::Event::Commit(farglass_surface::Commit {
        id,
        width: 64,
        height: 64,
        scale: 1,
        format: farglass_surface::Format::Argb8888,
        damage: vec![farglass_surface::Rect {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        }],
        geometry: None,
        pixels: std::sync::Arc::new([shade, shade, shade, 255].repeat(64 * 64)),
    })
}

/// Returns the sequences of the end messages in the data frames that come in `time`.
async fn frames_in(link: &mut Framed<UnixStream, FrameCodec>, time: Duration) -> Vec<(u32, u8)> {
    let mut ends = Vec::new();
    let _ = tokio::time::timeout(time, async {
        loop {
            let frame = link.next().await.unwrap().unwrap();
            if frame.frame_type() != FrameType::Data {
                continue;
            }
            let bytes = frame.payload();
            // A frame of one fill: frame (9 bytes), fill (11 bytes), end (5 bytes). The fill has the shade.
            if bytes.len() == 25 && bytes[0] == 1 && bytes[20] == 8 {
                ends.push((
                    u32::from_le_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]),
                    bytes[16],
                ));
            }
        }
    })
    .await;
    ends
}

#[tokio::test]
async fn a_client_that_does_not_acknowledge_gets_two_frames_and_then_the_newest_only() {
    let surfaces = Surfaces::new();
    surfaces.handle(farglass_surface::Event::ToplevelNew { id: 1, pid: None });
    let mut link = link(surfaces.clone()).await;
    link.send(
        Frame::open(
            ChannelId::new(3),
            &Open::Surface {
                id: 1,
                video: false,
            },
        )
        .unwrap(),
    )
    .await
    .unwrap();
    for shade in 1..=6 {
        surfaces.handle(commit(1, shade));
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let first = frames_in(&mut link, Duration::from_millis(300)).await;
    assert_eq!(first.len(), 2, "2 frames with no acknowledgment: {first:?}");
    let ack = farglass_proto::message::SurfaceAck::Ack { seq: first[1].0 };
    link.send(Frame::json(ChannelId::new(3), FrameType::Control, &ack).unwrap())
        .await
        .unwrap();
    let after = frames_in(&mut link, Duration::from_millis(300)).await;
    assert_eq!(
        after.len(),
        1,
        "the waiting commits went as one frame: {after:?}"
    );
    assert_eq!(after[0].1, 6, "the frame has the newest pixels");
}

/// The clipboard channel: a content of the shell in two data frames goes to the compositor,
/// and a copy in an app goes to the shell as an offer with its data (F030).
#[tokio::test]
async fn the_clipboard_goes_both_ways_on_its_channel() {
    use farglass_proto::message::{ClipboardMessage, ClipboardMime};
    use farglass_surface::{Clip, Command, Event};

    let surfaces = Surfaces::new();
    let commands = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = commands.clone();
    surfaces.set_commands(move |command| sink.lock().unwrap().push(command));
    let mut link = link(surfaces.clone()).await;
    let clip = ChannelId::new(3);
    link.send(Frame::open(clip, &Open::Clipboard).unwrap())
        .await
        .unwrap();
    let offer = ClipboardMessage::Offer {
        mime: ClipboardMime::Text,
        size: 11,
    };
    link.send(Frame::json(clip, FrameType::Control, &offer).unwrap())
        .await
        .unwrap();
    link.send(Frame::data(clip, &b"hello "[..]).unwrap())
        .await
        .unwrap();
    link.send(Frame::data(clip, &b"world"[..]).unwrap())
        .await
        .unwrap();
    let stored = tokio::time::timeout(WAIT, async {
        loop {
            let frame = link.next().await.unwrap().unwrap();
            if frame.channel() == clip && frame.frame_type() == FrameType::Control {
                return frame.parse::<ClipboardMessage>().unwrap();
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(stored, ClipboardMessage::Stored);
    assert_eq!(
        *commands.lock().unwrap(),
        [Command::SetClipboard(Clip::Text("hello world".into()))]
    );

    let png = vec![0x89; 300 * 1024];
    surfaces.handle(Event::Clipboard(Clip::Png(png.clone())));
    let got = tokio::time::timeout(WAIT, async {
        let mut size = None;
        let mut bytes = Vec::new();
        loop {
            let frame = link.next().await.unwrap().unwrap();
            if frame.channel() != clip {
                continue;
            }
            if frame.frame_type() == FrameType::Control {
                let ClipboardMessage::Offer { mime, size: s } = frame.parse().unwrap() else {
                    panic!("not an offer");
                };
                assert_eq!(mime, ClipboardMime::Png);
                size = Some(s as usize);
            } else {
                bytes.extend_from_slice(frame.payload());
            }
            if size == Some(bytes.len()) {
                return bytes;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(got, png, "a large image comes in more than one data frame");
}
