//! The TLS certificate of the server.
//!
//! The certificate comes from PEM files of the administrator, or farglassd
//! makes a self-signed certificate. A [`CertStore`] holds the current
//! certificate and can replace it while the server runs.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::net::IpAddr;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use arc_swap::ArcSwap;
use rcgen::{
    CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, KeyPair, KeyUsagePurpose,
};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use rustls::{ServerConfig, version};
use sha2::{Digest as _, Sha256};
use time::{Duration, OffsetDateTime};
use x509_parser::extensions::GeneralName;

/// The time for which a self-signed certificate is valid.
pub const SELF_SIGNED_VALIDITY: Duration = Duration::days(30);

/// farglassd makes a new self-signed certificate when the old one expires in less than this time.
pub const RENEW_BEFORE: Duration = Duration::days(1);

/// The start of the validity is this much before the time of the creation.
/// Thus a client with a slow clock accepts a new certificate.
const CLOCK_TOLERANCE: Duration = Duration::minutes(5);

const SELF_SIGNED_CERT: &str = "self-signed-cert.pem";
const SELF_SIGNED_KEY: &str = "self-signed-key.pem";

/// Where the certificate of the server comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TlsSource {
    /// PEM files of the administrator.
    Files {
        /// The certificate chain, the server certificate first.
        cert: PathBuf,
        /// The private key.
        key: PathBuf,
    },
    /// A self-signed certificate that farglassd makes and keeps in `dir`.
    SelfSigned {
        /// The directory for the certificate and the key.
        dir: PathBuf,
        /// More names and addresses for the certificate.
        names: Vec<String>,
    },
}

/// An error from the load or the creation of a certificate.
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    /// A PEM file did not load.
    #[error(
        "The {what} file {} did not load: {source}. \
         Make sure that the file exists, that it is PEM, and that the user of farglassd can read it.",
        path.display()
    )]
    Pem {
        /// "certificate" or "key".
        what: &'static str,
        /// The file.
        path: PathBuf,
        /// The cause.
        source: rustls::pki_types::pem::Error,
    },

    /// The certificate file has no certificate.
    #[error("The certificate file {} has no certificate.", path.display())]
    NoCertificate {
        /// The file.
        path: PathBuf,
    },

    /// The key does not fit the certificate, or the key type is not supported.
    #[error(
        "The key file {} and the certificate file {} do not make a valid pair: {source}.",
        key.display(),
        cert.display()
    )]
    Pair {
        /// The certificate file.
        cert: PathBuf,
        /// The key file.
        key: PathBuf,
        /// The cause.
        source: rustls::Error,
    },

    /// The certificate is not valid X.509.
    #[error("The certificate file {} is not a valid X.509 certificate: {detail}.", path.display())]
    X509 {
        /// The file.
        path: PathBuf,
        /// The cause.
        detail: String,
    },

    /// A file or a directory of the self-signed certificate did not open.
    #[error(
        "The path {} was not accessible: {source}. \
         Make sure that the user of farglassd can write to the state directory.",
        path.display()
    )]
    Io {
        /// The file or the directory.
        path: PathBuf,
        /// The cause.
        source: io::Error,
    },

    /// The creation of the self-signed certificate failed.
    #[error("The creation of the self-signed certificate failed: {0}.")]
    Generate(#[from] rcgen::Error),

    /// The TLS library refused the configuration.
    #[error("The TLS configuration is not valid: {0}.")]
    Rustls(#[from] rustls::Error),
}

/// One loaded certificate with its key.
struct Loaded {
    key: Arc<CertifiedKey>,
    info: CertInfo,
    /// The modification times of the certificate file and the key file.
    stamp: (Option<SystemTime>, Option<SystemTime>),
}

/// The facts about a certificate that an operator needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertInfo {
    /// The SHA-256 fingerprint of the server certificate, as hexadecimal bytes with colons.
    pub fingerprint: String,
    /// The start of the validity.
    pub not_before: OffsetDateTime,
    /// The end of the validity.
    pub not_after: OffsetDateTime,
    /// The DNS names and the IP addresses of the certificate, in sorted sequence.
    pub names: Vec<String>,
    /// Reports whether farglassd made the certificate.
    pub self_signed: bool,
}

/// Holds the current certificate and gives it to each TLS handshake.
///
/// A replacement applies to new connections. Open connections continue.
pub struct CertStore {
    source: TlsSource,
    provider: Arc<CryptoProvider>,
    current: ArcSwap<Loaded>,
}

impl fmt::Debug for CertStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CertStore")
            .field("source", &self.source)
            .field("info", &self.info())
            .finish_non_exhaustive()
    }
}

impl CertStore {
    /// Loads the certificate from `source`.
    ///
    /// For [`TlsSource::SelfSigned`], the function makes a certificate if the
    /// directory has no certificate that is valid now.
    ///
    /// # Errors
    ///
    /// [`TlsError`] if the certificate does not load and cannot be made.
    pub fn open(source: TlsSource) -> Result<Arc<Self>, TlsError> {
        Self::open_at(source, OffsetDateTime::now_utc())
    }

    /// Does the work of [`CertStore::open`] with `now` as the current time.
    pub fn open_at(source: TlsSource, now: OffsetDateTime) -> Result<Arc<Self>, TlsError> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let loaded = load(&source, &provider, now)?;
        Ok(Arc::new(Self {
            source,
            provider,
            current: ArcSwap::from_pointee(loaded),
        }))
    }

    /// Returns the names of the self-signed certificate, or `None` for a certificate of the administrator.
    #[must_use]
    pub fn self_signed(&self) -> Option<Vec<String>> {
        match &self.source {
            TlsSource::SelfSigned { names, .. } => Some(self_signed_names(names)),
            TlsSource::Files { .. } => None,
        }
    }

    /// Returns the cryptographic provider of the TLS configurations.
    #[must_use]
    pub fn provider(&self) -> Arc<CryptoProvider> {
        self.provider.clone()
    }

    /// Returns the facts about the current certificate.
    #[must_use]
    pub fn info(&self) -> CertInfo {
        self.current.load().info.clone()
    }

    /// Replaces the certificate if it is necessary.
    ///
    /// For files, that is when a modification time changed.
    /// For a self-signed certificate, that is when it expires in less than [`RENEW_BEFORE`].
    /// Returns `true` if the function replaced the certificate.
    ///
    /// # Errors
    ///
    /// [`TlsError`] if the new certificate does not load. The store keeps the old one.
    pub fn refresh(&self) -> Result<bool, TlsError> {
        self.refresh_at(OffsetDateTime::now_utc())
    }

    /// Does the work of [`CertStore::refresh`] with `now` as the current time.
    pub fn refresh_at(&self, now: OffsetDateTime) -> Result<bool, TlsError> {
        let current = self.current.load();
        let necessary = match &self.source {
            TlsSource::Files { cert, key } => current.stamp != (mtime(cert), mtime(key)),
            TlsSource::SelfSigned { .. } => !in_validity(&current.info, now),
        };
        if !necessary {
            return Ok(false);
        }
        self.reload_at(now)?;
        Ok(true)
    }

    /// Loads the certificate again from the source.
    ///
    /// # Errors
    ///
    /// [`TlsError`] if the certificate does not load. The store keeps the old one.
    pub fn reload(&self) -> Result<(), TlsError> {
        self.reload_at(OffsetDateTime::now_utc())
    }

    fn reload_at(&self, now: OffsetDateTime) -> Result<(), TlsError> {
        let loaded = load(&self.source, &self.provider, now)?;
        self.current.store(Arc::new(loaded));
        Ok(())
    }

    /// Returns a TLS server configuration that uses this store.
    ///
    /// The configuration offers HTTP/2 and HTTP/1.1 through ALPN.
    ///
    /// # Errors
    ///
    /// [`TlsError::Rustls`] if the TLS library refuses the protocol versions.
    pub fn server_config(self: &Arc<Self>) -> Result<ServerConfig, TlsError> {
        let mut config = ServerConfig::builder_with_provider(self.provider.clone())
            .with_protocol_versions(&[&version::TLS13, &version::TLS12])?
            .with_no_client_auth()
            .with_cert_resolver(self.clone());
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Ok(config)
    }
}

impl ResolvesServerCert for CertStore {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.current.load().key.clone())
    }
}

/// Loads the certificate of `source`. Makes a self-signed certificate if it is necessary.
fn load(
    source: &TlsSource,
    provider: &CryptoProvider,
    now: OffsetDateTime,
) -> Result<Loaded, TlsError> {
    match source {
        TlsSource::Files { cert, key } => load_files(cert, key, provider, false),
        TlsSource::SelfSigned { dir, names } => {
            let names = self_signed_names(names);
            let cert = dir.join(SELF_SIGNED_CERT);
            let key = dir.join(SELF_SIGNED_KEY);
            match load_files(&cert, &key, provider, true) {
                Ok(loaded) if in_validity(&loaded.info, now) && loaded.info.names == names => {
                    return Ok(loaded);
                }
                Ok(_) => tracing::info!("the self-signed certificate is old, making a new one"),
                Err(TlsError::Pem { source, .. }) if is_not_found(&source) => {}
                Err(error) => tracing::warn!(%error, "making a new self-signed certificate"),
            }
            generate(dir, &names, now)?;
            load_files(&cert, &key, provider, true)
        }
    }
}

fn is_not_found(error: &rustls::pki_types::pem::Error) -> bool {
    matches!(error, rustls::pki_types::pem::Error::Io(e) if e.kind() == io::ErrorKind::NotFound)
}

/// Reports whether a self-signed certificate is valid at `now` and stays valid for [`RENEW_BEFORE`].
fn in_validity(info: &CertInfo, now: OffsetDateTime) -> bool {
    info.not_before <= now && now + RENEW_BEFORE < info.not_after
}

fn mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

/// Loads a certificate chain and its key from PEM files.
fn load_files(
    cert: &Path,
    key: &Path,
    provider: &CryptoProvider,
    self_signed: bool,
) -> Result<Loaded, TlsError> {
    // Read the times first. If a file changes during the load, the next refresh loads it again.
    let stamp = (mtime(cert), mtime(key));

    let chain = CertificateDer::pem_file_iter(cert)
        .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        .map_err(|source| TlsError::Pem {
            what: "certificate",
            path: cert.to_owned(),
            source,
        })?;
    let Some(leaf) = chain.first() else {
        return Err(TlsError::NoCertificate {
            path: cert.to_owned(),
        });
    };
    let info = cert_info(leaf, self_signed).map_err(|detail| TlsError::X509 {
        path: cert.to_owned(),
        detail,
    })?;

    let key_der = PrivateKeyDer::from_pem_file(key).map_err(|source| TlsError::Pem {
        what: "key",
        path: key.to_owned(),
        source,
    })?;
    let certified =
        CertifiedKey::from_der(chain, key_der, provider).map_err(|source| TlsError::Pair {
            cert: cert.to_owned(),
            key: key.to_owned(),
            source,
        })?;

    Ok(Loaded {
        key: Arc::new(certified),
        info,
        stamp,
    })
}

/// Reads the facts about a certificate from its DER bytes.
fn cert_info(der: &CertificateDer<'_>, self_signed: bool) -> Result<CertInfo, String> {
    let (_, cert) =
        x509_parser::parse_x509_certificate(der.as_ref()).map_err(|error| error.to_string())?;

    let mut names = BTreeSet::new();
    if let Some(san) = cert
        .subject_alternative_name()
        .map_err(|error| error.to_string())?
    {
        for name in &san.value.general_names {
            match name {
                GeneralName::DNSName(dns) => {
                    names.insert((*dns).to_owned());
                }
                GeneralName::IPAddress(bytes) => {
                    let ip = <[u8; 4]>::try_from(*bytes)
                        .map(IpAddr::from)
                        .or_else(|_| <[u8; 16]>::try_from(*bytes).map(IpAddr::from));
                    if let Ok(ip) = ip {
                        names.insert(ip.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    let fingerprint = Sha256::digest(der.as_ref())
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":");

    Ok(CertInfo {
        fingerprint,
        not_before: cert.validity().not_before.to_datetime(),
        not_after: cert.validity().not_after.to_datetime(),
        names: names.into_iter().collect(),
        self_signed,
    })
}

/// Returns the names of the self-signed certificate, in sorted sequence.
///
/// The certificate always has the hostname, `localhost`, and the loopback addresses.
pub(crate) fn self_signed_names(more: &[String]) -> Vec<String> {
    let mut names: BTreeSet<String> = ["localhost", "127.0.0.1", "::1"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    if let Some(hostname) = hostname() {
        names.insert(hostname);
    }
    for name in more {
        // Write an IP address in its standard form, as the certificate gives it back.
        let name = name.trim();
        match name.parse::<IpAddr>() {
            Ok(ip) => names.insert(ip.to_string()),
            Err(_) => names.insert(name.to_ascii_lowercase()),
        };
    }
    names.remove("");
    names.into_iter().collect()
}

pub(crate) fn hostname() -> Option<String> {
    let name = fs::read_to_string("/proc/sys/kernel/hostname").ok()?;
    let name = name.trim().to_ascii_lowercase();
    (!name.is_empty() && name != "(none)").then_some(name)
}

/// Makes a self-signed certificate and writes it and its key to `dir`.
///
/// The certificate is valid from `now` for [`SELF_SIGNED_VALIDITY`].
fn generate(dir: &Path, names: &[String], now: OffsetDateTime) -> Result<(), TlsError> {
    let mut params = CertificateParams::new(names.to_vec())?;
    params.not_before = now - CLOCK_TOLERANCE;
    params.not_after = now + SELF_SIGNED_VALIDITY;
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(
        DnType::CommonName,
        format!(
            "farglass self-signed ({})",
            hostname().as_deref().unwrap_or("localhost")
        ),
    );
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

    // ECDSA P-256. All browsers accept it, and WebTransport needs it for a pinned certificate.
    let key = KeyPair::generate()?;
    let cert = params.self_signed(&key)?;

    let io_error = |path: &Path| {
        let path = path.to_owned();
        move |source| TlsError::Io { path, source }
    };
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(io_error(dir))?;
    // Write the key first. A certificate with no key or an old key does not load,
    // and then the next start makes a new pair.
    let key_path = dir.join(SELF_SIGNED_KEY);
    write_atomic(&key_path, key.serialize_pem().as_bytes(), 0o600).map_err(io_error(&key_path))?;
    let cert_path = dir.join(SELF_SIGNED_CERT);
    write_atomic(&cert_path, cert.pem().as_bytes(), 0o644).map_err(io_error(&cert_path))?;
    Ok(())
}

/// Writes `contents` to `path` with the permissions `mode`, through a temporary file.
fn write_atomic(path: &Path, contents: &[u8], mode: u32) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    // Remove a file from a write that did not complete. The new file must get `mode`.
    match fs::remove_file(&tmp) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&tmp)?;
    file.write_all(contents)?;
    file.sync_all()?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    fn self_signed(dir: &Path, names: &[&str]) -> TlsSource {
        TlsSource::SelfSigned {
            dir: dir.to_owned(),
            names: names.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    fn files(dir: &Path) -> TlsSource {
        TlsSource::Files {
            cert: dir.join(SELF_SIGNED_CERT),
            key: dir.join(SELF_SIGNED_KEY),
        }
    }

    fn now() -> OffsetDateTime {
        // A certificate has a resolution of one second.
        OffsetDateTime::now_utc().replace_nanosecond(0).unwrap()
    }

    #[test]
    fn first_start_makes_a_certificate_for_30_days() {
        let dir = tempfile::tempdir().unwrap();
        let now = now();
        let store = CertStore::open_at(self_signed(dir.path(), &[]), now).unwrap();
        let info = store.info();

        assert!(info.self_signed);
        assert_eq!(info.not_after, now + Duration::days(30));
        assert_eq!(info.not_before, now - CLOCK_TOLERANCE);
        assert!(info.not_before <= now && now < info.not_after);
        for name in ["localhost", "127.0.0.1", "::1"] {
            assert!(info.names.iter().any(|n| n == name), "{name} is missing");
        }
        assert_eq!(info.fingerprint.len(), 32 * 3 - 1);
    }

    #[test]
    fn key_file_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let tls = dir.path().join("tls");
        CertStore::open(self_signed(&tls, &[])).unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&tls.join(SELF_SIGNED_KEY)), 0o600);
        assert_eq!(mode(&tls), 0o700);
    }

    #[test]
    fn second_start_uses_the_same_certificate() {
        let dir = tempfile::tempdir().unwrap();
        let now = now();
        let first = CertStore::open_at(self_signed(dir.path(), &[]), now).unwrap();
        let second =
            CertStore::open_at(self_signed(dir.path(), &[]), now + Duration::days(28)).unwrap();
        assert_eq!(first.info(), second.info());
    }

    #[test]
    fn start_makes_a_new_certificate_before_the_old_one_expires() {
        let dir = tempfile::tempdir().unwrap();
        let now = now();
        let first = CertStore::open_at(self_signed(dir.path(), &[]), now).unwrap();

        let later = now + Duration::days(29) + Duration::hours(1);
        let second = CertStore::open_at(self_signed(dir.path(), &[]), later).unwrap();
        assert_ne!(first.info().fingerprint, second.info().fingerprint);
        assert_eq!(second.info().not_after, later + Duration::days(30));
    }

    #[test]
    fn start_makes_a_new_certificate_when_the_old_one_is_not_valid_yet() {
        let dir = tempfile::tempdir().unwrap();
        let now = now();
        let first =
            CertStore::open_at(self_signed(dir.path(), &[]), now + Duration::days(2)).unwrap();
        let second = CertStore::open_at(self_signed(dir.path(), &[]), now).unwrap();
        assert_ne!(first.info().fingerprint, second.info().fingerprint);
    }

    #[test]
    fn start_makes_a_new_certificate_when_the_names_change() {
        let dir = tempfile::tempdir().unwrap();
        let first = CertStore::open(self_signed(dir.path(), &[])).unwrap();
        let names = ["Desk.Cyberdyne.Example", "192.0.2.10", "2001:DB8::1"];
        let second = CertStore::open(self_signed(dir.path(), &names)).unwrap();
        assert_ne!(first.info().fingerprint, second.info().fingerprint);
        for name in ["desk.cyberdyne.example", "192.0.2.10", "2001:db8::1"] {
            assert!(second.info().names.iter().any(|n| n == name), "{name}");
        }
        // The same names in a different form do not make a new certificate.
        let third = CertStore::open(self_signed(dir.path(), &names)).unwrap();
        assert_eq!(second.info(), third.info());
    }

    #[test]
    fn start_makes_a_new_certificate_when_a_file_is_damaged() {
        let dir = tempfile::tempdir().unwrap();
        let first = CertStore::open(self_signed(dir.path(), &[])).unwrap();
        fs::write(dir.path().join(SELF_SIGNED_KEY), "not a key").unwrap();
        let second = CertStore::open(self_signed(dir.path(), &[])).unwrap();
        assert_ne!(first.info().fingerprint, second.info().fingerprint);
    }

    #[test]
    fn refresh_replaces_a_self_signed_certificate_only_near_its_end() {
        let dir = tempfile::tempdir().unwrap();
        let now = now();
        let store = CertStore::open_at(self_signed(dir.path(), &[]), now).unwrap();
        let first = store.info();

        assert!(!store.refresh_at(now + Duration::days(28)).unwrap());
        assert_eq!(store.info(), first);

        assert!(
            store
                .refresh_at(now + Duration::days(29) + Duration::minutes(1))
                .unwrap()
        );
        assert_ne!(store.info().fingerprint, first.fingerprint);
    }

    #[test]
    fn files_of_the_administrator_load() {
        let dir = tempfile::tempdir().unwrap();
        let made = CertStore::open(self_signed(dir.path(), &["desk.cyberdyne.example"])).unwrap();
        let store = CertStore::open(files(dir.path())).unwrap();
        assert!(!store.info().self_signed);
        assert_eq!(store.info().fingerprint, made.info().fingerprint);
    }

    #[test]
    fn refresh_loads_changed_files_and_keeps_the_old_ones_after_an_error() {
        let dir = tempfile::tempdir().unwrap();
        CertStore::open(self_signed(dir.path(), &[])).unwrap();
        let store = CertStore::open(files(dir.path())).unwrap();
        let first = store.info();
        assert!(!store.refresh().unwrap());

        // Replace the pair, as a certificate client does.
        let other = tempfile::tempdir().unwrap();
        CertStore::open(self_signed(other.path(), &[])).unwrap();
        let cert_path = dir.path().join(SELF_SIGNED_CERT);
        let old_time = mtime(&cert_path).unwrap();
        for name in [SELF_SIGNED_KEY, SELF_SIGNED_CERT] {
            fs::copy(other.path().join(name), dir.path().join(name)).unwrap();
        }
        let file = fs::File::options().write(true).open(&cert_path).unwrap();
        file.set_modified(old_time + std::time::Duration::from_secs(2))
            .unwrap();
        assert!(store.refresh().unwrap());
        let second = store.info();
        assert_ne!(second.fingerprint, first.fingerprint);

        fs::write(&cert_path, "damaged").unwrap();
        let file = fs::File::options().write(true).open(&cert_path).unwrap();
        file.set_modified(old_time + std::time::Duration::from_secs(4))
            .unwrap();
        assert!(store.refresh().is_err());
        assert_eq!(store.info(), second);
    }

    #[test]
    fn key_of_a_different_certificate_is_refused() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        CertStore::open(self_signed(a.path(), &[])).unwrap();
        CertStore::open(self_signed(b.path(), &[])).unwrap();
        let error = CertStore::open(TlsSource::Files {
            cert: a.path().join(SELF_SIGNED_CERT),
            key: b.path().join(SELF_SIGNED_KEY),
        })
        .unwrap_err();
        assert!(matches!(error, TlsError::Pair { .. }), "{error}");
    }

    #[test]
    fn missing_file_gives_a_message_with_the_path() {
        let error = CertStore::open(TlsSource::Files {
            cert: PathBuf::from("/nonexistent/fullchain.pem"),
            key: PathBuf::from("/nonexistent/privkey.pem"),
        })
        .unwrap_err();
        let text = error.to_string();
        assert!(text.contains("/nonexistent/fullchain.pem"), "{text}");
        assert!(text.contains("certificate file"), "{text}");
    }

    #[test]
    fn file_with_no_certificate_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        CertStore::open(self_signed(dir.path(), &[])).unwrap();
        let empty = dir.path().join("empty.pem");
        fs::write(&empty, "").unwrap();
        let error = CertStore::open(TlsSource::Files {
            cert: empty,
            key: dir.path().join(SELF_SIGNED_KEY),
        })
        .unwrap_err();
        assert!(matches!(error, TlsError::NoCertificate { .. }), "{error}");
    }
}
