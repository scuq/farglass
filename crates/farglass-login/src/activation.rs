//! The socket of farglass-login: from systemd socket activation, or bound by the program.

use std::io;
use std::os::fd::{FromRawFd as _, OwnedFd};
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use tokio::net::UnixListener;

/// The first file descriptor that systemd gives (SD_LISTEN_FDS_START).
const LISTEN_FDS_START: i32 = 3;

/// Returns the socket that systemd gave, or `None` if the process has no socket from systemd.
///
/// # Errors
///
/// An I/O error if the socket from systemd is not usable.
pub fn from_systemd() -> io::Result<Option<UnixListener>> {
    let pid_ok = std::env::var("LISTEN_PID")
        .ok()
        .and_then(|pid| pid.parse::<u32>().ok())
        == Some(std::process::id());
    let count = std::env::var("LISTEN_FDS")
        .ok()
        .and_then(|count| count.parse::<u32>().ok());
    match (pid_ok, count) {
        (true, Some(1)) => {}
        (true, Some(n)) if n > 1 => {
            return Err(io::Error::other(format!(
                "systemd gave {n} sockets, farglass-login uses one"
            )));
        }
        _ => return Ok(None),
    }
    // SAFETY: systemd gives the socket as file descriptor 3 to this process
    // (LISTEN_PID is this process), and no other code uses that descriptor.
    let fd = unsafe { OwnedFd::from_raw_fd(LISTEN_FDS_START) };
    let listener = std::os::unix::net::UnixListener::from(fd);
    listener.set_nonblocking(true)?;
    UnixListener::from_std(listener).map(Some)
}

/// Binds the socket at `path` with the permissions `mode`. Removes an old socket first.
///
/// # Errors
///
/// An I/O error if the socket cannot be created.
pub fn bind(path: &Path, mode: u32) -> io::Result<UnixListener> {
    use std::os::unix::fs::FileTypeExt as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_socket() => std::fs::remove_file(path)?,
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} exists and is not a socket", path.display()),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(listener)
}
