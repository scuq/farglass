//! Tests of the stats channel: the use of the host by a terminal window (F045).

use std::time::Duration;

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{LinkMessage, Open, StatsEvent};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglass_session::link::Session;
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

#[tokio::test(flavor = "multi_thread")]
async fn a_busy_terminal_shows_its_cpu_and_the_agent_shows_its_own() {
    let (ours, theirs) = UnixStream::pair().unwrap();
    tokio::spawn(async move {
        let mut session = Session::default();
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
    let pty = ChannelId::new(1);
    let stats = ChannelId::new(3);
    link.send(
        Frame::open(
            pty,
            &Open::Pty {
                terminal: "window-t1".into(),
                cols: 80,
                rows: 24,
            },
        )
        .unwrap(),
    )
    .await
    .unwrap();
    // A busy loop in the terminal for a few seconds.
    link.send(Frame::data(pty, &b"timeout 8 sh -c 'while :; do :; done'\n"[..]).unwrap())
        .await
        .unwrap();
    link.send(Frame::open(stats, &Open::Stats).unwrap())
        .await
        .unwrap();
    let found = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let frame = link.next().await.unwrap().unwrap();
            if frame.channel() != stats || frame.frame_type() != FrameType::Control {
                continue;
            }
            let StatsEvent::Sample {
                windows,
                agent,
                host,
                ..
            } = frame.parse().unwrap();
            assert!(agent.memory > 1_000_000, "the agent has its memory");
            // The first sample has no interval for the host yet.
            if let Some(host) = &host {
                assert!(host.memory_total > host.memory && host.memory > 0);
                assert!((0.0..=100.0).contains(&host.cpu));
            }
            if let Some(window) = windows.iter().find(|w| w.window == "window-t1")
                && window.usage.cpu > 50.0
                && host.is_some()
            {
                return window.usage.clone();
            }
        }
    })
    .await
    .expect("no busy sample of the terminal");
    assert!(found.memory > 0);
}
