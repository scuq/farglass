//! The windows of the Wayland compositor of the session (F024, ADR-007).
//!
//! The agent runs the compositor of farglass-surface on a socket in
//! `XDG_RUNTIME_DIR`, and each terminal gets that socket as `WAYLAND_DISPLAY`.
//! A surfaces channel of the shell gets each top-level window, each popup,
//! and the facts of each commit. The shell sends the size, the focus, and the
//! close of each window. The pixels of the commits go to the shell with the
//! tile codec (F025). The agent keeps the last buffer of each surface for it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use farglass_proto::message::{SurfaceEvent, SurfaceInput, SurfaceRect, SurfaceRequest};
use farglass_surface::{Clip, Command, Compositor, Event, Rect, SurfaceError};

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
type FrameSink = Arc<dyn Fn(FrameUpdate) + Send + Sync>;

/// The new pixels of a surface for a surface channel.
#[derive(Clone, Debug)]
pub struct FrameUpdate {
    /// The width in pixels.
    pub width: usize,
    /// The height in pixels.
    pub height: usize,
    /// The BGRA pixels of the whole buffer.
    pub pixels: Arc<Vec<u8>>,
    /// The parts that changed.
    pub damage: Vec<Rect>,
}
type CommandSink = Box<dyn Fn(Command) + Send + Sync>;
type ClipSink = Arc<dyn Fn(&Clip) + Send + Sync>;

#[derive(Clone, Debug, Default)]
struct Toplevel {
    title: Option<String>,
    app_id: Option<String>,
    parent: Option<u32>,
    /// The process of the client (F045).
    pid: Option<u32>,
}

#[derive(Default)]
struct Inner {
    toplevels: BTreeMap<u32, Toplevel>,
    popups: BTreeMap<u32, (u32, SurfaceRect, bool)>,
    /// The last commit of each surface, with the whole buffer as damage, for a new channel.
    commits: HashMap<u32, SurfaceEvent>,
    /// The last pixels of each surface, for the tile codec.
    pixels: HashMap<u32, (usize, usize, Arc<Vec<u8>>)>,
    viewers: HashMap<u64, EventSink>,
    /// The surface channels of each surface.
    watchers: HashMap<u32, HashMap<u64, FrameSink>>,
    /// The clipboard channels.
    clipboards: HashMap<u64, ClipSink>,
    /// The surface channels of each surface that send a video region (F048).
    videos: HashMap<u32, HashSet<u64>>,
    /// The last channels and video of each surface that the compositor got (F048).
    paced: HashMap<u32, (bool, bool)>,
}

impl Inner {
    /// Returns the command for the compositor if the channels or the video of the surface `id` changed (F048).
    fn viewers(&mut self, id: u32) -> Option<Command> {
        let watched = self.watchers.get(&id).is_some_and(|w| !w.is_empty());
        let video = self.videos.get(&id).is_some_and(|v| !v.is_empty());
        (self.paced.insert(id, (watched, video)) != Some((watched, video)))
            .then_some(Command::Viewers { id, watched, video })
    }

    /// Forgets the channel state of a closed surface.
    fn forget(&mut self, id: u32) {
        self.paced.remove(&id);
        self.videos.remove(&id);
    }
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

/// One surface channel. Dropping it ends the watch.
pub struct WatchGuard {
    surfaces: Arc<Surfaces>,
    surface: u32,
    id: u64,
}

impl std::fmt::Debug for WatchGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchGuard")
            .field("surface", &self.surface)
            .finish()
    }
}

impl WatchGuard {
    /// Returns the flag that tells the compositor that this channel sends a video region (F048).
    #[must_use]
    pub fn video(&self) -> VideoFlag {
        VideoFlag {
            surfaces: self.surfaces.clone(),
            surface: self.surface,
            id: self.id,
        }
    }
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        let command = {
            let mut inner = self.surfaces.lock();
            if let Some(watchers) = inner.watchers.get_mut(&self.surface) {
                watchers.remove(&self.id);
            }
            if let Some(videos) = inner.videos.get_mut(&self.surface) {
                videos.remove(&self.id);
            }
            inner.viewers(self.surface)
        };
        if let Some(command) = command {
            self.surfaces.command(command);
        }
    }
}

/// Tells the compositor that a surface channel sends a video region: the window keeps the full frame rate (F048).
pub struct VideoFlag {
    surfaces: Arc<Surfaces>,
    surface: u32,
    id: u64,
}

impl std::fmt::Debug for VideoFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoFlag")
            .field("surface", &self.surface)
            .finish()
    }
}

impl VideoFlag {
    /// Sets whether the channel sends a video region now.
    pub fn set(&self, video: bool) {
        let command = {
            let mut inner = self.surfaces.lock();
            // A channel that ended does not set its video again.
            if !inner
                .watchers
                .get(&self.surface)
                .is_some_and(|w| w.contains_key(&self.id))
            {
                return;
            }
            let videos = inner.videos.entry(self.surface).or_default();
            if video {
                videos.insert(self.id);
            } else {
                videos.remove(&self.id);
            }
            inner.viewers(self.surface)
        };
        if let Some(command) = command {
            self.surfaces.command(command);
        }
    }
}

/// One clipboard channel. Dropping it removes the channel.
pub struct ClipboardGuard {
    surfaces: Arc<Surfaces>,
    id: u64,
}

impl std::fmt::Debug for ClipboardGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipboardGuard")
            .field("id", &self.id)
            .finish()
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        self.surfaces.lock().clipboards.remove(&self.id);
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
    /// The compositor also starts Xwayland for X11 apps, if [`crate::desktop::x11_wanted`] permits it.
    /// The compositor stops when the returned value drops.
    ///
    /// # Errors
    ///
    /// [`SurfaceError`] if the compositor does not start.
    pub fn start(self: &Arc<Self>, socket: &Path) -> Result<Compositor, SurfaceError> {
        let surfaces = self.clone();
        let x11 = crate::desktop::x11_wanted();
        let compositor = Compositor::spawn_with(socket, x11, move |event| surfaces.handle(event))?;
        // Each terminal and each app gets the X11 display from now on (F082).
        if let Some(number) = compositor.x11_display() {
            crate::desktop::set_x11_display(number);
        }
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
                .map(|(id, (parent, geometry, subsurface))| SurfaceEvent::Popup {
                    id: *id,
                    parent: *parent,
                    geometry: *geometry,
                    subsurface: *subsurface,
                });
            toplevels
                .chain(popups)
                .chain(inner.commits.values().cloned())
                .chain(std::iter::once(SurfaceEvent::Synced))
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
        self.lock()
            .pixels
            .get(&id)
            .map(|(_, _, pixels)| pixels.clone())
    }

    /// Watches the pixels of the surface `id` for a surface channel. `send` gets the last
    /// buffer at once, with the whole buffer as damage, then each commit.
    pub fn watch(
        self: &Arc<Self>,
        surface: u32,
        send: impl Fn(FrameUpdate) + Send + Sync + 'static,
    ) -> WatchGuard {
        let id = self.next_viewer.fetch_add(1, Ordering::Relaxed);
        let send: FrameSink = Arc::new(send);
        let (last, command) = {
            let mut inner = self.lock();
            inner
                .watchers
                .entry(surface)
                .or_default()
                .insert(id, send.clone());
            (inner.pixels.get(&surface).cloned(), inner.viewers(surface))
        };
        if let Some(command) = command {
            self.command(command);
        }
        if let Some((width, height, pixels)) = last {
            let full = Rect {
                x: 0,
                y: 0,
                width: i32::try_from(width).unwrap_or(i32::MAX),
                height: i32::try_from(height).unwrap_or(i32::MAX),
            };
            send(FrameUpdate {
                width,
                height,
                pixels,
                damage: vec![full],
            });
        }
        WatchGuard {
            surfaces: self.clone(),
            surface,
            id,
        }
    }

    /// Adds a clipboard channel. `send` gets each content that an app copies.
    /// It does not get the current content: a new channel must not replace the clipboard of the client computer.
    pub fn add_clipboard(
        self: &Arc<Self>,
        send: impl Fn(&Clip) + Send + Sync + 'static,
    ) -> ClipboardGuard {
        let id = self.next_viewer.fetch_add(1, Ordering::Relaxed);
        self.lock().clipboards.insert(id, Arc::new(send));
        ClipboardGuard {
            surfaces: self.clone(),
            id,
        }
    }

    /// Sets the clipboard of the apps to a content of the shell.
    pub fn set_clipboard(&self, clip: Clip) {
        self.command(Command::SetClipboard(clip));
    }

    fn command(&self, command: Command) {
        if let Some(commands) = &*self.commands.lock().unwrap_or_else(PoisonError::into_inner) {
            commands(command);
        }
    }

    /// Returns the process of the client of each top-level window: the id of the window and the process id (F045).
    #[must_use]
    pub fn processes(&self) -> Vec<(u32, u32)> {
        self.lock()
            .toplevels
            .iter()
            .filter_map(|(id, toplevel)| toplevel.pid.map(|pid| (*id, pid)))
            .collect()
    }

    /// Gives a request of the shell to the compositor.
    pub fn request(&self, request: SurfaceRequest) {
        let command = match request {
            SurfaceRequest::Configure { id, width, height } => {
                tracing::debug!(id, width, height, "the shell gives a window a size");
                Command::Configure { id, width, height }
            }
            SurfaceRequest::Activate { id } => Command::Activate { id },
            SurfaceRequest::Close { id } => Command::Close { id },
            SurfaceRequest::PopupDismiss { id } => Command::PopupDismiss { id },
        };
        self.command(command);
    }

    /// Gives input of the shell to the compositor.
    pub fn input(&self, input: SurfaceInput) {
        let command = match input {
            SurfaceInput::Pointer { id, x, y } => Command::PointerMotion { id, x, y },
            SurfaceInput::Leave => Command::PointerLeave,
            SurfaceInput::Button { button, pressed } => Command::PointerButton {
                // The Linux codes of BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_SIDE, and BTN_EXTRA.
                button: match button {
                    0 => 0x110,
                    1 => 0x112,
                    2 => 0x111,
                    3 => 0x113,
                    _ => 0x114,
                },
                pressed,
            },
            SurfaceInput::Axis { dx, dy } => Command::PointerAxis { dx, dy },
            SurfaceInput::Key { id, code, pressed } => Command::Key { id, code, pressed },
        };
        self.command(command);
    }

    /// Takes an event of the compositor: keeps the state for new channels, and tells each channel.
    pub fn handle(&self, event: Event) {
        if let Event::Clipboard(clip) = event {
            let sinks: Vec<ClipSink> = self.lock().clipboards.values().cloned().collect();
            tracing::debug!(channels = sinks.len(), "an app set the clipboard");
            for send in sinks {
                send(&clip);
            }
            return;
        }
        let mut pending = None;
        let message = {
            let mut inner = self.lock();
            match event {
                Event::ToplevelNew { id, pid } => {
                    // A window with no channel yet gets no frame callbacks until the shell opens one (F048).
                    pending = inner.viewers(id);
                    inner.toplevels.insert(
                        id,
                        Toplevel {
                            pid,
                            ..Toplevel::default()
                        },
                    );
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
                    let pid = inner.toplevels.get(&id).and_then(|t| t.pid);
                    inner.toplevels.insert(
                        id,
                        Toplevel {
                            title: title.clone(),
                            app_id: app_id.clone(),
                            parent,
                            pid,
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
                    inner.forget(id);
                    inner.toplevels.remove(&id);
                    inner.commits.remove(&id);
                    inner.pixels.remove(&id);
                    SurfaceEvent::Closed { id }
                }
                Event::PopupNew {
                    id,
                    parent,
                    geometry,
                    subsurface,
                } => {
                    inner
                        .popups
                        .insert(id, (parent, rect(geometry), subsurface));
                    SurfaceEvent::Popup {
                        id,
                        parent,
                        geometry: rect(geometry),
                        subsurface,
                    }
                }
                Event::PopupClosed { id } => {
                    inner.forget(id);
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
                    let (width, height) = (
                        usize::try_from(commit.width).unwrap_or(0),
                        usize::try_from(commit.height).unwrap_or(0),
                    );
                    inner
                        .pixels
                        .insert(commit.id, (width, height, commit.pixels.clone()));
                    let update = FrameUpdate {
                        width,
                        height,
                        pixels: commit.pixels.clone(),
                        damage: commit.damage.clone(),
                    };
                    for watch in inner
                        .watchers
                        .get(&commit.id)
                        .into_iter()
                        .flat_map(HashMap::values)
                    {
                        watch(update.clone());
                    }
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
                Event::MoveRequest { id } => {
                    tracing::debug!(id, "the app asks to move its window");
                    SurfaceEvent::MoveRequest { id }
                }
                Event::Cursor(name) => SurfaceEvent::Cursor { name },
                Event::Clipboard(_) => return,
            }
        };
        if let Some(command) = pending {
            self.command(command);
        }
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
        surfaces.handle(Event::ToplevelNew { id: 1, pid: None });
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
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[2], SurfaceEvent::Synced);
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
        surfaces.handle(Event::ToplevelNew { id: 3, pid: None });
        surfaces.handle(commit(3));
        surfaces.handle(Event::ToplevelClosed { id: 3 });
        assert!(surfaces.pixels(3).is_none());
        surfaces.request(SurfaceRequest::Close { id: 3 });
        surfaces.request(SurfaceRequest::Activate { id: None });
        assert_eq!(
            *commands.lock().unwrap(),
            [
                Command::Viewers {
                    id: 3,
                    watched: false,
                    video: false
                },
                Command::Close { id: 3 },
                Command::Activate { id: None }
            ]
        );
    }

    #[test]
    fn the_compositor_gets_the_channels_and_the_video_of_a_window() {
        let surfaces = Surfaces::new();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let sink = commands.clone();
        surfaces.set_commands(move |command| sink.lock().unwrap().push(command));
        let viewers = |watched, video| Command::Viewers {
            id: 5,
            watched,
            video,
        };
        surfaces.handle(Event::ToplevelNew { id: 5, pid: None });
        let first = surfaces.watch(5, |_| {});
        let second = surfaces.watch(5, |_| {});
        first.video().set(true);
        second.video().set(true);
        first.video().set(false);
        drop(second);
        let flag = first.video();
        drop(first);
        flag.set(true);
        assert_eq!(
            *commands.lock().unwrap(),
            [
                viewers(false, false),
                viewers(true, false),
                viewers(true, true),
                viewers(true, false),
                viewers(false, false)
            ]
        );
    }
}
