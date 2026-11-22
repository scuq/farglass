//! The plans of install, update, and uninstall (F067).
//!
//! The functions here build a list of steps from the facts of the host. They
//! change nothing: `plan::apply` does the steps, and `--dry-run` prints them.

use std::path::{Path, PathBuf};

use crate::os::Distro;
use crate::plan::{self, Step};

/// The programs of farglass.
pub const LIBEXEC: &str = "/usr/libexec/farglass";
/// The shared files: the web files, the extension, and the version.
pub const SHARE: &str = "/usr/share/farglass";
/// The configuration file. An install never changes an existing one.
pub const CONF: &str = "/etc/farglass/farglassd.conf";
/// The state of farglassd: the certificate, the passkeys, the PIN.
pub const STATE: &str = "/var/lib/farglass";
/// The systemd units.
pub const UNITS_DIR: &str = "/etc/systemd/system";
/// The units of farglass.
pub const UNITS: [&str; 3] = [
    "farglass-login.socket",
    "farglass-login.service",
    "farglassd.service",
];
/// The PAM files.
pub const PAM_FILES: [&str; 2] = ["farglass", "farglass-session"];
/// The portal configuration of the farglass desktop.
pub const PORTAL: &str = "/usr/share/xdg-desktop-portal/farglass-portals.conf";
/// The bash completion of farglassctl.
pub const COMPLETION: &str = "/usr/share/bash-completion/completions/farglassctl";
/// The installed farglassctl.
pub const SELF: &str = "/usr/local/sbin/farglassctl";
/// The group of the users who can log in (F058).
pub const USERS_GROUP: &str = "farglass-users";
/// The port of farglassd in the default configuration.
pub const PORT: u16 = 8443;

/// What farglassctl found on the host before it plans.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// The group farglass exists.
    pub group_farglass: bool,
    /// The user farglass exists.
    pub user_farglass: bool,
    /// The group farglass-users exists.
    pub group_users: bool,
    /// The configuration file exists.
    pub config: bool,
    /// SELinux is on, and restorecon is there.
    pub selinux: bool,
    /// firewalld runs.
    pub firewalld: bool,
    /// The address of the default route, for the self-signed certificate.
    pub address: Option<String>,
    /// The running farglassctl.
    pub self_exe: Option<PathBuf>,
}

impl Facts {
    /// Returns the facts of this host.
    #[must_use]
    pub fn probe() -> Self {
        Self {
            group_farglass: plan::succeeds(&["getent", "group", "farglass"]),
            user_farglass: plan::succeeds(&["getent", "passwd", "farglass"]),
            group_users: plan::succeeds(&["getent", "group", USERS_GROUP]),
            config: Path::new(CONF).exists(),
            selinux: Path::new("/sys/fs/selinux/enforce").exists()
                && (Path::new("/usr/sbin/restorecon").exists()
                    || Path::new("/sbin/restorecon").exists()),
            firewalld: plan::succeeds(&["firewall-cmd", "--state"]),
            address: primary_address(),
            self_exe: std::env::current_exe().ok(),
        }
    }
}

/// Returns the IPv4 address of the default route. No packet goes out: a UDP connect only selects the route.
fn primary_address() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then(|| ip.to_string())
}

/// Reports whether farglass is installed: the version file exists.
#[must_use]
pub fn installed() -> Option<String> {
    std::fs::read_to_string(Path::new(SHARE).join("VERSION"))
        .ok()
        .map(|text| text.trim().to_owned())
}

fn run(args: &[&str]) -> Step {
    Step::Run(args.iter().map(|arg| (*arg).to_owned()).collect())
}

fn try_run(args: &[&str]) -> Step {
    Step::TryRun(args.iter().map(|arg| (*arg).to_owned()).collect())
}

/// Returns the configuration file for a new install: the template with the listen address and the certificate names set.
#[must_use]
pub fn config_text(template: &str, address: Option<&str>) -> String {
    let names = address.map_or_else(String::new, |address| format!("\"{address}\""));
    template
        .lines()
        .map(|line| {
            if line.starts_with("#listen = ") {
                format!("listen = \"[::]:{PORT}\"")
            } else if line.starts_with("#names = ") && !names.is_empty() {
                format!("names = [{names}]")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// The options of an install or an update.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Users to add to farglass-users.
    pub users: Vec<String>,
    /// Open the port in firewalld.
    pub open_firewall: bool,
    /// An update: the configuration and the users stay as they are.
    pub update: bool,
}

/// Returns the steps that install or update farglass from the extracted release at `root`.
///
/// # Errors
///
/// A message if the configuration template of the release cannot be read.
pub fn install_plan(
    distro: &Distro,
    root: &Path,
    facts: &Facts,
    options: &Options,
    completion: &str,
) -> Result<Vec<Step>, String> {
    let mut plan = Vec::new();
    for command in distro.package_commands() {
        plan.push(Step::Run(command));
    }
    if !facts.group_farglass {
        plan.push(run(&["groupadd", "--system", "farglass"]));
    }
    if !facts.user_farglass {
        plan.push(run(&[
            "useradd",
            "--system",
            "--gid",
            "farglass",
            "--home-dir",
            STATE,
            "--no-create-home",
            "--shell",
            "/usr/sbin/nologin",
            "--comment",
            "farglass web server",
            "farglass",
        ]));
    }
    if !facts.group_users {
        plan.push(run(&["groupadd", "--system", USERS_GROUP]));
    }
    for program in ["farglassd", "farglass-login", "farglass-session"] {
        plan.push(Step::Copy {
            from: root.join("bin").join(program),
            to: Path::new(LIBEXEC).join(program),
            mode: 0o755,
        });
    }
    plan.push(Step::ReplaceDir {
        from: root.join("web"),
        to: Path::new(SHARE).join("web"),
    });
    plan.push(Step::ReplaceDir {
        from: root.join("extension/redirect"),
        to: Path::new(SHARE).join("redirect-extension"),
    });
    for file in PAM_FILES {
        plan.push(Step::Copy {
            from: root.join(distro.pam_dir()).join(file),
            to: Path::new("/etc/pam.d").join(file),
            mode: 0o644,
        });
    }
    plan.push(Step::Copy {
        from: root.join("packaging/xdg-desktop-portal/farglass-portals.conf"),
        to: PathBuf::from(PORTAL),
        mode: 0o644,
    });
    for unit in UNITS {
        plan.push(Step::Copy {
            from: root.join("packaging/systemd").join(unit),
            to: Path::new(UNITS_DIR).join(unit),
            mode: 0o644,
        });
    }
    plan.push(Step::Write {
        to: PathBuf::from(COMPLETION),
        content: completion.to_owned(),
        mode: 0o644,
    });
    if let Some(exe) = &facts.self_exe
        && exe != Path::new(SELF)
    {
        plan.push(Step::Copy {
            from: exe.clone(),
            to: PathBuf::from(SELF),
            mode: 0o755,
        });
    }
    plan.push(Step::Copy {
        from: root.join("VERSION"),
        to: Path::new(SHARE).join("VERSION"),
        mode: 0o644,
    });
    if !facts.config {
        let template = std::fs::read_to_string(root.join("packaging/farglassd.conf"))
            .map_err(|error| format!("the release has no farglassd.conf: {error}"))?;
        plan.push(Step::Write {
            to: PathBuf::from(CONF),
            content: config_text(&template, facts.address.as_deref()),
            mode: 0o644,
        });
    }
    if !options.update {
        for user in &options.users {
            plan.push(run(&["usermod", "-aG", USERS_GROUP, user]));
        }
    }
    if facts.selinux {
        plan.push(run(&[
            "restorecon",
            "-R",
            LIBEXEC,
            SHARE,
            "/etc/farglass",
            "/etc/pam.d",
            UNITS_DIR,
            SELF,
        ]));
        // The default policy refuses the start of the agent in the context of the user (F081).
        let module = root.join("packaging/selinux/farglass.cil");
        plan.push(run(&[
            "semodule",
            "-X",
            "200",
            "-i",
            &module.to_string_lossy(),
        ]));
    }
    plan.push(run(&["systemctl", "daemon-reload"]));
    plan.push(run(&[
        "systemctl",
        "enable",
        "--now",
        "farglass-login.socket",
    ]));
    // A running farglass-login keeps the old program: the socket starts the new one at the next login.
    plan.push(try_run(&["systemctl", "stop", "farglass-login.service"]));
    plan.push(run(&["systemctl", "enable", "farglassd.service"]));
    plan.push(run(&["systemctl", "restart", "farglassd.service"]));
    if facts.firewalld {
        if options.open_firewall {
            let port = format!("--add-port={PORT}/tcp");
            let udp = format!("--add-port={PORT}/udp");
            plan.push(run(&["firewall-cmd", "--permanent", &port, &udp]));
            plan.push(run(&["firewall-cmd", "--reload"]));
        } else {
            plan.push(Step::Note(format!(
                "firewalld runs: open the port with: firewall-cmd --permanent --add-port={PORT}/tcp --add-port={PORT}/udp && firewall-cmd --reload, or install with --open-firewall"
            )));
        }
    }
    Ok(plan)
}

/// Returns the steps that remove farglass. With `purge`, also the configuration, the state, the users, and farglassctl.
#[must_use]
pub fn uninstall_plan(purge: bool) -> Vec<Step> {
    let mut plan = vec![try_run(&[
        "systemctl",
        "disable",
        "--now",
        "farglassd.service",
        "farglass-login.socket",
        "farglass-login.service",
    ])];
    for unit in UNITS {
        plan.push(Step::Remove(Path::new(UNITS_DIR).join(unit)));
    }
    plan.push(run(&["systemctl", "daemon-reload"]));
    for path in [LIBEXEC, SHARE, PORTAL, COMPLETION, "/run/farglass"] {
        plan.push(Step::Remove(PathBuf::from(path)));
    }
    for file in PAM_FILES {
        plan.push(Step::Remove(Path::new("/etc/pam.d").join(file)));
    }
    // The shell gives an exit status on a host with no semodule, where the plan must continue.
    plan.push(try_run(&["sh", "-c", "semodule -X 200 -r farglass"]));
    if purge {
        for path in ["/etc/farglass", STATE, crate::release::TOOLS_DIR, SELF] {
            plan.push(Step::Remove(PathBuf::from(path)));
        }
        plan.push(try_run(&["userdel", "farglass"]));
        plan.push(try_run(&["groupdel", "farglass"]));
        plan.push(try_run(&["groupdel", USERS_GROUP]));
        plan.push(Step::Note(
            "Removed farglass, its configuration, its state, and its users.".to_owned(),
        ));
    } else {
        plan.push(Step::Note(format!(
            "Removed farglass. /etc/farglass, {STATE}, the user farglass, the group {USERS_GROUP}, and {SELF} stay. --purge removes them."
        )));
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn debian() -> Distro {
        Distro::parse("ID=debian\nVERSION_ID=\"13\"\n").unwrap()
    }

    fn rocky() -> Distro {
        Distro::parse("ID=rocky\nVERSION_ID=\"9.6\"\n").unwrap()
    }

    fn root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("packaging")).unwrap();
        std::fs::write(
            dir.path().join("packaging/farglassd.conf"),
            "# x\n#listen = \"[::]:8443\"\n#names = [\"a\"]\n",
        )
        .unwrap();
        dir
    }

    fn has(plan: &[Step], text: &str) -> bool {
        plan.iter().any(|step| step.to_string().contains(text))
    }

    #[test]
    fn a_new_install_creates_the_users_the_config_and_starts_the_units() {
        let dir = root();
        let facts = Facts {
            address: Some("192.0.2.10".to_owned()),
            self_exe: Some(PathBuf::from("/tmp/farglassctl-linux-arm64")),
            ..Facts::default()
        };
        let options = Options {
            users: vec!["alice".to_owned()],
            ..Options::default()
        };
        let plan = install_plan(&debian(), dir.path(), &facts, &options, "complete").unwrap();
        for text in [
            "run: apt-get install",
            "run: groupadd --system farglass",
            "run: useradd --system --gid farglass",
            "run: groupadd --system farglass-users",
            "to /usr/libexec/farglass/farglassd (mode 755)",
            "packaging/pam.d/farglass to /etc/pam.d/farglass",
            "write /etc/farglass/farglassd.conf",
            "write /usr/share/bash-completion/completions/farglassctl",
            "to /usr/local/sbin/farglassctl",
            "run: usermod -aG farglass-users alice",
            "run: systemctl enable --now farglass-login.socket",
            "run: systemctl restart farglassd.service",
        ] {
            assert!(
                has(&plan, text),
                "the plan has no {text}:\n{}",
                plan.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        assert!(!has(&plan, "restorecon") && !has(&plan, "semodule") && !has(&plan, "firewall"));
        let config = plan.iter().find_map(|step| match step {
            Step::Write { to, content, .. } if to == Path::new(CONF) => Some(content.clone()),
            _ => None,
        });
        assert_eq!(
            config.unwrap(),
            "# x\nlisten = \"[::]:8443\"\nnames = [\"192.0.2.10\"]\n"
        );
    }

    #[test]
    fn an_update_keeps_the_config_and_the_users_and_red_hat_gets_its_pam_files_and_selinux() {
        let dir = root();
        let facts = Facts {
            group_farglass: true,
            user_farglass: true,
            group_users: true,
            config: true,
            selinux: true,
            firewalld: true,
            self_exe: Some(PathBuf::from(SELF)),
            ..Facts::default()
        };
        let options = Options {
            users: vec!["alice".to_owned()],
            update: true,
            ..Options::default()
        };
        let plan = install_plan(&rocky(), dir.path(), &facts, &options, "").unwrap();
        assert!(has(&plan, "run: dnf install -y pam"));
        assert!(has(
            &plan,
            "packaging/pam.d/rhel/farglass-session to /etc/pam.d/farglass-session"
        ));
        assert!(has(&plan, "run: restorecon -R"));
        assert!(has(&plan, "run: semodule -X 200 -i"));
        assert!(has(&plan, "packaging/selinux/farglass.cil"));
        assert!(has(&plan, "note: firewalld runs"));
        for text in [
            "groupadd",
            "useradd",
            "write /etc/farglass",
            "usermod",
            "to /usr/local/sbin/farglassctl",
        ] {
            assert!(!has(&plan, text), "an update has {text}");
        }
    }

    #[test]
    fn open_firewall_adds_the_port_for_tcp_and_udp() {
        let dir = root();
        let facts = Facts {
            firewalld: true,
            config: true,
            ..Facts::default()
        };
        let options = Options {
            open_firewall: true,
            ..Options::default()
        };
        let plan = install_plan(&rocky(), dir.path(), &facts, &options, "").unwrap();
        assert!(has(
            &plan,
            "firewall-cmd --permanent --add-port=8443/tcp --add-port=8443/udp"
        ));
    }

    #[test]
    fn uninstall_keeps_the_data_unless_purge() {
        let plan = uninstall_plan(false);
        assert!(
            has(&plan, "remove /usr/libexec/farglass")
                && has(&plan, "remove /etc/pam.d/farglass-session")
                && has(&plan, "semodule -X 200 -r farglass")
        );
        assert!(
            !has(&plan, "remove /etc/farglass\n")
                && !has(&plan, "remove /var/lib/farglass")
                && !has(&plan, "userdel")
        );
        let plan = uninstall_plan(true);
        assert!(
            has(&plan, "remove /var/lib/farglass")
                && has(&plan, "userdel farglass")
                && has(&plan, "groupdel farglass-users")
        );
    }
}
