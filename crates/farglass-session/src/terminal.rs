//! The terminals of the session.
//!
//! A terminal is a shell of the user on a pty. It continues when no channel is
//! attached: jobs continue, and the output goes to the scrollback. A channel
//! that attaches gets the scrollback first, then the live output.
//!
//! Each terminal is one task. It reads the pty only while each attached
//! channel has less than [`PTY_WINDOW`] bytes that the shell did not
//! acknowledge. Thus a fast program waits for a slow browser, and the queues
//! between the agent and the browser stay small.

use std::collections::{HashMap, VecDeque};
use std::io::{self, Read as _};
use std::os::unix::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use bytes::Bytes;
use farglass_proto::{PTY_SCROLLBACK, PTY_WINDOW};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::process::Child;
use tokio::sync::mpsc;

/// The largest number of terminals that run at the same time.
pub const MAX_TERMINALS: usize = 64;

/// The size of one read from the pty, and thus the largest data frame of output.
const READ_SIZE: usize = 32 * 1024;

/// After the program exited, the agent waits this long for more output.
const DRAIN_TIME: Duration = Duration::from_millis(100);

/// After SIGHUP, the agent waits this long before SIGKILL.
const KILL_DELAY: Duration = Duration::from_secs(3);

/// A terminal whose program exited with no attached channel stays this long, for its scrollback.
const EXITED_KEEP: Duration = Duration::from_secs(24 * 60 * 60);

/// The id of one attached channel. Each attach gets a new id.
pub type ViewerId = u64;

/// What a terminal sends to an attached channel.
#[derive(Debug)]
pub enum Output {
    /// The channel is attached. `replay` is the scrollback.
    Attached {
        /// Reports whether the terminal is new.
        new: bool,
        /// The scrollback, in sequence.
        replay: Vec<Bytes>,
    },
    /// Live output.
    Data(Bytes),
    /// The program stopped. The terminal sends nothing more to this channel.
    Exited {
        /// The exit code.
        code: Option<i32>,
        /// The signal.
        signal: Option<i32>,
    },
}

/// The queue from the terminals to the link. Each item names its channel.
pub type Sink = mpsc::UnboundedSender<(ViewerId, Output)>;

/// How the agent starts a terminal.
#[derive(Clone, Debug)]
pub struct TerminalOptions {
    /// Start each terminal in its own systemd --user scope.
    pub scope: bool,
    /// The program of the terminal. `None` is the login shell of the user.
    pub shell: Option<PathBuf>,
    /// The Wayland socket of the session. A terminal gets it as `WAYLAND_DISPLAY` if it exists.
    pub wayland: Option<PathBuf>,
}

impl TerminalOptions {
    /// Returns the options for this process. The scope needs a systemd user
    /// manager. `FARGLASS_TERMINAL_SCOPE=0` turns it off.
    #[must_use]
    pub fn from_env() -> Self {
        let wanted = std::env::var_os("FARGLASS_TERMINAL_SCOPE").is_none_or(|value| value != "0");
        let bus = std::env::var_os("XDG_RUNTIME_DIR")
            .map(|dir| PathBuf::from(dir).join("systemd/private"))
            .is_some_and(|path| path.exists());
        Self {
            scope: wanted && bus,
            shell: None,
            wayland: crate::surfaces::socket_path(),
        }
    }
}

/// A command from a channel to its terminal.
#[derive(Debug)]
enum Command {
    Attach {
        viewer: ViewerId,
        sink: Sink,
        new: bool,
        rows: u16,
        cols: u16,
    },
    Detach(ViewerId),
    Input(Bytes),
    Resize {
        rows: u16,
        cols: u16,
    },
    Ack {
        viewer: ViewerId,
        bytes: u32,
    },
    Terminate,
}

/// An error of an attach.
#[derive(Debug, thiserror::Error)]
pub enum AttachError {
    /// The agent runs [`MAX_TERMINALS`] terminals.
    #[error("the session has {MAX_TERMINALS} terminals, close one first")]
    Limit,
    /// The terminal did not start.
    #[error("the terminal did not start: {0}")]
    Spawn(#[from] io::Error),
}

/// The terminals of the session. They live as long as the agent, also when
/// the link to farglassd breaks.
#[derive(Debug)]
pub struct Terminals {
    options: TerminalOptions,
    tasks: HashMap<String, mpsc::UnboundedSender<Command>>,
    /// The process of the program of each terminal, by terminal id (F045).
    pids: Arc<Mutex<HashMap<String, u32>>>,
}

static NEXT_VIEWER: AtomicU64 = AtomicU64::new(1);

impl Terminals {
    /// Returns a set of terminals with `options` and no terminal.
    #[must_use]
    pub fn new(options: TerminalOptions) -> Self {
        Self {
            options,
            tasks: HashMap::new(),
            pids: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Attaches a channel to the terminal `id`, or starts the terminal.
    ///
    /// The terminal sends its output to `sink`. Dropping the returned link detaches the channel.
    ///
    /// # Errors
    ///
    /// [`AttachError`] if the terminal does not start or the session has too many terminals.
    pub fn attach(
        &mut self,
        id: &str,
        rows: u16,
        cols: u16,
        sink: Sink,
    ) -> Result<TerminalLink, AttachError> {
        self.tasks.retain(|_, task| !task.is_closed());
        let viewer = NEXT_VIEWER.fetch_add(1, Ordering::Relaxed);
        let (rows, cols) = (rows.max(1), cols.max(1));

        if let Some(task) = self.tasks.get(id) {
            let attach = Command::Attach {
                viewer,
                sink: sink.clone(),
                new: false,
                rows,
                cols,
            };
            if task.send(attach).is_ok() {
                return Ok(TerminalLink {
                    viewer,
                    task: task.clone(),
                });
            }
        }

        if self.tasks.len() >= MAX_TERMINALS {
            return Err(AttachError::Limit);
        }
        let (child, pty, unit) = spawn(&self.options, rows, cols)?;
        if let Some(pid) = child.id() {
            self.pids
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id.to_owned(), pid);
        }
        let (task, commands) = mpsc::unbounded_channel();
        tokio::spawn(run(child, pty, unit, commands));
        let _ = task.send(Command::Attach {
            viewer,
            sink,
            new: true,
            rows,
            cols,
        });
        self.tasks.insert(id.to_owned(), task.clone());
        Ok(TerminalLink { viewer, task })
    }

    /// Returns the process of the program of each terminal, by terminal id. The map stays current.
    #[must_use]
    pub fn pids(&self) -> Arc<Mutex<HashMap<String, u32>>> {
        self.pids.clone()
    }

    /// Returns the number of terminals that run or keep their scrollback.
    #[must_use]
    pub fn len(&mut self) -> usize {
        self.tasks.retain(|_, task| !task.is_closed());
        let open: Vec<String> = self.tasks.keys().cloned().collect();
        self.pids
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|id, _| open.contains(id));
        self.tasks.len()
    }

    /// Reports whether no terminal runs.
    #[must_use]
    pub fn is_empty(&mut self) -> bool {
        self.len() == 0
    }
}

/// One channel that is attached to a terminal. Dropping it detaches the channel.
#[derive(Debug)]
pub struct TerminalLink {
    viewer: ViewerId,
    task: mpsc::UnboundedSender<Command>,
}

impl TerminalLink {
    /// Returns the id of the channel for the output of the terminal.
    #[must_use]
    pub const fn viewer(&self) -> ViewerId {
        self.viewer
    }

    /// Writes input to the terminal.
    pub fn input(&self, bytes: Bytes) {
        let _ = self.task.send(Command::Input(bytes));
    }

    /// Changes the size of the terminal.
    pub fn resize(&self, rows: u16, cols: u16) {
        let _ = self.task.send(Command::Resize {
            rows: rows.max(1),
            cols: cols.max(1),
        });
    }

    /// Acknowledges output bytes that the shell processed.
    pub fn ack(&self, bytes: u32) {
        let _ = self.task.send(Command::Ack {
            viewer: self.viewer,
            bytes,
        });
    }

    /// Stops the programs of the terminal.
    pub fn terminate(&self) {
        let _ = self.task.send(Command::Terminate);
    }
}

impl Drop for TerminalLink {
    fn drop(&mut self) {
        let _ = self.task.send(Command::Detach(self.viewer));
    }
}

/// The last output of a terminal, at most [`PTY_SCROLLBACK`] bytes.
#[derive(Debug, Default)]
struct Scrollback {
    chunks: VecDeque<Bytes>,
    len: usize,
}

impl Scrollback {
    fn push(&mut self, chunk: Bytes) {
        self.len += chunk.len();
        self.chunks.push_back(chunk);
        while self.len > PTY_SCROLLBACK as usize {
            let Some(old) = self.chunks.pop_front() else {
                break;
            };
            self.len -= old.len();
        }
    }

    fn snapshot(&self) -> Vec<Bytes> {
        self.chunks.iter().cloned().collect()
    }
}

/// One attached channel.
#[derive(Debug)]
struct Viewer {
    id: ViewerId,
    sink: Sink,
    /// The output bytes that the shell did not acknowledge.
    unacked: usize,
}

/// An operation of the writer task.
#[derive(Debug)]
enum Write {
    Input(Bytes),
    Resize { rows: u16, cols: u16 },
}

/// Starts the program of a terminal on a new pty.
fn spawn(
    options: &TerminalOptions,
    rows: u16,
    cols: u16,
) -> io::Result<(Child, pty_process::Pty, Option<String>)> {
    let (pty, pts) = pty_process::open().map_err(io::Error::other)?;
    pty.resize(pty_process::Size::new(rows, cols))
        .map_err(io::Error::other)?;

    let account = nix::unistd::User::from_uid(nix::unistd::getuid())
        .ok()
        .flatten();
    let shell = options
        .shell
        .clone()
        .or_else(|| account.as_ref().map(|user| user.shell.clone()))
        .filter(|shell| !shell.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("/bin/sh"));
    let home = account.map_or_else(|| PathBuf::from("/"), |user| user.dir);

    let unit = options
        .scope
        .then(|| format!("farglass-terminal-{}", random_hex()));
    let command = match &unit {
        Some(unit) => pty_process::Command::new("systemd-run")
            .args(["--user", "--scope", "--quiet", "--collect"])
            .arg(format!("--unit={unit}"))
            .arg("--description=farglass terminal")
            .arg("--")
            .arg(&shell)
            .arg("-l"),
        None => pty_process::Command::new(&shell).arg("-l"),
    };
    // The Wayland apps of a terminal show in the shell (F024).
    let command = match options.wayland.as_deref().filter(|path| path.exists()) {
        // A terminal of a local desktop must not give the display of that desktop to its apps.
        Some(socket) => command
            .env_remove("DISPLAY")
            .envs(crate::desktop::variables(socket)),
        None => command,
    };
    let child = command
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env_remove("FARGLASS_AGENT_SOCKET")
        .current_dir(home)
        .spawn(pts)
        .map_err(io::Error::other)?;
    tracing::info!(shell = %shell.display(), scope = ?unit, pid = child.id(), "started a terminal");
    Ok((child, pty, unit))
}

/// Returns 8 random bytes as hexadecimal, for the name of a scope.
pub(crate) fn random_hex() -> String {
    let mut bytes = [0_u8; 8];
    if let Ok(mut random) = std::fs::File::open("/dev/urandom") {
        let _ = random.read_exact(&mut bytes);
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Stops the programs of a terminal with SIGHUP, as a terminal hang-up does.
///
/// With a scope, the signal goes to each process of the scope, also to jobs in the
/// background. With no scope, it goes to the process group of the program.
/// SIGKILL follows if the programs are still there after [`KILL_DELAY`].
async fn terminate(pid: Option<u32>, unit: Option<&str>) {
    if let Some(unit) = unit {
        let scope = format!("{unit}.scope");
        if kill_scope(&scope, "SIGHUP").await {
            tokio::spawn(async move {
                tokio::time::sleep(KILL_DELAY).await;
                kill_scope(&scope, "SIGKILL").await;
            });
            return;
        }
    }
    let Some(pid) = pid.and_then(|pid| i32::try_from(pid).ok()) else {
        return;
    };
    // The program is the leader of its own session, thus its pid is the process group.
    let group = Pid::from_raw(pid);
    let _ = killpg(group, Signal::SIGHUP);
    tokio::spawn(async move {
        tokio::time::sleep(KILL_DELAY).await;
        let _ = killpg(group, Signal::SIGKILL);
    });
}

/// Sends `signal` to each process of the systemd user scope `scope`. Reports whether systemctl succeeded.
async fn kill_scope(scope: &str, signal: &str) -> bool {
    tokio::process::Command::new("systemctl")
        .args(["--user", "kill", "--kill-whom=all"])
        .arg(format!("--signal={signal}"))
        .arg(scope)
        .status()
        .await
        .is_ok_and(|status| status.success())
}

/// The task of one terminal.
async fn run(
    mut child: Child,
    pty: pty_process::Pty,
    unit: Option<String>,
    mut commands: mpsc::UnboundedReceiver<Command>,
) {
    let pid = child.id();
    let (mut reader, mut writer) = pty.into_split();

    // A separate writer: a program that does not read its input cannot stop the output.
    let (writes, mut pending) = mpsc::unbounded_channel::<Write>();
    tokio::spawn(async move {
        while let Some(write) = pending.recv().await {
            match write {
                Write::Input(bytes) => {
                    if writer.write_all(&bytes).await.is_err() {
                        break;
                    }
                }
                Write::Resize { rows, cols } => {
                    let _ = writer.resize(pty_process::Size::new(rows, cols));
                }
            }
        }
    });

    let mut scrollback = Scrollback::default();
    let mut viewers: Vec<Viewer> = Vec::new();
    let mut exited: Option<(Option<i32>, Option<i32>)> = None;
    let mut eof = false;
    let mut buffer = vec![0_u8; READ_SIZE];
    let keep = tokio::time::sleep(EXITED_KEEP);
    tokio::pin!(keep);

    loop {
        let can_read = !eof
            && viewers
                .iter()
                .all(|viewer| viewer.unacked < PTY_WINDOW as usize);
        tokio::select! {
            command = commands.recv() => match command {
                // The agent stops.
                None => {
                    if exited.is_none() {
                        terminate(pid, unit.as_deref()).await;
                    }
                    return;
                }
                Some(Command::Attach { viewer, sink, new, rows, cols }) => {
                    let replay = scrollback.snapshot();
                    let unacked = replay.iter().map(Bytes::len).sum();
                    let _ = sink.send((viewer, Output::Attached { new, replay }));
                    if let Some((code, signal)) = exited {
                        // The program stopped while no channel was attached. This channel gets
                        // the scrollback and the exit, and the terminal ends.
                        let _ = sink.send((viewer, Output::Exited { code, signal }));
                        return;
                    }
                    viewers.push(Viewer { id: viewer, sink, unacked });
                    let _ = writes.send(Write::Resize { rows, cols });
                }
                Some(Command::Detach(id)) => viewers.retain(|viewer| viewer.id != id),
                Some(Command::Input(bytes)) => {
                    if exited.is_none() {
                        let _ = writes.send(Write::Input(bytes));
                    }
                }
                Some(Command::Resize { rows, cols }) => {
                    let _ = writes.send(Write::Resize { rows, cols });
                }
                Some(Command::Ack { viewer, bytes }) => {
                    if let Some(viewer) = viewers.iter_mut().find(|other| other.id == viewer) {
                        viewer.unacked = viewer.unacked.saturating_sub(bytes as usize);
                    }
                }
                Some(Command::Terminate) => {
                    if exited.is_none() {
                        terminate(pid, unit.as_deref()).await;
                    }
                }
            },
            read = reader.read(&mut buffer), if can_read => match read {
                // EIO: the program and all its children closed the terminal.
                Ok(0) | Err(_) => eof = true,
                Ok(n) => {
                    let chunk = Bytes::copy_from_slice(&buffer[..n]);
                    scrollback.push(chunk.clone());
                    for viewer in &mut viewers {
                        viewer.unacked += n;
                        let _ = viewer.sink.send((viewer.id, Output::Data(chunk.clone())));
                    }
                }
            },
            status = child.wait(), if exited.is_none() => {
                let (code, signal) = status.map_or((None, None), |status| (status.code(), status.signal()));
                exited = Some((code, signal));
                // The last output can come after the exit. Read it for a short time, with no window.
                let drain = tokio::time::sleep(DRAIN_TIME);
                tokio::pin!(drain);
                while !eof {
                    tokio::select! {
                        () = &mut drain => break,
                        read = reader.read(&mut buffer) => match read {
                            Ok(0) | Err(_) => eof = true,
                            Ok(n) => {
                                let chunk = Bytes::copy_from_slice(&buffer[..n]);
                                scrollback.push(chunk.clone());
                                for viewer in &mut viewers {
                                    let _ = viewer.sink.send((viewer.id, Output::Data(chunk.clone())));
                                }
                            }
                        },
                    }
                }
                tracing::info!(?code, ?signal, scope = ?unit, "a terminal stopped");
                if !viewers.is_empty() {
                    for viewer in viewers.drain(..) {
                        let _ = viewer.sink.send((viewer.id, Output::Exited { code, signal }));
                    }
                    return;
                }
                keep.as_mut().reset(tokio::time::Instant::now() + EXITED_KEEP);
            },
            () = &mut keep, if exited.is_some() => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollback_keeps_the_last_bytes() {
        let mut scrollback = Scrollback::default();
        let chunk = Bytes::from(vec![b'x'; 300 * 1024]);
        for _ in 0..5 {
            scrollback.push(chunk.clone());
        }
        assert!(scrollback.len <= PTY_SCROLLBACK as usize);
        assert_eq!(scrollback.snapshot().len(), 3);
    }
}
