//! Tests of the polkit agent with a fake helper. The fake helper speaks the
//! protocol of polkit-agent-helper-1 and accepts the password "secret".

use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use farglass_proto::codec::FrameCodec;
use farglass_proto::message::{LinkMessage, Open, PolkitEvent, PolkitReply};
use farglass_proto::{ChannelId, Frame, FrameType, PROTOCOL_VERSION};
use farglass_session::link::Session;
use farglass_session::polkit::{Outcome, Polkit};
use futures_util::{SinkExt as _, StreamExt as _};
use tempfile::TempDir;
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio_util::codec::Framed;
use zeroize::Zeroizing;

const WAIT: Duration = Duration::from_secs(10);
const COOKIE: &str = "cookie-42";

/// Writes the fake helper. It checks the cookie, gives information and a password prompt, and checks the answer.
fn fake_helper() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("polkit-agent-helper-1");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\nIFS= read -r cookie\n[ \"$cookie\" = {COOKIE} ] || {{ echo FAILURE; exit 1; }}\n\
             echo \"PAM_TEXT_INFO Authenticating as $1\"\necho 'PAM_PROMPT_ECHO_OFF Password: '\n\
             IFS= read -r answer\nif [ \"$answer\" = secret ]; then echo SUCCESS; else echo FAILURE; fi\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, path)
}

fn me() -> String {
    nix::unistd::User::from_uid(nix::unistd::getuid())
        .unwrap()
        .unwrap()
        .name
}

/// A shell side that records the events of one polkit channel.
fn viewer(
    polkit: &Arc<Polkit>,
) -> (
    farglass_session::polkit::ViewerGuard,
    mpsc::UnboundedReceiver<PolkitEvent>,
) {
    let (send, events) = mpsc::unbounded_channel();
    let guard = polkit.add_viewer(move |event| {
        let _ = send.send(event);
    });
    (guard, events)
}

async fn next(events: &mut mpsc::UnboundedReceiver<PolkitEvent>) -> PolkitEvent {
    tokio::time::timeout(WAIT, events.recv())
        .await
        .expect("no event")
        .unwrap()
}

/// Starts a request with the identities of this user and root.
fn request(polkit: &Arc<Polkit>) -> tokio::task::JoinHandle<Result<(), Outcome>> {
    let polkit = polkit.clone();
    tokio::spawn(async move {
        polkit
            .authenticate(
                "org.example.action",
                "Authentication is necessary.",
                vec![me(), "root".to_owned()],
                COOKIE,
            )
            .await
    })
}

async fn until_prompt(events: &mut mpsc::UnboundedReceiver<PolkitEvent>) -> u32 {
    loop {
        if let PolkitEvent::Prompt {
            request,
            echo: false,
            ..
        } = next(events).await
        {
            return request;
        }
    }
}

fn answer(polkit: &Polkit, request: u32, text: &str) {
    polkit.reply(PolkitReply::Answer {
        request,
        text: Zeroizing::new(text.to_owned()),
    });
}

#[tokio::test]
async fn correct_password_succeeds_and_the_shell_sees_the_conversation() {
    let (_dir, helper) = fake_helper();
    let polkit = Polkit::new(helper);
    let (_guard, mut events) = viewer(&polkit);
    let task = request(&polkit);

    let PolkitEvent::Begin {
        request,
        action,
        identities,
        identity,
        ..
    } = next(&mut events).await
    else {
        panic!()
    };
    assert_eq!(action, "org.example.action");
    assert_eq!(identities, [me(), "root".to_owned()]);
    assert_eq!(identity, me(), "the user of the agent comes first");
    assert_eq!(
        next(&mut events).await,
        PolkitEvent::Info {
            request,
            text: format!("Authenticating as {}", me())
        }
    );
    assert_eq!(
        next(&mut events).await,
        PolkitEvent::Prompt {
            request,
            text: "Password: ".to_owned(),
            echo: false
        }
    );
    answer(&polkit, request, "secret");
    assert_eq!(
        next(&mut events).await,
        PolkitEvent::Done {
            request,
            success: true
        }
    );
    assert_eq!(task.await.unwrap(), Ok(()));
}

#[tokio::test]
async fn wrong_password_gives_a_failure_and_a_new_attempt_can_follow() {
    let (_dir, helper) = fake_helper();
    let polkit = Polkit::new(helper);
    let (_guard, mut events) = viewer(&polkit);
    let task = request(&polkit);
    let request = until_prompt(&mut events).await;
    answer(&polkit, request, "guess");
    assert_eq!(next(&mut events).await, PolkitEvent::Failed { request });

    polkit.reply(PolkitReply::Start {
        request,
        identity: me(),
    });
    until_prompt(&mut events).await;
    answer(&polkit, request, "secret");
    assert_eq!(task.await.unwrap(), Ok(()));
}

#[tokio::test]
async fn cancel_by_the_user_or_by_polkit_ends_the_request() {
    let (_dir, helper) = fake_helper();
    let polkit = Polkit::new(helper);
    let (_guard, mut events) = viewer(&polkit);

    let task = request(&polkit);
    let request_id = until_prompt(&mut events).await;
    polkit.reply(PolkitReply::Cancel {
        request: request_id,
    });
    assert_eq!(
        next(&mut events).await,
        PolkitEvent::Done {
            request: request_id,
            success: false
        }
    );
    assert!(matches!(task.await.unwrap(), Err(Outcome::Cancelled(_))));

    let task = request(&polkit);
    until_prompt(&mut events).await;
    polkit.cancel_cookie(COOKIE);
    assert!(matches!(task.await.unwrap(), Err(Outcome::Cancelled(_))));
}

#[tokio::test]
async fn request_with_no_client_or_after_the_last_client_closes_is_cancelled() {
    let (_dir, helper) = fake_helper();
    let polkit = Polkit::new(helper);
    let result = request(&polkit).await.unwrap();
    assert!(matches!(result, Err(Outcome::Cancelled(why)) if why.contains("no client")));

    let (guard, mut events) = viewer(&polkit);
    let task = request(&polkit);
    until_prompt(&mut events).await;
    drop(guard);
    assert!(
        matches!(task.await.unwrap(), Err(Outcome::Cancelled(why)) if why.contains("last client"))
    );
}

#[tokio::test]
async fn each_tab_gets_the_request_and_the_end() {
    let (_dir, helper) = fake_helper();
    let polkit = Polkit::new(helper);
    let (_first_guard, mut first) = viewer(&polkit);
    let (_second_guard, mut second) = viewer(&polkit);
    let task = request(&polkit);
    let request = until_prompt(&mut first).await;
    assert_eq!(until_prompt(&mut second).await, request);
    // The second tab answers. The first tab sees the end.
    answer(&polkit, request, "secret");
    assert_eq!(
        next(&mut first).await,
        PolkitEvent::Done {
            request,
            success: true
        }
    );
    assert_eq!(task.await.unwrap(), Ok(()));
}

#[tokio::test]
async fn polkit_channel_of_the_link_carries_the_request_and_the_answer() {
    let (_dir, helper) = fake_helper();
    let polkit = Polkit::new(helper);
    let agent_polkit = polkit.clone();
    let (ours, theirs) = UnixStream::pair().unwrap();
    tokio::spawn(async move {
        let mut session = Session {
            polkit: agent_polkit,
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
    link.send(Frame::open(ChannelId::new(1), &Open::Polkit).unwrap())
        .await
        .unwrap();
    assert_eq!(
        link.next().await.unwrap().unwrap().frame_type(),
        FrameType::Ready
    );

    let task = request(&polkit);
    let request = loop {
        let frame = tokio::time::timeout(WAIT, link.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Ok(PolkitEvent::Prompt { request, .. }) = frame.parse::<PolkitEvent>() {
            break request;
        }
    };
    let reply = PolkitReply::Answer {
        request,
        text: Zeroizing::new("secret".to_owned()),
    };
    link.send(Frame::json(ChannelId::new(1), FrameType::Control, &reply).unwrap())
        .await
        .unwrap();
    assert_eq!(task.await.unwrap(), Ok(()));
}
