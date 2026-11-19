//! Channel protocol between the shell and the host programs.

/// The version of the channel protocol.
pub const PROTOCOL_VERSION: u16 = 0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_is_zero() {
        assert_eq!(PROTOCOL_VERSION, 0);
    }
}
