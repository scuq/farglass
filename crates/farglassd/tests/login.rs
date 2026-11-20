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
use farglassd::login::Identity;
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
        Self::start_full(host, agent_wait, None).await
    }

    async fn start_full(
        host: Arc<dyn farglass_login::host::Host>,
        agent_wait: Duration,
        allowed_group: Option<String>,
    ) -> Self {
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
            allowed_group,
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
    hello_locked: Option<String>,
    to_server: mpsc::Sender<Result<Frame, Violation>>,
    from_server: mpsc::Receiver<Frame>,
    _task: JoinHandle<Result<(), Violation>>,
}

impl Browser {
    async fn connect(context: &Arc<Context>) -> Self {
        Self::connect_as(context, context.dev_user.clone()).await
    }

    /// Connects with the user of a session cookie.
    async fn connect_as(context: &Arc<Context>, user: Option<Identity>) -> Self {
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
            let client = connection::Client {
                addr: Some("192.0.2.10".parse().unwrap()),
                user,
                ..connection::Client::default()
            };
            connection::run(&context, client, &mut incoming, &mut outgoing).await
        });
        let hello = tokio::time::timeout(WAIT, from_server.recv())
            .await
            .unwrap()
            .unwrap();
        let ConnectionMessage::Hello { user, locked, .. } = hello.parse().unwrap() else {
            panic!("the first message was not a hello message");
        };
        let browser = Self {
            hello_user: user,
            hello_locked: locked,
            to_server,
            from_server,
            _task: task,
        };
        let ours = ConnectionMessage::Hello {
            version: PROTOCOL_VERSION,
            user: None,
            locked: None,
            pin: None,
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
        agent_wait: Duration::ZERO,
        ..Context::new(Registry::new(), dir.path().join("missing.sock"))
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
        dev_user: Some(farglassd::login::Identity::from_uid(uid)),
        agent_wait: Duration::ZERO,
        ..Context::new(Registry::new(), "/nonexistent".into())
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

    fn end_web_sessions(&self, _uid: u32) -> std::io::Result<u32> {
        Ok(0)
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

/// A host that records the logouts.
#[derive(Debug, Default)]
struct LogoutHost(std::sync::Mutex<Vec<u32>>);

impl farglass_login::host::Host for LogoutHost {
    fn local_sessions(
        &self,
        _uid: u32,
    ) -> std::io::Result<Vec<farglass_proto::login::LocalSession>> {
        Ok(Vec::new())
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

    fn end_web_sessions(&self, uid: u32) -> std::io::Result<u32> {
        self.0.lock().unwrap().push(uid);
        Ok(1)
    }
}

/// Waits for the logout message of farglassd on a connection.
async fn logged_out(browser: &mut Browser) {
    loop {
        let frame = browser.recv().await;
        if frame.channel().is_connection() {
            assert_eq!(
                frame.parse::<ConnectionMessage>().unwrap(),
                ConnectionMessage::LoggedOut
            );
            return;
        }
    }
}

#[tokio::test]
async fn logout_reaches_each_tab_of_the_user_and_ends_the_web_session() {
    let host = Arc::new(LogoutHost::default());
    let service = LoginService::start_with(host.clone(), Duration::ZERO).await;
    let (name, uid) = current_user();
    let mut first = Browser::connect(&service.context).await;
    let mut second = Browser::connect(&service.context).await;
    first.login(1, &name, "secret").await;
    second.login(1, &name, "secret").await;

    // A connection of a different user, here one with no login, continues.
    let mut other = Browser::connect(&service.context).await;

    farglassd::login::logout(&service.context, uid, "test").await;
    logged_out(&mut first).await;
    logged_out(&mut second).await;
    assert_eq!(*host.0.lock().unwrap(), [uid]);

    other.send(Frame::open(ch(1), &Open::Echo).unwrap()).await;
    assert_eq!(
        other.recv().await.parse::<Close>().unwrap().reason,
        CloseReason::AccessDenied
    );
}

#[tokio::test]
async fn idle_logout_ends_a_session_with_no_tab_and_keeps_a_session_with_a_tab() {
    let host = Arc::new(LogoutHost::default());
    let service = LoginService::start_with(host.clone(), Duration::ZERO).await;
    let (name, uid) = current_user();
    let _agent = fake_agent(&service.registry, uid).await;

    let mut browser = Browser::connect(&service.context).await;
    browser.login(1, &name, "secret").await;
    farglassd::server::logout_idle(&service.context, Duration::ZERO).await;
    assert!(
        host.0.lock().unwrap().is_empty(),
        "a user with a tab is not idle"
    );

    drop(browser);
    // The connection task ends after the transport closes.
    tokio::time::timeout(WAIT, async {
        loop {
            farglassd::server::logout_idle(&service.context, Duration::ZERO).await;
            if !host.0.lock().unwrap().is_empty() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the idle logout did not end the session");
    assert_eq!(*host.0.lock().unwrap(), [uid]);
}

/// Waits for the connection message `expected` of farglassd, and skips the frames of channels.
async fn connection_message(browser: &mut Browser, expected: ConnectionMessage) {
    loop {
        let frame = browser.recv().await;
        if frame.channel().is_connection() {
            assert_eq!(frame.parse::<ConnectionMessage>().unwrap(), expected);
            return;
        }
    }
}

/// Waits for the LockedHint message on the agent link.
async fn locked_hint(agent: &mut Framed<UnixStream, FrameCodec>) -> bool {
    loop {
        let frame = tokio::time::timeout(WAIT, agent.next())
            .await
            .expect("no LockedHint")
            .unwrap()
            .unwrap();
        if frame.channel().is_connection()
            && let Ok(LinkMessage::LockedHint { locked }) = frame.parse()
        {
            return locked;
        }
    }
}

async fn lock_service() -> (LoginService, String, u32, Framed<UnixStream, FrameCodec>) {
    let service = LoginService::start().await;
    tokio::spawn(farglassd::lock::agent_loop(
        service.context.presence.clone(),
        service.registry.clone(),
    ));
    let (name, uid) = current_user();
    let mut agent = fake_agent(&service.registry, uid).await;
    assert!(
        !locked_hint(&mut agent).await,
        "a new agent gets the lock state"
    );
    (service, name, uid, agent)
}

#[tokio::test]
async fn lock_closes_each_tab_and_only_a_login_of_the_user_unlocks() {
    let (service, name, uid, mut agent) = lock_service().await;
    let mut first = Browser::connect(&service.context).await;
    first.login(1, &name, "secret").await;
    let mut second = Browser::connect_as(
        &service.context,
        Some(Identity {
            uid,
            name: name.clone(),
        }),
    )
    .await;
    assert_eq!(second.hello_user.as_deref(), Some(name.as_str()));

    first
        .send(Frame::connection(&ConnectionMessage::Lock).unwrap())
        .await;
    connection_message(&mut first, ConnectionMessage::Locked).await;
    connection_message(&mut second, ConnectionMessage::Locked).await;
    assert!(locked_hint(&mut agent).await);

    // A new tab with the cookie gets the lock screen and no channel of the agent.
    let identity = Identity {
        uid,
        name: name.clone(),
    };
    let mut locked = Browser::connect_as(&service.context, Some(identity.clone())).await;
    assert_eq!(
        (locked.hello_user.as_deref(), locked.hello_locked.as_deref()),
        (None, Some(name.as_str()))
    );
    locked.send(Frame::open(ch(1), &Open::Echo).unwrap()).await;
    assert_eq!(
        locked.recv().await.parse::<Close>().unwrap().reason,
        CloseReason::AccessDenied
    );
    let mut waiting = Browser::connect_as(&service.context, Some(identity)).await;

    // The lock screen asks only for the password of the locked user.
    assert!(matches!(
        locked.login(3, "unused", "wrong").await,
        LoginEvent::Failure { .. }
    ));
    assert!(service.context.presence.is_locked(uid));
    let LoginEvent::Success { user, .. } = locked.login(5, "unused", "secret").await else {
        panic!("the unlock failed");
    };
    assert_eq!(user, name);
    assert!(!service.context.presence.is_locked(uid));
    assert!(!locked_hint(&mut agent).await);
    // The other tab on the lock screen connects again. The tab of the login keeps its connection.
    connection_message(&mut waiting, ConnectionMessage::Unlocked).await;
    locked.send(Frame::open(ch(7), &Open::Echo).unwrap()).await;
    let ready = loop {
        let frame = tokio::time::timeout(WAIT, agent.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if frame.frame_type() == FrameType::Open {
            break frame;
        }
    };
    assert_eq!(ready.parse::<Open>().unwrap(), Open::Echo);
}

#[tokio::test]
async fn idle_lock_and_logind_lock_the_session_and_logind_unlocks_it() {
    let (service, name, uid, mut agent) = lock_service().await;
    let mut browser = Browser::connect(&service.context).await;
    browser.login(1, &name, "secret").await;
    browser
        .send(Frame::connection(&ConnectionMessage::Activity).unwrap())
        .await;
    farglassd::server::lock_idle(&service.context, Duration::from_secs(3600));
    assert!(
        !service.context.presence.is_locked(uid),
        "a user with activity is not idle"
    );
    farglassd::server::lock_idle(&service.context, Duration::ZERO);
    connection_message(&mut browser, ConnectionMessage::Locked).await;
    assert!(locked_hint(&mut agent).await);

    // loginctl unlock-session, then loginctl lock-session.
    agent
        .send(
            Frame::json(
                ChannelId::CONNECTION,
                FrameType::Control,
                &LinkMessage::Unlock,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(!locked_hint(&mut agent).await);
    assert!(!service.context.presence.is_locked(uid));
    let mut browser = Browser::connect_as(&service.context, Some(Identity { uid, name })).await;
    assert!(browser.hello_user.is_some());
    agent
        .send(
            Frame::json(
                ChannelId::CONNECTION,
                FrameType::Control,
                &LinkMessage::Lock,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    connection_message(&mut browser, ConnectionMessage::Locked).await;
    assert!(service.context.presence.is_locked(uid));
}

/// The queues and the task of a connection from [`raw_connection`].
type Raw = (
    mpsc::Sender<Result<Frame, Violation>>,
    mpsc::Receiver<Frame>,
    JoinHandle<Result<(), Violation>>,
);

/// Starts a connection with no hello from the browser. Returns the queues and the task.
fn raw_connection(context: &Arc<Context>, user: Option<Identity>) -> Raw {
    let (to_server, server_in) = mpsc::channel(16);
    let (server_out, from_server) = mpsc::channel(16);
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
        let client = connection::Client {
            addr: Some("2001:db8:5:6::1".parse().unwrap()),
            user,
            ..connection::Client::default()
        };
        connection::run(&context, client, &mut incoming, &mut outgoing).await
    });
    (to_server, from_server, task)
}

/// Starts a connection with `client` and does the hello exchange. Returns the queues and the task.
async fn hello_connection(context: &Arc<Context>, client: connection::Client) -> Raw {
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
        connection::run(&context, client, &mut incoming, &mut outgoing).await
    });
    recv_frame(&mut from_server).await;
    let hello = ConnectionMessage::Hello {
        version: PROTOCOL_VERSION,
        user: None,
        locked: None,
        pin: None,
    };
    to_server
        .send(Ok(Frame::connection(&hello).unwrap()))
        .await
        .unwrap();
    (to_server, from_server, task)
}

fn limited(service: &LoginService, adjust: impl FnOnce(&mut Context)) -> Arc<Context> {
    let mut context = Context::new(
        service.registry.clone(),
        service.context.login_socket.clone(),
    );
    adjust(&mut context);
    Arc::new(context)
}

#[tokio::test]
async fn a_connection_with_no_hello_closes_after_the_hello_time() {
    let service = LoginService::start().await;
    let context = limited(&service, |c| c.hello_timeout = Duration::from_millis(200));
    let (_to_server, mut from_server, task) = raw_connection(&context, None);
    assert!(from_server.recv().await.unwrap().channel().is_connection());
    let result = tokio::time::timeout(WAIT, task).await.unwrap().unwrap();
    assert!(result.is_ok());
}

#[tokio::test]
async fn an_address_gets_a_limited_number_of_connections_with_no_user() {
    let service = LoginService::start().await;
    let context = limited(&service, |c| c.max_anonymous = 1);
    let (_first_to, mut first_from, _first) = raw_connection(&context, None);
    assert!(
        first_from.recv().await.is_some(),
        "the first connection gets the hello"
    );
    // The same /64 network has no place: the connection closes with no hello.
    let (_second_to, mut second_from, second) = raw_connection(&context, None);
    assert!(second_from.recv().await.is_none());
    assert!(second.await.unwrap().is_ok());
    // A connection with a user from a cookie does not need a place.
    let (_user_to, mut user_from, _user) = raw_connection(
        &context,
        Some(Identity {
            uid: 4242,
            name: "alice".to_owned(),
        }),
    );
    assert!(user_from.recv().await.is_some());
}

#[tokio::test]
async fn a_connection_with_no_user_closes_after_the_login_idle_time() {
    let service = LoginService::start().await;
    let context = limited(&service, |c| c.login_idle = Duration::from_millis(300));
    let mut browser = Browser::connect_as(&context, None).await;
    let closed = tokio::time::timeout(WAIT, browser.from_server.recv())
        .await
        .unwrap();
    assert!(closed.is_none(), "the idle connection did not close");
}

#[tokio::test]
async fn a_session_cookie_of_a_user_outside_the_group_gives_the_login_screen() {
    let (name, uid) = current_user();
    let identity = Identity { uid, name };
    let primary = nix::unistd::Group::from_gid(nix::unistd::getgid())
        .unwrap()
        .unwrap()
        .name;
    let member = LoginService::start_full(
        Arc::new(farglass_login::host::NoSessions),
        Duration::ZERO,
        Some(primary),
    )
    .await;
    let context = limited(&member, |c| c.check_membership = true);
    let browser = Browser::connect_as(&context, Some(identity.clone())).await;
    assert_eq!(browser.hello_user.as_deref(), Some(identity.name.as_str()));

    let outside = LoginService::start_full(
        Arc::new(farglass_login::host::NoSessions),
        Duration::ZERO,
        Some("root".to_owned()),
    )
    .await;
    let context = limited(&outside, |c| c.check_membership = true);
    let browser = Browser::connect_as(&context, Some(identity.clone())).await;
    assert_eq!(
        browser.hello_user, None,
        "a user outside the group kept the session"
    );

    // No farglass-login: the check fails closed.
    let context = Arc::new(Context {
        check_membership: true,
        ..Context::new(Registry::new(), "/nonexistent/login.sock".into())
    });
    let browser = Browser::connect_as(&context, Some(identity)).await;
    assert_eq!(browser.hello_user, None, "a fault kept the session");
}

#[tokio::test]
async fn the_machine_pin_comes_before_each_login_channel() {
    let service = LoginService::start().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pin");
    farglassd::pin::write_pin(&path, "4829").unwrap();
    let context = limited(&service, |c| {
        c.pin = Arc::new(farglassd::pin::PinStore::new(Some(path.clone())));
    });
    let (to_server, mut from_server, _task) = raw_connection(&context, None);
    let ConnectionMessage::Hello { pin, .. } = recv_frame(&mut from_server).await.parse().unwrap()
    else {
        panic!("no hello");
    };
    assert_eq!(pin, Some(true));
    let hello = ConnectionMessage::Hello {
        version: PROTOCOL_VERSION,
        user: None,
        locked: None,
        pin: None,
    };
    to_server
        .send(Ok(Frame::connection(&hello).unwrap()))
        .await
        .unwrap();

    // No PIN: the login channel is refused.
    to_server
        .send(Ok(Frame::open(ch(1), &Open::Login).unwrap()))
        .await
        .unwrap();
    let close = recv_frame(&mut from_server).await;
    assert_eq!(close.channel(), ch(1));
    assert_eq!(
        close.parse::<Close>().unwrap().reason,
        CloseReason::AccessDenied
    );

    let pin = |text: &str| {
        Ok(Frame::connection(&ConnectionMessage::Pin {
            pin: text.to_owned(),
        })
        .unwrap())
    };
    to_server.send(pin("0000")).await.unwrap();
    assert_eq!(
        recv_frame(&mut from_server)
            .await
            .parse::<ConnectionMessage>()
            .unwrap(),
        ConnectionMessage::PinResult {
            accepted: false,
            retry_after: None
        }
    );
    to_server.send(pin("4829")).await.unwrap();
    assert_eq!(
        recv_frame(&mut from_server)
            .await
            .parse::<ConnectionMessage>()
            .unwrap(),
        ConnectionMessage::PinResult {
            accepted: true,
            retry_after: None
        }
    );
    to_server
        .send(Ok(Frame::open(ch(3), &Open::Login).unwrap()))
        .await
        .unwrap();
    assert_eq!(
        recv_frame(&mut from_server).await.frame_type(),
        FrameType::Ready
    );
}

#[tokio::test]
async fn a_cookie_user_needs_no_pin_and_a_damaged_pin_file_refuses_each_pin() {
    let service = LoginService::start().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pin");
    std::fs::write(&path, "damaged").unwrap();
    let context = limited(&service, |c| {
        c.pin = Arc::new(farglassd::pin::PinStore::new(Some(path.clone())));
    });
    let user = Browser::connect_as(
        &context,
        Some(Identity {
            uid: 4242,
            name: "alice".to_owned(),
        }),
    )
    .await;
    assert_eq!(user.hello_user.as_deref(), Some("alice"));

    let (to_server, mut from_server, _task) = raw_connection(&context, None);
    let hello = from_server.recv().await.unwrap();
    let ConnectionMessage::Hello { pin, .. } = hello.parse().unwrap() else {
        panic!("no hello");
    };
    assert_eq!(pin, Some(true), "a damaged file must close the gate");
    let ours = ConnectionMessage::Hello {
        version: PROTOCOL_VERSION,
        user: None,
        locked: None,
        pin: None,
    };
    to_server
        .send(Ok(Frame::connection(&ours).unwrap()))
        .await
        .unwrap();
    to_server
        .send(Ok(Frame::connection(&ConnectionMessage::Pin {
            pin: "1234".to_owned(),
        })
        .unwrap()))
        .await
        .unwrap();
    let answer = tokio::time::timeout(WAIT, from_server.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        answer.parse::<ConnectionMessage>().unwrap(),
        ConnectionMessage::PinResult {
            accepted: false,
            retry_after: None
        }
    );
}

async fn recv_frame(from: &mut mpsc::Receiver<Frame>) -> Frame {
    tokio::time::timeout(WAIT, from.recv())
        .await
        .unwrap()
        .unwrap()
}

fn alice() -> Identity {
    Identity {
        uid: 4242,
        name: "alice".to_owned(),
    }
}

#[tokio::test]
async fn a_connection_from_a_cookie_ends_at_the_end_of_the_cookie() {
    let service = LoginService::start().await;
    let context = limited(&service, |_| {});
    let client = connection::Client {
        user: Some(alice()),
        expires: Some(std::time::Instant::now() + Duration::from_millis(300)),
        ..connection::Client::default()
    };
    let (_to, mut from, task) = hello_connection(&context, client).await;
    let result = tokio::time::timeout(WAIT, task).await.unwrap().unwrap();
    assert!(result.is_ok());
    assert!(from.recv().await.is_none());
}

#[tokio::test]
async fn a_connection_that_misses_user_events_ends() {
    let service = LoginService::start().await;
    let context = limited(&service, |_| {});
    let client = connection::Client {
        user: Some(alice()),
        ..connection::Client::default()
    };
    let (_to, _from, task) = hello_connection(&context, client).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    // More events than the queue holds, with no await: the connection misses some.
    // One of them could be a lock or a logout of its user.
    for _ in 0..200 {
        context.presence.announce_logout(9999);
    }
    let result = tokio::time::timeout(WAIT, task).await;
    assert!(
        result.is_ok(),
        "the connection did not end after missed events"
    );
}

#[tokio::test]
async fn an_agent_that_stops_and_does_not_come_back_logs_its_user_out() {
    let service = LoginService::start().await;
    let context = limited(&service, |_| {});
    tokio::spawn(farglassd::server::agent_gone_loop(
        context.clone(),
        Duration::from_millis(200),
    ));
    let identity = Identity {
        uid: 4242,
        name: "alice".to_owned(),
    };
    let token = context
        .sessions
        .redeem(&context.sessions.ticket(identity.clone()))
        .unwrap();
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::COOKIE,
        format!("{}={token}", farglassd::cookie::COOKIE_NAME)
            .parse()
            .unwrap(),
    );
    let agent = fake_agent(&context.registry, 4242).await;
    let mut browser = Browser::connect_as(&context, Some(identity)).await;
    // The web session ends: the agent stops.
    drop(agent);
    logged_out(&mut browser).await;
    assert_eq!(
        context.sessions.user(&headers),
        None,
        "the cookie still gives the user"
    );
}
