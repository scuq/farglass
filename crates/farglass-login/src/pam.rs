//! A small binding of Linux-PAM: authenticate a user and check the account,
//! with a conversation that the caller answers.
//!
//! The code comes from `rcd/crates/rcdd/src/pam.rs`.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::Path;
use std::ptr;

use zeroize::Zeroizing;

#[allow(non_camel_case_types)]
type pam_handle_t = c_void;

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

#[repr(C)]
struct PamConv {
    conv: extern "C" fn(c_int, *mut *const PamMessage, *mut *mut PamResponse, *mut c_void) -> c_int,
    appdata_ptr: *mut c_void,
}

const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_NEW_AUTHTOK_REQD: c_int = 12;
const PAM_CONV_ERR: c_int = 19;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;
const PAM_USER: c_int = 2;
const PAM_TTY: c_int = 3;
const PAM_RHOST: c_int = 4;
const PAM_DISALLOW_NULL_AUTHTOK: c_int = 0x0001;

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start_confdir(
        service: *const c_char,
        user: *const c_char,
        conv: *const PamConv,
        confdir: *const c_char,
        pamh: *mut *mut pam_handle_t,
    ) -> c_int;
    fn pam_authenticate(pamh: *mut pam_handle_t, flags: c_int) -> c_int;
    fn pam_acct_mgmt(pamh: *mut pam_handle_t, flags: c_int) -> c_int;
    fn pam_end(pamh: *mut pam_handle_t, status: c_int) -> c_int;
    fn pam_get_item(pamh: *const pam_handle_t, item: c_int, value: *mut *const c_void) -> c_int;
    fn pam_set_item(pamh: *mut pam_handle_t, item: c_int, value: *const c_void) -> c_int;
    fn pam_strerror(pamh: *mut pam_handle_t, errnum: c_int) -> *const c_char;
}

/// The side that answers the questions of the PAM modules.
pub trait Conversation {
    /// Returns the answer to `text`, or `None` to stop the login.
    ///
    /// If `echo` is `false`, the answer is secret, for example a password.
    fn prompt(&mut self, text: &str, echo: bool) -> Option<Zeroizing<String>>;
    /// Gives information to the user.
    fn info(&mut self, text: &str);
    /// Gives an error message to the user.
    fn error(&mut self, text: &str);
}

/// A failed login.
#[derive(Debug)]
pub struct Failure {
    /// The username that PAM had, if any.
    pub user: Option<String>,
    /// The cause, from the PAM modules.
    pub reason: String,
    /// Reports whether the authentication passed and the account check failed,
    /// for example for an expired account. Only then can the user see the cause:
    /// other causes can tell which usernames exist.
    pub account: bool,
}

/// The parameters of one login.
#[derive(Clone, Copy, Debug)]
pub struct Login<'a> {
    /// The PAM service, for example `farglass`.
    pub service: &'a str,
    /// A directory that replaces `/etc/pam.d`, for tests.
    pub confdir: Option<&'a Path>,
    /// The username, or `None` to let the modules ask for it.
    pub user: Option<&'a str>,
    /// The address of the client.
    pub rhost: &'a str,
}

extern "C" fn converse(
    num_msg: c_int,
    msg: *mut *const PamMessage,
    resp: *mut *mut PamResponse,
    appdata: *mut c_void,
) -> c_int {
    let Ok(n) = usize::try_from(num_msg) else {
        return PAM_CONV_ERR;
    };
    if n == 0 || msg.is_null() || resp.is_null() || appdata.is_null() {
        return PAM_CONV_ERR;
    }
    // SAFETY: PAM frees the responses with free(), thus they must come from calloc.
    let out = unsafe { libc::calloc(n, size_of::<PamResponse>()) }.cast::<PamResponse>();
    if out.is_null() {
        return PAM_BUF_ERR;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: `appdata` is the pointer that `login` gave to pam_start_confdir.
        // It points to a `&mut dyn Conversation` that lives during the PAM calls.
        let conv = unsafe { &mut *appdata.cast::<&mut dyn Conversation>() };
        for i in 0..n {
            // SAFETY: Linux-PAM gives an array of `n` pointers to messages.
            let message = unsafe { &**msg.add(i) };
            let text = if message.msg.is_null() {
                String::new()
            } else {
                // SAFETY: PAM gives a string that ends with a NUL byte.
                unsafe { CStr::from_ptr(message.msg) }
                    .to_string_lossy()
                    .into_owned()
            };
            match message.msg_style {
                PAM_PROMPT_ECHO_OFF | PAM_PROMPT_ECHO_ON => {
                    let answer = conv
                        .prompt(&text, message.msg_style == PAM_PROMPT_ECHO_ON)
                        .ok_or(PAM_CONV_ERR)?;
                    // A NUL byte in the answer would cut it. Refuse it.
                    let mut bytes = Zeroizing::new(answer.as_bytes().to_vec());
                    if bytes.contains(&0) {
                        return Err(PAM_CONV_ERR);
                    }
                    bytes.push(0);
                    // SAFETY: `bytes` ends with a NUL byte and has no other NUL byte.
                    let dup = unsafe { libc::strdup(bytes.as_ptr().cast::<c_char>()) };
                    if dup.is_null() {
                        return Err(PAM_BUF_ERR);
                    }
                    // SAFETY: `out` has `n` elements.
                    unsafe { (*out.add(i)).resp = dup };
                }
                PAM_ERROR_MSG => conv.error(&text),
                PAM_TEXT_INFO => conv.info(&text),
                _ => return Err(PAM_CONV_ERR),
            }
        }
        Ok(())
    }));
    match result {
        Ok(Ok(())) => {
            // SAFETY: `resp` is not null. PAM takes ownership of `out`.
            unsafe { *resp = out };
            PAM_SUCCESS
        }
        Ok(Err(code)) => {
            free_responses(out, n);
            code
        }
        Err(_) => {
            free_responses(out, n);
            PAM_CONV_ERR
        }
    }
}

/// Clears and frees the responses after an error.
fn free_responses(out: *mut PamResponse, n: usize) {
    for i in 0..n {
        // SAFETY: `out` has `n` elements from calloc. Each `resp` is null or from strdup.
        unsafe {
            let response = (*out.add(i)).resp;
            if !response.is_null() {
                ptr::write_bytes(response, 0, libc::strlen(response));
                libc::free(response.cast());
            }
        }
    }
    // SAFETY: `out` comes from calloc.
    unsafe { libc::free(out.cast()) };
}

/// Authenticates a user and checks the account. Returns the username that PAM accepted.
///
/// The function blocks until the PAM modules complete. Call it in a separate thread.
///
/// # Errors
///
/// [`Failure`] if the authentication or the account check fails.
pub fn login(params: Login<'_>, mut conv: &mut dyn Conversation) -> Result<String, Failure> {
    let fail = |reason: &str| Failure {
        user: params.user.map(str::to_owned),
        reason: reason.to_owned(),
        account: false,
    };
    let service =
        CString::new(params.service).map_err(|_| fail("the PAM service name is not valid"))?;
    let user = params
        .user
        .map(CString::new)
        .transpose()
        .map_err(|_| fail("the username is not valid"))?;
    let confdir = params
        .confdir
        .map(|dir| CString::new(dir.as_os_str().as_encoded_bytes()))
        .transpose()
        .map_err(|_| fail("the PAM directory is not valid"))?;
    let rhost = CString::new(params.rhost).unwrap_or_default();
    let tty = c"farglass";

    let pam_conv = PamConv {
        conv: converse,
        appdata_ptr: ptr::from_mut(&mut conv).cast(),
    };
    let mut pamh: *mut pam_handle_t = ptr::null_mut();
    // SAFETY: All pointers are valid C strings or null, and `pam_conv` lives until pam_end.
    let rc = unsafe {
        pam_start_confdir(
            service.as_ptr(),
            user.as_ref().map_or(ptr::null(), |u| u.as_ptr()),
            &raw const pam_conv,
            confdir.as_ref().map_or(ptr::null(), |d| d.as_ptr()),
            &raw mut pamh,
        )
    };
    if rc != PAM_SUCCESS || pamh.is_null() {
        return Err(fail(&format!("pam_start failed with code {rc}")));
    }

    let strerror = |rc: c_int| {
        // SAFETY: `pamh` is valid until pam_end, and pam_strerror gives a static string.
        unsafe { CStr::from_ptr(pam_strerror(pamh, rc)) }
            .to_string_lossy()
            .into_owned()
    };
    // SAFETY: The PAM items are C strings that live until pam_end. PAM copies them.
    unsafe {
        pam_set_item(pamh, PAM_RHOST, rhost.as_ptr().cast());
        pam_set_item(pamh, PAM_TTY, tty.as_ptr().cast());
    }
    let result = (|| {
        // SAFETY: `pamh` is valid. The conversation runs during this call.
        let rc = unsafe { pam_authenticate(pamh, PAM_DISALLOW_NULL_AUTHTOK) };
        if rc != PAM_SUCCESS {
            return Err((rc, strerror(rc), false));
        }
        // SAFETY: `pamh` is valid.
        let rc = unsafe { pam_acct_mgmt(pamh, PAM_DISALLOW_NULL_AUTHTOK) };
        match rc {
            PAM_SUCCESS => Ok(()),
            // F006 has no password change through the browser.
            PAM_NEW_AUTHTOK_REQD => Err((
                rc,
                "The password expired. Change the password on the host, then log in again."
                    .to_owned(),
                true,
            )),
            _ => Err((rc, strerror(rc), true)),
        }
    })();

    let mut item: *const c_void = ptr::null();
    // SAFETY: `pamh` is valid. PAM_USER is a C string that lives until pam_end.
    let user = unsafe {
        (pam_get_item(pamh, PAM_USER, &raw mut item) == PAM_SUCCESS && !item.is_null())
            .then(|| CStr::from_ptr(item.cast()).to_string_lossy().into_owned())
    };
    let status = match &result {
        Ok(()) => PAM_SUCCESS,
        Err((rc, _, _)) => *rc,
    };
    // SAFETY: `pamh` is valid and is not used after this call.
    unsafe { pam_end(pamh, status) };

    match (result, user) {
        (Ok(()), Some(user)) => Ok(user),
        (Ok(()), None) => Err(fail("PAM gave no username")),
        (Err((_, reason, account)), user) => Err(Failure {
            user,
            reason,
            account,
        }),
    }
}

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_open_session(pamh: *mut pam_handle_t, flags: c_int) -> c_int;
    fn pam_close_session(pamh: *mut pam_handle_t, flags: c_int) -> c_int;
    fn pam_putenv(pamh: *mut pam_handle_t, name_value: *const c_char) -> c_int;
    fn pam_getenvlist(pamh: *mut pam_handle_t) -> *mut *mut c_char;
}

/// A conversation with no user: it logs the messages and refuses each prompt.
extern "C" fn silent_converse(
    num_msg: c_int,
    msg: *mut *const PamMessage,
    resp: *mut *mut PamResponse,
    _appdata: *mut c_void,
) -> c_int {
    let Ok(n) = usize::try_from(num_msg) else {
        return PAM_CONV_ERR;
    };
    if n == 0 || msg.is_null() || resp.is_null() {
        return PAM_CONV_ERR;
    }
    for i in 0..n {
        // SAFETY: Linux-PAM gives an array of `n` pointers to messages.
        let message = unsafe { &**msg.add(i) };
        if matches!(message.msg_style, PAM_PROMPT_ECHO_OFF | PAM_PROMPT_ECHO_ON) {
            return PAM_CONV_ERR;
        }
        if !message.msg.is_null() {
            // SAFETY: PAM gives a string that ends with a NUL byte.
            let text = unsafe { CStr::from_ptr(message.msg) }.to_string_lossy();
            tracing::info!(message = %text, "PAM session message");
        }
    }
    // SAFETY: PAM frees the responses with free(), thus they must come from calloc.
    let out = unsafe { libc::calloc(n, size_of::<PamResponse>()) }.cast::<PamResponse>();
    if out.is_null() {
        return PAM_BUF_ERR;
    }
    // SAFETY: `resp` is not null. PAM takes ownership of `out`.
    unsafe { *resp = out };
    PAM_SUCCESS
}

/// An open PAM session. With pam_systemd, it is a logind session, and the
/// calling process moves into it. Dropping the value closes the session.
#[derive(Debug)]
pub struct OpenSession {
    pamh: *mut pam_handle_t,
    /// PAM keeps a pointer to the conversation while the handle lives.
    _conv: Box<PamConv>,
}

impl std::fmt::Debug for PamConv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PamConv")
    }
}

/// Opens a PAM session for `user` with no authentication.
///
/// farglass-login authenticated the user before. `env` goes to the PAM
/// environment, for example `XDG_SESSION_TYPE` for pam_systemd.
///
/// # Errors
///
/// A text with the cause if a PAM call fails.
pub fn open_session(
    service: &str,
    confdir: Option<&Path>,
    user: &str,
    env: &[(&str, &str)],
) -> Result<OpenSession, String> {
    let service = CString::new(service).map_err(|_| "the PAM service name is not valid")?;
    let user = CString::new(user).map_err(|_| "the username is not valid")?;
    let confdir = confdir
        .map(|dir| CString::new(dir.as_os_str().as_encoded_bytes()))
        .transpose()
        .map_err(|_| "the PAM directory is not valid")?;
    let conv = Box::new(PamConv {
        conv: silent_converse,
        appdata_ptr: ptr::null_mut(),
    });
    let mut pamh: *mut pam_handle_t = ptr::null_mut();
    // SAFETY: All pointers are valid C strings or null. `conv` lives in the returned value.
    let rc = unsafe {
        pam_start_confdir(
            service.as_ptr(),
            user.as_ptr(),
            &raw const *conv,
            confdir.as_ref().map_or(ptr::null(), |d| d.as_ptr()),
            &raw mut pamh,
        )
    };
    if rc != PAM_SUCCESS || pamh.is_null() {
        return Err(format!("pam_start failed with code {rc}"));
    }
    let session = OpenSession { pamh, _conv: conv };
    let fail = |rc: c_int, what: &str| {
        // SAFETY: `pamh` is valid, and pam_strerror gives a static string.
        let text = unsafe { CStr::from_ptr(pam_strerror(pamh, rc)) }
            .to_string_lossy()
            .into_owned();
        format!("{what}: {text}")
    };
    for (key, value) in env {
        let pair = CString::new(format!("{key}={value}"))
            .map_err(|_| "an environment value is not valid")?;
        // SAFETY: `pamh` is valid. PAM copies the string.
        let rc = unsafe { pam_putenv(pamh, pair.as_ptr()) };
        if rc != PAM_SUCCESS {
            return Err(fail(rc, "pam_putenv"));
        }
    }
    // SAFETY: `pamh` is valid.
    let rc = unsafe { pam_acct_mgmt(pamh, 0) };
    if rc != PAM_SUCCESS {
        return Err(fail(rc, "the account check failed"));
    }
    // SAFETY: `pamh` is valid.
    let rc = unsafe { pam_open_session(pamh, 0) };
    if rc != PAM_SUCCESS {
        return Err(fail(rc, "pam_open_session failed"));
    }
    Ok(session)
}

impl OpenSession {
    /// Returns the PAM environment, for example `XDG_SESSION_ID` and `XDG_RUNTIME_DIR`.
    #[must_use]
    pub fn env(&self) -> Vec<(String, String)> {
        // SAFETY: `pamh` is valid. PAM gives a NULL-terminated array that the caller frees.
        let list = unsafe { pam_getenvlist(self.pamh) };
        let mut out = Vec::new();
        if list.is_null() {
            return out;
        }
        let mut i = 0;
        loop {
            // SAFETY: The array ends with a null pointer.
            let item = unsafe { *list.add(i) };
            if item.is_null() {
                break;
            }
            // SAFETY: Each item is a C string from malloc.
            let text = unsafe { CStr::from_ptr(item) }
                .to_string_lossy()
                .into_owned();
            if let Some((key, value)) = text.split_once('=') {
                out.push((key.to_owned(), value.to_owned()));
            }
            // SAFETY: The caller frees each item and the array.
            unsafe { libc::free(item.cast()) };
            i += 1;
        }
        // SAFETY: The array comes from malloc.
        unsafe { libc::free(list.cast()) };
        out
    }
}

impl Drop for OpenSession {
    fn drop(&mut self) {
        // SAFETY: `pamh` is valid, and the value is not used after this call.
        unsafe {
            pam_close_session(self.pamh, 0);
            pam_end(self.pamh, PAM_SUCCESS);
        }
    }
}
