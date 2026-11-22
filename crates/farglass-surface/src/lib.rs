//! farglass-surface: the headless Wayland compositor of a farglass session (ADR-007).
//!
//! The compositor draws nothing. Each top-level window of a Wayland client is
//! one surface for the shell: the compositor gives the buffer and the damage
//! of each commit as an [`Event`], and the shell gives the size, the focus,
//! and the close of each window as a [`Command`]. The shell is the only
//! window manager, thus the compositor asks each client for server-side
//! decorations and does not place windows.
//!
//! The compositor runs on its own thread with a calloop event loop. It
//! listens on a Unix socket at a path that the caller selects. A client uses
//! that path as `WAYLAND_DISPLAY`.

// Smithay gives the memory of an shm pool as a raw pointer, and the Wayland
// display needs a mutable access from the event source. Each unsafe block
// names its reason.
#![deny(unsafe_code)]

pub mod cache;
pub mod classify;
pub mod clipboard;
pub mod cursor;
pub mod pace;
pub mod scroll;
mod state;
pub mod tiles;
pub mod video;

use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use smithay::reexports::calloop::channel::Sender;
use smithay::reexports::calloop::{LoopSignal, channel};

pub use clipboard::Clip;
pub use state::{Command, Commit, CursorImage, Event, Format, Rect};

/// An error of the start of the compositor.
#[derive(Debug, thiserror::Error)]
pub enum SurfaceError {
    /// The Unix socket did not open.
    #[error("The Wayland socket {} did not open: {source}.", path.display())]
    Socket {
        /// The path.
        path: PathBuf,
        /// The cause.
        source: std::io::Error,
    },
    /// The event loop or the display did not start.
    #[error("The Wayland compositor did not start: {0}.")]
    Start(String),
}

/// The running compositor. Dropping it stops the compositor and removes its socket.
#[derive(Debug)]
pub struct Compositor {
    socket: PathBuf,
    x11_display: Option<u32>,
    commands: Sender<Command>,
    signal: LoopSignal,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Compositor {
    /// Starts a compositor on a new Unix socket at `socket`. `events` gets each event on the thread of the compositor.
    ///
    /// A file at `socket` that is a socket with no listener goes first, for example after a crash.
    ///
    /// # Errors
    ///
    /// [`SurfaceError`] if the socket does not open or the event loop does not start.
    pub fn spawn(
        socket: &Path,
        events: impl FnMut(Event) + Send + 'static,
    ) -> Result<Self, SurfaceError> {
        Self::spawn_with(socket, false, events)
    }

    /// Starts a compositor as [`Compositor::spawn`] does. With `x11`, it also starts Xwayland
    /// for X11 apps (F082). The program `Xwayland` must be in `PATH`.
    /// If Xwayland does not start, the compositor runs with no X11 display.
    ///
    /// # Errors
    ///
    /// [`SurfaceError`] if the socket does not open or the event loop does not start.
    pub fn spawn_with(
        socket: &Path,
        x11: bool,
        events: impl FnMut(Event) + Send + 'static,
    ) -> Result<Self, SurfaceError> {
        let listener = bind(socket)?;
        let (commands, receiver) = channel::channel::<Command>();
        let (ready_tx, ready_rx) =
            std::sync::mpsc::channel::<Result<(LoopSignal, Option<u32>), String>>();
        let events: Box<dyn FnMut(Event) + Send> = Box::new(events);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = std::thread::Builder::new()
            .name("farglass-wayland".to_owned())
            .spawn(move || state::run(listener, receiver, events, x11, &thread_stop, &ready_tx))
            .map_err(|e| SurfaceError::Start(e.to_string()))?;
        let (signal, x11_display) = ready_rx
            .recv()
            .map_err(|e| SurfaceError::Start(e.to_string()))?
            .map_err(SurfaceError::Start)?;
        Ok(Self {
            socket: socket.to_owned(),
            x11_display,
            commands,
            signal,
            stop,
            thread: Some(thread),
        })
    }

    /// Returns the path of the socket, the value for `WAYLAND_DISPLAY`.
    #[must_use]
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// Returns the number of the X11 display of Xwayland, the `N` of `DISPLAY=:N`.
    /// `None` if the compositor has no Xwayland.
    #[must_use]
    pub fn x11_display(&self) -> Option<u32> {
        self.x11_display
    }

    /// Gives a command to the compositor. Returns `false` if the compositor stopped.
    pub fn send(&self, command: Command) -> bool {
        self.commands.send(command).is_ok()
    }

    /// Returns a sender of commands that other threads can keep.
    #[must_use]
    pub fn commands(&self) -> CommandSender {
        CommandSender(Arc::new(self.commands.clone()))
    }
}

/// A sender of commands to the compositor.
#[derive(Clone, Debug)]
pub struct CommandSender(Arc<Sender<Command>>);

impl CommandSender {
    /// Gives a command to the compositor. Returns `false` if the compositor stopped.
    pub fn send(&self, command: Command) -> bool {
        self.0.send(command).is_ok()
    }
}

impl Drop for Compositor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.signal.wakeup();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// Opens the listening socket. Removes a stale socket file first.
fn bind(path: &Path) -> Result<UnixListener, SurfaceError> {
    let error = |source| SurfaceError::Socket {
        path: path.to_owned(),
        source,
    };
    if path.exists() {
        if std::os::unix::net::UnixStream::connect(path).is_ok() {
            return Err(error(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "a different compositor listens on it",
            )));
        }
        std::fs::remove_file(path).map_err(error)?;
    }
    let listener = UnixListener::bind(path).map_err(error)?;
    listener.set_nonblocking(true).map_err(error)?;
    Ok(listener)
}
