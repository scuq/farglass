//! The link from the session worker (F057).
//!
//! In a web session, the worker of farglass-login runs as root and gives the
//! agent each link to farglassd through a private socket pair: the agent sends
//! one byte, and the worker answers with one byte and the open link as
//! SCM_RIGHTS. farglassd accepts an agent link only from the worker, thus a
//! program of the user cannot replace the agent.
//!
//! The only unsafe code of the crate is here: the fd numbers from the
//! environment and from SCM_RIGHTS become owned file descriptors.

#![allow(unsafe_code)]

use std::io::{self, IoSliceMut, Write as _};
use std::os::fd::{AsRawFd as _, BorrowedFd, FromRawFd as _, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;

use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::sys::socket::{ControlMessageOwned, MsgFlags, recvmsg};

/// The environment variable with the number of the end of the socket pair of the agent.
pub const BROKER_ENV: &str = "FARGLASS_BROKER_FD";

/// The end of the socket pair of the agent.
#[derive(Debug)]
pub struct Broker {
    stream: UnixStream,
}

impl Broker {
    /// Takes the socket pair from the environment and removes the variable, or returns `None`.
    ///
    /// Call it at the start of `main`, before other threads: it changes the environment.
    /// The file descriptor gets close-on-exec: the apps of the agent do not get it.
    #[must_use]
    pub fn take_from_env() -> Option<Self> {
        let value = std::env::var(BROKER_ENV).ok()?;
        // SAFETY: the caller calls this function before it starts other threads.
        unsafe { std::env::remove_var(BROKER_ENV) };
        let number: RawFd = value.parse().ok()?;
        if number < 3 {
            return None;
        }
        // SAFETY: the fd is open for the time of this call: fcntl only checks it.
        let borrowed = unsafe { BorrowedFd::borrow_raw(number) };
        fcntl(borrowed, FcntlArg::F_GETFD).ok()?;
        fcntl(borrowed, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)).ok()?;
        // SAFETY: the worker gave this fd to the agent, and nothing else in the agent owns it.
        let owned = unsafe { OwnedFd::from_raw_fd(number) };
        Some(Self::from_stream(UnixStream::from(owned)))
    }

    /// Returns a broker on `stream`, for tests.
    #[must_use]
    pub fn from_stream(stream: UnixStream) -> Self {
        Self { stream }
    }

    /// Asks the worker for a new link to farglassd. Blocks until the answer.
    ///
    /// # Errors
    ///
    /// An error if the worker gives no link, or the socket pair failed.
    pub fn link(&self) -> io::Result<UnixStream> {
        (&self.stream).write_all(&[1])?;
        let mut answer = [0_u8; 1];
        let mut iov = [IoSliceMut::new(&mut answer)];
        let mut space = nix::cmsg_space!([RawFd; 1]);
        let message = recvmsg::<()>(
            self.stream.as_raw_fd(),
            &mut iov,
            Some(&mut space),
            MsgFlags::MSG_CMSG_CLOEXEC,
        )
        .map_err(io::Error::from)?;
        let mut link = None;
        for control in message.cmsgs().map_err(io::Error::from)? {
            if let ControlMessageOwned::ScmRights(fds) = control {
                for fd in fds {
                    // SAFETY: the kernel gave this process a new fd with SCM_RIGHTS: nothing else owns it.
                    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
                    if link.is_none() {
                        link = Some(UnixStream::from(owned));
                    }
                }
            }
        }
        if message.bytes == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the session worker stopped",
            ));
        }
        link.ok_or_else(|| io::Error::other("the session worker has no link to farglassd"))
    }
}

/// Stops other processes of the user from reading the memory of the agent or attaching a debugger (F057).
///
/// The agent then has no core dump, and its files in /proc belong to root.
///
/// # Errors
///
/// An error if the kernel refuses the change.
pub fn protect() -> io::Result<()> {
    nix::sys::prctl::set_dumpable(false).map_err(io::Error::from)
}
