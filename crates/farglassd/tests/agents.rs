//! Tests of the agent link: the uid check, the routes of the channels, and the end of a link.

use std::os::unix::fs::MetadataExt as _;
use std::sync::Arc;
use std::time::Duration;

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{Close, CloseReason, ConnectionMessage, LinkMessage, Open};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglassd::agents::{self, Registry};
use farglassd::connection::{self, Context, Violation};
use farglassd::login::Identity;
use futures_util::{SinkExt as _, StreamExt as _, sink, stream};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::codec::Framed;

const WAIT: Duration = Duration::from_secs(5);

fn ch(number: u32) -> ChannelId {
    ChannelId::new(number)
}

/// The browser side of one connection, through channels instead of a transport.
struct Browser {
    to_server: mpsc::Sender<Result<Frame, Violation>>,
    from_server: mpsc::Receiver<Frame>,
    task: JoinHandle<Result<(), Violation>>,
}

impl Browser {
    /// Connects as the user with the uid `user`, with no login, and does the hello exchange.
    async fn connect(registry: &Arc<Registry>, user: Option<u32>) -> Self {
        let (to_server, server_in) = mpsc::channel(16);
        let (server_out, from_server) = mpsc::channel(16);
        let context = Context {
            dev_user: user.map(|uid| Identity {
                uid,
                name: format!("user{uid}"),
            }),
            agent_wait: Duration::ZERO,
            ..Context::new(registry.clone(), "/nonexistent/login.sock".into())
        };
        let task = tokio::spawn(async move {
            let mut incoming = stream::unfold(server_in, |mut rx| async move {
                rx.recv().await.map(|item| (item, rx))
            })
            .boxed();
            let outgoing = sink::unfold(server_out, |tx, frame: Frame| async move {
                tx.send(frame).await.map(|()| tx)
            });
            let mut outgoing = std::pin::pin!(outgoing);
            connection::run(
                &context,
                None,
                context.dev_user.clone(),
                None,
                &mut incoming,
                &mut outgoing,
            )
            .await
        });
        let mut browser = Self {
            to_server,
            from_server,
            task,
        };
        let hello = browser.recv().await;
        assert!(hello.channel().is_connection());
        browser
            .send(
                Frame::connection(&ConnectionMessage::Hello {
                    version: PROTOCOL_VERSION,
                    user: None,
                    locked: None,
                })
                .unwrap(),
            )
            .await;
        browser
    }

    async fn send(&self, frame: Frame) {
        self.to_server.send(Ok(frame)).await.unwrap();
    }

    async fn open(&self, number: u32) {
        self.send(Frame::open(ch(number), &Open::Echo).unwrap())
            .await;
    }

    async fn recv(&mut self) -> Frame {
        tokio::time::timeout(WAIT, self.from_server.recv())
            .await
            .expect("no frame from the server")
            .expect("the server stopped")
    }

    /// Stops the connection as a closed transport does.
    async fn stop(self) -> Result<(), Violation> {
        drop(self.to_server);
        self.task.await.unwrap()
    }

    /// Waits for the result of the connection task.
    async fn result(self) -> Result<(), Violation> {
        tokio::time::timeout(WAIT, self.task)
            .await
            .unwrap()
            .unwrap()
    }
}

/// An agent that the test controls. It sends each data frame back.
struct FakeAgent {
    link: Framed<UnixStream, FrameCodec>,
}

impl FakeAgent {
    /// Connects a fake agent for `uid` to `registry` and does the hello exchange.
    async fn connect(registry: &Arc<Registry>, uid: u32) -> Self {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let registry_task = registry.clone();
        tokio::spawn(async move { registry_task.serve(theirs, uid).await });
        let mut agent = Self {
            link: Framed::new(ours, FrameCodec),
        };
        let hello = agent.recv().await;
        assert_eq!(
            link(&hello),
            LinkMessage::Hello {
                version: PROTOCOL_VERSION
            }
        );
        agent
            .link
            .send(
                Frame::json(
                    ChannelId::CONNECTION,
                    FrameType::Control,
                    &LinkMessage::Hello {
                        version: PROTOCOL_VERSION,
                    },
                )
                .unwrap(),
            )
            .await
            .unwrap();
        // The registry has the agent after it got the hello message.
        tokio::time::timeout(WAIT, async {
            while registry.get(uid).is_none() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        agent
    }

    async fn recv(&mut self) -> Frame {
        tokio::time::timeout(WAIT, self.link.next())
            .await
            .expect("no frame from farglassd")
            .expect("the link stopped")
            .unwrap()
    }

    async fn send(&mut self, frame: Frame) {
        self.link.send(frame).await.unwrap();
    }

    /// Reports whether the agent gets no frame in a short time.
    async fn is_quiet(&mut self) -> bool {
        tokio::time::timeout(Duration::from_millis(100), self.link.next())
            .await
            .is_err()
    }
}

fn link(frame: &Frame) -> LinkMessage {
    assert!(frame.channel().is_connection());
    frame.parse().unwrap()
}

fn data(number: u32, payload: &'static [u8]) -> Frame {
    Frame::data(ch(number), payload).unwrap()
}

fn close(number: u32, reason: CloseReason) -> Frame {
    Frame::close(ch(number), &Close::new(reason)).unwrap()
}

fn reason(frame: &Frame) -> CloseReason {
    assert_eq!(frame.frame_type(), FrameType::Close);
    frame.parse::<Close>().unwrap().reason
}

#[tokio::test]
async fn real_agent_connects_and_farglassd_knows_its_uid_from_the_kernel() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run/agent.sock");
    let listener = agents::bind(&path).unwrap();
    let registry = Registry::new();
    tokio::spawn(agents::accept_loop(listener, registry.clone()));

    // The uid of this process is the owner of a file that it creates.
    let uid = std::fs::metadata(dir.path()).unwrap().uid();
    let session = tokio::spawn({
        let path = path.clone();
        async move {
            farglass_session::link::connect(&path, &mut farglass_session::link::Session::default())
                .await
        }
    });
    tokio::time::timeout(WAIT, async {
        while registry.uids() != [uid] {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    // A browser connection of the user reaches the echo channel of the agent.
    let mut browser = Browser::connect(&registry, Some(uid)).await;
    browser.open(1).await;
    let ready = browser.recv().await;
    assert_eq!(
        (ready.channel(), ready.frame_type()),
        (ch(1), FrameType::Ready)
    );
    browser.send(data(1, b"ping")).await;
    let echo = browser.recv().await;
    assert_eq!(
        (echo.channel(), echo.payload().as_ref()),
        (ch(1), &b"ping"[..])
    );
    browser.send(close(1, CloseReason::Done)).await;
    assert_eq!(reason(&browser.recv().await), CloseReason::Done);

    // A browser connection of a different user does not reach the agent.
    let mut other = Browser::connect(&registry, Some(uid + 1)).await;
    other.open(1).await;
    assert_eq!(reason(&other.recv().await), CloseReason::NotFound);

    session.abort();
}

#[tokio::test]
async fn socket_is_open_to_all_users_and_replaces_an_old_socket_only() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent.sock");
    drop(agents::bind(&path).unwrap());
    // The old socket of a stopped farglassd stays on the disk. A second bind replaces it.
    let _listener = agents::bind(&path).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o666);

    let file = dir.path().join("not-a-socket");
    std::fs::write(&file, "data").unwrap();
    assert!(agents::bind(&file).is_err());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "data");
}

#[tokio::test]
async fn each_user_reaches_only_the_agent_of_that_user() {
    let registry = Registry::new();
    let mut alice = FakeAgent::connect(&registry, 1000).await;
    let mut bob = FakeAgent::connect(&registry, 1001).await;
    assert_eq!(registry.uids(), [1000, 1001]);

    let mut alice_browser = Browser::connect(&registry, Some(1000)).await;
    let mut bob_browser = Browser::connect(&registry, Some(1001)).await;

    alice_browser.open(1).await;
    alice_browser.send(data(1, b"from alice")).await;
    assert!(matches!(
        link(&alice.recv().await),
        LinkMessage::Attach { .. }
    ));
    let open = alice.recv().await;
    assert_eq!(open.frame_type(), FrameType::Open);
    assert_eq!(alice.recv().await.payload().as_ref(), b"from alice");

    bob_browser.open(1).await;
    bob_browser.send(data(1, b"from bob")).await;
    assert!(matches!(
        link(&bob.recv().await),
        LinkMessage::Attach { .. }
    ));
    let bob_open = bob.recv().await;
    assert_eq!(bob.recv().await.payload().as_ref(), b"from bob");

    // Each agent answers on its own link, and only its own user gets the answer.
    alice.send(data(open.channel().get(), b"to alice")).await;
    bob.send(data(bob_open.channel().get(), b"to bob")).await;
    assert_eq!(alice_browser.recv().await.payload().as_ref(), b"to alice");
    assert_eq!(bob_browser.recv().await.payload().as_ref(), b"to bob");
    assert!(alice.is_quiet().await);
    assert!(bob.is_quiet().await);
}

#[tokio::test]
async fn two_connections_of_one_user_share_the_agent_with_separate_channels() {
    let registry = Registry::new();
    let mut agent = FakeAgent::connect(&registry, 1000).await;
    let mut first = Browser::connect(&registry, Some(1000)).await;
    let mut second = Browser::connect(&registry, Some(1000)).await;

    // The two shells use the same channel number.
    first.open(1).await;
    let LinkMessage::Attach { connection: c1 } = link(&agent.recv().await) else {
        panic!()
    };
    let a = agent.recv().await.channel();
    second.open(1).await;
    let LinkMessage::Attach { connection: c2 } = link(&agent.recv().await) else {
        panic!()
    };
    let b = agent.recv().await.channel();
    assert_ne!(c1, c2);
    assert_ne!(a, b, "the agent must see two channels");

    agent.send(Frame::data(b, &b"second"[..]).unwrap()).await;
    agent.send(Frame::data(a, &b"first"[..]).unwrap()).await;
    let to_first = first.recv().await;
    let to_second = second.recv().await;
    assert_eq!(
        (to_first.channel(), to_first.payload().as_ref()),
        (ch(1), &b"first"[..])
    );
    assert_eq!(
        (to_second.channel(), to_second.payload().as_ref()),
        (ch(1), &b"second"[..])
    );
}

#[tokio::test]
async fn end_of_a_browser_connection_closes_its_channels_on_the_agent() {
    let registry = Registry::new();
    let mut agent = FakeAgent::connect(&registry, 1000).await;
    let browser = Browser::connect(&registry, Some(1000)).await;
    browser.open(1).await;
    browser.open(3).await;
    browser.send(close(3, CloseReason::Done)).await;
    let LinkMessage::Attach { connection } = link(&agent.recv().await) else {
        panic!()
    };
    let one = agent.recv().await.channel();
    let three = agent.recv().await.channel();
    assert_eq!(agent.recv().await.channel(), three);

    browser.stop().await.unwrap();
    // The shell closed channel 3 itself. Only channel 1 gets a close frame.
    let closed = agent.recv().await;
    assert_eq!(closed.channel(), one);
    assert_eq!(reason(&closed), CloseReason::Cancelled);
    assert_eq!(
        link(&agent.recv().await),
        LinkMessage::Detach { connection }
    );
}

#[tokio::test]
async fn end_of_the_agent_closes_the_channels_in_the_browser() {
    let registry = Registry::new();
    let mut agent = FakeAgent::connect(&registry, 1000).await;
    let mut browser = Browser::connect(&registry, Some(1000)).await;
    browser.open(1).await;
    agent.recv().await;
    agent.recv().await;

    drop(agent);
    let closed = browser.recv().await;
    assert_eq!(closed.channel(), ch(1));
    assert_eq!(reason(&closed), CloseReason::InternalError);

    tokio::time::timeout(WAIT, async {
        while registry.get(1000).is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // A new channel finds no agent now.
    browser.open(3).await;
    assert_eq!(reason(&browser.recv().await), CloseReason::NotFound);
}

#[tokio::test]
async fn new_agent_of_a_user_replaces_the_old_one() {
    let registry = Registry::new();
    let mut old = FakeAgent::connect(&registry, 1000).await;
    let mut browser = Browser::connect(&registry, Some(1000)).await;
    browser.open(1).await;
    old.recv().await;
    old.recv().await;

    let mut new = FakeAgent::connect(&registry, 1000).await;
    assert_eq!(reason(&browser.recv().await), CloseReason::InternalError);

    browser.open(3).await;
    assert!(matches!(
        link(&new.recv().await),
        LinkMessage::Attach { .. }
    ));
    assert_eq!(new.recv().await.frame_type(), FrameType::Open);
}

#[tokio::test]
async fn channel_is_free_after_the_close_frames_of_the_two_sides() {
    let registry = Registry::new();
    let mut agent = FakeAgent::connect(&registry, 1000).await;
    let mut browser = Browser::connect(&registry, Some(1000)).await;
    browser.open(1).await;
    agent.recv().await;
    let number = agent.recv().await.channel();

    agent
        .send(Frame::close(number, &Close::new(CloseReason::Done)).unwrap())
        .await;
    assert_eq!(reason(&browser.recv().await), CloseReason::Done);
    browser.send(close(1, CloseReason::Done)).await;
    assert_eq!(reason(&agent.recv().await), CloseReason::Done);

    // Late frames on the closed channel go nowhere.
    agent.send(Frame::data(number, &b"late"[..]).unwrap()).await;
    browser.send(data(1, b"late")).await;
    assert!(agent.is_quiet().await);
    browser.open(3).await;
    let next = agent.recv().await;
    assert_eq!(next.frame_type(), FrameType::Open);
    assert_ne!(next.channel(), number);
}

#[tokio::test]
async fn connection_with_no_login_reaches_no_agent() {
    let registry = Registry::new();
    let mut agent = FakeAgent::connect(&registry, 1000).await;
    let mut browser = Browser::connect(&registry, None).await;
    browser.open(1).await;
    assert_eq!(reason(&browser.recv().await), CloseReason::AccessDenied);
    assert!(agent.is_quiet().await);
}

#[tokio::test]
async fn agent_that_opens_a_channel_loses_its_link() {
    let registry = Registry::new();
    let mut agent = FakeAgent::connect(&registry, 1000).await;
    agent.send(Frame::open(ch(2), &Open::Echo).unwrap()).await;
    tokio::time::timeout(WAIT, async {
        while registry.get(1000).is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn shell_violations_stop_the_connection() {
    let registry = Registry::new();
    let _agent = FakeAgent::connect(&registry, 1000).await;

    let reused = Browser::connect(&registry, Some(1000)).await;
    reused.open(3).await;
    reused.open(3).await;
    assert!(matches!(
        reused.result().await,
        Err(Violation::ChannelNumber(_))
    ));

    let lower = Browser::connect(&registry, Some(1000)).await;
    lower.open(5).await;
    lower.open(1).await;
    assert!(matches!(
        lower.result().await,
        Err(Violation::ChannelNumber(_))
    ));

    let never = Browser::connect(&registry, Some(1000)).await;
    never.send(data(7, b"x")).await;
    assert!(matches!(never.result().await, Err(Violation::NotOpen(_))));

    let second_hello = Browser::connect(&registry, Some(1000)).await;
    second_hello
        .send(
            Frame::connection(&ConnectionMessage::Hello {
                version: PROTOCOL_VERSION,
                user: None,
                locked: None,
            })
            .unwrap(),
        )
        .await;
    assert!(matches!(
        second_hello.result().await,
        Err(Violation::SecondHello)
    ));
}
