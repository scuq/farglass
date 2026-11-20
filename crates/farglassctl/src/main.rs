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

use cli::{Cli, Command};

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
            "`{}` changes the host. Run it as root, for example with sudo.",
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
        other => Err(format!(
            "`{}` is not ready in this version of farglassctl.",
            other.name()
        )),
    }
}

/// Returns the version of the release: the tag that the release workflow gives, or the crate version.
#[must_use]
pub fn version() -> &'static str {
    option_env!("FARGLASS_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}
