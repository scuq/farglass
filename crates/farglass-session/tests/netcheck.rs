//! Tests of the netcheck channel: the checks files, a save, and a run (F111).

use std::time::Duration;

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{
    LinkMessage, NetcheckCheck, NetcheckEvent, NetcheckKind, NetcheckRequest, NetcheckState, Open,
};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglass_session::link::Session;
use farglass_session::netcheck::{Netcheck, Proxy};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;

const WAIT: Duration = Duration::from_secs(20);
const CHECKS: ChannelId = ChannelId::new(1);

async fn next_event(link: &mut Framed<UnixStream, FrameCodec>) -> NetcheckEvent {
    tokio::time::timeout(WAIT, async {
        loop {
            let frame = link.next().await.unwrap().unwrap();
            if frame.channel() == CHECKS && frame.frame_type() == FrameType::Control {
                return frame.parse().unwrap();
            }
        }
    })
    .await
    .expect("no event on the netcheck channel")
}

async fn open(netcheck: std::sync::Arc<Netcheck>) -> Framed<UnixStream, FrameCodec> {
    let (ours, theirs) = UnixStream::pair().unwrap();
    tokio::spawn(async move {
        let mut session = Session {
            netcheck,
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
    link.send(Frame::open(CHECKS, &Open::Netcheck).unwrap())
        .await
        .unwrap();
    link
}

#[tokio::test]
async fn the_channel_loads_saves_and_runs_the_checks() {
    let dir = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let admin = dir.path().join("admin.toml");
    std::fs::write(
        &admin,
        "[[check]]\nname = \"loopback\"\nkind = \"icmp\"\nhost = \"127.0.0.1\"\n",
    )
    .unwrap();
    let mut link = open(Netcheck::new(
        dir.path().join("farglass/netchecks.toml"),
        admin,
        None,
    ))
    .await;

    let NetcheckEvent::Loaded {
        user, items, error, ..
    } = next_event(&mut link).await
    else {
        panic!("the channel starts with the checks");
    };
    assert!(user.is_empty());
    assert_eq!(error, None);
    assert!(items.iter().any(|item| item.id == "admin:0"));

    let checks = vec![NetcheckCheck {
        name: "local".to_owned(),
        kind: NetcheckKind::Tcp,
        host: "127.0.0.1".to_owned(),
        port: Some(port),
        timeout: Some(1000),
    }];
    let save = NetcheckRequest::Save {
        checks: checks.clone(),
    };
    link.send(Frame::json(CHECKS, FrameType::Control, &save).unwrap())
        .await
        .unwrap();
    let NetcheckEvent::Loaded { user, .. } = next_event(&mut link).await else {
        panic!("a save answers with the checks");
    };
    assert_eq!(user, checks);

    let bad = NetcheckRequest::Save {
        checks: vec![NetcheckCheck {
            port: None,
            ..checks[0].clone()
        }],
    };
    link.send(Frame::json(CHECKS, FrameType::Control, &bad).unwrap())
        .await
        .unwrap();
    assert!(matches!(
        next_event(&mut link).await,
        NetcheckEvent::Failed { .. }
    ));

    let run = NetcheckRequest::Run {
        ids: Some(vec!["user:0".to_owned(), "admin:0".to_owned()]),
    };
    link.send(Frame::json(CHECKS, FrameType::Control, &run).unwrap())
        .await
        .unwrap();
    let mut done = Vec::new();
    for _ in 0..2 {
        let NetcheckEvent::Result { id, state, .. } = next_event(&mut link).await else {
            panic!("a run answers with results");
        };
        assert_eq!(state, NetcheckState::Ok, "{id}");
        done.push(id);
    }
    done.sort();
    assert_eq!(done, ["admin:0", "user:0"]);
}

/// Runs the built-in checks on the network of this host. It needs the internet:
/// `cargo test -p farglass-session --test netcheck -- --ignored --nocapture`.
#[tokio::test]
#[ignore = "needs the internet"]
async fn the_built_in_checks_reach_the_internet() {
    let dir = tempfile::tempdir().unwrap();
    let netcheck = Netcheck::new(
        dir.path().join("user.toml"),
        dir.path().join("admin.toml"),
        Proxy::from_env(),
    );
    for job in netcheck.plan(None) {
        let event = netcheck.run(job).await;
        println!("{event:?}");
        let NetcheckEvent::Result { state, .. } = event else {
            panic!("a run answers with a result");
        };
        assert_eq!(state, NetcheckState::Ok);
    }
}
