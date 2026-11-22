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

/// Starts a compositor with Xwayland and the X11 client `program` with `args` on its display.
/// With `pipe`, the output of the client goes to a pipe.
/// Returns the compositor, its events, the client, and the id of the client window after its first buffer.
fn start_client(
    dir: &std::path::Path,
    program: &str,
    args: &[&str],
    pipe: bool,
) -> (Compositor, mpsc::Receiver<Event>, std::process::Child, u32) {
    let (sender, events) = mpsc::channel();
    let compositor = Compositor::spawn_with(&dir.join("wayland"), true, move |event| {
        let _ = sender.send(event);
    })
    .unwrap();
    let display = compositor.x11_display().expect("Xwayland starts");
    let client = Process::new(program)
        .args(args)
        .env("DISPLAY", format!(":{display}"))
        .env_remove("WAYLAND_DISPLAY")
        .stdout(if pipe { Stdio::piped() } else { Stdio::null() })
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let id = wait(&events, "the new window", |event| match event {
        Event::ToplevelNew { id, .. } => Some(id),
        _ => None,
    });
    // A commit comes only after Xwayland gave the Wayland surface of the window: then keys can reach it.
    wait(&events, "the first buffer", |event| match event {
        Event::Commit(commit) if commit.id == id => Some(()),
        _ => None,
    });
    (compositor, events, client, id)
}

/// Linux key codes.
const KEY_H: u32 = 35;
const KEY_I: u32 = 23;
const KEY_ENTER: u32 = 28;
const KEY_LEFTSHIFT: u32 = 42;
/// The Linux code of the left pointer button.
const BTN_LEFT: u32 = 0x110;

#[test]
fn keys_of_the_shell_reach_an_x11_window() {
    if !installed("Xwayland") || !installed("xterm") {
        eprintln!("skipped: the host has no Xwayland or no xterm");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("typed");
    // The shell in xterm writes the first line that it reads to a file.
    let script = format!(
        "IFS= read -r line; printf %s \"$line\" > '{}'",
        out.display()
    );
    let (compositor, _events, mut client, id) =
        start_client(dir.path(), "xterm", &["-e", "sh", "-c", &script], false);

    compositor.send(Command::Activate { id: Some(id) });
    // Shift+H, I, Enter: the line "Hi", with a modifier.
    for (code, pressed) in [
        (KEY_LEFTSHIFT, true),
        (KEY_H, true),
        (KEY_H, false),
        (KEY_LEFTSHIFT, false),
        (KEY_I, true),
        (KEY_I, false),
        (KEY_ENTER, true),
        (KEY_ENTER, false),
    ] {
        compositor.send(Command::Key {
            id: Some(id),
            code,
            pressed,
        });
    }

    // The shell stops after the line, and xterm stops with it.
    let end = Instant::now() + WAIT;
    while client.try_wait().unwrap().is_none() {
        if Instant::now() > end {
            let _ = client.kill();
            panic!("xterm did not get the line");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "Hi");
}

#[test]
fn keys_and_the_pointer_of_the_shell_reach_an_x11_window() {
    if !installed("Xwayland") || !installed("xev") {
        eprintln!("skipped: the host has no Xwayland or no xev");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (compositor, _events, mut client, id) = start_client(
        dir.path(),
        "xev",
        &["-event", "keyboard", "-event", "mouse"],
        true,
    );
    // xev prints each event that it gets.
    let (lines_tx, lines) = mpsc::channel::<String>();
    let stdout = client.stdout.take().unwrap();
    std::thread::spawn(move || {
        for line in std::io::BufRead::lines(std::io::BufReader::new(stdout)) {
            let Ok(line) = line else { break };
            if lines_tx.send(line).is_err() {
                break;
            }
        }
    });
    let expect = |text: &str| {
        let end = Instant::now() + WAIT;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match lines.recv_timeout(left) {
                Ok(line) if line.contains(text) => return,
                Ok(_) => {}
                Err(_) => panic!("xev did not print {text}"),
            }
        }
    };

    compositor.send(Command::Activate { id: Some(id) });
    for (code, pressed) in [
        (KEY_LEFTSHIFT, true),
        (KEY_H, true),
        (KEY_H, false),
        (KEY_LEFTSHIFT, false),
        (KEY_I, true),
        (KEY_I, false),
    ] {
        compositor.send(Command::Key {
            id: Some(id),
            code,
            pressed,
        });
    }
    expect("Shift_L");
    expect("keysym 0x48, H)");
    expect("keysym 0x69, i)");

    // The window is at the origin of the X11 screen, thus the place in the window is the place on the screen.
    compositor.send(Command::PointerMotion {
        id,
        x: 50.0,
        y: 60.0,
    });
    expect("root:(50,60)");
    compositor.send(Command::PointerButton {
        button: BTN_LEFT,
        pressed: true,
    });
    compositor.send(Command::PointerButton {
        button: BTN_LEFT,
        pressed: false,
    });
    expect("ButtonPress event");
    expect("button 1,");

    let _ = client.kill();
    let _ = client.wait();
}
