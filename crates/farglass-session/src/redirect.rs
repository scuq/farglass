//! Video redirection (F039, F040).
//!
//! A video player of a web page gives the compressed stream to the browser
//! through Media Source Extensions. The farglass extension in the host
//! browser copies each call of the page and the state of the video element,
//! and gives them to its native messaging host. The native messaging host is
//! this program with the origin of the extension as its argument: it copies
//! the messages to the socket of the agent ([`socket_path`]) and back. The
//! agent gives the calls to each redirect channel of the shell. The client
//! browser plays the same stream with its own decoder, under the window of
//! the host browser.
//!
//! The messages on the socket have the framing of native messaging: a
//! length as u32 in native byte order, then UTF-8 JSON.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine as _;
use bytes::Bytes;
use farglass_proto::message::{RedirectEvent, RedirectRect, RedirectRequest};
use serde::Deserialize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::mpsc;

/// The name of the native messaging host.
pub const NATIVE_HOST: &str = "org.farglass.redirect";

/// The id of the farglass extension. The key in its manifest gives it.
pub const EXTENSION_ID: &str = "alhjaabffmdgbbogejdgmenkjfkahfog";

/// The largest message of the extension: 64 MiB, a large media segment in base64.
const MAX_MESSAGE: usize = 64 << 20;

/// The media sources of one browser connection at the same time (F052).
pub const MAX_MEDIAS: usize = 32;

/// The SourceBuffers of one media source (F052).
pub const MAX_SOURCES: usize = 8;

/// The largest initialization segment (F052). A real one has some kilobytes.
pub const MAX_INIT: usize = 4 << 20;

/// The bytes that the agent keeps of all media sources of one browser connection (F052).
pub const MAX_KEPT: usize = 64 << 20;

/// The appends that the agent keeps for each media source, for a shell that connects later.
const KEEP_APPENDS: usize = 300;

/// The bytes of the appends that the agent keeps for each media source.
const KEEP_BYTES: usize = 16 << 20;

/// Returns the path of the socket: `FARGLASS_REDIRECT_SOCKET`, or `farglass-redirect.sock` in `XDG_RUNTIME_DIR`.
#[must_use]
pub fn socket_path() -> Option<PathBuf> {
    std::env::var_os("FARGLASS_REDIRECT_SOCKET")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_RUNTIME_DIR")
                .map(|dir| PathBuf::from(dir).join("farglass-redirect.sock"))
        })
}

/// A message of the extension: a call of the page, with the tab that made it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PlayerMessage {
    /// `MediaSource.addSourceBuffer`.
    Source {
        /// The tab.
        tab: i64,
        /// The media source in the tab.
        media: String,
        /// The SourceBuffer.
        source: String,
        /// The MIME type.
        mime: String,
    },
    /// `SourceBuffer.appendBuffer`, with the data in base64.
    Append {
        /// The tab.
        tab: i64,
        /// The media source in the tab.
        media: String,
        /// The SourceBuffer.
        source: String,
        /// The `timestampOffset`.
        #[serde(default)]
        offset: f64,
        /// The data in base64.
        data: String,
    },
    /// `SourceBuffer.changeType`.
    ChangeType {
        /// The tab.
        tab: i64,
        /// The media source in the tab.
        media: String,
        /// The SourceBuffer.
        source: String,
        /// The new MIME type.
        mime: String,
    },
    /// `SourceBuffer.remove`.
    Remove {
        /// The tab.
        tab: i64,
        /// The media source in the tab.
        media: String,
        /// The SourceBuffer.
        source: String,
        /// The start.
        start: f64,
        /// The end. None is infinite.
        end: Option<f64>,
    },
    /// `SourceBuffer.abort`.
    Abort {
        /// The tab.
        tab: i64,
        /// The media source in the tab.
        media: String,
        /// The SourceBuffer.
        source: String,
    },
    /// The `duration` setter.
    Duration {
        /// The tab.
        tab: i64,
        /// The media source in the tab.
        media: String,
        /// The duration. None is infinite.
        value: Option<f64>,
    },
    /// `MediaSource.endOfStream`.
    End {
        /// The tab.
        tab: i64,
        /// The media source in the tab.
        media: String,
    },
    /// The state of the video element.
    State {
        /// The tab.
        tab: i64,
        /// The media source in the tab.
        media: String,
        /// The current time.
        time: f64,
        /// Reports whether the video is paused.
        paused: bool,
        /// The playback rate.
        rate: f64,
        /// The video element in the viewport.
        rect: RedirectRect,
        /// The viewport in the window.
        viewport: RedirectRect,
        /// The zoom of the page.
        zoom: f64,
        /// The title of the page.
        title: String,
        /// Reports whether the video shows.
        visible: bool,
    },
    /// The media source is gone.
    Gone {
        /// The tab.
        tab: i64,
        /// The media source in the tab.
        media: String,
    },
    /// The tab closed: each of its media sources is gone.
    TabClosed {
        /// The tab.
        tab: i64,
    },
}

type Sink = Arc<dyn Fn(&RedirectEvent, Option<&Bytes>) + Send + Sync>;

/// What the agent keeps of one media source for a shell that connects later.
#[derive(Debug, Default)]
struct MediaLog {
    sources: Vec<(String, String)>,
    /// The first append of each SourceBuffer: the initialization segment.
    init: HashMap<String, (f64, Bytes)>,
    recent: VecDeque<(String, f64, Bytes)>,
    recent_bytes: usize,
    duration: Option<Option<f64>>,
    ended: bool,
    state: Option<RedirectEvent>,
}

impl MediaLog {
    /// Returns the events that give this media source to a new channel.
    fn replay(&self, media: &str) -> Vec<(RedirectEvent, Option<Bytes>)> {
        let append = |source: &str, offset: f64, data: &Bytes| {
            (
                RedirectEvent::Append {
                    media: media.to_owned(),
                    source: source.to_owned(),
                    offset,
                    size: u32::try_from(data.len()).unwrap_or(u32::MAX),
                },
                Some(data.clone()),
            )
        };
        let mut out: Vec<(RedirectEvent, Option<Bytes>)> = self
            .sources
            .iter()
            .map(|(source, mime)| {
                (
                    RedirectEvent::Source {
                        media: media.to_owned(),
                        source: source.clone(),
                        mime: mime.clone(),
                    },
                    None,
                )
            })
            .collect();
        if let Some(value) = self.duration {
            out.push((
                RedirectEvent::Duration {
                    media: media.to_owned(),
                    value,
                },
                None,
            ));
        }
        for (source, _) in &self.sources {
            if let Some((offset, data)) = self.init.get(source) {
                out.push(append(source, *offset, data));
            }
        }
        for (source, offset, data) in &self.recent {
            out.push(append(source, *offset, data));
        }
        if self.ended {
            out.push((
                RedirectEvent::End {
                    media: media.to_owned(),
                },
                None,
            ));
        }
        if let Some(state) = &self.state {
            out.push((state.clone(), None));
        }
        out
    }
}

#[derive(Default)]
struct Inner {
    viewers: HashMap<u64, Sink>,
    medias: BTreeMap<String, MediaLog>,
    browsers: HashMap<u64, mpsc::UnboundedSender<Vec<u8>>>,
    /// The process of the host browser of each browser connection: the parent of its native messaging host.
    pids: HashMap<u64, u32>,
}

/// Returns the windows of a process: the top-level windows whose Wayland client has that pid.
pub type WindowsOf = Arc<dyn Fn(u32) -> Vec<u32> + Send + Sync>;

/// The players of the host browsers and the redirect channels of the shell.
pub struct Redirect {
    inner: Mutex<Inner>,
    next: AtomicU64,
    windows: Mutex<Option<WindowsOf>>,
}

impl std::fmt::Debug for Redirect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redirect").finish_non_exhaustive()
    }
}

/// One redirect channel. Dropping it removes the channel.
pub struct RedirectGuard {
    redirect: Arc<Redirect>,
    id: u64,
}

impl std::fmt::Debug for RedirectGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedirectGuard")
            .field("id", &self.id)
            .finish()
    }
}

impl Drop for RedirectGuard {
    fn drop(&mut self) {
        let mut inner = self.redirect.lock();
        inner.viewers.remove(&self.id);
        // With no channel, the host draws its videos again.
        if inner.viewers.is_empty() {
            let medias: Vec<String> = inner.medias.keys().cloned().collect();
            drop(inner);
            for media in medias {
                self.redirect.request(RedirectRequest::Showing {
                    media,
                    showing: false,
                });
            }
        }
    }
}

/// Returns the key of a media source of a browser connection: `browser:tab:media`.
fn key(browser: u64, tab: i64, media: &str) -> String {
    format!("{browser}:{tab}:{media}")
}

/// Returns the frame of a message for the extension: the length in native byte order, then the JSON.
fn frame(json: &serde_json::Value) -> Vec<u8> {
    let text = json.to_string();
    let mut out = u32::try_from(text.len())
        .unwrap_or(u32::MAX)
        .to_ne_bytes()
        .to_vec();
    out.extend_from_slice(text.as_bytes());
    out
}

impl Redirect {
    /// Returns a hub with no browser and no channel.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            next: AtomicU64::new(1),
            windows: Mutex::new(None),
        })
    }

    /// Sets the function that gives the windows of a process (F052). Without it, no video has a window.
    pub fn set_windows(&self, windows: WindowsOf) {
        *self.windows.lock().unwrap_or_else(PoisonError::into_inner) = Some(windows);
    }

    /// Sets the process of the host browser of the connection `browser`.
    pub fn set_browser_pid(&self, browser: u64, pid: u32) {
        self.lock().pids.insert(browser, pid);
    }

    /// Returns the windows of the host browser of the connection `browser`. An unknown process gives none.
    fn windows_of(&self, browser: u64) -> Vec<u32> {
        let Some(pid) = self.lock().pids.get(&browser).copied() else {
            return Vec::new();
        };
        let windows = self
            .windows
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        windows.map_or_else(Vec::new, |windows| windows(pid))
    }

    /// Checks the caps of a new media source and a new SourceBuffer (F052).
    fn check_caps(
        inner: &Inner,
        browser: u64,
        media: &str,
        new_source: bool,
    ) -> Result<(), String> {
        let prefix = format!("{browser}:");
        if !inner.medias.contains_key(media)
            && inner
                .medias
                .keys()
                .filter(|key| key.starts_with(&prefix))
                .count()
                >= MAX_MEDIAS
        {
            return Err(format!("more than {MAX_MEDIAS} media sources"));
        }
        if new_source
            && inner
                .medias
                .get(media)
                .is_some_and(|log| log.sources.len() >= MAX_SOURCES)
        {
            return Err(format!("more than {MAX_SOURCES} SourceBuffers"));
        }
        Ok(())
    }

    /// Returns the bytes that the agent keeps of the media sources of the connection `browser`.
    fn kept(inner: &Inner, browser: u64) -> usize {
        let prefix = format!("{browser}:");
        inner
            .medias
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .map(|(_, log)| {
                log.recent_bytes + log.init.values().map(|(_, data)| data.len()).sum::<usize>()
            })
            .sum()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds a redirect channel. It gets each media source that plays now, then each change.
    pub fn add_viewer(
        self: &Arc<Self>,
        send: impl Fn(&RedirectEvent, Option<&Bytes>) + Send + Sync + 'static,
    ) -> RedirectGuard {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let send: Sink = Arc::new(send);
        let replay: Vec<(RedirectEvent, Option<Bytes>)> = {
            let mut inner = self.lock();
            inner.viewers.insert(id, send.clone());
            inner
                .medias
                .iter()
                .flat_map(|(media, log)| log.replay(media))
                .collect()
        };
        for (event, data) in &replay {
            send(event, data.as_ref());
        }
        RedirectGuard {
            redirect: self.clone(),
            id,
        }
    }

    /// Takes a message of the browser connection `browser`.
    ///
    /// # Errors
    ///
    /// A message if the data of an append is not valid base64.
    pub fn take(&self, browser: u64, message: PlayerMessage) -> Result<(), String> {
        // The windows first: `windows_of` takes the lock itself.
        let windows = if matches!(message, PlayerMessage::State { .. }) {
            self.windows_of(browser)
        } else {
            Vec::new()
        };
        let mut data = None;
        let mut inner = self.lock();
        let event = match message {
            PlayerMessage::Source {
                tab,
                media,
                source,
                mime,
            } => {
                let media = key(browser, tab, &media);
                Self::check_caps(&inner, browser, &media, true)?;
                let log = inner.medias.entry(media.clone()).or_default();
                log.sources.push((source.clone(), mime.clone()));
                RedirectEvent::Source {
                    media,
                    source,
                    mime,
                }
            }
            PlayerMessage::Append {
                tab,
                media,
                source,
                offset,
                data: text,
            } => {
                let bytes = Bytes::from(
                    base64::engine::general_purpose::STANDARD
                        .decode(text)
                        .map_err(|e| format!("The data of an append is not valid base64: {e}"))?,
                );
                let media = key(browser, tab, &media);
                Self::check_caps(&inner, browser, &media, false)?;
                let kept = Self::kept(&inner, browser);
                let log = inner.medias.entry(media.clone()).or_default();
                if !log.sources.iter().any(|(id, _)| *id == source) {
                    return Err("an append to a SourceBuffer that the page did not add".to_owned());
                }
                if !log.init.contains_key(&source) && bytes.len() > MAX_INIT {
                    return Err(format!(
                        "an initialization segment of {} bytes",
                        bytes.len()
                    ));
                }
                if kept + bytes.len() > MAX_KEPT {
                    // Keep less of the recent appends; the stream itself continues.
                    log.recent.clear();
                    log.recent_bytes = 0;
                }
                if log.init.contains_key(&source) {
                    log.recent_bytes += bytes.len();
                    log.recent
                        .push_back((source.clone(), offset, bytes.clone()));
                    while log.recent.len() > KEEP_APPENDS || log.recent_bytes > KEEP_BYTES {
                        if let Some((_, _, old)) = log.recent.pop_front() {
                            log.recent_bytes -= old.len();
                        }
                    }
                } else {
                    log.init.insert(source.clone(), (offset, bytes.clone()));
                }
                let size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
                data = Some(bytes);
                RedirectEvent::Append {
                    media,
                    source,
                    offset,
                    size,
                }
            }
            PlayerMessage::ChangeType {
                tab,
                media,
                source,
                mime,
            } => {
                let media = key(browser, tab, &media);
                let log = inner.medias.entry(media.clone()).or_default();
                // A later channel gets the new type and no segment of the old one.
                if let Some(entry) = log.sources.iter_mut().find(|(id, _)| *id == source) {
                    entry.1.clone_from(&mime);
                }
                log.init.remove(&source);
                log.recent.retain(|(id, _, _)| *id != source);
                log.recent_bytes = log.recent.iter().map(|(_, _, data)| data.len()).sum();
                RedirectEvent::ChangeType {
                    media,
                    source,
                    mime,
                }
            }
            PlayerMessage::Remove {
                tab,
                media,
                source,
                start,
                end,
            } => RedirectEvent::Remove {
                media: key(browser, tab, &media),
                source,
                start,
                end: end.unwrap_or(f64::MAX),
            },
            PlayerMessage::Abort { tab, media, source } => RedirectEvent::Abort {
                media: key(browser, tab, &media),
                source,
            },
            PlayerMessage::Duration { tab, media, value } => {
                let media = key(browser, tab, &media);
                inner.medias.entry(media.clone()).or_default().duration = Some(value);
                RedirectEvent::Duration { media, value }
            }
            PlayerMessage::End { tab, media } => {
                let media = key(browser, tab, &media);
                inner.medias.entry(media.clone()).or_default().ended = true;
                RedirectEvent::End { media }
            }
            PlayerMessage::State {
                tab,
                media,
                time,
                paused,
                rate,
                rect,
                viewport,
                zoom,
                title,
                visible,
            } => {
                let media = key(browser, tab, &media);
                let event = RedirectEvent::State {
                    media: media.clone(),
                    time,
                    paused,
                    rate,
                    rect,
                    viewport,
                    zoom,
                    title,
                    visible,
                    windows,
                };
                // A state of a media source with no SourceBuffer yet: nothing to show.
                match inner.medias.get_mut(&media) {
                    Some(log) => log.state = Some(event.clone()),
                    None => return Ok(()),
                }
                event
            }
            PlayerMessage::Gone { tab, media } => {
                let media = key(browser, tab, &media);
                if inner.medias.remove(&media).is_none() {
                    return Ok(());
                }
                RedirectEvent::Gone { media }
            }
            PlayerMessage::TabClosed { tab } => {
                drop(inner);
                self.remove_prefix(&format!("{browser}:{tab}:"));
                return Ok(());
            }
        };
        let viewers: Vec<Sink> = inner.viewers.values().cloned().collect();
        drop(inner);
        for send in viewers {
            send(&event, data.as_ref());
        }
        Ok(())
    }

    /// Takes a request of the shell: tells the browser of the media source if the client shows it.
    pub fn request(&self, request: RedirectRequest) {
        let RedirectRequest::Showing { media, showing } = request;
        let mut parts = media.splitn(3, ':');
        let (Some(browser), Some(tab), Some(local)) = (
            parts.next().and_then(|b| b.parse::<u64>().ok()),
            parts.next().and_then(|t| t.parse::<i64>().ok()),
            parts.next(),
        ) else {
            return;
        };
        if let Some(writer) = self.lock().browsers.get(&browser) {
            let _ = writer.send(frame(&serde_json::json!({
                "type": "showing",
                "tab": tab,
                "media": local,
                "showing": showing,
            })));
        }
    }

    /// Removes each media source of a browser connection that closed.
    fn browser_gone(&self, browser: u64) {
        {
            let mut inner = self.lock();
            inner.browsers.remove(&browser);
            inner.pids.remove(&browser);
        }
        self.remove_prefix(&format!("{browser}:"));
    }

    /// Removes each media source whose key starts with `prefix`, and tells each channel.
    fn remove_prefix(&self, prefix: &str) {
        let mut inner = self.lock();
        let gone: Vec<String> = inner
            .medias
            .keys()
            .filter(|media| media.starts_with(prefix))
            .cloned()
            .collect();
        for media in &gone {
            inner.medias.remove(media);
        }
        let viewers: Vec<Sink> = inner.viewers.values().cloned().collect();
        drop(inner);
        for media in gone {
            let event = RedirectEvent::Gone { media };
            for send in &viewers {
                send(&event, None);
            }
        }
    }

    /// Listens on the socket `path` for native messaging hosts. A stale socket file goes first.
    ///
    /// # Errors
    ///
    /// An I/O error if the socket does not open.
    pub fn listen(self: &Arc<Self>, path: &Path) -> io::Result<()> {
        if path.exists() {
            if std::os::unix::net::UnixStream::connect(path).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "a different agent listens on the redirect socket",
                ));
            }
            std::fs::remove_file(path)?;
        }
        let listener = tokio::net::UnixListener::bind(path)?;
        let redirect = self.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let redirect = redirect.clone();
                tokio::spawn(async move { redirect.serve(stream).await });
            }
        });
        Ok(())
    }

    /// Serves one native messaging host until it closes.
    async fn serve(self: Arc<Self>, stream: tokio::net::UnixStream) {
        let browser = self.next.fetch_add(1, Ordering::Relaxed);
        // The native messaging host is a child of the host browser: its parent owns the windows (F052).
        match stream
            .peer_cred()
            .ok()
            .and_then(|cred| cred.pid())
            .and_then(|pid| parent_pid(u32::try_from(pid).ok()?))
        {
            Some(pid) => self.set_browser_pid(browser, pid),
            None => tracing::warn!(
                browser,
                "the host browser of a native messaging host is not known: its videos show on no window"
            ),
        }
        let (mut reader, mut writer) = stream.into_split();
        let (out, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        self.lock().browsers.insert(browser, out);
        let write = tokio::spawn(async move {
            while let Some(bytes) = out_rx.recv().await {
                if writer.write_all(&bytes).await.is_err() {
                    break;
                }
            }
        });
        tracing::info!(browser, "a host browser connected for video redirection");
        loop {
            let mut length = [0_u8; 4];
            if reader.read_exact(&mut length).await.is_err() {
                break;
            }
            let length = u32::from_ne_bytes(length) as usize;
            if length > MAX_MESSAGE {
                tracing::warn!(length, "a message of the host browser is too large");
                break;
            }
            let mut text = vec![0; length];
            if reader.read_exact(&mut text).await.is_err() {
                break;
            }
            match serde_json::from_slice::<PlayerMessage>(&text) {
                Ok(message) => {
                    if let Err(error) = self.take(browser, message) {
                        refused(&error);
                    }
                }
                Err(error) => tracing::debug!(%error, "an unknown message of the host browser"),
            }
        }
        write.abort();
        self.browser_gone(browser);
        tracing::info!(browser, "a host browser left");
    }
}

/// Logs a refused message of a host browser in the form of the security log (ADR-043).
///
/// The agent does not link farglass-login, thus it writes the fields itself.
fn refused(error: &str) {
    tracing::warn!(
        target: "farglass::security",
        "event=redirect result=refused reason=message addr=- user=- uid={} detail={error:?}",
        nix::unistd::getuid()
    );
}

/// Returns the parent process of `pid`, from `/proc/PID/stat`.
fn parent_pid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name is in parentheses and can have spaces: the fields start after the last ')'.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(1)?.parse().ok()
}

/// Runs the native messaging host: copies stdin to the socket of the agent and the socket to stdout, until one side closes.
///
/// # Errors
///
/// An I/O error if the agent does not listen.
pub fn native_host(socket: &Path) -> io::Result<()> {
    let stream = std::os::unix::net::UnixStream::connect(socket)?;
    let mut from_agent = stream.try_clone()?;
    let back = std::thread::spawn(move || {
        let mut stdout = io::stdout().lock();
        let mut buffer = [0_u8; 64 * 1024];
        while let Ok(n) = from_agent.read(&mut buffer) {
            if n == 0 || stdout.write_all(&buffer[..n]).is_err() || stdout.flush().is_err() {
                break;
            }
        }
    });
    let mut to_agent = stream;
    io::copy(&mut io::stdin().lock(), &mut to_agent)?;
    let _ = to_agent.shutdown(std::net::Shutdown::Both);
    let _ = back.join();
    Ok(())
}

/// Writes the manifest of the native messaging host for each Chromium browser of the user:
/// Chromium, Google Chrome, and Brave. `program` is this program.
///
/// # Errors
///
/// An I/O error if a manifest does not write.
pub fn install_manifests(program: &Path) -> io::Result<()> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or_else(|| io::Error::other("no HOME"))?;
    let manifest = manifest(program);
    for browser in ["chromium", "google-chrome", "BraveSoftware/Brave-Browser"] {
        let dir = config.join(browser).join("NativeMessagingHosts");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(format!("{NATIVE_HOST}.json")), &manifest)?;
    }
    Ok(())
}

/// Returns the manifest of the native messaging host for `program`.
#[must_use]
pub fn manifest(program: &Path) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "name": NATIVE_HOST,
        "description": "farglass video redirection",
        "path": program,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{EXTENSION_ID}/")],
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> RedirectRect {
        RedirectRect {
            x: 0.0,
            y: 0.0,
            width: 640.0,
            height: 360.0,
        }
    }

    fn append(source: &str, data: &[u8]) -> PlayerMessage {
        PlayerMessage::Append {
            tab: 7,
            media: "m1".into(),
            source: source.into(),
            offset: 0.0,
            data: base64::engine::general_purpose::STANDARD.encode(data),
        }
    }

    #[test]
    fn the_calls_of_a_page_go_to_each_channel_and_a_late_channel_gets_the_init_and_recent_appends()
    {
        let redirect = Redirect::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let _first = redirect.add_viewer(move |event, data| {
            sink.lock()
                .unwrap()
                .push((event.clone(), data.map(|d| d.to_vec())));
        });
        let message: PlayerMessage = serde_json::from_str(
            r#"{"type":"source","tab":7,"media":"m1","source":"s0","mime":"video/webm; codecs=\"vp9\""}"#,
        )
        .unwrap();
        redirect.take(3, message).unwrap();
        redirect.take(3, append("s0", b"init")).unwrap();
        redirect.take(3, append("s0", b"media 1")).unwrap();
        redirect
            .take(
                3,
                PlayerMessage::State {
                    tab: 7,
                    media: "m1".into(),
                    time: 1.5,
                    paused: false,
                    rate: 1.0,
                    rect: rect(),
                    viewport: rect(),
                    zoom: 1.0,
                    title: "A video".into(),
                    visible: true,
                },
            )
            .unwrap();
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 4);
        assert!(matches!(&seen[0].0, RedirectEvent::Source { media, .. } if media == "3:7:m1"));
        assert_eq!(seen[2].1.as_deref(), Some(&b"media 1"[..]));

        let late = Arc::new(Mutex::new(Vec::new()));
        let sink = late.clone();
        let _second = redirect.add_viewer(move |event, data| {
            sink.lock()
                .unwrap()
                .push((event.clone(), data.map(|d| d.to_vec())));
        });
        let late = late.lock().unwrap().clone();
        assert_eq!(
            late.len(),
            4,
            "the source, the init, the media, and the state"
        );
        assert_eq!(late[1].1.as_deref(), Some(&b"init"[..]));
        assert!(matches!(late[3].0, RedirectEvent::State { .. }));

        redirect.browser_gone(3);
        assert!(redirect.lock().medias.is_empty());
    }

    #[test]
    fn showing_goes_to_the_browser_of_the_media_with_its_own_names() {
        let redirect = Redirect::new();
        let (out, mut rx) = mpsc::unbounded_channel();
        redirect.lock().browsers.insert(3, out);
        redirect.request(RedirectRequest::Showing {
            media: "3:7:m1".into(),
            showing: true,
        });
        let bytes = rx.try_recv().unwrap();
        let length = u32::from_ne_bytes(bytes[..4].try_into().unwrap()) as usize;
        let json: serde_json::Value = serde_json::from_slice(&bytes[4..]).unwrap();
        assert_eq!(length, bytes.len() - 4);
        assert_eq!(
            json,
            serde_json::json!({"type": "showing", "tab": 7, "media": "m1", "showing": true})
        );
    }

    #[test]
    fn the_manifest_names_this_program_and_the_extension() {
        let json: serde_json::Value = serde_json::from_str(&manifest(Path::new(
            "/usr/libexec/farglass/farglass-session",
        )))
        .unwrap();
        assert_eq!(json["name"], NATIVE_HOST);
        assert_eq!(
            json["allowed_origins"][0],
            format!("chrome-extension://{EXTENSION_ID}/")
        );
    }

    fn source(media: &str, source: &str) -> PlayerMessage {
        PlayerMessage::Source {
            tab: 7,
            media: media.into(),
            source: source.into(),
            mime: "video/webm".into(),
        }
    }

    fn state() -> PlayerMessage {
        PlayerMessage::State {
            tab: 7,
            media: "m1".into(),
            time: 0.0,
            paused: false,
            rate: 1.0,
            rect: rect(),
            viewport: rect(),
            zoom: 1.0,
            title: "A video".into(),
            visible: true,
        }
    }

    #[test]
    fn a_browser_connection_has_caps_for_medias_sources_and_init_segments() {
        let redirect = Redirect::new();
        for i in 0..MAX_MEDIAS {
            redirect.take(3, source(&format!("m{i}"), "s0")).unwrap();
        }
        assert!(redirect.take(3, source("one-more", "s0")).is_err());
        // A different browser connection has its own caps.
        redirect.take(4, source("m0", "s0")).unwrap();
        for i in 1..MAX_SOURCES {
            redirect.take(3, source("m0", &format!("s{i}"))).unwrap();
        }
        assert!(redirect.take(3, source("m0", "s-more")).is_err());

        assert!(
            redirect.take(3, append("unknown", b"x")).is_err(),
            "an append needs a SourceBuffer of the page"
        );
        let huge = vec![0_u8; MAX_INIT + 1];
        let message = PlayerMessage::Append {
            tab: 7,
            media: "m1".into(),
            source: "s0".into(),
            offset: 0.0,
            data: base64::engine::general_purpose::STANDARD.encode(&huge),
        };
        assert!(redirect.take(3, message).is_err());
        assert!(redirect.lock().medias["3:7:m1"].init.is_empty());
    }

    #[test]
    fn a_state_names_only_the_windows_of_the_browser_process() {
        let redirect = Redirect::new();
        redirect.set_windows(Arc::new(
            |pid| if pid == 500 { vec![11, 12] } else { vec![99] },
        ));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let _viewer = redirect.add_viewer(move |event, _| sink.lock().unwrap().push(event.clone()));
        redirect.take(3, source("m1", "s0")).unwrap();
        redirect.take(3, state()).unwrap();
        // An unknown browser process: no window.
        assert!(
            matches!(seen.lock().unwrap().last(), Some(RedirectEvent::State { windows, .. }) if windows.is_empty())
        );
        redirect.set_browser_pid(3, 500);
        redirect.take(3, state()).unwrap();
        assert!(
            matches!(seen.lock().unwrap().last(), Some(RedirectEvent::State { windows, .. }) if *windows == [11, 12])
        );
    }

    #[test]
    fn the_parent_of_this_process_is_known() {
        let me = std::process::id();
        assert_eq!(parent_pid(me), Some(std::os::unix::process::parent_id()));
        assert_eq!(parent_pid(u32::MAX), None);
    }
}
