//! Tests of the echo channel of the agent, through a link that the test controls.

use std::time::Duration;

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{Close, CloseReason, LinkMessage, Open};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

const WAIT: Duration = Duration::from_secs(5);

/// The farglassd side of the link.
struct Link(Framed<UnixStream, FrameCodec>);

impl Link {
    async fn start() -> Self {
        let (ours, theirs) = UnixStream::pair().unwrap();
        tokio::spawn(farglass_session::link::serve(theirs));
        let mut link = Self(Framed::new(ours, FrameCodec));
        let hello = link.recv().await;
        assert_eq!(
            hello.parse::<LinkMessage>().unwrap(),
            LinkMessage::Hello {
                version: PROTOCOL_VERSION
            }
        );
        let ours = LinkMessage::Hello {
            version: PROTOCOL_VERSION,
        };
        link.send(Frame::json(ChannelId::CONNECTION, FrameType::Control, &ours).unwrap())
            .await;
        link
    }

    async fn send(&mut self, frame: Frame) {
        self.0.send(frame).await.unwrap();
    }

    async fn recv(&mut self) -> Frame {
        tokio::time::timeout(WAIT, self.0.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    }

    async fn is_quiet(&mut self) -> bool {
        tokio::time::timeout(Duration::from_millis(100), self.0.next())
            .await
            .is_err()
    }
}

fn ch(number: u32) -> ChannelId {
    ChannelId::new(number)
}

#[tokio::test]
async fn echo_channel_sends_each_frame_back_and_answers_the_close() {
    let mut link = Link::start().await;
    link.send(Frame::open(ch(1), &Open::Echo).unwrap()).await;
    assert_eq!(link.recv().await.frame_type(), FrameType::Ready);

    let payload: Vec<u8> = (0..=255).collect();
    link.send(Frame::data(ch(1), payload.clone()).unwrap())
        .await;
    let echo = link.recv().await;
    assert_eq!(
        (echo.channel(), echo.frame_type()),
        (ch(1), FrameType::Data)
    );
    assert_eq!(echo.payload().as_ref(), payload);

    let control = Frame::new(ch(1), FrameType::Control, &br#"{"ping":1}"#[..]).unwrap();
    link.send(control.clone()).await;
    assert_eq!(link.recv().await, control);

    link.send(Frame::close(ch(1), &Close::new(CloseReason::Done)).unwrap())
        .await;
    let close = link.recv().await;
    assert_eq!(close.parse::<Close>().unwrap().reason, CloseReason::Done);

    // After the two close frames, the channel does not exist.
    link.send(Frame::data(ch(1), &b"late"[..]).unwrap()).await;
    assert!(link.is_quiet().await);
}

#[tokio::test]
async fn channels_are_independent() {
    let mut link = Link::start().await;
    for number in [1, 3] {
        link.send(Frame::open(ch(number), &Open::Echo).unwrap())
            .await;
        assert_eq!(link.recv().await.frame_type(), FrameType::Ready);
    }
    link.send(Frame::data(ch(3), &b"three"[..]).unwrap()).await;
    link.send(Frame::data(ch(1), &b"one"[..]).unwrap()).await;
    let first = link.recv().await;
    let second = link.recv().await;
    assert_eq!(
        (first.channel(), first.payload().as_ref()),
        (ch(3), &b"three"[..])
    );
    assert_eq!(
        (second.channel(), second.payload().as_ref()),
        (ch(1), &b"one"[..])
    );
}

#[tokio::test]
async fn unknown_channel_kind_is_refused() {
    let mut link = Link::start().await;
    link.send(Frame::new(ch(1), FrameType::Open, &br#"{"kind":"telepathy"}"#[..]).unwrap())
        .await;
    let close = link.recv().await;
    assert_eq!(
        close.parse::<Close>().unwrap().reason,
        CloseReason::NotSupported
    );
    // Data that the opener sent before the refusal gets no echo.
    link.send(Frame::data(ch(1), &b"early"[..]).unwrap()).await;
    assert!(link.is_quiet().await);
    // The close frame of farglassd answers the refusal. The agent sends nothing.
    link.send(Frame::close(ch(1), &Close::new(CloseReason::Done)).unwrap())
        .await;
    assert!(link.is_quiet().await);
}

mod state {
    use farglass_proto::message::{StateEvent, StateRequest};
    use serde_json::json;

    use super::*;

    fn request(number: u32, request: &StateRequest) -> Frame {
        Frame::json(ch(number), FrameType::Control, request).unwrap()
    }

    async fn value(link: &mut Link) -> (ChannelId, StateEvent) {
        let frame = link.recv().await;
        (frame.channel(), frame.parse().unwrap())
    }

    #[tokio::test]
    async fn value_survives_the_channel_and_changes_reach_other_channels() {
        let mut link = Link::start().await;
        for number in [1, 3] {
            link.send(Frame::open(ch(number), &Open::State).unwrap())
                .await;
            assert_eq!(link.recv().await.frame_type(), FrameType::Ready);
        }
        link.send(request(
            1,
            &StateRequest::Get {
                key: "windows".to_owned(),
            },
        ))
        .await;
        assert_eq!(
            value(&mut link).await,
            (
                ch(1),
                StateEvent::Value {
                    key: "windows".to_owned(),
                    value: json!(null)
                }
            )
        );

        // Channel 3 sets the key. Channel 1 asked for it and gets the change. Channel 3 gets no echo.
        let windows = json!([{"app": "terminal", "x": 10}]);
        link.send(request(
            3,
            &StateRequest::Set {
                key: "windows".to_owned(),
                value: windows.clone(),
            },
        ))
        .await;
        assert_eq!(
            value(&mut link).await,
            (
                ch(1),
                StateEvent::Value {
                    key: "windows".to_owned(),
                    value: windows.clone()
                }
            )
        );
        assert!(link.is_quiet().await);

        // A new channel, as after a reload, gets the value.
        link.send(Frame::close(ch(1), &Close::new(CloseReason::Done)).unwrap())
            .await;
        link.recv().await;
        link.send(Frame::open(ch(5), &Open::State).unwrap()).await;
        link.recv().await;
        link.send(request(
            5,
            &StateRequest::Get {
                key: "windows".to_owned(),
            },
        ))
        .await;
        assert_eq!(
            value(&mut link).await,
            (
                ch(5),
                StateEvent::Value {
                    key: "windows".to_owned(),
                    value: windows
                }
            )
        );
    }

    #[tokio::test]
    async fn store_continues_when_the_link_to_farglassd_breaks() {
        let mut session = farglass_session::link::Session::default();
        session.store.set("theme", json!("dark")).unwrap();
        let (ours, theirs) = UnixStream::pair().unwrap();
        drop(ours);
        // The link stops at once. The store stays with the caller.
        let _ = farglass_session::link::serve_with(theirs, &mut session).await;
        assert_eq!(session.store.get("theme"), json!("dark"));
    }

    #[tokio::test]
    async fn request_that_is_not_valid_closes_the_channel() {
        let mut link = Link::start().await;
        link.send(Frame::open(ch(1), &Open::State).unwrap()).await;
        link.recv().await;
        link.send(Frame::new(ch(1), FrameType::Control, &br#"{"type":"delete"}"#[..]).unwrap())
            .await;
        let close = link.recv().await.parse::<Close>().unwrap();
        assert_eq!(close.reason, CloseReason::ProtocolError);
    }
}

/// A newer farglassd can send a link message that the agent does not know. The link continues.
#[tokio::test]
async fn an_unknown_link_message_does_not_stop_the_link() {
    let (ours, theirs) = tokio::net::UnixStream::pair().unwrap();
    tokio::spawn(async move {
        let _ = farglass_session::link::serve(theirs).await;
    });
    let mut link = tokio_util::codec::Framed::new(ours, farglass_proto::codec::FrameCodec);
    use futures_util::{SinkExt as _, StreamExt as _};
    link.next().await.unwrap().unwrap();
    let hello = farglass_proto::message::LinkMessage::Hello {
        version: farglass_proto::PROTOCOL_VERSION,
    };
    let connection = farglass_proto::ChannelId::CONNECTION;
    let control = farglass_proto::FrameType::Control;
    link.send(farglass_proto::Frame::json(connection, control, &hello).unwrap())
        .await
        .unwrap();
    let unknown = serde_json::json!({"type": "from-the-future", "value": 1});
    link.send(farglass_proto::Frame::json(connection, control, &unknown).unwrap())
        .await
        .unwrap();
    let channel = farglass_proto::ChannelId::new(1);
    link.send(farglass_proto::Frame::open(channel, &farglass_proto::message::Open::Echo).unwrap())
        .await
        .unwrap();
    let ready = tokio::time::timeout(std::time::Duration::from_secs(5), link.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(ready.frame_type(), farglass_proto::FrameType::Ready);
}
