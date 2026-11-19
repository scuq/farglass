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
