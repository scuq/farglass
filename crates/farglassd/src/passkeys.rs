//! Passkeys as a second factor after the PAM login (F034).
//!
//! farglassd keeps the passkeys of each user in `STATE_DIR/passkeys/UID.json`,
//! readable only by farglassd. A user with at least one passkey must use it, or
//! a recovery code, after PAM accepted the password. farglass-login waits for
//! the answer of farglassd before it starts the session.
//!
//! A passkey belongs to the host name of the page where the user made it
//! (the relying party id of WebAuthn). An IP address cannot have a passkey.
//! A recovery code works one time, on each host name. farglassd keeps only
//! the SHA-256 hash of each code. To remove all passkeys of a user, an
//! administrator removes the file of the user.

use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use farglass_proto::message::{PasskeyEvent, PasskeyInfo, PasskeyRequest};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use webauthn_rs::prelude::{
    Passkey, PasskeyAuthentication, PasskeyRegistration, PublicKeyCredential,
    RegisterPublicKeyCredential, Url, Uuid, Webauthn, WebauthnBuilder,
};

/// The count of recovery codes that a user gets.
pub const CODES: usize = 8;

/// The name of the relying party that the authenticator shows.
const RP_NAME: &str = "farglass";

/// A passkey with its facts for the list.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    name: String,
    host: String,
    created: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    used: Option<u64>,
    passkey: Passkey,
}

impl Entry {
    fn id(&self) -> String {
        let bytes: &[u8] = self.passkey.cred_id().as_ref();
        URL_SAFE_NO_PAD.encode(bytes)
    }
}

/// The file of one user.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct UserFile {
    /// The user handle of WebAuthn: random, thus it does not tell the uid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    handle: Option<Uuid>,
    #[serde(default)]
    passkeys: Vec<Entry>,
    /// The SHA-256 hashes of the recovery codes that the user did not use, in hexadecimal.
    #[serde(default)]
    codes: Vec<String>,
}

/// A failure of a passkey action, with the text for the user.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct PasskeyError(pub String);

impl PasskeyError {
    fn new(text: &str) -> Self {
        Self(text.to_owned())
    }
}

/// The second factor that a login of a user needs.
#[derive(Debug)]
pub struct Challenge {
    /// The options for `navigator.credentials.get`, or `None` if no passkey belongs to this host name.
    pub options: Option<serde_json::Value>,
    /// The host names of the other passkeys of the user.
    pub elsewhere: Vec<String>,
    state: Option<PasskeyAuthentication>,
    host: Option<String>,
}

/// The passkeys of all users.
#[derive(Debug)]
pub struct PasskeyStore {
    dir: PathBuf,
    /// One change at a time: a read, a change, and a write of a file.
    lock: Mutex<()>,
}

impl PasskeyStore {
    /// Returns the store in `dir`. The directory is made at the first write.
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            lock: Mutex::new(()),
        }
    }

    fn path(&self, uid: u32) -> PathBuf {
        self.dir.join(format!("{uid}.json"))
    }

    /// Reads the file of the user. Only a missing file means no passkeys.
    ///
    /// Each other failure, for example a damaged file, no free file descriptor, or a wrong owner,
    /// is an error: the second factor stays required, and no change overwrites the file.
    fn load(&self, uid: u32) -> Result<UserFile, PasskeyError> {
        let unreadable =
            || PasskeyError::new("The server cannot read your passkeys. Tell the administrator.");
        match std::fs::read(self.path(uid)) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
                passkey_fault(uid, "damaged", &error.to_string());
                unreadable()
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(UserFile::default()),
            Err(error) => {
                passkey_fault(uid, "read-error", &error.to_string());
                Err(unreadable())
            }
        }
    }

    fn save(&self, uid: u32, file: &UserFile) -> Result<(), PasskeyError> {
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(&self.dir)?;
            let path = self.path(uid);
            let temporary = path.with_extension("json.new");
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&temporary)?;
            out.write_all(&serde_json::to_vec_pretty(file).map_err(std::io::Error::other)?)?;
            out.sync_all()?;
            std::fs::rename(&temporary, &path)
        };
        write().map_err(|error| {
            passkey_fault(
                uid,
                "write-error",
                &format!("{}: {error}", self.dir.display()),
            );
            PasskeyError::new("The server could not save the passkeys. Tell the administrator.")
        })
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Reports whether the user must use a second factor: the user has a passkey, or a file that cannot be read.
    #[must_use]
    pub fn required(&self, uid: u32) -> bool {
        self.load(uid)
            .map_or(true, |file| !file.passkeys.is_empty())
    }

    /// Returns the list of the passkeys of the user and the count of recovery codes.
    #[must_use]
    pub fn list(&self, uid: u32) -> (Vec<PasskeyInfo>, u32) {
        let file = self.load(uid).unwrap_or_default();
        let list = file
            .passkeys
            .iter()
            .map(|entry| PasskeyInfo {
                id: entry.id(),
                name: entry.name.clone(),
                host: entry.host.clone(),
                created: entry.created,
                used: entry.used,
            })
            .collect();
        (list, u32::try_from(file.codes.len()).unwrap_or(u32::MAX))
    }

    /// Starts the registration of a passkey for the user `name` on the page `origin`.
    ///
    /// # Errors
    ///
    /// [`PasskeyError`] if the origin has no host name.
    pub fn start_registration(
        &self,
        uid: u32,
        name: &str,
        origin: &str,
    ) -> Result<(serde_json::Value, PasskeyRegistration, String), PasskeyError> {
        let (webauthn, host) = webauthn(origin)?;
        let _guard = self.guard();
        let mut file = self.load(uid)?;
        let handle = match file.handle {
            Some(handle) => handle,
            None => {
                let handle = Uuid::new_v4();
                file.handle = Some(handle);
                self.save(uid, &file)?;
                handle
            }
        };
        let exclude = file
            .passkeys
            .iter()
            .map(|entry| entry.passkey.cred_id().clone())
            .collect();
        let (options, state) = webauthn
            .start_passkey_registration(handle, name, name, Some(exclude))
            .map_err(|error| {
                tracing::warn!(uid, %error, "a passkey registration did not start");
                PasskeyError::new("The registration did not start.")
            })?;
        let options = serde_json::to_value(&options)
            .map_err(|_| PasskeyError::new("The registration did not start."))?;
        Ok((options["publicKey"].clone(), state, host))
    }

    /// Ends a registration and saves the passkey with the name `label`.
    /// Returns new recovery codes if it is the first passkey of the user.
    ///
    /// # Errors
    ///
    /// [`PasskeyError`] if the credential is not valid for the registration.
    pub fn finish_registration(
        &self,
        uid: u32,
        label: &str,
        origin: &str,
        state: &PasskeyRegistration,
        credential: serde_json::Value,
    ) -> Result<Option<Vec<String>>, PasskeyError> {
        let (webauthn, host) = webauthn(origin)?;
        let credential: RegisterPublicKeyCredential = serde_json::from_value(credential)
            .map_err(|_| PasskeyError::new("The browser gave a passkey that is not valid."))?;
        let passkey = webauthn
            .finish_passkey_registration(&credential, state)
            .map_err(|error| {
                tracing::warn!(uid, %error, "a passkey registration failed");
                PasskeyError::new("The passkey was not accepted.")
            })?;
        let _guard = self.guard();
        let mut file = self.load(uid)?;
        file.passkeys.push(Entry {
            name: label.to_owned(),
            host,
            created: now(),
            used: None,
            passkey,
        });
        let codes = file.codes.is_empty().then(|| replace_codes(&mut file));
        self.save(uid, &file)?;
        tracing::info!(
            uid,
            passkeys = file.passkeys.len(),
            "the user added a passkey"
        );
        Ok(codes)
    }

    /// Removes the passkey `id` of the user. The recovery codes go with the last passkey.
    ///
    /// # Errors
    ///
    /// [`PasskeyError`] if the file was not written.
    pub fn remove(&self, uid: u32, id: &str) -> Result<(), PasskeyError> {
        let _guard = self.guard();
        let mut file = self.load(uid)?;
        file.passkeys.retain(|entry| entry.id() != id);
        if file.passkeys.is_empty() {
            file.codes.clear();
        }
        self.save(uid, &file)?;
        tracing::info!(
            uid,
            passkeys = file.passkeys.len(),
            "the user removed a passkey"
        );
        Ok(())
    }

    /// Makes new recovery codes for the user. The old codes stop working.
    ///
    /// # Errors
    ///
    /// [`PasskeyError`] if the user has no passkey or the file was not written.
    pub fn new_codes(&self, uid: u32) -> Result<Vec<String>, PasskeyError> {
        let _guard = self.guard();
        let mut file = self.load(uid)?;
        if file.passkeys.is_empty() {
            return Err(PasskeyError::new(
                "Add a passkey first. Recovery codes replace a lost passkey.",
            ));
        }
        let codes = replace_codes(&mut file);
        self.save(uid, &file)?;
        tracing::info!(uid, "the user made new recovery codes");
        Ok(codes)
    }

    /// Starts the second factor of a login of the user on the page `origin`.
    #[must_use]
    pub fn challenge(&self, uid: u32, origin: Option<&str>) -> Challenge {
        // An unreadable file gives no passkey and no code: the login fails until the administrator repairs it.
        let file = self.load(uid).unwrap_or_default();
        let current = origin.and_then(|origin| webauthn(origin).ok());
        let here: Vec<Passkey> = file
            .passkeys
            .iter()
            .filter(|entry| {
                current
                    .as_ref()
                    .is_some_and(|(_, host)| *host == entry.host)
            })
            .map(|entry| entry.passkey.clone())
            .collect();
        let mut elsewhere: Vec<String> = file
            .passkeys
            .iter()
            .filter(|entry| current.as_ref().is_none_or(|(_, host)| *host != entry.host))
            .map(|entry| entry.host.clone())
            .collect();
        elsewhere.sort();
        elsewhere.dedup();
        let started = current
            .as_ref()
            .filter(|_| !here.is_empty())
            .and_then(|(webauthn, _)| {
                webauthn
                    .start_passkey_authentication(&here)
                    .map_err(|error| tracing::warn!(uid, %error, "a passkey login did not start"))
                    .ok()
            });
        match started {
            Some((options, state)) => Challenge {
                options: serde_json::to_value(&options)
                    .ok()
                    .map(|value| value["publicKey"].clone()),
                elsewhere,
                state: Some(state),
                host: current.map(|(_, host)| host),
            },
            None => Challenge {
                options: None,
                elsewhere,
                state: None,
                host: None,
            },
        }
    }

    /// Checks the passkey answer of a login. Reports whether it is valid.
    #[must_use]
    pub fn verify(
        &self,
        uid: u32,
        origin: Option<&str>,
        challenge: &Challenge,
        credential: serde_json::Value,
    ) -> bool {
        let (Some(state), Some(origin)) = (&challenge.state, origin) else {
            return false;
        };
        let Ok((webauthn, host)) = webauthn(origin) else {
            return false;
        };
        if challenge.host.as_deref() != Some(host.as_str()) {
            return false;
        }
        let Ok(credential) = serde_json::from_value::<PublicKeyCredential>(credential) else {
            return false;
        };
        let result = match webauthn.finish_passkey_authentication(&credential, state) {
            Ok(result) => result,
            Err(error) => {
                tracing::info!(uid, %error, "a passkey login failed");
                return false;
            }
        };
        let _guard = self.guard();
        let Ok(mut file) = self.load(uid) else {
            return false;
        };
        for entry in &mut file.passkeys {
            if entry.passkey.update_credential(&result).is_some() {
                entry.used = Some(now());
            }
        }
        // The login works also if the counter was not written: the next login checks it again.
        let _ = self.save(uid, &file);
        true
    }

    /// Uses a recovery code of the user. Reports whether it was valid. A code works one time.
    #[must_use]
    pub fn use_code(&self, uid: u32, code: &str) -> bool {
        let hash = code_hash(code);
        let _guard = self.guard();
        let Ok(mut file) = self.load(uid) else {
            return false;
        };
        let Some(index) = file.codes.iter().position(|known| *known == hash) else {
            return false;
        };
        file.codes.remove(index);
        if self.save(uid, &file).is_err() {
            return false;
        }
        tracing::info!(
            uid,
            left = file.codes.len(),
            "the user logged in with a recovery code"
        );
        true
    }
}

/// The task of one passkeys channel: the list, the registration, the removal, and the recovery codes.
#[derive(Debug)]
pub struct PasskeysTask {
    /// The store.
    pub store: std::sync::Arc<PasskeyStore>,
    /// The user of the connection.
    pub user: crate::login::Identity,
    /// The origin of the page.
    pub origin: Option<String>,
    /// The channel of the shell.
    pub channel: farglass_proto::ChannelId,
    /// The requests of the shell. The task ends when the shell closes the channel.
    pub requests: tokio::sync::mpsc::Receiver<PasskeyRequest>,
    /// The queue to the shell.
    pub to_shell: tokio::sync::mpsc::Sender<farglass_proto::Frame>,
}

/// The time for which a proof lets the channel change the passkeys (F053).
pub const PROOF_LIFETIME: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// The wrong proofs on one channel before farglassd closes it.
pub const MAX_WRONG_PROOFS: u32 = 3;

impl PasskeysTask {
    /// Answers the requests until the shell closes the channel. Ends with a close frame.
    ///
    /// A user with a passkey, or with a file that cannot be read, must give a fresh
    /// passkey or a recovery code before a registration, a removal, or new codes
    /// (F053). The proof is valid on this channel for [`PROOF_LIFETIME`].
    pub async fn run(mut self) {
        use farglass_proto::message::{Close, CloseReason, Ready};
        use farglass_proto::{Frame, FrameType};
        let ready = Frame::ready(self.channel, &Ready {});
        self.send_frame(ready).await;
        let uid = self.user.uid;
        // The registration that waits for the credential of the browser, with the name of the passkey.
        let mut pending: Option<(PasskeyRegistration, String)> = None;
        // The change that waits for a proof, and its challenge.
        let mut waiting: Option<(PasskeyRequest, Challenge)> = None;
        let mut fresh_until: Option<std::time::Instant> = None;
        let mut wrong = 0_u32;
        while let Some(request) = self.requests.recv().await {
            let events = match request {
                PasskeyRequest::List => vec![self.list()],
                PasskeyRequest::Finish { credential } => match pending.take() {
                    None => vec![PasskeyEvent::Error {
                        message: "No registration is running.".to_owned(),
                    }],
                    Some((state, name)) => self.finish(&state, &name, credential),
                },
                PasskeyRequest::Proof { .. } | PasskeyRequest::ProofCode { .. } => {
                    let Some((change, challenge)) = waiting.take() else {
                        continue;
                    };
                    let valid = match request {
                        PasskeyRequest::Proof { credential } => {
                            self.store
                                .verify(uid, self.origin.as_deref(), &challenge, credential)
                        }
                        PasskeyRequest::ProofCode { code } => self.store.use_code(uid, &code),
                        _ => false,
                    };
                    let event = farglass_login::audit::Event {
                        event: "passkey-proof",
                        user: Some(&self.user.name),
                        uid: Some(uid),
                        ..Default::default()
                    };
                    if valid {
                        event.accepted();
                        fresh_until = Some(std::time::Instant::now() + PROOF_LIFETIME);
                        self.change(change, &mut pending)
                    } else {
                        wrong += 1;
                        farglass_login::audit::Event {
                            reason: "wrong-proof",
                            ..event
                        }
                        .refused();
                        if wrong >= MAX_WRONG_PROOFS {
                            self.send_frame(Frame::close(
                                self.channel,
                                &Close {
                                    reason: CloseReason::AccessDenied,
                                    message: Some(
                                        "Too many wrong answers. Open Settings again.".to_owned(),
                                    ),
                                },
                            ))
                            .await;
                            return;
                        }
                        vec![PasskeyEvent::Error {
                            message: "The passkey or the recovery code was not accepted. Nothing changed."
                                .to_owned(),
                        }]
                    }
                }
                change @ (PasskeyRequest::Register { .. }
                | PasskeyRequest::Remove { .. }
                | PasskeyRequest::NewCodes) => {
                    let fresh = fresh_until.is_some_and(|until| std::time::Instant::now() < until);
                    // `required` is also true for a file that cannot be read (ADR-042).
                    if fresh || !self.store.required(uid) {
                        self.change(change, &mut pending)
                    } else {
                        let challenge = self.store.challenge(uid, self.origin.as_deref());
                        let event = PasskeyEvent::Confirm {
                            options: challenge.options.clone(),
                            elsewhere: challenge.elsewhere.clone(),
                        };
                        waiting = Some((change, challenge));
                        vec![event]
                    }
                }
            };
            for event in events {
                self.send_frame(Frame::json(self.channel, FrameType::Control, &event))
                    .await;
            }
        }
        self.send_frame(Frame::close(self.channel, &Close::new(CloseReason::Done)))
            .await;
    }

    /// Does a change that passed the check: a registration start, a removal, or new codes.
    fn change(
        &self,
        request: PasskeyRequest,
        pending: &mut Option<(PasskeyRegistration, String)>,
    ) -> Vec<PasskeyEvent> {
        let uid = self.user.uid;
        let origin = self.origin.clone().unwrap_or_default();
        let result = match request {
            PasskeyRequest::Register { name } => {
                let name = name.trim().chars().take(64).collect::<String>();
                let name = if name.is_empty() {
                    "Passkey".to_owned()
                } else {
                    name
                };
                return match self.store.start_registration(uid, &self.user.name, &origin) {
                    Ok((options, state, _)) => {
                        *pending = Some((state, name));
                        vec![PasskeyEvent::Create { options }]
                    }
                    Err(error) => vec![PasskeyEvent::Error { message: error.0 }],
                };
            }
            PasskeyRequest::Remove { id } => self
                .store
                .remove(uid, &id)
                .map(|()| ("remove", vec![self.list()])),
            PasskeyRequest::NewCodes => self.store.new_codes(uid).map(|codes| {
                (
                    "new-codes",
                    vec![PasskeyEvent::Codes { codes }, self.list()],
                )
            }),
            _ => return Vec::new(),
        };
        self.logged(result)
    }

    /// Ends a registration that passed the check.
    fn finish(
        &self,
        state: &PasskeyRegistration,
        name: &str,
        credential: serde_json::Value,
    ) -> Vec<PasskeyEvent> {
        let origin = self.origin.clone().unwrap_or_default();
        let result = self
            .store
            .finish_registration(self.user.uid, name, &origin, state, credential)
            .map(|codes| {
                let mut events: Vec<PasskeyEvent> = codes
                    .map(|codes| PasskeyEvent::Codes { codes })
                    .into_iter()
                    .collect();
                events.push(self.list());
                ("add", events)
            });
        self.logged(result)
    }

    /// Logs a change of the passkeys and returns its events.
    fn logged(&self, result: Result<(&str, Vec<PasskeyEvent>), PasskeyError>) -> Vec<PasskeyEvent> {
        let event = farglass_login::audit::Event {
            event: "passkey-change",
            user: Some(&self.user.name),
            uid: Some(self.user.uid),
            ..Default::default()
        };
        match result {
            Ok((reason, events)) => {
                farglass_login::audit::Event { reason, ..event }.accepted();
                events
            }
            Err(error) => {
                // A read or write fault of the file logs its own fault event in `load` or `save`.
                farglass_login::audit::Event {
                    reason: "failed",
                    detail: Some(&error.0),
                    ..event
                }
                .refused();
                vec![PasskeyEvent::Error { message: error.0 }]
            }
        }
    }

    fn list(&self) -> PasskeyEvent {
        let (passkeys, codes) = self.store.list(self.user.uid);
        let host = self
            .origin
            .as_deref()
            .and_then(|origin| Url::parse(origin).ok())
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_default();
        PasskeyEvent::List {
            passkeys,
            codes,
            host,
        }
    }

    async fn send_frame(&self, frame: Result<farglass_proto::Frame, farglass_proto::FrameError>) {
        if let Ok(frame) = frame {
            let _ = self.to_shell.send(frame).await;
        }
    }
}

/// Logs a fault of the passkey file of `uid` (ADR-042, ADR-043).
fn passkey_fault(uid: u32, reason: &str, detail: &str) {
    farglass_login::audit::Event {
        event: "passkey-file",
        reason,
        uid: Some(uid),
        detail: Some(detail),
        ..Default::default()
    }
    .fault();
}

/// Gives the file new recovery codes and returns them.
fn replace_codes(file: &mut UserFile) -> Vec<String> {
    let codes: Vec<String> = (0..CODES).map(|_| new_code()).collect();
    file.codes = codes.iter().map(|code| code_hash(code)).collect();
    codes
}

/// Returns a recovery code of 16 hexadecimal characters in four groups, for example `3f9a-0c41-77be-12d0`.
fn new_code() -> String {
    let secret = crate::cookie::secret();
    let digits = &secret[..16];
    format!(
        "{}-{}-{}-{}",
        &digits[..4],
        &digits[4..8],
        &digits[8..12],
        &digits[12..]
    )
}

/// Returns the hash of a code. Spaces, dashes, and the case do not count.
fn code_hash(code: &str) -> String {
    let clean: String = code
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    Sha256::digest(clean.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Returns the WebAuthn verifier for the page `origin`, and the host name of the page.
fn webauthn(origin: &str) -> Result<(Webauthn, String), PasskeyError> {
    let url =
        Url::parse(origin).map_err(|_| PasskeyError::new("The page has no valid address."))?;
    let Some(host) = url.domain().map(str::to_owned) else {
        return Err(PasskeyError::new(
            "A passkey needs a host name. Open farglass with its host name, not with an IP address.",
        ));
    };
    let webauthn = WebauthnBuilder::new(&host, &url)
        .and_then(|builder| builder.rp_name(RP_NAME).build())
        .map_err(|_| PasskeyError::new("The page has no valid address."))?;
    Ok((webauthn, host))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recovery_code_works_one_time_and_ignores_case_and_dashes() {
        let dir = tempfile::tempdir().unwrap();
        let store = PasskeyStore::new(dir.path().to_owned());
        let mut file = UserFile::default();
        let codes = replace_codes(&mut file);
        store.save(1000, &file).unwrap();
        assert_eq!(codes.len(), CODES);
        assert!(!store.use_code(1000, "0000-0000-0000-0000"));
        let typed = codes[2].to_uppercase().replace('-', " ");
        assert!(store.use_code(1000, &typed));
        assert!(!store.use_code(1000, &codes[2]), "a code works one time");
        assert_eq!(store.list(1000).1, 7);
    }

    #[test]
    fn a_passkey_needs_a_host_name() {
        assert!(webauthn("https://192.0.2.10:8443").is_err());
        let (_, host) = webauthn("https://farglass.cyberdyne.example:8443").unwrap();
        assert_eq!(host, "farglass.cyberdyne.example");
        assert!(webauthn("https://localhost:8443").is_ok());
    }

    #[test]
    fn a_user_with_no_passkey_needs_no_second_factor_and_a_damaged_file_does() {
        let dir = tempfile::tempdir().unwrap();
        let store = PasskeyStore::new(dir.path().to_owned());
        assert!(!store.required(1000));
        let challenge = store.challenge(1000, Some("https://localhost:8443"));
        assert!(challenge.options.is_none() && challenge.elsewhere.is_empty());
        std::fs::write(dir.path().join("1000.json"), b"{ not json").unwrap();
        assert!(store.required(1000));
        assert!(
            store
                .start_registration(1000, "alice", "https://localhost")
                .is_err(),
            "a change must not overwrite a file that cannot be read"
        );
        assert_eq!(
            std::fs::read(dir.path().join("1000.json")).unwrap(),
            b"{ not json"
        );
    }

    #[test]
    fn a_file_that_cannot_be_read_keeps_the_second_factor() {
        let dir = tempfile::tempdir().unwrap();
        let store = PasskeyStore::new(dir.path().to_owned());
        // A directory in place of the file: the read fails with an error that is not NotFound.
        std::fs::create_dir(dir.path().join("1000.json")).unwrap();
        assert!(store.required(1000));
        assert!(!store.use_code(1000, "0000-0000-0000-0000"));
    }

    #[test]
    fn registration_options_name_the_host_of_the_page() {
        let dir = tempfile::tempdir().unwrap();
        let store = PasskeyStore::new(dir.path().to_owned());
        let (options, _, host) = store
            .start_registration(1000, "alice", "https://farglass.cyberdyne.example")
            .unwrap();
        assert_eq!(host, "farglass.cyberdyne.example");
        assert_eq!(options["rp"]["id"], "farglass.cyberdyne.example");
        assert!(options["challenge"].is_string());
        // The file keeps the user handle, with mode 0600.
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(dir.path().join("1000.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    /// Starts a passkeys task for uid 1000 on `store`. Returns the request queue and the frames to the shell.
    fn task(
        store: std::sync::Arc<PasskeyStore>,
    ) -> (
        tokio::sync::mpsc::Sender<PasskeyRequest>,
        tokio::sync::mpsc::Receiver<farglass_proto::Frame>,
    ) {
        let (requests, receiver) = tokio::sync::mpsc::channel(4);
        let (to_shell, from_task) = tokio::sync::mpsc::channel(16);
        let task = PasskeysTask {
            store,
            user: crate::login::Identity {
                uid: 1000,
                name: "alice".to_owned(),
            },
            origin: Some("https://localhost:8443".to_owned()),
            channel: farglass_proto::ChannelId::new(1),
            requests: receiver,
            to_shell,
        };
        tokio::spawn(task.run());
        (requests, from_task)
    }

    async fn next(
        from: &mut tokio::sync::mpsc::Receiver<farglass_proto::Frame>,
    ) -> farglass_proto::Frame {
        tokio::time::timeout(std::time::Duration::from_secs(5), from.recv())
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn a_user_with_no_passkey_adds_one_with_no_proof() {
        let dir = tempfile::tempdir().unwrap();
        let store = std::sync::Arc::new(PasskeyStore::new(dir.path().to_owned()));
        let (requests, mut from) = task(store);
        assert_eq!(
            next(&mut from).await.frame_type(),
            farglass_proto::FrameType::Ready
        );
        requests
            .send(PasskeyRequest::Register {
                name: "Key".to_owned(),
            })
            .await
            .unwrap();
        let event: PasskeyEvent = next(&mut from).await.parse().unwrap();
        assert!(matches!(event, PasskeyEvent::Create { .. }), "{event:?}");
    }

    #[tokio::test]
    async fn a_change_with_a_file_that_cannot_be_read_needs_a_proof_that_fails() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("1000.json"), b"{ not json").unwrap();
        let store = std::sync::Arc::new(PasskeyStore::new(dir.path().to_owned()));
        let (requests, mut from) = task(store);
        next(&mut from).await;
        for change in [
            PasskeyRequest::Remove {
                id: "abc".to_owned(),
            },
            PasskeyRequest::NewCodes,
            PasskeyRequest::Register {
                name: "Mallory".to_owned(),
            },
        ] {
            requests.send(change).await.unwrap();
            let event: PasskeyEvent = next(&mut from).await.parse().unwrap();
            assert!(
                matches!(event, PasskeyEvent::Confirm { options: None, .. }),
                "{event:?}"
            );
            requests
                .send(PasskeyRequest::ProofCode {
                    code: "0000-0000-0000-0000".to_owned(),
                })
                .await
                .unwrap();
            let frame = next(&mut from).await;
            if frame.frame_type() == farglass_proto::FrameType::Close {
                let close: farglass_proto::message::Close = frame.parse().unwrap();
                assert_eq!(
                    close.reason,
                    farglass_proto::message::CloseReason::AccessDenied
                );
                break;
            }
            let event: PasskeyEvent = frame.parse().unwrap();
            assert!(matches!(event, PasskeyEvent::Error { .. }), "{event:?}");
        }
        assert_eq!(
            std::fs::read(dir.path().join("1000.json")).unwrap(),
            b"{ not json",
            "nothing changed the file"
        );
    }
}
