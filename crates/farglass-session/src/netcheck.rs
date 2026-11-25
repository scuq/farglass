//! The connection checks of the host (F111, ADR-055).
//!
//! The agent runs the checks as the user. The built-in checks are HTTPS to
//! 1.1.1.1 and google.com, also through the proxy of `https_proxy`, and a DNS
//! lookup of google.com. The user adds ICMP and TCP checks in
//! `$XDG_CONFIG_HOME/farglass/netchecks.toml`. The administrator adds checks
//! for all users in `/etc/farglass/netchecks.toml`, which the app does not change.
//!
//! ```toml
//! [[check]]
//! name = "Firewall"
//! kind = "icmp"
//! host = "fw01.cyberdyne.example"
//!
//! [[check]]
//! name = "Git over SSH"
//! kind = "tcp"
//! host = "git.cyberdyne.example"
//! port = 22
//! timeout = 1500
//! ```

use std::collections::HashSet;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use base64::Engine as _;
use farglass_proto::message::{
    NetcheckCheck, NetcheckEvent, NetcheckGroup, NetcheckItem, NetcheckKind, NetcheckProblem,
    NetcheckState,
};
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncBufReadExt as _, AsyncRead, AsyncWrite, AsyncWriteExt as _, BufReader};
use tokio::net::TcpStream;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, TableLike, Value};

/// The checks file of the administrator.
pub const ADMIN_PATH: &str = "/etc/farglass/netchecks.toml";

/// The largest checks file in bytes.
const MAX_FILE: u64 = 1 << 20;
/// The most checks in one file.
pub const MAX_CHECKS: usize = 64;
/// The timeout of a check of a file with no timeout, in milliseconds.
pub const DEFAULT_TIMEOUT: u32 = 2000;
/// The shortest timeout of a check, in milliseconds.
pub const MIN_TIMEOUT: u32 = 100;
/// The longest timeout of a check, in milliseconds.
pub const MAX_TIMEOUT: u32 = 10_000;
/// The timeout of an HTTPS check: DNS, TCP, TLS, and the first line of the answer.
const HTTPS_TIMEOUT: Duration = Duration::from_millis(4000);
/// The timeout of the DNS check.
const DNS_TIMEOUT: Duration = Duration::from_millis(3000);
/// The hosts of the HTTPS checks.
const INTERNET: [(&str, &str); 2] = [("1.1.1.1", "Cloudflare"), ("google.com", "Google")];
/// The name of the DNS check.
const DNS_NAME: &str = "google.com";
/// The CA files of the distributions. `SSL_CERT_FILE` goes first.
const CA_FILES: [&str; 4] = [
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/ssl/ca-bundle.pem",
    "/etc/ssl/cert.pem",
];

/// Returns the path of the checks file of the user: `$XDG_CONFIG_HOME/farglass/netchecks.toml`.
#[must_use]
pub fn default_path() -> PathBuf {
    crate::bookmarks::config_dir().join("netchecks.toml")
}

/// The checks of a file and the problem of each check, by its position.
pub type Parsed = (Vec<NetcheckCheck>, Vec<(usize, String)>);

/// Reads the checks of a file. A check with a kind that is not known gets the kind TCP and a problem.
///
/// # Errors
///
/// A message for the user if the text is not valid TOML.
pub fn parse(text: &str) -> Result<Parsed, String> {
    let doc: DocumentMut = text.parse().map_err(|error| format!("{error}"))?;
    let mut problems = Vec::new();
    let checks = doc
        .get("check")
        .and_then(Item::as_array_of_tables)
        .map(|tables| {
            tables
                .iter()
                .enumerate()
                .map(|(index, table)| {
                    let text = |key: &str| {
                        table
                            .get(key)
                            .and_then(Item::as_str)
                            .unwrap_or_default()
                            .to_owned()
                    };
                    let number = |key: &str| table.get(key).and_then(Item::as_integer);
                    let kind = match text("kind").as_str() {
                        "icmp" => NetcheckKind::Icmp,
                        "tcp" => NetcheckKind::Tcp,
                        other => {
                            problems.push((
                                index,
                                format!("The kind \"{other}\" is not known: use icmp or tcp."),
                            ));
                            NetcheckKind::Tcp
                        }
                    };
                    let port = number("port").map(|port| {
                        u16::try_from(port).unwrap_or_else(|_| {
                            problems.push((index, format!("The port {port} is not valid.")));
                            0
                        })
                    });
                    let timeout = number("timeout").map(|ms| u32::try_from(ms).unwrap_or(0));
                    NetcheckCheck {
                        name: text("name"),
                        kind,
                        host: text("host"),
                        port,
                        timeout,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Ok((checks, problems))
}

/// Checks one check: a name, a host that is a name or an address, a port for TCP, and a timeout in the range.
///
/// # Errors
///
/// A message for the user.
pub fn check_one(check: &NetcheckCheck) -> Result<(), String> {
    if check.name.trim().is_empty() {
        return Err("The check has no name.".to_owned());
    }
    let host = check.host.as_str();
    if host.is_empty() {
        return Err("The check has no host.".to_owned());
    }
    if host.len() > 253
        || host.starts_with('-')
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'))
    {
        return Err(format!(
            "The host \"{host}\" is not a host name or an IP address."
        ));
    }
    if check.kind == NetcheckKind::Tcp && check.port.is_none_or(|port| port == 0) {
        return Err("A TCP check needs a port from 1 to 65535.".to_owned());
    }
    if let Some(timeout) = check.timeout
        && !(MIN_TIMEOUT..=MAX_TIMEOUT).contains(&timeout)
    {
        return Err(format!(
            "The timeout must be from {MIN_TIMEOUT} to {MAX_TIMEOUT} ms."
        ));
    }
    Ok(())
}

/// Checks the checks before a save: each check is valid, each name is unique, and there are at most 64.
///
/// # Errors
///
/// A message for the user.
pub fn check_all(checks: &[NetcheckCheck]) -> Result<(), String> {
    if checks.len() > MAX_CHECKS {
        return Err(format!("A file can have at most {MAX_CHECKS} checks."));
    }
    let mut names = HashSet::new();
    for check in checks {
        let name = check.name.trim();
        check_one(check).map_err(|error| {
            if name.is_empty() {
                error
            } else {
                format!("{name}: {error}")
            }
        })?;
        if name != check.name {
            return Err(format!(
                "The name \"{}\" must not start or end with a space.",
                check.name
            ));
        }
        if !names.insert(name) {
            return Err(format!("Two checks have the name {name}."));
        }
    }
    Ok(())
}

/// Returns the text of the file with `checks`. The comments and the layout of
/// `old` stay: a check keeps its table if its name stays.
#[must_use]
pub fn render(old: &DocumentMut, checks: &[NetcheckCheck]) -> String {
    let mut doc = old.clone();
    let mut old_tables: Vec<Table> = old
        .get("check")
        .and_then(Item::as_array_of_tables)
        .map(|tables| tables.iter().cloned().collect())
        .unwrap_or_default();
    // The comment at the top of the file is the decor of the first table. The part up to its last
    // empty line belongs to the file and stays at the top. The rest belongs to the check.
    let mut header = String::new();
    if let Some(first) = old_tables.first_mut() {
        let prefix = first
            .decor()
            .prefix()
            .and_then(|prefix| prefix.as_str())
            .unwrap_or_default()
            .to_owned();
        if let Some(end) = prefix.rfind("\n\n") {
            header = prefix[..end + 2].to_owned();
            first.decor_mut().set_prefix(&prefix[end + 2..]);
        }
    }
    let mut tables = ArrayOfTables::new();
    for check in checks {
        let mut table = old_tables
            .iter()
            .find(|table| table.get("name").and_then(Item::as_str) == Some(&check.name))
            .cloned()
            .unwrap_or_default();
        set(&mut table, "name", check.name.as_str().into());
        let kind = match check.kind {
            NetcheckKind::Icmp => "icmp",
            NetcheckKind::Tcp => "tcp",
        };
        set(&mut table, "kind", kind.into());
        set(&mut table, "host", check.host.as_str().into());
        match check.port {
            Some(port) if check.kind == NetcheckKind::Tcp => {
                set(&mut table, "port", i64::from(port).into());
            }
            _ => {
                table.remove("port");
            }
        }
        match check.timeout {
            Some(timeout) => set(&mut table, "timeout", i64::from(timeout).into()),
            None => {
                table.remove("timeout");
            }
        }
        if tables.is_empty() && !header.is_empty() {
            let prefix = table
                .decor()
                .prefix()
                .and_then(|prefix| prefix.as_str())
                .unwrap_or_default()
                .to_owned();
            table
                .decor_mut()
                .set_prefix(format!("{}{prefix}", std::mem::take(&mut header)));
        }
        tables.push(table);
    }
    doc.remove("check");
    if !tables.is_empty() {
        doc.insert("check", Item::ArrayOfTables(tables));
    }
    doc.to_string()
}

/// Sets `key` of `table` to `new`. A value that was there keeps its comments.
fn set(table: &mut dyn TableLike, key: &str, mut new: Value) {
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(old) => {
            *new.decor_mut() = old.decor().clone();
            *old = new;
        }
        None => {
            table.insert(key, Item::Value(new));
        }
    }
}

/// The text of a new file: a comment that tells the format.
const NEW_FILE: &str = "\
# The connection checks of farglass. The Network check app shows them.
#
# kind: icmp (a ping) or tcp (a connection to the port). host: a host name or an IP address.
# port: the port of a tcp check. timeout: the time to wait in ms, 100 to 10000 (default 2000).
";

/// An HTTP proxy for the HTTPS checks: the value of `https_proxy`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proxy {
    /// The host of the proxy.
    pub host: String,
    /// The port of the proxy.
    pub port: u16,
    /// The value of a `Proxy-Authorization` header, from the user and password of the URL.
    authorization: Option<String>,
}

impl Proxy {
    /// Returns the proxy of `https_proxy`, `HTTPS_PROXY`, `all_proxy`, or `ALL_PROXY`, if it is an `http://` URL.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        ["https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"]
            .iter()
            .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
            .and_then(|value| Self::parse(&value))
    }

    /// Returns the proxy of the URL `value`. A value with no scheme is an `http://` URL.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let value = if value.contains("://") {
            value.to_owned()
        } else {
            format!("http://{value}")
        };
        let url = url::Url::parse(&value).ok()?;
        if url.scheme() != "http" {
            return None;
        }
        let host = url.host_str()?.trim_matches(['[', ']']).to_owned();
        let authorization = (!url.username().is_empty()).then(|| {
            let user = percent_decode(url.username());
            let password = percent_decode(url.password().unwrap_or_default());
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
            )
        });
        Some(Self {
            host,
            port: url.port().unwrap_or(80),
            authorization,
        })
    }

    /// The host and the port, for the user. It never shows the password.
    #[must_use]
    pub fn display(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// Decodes the `%XX` escapes of a part of a URL.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What a check does.
#[derive(Clone, Debug)]
enum Probe {
    /// HTTPS to port 443 of the host, directly or through the proxy.
    Https { host: String, proxy: Option<Proxy> },
    /// A lookup of the name with the resolver of the system.
    Dns { name: String, servers: Vec<String> },
    /// An ICMP echo request.
    Icmp { host: String },
    /// A TCP connection.
    Tcp { host: String, port: u16 },
    /// A check of a file that is not valid.
    Invalid(String),
}

/// A check to run: its id, what it does, and its timeout.
#[derive(Clone, Debug)]
pub struct Job {
    /// The id of [`NetcheckItem::id`].
    pub id: String,
    probe: Probe,
    timeout: Duration,
}

/// The checks files of the session and the checks that they name.
#[derive(Debug)]
pub struct Netcheck {
    path: PathBuf,
    admin_path: PathBuf,
    proxy: Option<Proxy>,
    /// The file with the own IP address of the host: `~/.egressip`.
    outgoing_ip: PathBuf,
    /// The CA certificates of the system, read at the first HTTPS check.
    tls: OnceLock<Result<Arc<rustls::ClientConfig>, String>>,
    /// Only one save at a time.
    lock: Mutex<()>,
}

/// The checks of a file and their problems, or the cause why the file is not valid.
struct FileChecks {
    checks: Vec<NetcheckCheck>,
    problems: Vec<(usize, String)>,
    error: Option<String>,
}

impl Netcheck {
    /// Returns the checks of the file of the user `path` and of the file of the administrator `admin_path`.
    /// The HTTPS checks also go through `proxy`. The own IP address comes from `~/.egressip`.
    #[must_use]
    pub fn new(path: PathBuf, admin_path: PathBuf, proxy: Option<Proxy>) -> Arc<Self> {
        Self::with_outgoing_ip(
            path,
            admin_path,
            crate::terminal::home_dir().join(".egressip"),
            proxy,
        )
    }

    /// Returns the checks as [`Netcheck::new`], with the own IP address from the file `outgoing_ip`.
    #[must_use]
    pub fn with_outgoing_ip(
        path: PathBuf,
        admin_path: PathBuf,
        outgoing_ip: PathBuf,
        proxy: Option<Proxy>,
    ) -> Arc<Self> {
        Arc::new(Self {
            path,
            admin_path,
            proxy,
            outgoing_ip,
            tls: OnceLock::new(),
            lock: Mutex::new(()),
        })
    }

    /// Reads one file: its checks, the problem of each check, and the cause if the file is not valid.
    fn read_checks(path: &Path) -> FileChecks {
        match read(path).and_then(|text| text.map_or(Ok(Default::default()), |t| parse(&t))) {
            Ok((checks, mut problems)) => {
                let mut names = HashSet::new();
                for (index, check) in checks.iter().enumerate() {
                    if let Err(message) = check_one(check) {
                        problems.push((index, message));
                    } else if !names.insert(check.name.as_str()) {
                        problems.push((index, format!("Two checks have the name {}.", check.name)));
                    }
                }
                if checks.len() > MAX_CHECKS {
                    problems.push((
                        MAX_CHECKS,
                        format!("Only the first {MAX_CHECKS} checks run."),
                    ));
                }
                FileChecks {
                    checks,
                    problems,
                    error: None,
                }
            }
            Err(error) => FileChecks {
                checks: Vec::new(),
                problems: Vec::new(),
                error: Some(error),
            },
        }
    }

    /// Reads the name servers of `/etc/resolv.conf`.
    fn name_servers() -> Vec<String> {
        std::fs::read_to_string("/etc/resolv.conf")
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.trim().strip_prefix("nameserver"))
            .map(|server| server.trim().to_owned())
            .filter(|server| !server.is_empty())
            .collect()
    }

    /// Returns the checks in the order to show, with what each one does.
    fn jobs(&self) -> (Vec<(NetcheckItem, Job)>, FileChecks, FileChecks) {
        let mut jobs = Vec::new();
        let mut add = |id: String, group, name: String, target: String, probe, timeout| {
            jobs.push((
                NetcheckItem {
                    id: id.clone(),
                    group,
                    name,
                    target,
                },
                Job { id, probe, timeout },
            ));
        };
        for (host, name) in INTERNET {
            add(
                format!("internet:{host}"),
                NetcheckGroup::Internet,
                name.to_owned(),
                format!("https://{host}"),
                Probe::Https {
                    host: host.to_owned(),
                    proxy: None,
                },
                HTTPS_TIMEOUT,
            );
        }
        if let Some(proxy) = &self.proxy {
            for (host, name) in INTERNET {
                add(
                    format!("proxy:{host}"),
                    NetcheckGroup::Internet,
                    format!("{name} through the proxy"),
                    format!("https://{host} through {}", proxy.display()),
                    Probe::Https {
                        host: host.to_owned(),
                        proxy: Some(proxy.clone()),
                    },
                    HTTPS_TIMEOUT,
                );
            }
        }
        let servers = Self::name_servers();
        let target = if servers.is_empty() {
            DNS_NAME.to_owned()
        } else {
            format!("{DNS_NAME} with {}", servers.join(", "))
        };
        add(
            "dns".to_owned(),
            NetcheckGroup::Dns,
            "Name resolution".to_owned(),
            target,
            Probe::Dns {
                name: DNS_NAME.to_owned(),
                servers,
            },
            DNS_TIMEOUT,
        );
        let admin = Self::read_checks(&self.admin_path);
        let user = Self::read_checks(&self.path);
        for (group, prefix, file) in [
            (NetcheckGroup::Admin, "admin", &admin),
            (NetcheckGroup::User, "user", &user),
        ] {
            for (index, check) in file.checks.iter().enumerate().take(MAX_CHECKS) {
                let problem = file
                    .problems
                    .iter()
                    .find(|(i, _)| *i == index)
                    .map(|(_, message)| message.clone());
                let (target, probe) = match check.kind {
                    NetcheckKind::Icmp => (
                        format!("ICMP {}", check.host),
                        Probe::Icmp {
                            host: check.host.clone(),
                        },
                    ),
                    NetcheckKind::Tcp => {
                        let port = check.port.unwrap_or(0);
                        let target = if check.host.contains(':') {
                            format!("TCP [{}]:{port}", check.host)
                        } else {
                            format!("TCP {}:{port}", check.host)
                        };
                        (
                            target,
                            Probe::Tcp {
                                host: check.host.clone(),
                                port,
                            },
                        )
                    }
                };
                let timeout = check
                    .timeout
                    .unwrap_or(DEFAULT_TIMEOUT)
                    .clamp(MIN_TIMEOUT, MAX_TIMEOUT);
                add(
                    format!("{prefix}:{index}"),
                    group,
                    check.name.clone(),
                    target,
                    problem.map_or(probe, Probe::Invalid),
                    Duration::from_millis(u64::from(timeout)),
                );
            }
        }
        (jobs, admin, user)
    }

    /// Reads the files and returns all checks.
    #[must_use]
    pub fn load(&self) -> NetcheckEvent {
        let (jobs, admin, user) = self.jobs();
        let mut problems = Vec::new();
        for (group, file) in [(NetcheckGroup::Admin, &admin), (NetcheckGroup::User, &user)] {
            for (index, message) in &file.problems {
                if let Ok(index) = u32::try_from(*index) {
                    problems.push(NetcheckProblem {
                        group,
                        index,
                        message: message.clone(),
                    });
                }
            }
        }
        // A broken file of the administrator is a problem of the whole file: an index after its last check.
        if let Some(error) = &admin.error {
            tracing::warn!(%error, "the checks file of the administrator is not valid");
            problems.push(NetcheckProblem {
                group: NetcheckGroup::Admin,
                index: 0,
                message: format!(
                    "The file {} is not valid: {error}",
                    self.admin_path.display()
                ),
            });
        }
        NetcheckEvent::Loaded {
            path: self.path.display().to_string(),
            admin_path: self.admin_path.display().to_string(),
            user: user.checks,
            items: jobs.into_iter().map(|(item, _)| item).collect(),
            problems,
            proxy: self.proxy.as_ref().map(Proxy::display),
            outgoing_ip: read_outgoing_ip(&self.outgoing_ip),
            error: user.error,
        }
    }

    /// Writes the checks of the user and returns all checks. The file does not change if a check is
    /// not valid, or if the file on the disk is not valid TOML: a save does not overwrite a change by hand.
    ///
    /// # Errors
    ///
    /// A message for the user.
    pub fn save(&self, checks: &[NetcheckCheck]) -> Result<NetcheckEvent, String> {
        check_all(checks)?;
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let old = read(&self.path)?;
        let doc: DocumentMut = old.as_deref().unwrap_or_default().parse().map_err(|error| {
            format!("The file is not valid TOML, thus farglass does not save over it. Correct the file first: {error}")
        })?;
        let mut text = render(&doc, checks);
        if old.is_none() {
            text = format!("{NEW_FILE}\n{text}");
        }
        crate::bookmarks::write_atomic(&self.path, text.as_bytes())
            .map_err(|error| format!("farglass cannot write the file: {error}."))?;
        Ok(self.load())
    }

    /// Returns the checks of `ids`, or all checks with none. It reads the files: call it on a blocking thread.
    #[must_use]
    pub fn plan(&self, ids: Option<&[String]>) -> Vec<Job> {
        let (jobs, _, _) = self.jobs();
        let jobs: Vec<Job> = jobs
            .into_iter()
            .map(|(_, job)| job)
            .filter(|job| ids.is_none_or(|ids| ids.contains(&job.id)))
            .collect();
        if jobs
            .iter()
            .any(|job| matches!(job.probe, Probe::Https { .. }))
        {
            let _ = self.tls();
        }
        jobs
    }

    /// Returns the TLS configuration with the CA certificates of the system. It reads them at the first call.
    fn tls(&self) -> Result<Arc<rustls::ClientConfig>, String> {
        self.tls.get_or_init(tls_config).clone()
    }

    /// Runs `job` and returns its result.
    pub async fn run(&self, job: Job) -> NetcheckEvent {
        let start = Instant::now();
        let stage = Stage::default();
        let result = tokio::time::timeout(job.timeout, async {
            match &job.probe {
                Probe::Https { host, proxy } => {
                    let tls = self.tls()?;
                    https(host, proxy.as_ref(), tls, &stage).await
                }
                Probe::Dns { name, servers } => dns(name, servers).await,
                Probe::Icmp { host } => icmp(host, job.timeout, &stage).await,
                Probe::Tcp { host, port } => tcp(host, *port, &stage).await,
                Probe::Invalid(message) => Err(format!("The check does not run: {message}")),
            }
        })
        .await;
        let ms = u32::try_from(start.elapsed().as_millis()).unwrap_or(u32::MAX);
        let (state, ms, detail) = match result {
            Ok(Ok(detail)) => (NetcheckState::Ok, Some(ms), detail),
            Ok(Err(detail)) => (NetcheckState::Failed, None, detail),
            Err(_) => {
                let waiting = match stage.get() {
                    "" => String::new(),
                    stage => format!(" The check waited for {stage}."),
                };
                (
                    NetcheckState::Timeout,
                    None,
                    format!(
                        "No answer in {} ms: a firewall can drop the packets.{waiting}",
                        job.timeout.as_millis()
                    ),
                )
            }
        };
        NetcheckEvent::Result {
            id: job.id,
            state,
            ms,
            detail,
        }
    }
}

/// The step that a check waits for, for the message of a timeout.
#[derive(Debug, Default)]
struct Stage(Mutex<&'static str>);

impl Stage {
    fn set(&self, stage: &'static str) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = stage;
    }

    fn get(&self) -> &'static str {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Reads the file `path`. `Ok(None)` if it does not exist.
fn read(path: &Path) -> Result<Option<String>, String> {
    match std::fs::metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("farglass cannot read the file: {error}.")),
        Ok(meta) if meta.len() > MAX_FILE => {
            return Err("The file is larger than 1 MiB.".to_owned());
        }
        Ok(_) => {}
    }
    std::fs::read_to_string(path)
        .map(Some)
        .map_err(|error| format!("farglass cannot read the file: {error}."))
}

/// The longest own IP address that the page shows.
const MAX_OUTGOING_IP: usize = 100;

/// Returns the first line of the file `path` with no spaces at its ends: the own IP address of the host.
/// Returns none if the file does not exist, cannot be read, or has no text.
fn read_outgoing_ip(path: &Path) -> Option<String> {
    use std::io::Read as _;
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(4096)
        .read_to_string(&mut text)
        .ok()?;
    let line = text.lines().next()?.trim();
    let line: String = line.chars().take(MAX_OUTGOING_IP).collect();
    (!line.is_empty()).then_some(line)
}

/// Returns a TLS configuration with the CA certificates of the system.
fn tls_config() -> Result<Arc<rustls::ClientConfig>, String> {
    let files = std::env::var("SSL_CERT_FILE")
        .ok()
        .into_iter()
        .chain(CA_FILES.iter().map(|file| (*file).to_owned()));
    let mut roots = rustls::RootCertStore::empty();
    for file in files {
        let Ok(certs) = CertificateDer::pem_file_iter(&file) else {
            continue;
        };
        let (added, _) = roots.add_parsable_certificates(certs.filter_map(Result::ok));
        if added > 0 {
            break;
        }
    }
    if roots.is_empty() {
        return Err(
            "The host has no CA certificates: install the package ca-certificates.".to_owned(),
        );
    }
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| error.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// Returns the addresses of `host`, an IP address or a name.
async fn resolve(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| format!("The name {host} does not resolve: {error}."))?
        .collect();
    if addrs.is_empty() {
        return Err(format!("The name {host} has no address."));
    }
    Ok(addrs)
}

/// Returns the cause of a failed connection, for the user.
fn cause(error: &io::Error, port: u16) -> String {
    match error.kind() {
        io::ErrorKind::ConnectionRefused => format!(
            "Refused: the host answered, but no program listens on port {port}, or a firewall rejects the connection."
        ),
        io::ErrorKind::NetworkUnreachable => "The network is not reachable: no route.".to_owned(),
        io::ErrorKind::HostUnreachable => "The host is not reachable: no route.".to_owned(),
        io::ErrorKind::PermissionDenied => {
            "The host does not permit the connection: a local firewall rule.".to_owned()
        }
        _ => format!("The connection failed: {error}."),
    }
}

/// Connects to the first address of `addrs` that accepts.
async fn connect(addrs: &[SocketAddr], port: u16) -> Result<TcpStream, String> {
    let mut last = None;
    for addr in addrs {
        match TcpStream::connect(addr).await {
            Ok(stream) => return Ok(stream),
            Err(error) => last = Some(error),
        }
    }
    Err(last.map_or_else(|| "No address.".to_owned(), |error| cause(&error, port)))
}

/// Opens a TCP connection to `host` and `port`.
async fn tcp(host: &str, port: u16, stage: &Stage) -> Result<String, String> {
    stage.set("the name resolution");
    let addrs = resolve(host, port).await?;
    stage.set("the TCP connection");
    let stream = connect(&addrs, port).await?;
    let peer = stream
        .peer_addr()
        .map_or_else(|_| String::new(), |addr| format!(" at {}", addr.ip()));
    Ok(format!("Open{peer}."))
}

/// Resolves `name` with the resolver of the system.
async fn dns(name: &str, servers: &[String]) -> Result<String, String> {
    let mut ips: Vec<IpAddr> = resolve(name, 443)
        .await?
        .into_iter()
        .map(|addr| addr.ip())
        .collect();
    ips.dedup();
    let ips: Vec<String> = ips.iter().take(4).map(ToString::to_string).collect();
    let via = if servers.iter().any(|server| server == "127.0.0.53") {
        " through systemd-resolved".to_owned()
    } else {
        String::new()
    };
    Ok(format!("{name} is {}{via}.", ips.join(", ")))
}

/// Opens HTTPS to port 443 of `host`, directly or through `proxy`, and reads the status of a HEAD request.
async fn https(
    host: &str,
    proxy: Option<&Proxy>,
    tls: Arc<rustls::ClientConfig>,
    stage: &Stage,
) -> Result<String, String> {
    let stream = match proxy {
        None => {
            stage.set("the name resolution");
            let addrs = resolve(host, 443).await?;
            stage.set("the TCP connection");
            connect(&addrs, 443).await?
        }
        Some(proxy) => {
            stage.set("the name resolution of the proxy");
            let addrs = resolve(&proxy.host, proxy.port).await?;
            stage.set("the connection to the proxy");
            let mut stream = connect(&addrs, proxy.port)
                .await
                .map_err(|error| format!("The proxy: {error}"))?;
            stage.set("the answer of the proxy");
            tunnel(&mut stream, proxy, host).await?;
            stream
        }
    };
    let name = ServerName::try_from(host.to_owned())
        .map_err(|_| format!("{host} is not a valid server name."))?;
    stage.set("the TLS handshake");
    let mut stream = tokio_rustls::TlsConnector::from(tls)
        .connect(name, stream)
        .await
        .map_err(|error| tls_cause(&error))?;
    stage.set("the HTTP answer");
    let request = format!(
        "HEAD / HTTP/1.1\r\nHost: {host}\r\nUser-Agent: farglass-netcheck\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|error| format!("The request failed: {error}."))?;
    let status = status_line(&mut stream).await?;
    Ok(format!("HTTPS works: the server answered HTTP {status}."))
}

/// Returns the cause of a failed TLS handshake, for the user.
fn tls_cause(error: &io::Error) -> String {
    let tls = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>());
    match tls {
        Some(rustls::Error::InvalidCertificate(certificate)) => format!(
            "The certificate is not valid: {certificate:?}. A proxy that inspects TLS can cause this."
        ),
        Some(other) => format!("The TLS handshake failed: {other}."),
        None => format!("The TLS handshake failed: {error}."),
    }
}

/// Asks `proxy` on `stream` for a tunnel to port 443 of `host`.
async fn tunnel(stream: &mut TcpStream, proxy: &Proxy, host: &str) -> Result<(), String> {
    let target = if host.contains(':') {
        format!("[{host}]:443")
    } else {
        format!("{host}:443")
    };
    let mut request = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if let Some(authorization) = &proxy.authorization {
        request.push_str(&format!("Proxy-Authorization: {authorization}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|error| format!("The proxy: {error}."))?;
    let mut reader = BufReader::new(stream);
    let status = status_line(&mut reader).await?;
    // The headers of the answer end with an empty line. The tunnel starts after it.
    let mut total = 0;
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .await
            .map_err(|error| format!("The proxy: {error}."))?;
        total += n;
        if n == 0 || line == "\r\n" || line == "\n" || total > 8192 {
            break;
        }
    }
    if !reader.buffer().is_empty() {
        return Err("The proxy sent data after its answer.".to_owned());
    }
    match status {
        200..=299 => Ok(()),
        407 => Err("The proxy needs a user and a password (HTTP 407).".to_owned()),
        403 => Err(format!(
            "The proxy refuses the connection to {host} (HTTP 403)."
        )),
        other => Err(format!("The proxy answered HTTP {other}.")),
    }
}

/// Reads the status line of an HTTP answer and returns the status code.
async fn status_line<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut S) -> Result<u16, String> {
    let mut line = Vec::new();
    let mut byte = [0_u8; 1];
    // One byte at a time: the bytes after the line stay in the stream.
    while line.len() < 1024 {
        let n = tokio::io::AsyncReadExt::read(stream, &mut byte)
            .await
            .map_err(|error| format!("The answer failed: {error}."))?;
        if n == 0 || byte[0] == b'\n' {
            break;
        }
        line.push(byte[0]);
    }
    let line = String::from_utf8_lossy(&line);
    line.split_whitespace()
        .nth(1)
        .filter(|_| line.starts_with("HTTP/"))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| "The answer is not HTTP.".to_owned())
}

/// Sends an ICMP echo request to `host`. It uses the ping socket of the kernel, and the program
/// `ping` if the kernel does not permit the socket to the user (`net.ipv4.ping_group_range`).
async fn icmp(host: &str, timeout: Duration, stage: &Stage) -> Result<String, String> {
    stage.set("the name resolution");
    let addrs = resolve(host, 0).await?;
    let ip = addrs
        .iter()
        .map(SocketAddr::ip)
        .find(IpAddr::is_ipv4)
        .unwrap_or_else(|| addrs[0].ip());
    stage.set("the echo reply");
    match ping_socket(ip).await {
        Err(PingError::NotPermitted) => ping_program(ip, timeout).await,
        Err(PingError::Failed(message)) => Err(message),
        Ok(()) => Ok(format!("{ip} answered.")),
    }
}

/// Why the ping socket did not get a reply.
enum PingError {
    /// The kernel does not permit the socket to the user.
    NotPermitted,
    /// The host or the network sent an error.
    Failed(String),
}

/// Sends an echo request on a ping socket and waits for the reply.
async fn ping_socket(ip: IpAddr) -> Result<(), PingError> {
    use nix::sys::socket::{AddressFamily, SockFlag, SockProtocol, SockType, socket};
    let (family, protocol, request, reply) = match ip {
        IpAddr::V4(_) => (AddressFamily::Inet, SockProtocol::Icmp, 8, 0),
        IpAddr::V6(_) => (AddressFamily::Inet6, SockProtocol::IcmpV6, 128, 129),
    };
    let fd = socket(
        family,
        SockType::Datagram,
        SockFlag::SOCK_NONBLOCK | SockFlag::SOCK_CLOEXEC,
        protocol,
    )
    .map_err(|errno| match errno {
        nix::errno::Errno::EACCES | nix::errno::Errno::EPERM => PingError::NotPermitted,
        other => PingError::Failed(format!("The ping socket failed: {other}.")),
    })?;
    let socket = tokio::net::UdpSocket::from_std(std::net::UdpSocket::from(fd))
        .map_err(|error| PingError::Failed(format!("The ping socket failed: {error}.")))?;
    let sequence = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |time| (time.subsec_nanos() >> 10) as u16);
    // Type, code, checksum, identifier, sequence, and 8 bytes of data. The kernel sets the checksum and the identifier.
    let mut packet = [0_u8; 16];
    packet[0] = request;
    packet[6..8].copy_from_slice(&sequence.to_be_bytes());
    packet[8..].copy_from_slice(b"farglass");
    socket
        .send_to(&packet, SocketAddr::new(ip, 0))
        .await
        .map_err(|error| PingError::Failed(cause(&error, 0)))?;
    let mut buffer = [0_u8; 1500];
    loop {
        let (n, _) = socket
            .recv_from(&mut buffer)
            .await
            .map_err(|error| PingError::Failed(cause(&error, 0)))?;
        if n >= 8 && buffer[0] == reply && buffer[6..8] == sequence.to_be_bytes() {
            return Ok(());
        }
    }
}

/// Runs the program `ping` for one echo request.
async fn ping_program(ip: IpAddr, timeout: Duration) -> Result<String, String> {
    let seconds = timeout.as_millis().div_ceil(1000).max(1).to_string();
    let output = tokio::process::Command::new("ping")
        .args(["-n", "-c", "1", "-W", &seconds, "--", &ip.to_string()])
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|error| {
            format!("The kernel does not permit ICMP to this user, and the program ping does not start: {error}.")
        })?;
    match output.status.code() {
        Some(0) => Ok(format!("{ip} answered.")),
        Some(1) => Err(format!("{ip} did not answer.")),
        _ => Err(format!(
            "ping failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tcp_check(name: &str, host: &str, port: u16) -> NetcheckCheck {
        NetcheckCheck {
            name: name.to_owned(),
            kind: NetcheckKind::Tcp,
            host: host.to_owned(),
            port: Some(port),
            timeout: None,
        }
    }

    #[test]
    fn parse_reads_each_check_and_marks_an_unknown_kind() {
        let (checks, problems) = parse(
            "[[check]]\nname = \"fw\"\nkind = \"icmp\"\nhost = \"10.0.0.1\"\n\n\
             [[check]]\nname = \"git\"\nkind = \"tcp\"\nhost = \"git.example\"\nport = 22\ntimeout = 500\n\n\
             [[check]]\nname = \"web\"\nkind = \"http\"\nhost = \"web.example\"\n",
        )
        .unwrap();
        assert_eq!(checks.len(), 3);
        assert_eq!(checks[0].kind, NetcheckKind::Icmp);
        assert_eq!(checks[1], {
            let mut git = tcp_check("git", "git.example", 22);
            git.timeout = Some(500);
            git
        });
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].0, 2);
        assert!(parse("[[check]\n").is_err());
    }

    #[test]
    fn check_refuses_a_bad_host_a_missing_port_a_bad_timeout_and_a_double_name() {
        assert!(check_all(&[tcp_check("a", "host.example", 443)]).is_ok());
        assert!(check_all(&[tcp_check("a", "-oProxyCommand", 443)]).is_err());
        assert!(check_all(&[tcp_check("a", "two words", 443)]).is_err());
        assert!(check_all(&[tcp_check("", "host.example", 443)]).is_err());
        assert!(check_all(&[tcp_check(" a", "host.example", 443)]).is_err());
        assert!(check_all(&[tcp_check("a", "host.example", 0)]).is_err());
        let mut slow = tcp_check("a", "host.example", 443);
        slow.timeout = Some(60_000);
        assert!(check_all(&[slow]).is_err());
        assert!(
            check_all(&[
                tcp_check("a", "host.example", 443),
                tcp_check("a", "other.example", 443)
            ])
            .is_err()
        );
        let mut ping = tcp_check("p", "fe80::1", 0);
        ping.kind = NetcheckKind::Icmp;
        ping.port = None;
        assert!(check_all(&[ping]).is_ok());
    }

    #[test]
    fn a_save_keeps_the_comments_and_writes_a_new_file() {
        let old: DocumentMut = "# my checks\n\n# the firewall\n[[check]]\nname = \"fw\" # main\nkind = \"icmp\"\nhost = \"10.0.0.1\"\n\n\
                                [[check]]\nname = \"old\"\nkind = \"tcp\"\nhost = \"old.example\"\nport = 80\n"
            .parse()
            .unwrap();
        let mut fw = tcp_check("fw", "10.0.0.1", 0);
        fw.kind = NetcheckKind::Icmp;
        fw.port = None;
        fw.timeout = Some(800);
        let checks = vec![tcp_check("new", "new.example", 443), fw];
        let text = render(&old, &checks);
        assert!(text.starts_with("# my checks\n"), "{text}");
        assert!(text.contains("# the firewall\n"), "{text}");
        assert!(text.contains("name = \"fw\" # main"), "{text}");
        assert!(!text.contains("old.example"), "{text}");
        assert_eq!(parse(&text).unwrap().0, checks);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("farglass/netchecks.toml");
        let netcheck = Netcheck::new(path.clone(), dir.path().join("admin.toml"), None);
        let NetcheckEvent::Loaded { user, items, .. } = netcheck.save(&checks).unwrap() else {
            panic!("a save answers with the checks");
        };
        assert_eq!(user, checks);
        assert!(items.iter().any(|item| item.id == "user:1"));
        assert!(items.iter().all(|item| !item.id.starts_with("proxy:")));
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(
            saved.starts_with("# The connection checks of farglass."),
            "{saved}"
        );

        std::fs::write(&path, "[[check]\n").unwrap();
        assert!(netcheck.save(&checks).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[[check]\n");
    }

    #[test]
    fn the_admin_checks_load_and_the_proxy_adds_checks() {
        let dir = tempfile::tempdir().unwrap();
        let admin = dir.path().join("admin.toml");
        std::fs::write(
            &admin,
            "[[check]]\nname = \"vpn\"\nkind = \"tcp\"\nhost = \"vpn.example\"\nport = 443\n\
             [[check]]\nname = \"broken\"\nkind = \"tcp\"\nhost = \"x.example\"\n",
        )
        .unwrap();
        let proxy = Proxy::parse("http://me:p%40ss@proxy.example:3128");
        let netcheck = Netcheck::new(dir.path().join("user.toml"), admin, proxy);
        let NetcheckEvent::Loaded {
            items,
            problems,
            proxy,
            ..
        } = netcheck.load()
        else {
            panic!("a load answers with the checks");
        };
        assert_eq!(proxy.as_deref(), Some("proxy.example:3128"));
        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "internet:1.1.1.1",
                "internet:google.com",
                "proxy:1.1.1.1",
                "proxy:google.com",
                "dns",
                "admin:0",
                "admin:1"
            ]
        );
        assert_eq!(items[5].target, "TCP vpn.example:443");
        assert_eq!(problems.len(), 1);
        assert_eq!(
            (problems[0].group, problems[0].index),
            (NetcheckGroup::Admin, 1)
        );
    }

    #[test]
    fn the_own_ip_address_is_the_first_line_of_the_file_or_none() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(".egressip");
        let netcheck = Netcheck::with_outgoing_ip(
            dir.path().join("user.toml"),
            dir.path().join("admin.toml"),
            file.clone(),
            None,
        );
        let ip = || {
            let NetcheckEvent::Loaded { outgoing_ip, .. } = netcheck.load() else {
                panic!("a load answers with the checks");
            };
            outgoing_ip
        };
        assert_eq!(ip(), None, "no file gives no address");
        std::fs::write(&file, "  203.0.113.7 \nsecond line\n").unwrap();
        assert_eq!(ip().as_deref(), Some("203.0.113.7"));
        std::fs::write(&file, "\n").unwrap();
        assert_eq!(ip(), None, "an empty file gives no address");
    }

    #[test]
    fn a_proxy_url_gives_the_host_the_port_and_the_authorization() {
        let proxy = Proxy::parse("http://me:p%40ss@proxy.example:3128/").unwrap();
        assert_eq!(proxy.display(), "proxy.example:3128");
        assert_eq!(
            proxy.authorization.as_deref(),
            Some(
                format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode("me:p@ss")
                )
                .as_str()
            )
        );
        assert_eq!(Proxy::parse("proxy.example:8080").unwrap().port, 8080);
        assert!(Proxy::parse("socks5://proxy.example:1080").is_none());
    }

    #[tokio::test]
    async fn a_tcp_check_tells_open_from_refused_and_an_invalid_check_fails() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let open = listener.local_addr().unwrap().port();
        let closed = {
            let other = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            other.local_addr().unwrap().port()
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("user.toml");
        let netcheck = Netcheck::new(path, dir.path().join("admin.toml"), None);
        std::fs::write(
            dir.path().join("user.toml"),
            format!(
                "[[check]]\nname = \"open\"\nkind = \"tcp\"\nhost = \"127.0.0.1\"\nport = {open}\n\
                 [[check]]\nname = \"closed\"\nkind = \"tcp\"\nhost = \"127.0.0.1\"\nport = {closed}\n\
                 [[check]]\nname = \"bad\"\nkind = \"tcp\"\nhost = \"127.0.0.1\"\nport = 1\ntimeout = 5\n\
                 [[check]]\nname = \"ping\"\nkind = \"icmp\"\nhost = \"127.0.0.1\"\n"
            ),
        )
        .unwrap();
        let ids = ["user:0", "user:1", "user:2", "user:3"].map(str::to_owned);
        let jobs = netcheck.plan(Some(&ids));
        assert_eq!(jobs.len(), 4);
        let mut results = Vec::new();
        for job in jobs {
            let NetcheckEvent::Result {
                state, detail, ms, ..
            } = netcheck.run(job).await
            else {
                panic!("a run answers with a result");
            };
            results.push((state, detail, ms));
        }
        assert_eq!(results[0].0, NetcheckState::Ok, "{:?}", results[0]);
        assert!(results[0].2.is_some());
        assert_eq!(results[1].0, NetcheckState::Failed, "{:?}", results[1]);
        assert!(results[1].1.starts_with("Refused"), "{:?}", results[1]);
        assert_eq!(results[2].0, NetcheckState::Failed, "{:?}", results[2]);
        assert!(results[2].1.contains("timeout"), "{:?}", results[2]);
        assert_eq!(results[3].0, NetcheckState::Ok, "{:?}", results[3]);
        drop(listener);
    }
}
