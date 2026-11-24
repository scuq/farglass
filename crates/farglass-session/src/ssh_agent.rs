//! The ssh-agent of the web session (F093).
//!
//! The agent of farglass starts one `ssh-agent` for the web session, with its socket next to
//! the Wayland socket. Terminals, apps, the systemd user manager, and D-Bus activation get
//! `SSH_AUTH_SOCK`. The ssh-agent runs as long as the agent of farglass, thus loaded keys stay
//! while the browser connects again. The end of the web session stops the ssh-agent with it.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The time for a new ssh-agent to open its socket.
const START_WAIT: Duration = Duration::from_secs(5);

/// The socket of the ssh-agent of this session, after [`SshAgent::publish`].
static SOCKET: OnceLock<PathBuf> = OnceLock::new();

/// Returns the socket of the ssh-agent of this session, if it runs.
#[must_use]
pub fn socket() -> Option<&'static Path> {
    SOCKET.get().map(PathBuf::as_path)
}

/// Returns the socket of the ssh-agent for the Wayland socket `wayland`: a file next to it.
#[must_use]
pub fn socket_for(wayland: &Path) -> PathBuf {
    let mut name = wayland.file_name().unwrap_or_default().to_os_string();
    name.push("-ssh-agent");
    wayland.with_file_name(name)
}

/// A running ssh-agent. Dropping it stops its command, thus the ssh-agent, and removes the socket.
#[derive(Debug)]
pub struct SshAgent {
    child: Child,
    socket: PathBuf,
}

impl SshAgent {
    /// Starts `ssh-agent` on `socket` and waits until the socket exists.
    ///
    /// ssh-agent runs a command as its parent and stops 10 s after that command stops. The
    /// command is `sleep infinity` with SIGKILL at the end of this thread, through `setpriv` if it
    /// is installed. Thus the ssh-agent also stops after a crash of the agent of farglass. Call it
    /// from the main thread. ssh-agent itself cannot take that signal: Debian installs it setgid,
    /// and the start of a setgid program clears the signal.
    ///
    /// # Errors
    ///
    /// An I/O error if ssh-agent does not start or does not open the socket in 5 s.
    pub fn start(socket: &Path) -> io::Result<Self> {
        // A socket of an earlier ssh-agent that stopped is in the way of the new one.
        match std::fs::remove_file(socket) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let mut command = Command::new("ssh-agent");
        command.arg("-a").arg(socket);
        if in_path("setpriv") {
            command.args(["setpriv", "--pdeathsig", "KILL", "--"]);
        }
        let mut child = command
            .args(["sleep", "infinity"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let start = Instant::now();
        while !socket.exists() {
            if let Some(status) = child.try_wait()? {
                return Err(io::Error::other(format!("ssh-agent stopped: {status}")));
            }
            if start.elapsed() > START_WAIT {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::other("ssh-agent did not open its socket in 5 s"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(Self {
            child,
            socket: socket.to_owned(),
        })
    }

    /// Returns the socket of this ssh-agent.
    #[must_use]
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// Makes this ssh-agent the one of the session: [`socket`] returns its socket.
    /// Only the first call has an effect.
    pub fn publish(&self) {
        let _ = SOCKET.set(self.socket.clone());
    }
}

impl Drop for SshAgent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// Removes all keys from the ssh-agent at `socket`.
///
/// # Errors
///
/// An I/O error if ssh-add does not start or fails.
pub fn clear(socket: &Path) -> io::Result<()> {
    let status = Command::new("ssh-add")
        .arg("-D")
        .env("SSH_AUTH_SOCK", socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("ssh-add -D failed: {status}")))
    }
}

/// Removes the keys of the ssh-agent of the session after a lock, if the configuration asks for it.
pub fn clear_on_lock() {
    let Some(socket) = socket() else {
        return;
    };
    if !crate::desktop::ssh_agent_clear_on_lock() {
        return;
    }
    tokio::task::spawn_blocking(move || match clear(socket) {
        Ok(()) => tracing::info!("the session locked: the ssh-agent has no keys now"),
        Err(error) => {
            tracing::warn!(%error, "the session locked, but the keys of the ssh-agent stay")
        }
    });
}

/// Returns `true` if `program` is a file in a directory of `PATH`.
fn in_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(socket: &Path) -> std::process::Output {
        Command::new("ssh-add")
            .arg("-l")
            .env("SSH_AUTH_SOCK", socket)
            .output()
            .unwrap()
    }

    #[test]
    fn the_socket_is_next_to_the_wayland_socket() {
        assert_eq!(
            socket_for(Path::new("/run/user/1000/farglass-0")),
            Path::new("/run/user/1000/farglass-0-ssh-agent")
        );
    }

    #[test]
    fn an_ssh_agent_takes_a_key_and_clear_removes_it() {
        if !in_path("ssh-agent") || !in_path("ssh-keygen") {
            eprintln!("SKIP: no ssh-agent or ssh-keygen");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("wayland-0-ssh-agent");
        // A stale socket file does not stop the start.
        std::fs::write(&socket, b"").unwrap();
        let agent = SshAgent::start(&socket).unwrap();
        let key = dir.path().join("id_ed25519");
        let made = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&key)
            .status()
            .unwrap();
        assert!(made.success());
        let added = Command::new("ssh-add")
            .arg(&key)
            .env("SSH_AUTH_SOCK", agent.socket())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(added.success());
        assert!(String::from_utf8_lossy(&keys(&socket).stdout).contains("ED25519"));
        clear(&socket).unwrap();
        // ssh-add -l exits with 1 for an agent with no keys.
        assert_eq!(keys(&socket).status.code(), Some(1));
        drop(agent);
        assert!(!socket.exists());
    }
}
