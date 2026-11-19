//! The configuration of farglassd.

use std::fs;
use std::io;
use std::net::{Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::tls::TlsSource;

/// The configuration file that farglassd reads when the command line names no file.
pub const DEFAULT_CONFIG_PATH: &str = "/etc/farglass/farglassd.conf";

/// An error in the configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The configuration file did not load.
    #[error("The configuration file {} did not load: {source}.", path.display())]
    Read {
        /// The configuration file.
        path: PathBuf,
        /// The cause.
        source: io::Error,
    },

    /// The configuration file is not valid TOML or has an unknown key.
    #[error("The configuration file {} is not valid: {source}", path.display())]
    Parse {
        /// The configuration file.
        path: PathBuf,
        /// The cause, with the line and the column.
        source: Box<toml::de::Error>,
    },

    /// `dev_user` is set, but the listen address is not a loopback address.
    #[error(
        "The option dev_user works only on a loopback address, but farglassd listens on {listen}. \
         Set listen to 127.0.0.1 or [::1], or remove dev_user."
    )]
    DevUserNotLoopback {
        /// The listen address.
        listen: SocketAddr,
    },

    /// The user of `dev_user` does not exist.
    #[error("The user {0} of the option dev_user does not exist.")]
    DevUserUnknown(String),

    /// Only one of the certificate file and the key file is set.
    #[error(
        "The TLS configuration has {present} but not {missing}. \
         Set the two, or set none of them to use a self-signed certificate."
    )]
    TlsPair {
        /// The option that is set.
        present: &'static str,
        /// The option that is not set.
        missing: &'static str,
    },
}

/// The configuration of farglassd.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The address and the port that farglassd listens on.
    pub listen: SocketAddr,
    /// The directory with the shell files.
    pub web_root: PathBuf,
    /// The directory for the files that farglassd writes.
    pub state_dir: PathBuf,
    /// The Unix socket that the agents connect to.
    pub agent_socket: PathBuf,
    /// The Unix socket of farglass-login.
    pub login_socket: PathBuf,
    /// More origins that the WebSocket endpoint and the session endpoint accept,
    /// for example `http://localhost:5173` for the Vite server.
    pub allowed_origins: Vec<String>,
    /// The user of each browser connection, with no login. Only for development.
    ///
    /// A username or a numeric uid. farglassd refuses this option if it does
    /// not listen on a loopback address.
    pub dev_user: Option<String>,
    /// The TLS configuration.
    pub tls: TlsConfig,
}

/// The TLS configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TlsConfig {
    /// The PEM file with the certificate chain, the server certificate first.
    pub cert: Option<PathBuf>,
    /// The PEM file with the private key.
    pub key: Option<PathBuf>,
    /// More names and addresses for the self-signed certificate.
    pub names: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from((Ipv6Addr::UNSPECIFIED, 8443)),
            web_root: PathBuf::from("/usr/share/farglass/web"),
            state_dir: PathBuf::from("/var/lib/farglass"),
            agent_socket: PathBuf::from("/run/farglass/agent.sock"),
            login_socket: PathBuf::from("/run/farglass/login.sock"),
            allowed_origins: Vec::new(),
            dev_user: None,
            tls: TlsConfig::default(),
        }
    }
}

impl Config {
    /// Loads the configuration.
    ///
    /// If `path` is `None`, the function reads [`DEFAULT_CONFIG_PATH`].
    /// If that file does not exist, the function returns the defaults.
    ///
    /// # Errors
    ///
    /// - [`ConfigError::Read`] if the file does not load.
    ///   A missing file is an error only when `path` names it.
    /// - [`ConfigError::Parse`] if the file is not valid.
    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        let (path, explicit) = match path {
            Some(path) => (path, true),
            None => (Path::new(DEFAULT_CONFIG_PATH), false),
        };
        match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text, path),
            Err(error) if error.kind() == io::ErrorKind::NotFound && !explicit => {
                Ok(Self::default())
            }
            Err(source) => Err(ConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    /// Parses the text of a configuration file. `path` is for the error message.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Parse`] if the text is not valid.
    pub fn parse(text: &str, path: &Path) -> Result<Self, ConfigError> {
        toml::from_str(text).map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source: Box::new(source),
        })
    }

    /// Returns the uid of `dev_user`, or `None` if the option is not set.
    ///
    /// # Errors
    ///
    /// - [`ConfigError::DevUserNotLoopback`] if the listen address is not a loopback address.
    /// - [`ConfigError::DevUserUnknown`] if the user does not exist.
    pub fn dev_uid(&self) -> Result<Option<u32>, ConfigError> {
        let Some(name) = &self.dev_user else {
            return Ok(None);
        };
        if !self.listen.ip().is_loopback() {
            return Err(ConfigError::DevUserNotLoopback {
                listen: self.listen,
            });
        }
        if let Ok(uid) = name.parse::<u32>() {
            return Ok(Some(uid));
        }
        match nix::unistd::User::from_name(name) {
            Ok(Some(user)) => Ok(Some(user.uid.as_raw())),
            _ => Err(ConfigError::DevUserUnknown(name.clone())),
        }
    }

    /// Returns where the TLS certificate comes from.
    ///
    /// With a certificate file and a key file, farglassd uses those files.
    /// With none of them, farglassd keeps a self-signed certificate in `state_dir`.
    ///
    /// # Errors
    ///
    /// [`ConfigError::TlsPair`] if only one of the two files is set.
    pub fn tls_source(&self) -> Result<TlsSource, ConfigError> {
        match (&self.tls.cert, &self.tls.key) {
            (Some(cert), Some(key)) => Ok(TlsSource::Files {
                cert: cert.clone(),
                key: key.clone(),
            }),
            (None, None) => Ok(TlsSource::SelfSigned {
                dir: self.state_dir.join("tls"),
                names: self.tls.names.clone(),
            }),
            (Some(_), None) => Err(ConfigError::TlsPair {
                present: "a certificate file",
                missing: "a key file",
            }),
            (None, Some(_)) => Err(ConfigError::TlsPair {
                present: "a key file",
                missing: "a certificate file",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_the_defaults() {
        let config = Config::parse("", Path::new("test.conf")).unwrap();
        assert_eq!(config, Config::default());
        assert!(matches!(
            config.tls_source().unwrap(),
            TlsSource::SelfSigned { .. }
        ));
    }

    #[test]
    fn packaged_file_is_valid_and_gives_the_defaults() {
        let text = include_str!("../../../packaging/farglassd.conf");
        let config = Config::parse(text, Path::new("farglassd.conf")).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn packaged_file_is_valid_with_all_options_active() {
        let text = include_str!("../../../packaging/farglassd.conf");
        let active: String = text
            .lines()
            .map(|line| match line.strip_prefix('#') {
                // An option line has no space after the number sign.
                Some(rest) if !rest.starts_with(' ') && !rest.is_empty() => rest,
                _ => line,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let config = Config::parse(&active, Path::new("farglassd.conf")).unwrap();
        assert!(matches!(
            config.tls_source().unwrap(),
            TlsSource::Files { .. }
        ));
        assert!(!config.tls.names.is_empty());
    }

    #[test]
    fn unknown_key_is_refused() {
        let error = Config::parse("lisen = \"[::]:443\"", Path::new("test.conf")).unwrap_err();
        assert!(error.to_string().contains("lisen"), "{error}");
    }

    #[test]
    fn dev_user_works_only_on_loopback() {
        let mut config = Config::parse("dev_user = \"1000\"", Path::new("t")).unwrap();
        assert!(matches!(
            config.dev_uid(),
            Err(ConfigError::DevUserNotLoopback { .. })
        ));
        for listen in ["127.0.0.1:8443", "[::1]:8443"] {
            config.listen = listen.parse().unwrap();
            assert_eq!(config.dev_uid().unwrap(), Some(1000), "{listen}");
        }
    }

    #[test]
    fn dev_user_resolves_a_username() {
        let mut config = Config::parse("listen = \"127.0.0.1:8443\"", Path::new("t")).unwrap();
        assert_eq!(config.dev_uid().unwrap(), None);
        config.dev_user = Some("root".to_owned());
        assert_eq!(config.dev_uid().unwrap(), Some(0));
        config.dev_user = Some("no-such-user-farglass".to_owned());
        assert!(matches!(
            config.dev_uid(),
            Err(ConfigError::DevUserUnknown(_))
        ));
    }

    #[test]
    fn certificate_with_no_key_is_refused() {
        let config = Config::parse("[tls]\ncert = \"/a.pem\"", Path::new("t")).unwrap();
        let error = config.tls_source().unwrap_err();
        assert!(error.to_string().contains("not a key file"), "{error}");
    }
}
