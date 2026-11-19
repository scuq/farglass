//! The mirrored tile cache (ADR-011, F026).
//!
//! The host and the client run the same allocator: a ring of slots. A tile
//! message takes the next slot on both sides, and a keep message copies a slot
//! to the next slot. Thus the host knows each slot of the client with no
//! acknowledgment. The host finds equal tiles with its own hash table. No hash
//! goes to the client.

use std::collections::HashMap;

/// The number of slots of a stream. 2048 tiles of 64 x 64 pixels are 32 MiB on the client.
pub const SLOTS: u16 = 2048;

/// The ring of slots on the host side.
#[derive(Debug)]
pub struct Cache {
    entries: Vec<Option<u128>>,
    by_hash: HashMap<u128, usize>,
    next: usize,
}

/// What the encoder sends for a tile that the cache has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// A reference to the slot.
    Slot(u16),
    /// The slot is old: a keep message copies it to a new slot, then a reference names the new slot.
    Keep {
        /// The old slot.
        from: u16,
        /// The new slot.
        to: u16,
    },
}

impl Cache {
    /// Returns an empty ring of `slots` slots.
    #[must_use]
    pub fn new(slots: u16) -> Self {
        Self {
            entries: vec![None; usize::from(slots.max(1))],
            by_hash: HashMap::new(),
            next: 0,
        }
    }

    /// Returns the hash of a tile: its pixels and its size.
    #[must_use]
    pub fn hash(pixels: &[u8], width: usize, height: usize) -> u128 {
        let mut hasher = xxhash_rust::xxh3::Xxh3::new();
        hasher.update(&u32::try_from(width).unwrap_or(0).to_le_bytes());
        hasher.update(&u32::try_from(height).unwrap_or(0).to_le_bytes());
        hasher.update(pixels);
        hasher.digest128()
    }

    fn slot(index: usize) -> u16 {
        u16::try_from(index).unwrap_or(u16::MAX)
    }

    /// The age of a slot: 0 for the newest.
    fn age(&self, slot: usize) -> usize {
        (self.next + self.entries.len() - slot - 1) % self.entries.len()
    }

    /// Finds a tile. A slot in the oldest quarter of the ring moves to a new slot, thus a tile in use stays.
    pub fn find(&mut self, hash: u128) -> Option<Hit> {
        let slot = *self.by_hash.get(&hash)?;
        if self.age(slot) * 4 < self.entries.len() * 3 {
            return Some(Hit::Slot(Self::slot(slot)));
        }
        let to = self.add(hash);
        Some(Hit::Keep {
            from: Self::slot(slot),
            to,
        })
    }

    /// Puts a tile in the next slot, as a tile message or a keep message does on the client. Returns the slot.
    pub fn add(&mut self, hash: u128) -> u16 {
        let slot = self.next;
        if let Some(old) = self.entries[slot].take()
            && self.by_hash.get(&old) == Some(&slot)
        {
            self.by_hash.remove(&old);
        }
        self.entries[slot] = Some(hash);
        self.by_hash.insert(hash, slot);
        self.next = (self.next + 1) % self.entries.len();
        Self::slot(slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tile_takes_the_next_slot_and_a_new_tile_overwrites_the_oldest() {
        let mut cache = Cache::new(4);
        assert_eq!(cache.add(10), 0);
        assert_eq!(cache.add(11), 1);
        assert_eq!(cache.find(11), Some(Hit::Slot(1)));
        assert_eq!(cache.find(99), None);
        cache.add(12);
        cache.add(13);
        // Slot 0 is the oldest: a new tile takes it, and tile 10 is gone.
        assert_eq!(cache.add(14), 0);
        assert_eq!(cache.find(10), None);
    }

    #[test]
    fn an_old_tile_in_use_moves_to_a_new_slot() {
        let mut cache = Cache::new(4);
        cache.add(10);
        cache.add(11);
        cache.add(12);
        cache.add(13);
        // Tile 10 is in slot 0, the oldest: a keep copies it to the next slot, which is slot 0 again here.
        assert_eq!(cache.find(10), Some(Hit::Keep { from: 0, to: 0 }));
        cache.add(14);
        assert_eq!(cache.find(10), Some(Hit::Slot(0)));
        assert_eq!(cache.find(11), None, "slot 1 went to tile 14");
    }
}
