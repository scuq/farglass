//! Runs the compositor on a socket and prints each event, for a look at a real client.
//!
//! Usage: cargo run -p farglass-surface --example serve -- SOCKET [DUMP_DIR]
//!
//! Start a client with `WAYLAND_DISPLAY=SOCKET`, for example
//! `WAYLAND_DISPLAY=/tmp/farglass-test GDK_BACKEND=wayland gnome-calculator`.
//! With DUMP_DIR, the last buffer of each surface goes to DUMP_DIR/ID.ppm.
//! The example stops after 30 s.

use std::io::Write as _;

use farglass_surface::{Command, Compositor, Event};

fn main() {
    let mut args = std::env::args().skip(1);
    let socket = std::path::PathBuf::from(args.next().expect("usage: serve SOCKET [DUMP_DIR]"));
    let dump = args.next().map(std::path::PathBuf::from);
    let (sender, events) = std::sync::mpsc::channel();
    let compositor = Compositor::spawn(&socket, move |event| {
        let _ = sender.send(event);
    })
    .expect("the compositor did not start");
    println!("listening on {}", compositor.socket().display());
    let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while let Ok(event) =
        events.recv_timeout(end.saturating_duration_since(std::time::Instant::now()))
    {
        match &event {
            Event::Commit(commit) => {
                println!(
                    "commit id={} {}x{} scale={} {:?} damage={:?} geometry={:?}",
                    commit.id,
                    commit.width,
                    commit.height,
                    commit.scale,
                    commit.format,
                    commit.damage,
                    commit.geometry
                );
                if let Some(dir) = &dump {
                    let mut ppm =
                        format!("P6\n{} {}\n255\n", commit.width, commit.height).into_bytes();
                    for pixel in commit.pixels.as_chunks::<4>().0 {
                        ppm.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
                    }
                    let _ = std::fs::File::create(dir.join(format!("{}.ppm", commit.id)))
                        .and_then(|mut file| file.write_all(&ppm));
                }
            }
            Event::ToplevelNew { id, .. } => {
                println!("{event:?}");
                compositor.send(Command::Activate { id: Some(*id) });
            }
            other => println!("{other:?}"),
        }
    }
}
