//! Tests of X11 apps through Xwayland (F082) with a real X11 client.
//! They need the programs `Xwayland` and `xclock`, and do nothing on a host with no such program.

use std::process::{Command as Process, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use farglass_surface::{Command, Compositor, Event};

const WAIT: Duration = Duration::from_secs(20);

fn installed(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// Returns the first value that `pick` gives for an event of the compositor.
fn wait<T>(
    events: &mpsc::Receiver<Event>,
    what: &str,
    mut pick: impl FnMut(Event) -> Option<T>,
) -> T {
    let end = Instant::now() + WAIT;
    loop {
        let left = end.saturating_duration_since(Instant::now());
        let event = events
            .recv_timeout(left)
            .unwrap_or_else(|_| panic!("no event in time: {what}"));
        if let Some(value) = pick(event) {
            return value;
        }
    }
}

#[test]
fn an_x11_window_shows_takes_a_size_and_closes() {
    if !installed("Xwayland") || !installed("xclock") {
        eprintln!("skipped: the host has no Xwayland or no xclock");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (sender, events) = mpsc::channel();
    let compositor = Compositor::spawn_with(&dir.path().join("wayland"), true, move |event| {
        let _ = sender.send(event);
    })
    .unwrap();
    let display = compositor.x11_display().expect("Xwayland starts");

    let mut client = Process::new("xclock")
        .env("DISPLAY", format!(":{display}"))
        .env_remove("WAYLAND_DISPLAY")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let (id, pid) = wait(&events, "the new window", |event| match event {
        Event::ToplevelNew { id, pid } => Some((id, pid)),
        _ => None,
    });
    assert_eq!(pid, Some(client.id()), "the window names its process");
    let (title, app_id) = wait(&events, "the title", |event| match event {
        Event::ToplevelChanged {
            id: changed,
            title,
            app_id,
            ..
        } if changed == id => Some((title, app_id)),
        _ => None,
    });
    assert_eq!(title.as_deref(), Some("xclock"));
    assert_eq!(app_id.as_deref(), Some("XClock"));
    let first = wait(&events, "the first buffer", |event| match event {
        Event::Commit(commit) if commit.id == id => Some((commit.width, commit.height)),
        _ => None,
    });
    assert!(first.0 > 0 && first.1 > 0);

    // The shell gives the focus and a size.
    compositor.send(Command::Activate { id: Some(id) });
    compositor.send(Command::Configure {
        id,
        width: 300,
        height: 200,
    });
    wait(&events, "a buffer with the new size", |event| match event {
        Event::Commit(commit) if commit.id == id && (commit.width, commit.height) == (300, 200) => {
            Some(())
        }
        _ => None,
    });

    compositor.send(Command::Close { id });
    wait(&events, "the close of the window", |event| match event {
        Event::ToplevelClosed { id: closed } if closed == id => Some(()),
        _ => None,
    });
    // xclock stops after the close request.
    let end = Instant::now() + WAIT;
    while client.try_wait().unwrap().is_none() {
        assert!(Instant::now() < end, "xclock did not stop");
        std::thread::sleep(Duration::from_millis(50));
    }
}
