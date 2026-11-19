//! Channel identifiers.

use std::fmt;

/// The side of a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// The shell in the client browser.
    Shell,
    /// A program on the host.
    Host,
}

/// The number of a channel on one connection.
///
/// Channel 0 is the connection channel.
/// The shell opens channels with odd numbers.
/// The host opens channels with even numbers.
/// A connection uses each number one time only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChannelId(u32);

impl ChannelId {
    /// The connection channel. It carries [`crate::message::ConnectionMessage`].
    pub const CONNECTION: Self = Self(0);

    /// Returns the channel with the number `raw`.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the number of the channel.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Reports whether this is the connection channel.
    #[must_use]
    pub const fn is_connection(self) -> bool {
        self.0 == 0
    }

    /// Returns the side that can open this channel.
    ///
    /// Returns `None` for the connection channel, which is always open.
    #[must_use]
    pub const fn opener(self) -> Option<Side> {
        match self.0 {
            0 => None,
            n if n % 2 == 1 => Some(Side::Shell),
            _ => Some(Side::Host),
        }
    }
}

impl fmt::Display for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Gives the channel numbers that one side can open, in sequence.
#[derive(Clone, Debug)]
pub struct ChannelIdAllocator {
    next: Option<u32>,
}

impl ChannelIdAllocator {
    /// Returns an allocator for the channels that `side` opens.
    #[must_use]
    pub const fn new(side: Side) -> Self {
        let first = match side {
            Side::Shell => 1,
            Side::Host => 2,
        };
        Self { next: Some(first) }
    }

    /// Returns the next channel number.
    ///
    /// Returns `None` when the side used all its numbers.
    /// Then the side must start a new connection.
    pub fn allocate(&mut self) -> Option<ChannelId> {
        let id = self.next?;
        self.next = id.checked_add(2);
        Some(ChannelId(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opener_follows_the_parity_of_the_number() {
        assert_eq!(ChannelId::CONNECTION.opener(), None);
        assert_eq!(ChannelId::new(1).opener(), Some(Side::Shell));
        assert_eq!(ChannelId::new(2).opener(), Some(Side::Host));
        assert_eq!(ChannelId::new(u32::MAX).opener(), Some(Side::Shell));
    }

    #[test]
    fn allocator_gives_only_the_numbers_of_its_side() {
        let mut shell = ChannelIdAllocator::new(Side::Shell);
        let mut host = ChannelIdAllocator::new(Side::Host);
        for _ in 0..100 {
            assert_eq!(shell.allocate().unwrap().opener(), Some(Side::Shell));
            assert_eq!(host.allocate().unwrap().opener(), Some(Side::Host));
        }
    }

    #[test]
    fn allocator_stops_at_the_last_number() {
        let mut shell = ChannelIdAllocator {
            next: Some(u32::MAX),
        };
        assert_eq!(shell.allocate(), Some(ChannelId::new(u32::MAX)));
        assert_eq!(shell.allocate(), None);

        let mut host = ChannelIdAllocator {
            next: Some(u32::MAX - 1),
        };
        assert_eq!(host.allocate(), Some(ChannelId::new(u32::MAX - 1)));
        assert_eq!(host.allocate(), None);
    }
}
