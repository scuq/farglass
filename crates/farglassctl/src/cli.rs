//! The command line of farglassctl.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueHint};
use clap_complete::Shell;

/// Installs and manages farglass on this host.
#[derive(Debug, Parser)]
#[command(name = "farglassctl", version, about, propagate_version = true)]
pub struct Cli {
    /// The action
    #[command(subcommand)]
    pub command: Command,
}

/// The subcommands of farglassctl.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Install farglass from a signed release, with the runtime packages
    Install {
        /// The release to install, for example v0.2.0 [default: the newest]
        #[arg(long, value_name = "TAG")]
        release: Option<String>,
        /// Install from the release files in DIR, not from GitHub. The signature is checked too
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        from: Option<PathBuf>,
        /// Let USER log in: add USER to the group farglass-users. Can come more than one time
        #[arg(long, value_name = "USER", value_hint = ValueHint::Username)]
        user: Vec<String>,
        /// Open the port of farglassd in firewalld
        #[arg(long)]
        open_firewall: bool,
    },
    /// Update farglass to a newer signed release
    Update {
        /// The release to install [default: the newest]
        #[arg(long, value_name = "TAG")]
        release: Option<String>,
        /// Update from the release files in DIR, not from GitHub
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        from: Option<PathBuf>,
    },
    /// Remove farglass. The configuration and the state stay, unless --purge
    Uninstall {
        /// Also remove /etc/farglass, /var/lib/farglass, and the user farglass
        #[arg(long)]
        purge: bool,
    },
    /// Show the units, the version, the address, the certificate, the PIN, and the users
    Status,
    /// Check the installation and give a hint for each fault
    Doctor,
    /// Manage the users who can log in
    #[command(subcommand)]
    User(UserCommand),
    /// Manage the web sessions
    #[command(subcommand)]
    Session(SessionCommand),
    /// Manage the machine PIN before the web login
    #[command(subcommand)]
    Pin(PinCommand),
    /// Show the log of farglassd and farglass-login
    Logs {
        /// Only the security events (farglass::security)
        #[arg(long)]
        security: bool,
        /// Show new lines as they come
        #[arg(long, short)]
        follow: bool,
    },
    /// Print the version of farglassctl
    Version,
    /// Print the completion script for a shell
    Completion {
        /// The shell
        #[arg(value_enum, default_value_t = Shell::Bash)]
        shell: Shell,
    },
}

/// The subcommands of `farglassctl user`.
#[derive(Debug, Subcommand)]
pub enum UserCommand {
    /// List the users of farglass-users, with their web sessions and passkeys
    List,
    /// Let a user log in: add the user to farglass-users
    Add {
        /// The user
        #[arg(value_hint = ValueHint::Username)]
        user: String,
    },
    /// Stop a user from logging in: remove the user from farglass-users
    Remove {
        /// The user
        #[arg(value_hint = ValueHint::Username)]
        user: String,
    },
    /// Stop a user from logging in and end the web sessions of the user
    Disable {
        /// The user
        #[arg(value_hint = ValueHint::Username)]
        user: String,
    },
    /// Let a disabled user log in again
    Enable {
        /// The user
        #[arg(value_hint = ValueHint::Username)]
        user: String,
    },
}

/// The subcommands of `farglassctl session`.
#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    /// List the web sessions
    List,
    /// End the web sessions of a user. The user can log in again
    Logout {
        /// The user
        #[arg(value_hint = ValueHint::Username)]
        user: String,
    },
}

/// The subcommands of `farglassctl pin`.
#[derive(Debug, Subcommand)]
pub enum PinCommand {
    /// Set the machine PIN: 4 to 6 digits, read with no echo
    Set,
    /// Remove the machine PIN
    Clear,
}

impl Command {
    /// Returns the name of the subcommand, with its subcommand, for messages.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::Install { .. } => "install".to_owned(),
            Self::Update { .. } => "update".to_owned(),
            Self::Uninstall { .. } => "uninstall".to_owned(),
            Self::Status => "status".to_owned(),
            Self::Doctor => "doctor".to_owned(),
            Self::User(command) => format!("user {}", command.name()),
            Self::Session(SessionCommand::List) => "session list".to_owned(),
            Self::Session(SessionCommand::Logout { .. }) => "session logout".to_owned(),
            Self::Pin(PinCommand::Set) => "pin set".to_owned(),
            Self::Pin(PinCommand::Clear) => "pin clear".to_owned(),
            Self::Logs { .. } => "logs".to_owned(),
            Self::Version => "version".to_owned(),
            Self::Completion { .. } => "completion".to_owned(),
        }
    }

    /// Reports whether the subcommand changes the host or reads files of root, thus needs root.
    #[must_use]
    pub fn needs_root(&self) -> bool {
        match self {
            Self::Install { .. }
            | Self::Update { .. }
            | Self::Uninstall { .. }
            | Self::Doctor
            | Self::Pin(_)
            | Self::Logs { .. }
            | Self::Session(SessionCommand::Logout { .. }) => true,
            Self::User(command) => !matches!(command, UserCommand::List),
            Self::Status
            | Self::Session(SessionCommand::List)
            | Self::Version
            | Self::Completion { .. } => false,
        }
    }
}

impl UserCommand {
    fn name(&self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Add { .. } => "add",
            Self::Remove { .. } => "remove",
            Self::Disable { .. } => "disable",
            Self::Enable { .. } => "enable",
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory as _;

    use super::*;

    #[test]
    fn the_command_line_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn subcommands_parse_and_name_themselves() {
        let cli = Cli::try_parse_from(["farglassctl", "user", "disable", "alice"]).unwrap();
        assert_eq!(cli.command.name(), "user disable");
        assert!(cli.command.needs_root());
        let cli =
            Cli::try_parse_from(["farglassctl", "install", "--user", "alice", "--user", "bob"])
                .unwrap();
        assert!(
            matches!(cli.command, Command::Install { ref user, .. } if user == &["alice", "bob"])
        );
        assert!(
            !Cli::try_parse_from(["farglassctl", "user", "list"])
                .unwrap()
                .command
                .needs_root()
        );
        assert!(
            Cli::try_parse_from(["farglassctl", "user", "add"]).is_err(),
            "add needs a user"
        );
    }

    #[test]
    fn the_bash_completion_names_each_subcommand() {
        let mut out = Vec::new();
        clap_complete::generate(Shell::Bash, &mut Cli::command(), "farglassctl", &mut out);
        let script = String::from_utf8(out).unwrap();
        assert!(
            script.contains("complete -F _farglassctl"),
            "no complete line"
        );
        for word in [
            "install",
            "update",
            "uninstall",
            "status",
            "doctor",
            "user",
            "session",
            "pin",
            "logs",
            "disable",
            "logout",
        ] {
            assert!(script.contains(word), "the completion has no {word}");
        }
    }
}
