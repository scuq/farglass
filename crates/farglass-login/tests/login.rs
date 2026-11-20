//! Tests of farglass-login with a real PAM stack and no root.
//!
//! The stack uses pam_exec: it asks for the password and gives it to a script
//! that compares it with "secret". A second pam_exec line gives an information message.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use std::io;
use std::sync::{Arc, Mutex};

use farglass_login::client::{Attempt, ClientError};
use farglass_login::host::{Host, Started};
use farglass_login::server::{Service, Settings};
use farglass_login::{activation, limiter};
use farglass_proto::login::{LocalSession, LoginEvent, LoginRequest};
use tempfile::TempDir;
use zeroize::Zeroizing;

const WAIT: Duration = Duration::from_secs(10);

struct Harness {
    socket: PathBuf,
    _dir: TempDir,
}

fn current_user() -> String {
    nix::unistd::User::from_uid(nix::unistd::getuid())
        .unwrap()
        .unwrap()
        .name
}

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

impl Harness {
    async fn start(adjust: impl FnOnce(&mut Settings)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let pam_dir = dir.path().join("pam.d");
        std::fs::create_dir(&pam_dir).unwrap();
        let info = script(dir.path(), "info.sh", "echo 'Welcome to the test host.'");
        // pam_exec writes the password and a NUL byte to standard input.
        let check = script(
            dir.path(),
            "check.sh",
            r#"IFS= read -r password; [ "$password" = secret ]"#,
        );
        std::fs::write(
            pam_dir.join("farglass"),
            format!(
                "auth optional pam_exec.so stdout quiet {}\n\
                 auth required pam_exec.so expose_authtok quiet {}\n\
                 account required pam_permit.so\n",
                info.display(),
                check.display()
            ),
        )
        .unwrap();

        let mut settings = Settings {
            pam_dir: Some(pam_dir),
            fail_delay: Duration::ZERO,
            ..Settings::default()
        };
        adjust(&mut settings);
        let socket = dir.path().join("login.sock");
        let listener = activation::bind(&socket, 0o600).unwrap();
        tokio::spawn(Service::new(settings).run(listener));
        Self { socket, _dir: dir }
    }

    async fn attempt(&self, user: Option<&str>) -> Attempt {
        self.attempt_from(user, "192.0.2.10", false).await
    }

    async fn attempt_from(&self, user: Option<&str>, rhost: &str, device: bool) -> Attempt {
        Attempt::start(
            &self.socket,
            user.map(str::to_owned),
            rhost.to_owned(),
            device,
        )
        .await
        .unwrap()
    }

    /// Does one login with `password` and returns the events up to the result.
    async fn login(&self, user: &str, password: &str) -> Vec<LoginEvent> {
        let mut attempt = self.attempt(Some(user)).await;
        let mut events = Vec::new();
        loop {
            let event = tokio::time::timeout(WAIT, next_event(&mut attempt))
                .await
                .unwrap()
                .unwrap();
            if let LoginEvent::Prompt { prompt, .. } = &event {
                attempt
                    .answer(*prompt, Zeroizing::new(password.to_owned()))
                    .await
                    .unwrap();
            }
            let done = matches!(
                event,
                LoginEvent::Success { .. }
                    | LoginEvent::Failure { .. }
                    | LoginEvent::Blocked { .. }
            );
            events.push(event);
            if done {
                return events;
            }
        }
    }
}

#[tokio::test]
async fn correct_password_succeeds_with_the_prompt_and_the_information_message() {
    let harness = Harness::start(|_| {}).await;
    let user = current_user();
    let events = harness.login(&user, "secret").await;

    assert!(
        events.contains(&LoginEvent::Info {
            text: "Welcome to the test host.".to_owned()
        }),
        "{events:?}"
    );
    let prompt = events.iter().find_map(|event| match event {
        LoginEvent::Prompt { text, echo, .. } => Some((text.clone(), *echo)),
        _ => None,
    });
    let (text, echo) = prompt.expect("no prompt");
    assert!(text.to_lowercase().contains("password"), "{text}");
    assert!(!echo, "a password prompt must not show the answer");

    let uid = nix::unistd::getuid().as_raw();
    assert_eq!(
        events.last(),
        Some(&LoginEvent::Success {
            user,
            uid,
            ticket: None
        })
    );
}

#[tokio::test]
async fn wrong_password_fails_with_a_message_that_does_not_tell_the_cause() {
    let harness = Harness::start(|_| {}).await;
    let events = harness.login(&current_user(), "guess").await;
    assert_eq!(
        events.last(),
        Some(&LoginEvent::Failure {
            message: "The login failed.".to_owned()
        })
    );
}

#[tokio::test]
async fn pam_asks_for_the_username_when_farglassd_gives_none() {
    let harness = Harness::start(|_| {}).await;
    let user = current_user();
    let mut attempt = harness.attempt(None).await;
    let mut asked_for_user = false;
    loop {
        match tokio::time::timeout(WAIT, next_event(&mut attempt))
            .await
            .unwrap()
            .unwrap()
        {
            LoginEvent::Prompt {
                prompt, echo: true, ..
            } => {
                asked_for_user = true;
                attempt
                    .answer(prompt, Zeroizing::new(user.clone()))
                    .await
                    .unwrap();
            }
            LoginEvent::Prompt {
                prompt,
                echo: false,
                ..
            } => {
                attempt
                    .answer(prompt, Zeroizing::new("secret".to_owned()))
                    .await
                    .unwrap();
            }
            LoginEvent::Success { user: name, .. } => {
                assert_eq!(name, user);
                break;
            }
            LoginEvent::Info { .. } => {}
            other => panic!("unexpected event {other:?}"),
        }
    }
    assert!(asked_for_user);
}

#[tokio::test]
async fn root_cannot_log_in_by_default() {
    let harness = Harness::start(|_| {}).await;
    let events = harness.login("root", "secret").await;
    assert_eq!(
        events.last(),
        Some(&LoginEvent::Failure {
            message: "The login as root is not permitted.".to_owned()
        })
    );

    let allowed = Harness::start(|settings| settings.allow_root = true).await;
    let events = allowed.login("root", "secret").await;
    assert_eq!(
        events.last(),
        Some(&LoginEvent::Success {
            user: "root".to_owned(),
            uid: 0,
            ticket: None
        })
    );
}

#[tokio::test]
async fn unknown_user_fails_as_a_wrong_password_does() {
    let harness = Harness::start(|_| {}).await;
    // pam_exec accepts each username. The account lookup after PAM refuses this one.
    let events = harness.login("no-such-user-farglass", "secret").await;
    assert_eq!(
        events.last(),
        Some(&LoginEvent::Failure {
            message: "The login failed.".to_owned()
        })
    );
}

#[tokio::test]
async fn repeated_failures_block_the_address_and_the_user() {
    let harness = Harness::start(|_| {}).await;
    let user = current_user();
    for _ in 0..limiter::FREE_FAILURES {
        harness.login(&user, "guess").await;
    }
    // The correct password does not help during the block.
    let events = harness.login(&user, "secret").await;
    let [LoginEvent::Blocked { retry_after }] = events.as_slice() else {
        panic!("not blocked: {events:?}");
    };
    assert!((1..=limiter::FIRST_BLOCK.as_secs()).contains(retry_after));
}

#[tokio::test]
async fn closed_connection_stops_the_attempt_and_frees_its_place() {
    let harness = Harness::start(|settings| {
        settings.max_attempts = 1;
        settings.reserved_attempts = 0;
    })
    .await;
    let user = current_user();

    let mut first = harness.attempt(Some(&user)).await;
    loop {
        if let LoginEvent::Prompt { .. } = tokio::time::timeout(WAIT, next_event(&mut first))
            .await
            .unwrap()
            .unwrap()
        {
            break;
        }
    }
    // A second attempt at the same time finds no free place.
    let events = harness.login(&user, "secret").await;
    assert!(
        matches!(events.as_slice(), [LoginEvent::Failure { message }] if message.contains("too many"))
    );

    drop(first);
    let result = tokio::time::timeout(WAIT, async {
        loop {
            let events = harness.login(&user, "secret").await;
            if matches!(events.last(), Some(LoginEvent::Success { .. })) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        result.is_ok(),
        "the place of the stopped attempt did not become free"
    );
}

/// A host that the test controls. It records each action.
#[derive(Debug, Default)]
struct FakeHost {
    local: Vec<LocalSession>,
    start_fails: bool,
    started_before: bool,
    actions: Mutex<Vec<String>>,
}

impl FakeHost {
    fn actions(&self) -> Vec<String> {
        self.actions.lock().unwrap().clone()
    }
}

impl Host for FakeHost {
    fn local_sessions(&self, uid: u32) -> io::Result<Vec<LocalSession>> {
        self.actions.lock().unwrap().push(format!("list {uid}"));
        Ok(self.local.clone())
    }

    fn end_sessions(&self, ids: &[String]) -> io::Result<()> {
        self.actions
            .lock()
            .unwrap()
            .push(format!("end {}", ids.join(",")));
        Ok(())
    }

    fn end_web_sessions(&self, uid: u32) -> io::Result<u32> {
        self.actions.lock().unwrap().push(format!("logout {uid}"));
        Ok(1)
    }

    fn start_session(&self, user: &str, uid: u32) -> io::Result<Started> {
        self.actions
            .lock()
            .unwrap()
            .push(format!("start {user} {uid}"));
        if self.start_fails {
            return Err(io::Error::other("systemd-run failed"));
        }
        Ok(if self.started_before {
            Started::Attached
        } else {
            Started::New
        })
    }
}

/// Returns the next event of the attempt. It answers [`LoginEvent::Verified`] as farglassd does
/// for a user with no passkey, thus the tests see the events of the shell only.
async fn next_event(attempt: &mut Attempt) -> Result<LoginEvent, ClientError> {
    loop {
        let event = attempt.next().await?;
        if matches!(event, LoginEvent::Verified { .. }) {
            attempt
                .request(LoginRequest::SecondFactor { accept: true })
                .await?;
            continue;
        }
        return Ok(event);
    }
}

fn gnome() -> LocalSession {
    LocalSession {
        id: "2".to_owned(),
        kind: "wayland".to_owned(),
        seat: Some("seat0".to_owned()),
    }
}

/// Logs in with the correct password, and answers a confirm event with `accept`.
async fn login_with_confirm(harness: &Harness, accept: bool) -> Vec<LoginEvent> {
    let user = current_user();
    let mut attempt = harness.attempt(Some(&user)).await;
    let mut events = Vec::new();
    loop {
        let event = tokio::time::timeout(WAIT, next_event(&mut attempt))
            .await
            .unwrap()
            .unwrap();
        match &event {
            LoginEvent::Prompt { prompt, .. } => {
                attempt
                    .answer(*prompt, Zeroizing::new("secret".to_owned()))
                    .await
                    .unwrap();
            }
            LoginEvent::Confirm { .. } => attempt.confirm(accept).await.unwrap(),
            _ => {}
        }
        let done = matches!(
            event,
            LoginEvent::Success { .. } | LoginEvent::Failure { .. }
        );
        events.push(event);
        if done {
            return events;
        }
    }
}

#[tokio::test]
async fn login_with_no_local_session_starts_the_web_session() {
    let host = Arc::new(FakeHost::default());
    let harness = Harness::start({
        let host = host.clone();
        |settings| settings.host = host
    })
    .await;
    let events = login_with_confirm(&harness, true).await;
    assert!(matches!(events.last(), Some(LoginEvent::Success { .. })));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, LoginEvent::Confirm { .. }))
    );
    let uid = nix::unistd::getuid().as_raw();
    assert_eq!(
        host.actions(),
        [
            format!("list {uid}"),
            format!("start {} {uid}", current_user())
        ]
    );
}

#[tokio::test]
async fn confirmed_login_ends_the_local_session_before_the_web_session_starts() {
    let host = Arc::new(FakeHost {
        local: vec![gnome()],
        ..FakeHost::default()
    });
    let harness = Harness::start({
        let host = host.clone();
        |settings| settings.host = host
    })
    .await;
    let events = login_with_confirm(&harness, true).await;
    assert!(
        events.contains(&LoginEvent::Confirm {
            sessions: vec![gnome()]
        }),
        "{events:?}"
    );
    assert!(matches!(events.last(), Some(LoginEvent::Success { .. })));
    let actions = host.actions();
    assert_eq!(actions[1], "end 2");
    assert!(actions[2].starts_with("start "));
}

#[tokio::test]
async fn declined_login_keeps_the_local_session_and_starts_nothing() {
    let host = Arc::new(FakeHost {
        local: vec![gnome()],
        ..FakeHost::default()
    });
    let harness = Harness::start({
        let host = host.clone();
        |settings| settings.host = host
    })
    .await;
    let events = login_with_confirm(&harness, false).await;
    assert_eq!(
        events.last(),
        Some(&LoginEvent::Failure {
            message: "The login stopped. The local session continues.".to_owned()
        })
    );
    assert_eq!(host.actions().len(), 1, "{:?}", host.actions());

    // A decline is not a wrong password: it does not count for the block.
    for _ in 0..limiter::FREE_FAILURES + 1 {
        login_with_confirm(&harness, false).await;
    }
    let events = login_with_confirm(&harness, false).await;
    assert!(!matches!(events.last(), Some(LoginEvent::Blocked { .. })));
}

#[tokio::test]
async fn closed_connection_at_the_confirm_ends_no_session() {
    let host = Arc::new(FakeHost {
        local: vec![gnome()],
        ..FakeHost::default()
    });
    let harness = Harness::start({
        let host = host.clone();
        |settings| settings.host = host
    })
    .await;
    let mut attempt = harness.attempt(Some(&current_user())).await;
    loop {
        match tokio::time::timeout(WAIT, next_event(&mut attempt))
            .await
            .unwrap()
            .unwrap()
        {
            LoginEvent::Prompt { prompt, .. } => {
                attempt
                    .answer(prompt, Zeroizing::new("secret".to_owned()))
                    .await
                    .unwrap();
            }
            LoginEvent::Confirm { .. } => break,
            _ => {}
        }
    }
    drop(attempt);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(host.actions().len(), 1, "{:?}", host.actions());
}

#[tokio::test]
async fn failed_session_start_gives_a_message_for_the_administrator() {
    let host = Arc::new(FakeHost {
        start_fails: true,
        ..FakeHost::default()
    });
    let harness = Harness::start({
        let host = host.clone();
        |settings| settings.host = host
    })
    .await;
    let events = login_with_confirm(&harness, true).await;
    let Some(LoginEvent::Failure { message }) = events.last() else {
        panic!("{events:?}");
    };
    assert!(message.contains("session did not start"), "{message}");
}

#[tokio::test]
async fn logout_ends_the_web_sessions_of_the_user() {
    let host = Arc::new(FakeHost::default());
    let harness = Harness::start({
        let host = host.clone();
        |settings| settings.host = host
    })
    .await;
    let ended = farglass_login::client::logout(&harness.socket, 4242)
        .await
        .unwrap();
    assert_eq!(ended, 1);
    assert_eq!(host.actions(), ["logout 4242"]);
}

#[tokio::test]
async fn a_refused_second_factor_fails_and_starts_no_session() {
    let host = Arc::new(FakeHost::default());
    let harness = Harness::start({
        let host = host.clone();
        |settings| settings.host = host
    })
    .await;
    let mut attempt = harness.attempt(Some(&current_user())).await;
    let uid = nix::unistd::getuid().as_raw();
    let result = loop {
        let event = tokio::time::timeout(WAIT, attempt.next())
            .await
            .unwrap()
            .unwrap();
        match event {
            LoginEvent::Prompt { prompt, .. } => attempt
                .answer(prompt, Zeroizing::new("secret".to_owned()))
                .await
                .unwrap(),
            LoginEvent::Verified { uid: verified, .. } => {
                assert_eq!(verified, uid);
                attempt
                    .request(LoginRequest::SecondFactor { accept: false })
                    .await
                    .unwrap();
            }
            LoginEvent::Info { .. } => {}
            other => break other,
        }
    };
    assert!(matches!(result, LoginEvent::Failure { .. }), "{result:?}");
    assert!(host.actions().is_empty(), "{:?}", host.actions());
}

/// Waits for the first prompt of `attempt` and returns its number and its echo flag.
async fn first_prompt(attempt: &mut Attempt) -> (u32, bool) {
    loop {
        match tokio::time::timeout(WAIT, next_event(attempt))
            .await
            .unwrap()
            .unwrap()
        {
            LoginEvent::Prompt { prompt, echo, .. } => return (prompt, echo),
            LoginEvent::Info { .. } => {}
            other => panic!("unexpected event {other:?}"),
        }
    }
}

/// Returns the result event of `attempt`, after the prompts.
async fn result(attempt: &mut Attempt) -> LoginEvent {
    loop {
        let event = tokio::time::timeout(WAIT, next_event(attempt))
            .await
            .unwrap()
            .unwrap();
        if !matches!(event, LoginEvent::Info { .. } | LoginEvent::Prompt { .. }) {
            return event;
        }
    }
}

#[tokio::test]
async fn one_address_gets_only_its_places_and_an_ipv6_network_is_one_address() {
    let harness = Harness::start(|settings| settings.max_per_address = 1).await;
    let user = current_user();
    let mut first = harness
        .attempt_from(Some(&user), "2001:db8:1:2::10", false)
        .await;
    first_prompt(&mut first).await;
    // The same /64 network has no free place.
    let mut second = harness
        .attempt_from(Some(&user), "2001:db8:1:2::99", false)
        .await;
    assert!(
        matches!(result(&mut second).await, LoginEvent::Failure { message } if message.contains("from your address"))
    );
    // A different address has its own place.
    let mut other = harness
        .attempt_from(Some(&user), "203.0.113.7", false)
        .await;
    first_prompt(&mut other).await;
}

#[tokio::test]
async fn a_browser_with_a_device_cookie_gets_a_reserved_place() {
    let harness = Harness::start(|settings| {
        settings.max_attempts = 2;
        settings.reserved_attempts = 1;
    })
    .await;
    let user = current_user();
    let mut holder = harness
        .attempt_from(Some(&user), "203.0.113.1", false)
        .await;
    first_prompt(&mut holder).await;
    // The open place is taken: a browser with no device cookie gets no place.
    let mut stranger = harness
        .attempt_from(Some(&user), "203.0.113.2", false)
        .await;
    assert!(
        matches!(result(&mut stranger).await, LoginEvent::Failure { message } if message.contains("too many logins"))
    );
    // A known browser uses the reserved place.
    let mut known = harness.attempt_from(Some(&user), "203.0.113.3", true).await;
    let (prompt, _) = first_prompt(&mut known).await;
    known
        .answer(prompt, Zeroizing::new("secret".to_owned()))
        .await
        .unwrap();
    assert!(matches!(
        result(&mut known).await,
        LoginEvent::Success { .. }
    ));
}

#[tokio::test]
async fn a_login_that_passes_the_deadline_fails_and_counts() {
    let harness =
        Harness::start(|settings| settings.login_deadline = Duration::from_millis(300)).await;
    let user = current_user();
    for _ in 0..limiter::FREE_FAILURES {
        let mut attempt = harness.attempt(Some(&user)).await;
        first_prompt(&mut attempt).await;
        // No answer: the deadline ends the login.
        assert!(
            matches!(result(&mut attempt).await, LoginEvent::Failure { message } if message.contains("too long"))
        );
    }
    let events = harness.login(&user, "secret").await;
    assert!(
        matches!(events.as_slice(), [LoginEvent::Blocked { .. }]),
        "{events:?}"
    );
}

#[tokio::test]
async fn an_abandoned_attempt_with_an_answer_counts_and_one_with_no_answer_does_not() {
    let harness = Harness::start(|_| {}).await;
    let user = current_user();
    // No answer: a reload of the login screen does not count.
    for _ in 0..limiter::FREE_FAILURES + 1 {
        let mut attempt = harness.attempt(None).await;
        first_prompt(&mut attempt).await;
        drop(attempt);
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut attempt = harness.attempt(None).await;
    first_prompt(&mut attempt).await;
    drop(attempt);

    // The username, then the attempt stops: it counts.
    for _ in 0..limiter::FREE_FAILURES {
        let mut attempt = harness.attempt(None).await;
        let (prompt, echo) = first_prompt(&mut attempt).await;
        assert!(echo);
        attempt
            .answer(prompt, Zeroizing::new(user.clone()))
            .await
            .unwrap();
        first_prompt(&mut attempt).await;
        drop(attempt);
    }
    let result = tokio::time::timeout(WAIT, async {
        loop {
            let events = harness.login(&user, "secret").await;
            if matches!(events.as_slice(), [LoginEvent::Blocked { .. }]) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(result.is_ok(), "the abandoned attempts did not block");
}

#[tokio::test]
async fn a_blocked_user_gets_no_password_prompt_from_a_new_address() {
    let harness = Harness::start(|_| {}).await;
    let user = current_user();
    // Wrong passwords from many addresses block the user, not one address.
    for i in 0..limiter::FREE_FAILURES {
        let mut attempt = harness
            .attempt_from(Some(&user), &format!("203.0.113.{}", i + 10), false)
            .await;
        let (prompt, _) = first_prompt(&mut attempt).await;
        attempt
            .answer(prompt, Zeroizing::new("wrong".to_owned()))
            .await
            .unwrap();
        assert!(matches!(
            result(&mut attempt).await,
            LoginEvent::Failure { .. }
        ));
    }
    // The login screen gives no username: PAM asks for it, and the block stops the password prompt.
    let mut attempt = harness.attempt_from(None, "198.51.100.1", false).await;
    let (prompt, echo) = first_prompt(&mut attempt).await;
    assert!(echo);
    attempt
        .answer(prompt, Zeroizing::new(user.clone()))
        .await
        .unwrap();
    let event = result(&mut attempt).await;
    assert!(matches!(event, LoginEvent::Blocked { .. }), "{event:?}");
}

/// Returns the name of the primary group of the current user.
fn primary_group() -> String {
    let gid = nix::unistd::getgid();
    nix::unistd::Group::from_gid(gid).unwrap().unwrap().name
}

#[tokio::test]
async fn a_member_of_the_allowed_group_logs_in() {
    let group = primary_group();
    let harness = Harness::start(|settings| settings.allowed_group = Some(group)).await;
    let events = harness.login(&current_user(), "secret").await;
    assert!(
        matches!(events.last(), Some(LoginEvent::Success { .. })),
        "{events:?}"
    );
}

#[tokio::test]
async fn a_user_outside_the_group_fails_as_a_wrong_password_does_and_gets_no_passkey_step() {
    let user = current_user();
    let wrong = Harness::start(|_| {}).await.login(&user, "wrong").await;
    // The user of the tests is not in the group root.
    let harness = Harness::start(|settings| settings.allowed_group = Some("root".to_owned())).await;
    let mut attempt = harness.attempt(Some(&user)).await;
    let mut events = Vec::new();
    loop {
        // No helper: a Verified event here would show the passkey step.
        let event = tokio::time::timeout(WAIT, attempt.next())
            .await
            .unwrap()
            .unwrap();
        if let LoginEvent::Prompt { prompt, .. } = &event {
            attempt
                .answer(*prompt, Zeroizing::new("secret".to_owned()))
                .await
                .unwrap();
        }
        assert!(
            !matches!(event, LoginEvent::Verified { .. }),
            "the passkey step showed"
        );
        let done = matches!(event, LoginEvent::Failure { .. });
        events.push(event);
        if done {
            break;
        }
    }
    assert_eq!(
        events.last(),
        wrong.last(),
        "the text differs from a wrong password"
    );
    // The refusal counts as a failure.
    for _ in 0..limiter::FREE_FAILURES {
        harness.login(&user, "secret").await;
    }
    let events = harness.login(&user, "secret").await;
    assert!(
        matches!(events.as_slice(), [LoginEvent::Blocked { .. }]),
        "{events:?}"
    );
}

#[tokio::test]
async fn a_missing_group_refuses_each_login() {
    let harness = Harness::start(|settings| {
        settings.allowed_group = Some("farglass-no-such-group".to_owned());
    })
    .await;
    let events = harness.login(&current_user(), "secret").await;
    assert_eq!(
        events.last(),
        Some(&LoginEvent::Failure {
            message: "The login failed.".to_owned()
        })
    );
}

#[tokio::test]
async fn the_member_question_follows_the_group() {
    let uid = nix::unistd::getuid().as_raw();
    let group = primary_group();
    let member = Harness::start(|settings| settings.allowed_group = Some(group)).await;
    assert!(
        farglass_login::client::member(&member.socket, uid)
            .await
            .unwrap()
    );
    let other = Harness::start(|settings| settings.allowed_group = Some("root".to_owned())).await;
    assert!(
        !farglass_login::client::member(&other.socket, uid)
            .await
            .unwrap()
    );
    let missing = Harness::start(|settings| {
        settings.allowed_group = Some("farglass-no-such-group".to_owned());
    })
    .await;
    assert!(
        !farglass_login::client::member(&missing.socket, uid)
            .await
            .unwrap()
    );
    let open = Harness::start(|_| {}).await;
    assert!(
        farglass_login::client::member(&open.socket, 4_000_000)
            .await
            .unwrap()
    );
}

#[test]
fn membership_counts_a_supplementary_group() {
    let user = nix::unistd::User::from_uid(nix::unistd::getuid())
        .unwrap()
        .unwrap();
    let name = std::ffi::CString::new(user.name.clone()).unwrap();
    let groups = nix::unistd::getgrouplist(&name, user.gid).unwrap();
    if let Some(extra) = groups.iter().find(|gid| **gid != user.gid) {
        let extra = nix::unistd::Group::from_gid(*extra).unwrap().unwrap().name;
        assert_eq!(
            farglass_login::server::membership(&extra, &user.name, user.gid),
            Ok(true)
        );
    }
    assert!(
        farglass_login::server::membership("farglass-no-such-group", &user.name, user.gid).is_err()
    );
}

#[tokio::test]
async fn a_password_check_starts_no_session_and_counts_failures() {
    let host = Arc::new(FakeHost::default());
    let fake = host.clone();
    let harness = Harness::start(move |settings| settings.host = fake).await;
    let user = current_user();
    let check = |password: &str| {
        let password = Zeroizing::new(password.to_owned());
        let socket = harness.socket.clone();
        let user = user.clone();
        async move {
            farglass_login::client::check_password(&socket, &user, "192.0.2.20", &password)
                .await
                .unwrap()
        }
    };
    assert!(check("secret").await);
    assert!(
        host.actions.lock().unwrap().is_empty(),
        "a check started a session"
    );
    for _ in 0..limiter::FREE_FAILURES {
        assert!(!check("wrong").await);
    }
    assert!(
        !check("secret").await,
        "the block of the user holds for a check too"
    );
}
