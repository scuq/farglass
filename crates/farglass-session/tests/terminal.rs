//! Tests of the terminals of the agent, with real ptys and /bin/sh.

use std::path::PathBuf;
use std::time::Duration;

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{Close, CloseReason, LinkMessage, Open, PtyEvent, PtyRequest};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION, PTY_WINDOW};
use farglass_session::link::Session;
use farglass_session::state::StateStore;
use farglass_session::terminal::{TerminalOptions, Terminals};
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_util::codec::Framed;

const WAIT: Duration = Duration::from_secs(10);

fn ch(number: u32) -> ChannelId {
    ChannelId::new(number)
}

fn options(scope: bool) -> TerminalOptions {
    TerminalOptions {
        scope,
        shell: Some(PathBuf::from("/bin/sh")),
    }
}

/// The farglassd side of one link.
struct Link(Framed<UnixStream, FrameCodec>);

/// Serves links in sequence with one session, as the agent does after a restart of farglassd.
fn agent(scope: bool) -> mpsc::UnboundedSender<UnixStream> {
    let (streams, mut next) = mpsc::unbounded_channel::<UnixStream>();
    tokio::spawn(async move {
        let mut session = Session {
            store: StateStore::default(),
            terminals: Terminals::new(options(scope)),
        };
        while let Some(stream) = next.recv().await {
            let _ = farglass_session::link::serve_with(stream, &mut session).await;
        }
    });
    streams
}

impl Link {
    async fn connect(agent: &mpsc::UnboundedSender<UnixStream>) -> Self {
        let (ours, theirs) = UnixStream::pair().unwrap();
        agent.send(theirs).unwrap();
        let mut link = Self(Framed::new(ours, FrameCodec));
        link.recv().await;
        let hello = LinkMessage::Hello {
            version: PROTOCOL_VERSION,
        };
        link.send(Frame::json(ChannelId::CONNECTION, FrameType::Control, &hello).unwrap())
            .await;
        link
    }

    async fn send(&mut self, frame: Frame) {
        self.0.send(frame).await.unwrap();
    }

    async fn recv(&mut self) -> Frame {
        tokio::time::timeout(WAIT, self.0.next())
            .await
            .expect("no frame")
            .unwrap()
            .unwrap()
    }

    /// Opens a pty channel and returns the attach event.
    async fn open(&mut self, number: u32, terminal: &str) -> PtyEvent {
        let open = Open::Pty {
            terminal: terminal.to_owned(),
            cols: 80,
            rows: 24,
        };
        self.send(Frame::open(ch(number), &open).unwrap()).await;
        assert_eq!(self.recv().await.frame_type(), FrameType::Ready);
        let attached = self.recv().await;
        attached.parse().unwrap()
    }

    async fn input(&mut self, number: u32, text: &str) {
        self.send(Frame::data(ch(number), text.as_bytes().to_vec()).unwrap())
            .await;
    }

    async fn request(&mut self, number: u32, request: &PtyRequest) {
        self.send(Frame::json(ch(number), FrameType::Control, request).unwrap())
            .await;
    }

    /// Reads data frames of `number` until the output contains `text`. Acknowledges all bytes.
    async fn until(&mut self, number: u32, text: &str) -> String {
        let mut output = String::new();
        tokio::time::timeout(WAIT, async {
            while !output.contains(text) {
                let frame = self.0.next().await.unwrap().unwrap();
                if frame.channel() == ch(number) && frame.frame_type() == FrameType::Data {
                    output.push_str(&String::from_utf8_lossy(frame.payload()));
                    let ack = PtyRequest::Ack {
                        bytes: u32::try_from(frame.payload().len()).unwrap(),
                    };
                    self.0
                        .send(Frame::json(ch(number), FrameType::Control, &ack).unwrap())
                        .await
                        .unwrap();
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("the output has no {text:?}:\n{output}"));
        output
    }

    /// Returns the number of data bytes that arrive until the link is quiet for `quiet`.
    async fn bytes_until_quiet(&mut self, number: u32, quiet: Duration) -> usize {
        let mut total = 0;
        while let Ok(Some(Ok(frame))) = tokio::time::timeout(quiet, self.0.next()).await {
            if frame.channel() == ch(number) && frame.frame_type() == FrameType::Data {
                total += frame.payload().len();
            }
        }
        total
    }

    /// Waits for the exit event and the close frame of `number`.
    async fn exited(&mut self, number: u32) -> PtyEvent {
        loop {
            let frame = self.recv().await;
            if frame.channel() == ch(number)
                && frame.frame_type() == FrameType::Control
                && let Ok(event @ PtyEvent::Exited { .. }) = frame.parse::<PtyEvent>()
            {
                let close = self.recv().await;
                assert_eq!(close.frame_type(), FrameType::Close);
                return event;
            }
        }
    }

    async fn close(&mut self, number: u32) {
        self.send(Frame::close(ch(number), &Close::new(CloseReason::Cancelled)).unwrap())
            .await;
        // The answer of the agent.
        loop {
            let frame = self.recv().await;
            if frame.channel() == ch(number) && frame.frame_type() == FrameType::Close {
                return;
            }
        }
    }
}

#[tokio::test]
async fn new_terminal_runs_a_shell_with_the_size_of_the_channel() {
    let agent = agent(false);
    let mut link = Link::connect(&agent).await;
    assert_eq!(
        link.open(1, "t1").await,
        PtyEvent::Attached {
            new: true,
            replay: 0
        }
    );
    link.input(1, "echo hello-$((40+2))\n").await;
    link.until(1, "hello-42").await;
    link.input(1, "stty size\n").await;
    link.until(1, "24 80").await;
    link.request(
        1,
        &PtyRequest::Resize {
            cols: 132,
            rows: 50,
        },
    )
    .await;
    link.input(1, "stty size\n").await;
    link.until(1, "50 132").await;
}

#[tokio::test]
async fn output_stops_at_the_window_until_the_shell_acknowledges() {
    let agent = agent(false);
    let mut link = Link::connect(&agent).await;
    link.open(1, "flow").await;
    // About 4 MiB of output, much more than the window.
    link.input(
        1,
        "head -c 4000000 /dev/zero | tr '\\\\0' x; echo; echo flow-end\n",
    )
    .await;
    let first = link.bytes_until_quiet(1, Duration::from_millis(500)).await;
    let window = PTY_WINDOW as usize;
    assert!(first >= window / 2, "only {first} bytes came");
    assert!(
        first <= window + 64 * 1024,
        "{first} bytes came, more than the window"
    );

    // The acknowledgments let the rest come.
    link.request(
        1,
        &PtyRequest::Ack {
            bytes: u32::try_from(first).unwrap(),
        },
    )
    .await;
    link.until(1, "flow-end").await;
}

#[tokio::test]
async fn reattach_gives_the_scrollback_and_the_same_shell() {
    let agent = agent(false);
    let mut link = Link::connect(&agent).await;
    link.open(1, "t2").await;
    link.input(1, "MARK=kept; echo first-$MARK\n").await;
    link.until(1, "first-kept").await;
    link.close(1).await;

    let PtyEvent::Attached { new, replay } = link.open(3, "t2").await else {
        panic!()
    };
    assert!(!new);
    assert!(replay > 0);
    let scrollback = link.until(3, "first-kept").await;
    assert!(scrollback.contains("echo first-$MARK"));
    // The shell kept its variable: it is the same process.
    link.input(3, "echo again-$MARK\n").await;
    link.until(3, "again-kept").await;
}

#[tokio::test]
async fn job_continues_with_no_channel_and_its_output_is_in_the_scrollback() {
    let agent = agent(false);
    let mut link = Link::connect(&agent).await;
    link.open(1, "t3").await;
    link.input(1, "sleep 0.5; echo done-while-away\n").await;
    link.close(1).await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    link.open(3, "t3").await;
    link.until(3, "done-while-away").await;
}

#[tokio::test]
async fn terminal_survives_a_break_of_the_link() {
    let agent = agent(false);
    let mut link = Link::connect(&agent).await;
    link.open(1, "t4").await;
    link.input(1, "echo before-break\n").await;
    link.until(1, "before-break").await;
    drop(link);

    let mut link = Link::connect(&agent).await;
    let PtyEvent::Attached { new, .. } = link.open(1, "t4").await else {
        panic!()
    };
    assert!(!new);
    link.until(1, "before-break").await;
}

#[tokio::test]
async fn exit_and_terminate_end_the_terminal() {
    let agent = agent(false);
    let mut link = Link::connect(&agent).await;
    link.open(1, "t5").await;
    link.input(1, "exit 3\n").await;
    assert_eq!(
        link.exited(1).await,
        PtyEvent::Exited {
            code: Some(3),
            signal: None
        }
    );

    link.open(3, "t6").await;
    link.request(3, &PtyRequest::Terminate).await;
    let PtyEvent::Exited { signal, .. } = link.exited(3).await else {
        panic!()
    };
    assert_eq!(signal, Some(1), "SIGHUP");

    // A new open with the same id starts a new shell.
    assert_eq!(
        link.open(5, "t6").await,
        PtyEvent::Attached {
            new: true,
            replay: 0
        }
    );
}

#[tokio::test]
async fn exit_with_no_channel_gives_the_scrollback_and_the_exit_at_the_next_attach() {
    let agent = agent(false);
    let mut link = Link::connect(&agent).await;
    link.open(1, "t7").await;
    link.input(1, "sleep 0.3; echo last-words; exit 7\n").await;
    link.close(1).await;
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let PtyEvent::Attached { new, .. } = link.open(3, "t7").await else {
        panic!()
    };
    assert!(!new);
    let mut output = String::new();
    let exit = loop {
        let frame = link.recv().await;
        match frame.frame_type() {
            FrameType::Data => output.push_str(&String::from_utf8_lossy(frame.payload())),
            FrameType::Control => break frame.parse::<PtyEvent>().unwrap(),
            _ => {}
        }
    };
    assert!(output.contains("last-words"), "{output}");
    assert_eq!(
        exit,
        PtyEvent::Exited {
            code: Some(7),
            signal: None
        }
    );
}

#[tokio::test]
async fn terminal_runs_in_its_own_systemd_user_scope() {
    if !TerminalOptions::from_env().scope {
        eprintln!("SKIP: no systemd user manager");
        return;
    }
    let agent = agent(true);
    let mut link = Link::connect(&agent).await;
    link.open(1, "scoped").await;
    link.input(1, "cat /proc/self/cgroup\n").await;
    let output = link.until(1, ".scope").await;
    assert!(output.contains("farglass-terminal-"), "{output}");
    link.request(1, &PtyRequest::Terminate).await;
    link.exited(1).await;
}
