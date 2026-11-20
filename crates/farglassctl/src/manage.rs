//! The users, the web sessions, the machine PIN, the status, the logs, and the checks (F068).

use std::collections::BTreeSet;
use std::io::{self, BufRead as _, Write as _};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::install::{self, CONF, LIBEXEC, PORT, STATE, USERS_GROUP};
use crate::plan::{output, succeeds};

/// The users that `user disable` stopped, one name for each line.
pub const DISABLED: &str = "/var/lib/farglassctl/disabled";

/// One web session from logind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebSession {
    /// The session id.
    pub id: String,
    /// The user.
    pub user: String,
    /// The uid.
    pub uid: u32,
    /// The state: active, online, or closing.
    pub state: String,
    /// The start of the session, as logind gives it.
    pub since: String,
}

/// Returns the members of a group from a line of `getent group`.
#[must_use]
pub fn group_members(line: &str) -> Vec<String> {
    line.trim()
        .rsplit(':')
        .next()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Returns the web session of the output of `loginctl show-session ID -p ...`, or `None` for a different session.
#[must_use]
pub fn parse_session(id: &str, text: &str) -> Option<WebSession> {
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{name}=")))
            .unwrap_or("")
            .to_owned()
    };
    (field("Type") == "web" && field("Service") == "farglass-session").then(|| WebSession {
        id: id.to_owned(),
        user: field("Name"),
        uid: field("User").parse().unwrap_or(0),
        state: field("State"),
        since: field("Timestamp"),
    })
}

/// Returns the port of the `listen` line of the configuration, or the default port.
#[must_use]
pub fn listen_port(config: &str) -> u16 {
    config
        .lines()
        .map(str::trim)
        .find_map(|line| {
            let value = line
                .strip_prefix("listen")?
                .trim_start()
                .strip_prefix('=')?
                .trim();
            value.trim_matches('"').rsplit(':').next()?.parse().ok()
        })
        .unwrap_or(PORT)
}

/// Returns the members of farglass-users.
fn members() -> Vec<String> {
    output(&["getent", "group", USERS_GROUP])
        .map_or_else(|_| Vec::new(), |line| group_members(&line))
}

/// Returns the users that `user disable` stopped.
fn disabled() -> BTreeSet<String> {
    std::fs::read_to_string(DISABLED)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

fn save_disabled(names: &BTreeSet<String>) -> Result<(), String> {
    let text: String = names.iter().map(|name| format!("{name}\n")).collect();
    std::fs::create_dir_all("/var/lib/farglassctl").map_err(|error| error.to_string())?;
    std::fs::write(DISABLED, text).map_err(|error| format!("{DISABLED}: {error}"))
}

/// Returns the web sessions of farglass from logind.
#[must_use]
pub fn web_sessions() -> Vec<WebSession> {
    let list = output(&["loginctl", "list-sessions", "--no-legend"]).unwrap_or_default();
    list.lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter_map(|id| {
            let text = output(&[
                "loginctl",
                "show-session",
                id,
                "-p",
                "Type",
                "-p",
                "Service",
                "-p",
                "Name",
                "-p",
                "User",
                "-p",
                "State",
                "-p",
                "Timestamp",
            ])
            .ok()?;
            parse_session(id, &text)
        })
        .collect()
}

fn require_user(user: &str) -> Result<(), String> {
    if succeeds(&["id", user]) {
        Ok(())
    } else {
        Err(format!("The user {user} does not exist."))
    }
}

fn run(args: &[&str]) -> Result<(), String> {
    output(args).map(|_| ())
}

/// `user list`: the members, the disabled users, their web sessions, and their passkeys.
pub fn user_list() {
    let sessions = web_sessions();
    let disabled = disabled();
    let members = members();
    if members.is_empty() && disabled.is_empty() {
        println!("No user can log in. Add one with: farglassctl user add USER");
        return;
    }
    println!(
        "{:<20} {:<9} {:<12} PASSKEYS",
        "USER", "STATE", "WEB SESSION"
    );
    let names: BTreeSet<String> = members
        .iter()
        .cloned()
        .chain(disabled.iter().cloned())
        .collect();
    for name in names {
        let state = if disabled.contains(&name) {
            "disabled"
        } else {
            "enabled"
        };
        let session = sessions
            .iter()
            .find(|session| session.user == name)
            .map_or("-", |session| session.id.as_str());
        let uid = output(&["id", "-u", &name])
            .ok()
            .map(|text| text.trim().to_owned());
        let passkeys = match uid {
            Some(uid) => match std::fs::metadata(
                Path::new(STATE)
                    .join("passkeys")
                    .join(format!("{uid}.json")),
            ) {
                Ok(_) => "yes",
                Err(error) if error.kind() == io::ErrorKind::PermissionDenied => "? (root)",
                Err(_) => "no",
            },
            None => "?",
        };
        println!("{name:<20} {state:<9} {session:<12} {passkeys}");
    }
}

/// `user add`: the user can log in.
pub fn user_add(user: &str) -> Result<(), String> {
    require_user(user)?;
    run(&["usermod", "-aG", USERS_GROUP, user])?;
    let mut list = disabled();
    if list.remove(user) {
        save_disabled(&list)?;
    }
    println!("{user} can log in.");
    Ok(())
}

/// `user remove`: the user cannot log in. A running web session continues until the next connection.
pub fn user_remove(user: &str) -> Result<(), String> {
    require_user(user)?;
    if members().iter().any(|member| member == user) {
        run(&["gpasswd", "-d", user, USERS_GROUP])?;
    }
    let mut list = disabled();
    if list.remove(user) {
        save_disabled(&list)?;
    }
    println!(
        "{user} cannot log in. A browser of the user gets the login screen at its next connection."
    );
    Ok(())
}

/// `user disable`: the user cannot log in, and the web sessions end.
pub fn user_disable(user: &str) -> Result<(), String> {
    require_user(user)?;
    if members().iter().any(|member| member == user) {
        run(&["gpasswd", "-d", user, USERS_GROUP])?;
    }
    let mut list = disabled();
    list.insert(user.to_owned());
    save_disabled(&list)?;
    let ended = logout(user)?;
    println!(
        "{user} is disabled: no login, and {ended} web session(s) ended. farglassctl user enable {user} undoes it."
    );
    Ok(())
}

/// `user enable`: a disabled user can log in again.
pub fn user_enable(user: &str) -> Result<(), String> {
    if !disabled().contains(user) {
        return Err(format!(
            "{user} is not disabled. To let a new user log in, use: farglassctl user add {user}"
        ));
    }
    user_add(user)
}

/// Ends the web sessions of `user`. Returns their number.
fn logout(user: &str) -> Result<usize, String> {
    let sessions: Vec<WebSession> = web_sessions()
        .into_iter()
        .filter(|session| session.user == user)
        .collect();
    for session in &sessions {
        run(&["loginctl", "terminate-session", &session.id])?;
    }
    Ok(sessions.len())
}

/// `session list`.
pub fn session_list() {
    let sessions = web_sessions();
    if sessions.is_empty() {
        println!("No web session.");
        return;
    }
    println!(
        "{:<8} {:<20} {:<8} {:<10} SINCE",
        "SESSION", "USER", "UID", "STATE"
    );
    for session in sessions {
        println!(
            "{:<8} {:<20} {:<8} {:<10} {}",
            session.id, session.user, session.uid, session.state, session.since
        );
    }
}

/// `session logout`: ends the web sessions of the user. The browsers get the login screen.
pub fn session_logout(user: &str) -> Result<(), String> {
    require_user(user)?;
    let ended = logout(user)?;
    if ended == 0 {
        println!("{user} has no web session.");
    } else {
        println!(
            "{ended} web session(s) of {user} ended. The browsers of the user show the login screen."
        );
    }
    Ok(())
}

/// Reads a line from the terminal with no echo.
fn read_secret(prompt: &str) -> Result<String, String> {
    use nix::sys::termios::{LocalFlags, SetArg, tcgetattr, tcsetattr};
    print!("{prompt}");
    io::stdout().flush().map_err(|error| error.to_string())?;
    let stdin = io::stdin();
    let saved = tcgetattr(&stdin).ok();
    if let Some(saved) = &saved {
        let mut quiet = saved.clone();
        quiet.local_flags.remove(LocalFlags::ECHO);
        let _ = tcsetattr(&stdin, SetArg::TCSANOW, &quiet);
    }
    let mut line = String::new();
    let result = stdin.lock().read_line(&mut line);
    if let Some(saved) = &saved {
        let _ = tcsetattr(&stdin, SetArg::TCSANOW, saved);
        println!();
    }
    result.map_err(|error| error.to_string())?;
    Ok(line.trim().to_owned())
}

/// `pin set`: reads the PIN two times and gives it to farglassd --set-pin.
pub fn pin_set() -> Result<(), String> {
    let pin = read_secret("New PIN (4 to 6 digits): ")?;
    if !(4..=6).contains(&pin.len()) || !pin.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("The PIN must have 4 to 6 digits.".to_owned());
    }
    if read_secret("The PIN again: ")? != pin {
        return Err("The two PINs are different. Nothing changed.".to_owned());
    }
    let mut child = Command::new(Path::new(LIBEXEC).join("farglassd"))
        .arg("--set-pin")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("farglassd did not start: {error}"))?;
    child
        .stdin
        .take()
        .ok_or("no input of farglassd")?
        .write_all(format!("{pin}\n").as_bytes())
        .map_err(|error| error.to_string())?;
    let status = child.wait().map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("farglassd did not set the PIN.".to_owned())
    }
}

/// `pin clear`.
pub fn pin_clear() -> Result<(), String> {
    let program = Path::new(LIBEXEC).join("farglassd");
    let program = program.to_string_lossy();
    run(&[&program, "--clear-pin"])?;
    println!("The machine PIN is removed.");
    Ok(())
}

/// `logs`: the journal of farglassd and farglass-login.
pub fn logs(security: bool, follow: bool) -> Result<(), String> {
    let mut command = Command::new("journalctl");
    command.args([
        "-u",
        "farglassd",
        "-u",
        "farglass-login",
        "--no-pager",
        "-o",
        "short-iso",
    ]);
    if security {
        command.args(["--grep", "farglass::security"]);
    }
    if follow {
        command.arg("--follow");
    } else {
        command.args(["-n", "200"]);
    }
    let status = command
        .status()
        .map_err(|error| format!("journalctl did not start: {error}"))?;
    if status.success() || status.code() == Some(1) {
        Ok(())
    } else {
        Err(format!("journalctl ended with {status}"))
    }
}

fn unit_state(unit: &str) -> String {
    output(&["systemctl", "is-active", unit])
        .unwrap_or_else(|_| "inactive".to_owned())
        .trim()
        .to_owned()
}

/// `status`.
pub fn status() {
    let version = install::installed().unwrap_or_else(|| "not installed".to_owned());
    println!("farglass:           {version}");
    println!("farglassctl:        {}", crate::version());
    for unit in ["farglassd.service", "farglass-login.socket"] {
        println!("{unit:<20}{}", unit_state(unit));
    }
    let port = listen_port(&std::fs::read_to_string(CONF).unwrap_or_default());
    println!("Port:               {port} (TCP and UDP)");
    let pin = match std::fs::metadata(Path::new(STATE).join("pin")) {
        Ok(_) => "on",
        Err(error) if error.kind() == io::ErrorKind::NotFound => "off",
        Err(_) => "unknown (the state cannot be read)",
    };
    println!("Machine PIN:        {pin}");
    let members = members();
    println!(
        "Users who can log in: {}",
        if members.is_empty() {
            "none".to_owned()
        } else {
            members.join(", ")
        }
    );
    println!("Web sessions:       {}", web_sessions().len());
}

/// The result of one check of `doctor`.
struct Check {
    ok: bool,
    text: String,
    hint: String,
}

/// `doctor`: checks the installation. Returns an error if a check fails.
pub fn doctor() -> Result<(), String> {
    let mut checks = Vec::new();
    let mut check = |ok: bool, text: &str, hint: &str| {
        checks.push(Check {
            ok,
            text: text.to_owned(),
            hint: hint.to_owned(),
        });
    };
    let installed = install::installed();
    check(
        installed.is_some(),
        "farglass is installed",
        "Run: farglassctl install",
    );
    for unit in ["farglassd.service", "farglass-login.socket"] {
        check(
            unit_state(unit) == "active",
            &format!("{unit} is active"),
            &format!("Run: systemctl status {unit}, and farglassctl logs"),
        );
    }
    for program in ["farglassd", "farglass-login", "farglass-session"] {
        let path = Path::new(LIBEXEC).join(program);
        let ldd = output(&["ldd", &path.to_string_lossy()])
            .unwrap_or_else(|error| format!("not found {error}"));
        check(
            !ldd.contains("not found"),
            &format!("{program} finds its libraries"),
            "Run: farglassctl update, which installs the runtime packages",
        );
    }
    for file in ["/etc/pam.d/farglass", "/etc/pam.d/farglass-session"] {
        check(
            Path::new(file).exists(),
            &format!("{file} exists"),
            "Run: farglassctl update",
        );
    }
    check(
        Path::new(CONF).exists(),
        &format!("{CONF} exists"),
        "Run: farglassctl update, which writes the default",
    );
    let members = members();
    check(
        !members.is_empty(),
        "a user can log in (farglass-users has members)",
        "Run: farglassctl user add USER",
    );
    let port = listen_port(&std::fs::read_to_string(CONF).unwrap_or_default());
    check(
        std::net::TcpStream::connect(("127.0.0.1", port)).is_ok()
            || std::net::TcpStream::connect(("::1", port)).is_ok(),
        &format!("farglassd answers on port {port}"),
        "Run: farglassctl logs",
    );
    if succeeds(&["firewall-cmd", "--state"]) {
        let ports = output(&["firewall-cmd", "--list-ports"]).unwrap_or_default();
        check(
            ports
                .split_whitespace()
                .any(|entry| entry == format!("{port}/tcp")),
            &format!("firewalld lets port {port}/tcp in"),
            &format!(
                "Run: firewall-cmd --permanent --add-port={port}/tcp --add-port={port}/udp && firewall-cmd --reload"
            ),
        );
    }
    let mut failed = 0;
    for check in &checks {
        if check.ok {
            println!("OK    {}", check.text);
        } else {
            failed += 1;
            println!("FAIL  {}\n      {}", check.text, check.hint);
        }
    }
    if failed == 0 {
        Ok(())
    } else {
        Err(format!("{failed} check(s) failed."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_members_come_after_the_last_colon() {
        assert_eq!(
            group_members("farglass-users:x:991:alice,bob\n"),
            ["alice", "bob"]
        );
        assert!(group_members("farglass-users:x:991:\n").is_empty());
        assert!(group_members("").is_empty());
    }

    #[test]
    fn only_a_farglass_web_session_is_a_web_session() {
        let text = "Type=web\nService=farglass-session\nName=alice\nUser=1000\nState=active\nTimestamp=Fri 2026-11-21 10:00:00 CET\n";
        assert_eq!(
            parse_session("7", text),
            Some(WebSession {
                id: "7".to_owned(),
                user: "alice".to_owned(),
                uid: 1000,
                state: "active".to_owned(),
                since: "Fri 2026-11-21 10:00:00 CET".to_owned(),
            })
        );
        assert_eq!(
            parse_session("2", "Type=wayland\nService=gdm-password\nName=alice\n"),
            None
        );
        assert_eq!(parse_session("3", "Type=web\nService=other\n"), None);
    }

    #[test]
    fn the_port_comes_from_the_listen_line() {
        assert_eq!(listen_port("listen = \"[::]:9443\"\n"), 9443);
        assert_eq!(listen_port("listen=\"0.0.0.0:8444\""), 8444);
        assert_eq!(listen_port("#listen = \"[::]:9443\"\n"), PORT);
        assert_eq!(listen_port(""), PORT);
    }
}
