//! The link broker of the session worker (F057).
//!
//! The worker runs as root. When the agent asks on its private socket pair,
//! the worker connects to the agent socket of farglassd, sends
//! [`LinkMessage::Claim`] with the uid of the session, and gives the open link
//! to the agent with SCM_RIGHTS. farglassd accepts an agent link only in this
//! way: a program of the user cannot connect a replacement agent.

use std::io::{self, Read as _, Write as _};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;

use farglass_proto::message::LinkMessage;
use farglass_proto::{ChannelId, Frame, FrameType};
use nix::sys::socket::{ControlMessage, MsgFlags, sendmsg};

/// The environment variable that gives the agent the number of its end of the socket pair.
pub const BROKER_ENV: &str = "FARGLASS_BROKER_FD";

/// Returns the bytes of the claim frame for the agent of `uid`.
///
/// # Errors
///
/// An error if the frame cannot be encoded.
pub fn claim_frame(uid: u32) -> io::Result<Vec<u8>> {
    let frame = Frame::json(
        ChannelId::CONNECTION,
        FrameType::Control,
        &LinkMessage::Claim { uid },
    )
    .map_err(io::Error::other)?;
    Ok(frame.to_bytes().to_vec())
}

/// Connects to the agent socket at `path` and sends the claim for `uid`.
fn connect_claim(path: &Path, uid: u32) -> io::Result<UnixStream> {
    let mut stream = UnixStream::connect(path)?;
    stream.write_all(&claim_frame(uid)?)?;
    Ok(stream)
}

/// Answers the link requests of the agent on `broker` until the agent closes it.
///
/// Each byte from the agent is one request. The answer is one byte: 1 with the
/// link as SCM_RIGHTS, or 0 with no link if farglassd does not answer.
pub fn serve(broker: &UnixStream, uid: u32, agent_socket: &Path) {
    let mut request = [0_u8; 1];
    loop {
        match (&*broker).read(&mut request) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let link = connect_claim(agent_socket, uid);
        if let Err(error) = &link {
            tracing::warn!(uid, %error, "no link to farglassd for the agent");
        }
        let fds: Vec<i32> = link.iter().map(AsRawFd::as_raw_fd).collect();
        let rights = [ControlMessage::ScmRights(&fds)];
        let control: &[ControlMessage] = if fds.is_empty() { &[] } else { &rights };
        let answer = [u8::from(!fds.is_empty())];
        let sent = sendmsg::<()>(
            broker.as_raw_fd(),
            &[io::IoSlice::new(&answer)],
            control,
            MsgFlags::empty(),
            None,
        );
        if sent.is_err() {
            return;
        }
        // The agent has its own copy of the link now: this copy closes.
        drop(link);
    }
}
