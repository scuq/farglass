//! Tests of farglass-login with a real PAM stack and no root.
//!
//! The stack uses pam_exec: it asks for the password and gives it to a script
//! that compares it with "secret". A second pam_exec line gives an information message.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use std::io;
use std::sync::{Arc, Mutex};

use farglass_login::client::Attempt;
use farglass_login::host::{Host, Started};
use farglass_login::server::{Service, Settings};
use farglass_login::{activation, limiter};
use farglass_proto::login::{LocalSession, LoginEvent};
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
        Attempt::start(
            &self.socket,
            user.map(str::to_owned),
            "192.0.2.10".to_owned(),
        )
        .await
        .unwrap()
    }

    /// Does one login with `password` and returns the events up to the result.
    async fn login(&self, user: &str, password: &str) -> Vec<LoginEvent> {
        let mut attempt = self.attempt(Some(user)).await;
        let mut events = Vec::new();
        loop {
            let event = tokio::time::timeout(WAIT, attempt.next())
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
        match tokio::time::timeout(WAIT, attempt.next())
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
    let harness = Harness::start(|settings| settings.max_attempts = 1).await;
    let user = current_user();

    let mut first = harness.attempt(Some(&user)).await;
    loop {
        if let LoginEvent::Prompt { .. } = tokio::time::timeout(WAIT, first.next())
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
        let event = tokio::time::timeout(WAIT, attempt.next())
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
        match tokio::time::timeout(WAIT, attempt.next())
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
