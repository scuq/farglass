//! The session worker: it opens a PAM session of type web for a user and runs the agent in it.
//!
//! farglass-login starts the worker as a transient systemd unit with
//! `farglass-login --session-worker USER`. Thus the worker does not get the
//! sandbox of farglass-login, and continues when farglass-login restarts.
//! The code comes from `rcd/crates/rcdd/src/worker.rs`.

use std::ffi::CString;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::Command;

use nix::sys::signal::{SigHandler, Signal, signal};
use nix::unistd::{Gid, Uid};

use crate::pam;

/// The environment that pam_systemd reads for the new logind session.
const SESSION_ENV: [(&str, &str); 3] = [
    ("XDG_SESSION_TYPE", "web"),
    ("XDG_SESSION_CLASS", "user"),
    ("XDG_SESSION_DESKTOP", "farglass"),
];

/// Opens the session of `user` and runs the agent until it stops. Then closes the session.
///
/// # Errors
///
/// A text with the cause if the user does not exist, the PAM session does not open,
/// or the agent does not start.
pub fn run(
    user: &str,
    service: &str,
    pam_dir: Option<&Path>,
    agent_program: &Path,
    agent_socket: &Path,
) -> Result<(), String> {
    let account = nix::unistd::User::from_name(user)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("the user {user} does not exist"))?;
    let uid = account.uid.as_raw();

    let session = pam::open_session(service, pam_dir, user, &SESSION_ENV)?;
    let pam_env = session.env();
    let id = pam_env
        .iter()
        .find(|(key, _)| key == "XDG_SESSION_ID")
        .map_or("unknown", |(_, value)| value.as_str());
    tracing::info!(user, session = id, "the web session is open");

    // The worker must close the PAM session after the agent stops. Thus it ignores
    // SIGTERM and SIGHUP. The agent gets the default action back before it starts.
    // SAFETY: SigIgn is not a function. No other thread of the worker uses signals.
    unsafe {
        signal(Signal::SIGTERM, SigHandler::SigIgn).map_err(|error| error.to_string())?;
        signal(Signal::SIGHUP, SigHandler::SigIgn).map_err(|error| error.to_string())?;
    }

    let mut command = Command::new(agent_program);
    command
        .env_clear()
        .env("HOME", &account.dir)
        .env("USER", user)
        .env("LOGNAME", user)
        .env("SHELL", &account.shell)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("XDG_RUNTIME_DIR", format!("/run/user/{uid}"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path=/run/user/{uid}/bus"),
        )
        .envs(SESSION_ENV)
        .envs(
            pam_env
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
        .env("FARGLASS_AGENT_SOCKET", agent_socket)
        .current_dir(&account.dir);
    let name = CString::new(user).map_err(|error| error.to_string())?;
    let gid = account.gid;
    // SAFETY: The closure runs in the child after fork and before exec. It calls only
    // functions that are safe after fork: initgroups, setgid, setuid, and signal.
    unsafe {
        command.pre_exec(move || {
            signal(Signal::SIGTERM, SigHandler::SigDfl).map_err(std::io::Error::from)?;
            signal(Signal::SIGHUP, SigHandler::SigDfl).map_err(std::io::Error::from)?;
            nix::unistd::initgroups(&name, gid).map_err(std::io::Error::from)?;
            nix::unistd::setgid(Gid::from_raw(gid.as_raw())).map_err(std::io::Error::from)?;
            nix::unistd::setuid(Uid::from_raw(uid)).map_err(std::io::Error::from)?;
            Ok(())
        });
    }
    let status = command.status().map_err(|error| {
        format!(
            "the agent {} did not start: {error}",
            agent_program.display()
        )
    })?;
    tracing::info!(user, session = id, %status, "the agent stopped, closing the web session");
    drop(session);
    Ok(())
}
