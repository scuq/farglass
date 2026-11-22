//! The distribution of the host, from /etc/os-release (F067).

/// The package manager and the file conventions of a distribution family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Debian and Ubuntu: apt, PAM with common-auth.
    Debian,
    /// RHEL, Rocky Linux, AlmaLinux, and Fedora: dnf, PAM with password-auth.
    Rhel,
}

/// A supported distribution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Distro {
    /// The ID of os-release, for example `debian` or `rocky`.
    pub id: String,
    /// The VERSION_ID of os-release, for example `12` or `9.5`.
    pub version: String,
    /// The family.
    pub family: Family,
}

/// The supported distributions and their oldest major version.
///
/// The rule: the newest release and the release before it. For Ubuntu, only LTS releases count.
/// A newer release than the newest one is not refused.
const SUPPORTED: [(&str, Family, u32); 6] = [
    ("debian", Family::Debian, 12),
    ("ubuntu", Family::Debian, 24),
    ("rhel", Family::Rhel, 9),
    ("rocky", Family::Rhel, 9),
    ("almalinux", Family::Rhel, 9),
    ("fedora", Family::Rhel, 44),
];

/// Returns the text that names the supported distributions, for an error message.
#[must_use]
pub fn supported_text() -> &'static str {
    "Debian 12 and later, Ubuntu 24.04 and later, RHEL, Rocky Linux, and AlmaLinux 9 and later, Fedora 44 and later"
}

impl Distro {
    /// Returns the distribution of the os-release text `text`.
    ///
    /// # Errors
    ///
    /// A message with the supported distributions if the host has a different one.
    pub fn parse(text: &str) -> Result<Self, String> {
        let field = |name: &str| {
            text.lines()
                .filter_map(|line| line.split_once('='))
                .find(|(key, _)| key.trim() == name)
                .map(|(_, value)| value.trim().trim_matches('"').to_owned())
        };
        let id = field("ID").unwrap_or_default();
        let version = field("VERSION_ID").unwrap_or_default();
        let major: u32 = version
            .split('.')
            .next()
            .and_then(|major| major.parse().ok())
            .unwrap_or(0);
        let pretty = field("PRETTY_NAME").unwrap_or_else(|| format!("{id} {version}"));
        match SUPPORTED.iter().find(|(name, _, _)| *name == id) {
            Some((_, family, oldest)) if major >= *oldest => Ok(Self {
                id,
                version,
                family: *family,
            }),
            _ => Err(format!(
                "{pretty} is not supported. farglass supports {}.",
                supported_text()
            )),
        }
    }

    /// Returns the distribution of this host.
    ///
    /// # Errors
    ///
    /// A message if /etc/os-release cannot be read or names an unsupported distribution.
    pub fn detect() -> Result<Self, String> {
        let text = std::fs::read_to_string("/etc/os-release")
            .map_err(|error| format!("/etc/os-release cannot be read: {error}"))?;
        Self::parse(&text)
    }

    /// Returns the command that installs the runtime packages of farglass (ADR-048).
    ///
    /// The packages give libpam, libpipewire, libxkbcommon, the sound server of the
    /// session, and the portal of the desktop. libc and libgcc_s are in each base system.
    #[must_use]
    pub fn package_commands(&self) -> Vec<Vec<String>> {
        let words = |list: &[&str]| {
            list.iter()
                .map(|word| (*word).to_owned())
                .collect::<Vec<_>>()
        };
        match self.family {
            Family::Debian => vec![
                words(&["apt-get", "update"]),
                words(&[
                    "apt-get",
                    "install",
                    "-y",
                    "--no-install-recommends",
                    "libpam-runtime",
                    "libpam-systemd",
                    "libxkbcommon0",
                    "pipewire",
                    "pipewire-pulse",
                    "wireplumber",
                    "dbus-user-session",
                    "xdg-desktop-portal-gtk",
                ]),
            ],
            Family::Rhel => vec![words(&[
                "dnf",
                "install",
                "-y",
                "pam",
                "libxkbcommon",
                "pipewire",
                "pipewire-pulseaudio",
                "wireplumber",
                "xdg-desktop-portal-gtk",
            ])],
        }
    }

    /// Returns a hint for a failed package install, or `None`.
    ///
    /// RHEL gets pipewire, wireplumber, and the portal from the AppStream
    /// repository. A host with no subscription has only the UBI repositories,
    /// which do not have these packages.
    #[must_use]
    pub fn package_hint(&self) -> Option<&'static str> {
        (self.id == "rhel").then_some(
            "On RHEL, dnf finds pipewire, wireplumber, and xdg-desktop-portal-gtk only in the AppStream repository. Register the system (subscription-manager register), then run the install again.",
        )
    }

    /// Returns the directory of the PAM files for this family in a release.
    #[must_use]
    pub fn pam_dir(&self) -> &'static str {
        match self.family {
            Family::Debian => "packaging/pam.d",
            Family::Rhel => "packaging/pam.d/rhel",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(id: &str, version: &str) -> String {
        format!("PRETTY_NAME=\"{id} {version}\"\nID={id}\nVERSION_ID=\"{version}\"\n")
    }

    #[test]
    fn the_supported_distributions_are_known_with_their_family() {
        for (id, version, family) in [
            ("debian", "12", Family::Debian),
            ("debian", "13", Family::Debian),
            ("ubuntu", "24.04", Family::Debian),
            ("ubuntu", "26.04", Family::Debian),
            ("rhel", "9.5", Family::Rhel),
            ("rhel", "10.3", Family::Rhel),
            ("rocky", "9.6", Family::Rhel),
            ("rocky", "10.0", Family::Rhel),
            ("almalinux", "9.4", Family::Rhel),
            ("almalinux", "10.1", Family::Rhel),
            ("fedora", "44", Family::Rhel),
            ("fedora", "45", Family::Rhel),
        ] {
            let distro = Distro::parse(&release(id, version)).unwrap();
            assert_eq!(distro.family, family, "{id} {version}");
        }
    }

    #[test]
    fn an_old_or_unknown_distribution_is_refused_with_the_list() {
        for (id, version) in [
            ("debian", "11"),
            ("ubuntu", "22.04"),
            ("fedora", "43"),
            ("rocky", "8.10"),
            ("arch", ""),
            ("", ""),
        ] {
            let error = Distro::parse(&release(id, version)).unwrap_err();
            assert!(error.contains("Debian 12"), "{error}");
        }
    }

    #[test]
    fn only_rhel_gets_the_hint_for_the_appstream_repository() {
        let rhel = Distro::parse(&release("rhel", "10.3")).unwrap();
        assert!(
            rhel.package_hint()
                .unwrap()
                .contains("subscription-manager")
        );
        for id in ["rocky", "almalinux", "fedora", "debian"] {
            let version = if id == "fedora" { "45" } else { "13" };
            let distro = Distro::parse(&release(id, version)).unwrap();
            assert_eq!(distro.package_hint(), None, "{id}");
        }
    }

    #[test]
    fn each_family_has_its_package_manager_and_pam_files() {
        let debian = Distro::parse(&release("debian", "13")).unwrap();
        assert_eq!(debian.package_commands()[1][0], "apt-get");
        assert_eq!(debian.pam_dir(), "packaging/pam.d");
        let rocky = Distro::parse(&release("rocky", "9.6")).unwrap();
        assert_eq!(rocky.package_commands()[0][0], "dnf");
        assert_eq!(rocky.pam_dir(), "packaging/pam.d/rhel");
    }
}
