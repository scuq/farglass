//! Tests of the login channel: farglassd with a real farglass-login and a PAM stack with no root.

use std::os::unix::fs::MetadataExt as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use farglass_login::server::{Service, Settings};
use farglass_proto::codec::FrameCodec;
use farglass_proto::login::{LoginEvent, LoginReply};
use farglass_proto::message::{Close, CloseReason, ConnectionMessage, LinkMessage, Open};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglassd::agents::Registry;
use farglassd::connection::{self, Context, Violation};
use futures_util::{SinkExt as _, StreamExt as _, sink, stream};
use tempfile::TempDir;
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::codec::Framed;
use zeroize::Zeroizing;

const WAIT: Duration = Duration::from_secs(10);

fn ch(number: u32) -> ChannelId {
    ChannelId::new(number)
}

/// A farglass-login with a PAM stack that accepts the password "secret".
struct LoginService {
    context: Arc<Context>,
    registry: Arc<Registry>,
    _dir: TempDir,
}

impl LoginService {
    async fn start() -> Self {
        Self::start_with(Arc::new(farglass_login::host::NoSessions), Duration::ZERO).await
    }

    async fn start_with(host: Arc<dyn farglass_login::host::Host>, agent_wait: Duration) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let pam_dir = dir.path().join("pam.d");
        std::fs::create_dir(&pam_dir).unwrap();
        let check = dir.path().join("check.sh");
        std::fs::write(
            &check,
            "#!/bin/sh\nIFS= read -r password; [ \"$password\" = secret ]\n",
        )
        .unwrap();
        std::fs::set_permissions(&check, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(
            pam_dir.join("farglass"),
            format!(
                "auth required pam_exec.so expose_authtok quiet {}\naccount required pam_permit.so\n",
                check.display()
            ),
        )
        .unwrap();
        let socket = dir.path().join("login.sock");
        let listener = farglass_login::activation::bind(&socket, 0o600).unwrap();
        let settings = Settings {
            pam_dir: Some(pam_dir),
            fail_delay: Duration::ZERO,
            host,
            ..Settings::default()
        };
        tokio::spawn(Service::new(settings).run(listener));

        let registry = Registry::new();
        let context = Arc::new(Context {
            agent_wait,
            ..Context::new(registry.clone(), socket)
        });
        Self {
            context,
            registry,
            _dir: dir,
        }
    }
}

/// The browser side of one connection.
struct Browser {
    hello_user: Option<String>,
    to_server: mpsc::Sender<Result<Frame, Violation>>,
    from_server: mpsc::Receiver<Frame>,
    _task: JoinHandle<Result<(), Violation>>,
}

impl Browser {
    async fn connect(context: &Arc<Context>) -> Self {
        let (to_server, server_in) = mpsc::channel(16);
        let (server_out, mut from_server) = mpsc::channel(16);
        let context = context.clone();
        let task = tokio::spawn(async move {
            let mut incoming = stream::unfold(server_in, |mut rx| async move {
                rx.recv().await.map(|item| (item, rx))
            })
            .boxed();
            let outgoing = sink::unfold(server_out, |tx, frame: Frame| async move {
                tx.send(frame).await.map(|()| tx)
            });
            let mut outgoing = std::pin::pin!(outgoing);
            let peer = Some("192.0.2.10".parse().unwrap());
            connection::run(
                &context,
                peer,
                context.dev_user.clone(),
                &mut incoming,
                &mut outgoing,
            )
            .await
        });
        let hello = tokio::time::timeout(WAIT, from_server.recv())
            .await
            .unwrap()
            .unwrap();
        let ConnectionMessage::Hello { user, .. } = hello.parse().unwrap();
        let browser = Self {
            hello_user: user,
            to_server,
            from_server,
            _task: task,
        };
        let ours = ConnectionMessage::Hello {
            version: PROTOCOL_VERSION,
            user: None,
        };
        browser.send(Frame::connection(&ours).unwrap()).await;
        browser
    }

    async fn send(&self, frame: Frame) {
        self.to_server.send(Ok(frame)).await.unwrap();
    }

    async fn recv(&mut self) -> Frame {
        tokio::time::timeout(WAIT, self.from_server.recv())
            .await
            .unwrap()
            .unwrap()
    }

    async fn event(&mut self, channel: u32) -> LoginEvent {
        let frame = self.recv().await;
        assert_eq!(
            (frame.channel(), frame.frame_type()),
            (ch(channel), FrameType::Control)
        );
        frame.parse().unwrap()
    }

    async fn answer(&self, channel: u32, prompt: u32, text: &str) {
        let answer = LoginReply::Answer {
            prompt,
            text: Zeroizing::new(text.to_owned()),
        };
        self.send(Frame::json(ch(channel), FrameType::Control, &answer).unwrap())
            .await;
    }

    /// Logs in on `channel` with `password` and returns the result event.
    async fn login(&mut self, channel: u32, user: &str, password: &str) -> LoginEvent {
        self.send(Frame::open(ch(channel), &Open::Login).unwrap())
            .await;
        assert_eq!(self.recv().await.frame_type(), FrameType::Ready);
        loop {
            match self.event(channel).await {
                LoginEvent::Prompt {
                    prompt, echo: true, ..
                } => self.answer(channel, prompt, user).await,
                LoginEvent::Prompt {
                    prompt,
                    echo: false,
                    ..
                } => self.answer(channel, prompt, password).await,
                LoginEvent::Info { .. } | LoginEvent::Error { .. } => {}
                result => {
                    let close = self.recv().await;
                    assert_eq!(close.parse::<Close>().unwrap().reason, CloseReason::Done);
                    self.send(Frame::close(ch(channel), &Close::new(CloseReason::Done)).unwrap())
                        .await;
                    return result;
                }
            }
        }
    }
}

fn current_user() -> (String, u32) {
    let uid = nix::unistd::getuid();
    (
        nix::unistd::User::from_uid(uid).unwrap().unwrap().name,
        uid.as_raw(),
    )
}

/// An agent for `uid` that answers each open with ready.
async fn fake_agent(registry: &Arc<Registry>, uid: u32) -> Framed<UnixStream, FrameCodec> {
    let (ours, theirs) = UnixStream::pair().unwrap();
    let task_registry = registry.clone();
    tokio::spawn(async move { task_registry.serve(theirs, uid).await });
    let mut link = Framed::new(ours, FrameCodec);
    link.next().await.unwrap().unwrap();
    let hello = LinkMessage::Hello {
        version: PROTOCOL_VERSION,
    };
    link.send(Frame::json(ChannelId::CONNECTION, FrameType::Control, &hello).unwrap())
        .await
        .unwrap();
    while registry.get(uid).is_none() {
        tokio::task::yield_now().await;
    }
    link
}

#[tokio::test]
async fn login_gives_the_connection_its_user_and_the_agent_of_that_user() {
    let service = LoginService::start().await;
    let (name, uid) = current_user();
    let mut agent = fake_agent(&service.registry, uid).await;
    let mut browser = Browser::connect(&service.context).await;
    assert_eq!(browser.hello_user, None);

    // Before the login, the connection reaches no agent.
    browser.send(Frame::open(ch(1), &Open::Echo).unwrap()).await;
    assert_eq!(
        browser.recv().await.parse::<Close>().unwrap().reason,
        CloseReason::AccessDenied
    );

    let result = browser.login(3, &name, "secret").await;
    assert!(
        matches!(result, LoginEvent::Success { ref user, uid: u, ticket: Some(_) } if *user == name && u == uid),
        "{result:?}"
    );

    browser.send(Frame::open(ch(5), &Open::Echo).unwrap()).await;
    let attach = tokio::time::timeout(WAIT, agent.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(
        attach.parse::<LinkMessage>().unwrap(),
        LinkMessage::Attach { .. }
    ));
    let open = tokio::time::timeout(WAIT, agent.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(open.frame_type(), FrameType::Open);
}

#[tokio::test]
async fn failed_login_leaves_the_connection_with_no_user_and_a_new_login_can_follow() {
    let service = LoginService::start().await;
    let (name, uid) = current_user();
    let mut browser = Browser::connect(&service.context).await;

    let result = browser.login(1, &name, "guess").await;
    assert_eq!(
        result,
        LoginEvent::Failure {
            message: "The login failed.".to_owned()
        }
    );
    browser.send(Frame::open(ch(3), &Open::Echo).unwrap()).await;
    assert_eq!(
        browser.recv().await.parse::<Close>().unwrap().reason,
        CloseReason::AccessDenied
    );

    let result = browser.login(5, &name, "secret").await;
    assert!(
        matches!(result, LoginEvent::Success { ref user, uid: u, ticket: Some(_) } if *user == name && u == uid),
        "{result:?}"
    );
}

#[tokio::test]
async fn second_login_on_a_connection_with_a_user_is_refused() {
    let service = LoginService::start().await;
    let (name, _) = current_user();
    let mut browser = Browser::connect(&service.context).await;
    browser.login(1, &name, "secret").await;

    browser
        .send(Frame::open(ch(3), &Open::Login).unwrap())
        .await;
    assert_eq!(
        browser.recv().await.parse::<Close>().unwrap().reason,
        CloseReason::NotSupported
    );
}

#[tokio::test]
async fn only_one_login_runs_on_a_connection_at_a_time() {
    let service = LoginService::start().await;
    let mut browser = Browser::connect(&service.context).await;
    browser
        .send(Frame::open(ch(1), &Open::Login).unwrap())
        .await;
    assert_eq!(browser.recv().await.frame_type(), FrameType::Ready);
    browser
        .send(Frame::open(ch(3), &Open::Login).unwrap())
        .await;
    let refused = loop {
        let frame = browser.recv().await;
        if frame.channel() == ch(3) {
            break frame;
        }
    };
    assert_eq!(
        refused.parse::<Close>().unwrap().reason,
        CloseReason::ResourceLimit
    );
}

#[tokio::test]
async fn shell_that_closes_the_login_channel_gets_a_close_answer() {
    let service = LoginService::start().await;
    let mut browser = Browser::connect(&service.context).await;
    browser
        .send(Frame::open(ch(1), &Open::Login).unwrap())
        .await;
    assert_eq!(browser.recv().await.frame_type(), FrameType::Ready);
    // pam_exec asks for the username first, because the start message has none.
    assert!(matches!(browser.event(1).await, LoginEvent::Prompt { .. }));

    browser
        .send(Frame::close(ch(1), &Close::new(CloseReason::Cancelled)).unwrap())
        .await;
    let answer = browser.recv().await;
    assert_eq!(
        (answer.channel(), answer.frame_type()),
        (ch(1), FrameType::Close)
    );
}

#[tokio::test]
async fn missing_login_service_closes_the_channel_with_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let context = Arc::new(Context {
        registry: Registry::new(),
        login_socket: dir.path().join("missing.sock"),
        dev_user: None,
        agent_wait: Duration::ZERO,
        sessions: Default::default(),
        origin: Default::default(),
        hsts: false,
    });
    let mut browser = Browser::connect(&context).await;
    browser
        .send(Frame::open(ch(1), &Open::Login).unwrap())
        .await;
    let close = browser.recv().await.parse::<Close>().unwrap();
    assert_eq!(close.reason, CloseReason::InternalError);
    assert_eq!(
        close.message.as_deref(),
        Some("The login service is not available.")
    );
}

#[tokio::test]
async fn dev_user_sends_the_user_in_the_hello_message() {
    let uid = std::fs::metadata(Path::new("/proc/self")).unwrap().uid();
    let context = Arc::new(Context {
        registry: Registry::new(),
        login_socket: "/nonexistent".into(),
        dev_user: Some(farglassd::login::Identity::from_uid(uid)),
        agent_wait: Duration::ZERO,
        sessions: Default::default(),
        origin: Default::default(),
        hsts: false,
    });
    let browser = Browser::connect(&context).await;
    assert_eq!(browser.hello_user, Some(current_user().0));
}

/// A host with one local GNOME session.
#[derive(Debug)]
struct OneLocalSession;

impl farglass_login::host::Host for OneLocalSession {
    fn local_sessions(
        &self,
        _uid: u32,
    ) -> std::io::Result<Vec<farglass_proto::login::LocalSession>> {
        Ok(vec![farglass_proto::login::LocalSession {
            id: "2".to_owned(),
            kind: "wayland".to_owned(),
            seat: Some("seat0".to_owned()),
        }])
    }

    fn end_sessions(&self, _ids: &[String]) -> std::io::Result<()> {
        Ok(())
    }

    fn start_session(
        &self,
        _user: &str,
        _uid: u32,
    ) -> std::io::Result<farglass_login::host::Started> {
        Ok(farglass_login::host::Started::New)
    }
}

#[tokio::test]
async fn confirm_goes_to_the_shell_and_its_answer_back() {
    let service = LoginService::start_with(Arc::new(OneLocalSession), Duration::ZERO).await;
    let (name, uid) = current_user();
    let mut browser = Browser::connect(&service.context).await;
    browser
        .send(Frame::open(ch(1), &Open::Login).unwrap())
        .await;
    assert_eq!(browser.recv().await.frame_type(), FrameType::Ready);
    loop {
        match browser.event(1).await {
            LoginEvent::Prompt {
                prompt, echo: true, ..
            } => browser.answer(1, prompt, &name).await,
            LoginEvent::Prompt {
                prompt,
                echo: false,
                ..
            } => browser.answer(1, prompt, "secret").await,
            LoginEvent::Confirm { sessions } => {
                assert_eq!(sessions[0].kind, "wayland");
                let accept = LoginReply::Confirm { accept: true };
                browser
                    .send(Frame::json(ch(1), FrameType::Control, &accept).unwrap())
                    .await;
            }
            LoginEvent::Info { .. } => {}
            result => {
                assert!(
                    matches!(result, LoginEvent::Success { ref user, uid: u, ticket: Some(_) } if *user == name && u == uid),
                    "{result:?}"
                );
                return;
            }
        }
    }
}

#[tokio::test]
async fn success_waits_for_the_agent_of_the_new_session() {
    let service = LoginService::start_with(Arc::new(farglass_login::host::NoSessions), WAIT).await;
    let (name, uid) = current_user();
    let registry = service.registry.clone();
    // The agent of the new session connects 300 ms after the success of PAM.
    let late_agent = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        fake_agent(&registry, uid).await
    });
    let mut browser = Browser::connect(&service.context).await;
    let result = browser.login(1, &name, "secret").await;
    assert!(
        matches!(result, LoginEvent::Success { ref user, uid: u, ticket: Some(_) } if *user == name && u == uid),
        "{result:?}"
    );
    assert!(
        service.registry.get(uid).is_some(),
        "the success came before the agent"
    );
    let _agent = late_agent.await.unwrap();
}
