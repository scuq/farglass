//! The actions on the host: the logind sessions of a user and the start of the web session.
//!
//! farglass-login uses the [`Host`] interface. [`Systemd`] does the actions
//! with loginctl and systemd-run. Tests use a different implementation.

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use farglass_proto::login::LocalSession;
use serde::Deserialize;

/// The time for the local sessions to end. After it, the web login continues.
const END_TIMEOUT: Duration = Duration::from_secs(20);

/// The time for a new web session to show in logind.
const START_TIMEOUT: Duration = Duration::from_secs(15);

/// The result of a session start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Started {
    /// A new web session started.
    New,
    /// The user already had a web session. The login attaches to it.
    Attached,
}

/// The actions of farglass-login on the host.
pub trait Host: fmt::Debug + Send + Sync {
    /// Returns the local graphical sessions of `uid`: class user, type x11, wayland, or mir.
    ///
    /// # Errors
    ///
    /// An I/O error if logind does not answer.
    fn local_sessions(&self, uid: u32) -> io::Result<Vec<LocalSession>>;

    /// Ends the sessions `ids` and waits until they are gone, for a limited time.
    ///
    /// # Errors
    ///
    /// An I/O error if logind refuses the request.
    fn end_sessions(&self, ids: &[String]) -> io::Result<()>;

    /// Starts the web session of `user`, or finds the one that is open.
    ///
    /// # Errors
    ///
    /// An I/O error if the session does not start.
    fn start_session(&self, user: &str, uid: u32) -> io::Result<Started>;

    /// Ends the web sessions of `uid`. Local sessions continue. Returns the number of sessions that ended.
    ///
    /// # Errors
    ///
    /// An I/O error if logind refuses the request.
    fn end_web_sessions(&self, uid: u32) -> io::Result<u32>;
}

/// A host with no sessions. For development and tests with no root.
#[derive(Debug, Default)]
pub struct NoSessions;

impl Host for NoSessions {
    fn local_sessions(&self, _uid: u32) -> io::Result<Vec<LocalSession>> {
        Ok(Vec::new())
    }

    fn end_sessions(&self, _ids: &[String]) -> io::Result<()> {
        Ok(())
    }

    fn start_session(&self, _user: &str, _uid: u32) -> io::Result<Started> {
        Ok(Started::Attached)
    }

    fn end_web_sessions(&self, _uid: u32) -> io::Result<u32> {
        Ok(0)
    }
}

/// The settings of the session worker.
#[derive(Clone, Debug)]
pub struct WorkerSettings {
    /// The program of farglass-login, which runs as the session worker.
    pub exe: PathBuf,
    /// The PAM service of the session.
    pub pam_service: String,
    /// A directory that replaces `/etc/pam.d`, for tests.
    pub pam_dir: Option<PathBuf>,
    /// The program of the agent.
    pub agent_program: PathBuf,
    /// The agent socket of farglassd.
    pub agent_socket: PathBuf,
}

/// The host actions through loginctl and systemd-run.
#[derive(Debug)]
pub struct Systemd {
    /// The settings of the session worker.
    pub worker: WorkerSettings,
}

/// One row of `loginctl --json=short list-sessions`.
#[derive(Debug, Deserialize)]
struct Row {
    session: String,
    uid: u32,
    class: String,
}

/// The properties of a session that farglass-login reads.
#[derive(Debug, Default, PartialEq, Eq)]
struct Properties {
    kind: String,
    state: String,
    seat: String,
    service: String,
}

fn loginctl(args: &[&str]) -> io::Result<String> {
    run(Command::new("loginctl").args(args))
}

fn run(command: &mut Command) -> io::Result<String> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Returns the user sessions of `uid` from the JSON of `loginctl list-sessions`.
fn user_sessions(json: &str, uid: u32) -> io::Result<Vec<String>> {
    let rows: Vec<Row> = serde_json::from_str(json).map_err(io::Error::other)?;
    Ok(rows
        .into_iter()
        .filter(|row| row.uid == uid && row.class == "user")
        .map(|row| row.session)
        .collect())
}

/// Parses the `key=value` lines of `loginctl show-session`.
fn parse_properties(text: &str) -> Properties {
    let mut properties = Properties::default();
    for (key, value) in text.lines().filter_map(|line| line.split_once('=')) {
        let value = value.to_owned();
        match key {
            "Type" => properties.kind = value,
            "State" => properties.state = value,
            "Seat" => properties.seat = value,
            "Service" => properties.service = value,
            _ => {}
        }
    }
    properties
}

/// Reports whether a session is local and graphical, and not closing.
fn is_local_graphical(properties: &Properties) -> bool {
    matches!(properties.kind.as_str(), "x11" | "wayland" | "mir") && properties.state != "closing"
}

impl Systemd {
    /// Returns the user sessions of `uid` with their properties.
    fn sessions(&self, uid: u32) -> io::Result<Vec<(String, Properties)>> {
        let list = loginctl(&["--json=short", "list-sessions"])?;
        let mut out = Vec::new();
        for id in user_sessions(&list, uid)? {
            let text = loginctl(&[
                "show-session",
                &id,
                "-p",
                "Type",
                "-p",
                "State",
                "-p",
                "Seat",
                "-p",
                "Service",
            ])?;
            out.push((id, parse_properties(&text)));
        }
        Ok(out)
    }

    /// Returns the open web sessions of `uid` that farglass started.
    fn web_sessions(&self, uid: u32) -> io::Result<Vec<String>> {
        Ok(self
            .sessions(uid)?
            .into_iter()
            .filter(|(_, properties)| {
                properties.kind == "web"
                    && properties.service == self.worker.pam_service
                    && properties.state != "closing"
            })
            .map(|(id, _)| id)
            .collect())
    }
}

impl Host for Systemd {
    fn local_sessions(&self, uid: u32) -> io::Result<Vec<LocalSession>> {
        Ok(self
            .sessions(uid)?
            .into_iter()
            .filter(|(_, properties)| is_local_graphical(properties))
            .map(|(id, properties)| LocalSession {
                id,
                kind: properties.kind,
                seat: (!properties.seat.is_empty()).then_some(properties.seat),
            })
            .collect())
    }

    fn end_sessions(&self, ids: &[String]) -> io::Result<()> {
        for id in ids {
            tracing::info!(session = id, "ending a local graphical session");
            loginctl(&["terminate-session", id])?;
        }
        let start = Instant::now();
        while start.elapsed() < END_TIMEOUT {
            // logind refuses show-session for a session that is gone.
            if ids
                .iter()
                .all(|id| loginctl(&["show-session", id, "-p", "State"]).is_err())
            {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        tracing::warn!(
            ?ids,
            "the local sessions did not end in time, the web login continues"
        );
        Ok(())
    }

    fn start_session(&self, user: &str, uid: u32) -> io::Result<Started> {
        if !self.web_sessions(uid)?.is_empty() {
            return Ok(Started::Attached);
        }
        let mut command = Command::new("systemd-run");
        command
            .arg(format!("--unit=farglass-session-{uid}"))
            .args(["--collect", "--quiet"])
            .arg(format!("--description=farglass web session of {user}"))
            .arg("--")
            .arg(&self.worker.exe)
            .args(["--session-worker", user])
            .arg("--session-pam-service")
            .arg(&self.worker.pam_service)
            .arg("--agent-program")
            .arg(&self.worker.agent_program)
            .arg("--agent-socket")
            .arg(&self.worker.agent_socket);
        if let Some(dir) = &self.worker.pam_dir {
            command.arg("--pam-dir").arg(dir);
        }
        run(&mut command)?;

        let start = Instant::now();
        while start.elapsed() < START_TIMEOUT {
            if !self.web_sessions(uid)?.is_empty() {
                return Ok(Started::New);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(io::Error::other(format!(
            "the web session of {user} did not show in logind in {} s",
            START_TIMEOUT.as_secs()
        )))
    }

    fn end_web_sessions(&self, uid: u32) -> io::Result<u32> {
        let ids = self.web_sessions(uid)?;
        for id in &ids {
            tracing::info!(session = id, uid, "ending a web session");
            loginctl(&["terminate-session", id])?;
        }
        Ok(u32::try_from(ids.len()).unwrap_or(u32::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The output of systemd 257 on a host with a GNOME session and a web session.
    const LIST: &str = r#"[{"session":"1","uid":1001,"user":"alice","seat":null,"leader":1001,"class":"manager","tty":null,"idle":false,"since":null},{"session":"2","uid":1001,"user":"alice","seat":"seat0","leader":1138,"class":"user","tty":"tty2","idle":false,"since":null},{"session":"5","uid":1001,"user":"alice","seat":null,"leader":2001,"class":"user","tty":null,"idle":false,"since":null},{"session":"7","uid":1002,"user":"bob","seat":null,"leader":3000,"class":"user","tty":"pts/0","idle":false,"since":null}]"#;

    #[test]
    fn user_sessions_skip_the_manager_and_other_users() {
        assert_eq!(user_sessions(LIST, 1001).unwrap(), ["2", "5"]);
        assert_eq!(user_sessions(LIST, 1002).unwrap(), ["7"]);
        assert!(user_sessions("not json", 1001).is_err());
    }

    #[test]
    fn local_graphical_sessions_are_x11_wayland_and_mir_that_are_not_closing() {
        let gnome =
            parse_properties("Type=wayland\nState=active\nSeat=seat0\nService=gdm-autologin\n");
        assert_eq!(gnome.seat, "seat0");
        assert!(is_local_graphical(&gnome));
        assert!(is_local_graphical(&parse_properties(
            "Type=x11\nState=online\n"
        )));
        assert!(!is_local_graphical(&parse_properties(
            "Type=wayland\nState=closing\n"
        )));
        assert!(!is_local_graphical(&parse_properties(
            "Type=tty\nState=active\n"
        )));
        assert!(!is_local_graphical(&parse_properties(
            "Type=web\nState=active\nService=farglass-session\n"
        )));
    }
}
