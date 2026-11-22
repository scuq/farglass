//! farglassctl installs and manages farglass on a host (ADR-047).
//!
//! It is one static binary. `farglassctl install` sets up a complete host
//! from a signed release, and the other subcommands manage the users, the
//! web sessions, the machine PIN, and the installation.
//!
//! Usage:
//!
//!     farglassctl install [--release TAG] [--from DIR] [--user USER]...
//!     farglassctl update | uninstall [--purge] | status | doctor
//!     farglassctl user list | add USER | remove USER | disable USER | enable USER
//!     farglassctl session list | logout USER
//!     farglassctl pin set | clear
//!     farglassctl logs [--security] [--follow]
//!     farglassctl completion bash
//!
//! Exit status: 0 on success, 1 on an error, 2 on a usage error.

use std::io;
use std::process::ExitCode;

use clap::{CommandFactory as _, Parser as _};

mod cli;
mod install;
mod manage;
mod os;
mod plan;
mod release;

use std::path::{Path, PathBuf};

use cli::{Cli, Command, PinCommand, SessionCommand, UserCommand};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("farglassctl: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    if cli.command.needs_root() && !nix::unistd::geteuid().is_root() {
        return Err(format!(
            "`{}` needs root. Run it with sudo.",
            cli.command.name()
        ));
    }
    match cli.command {
        Command::Version => {
            println!("farglassctl {}", version());
            Ok(())
        }
        Command::Completion { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "farglassctl", &mut io::stdout());
            Ok(())
        }
        Command::Install {
            release,
            from,
            user,
            open_firewall,
            dry_run,
            allow_unsigned,
        } => {
            if let Some(version) = install::installed() {
                return Err(format!(
                    "farglass {version} is installed already. Use: farglassctl update"
                ));
            }
            for name in &user {
                if !plan::succeeds(&["id", name]) {
                    return Err(format!("The user {name} does not exist."));
                }
            }
            let options = install::Options {
                users: user,
                open_firewall,
                update: false,
            };
            setup(
                release.as_deref(),
                from.as_deref(),
                &options,
                dry_run,
                allow_unsigned,
            )
        }
        Command::Update {
            release,
            from,
            dry_run,
            allow_unsigned,
        } => {
            let Some(old) = install::installed() else {
                return Err("farglass is not installed. Use: farglassctl install".to_owned());
            };
            println!("Installed: farglass {old}");
            let options = install::Options {
                update: true,
                ..install::Options::default()
            };
            setup(
                release.as_deref(),
                from.as_deref(),
                &options,
                dry_run,
                allow_unsigned,
            )?;
            if !dry_run {
                println!("Web sessions keep the old agent until the user logs out and in again.");
            }
            Ok(())
        }
        Command::Uninstall { purge, dry_run } => {
            let steps = install::uninstall_plan(purge);
            if dry_run {
                print_plan(&steps);
                return Ok(());
            }
            plan::apply(&steps)
        }
        Command::Status => {
            manage::status();
            Ok(())
        }
        Command::Doctor => manage::doctor(),
        Command::User(command) => match command {
            UserCommand::List => {
                manage::user_list();
                Ok(())
            }
            UserCommand::Add { user } => manage::user_add(&user),
            UserCommand::Remove { user } => manage::user_remove(&user),
            UserCommand::Disable { user } => manage::user_disable(&user),
            UserCommand::Enable { user } => manage::user_enable(&user),
        },
        Command::Session(SessionCommand::List) => {
            manage::session_list();
            Ok(())
        }
        Command::Session(SessionCommand::Logout { user }) => manage::session_logout(&user),
        Command::Pin(PinCommand::Set) => manage::pin_set(),
        Command::Pin(PinCommand::Clear) => manage::pin_clear(),
        Command::Logs { security, follow } => manage::logs(security, follow),
    }
}

/// Returns the version of the release: the tag that the release workflow gives, or the crate version.
#[must_use]
pub fn version() -> &'static str {
    option_env!("FARGLASS_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// Gets, verifies, and installs a release: from `from`, or the tag `release`, or the newest release.
fn setup(
    release: Option<&str>,
    from: Option<&Path>,
    options: &install::Options,
    dry_run: bool,
    allow_unsigned: bool,
) -> Result<(), String> {
    let distro = os::Distro::detect()?;
    let work = WorkDir::new()?;
    let source = match from {
        Some(dir) => dir.to_owned(),
        None => {
            let tag = match release {
                Some(tag) => tag.to_owned(),
                None => release::latest_tag()?,
            };
            println!("Release: {tag}");
            release::fetch(&tag, &work.0)?;
            work.0.clone()
        }
    };
    let tarball = release::find_tarball(&source)?;
    release::verify(&source, &tarball, allow_unsigned)?;
    let root = release::extract(&tarball, &work.0.join("release"))?;
    let version = release::version_of(&root)?;
    let facts = install::Facts::probe();
    let mut completion = Vec::new();
    clap_complete::generate(
        clap_complete::Shell::Bash,
        &mut Cli::command(),
        "farglassctl",
        &mut completion,
    );
    let steps = install::install_plan(
        &distro,
        &root,
        &facts,
        options,
        &String::from_utf8_lossy(&completion),
    )?;
    if dry_run {
        print_plan(&steps);
        return Ok(());
    }
    plan::apply(&steps).map_err(|error| match distro.package_hint() {
        Some(hint) if error.contains("dnf install") => format!("{error}\n{hint}"),
        _ => error,
    })?;
    println!();
    let port = manage::listen_port(&std::fs::read_to_string(install::CONF).unwrap_or_default());
    if manage::answers(port, std::time::Duration::from_secs(15)) {
        println!(
            "farglass {version} runs. Open https://{}:{port}/ in a browser.",
            facts.address.as_deref().unwrap_or("HOST"),
        );
    } else {
        println!(
            "WARNING: farglass {version} is installed, but farglassd does not answer on port {port}. Run: farglassctl doctor, and farglassctl logs"
        );
    }
    let members = plan::output(&["getent", "group", install::USERS_GROUP]).unwrap_or_default();
    if members.trim_end().ends_with(':') {
        println!(
            "WARNING: The group {} has no member: no user can log in. Add one with: farglassctl user add USER",
            install::USERS_GROUP
        );
    }
    Ok(())
}

fn print_plan(steps: &[plan::Step]) {
    println!("The steps (nothing changes with --dry-run):");
    for (number, step) in steps.iter().enumerate() {
        println!("{:3}. {step}", number + 1);
    }
}

/// A work directory that goes away at the end.
struct WorkDir(PathBuf);

impl WorkDir {
    fn new() -> Result<Self, String> {
        let base = std::env::temp_dir();
        for attempt in 0..100 {
            let path = base.join(format!("farglassctl-{}-{attempt}", std::process::id()));
            if std::fs::create_dir(&path).is_ok() {
                return Ok(Self(path));
            }
        }
        Err("no work directory in the temporary directory".to_owned())
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
