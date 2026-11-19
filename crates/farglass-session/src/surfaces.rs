//! The windows of the Wayland compositor of the session (F024, ADR-007).
//!
//! The agent runs the compositor of farglass-surface on a socket in
//! `XDG_RUNTIME_DIR`, and each terminal gets that socket as `WAYLAND_DISPLAY`.
//! A surfaces channel of the shell gets each top-level window, each popup,
//! and the facts of each commit. The shell sends the size, the focus, and the
//! close of each window. The pixels of the commits go to the shell with the
//! tile codec (F025). The agent keeps the last buffer of each surface for it.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use farglass_proto::message::{SurfaceEvent, SurfaceRect, SurfaceRequest};
use farglass_surface::{Command, Compositor, Event, Rect, SurfaceError};

/// The name of the socket in `XDG_RUNTIME_DIR`.
pub const SOCKET_NAME: &str = "farglass-wayland";

/// Returns the path of the Wayland socket of the session: `FARGLASS_WAYLAND_SOCKET`,
/// or [`SOCKET_NAME`] in `XDG_RUNTIME_DIR`. `None` if the session has no runtime directory.
#[must_use]
pub fn socket_path() -> Option<PathBuf> {
    std::env::var_os("FARGLASS_WAYLAND_SOCKET")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_RUNTIME_DIR").map(|dir| PathBuf::from(dir).join(SOCKET_NAME))
        })
}

type EventSink = Arc<dyn Fn(SurfaceEvent) + Send + Sync>;
type CommandSink = Box<dyn Fn(Command) + Send + Sync>;

#[derive(Clone, Debug, Default)]
struct Toplevel {
    title: Option<String>,
    app_id: Option<String>,
    parent: Option<u32>,
}

#[derive(Default)]
struct Inner {
    toplevels: BTreeMap<u32, Toplevel>,
    popups: BTreeMap<u32, (u32, SurfaceRect)>,
    /// The last commit of each surface, with the whole buffer as damage, for a new channel.
    commits: HashMap<u32, SurfaceEvent>,
    /// The last pixels of each surface, for the tile codec.
    pixels: HashMap<u32, Arc<Vec<u8>>>,
    viewers: HashMap<u64, EventSink>,
}

/// The windows of the compositor and the surfaces channels of the shell.
pub struct Surfaces {
    inner: Mutex<Inner>,
    commands: Mutex<Option<CommandSink>>,
    next_viewer: AtomicU64,
}

impl std::fmt::Debug for Surfaces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Surfaces").finish_non_exhaustive()
    }
}

/// One open surfaces channel. Dropping it removes the channel.
pub struct ViewerGuard {
    surfaces: Arc<Surfaces>,
    id: u64,
}

impl std::fmt::Debug for ViewerGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewerGuard").field("id", &self.id).finish()
    }
}

impl Drop for ViewerGuard {
    fn drop(&mut self) {
        self.surfaces.lock().viewers.remove(&self.id);
    }
}

const fn rect(r: Rect) -> SurfaceRect {
    SurfaceRect {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
    }
}

impl Surfaces {
    /// Returns an empty set of windows with no compositor.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            commands: Mutex::new(None),
            next_viewer: AtomicU64::new(1),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts the compositor on `socket`. Its events come to this set, and the requests of the shell go to it.
    /// The compositor stops when the returned value drops.
    ///
    /// # Errors
    ///
    /// [`SurfaceError`] if the compositor does not start.
    pub fn start(self: &Arc<Self>, socket: &Path) -> Result<Compositor, SurfaceError> {
        let surfaces = self.clone();
        let compositor = Compositor::spawn(socket, move |event| surfaces.handle(event))?;
        let sender = compositor.commands();
        self.set_commands(move |command| {
            sender.send(command);
        });
        Ok(compositor)
    }

    /// Sets the receiver of the commands, for example a compositor or a test.
    pub fn set_commands(&self, commands: impl Fn(Command) + Send + Sync + 'static) {
        *self.commands.lock().unwrap_or_else(PoisonError::into_inner) = Some(Box::new(commands));
    }

    /// Adds a surfaces channel. It gets each window, each popup, and the last commit of each surface at once, then each change.
    pub fn add_viewer(
        self: &Arc<Self>,
        send: impl Fn(SurfaceEvent) + Send + Sync + 'static,
    ) -> ViewerGuard {
        let id = self.next_viewer.fetch_add(1, Ordering::Relaxed);
        let send: EventSink = Arc::new(send);
        let snapshot: Vec<SurfaceEvent> = {
            let mut inner = self.lock();
            inner.viewers.insert(id, send.clone());
            let toplevels = inner
                .toplevels
                .iter()
                .map(|(id, t)| SurfaceEvent::Toplevel {
                    id: *id,
                    title: t.title.clone(),
                    app_id: t.app_id.clone(),
                    parent: t.parent,
                });
            let popups = inner
                .popups
                .iter()
                .map(|(id, (parent, geometry))| SurfaceEvent::Popup {
                    id: *id,
                    parent: *parent,
                    geometry: *geometry,
                });
            toplevels
                .chain(popups)
                .chain(inner.commits.values().cloned())
                .collect()
        };
        for event in snapshot {
            send(event);
        }
        ViewerGuard {
            surfaces: self.clone(),
            id,
        }
    }

    /// Returns the pixels of the last commit of the surface `id`: `width * 4` bytes in each row.
    #[must_use]
    pub fn pixels(&self, id: u32) -> Option<Arc<Vec<u8>>> {
        self.lock().pixels.get(&id).cloned()
    }

    /// Gives a request of the shell to the compositor.
    pub fn request(&self, request: SurfaceRequest) {
        let command = match request {
            SurfaceRequest::Configure { id, width, height } => {
                Command::Configure { id, width, height }
            }
            SurfaceRequest::Activate { id } => Command::Activate { id },
            SurfaceRequest::Close { id } => Command::Close { id },
            SurfaceRequest::PopupDismiss { id } => Command::PopupDismiss { id },
        };
        if let Some(commands) = &*self.commands.lock().unwrap_or_else(PoisonError::into_inner) {
            commands(command);
        }
    }

    /// Takes an event of the compositor: keeps the state for new channels, and tells each channel.
    pub fn handle(&self, event: Event) {
        let message = {
            let mut inner = self.lock();
            match event {
                Event::ToplevelNew { id } => {
                    inner.toplevels.insert(id, Toplevel::default());
                    SurfaceEvent::Toplevel {
                        id,
                        title: None,
                        app_id: None,
                        parent: None,
                    }
                }
                Event::ToplevelChanged {
                    id,
                    title,
                    app_id,
                    parent,
                } => {
                    inner.toplevels.insert(
                        id,
                        Toplevel {
                            title: title.clone(),
                            app_id: app_id.clone(),
                            parent,
                        },
                    );
                    SurfaceEvent::Toplevel {
                        id,
                        title,
                        app_id,
                        parent,
                    }
                }
                Event::ToplevelClosed { id } => {
                    inner.toplevels.remove(&id);
                    inner.commits.remove(&id);
                    inner.pixels.remove(&id);
                    SurfaceEvent::Closed { id }
                }
                Event::PopupNew {
                    id,
                    parent,
                    geometry,
                } => {
                    inner.popups.insert(id, (parent, rect(geometry)));
                    SurfaceEvent::Popup {
                        id,
                        parent,
                        geometry: rect(geometry),
                    }
                }
                Event::PopupClosed { id } => {
                    inner.popups.remove(&id);
                    inner.commits.remove(&id);
                    inner.pixels.remove(&id);
                    SurfaceEvent::PopupClosed { id }
                }
                Event::Commit(commit) => {
                    let full = SurfaceEvent::Commit {
                        id: commit.id,
                        width: commit.width,
                        height: commit.height,
                        scale: commit.scale,
                        damage: vec![SurfaceRect {
                            x: 0,
                            y: 0,
                            width: commit.width,
                            height: commit.height,
                        }],
                        geometry: commit.geometry.map(rect),
                    };
                    inner.commits.insert(commit.id, full);
                    inner.pixels.insert(commit.id, commit.pixels.clone());
                    SurfaceEvent::Commit {
                        id: commit.id,
                        width: commit.width,
                        height: commit.height,
                        scale: commit.scale,
                        damage: commit.damage.into_iter().map(rect).collect(),
                        geometry: commit.geometry.map(rect),
                    }
                }
                Event::Unmapped { id } => {
                    inner.commits.remove(&id);
                    inner.pixels.remove(&id);
                    SurfaceEvent::Unmapped { id }
                }
                Event::MoveRequest { id } => SurfaceEvent::MoveRequest { id },
            }
        };
        let viewers: Vec<EventSink> = self.lock().viewers.values().cloned().collect();
        for send in viewers {
            send(message.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use farglass_surface::{Commit, Format};

    use super::*;

    fn commit(id: u32) -> Event {
        Event::Commit(Commit {
            id,
            width: 4,
            height: 2,
            scale: 1,
            format: Format::Argb8888,
            damage: vec![Rect {
                x: 1,
                y: 0,
                width: 1,
                height: 1,
            }],
            geometry: None,
            pixels: Arc::new(vec![7; 32]),
        })
    }

    #[test]
    fn a_new_channel_gets_each_window_and_the_last_commit_with_full_damage() {
        let surfaces = Surfaces::new();
        surfaces.handle(Event::ToplevelNew { id: 1 });
        surfaces.handle(Event::ToplevelChanged {
            id: 1,
            title: Some("Calculator".into()),
            app_id: None,
            parent: None,
        });
        surfaces.handle(commit(1));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let _viewer = surfaces.add_viewer(move |event| sink.lock().unwrap().push(event));
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2);
        assert!(
            matches!(&seen[0], SurfaceEvent::Toplevel { title: Some(t), .. } if t == "Calculator")
        );
        assert!(
            matches!(&seen[1], SurfaceEvent::Commit { damage, .. } if damage == &[SurfaceRect { x: 0, y: 0, width: 4, height: 2 }])
        );
        assert_eq!(surfaces.pixels(1).unwrap().len(), 32);
    }

    #[test]
    fn a_closed_window_goes_and_requests_become_commands() {
        let surfaces = Surfaces::new();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let sink = commands.clone();
        surfaces.set_commands(move |command| sink.lock().unwrap().push(command));
        surfaces.handle(Event::ToplevelNew { id: 3 });
        surfaces.handle(commit(3));
        surfaces.handle(Event::ToplevelClosed { id: 3 });
        assert!(surfaces.pixels(3).is_none());
        surfaces.request(SurfaceRequest::Close { id: 3 });
        surfaces.request(SurfaceRequest::Activate { id: None });
        assert_eq!(
            *commands.lock().unwrap(),
            [Command::Close { id: 3 }, Command::Activate { id: None }]
        );
    }
}
