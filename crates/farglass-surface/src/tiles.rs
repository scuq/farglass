//! The tile codec of the surfaces (ADR-007, F025).
//!
//! The encoder keeps a copy of what the client shows. For each commit, it
//! looks only at the tiles in the damage of the Wayland client, and sends the
//! tiles that differ from the copy. A tile of one color goes as a fill. Each
//! other tile goes as LZ4 of its BGRA pixels. The output is a stream of
//! messages, little-endian:
//!
//! | Byte | Message | Fields |
//! |---|---|---|
//! | 0 | hello | version u8, tile size u8, cache slots u16 |
//! | 1 | frame | sequence u32, width u16, height u16 |
//! | 2 | tile | x u16, y u16, w u8, h u8, length u32, LZ4 block of `w * h * 4` BGRA bytes. It takes the next cache slot. |
//! | 3 | ref | x u16, y u16, slot u16: draws the tile of a cache slot |
//! | 4 | move | x u16, y u16, w u16, h u16, dy i16: copies a block of the canvas `dy` rows down (up if negative) |
//! | 5 | keep | slot u16: copies a cache slot to the next slot, with no drawing |
//! | 6 | fill | x u16, y u16, w u8, h u8, BGRA u32 |
//! | 7 | jpeg | x u16, y u16, w u8, h u8, length u32, JPEG of the tile. It takes no cache slot. |
//! | 8 | end | sequence u32 |
//!
//! A new size in a frame message clears the canvas of the client to
//! transparent black. The client shows the canvas at the end message, and
//! acknowledges the sequence.

use std::time::{Duration, Instant};

use crate::Rect;
use crate::cache::{Cache, Hit, SLOTS};
use crate::{classify, scroll};

/// The version of the message stream.
pub const VERSION: u8 = 1;

/// The width and the height of a tile in pixels.
pub const TILE: usize = 64;

/// The message codes.
pub mod code {
    /// The first message of a stream.
    pub const HELLO: u8 = 0;
    /// The start of a frame.
    pub const FRAME: u8 = 1;
    /// A tile as an LZ4 block.
    pub const TILE: u8 = 2;
    /// A tile from a cache slot.
    pub const REF: u8 = 3;
    /// A copy of a block of the canvas to a different height, for a scroll.
    pub const MOVE: u8 = 4;
    /// A copy of a cache slot to the next slot.
    pub const KEEP: u8 = 5;
    /// A tile of one color.
    pub const FILL: u8 = 6;
    /// A lossy tile as a JPEG.
    pub const JPEG: u8 = 7;
    /// The end of a frame.
    pub const END: u8 = 8;
}

/// The numbers of one encoded frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// The tiles in the damage.
    pub damaged: u32,
    /// The tiles that went as LZ4.
    pub tiles: u32,
    /// The tiles that went as a fill.
    pub fills: u32,
    /// The tiles that the client had in its cache.
    pub cached: u32,
    /// The rows that a move message copied on the client.
    pub moved_rows: u32,
    /// The tiles that went as a JPEG.
    pub lossy: u32,
    /// The lossy tiles that the lossless pass made exact.
    pub refined: u32,
    /// The tiles in the damage that did not change.
    pub unchanged: u32,
    /// The bytes of the frame.
    pub bytes: u32,
    /// The CPU time of the encoder in microseconds.
    pub micros: u32,
}

/// One encoded frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Encoded {
    /// The sequence number. The client acknowledges it.
    pub seq: u32,
    /// The messages, from the frame message to the end message.
    pub bytes: Vec<u8>,
    /// The numbers.
    pub stats: Stats,
}

/// The pixels of a whole buffer: `width * 4` bytes in each row.
#[derive(Clone, Copy, Debug)]
pub struct Image<'a> {
    /// The width in pixels.
    pub width: usize,
    /// The height in pixels.
    pub height: usize,
    /// The BGRA pixels.
    pub pixels: &'a [u8],
}

/// The encoder of one surface channel.
#[derive(Debug)]
pub struct Encoder {
    /// What the client shows: the size and the pixels.
    shown: Option<(usize, usize, Vec<u8>)>,
    seq: u32,
    cache: Cache,
    /// Reports whether the encoder looks for scrolls (F027).
    pub scroll: bool,
    /// Reports whether image tiles go as JPEG first (F028).
    pub lossy: bool,
    /// For each tile: the time of its last lossy send, until the lossless pass.
    lossy_since: Vec<Option<Instant>>,
}

/// The time with no change after which a lossy tile goes again, exact.
pub const REFINE_AFTER: Duration = Duration::from_millis(300);

/// Writes one tile that differs from the client: a fill, a cache reference, a JPEG, or LZ4.
/// Returns `true` if the tile went lossy.
#[allow(clippy::too_many_arguments)]
fn write_tile(
    out: &mut Out,
    stats: &mut Stats,
    cache: &mut Cache,
    (x, y, w, h): (usize, usize, usize, usize),
    current: &[u8],
    cleared: bool,
    lossy: bool,
) -> bool {
    let header = |out: &mut Out, code: u8| {
        out.u8(code);
        out.u16(x);
        out.u16(y);
        out.u8(u8::try_from(w).unwrap_or(64));
        out.u8(u8::try_from(h).unwrap_or(64));
    };
    if let Some(color) = single_color(current) {
        // A cleared canvas is transparent black already.
        if !(cleared && color == [0; 4]) {
            header(out, code::FILL);
            out.0.extend_from_slice(&color);
        }
        stats.fills += 1;
        return false;
    }
    let hash = Cache::hash(current, w, h);
    if let Some(hit) = cache.find(hash) {
        let slot = match hit {
            Hit::Slot(slot) => slot,
            Hit::Keep { from, to } => {
                out.u8(code::KEEP);
                out.u16(usize::from(from));
                to
            }
        };
        out.u8(code::REF);
        out.u16(x);
        out.u16(y);
        out.u16(usize::from(slot));
        stats.cached += 1;
        return false;
    }
    if lossy
        && classify::is_image(current)
        && let Some(jpeg) = classify::jpeg(current, w, h)
    {
        header(out, code::JPEG);
        out.u32(u32::try_from(jpeg.len()).unwrap_or(u32::MAX));
        out.0.extend_from_slice(&jpeg);
        stats.lossy += 1;
        return true;
    }
    cache.add(hash);
    let compressed = lz4_flex::block::compress(current);
    header(out, code::TILE);
    out.u32(u32::try_from(compressed.len()).unwrap_or(u32::MAX));
    out.0.extend_from_slice(&compressed);
    stats.tiles += 1;
    false
}

impl Default for Encoder {
    fn default() -> Self {
        Self {
            shown: None,
            seq: 0,
            cache: Cache::new(SLOTS),
            scroll: true,
            lossy: true,
            lossy_since: Vec::new(),
        }
    }
}

/// Writes the little-endian bytes of the fields.
struct Out(Vec<u8>);

impl Out {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    fn u16(&mut self, value: usize) {
        self.0
            .extend_from_slice(&u16::try_from(value).unwrap_or(u16::MAX).to_le_bytes());
    }
    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
}

/// Returns the hello message of a stream with `slots` cache slots. An encoder uses [`SLOTS`].
#[must_use]
pub fn hello(slots: u16) -> Vec<u8> {
    let mut out = vec![code::HELLO, VERSION, u8::try_from(TILE).unwrap_or(64)];
    out.extend_from_slice(&slots.to_le_bytes());
    out
}

/// Copies the tile at (`x`, `y`) with the size `w` x `h` out of `image`.
fn tile_bytes(image: &Image<'_>, x: usize, y: usize, w: usize, h: usize, into: &mut Vec<u8>) {
    into.clear();
    for row in y..y + h {
        let start = (row * image.width + x) * 4;
        into.extend_from_slice(&image.pixels[start..start + w * 4]);
    }
}

/// Returns the color of a tile if all its pixels have it.
fn single_color(bytes: &[u8]) -> Option<[u8; 4]> {
    let (first, rest) = bytes.split_first_chunk::<4>()?;
    rest.as_chunks::<4>()
        .0
        .iter()
        .all(|pixel| pixel == first)
        .then_some(*first)
}

/// Moves the lossy marks with the rows of a scroll: a tile that gets rows of a lossy tile is lossy too.
fn shift_lossy(marks: &mut [Option<Instant>], columns: usize, step: scroll::Move) {
    let before: Vec<(usize, Instant)> = marks
        .iter()
        .enumerate()
        .filter_map(|(i, m)| m.map(|m| (i, m)))
        .collect();
    for (index, since) in before {
        let (column, row) = (index % columns, index / columns);
        let (x0, x1) = (column * TILE, column * TILE + TILE);
        if x1 <= step.x || x0 >= step.x + step.width {
            continue;
        }
        let top = (row * TILE).max(step.y);
        let bottom = (row * TILE + TILE).min(step.y + step.height);
        if bottom <= top {
            continue;
        }
        let (Some(to_top), Some(to_bottom)) = (
            top.checked_add_signed(step.dy),
            bottom.checked_add_signed(step.dy),
        ) else {
            continue;
        };
        for to_row in to_top / TILE..to_bottom.div_ceil(TILE) {
            if let Some(mark) = marks.get_mut(to_row * columns + column) {
                *mark = Some(mark.map_or(since, |old| old.min(since)));
            }
        }
    }
}

/// Returns the bounding box of the damage in the buffer, as left, top, right, and bottom.
fn bounds(damage: &[Rect], width: usize, height: usize) -> Option<(usize, usize, usize, usize)> {
    let clamp = |value: i32, max: usize| usize::try_from(value.max(0)).unwrap_or(0).min(max);
    let mut result: Option<(usize, usize, usize, usize)> = None;
    for rect in damage {
        let (x0, y0) = (clamp(rect.x, width), clamp(rect.y, height));
        let (x1, y1) = (
            clamp(rect.x.saturating_add(rect.width), width),
            clamp(rect.y.saturating_add(rect.height), height),
        );
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        result = Some(match result {
            None => (x0, y0, x1, y1),
            Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
        });
    }
    result
}

/// Returns the tiles that a damage rectangle touches, as (column, row) pairs.
fn tiles_of(rect: Rect, width: usize, height: usize, into: &mut [bool], columns: usize) {
    let clamp = |value: i32, max: usize| usize::try_from(value.max(0)).unwrap_or(0).min(max);
    let x0 = clamp(rect.x, width);
    let y0 = clamp(rect.y, height);
    let x1 = clamp(rect.x.saturating_add(rect.width), width);
    let y1 = clamp(rect.y.saturating_add(rect.height), height);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    for row in y0 / TILE..y1.div_ceil(TILE) {
        for column in x0 / TILE..x1.div_ceil(TILE) {
            into[row * columns + column] = true;
        }
    }
}

impl Encoder {
    /// Returns an encoder for a client that shows nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Encodes the changes of `image` in the `damage` since the last frame.
    ///
    /// A new size sends each tile. Returns `None` if nothing changed.
    pub fn encode(&mut self, image: &Image<'_>, damage: &[Rect]) -> Option<Encoded> {
        let started = Instant::now();
        let (width, height) = (image.width, image.height);
        let resized = !matches!(&self.shown, Some((w, h, _)) if *w == width && *h == height);
        let columns = width.div_ceil(TILE);
        let rows = height.div_ceil(TILE);
        if resized {
            self.shown = Some((width, height, vec![0; width * height * 4]));
            self.lossy_since = vec![None; columns * rows];
        }
        let mut dirty = vec![resized; columns * rows];
        if !resized {
            for rect in damage {
                tiles_of(*rect, width, height, &mut dirty, columns);
            }
        }
        let Some((_, _, shown)) = &mut self.shown else {
            return None;
        };
        let mut out = Out(Vec::new());
        let mut stats = Stats::default();
        let next = self.seq.wrapping_add(1);
        out.u8(code::FRAME);
        out.u32(next);
        out.u16(width);
        out.u16(height);

        // A scroll: the client copies the rows that moved, and the tile diff then sends the new strip only.
        if self.scroll
            && !resized
            && let Some((x0, y0, x1, y1)) = bounds(damage, width, height)
            && let Some(step) = scroll::detect(shown, image.pixels, width, x0, y0, x1, y1)
        {
            out.u8(code::MOVE);
            out.u16(step.x);
            out.u16(step.y);
            out.u16(step.width);
            out.u16(step.height);
            out.0
                .extend_from_slice(&i16::try_from(step.dy).unwrap_or(0).to_le_bytes());
            scroll::apply(shown, width, step);
            shift_lossy(&mut self.lossy_since, columns, step);
            stats.moved_rows = u32::try_from(step.height).unwrap_or(u32::MAX);
            // The move changed the copy of the client in the whole region, also between the damage rectangles.
            let region = Rect {
                x: i32::try_from(x0).unwrap_or(0),
                y: i32::try_from(y0).unwrap_or(0),
                width: i32::try_from(x1 - x0).unwrap_or(0),
                height: i32::try_from(y1 - y0).unwrap_or(0),
            };
            tiles_of(region, width, height, &mut dirty, columns);
        }

        let mut current = Vec::with_capacity(TILE * TILE * 4);
        let mut before = Vec::with_capacity(TILE * TILE * 4);
        for row in 0..rows {
            for column in 0..columns {
                if !dirty[row * columns + column] {
                    continue;
                }
                stats.damaged += 1;
                let (x, y) = (column * TILE, row * TILE);
                let (w, h) = (TILE.min(width - x), TILE.min(height - y));
                tile_bytes(image, x, y, w, h, &mut current);
                if !resized {
                    tile_bytes(
                        &Image {
                            width,
                            height,
                            pixels: shown,
                        },
                        x,
                        y,
                        w,
                        h,
                        &mut before,
                    );
                    if before == current {
                        stats.unchanged += 1;
                        continue;
                    }
                }
                let went_lossy = write_tile(
                    &mut out,
                    &mut stats,
                    &mut self.cache,
                    (x, y, w, h),
                    &current,
                    resized,
                    self.lossy,
                );
                self.lossy_since[row * columns + column] = went_lossy.then_some(started);
                for (line, chunk) in current.chunks_exact(w * 4).enumerate() {
                    let start = ((y + line) * width + x) * 4;
                    shown[start..start + w * 4].copy_from_slice(chunk);
                }
            }
        }
        if stats.tiles + stats.fills + stats.cached + stats.lossy + stats.moved_rows == 0
            && !resized
        {
            return None;
        }
        self.seq = next;
        out.u8(code::END);
        out.u32(next);
        stats.bytes = u32::try_from(out.0.len()).unwrap_or(u32::MAX);
        stats.micros = u32::try_from(started.elapsed().as_micros()).unwrap_or(u32::MAX);
        Some(Encoded {
            seq: next,
            bytes: out.0,
            stats,
        })
    }

    /// Returns the time of the next lossless pass, if a lossy tile waits for it.
    #[must_use]
    pub fn next_refine(&self) -> Option<Instant> {
        self.lossy_since
            .iter()
            .flatten()
            .min()
            .map(|since| *since + REFINE_AFTER)
    }

    /// The lossless pass: sends each lossy tile that did not change for [`REFINE_AFTER`] again, exact.
    pub fn refine(&mut self, now: Instant) -> Option<Encoded> {
        let started = Instant::now();
        let (width, height, shown) = self.shown.as_ref()?;
        let (width, height) = (*width, *height);
        let columns = width.div_ceil(TILE);
        let mut out = Out(Vec::new());
        let mut stats = Stats::default();
        let next = self.seq.wrapping_add(1);
        out.u8(code::FRAME);
        out.u32(next);
        out.u16(width);
        out.u16(height);
        let mut current = Vec::with_capacity(TILE * TILE * 4);
        for (index, since) in self.lossy_since.iter_mut().enumerate() {
            if !since.is_some_and(|since| now.duration_since(since) >= REFINE_AFTER) {
                continue;
            }
            *since = None;
            let (x, y) = ((index % columns) * TILE, (index / columns) * TILE);
            let (w, h) = (TILE.min(width - x), TILE.min(height - y));
            tile_bytes(
                &Image {
                    width,
                    height,
                    pixels: shown,
                },
                x,
                y,
                w,
                h,
                &mut current,
            );
            write_tile(
                &mut out,
                &mut stats,
                &mut self.cache,
                (x, y, w, h),
                &current,
                false,
                false,
            );
            stats.refined += 1;
        }
        if stats.refined == 0 {
            return None;
        }
        self.seq = next;
        out.u8(code::END);
        out.u32(next);
        stats.bytes = u32::try_from(out.0.len()).unwrap_or(u32::MAX);
        stats.micros = u32::try_from(started.elapsed().as_micros()).unwrap_or(u32::MAX);
        Some(Encoded {
            seq: next,
            bytes: out.0,
            stats,
        })
    }

    /// Forgets what the client shows. The next frame sends each tile.
    pub fn reset(&mut self) {
        self.shown = None;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A reference decoder: applies a stream to a canvas, as the client does.
    #[derive(Debug, Default)]
    pub(crate) struct Canvas {
        pub width: usize,
        pub height: usize,
        pub pixels: Vec<u8>,
        pub acked: Vec<u32>,
        pub slots: Vec<Option<(usize, usize, Vec<u8>)>>,
        pub next: usize,
        pub lossy_tiles: usize,
    }

    impl Canvas {
        fn draw(&mut self, x: usize, y: usize, w: usize, h: usize, pixels: &[u8]) {
            for line in 0..h {
                let start = ((y + line) * self.width + x) * 4;
                self.pixels[start..start + w * 4]
                    .copy_from_slice(&pixels[line * w * 4..(line + 1) * w * 4]);
            }
        }

        fn take_slot(&mut self) -> usize {
            let next = self.next;
            self.next = (next + 1) % self.slots.len();
            next
        }

        pub(crate) fn apply(&mut self, mut bytes: &[u8]) {
            let take = |bytes: &mut &[u8], n: usize| {
                let (head, tail) = bytes.split_at(n);
                *bytes = tail;
                head.to_vec()
            };
            let u16_ = |b: &[u8]| usize::from(u16::from_le_bytes([b[0], b[1]]));
            while let Some((&code, rest)) = bytes.split_first() {
                bytes = rest;
                match code {
                    code::HELLO => {
                        let f = take(&mut bytes, 4);
                        self.slots = vec![None; u16_(&f[2..4]).max(1)];
                        self.next = 0;
                    }
                    code::FRAME => {
                        let f = take(&mut bytes, 8);
                        let (w, h) = (u16_(&f[4..6]), u16_(&f[6..8]));
                        if (w, h) != (self.width, self.height) {
                            self.width = w;
                            self.height = h;
                            self.pixels = vec![0; w * h * 4];
                        }
                    }
                    code::MOVE => {
                        let f = take(&mut bytes, 10);
                        let step = scroll::Move {
                            x: u16_(&f[0..2]),
                            y: u16_(&f[2..4]),
                            width: u16_(&f[4..6]),
                            height: u16_(&f[6..8]),
                            dy: isize::from(i16::from_le_bytes([f[8], f[9]])),
                        };
                        scroll::apply(&mut self.pixels, self.width, step);
                    }
                    code::KEEP => {
                        let slot = u16_(&take(&mut bytes, 2));
                        let copy = self.slots[slot].clone();
                        let next = self.take_slot();
                        self.slots[next] = copy;
                    }
                    code::REF => {
                        let f = take(&mut bytes, 6);
                        let (x, y, slot) = (u16_(&f[0..2]), u16_(&f[2..4]), u16_(&f[4..6]));
                        let (w, h, pixels) = self.slots[slot].clone().expect("a slot with a tile");
                        self.draw(x, y, w, h, &pixels);
                    }
                    code::TILE | code::FILL => {
                        let f = take(&mut bytes, 6);
                        let (x, y, w, h) = (
                            u16_(&f[0..2]),
                            u16_(&f[2..4]),
                            usize::from(f[4]),
                            usize::from(f[5]),
                        );
                        let pixels = if code == code::TILE {
                            let len = u32::from_le_bytes(take(&mut bytes, 4).try_into().unwrap())
                                as usize;
                            let pixels =
                                lz4_flex::block::decompress(&take(&mut bytes, len), w * h * 4)
                                    .unwrap();
                            let next = self.take_slot();
                            self.slots[next] = Some((w, h, pixels.clone()));
                            pixels
                        } else {
                            take(&mut bytes, 4).repeat(w * h)
                        };
                        self.draw(x, y, w, h, &pixels);
                    }
                    code::JPEG => {
                        let f = take(&mut bytes, 10);
                        let len = u32::from_le_bytes([f[6], f[7], f[8], f[9]]) as usize;
                        let jpeg = take(&mut bytes, len);
                        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
                        self.lossy_tiles += 1;
                        // The reference decoder does not decode JPEG: it marks the tile with a color that no test image has.
                        let (x, y, w, h) = (
                            u16_(&f[0..2]),
                            u16_(&f[2..4]),
                            usize::from(f[4]),
                            usize::from(f[5]),
                        );
                        self.draw(x, y, w, h, &[1, 2, 3, 4].repeat(w * h));
                    }
                    code::END => {
                        let seq = u32::from_le_bytes(take(&mut bytes, 4).try_into().unwrap());
                        self.acked.push(seq);
                    }
                    other => panic!("unknown message {other}"),
                }
            }
        }
    }

    /// A test image: a background, a box, and a gradient.
    pub(crate) fn image(width: usize, height: usize, shift: u8) -> Vec<u8> {
        let mut pixels = vec![0; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let pixel = if (20..60).contains(&x) && (10..30).contains(&y) {
                    [shift, 0, 200, 255]
                } else if x > width / 2 {
                    [
                        u8::try_from(x % 256).unwrap(),
                        u8::try_from(y % 256).unwrap(),
                        shift,
                        255,
                    ]
                } else {
                    [240, 240, 240, 255]
                };
                pixels[(y * width + x) * 4..][..4].copy_from_slice(&pixel);
            }
        }
        pixels
    }

    #[test]
    fn each_frame_gives_the_exact_pixels_and_only_changed_tiles_go() {
        let (w, h) = (300, 200);
        // Exact pixels: no lossy tiles in these tests.
        let mut encoder = Encoder {
            lossy: false,
            ..Encoder::new()
        };
        let mut canvas = Canvas::default();
        canvas.apply(&hello(SLOTS));
        let first = image(w, h, 0);
        let full = Rect {
            x: 0,
            y: 0,
            width: 300,
            height: 200,
        };
        let encoded = encoder
            .encode(
                &Image {
                    width: w,
                    height: h,
                    pixels: &first,
                },
                &[full],
            )
            .unwrap();
        canvas.apply(&encoded.bytes);
        assert_eq!(canvas.pixels, first);
        assert_eq!(encoded.stats.damaged, 20);
        assert!(encoded.stats.fills > 0, "the background tiles are fills");

        // The box changes its color: only its tiles go, although the damage is the whole buffer.
        let second = image(w, h, 9);
        let encoded = encoder
            .encode(
                &Image {
                    width: w,
                    height: h,
                    pixels: &second,
                },
                &[full],
            )
            .unwrap();
        canvas.apply(&encoded.bytes);
        assert_eq!(canvas.pixels, second);
        assert!(encoded.stats.unchanged > 0);
        assert!(encoded.stats.tiles + encoded.stats.fills < 20);

        // No change: no frame.
        assert!(
            encoder
                .encode(
                    &Image {
                        width: w,
                        height: h,
                        pixels: &second
                    },
                    &[full]
                )
                .is_none()
        );
        // A damage that misses a change keeps the old pixels: the encoder trusts the damage.
        let third = image(w, h, 20);
        let encoded = encoder
            .encode(
                &Image {
                    width: w,
                    height: h,
                    pixels: &third,
                },
                &[Rect {
                    x: 0,
                    y: 0,
                    width: 64,
                    height: 64,
                }],
            )
            .unwrap();
        canvas.apply(&encoded.bytes);
        assert_eq!(canvas.pixels[..4], third[..4]);
        assert_eq!(canvas.acked, [1, 2, 3]);
    }

    #[test]
    fn a_new_size_clears_the_canvas_and_sends_each_tile() {
        // Exact pixels: no lossy tiles in these tests.
        let mut encoder = Encoder {
            lossy: false,
            ..Encoder::new()
        };
        let mut canvas = Canvas::default();
        canvas.apply(&hello(SLOTS));
        let small = image(100, 70, 1);
        canvas.apply(
            &encoder
                .encode(
                    &Image {
                        width: 100,
                        height: 70,
                        pixels: &small,
                    },
                    &[],
                )
                .unwrap()
                .bytes,
        );
        assert_eq!(canvas.pixels, small);
        let large = image(130, 90, 1);
        let encoded = encoder
            .encode(
                &Image {
                    width: 130,
                    height: 90,
                    pixels: &large,
                },
                &[],
            )
            .unwrap();
        canvas.apply(&encoded.bytes);
        assert_eq!((canvas.width, canvas.height), (130, 90));
        assert_eq!(canvas.pixels, large);
    }
}

#[cfg(test)]
mod cache_tests {
    use super::tests::Canvas;
    use super::*;

    /// A page of text: a few glyph tiles that repeat, as a text page and a toolbar have.
    fn text_page(width: usize, height: usize, offset: usize) -> Vec<u8> {
        let mut pixels = vec![255; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let glyph = ((x / TILE) * 7 + (y / TILE + offset) * 3) % 5;
                if (x % TILE + glyph * 5) % 11 < 3 && (y % TILE) % 9 < 6 {
                    pixels[(y * width + x) * 4..][..3].copy_from_slice(&[20, 20, 20]);
                }
            }
        }
        pixels
    }

    #[test]
    fn repeated_tiles_come_from_the_cache_and_the_pixels_stay_exact() {
        let (w, h) = (640, 640);
        let mut encoder = Encoder::new();
        let mut canvas = Canvas::default();
        canvas.apply(&hello(SLOTS));
        let full = [Rect {
            x: 0,
            y: 0,
            width: 640,
            height: 640,
        }];
        let mut cached = 0;
        let mut sent = 0;
        for page in 0..6 {
            let pixels = text_page(w, h, page);
            let encoded = encoder
                .encode(
                    &Image {
                        width: w,
                        height: h,
                        pixels: &pixels,
                    },
                    &full,
                )
                .unwrap();
            canvas.apply(&encoded.bytes);
            assert_eq!(canvas.pixels, pixels, "page {page}");
            cached += encoded.stats.cached;
            sent += encoded.stats.tiles + encoded.stats.cached;
        }
        let rate = f64::from(cached) / f64::from(sent);
        eprintln!("hit rate on a text page: {:.0} %", rate * 100.0);
        assert!(rate > 0.8, "hit rate {rate}");
    }

    #[test]
    fn a_full_ring_keeps_the_tiles_in_use() {
        let (w, h) = (64, 64);
        let mut encoder = Encoder {
            cache: Cache::new(8),
            ..Encoder::new()
        };
        let mut canvas = Canvas::default();
        canvas.apply(&hello(8));
        let full = [Rect {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        }];
        // Tile A, then many new tiles, with tile A again and again: it must stay in the ring.
        let tile = |seed: u8| -> Vec<u8> {
            (0..64 * 64 * 4)
                .map(|i| u8::try_from(i % 251).unwrap() ^ seed)
                .collect()
        };
        let mut hits = 0;
        for round in 0..40_u8 {
            let pixels = if round % 3 == 0 { tile(0) } else { tile(round) };
            if let Some(encoded) = encoder.encode(
                &Image {
                    width: w,
                    height: h,
                    pixels: &pixels,
                },
                &full,
            ) {
                canvas.apply(&encoded.bytes);
                hits += encoded.stats.cached;
            }
            assert_eq!(canvas.pixels, pixels, "round {round}");
        }
        assert!(hits >= 12, "tile A came from the cache {hits} times");
    }
}

#[cfg(test)]
mod scroll_tests {
    use super::tests::Canvas;
    use super::*;

    /// A text page of 800 x 600, scrolled by `scroll` rows.
    fn page(scroll: usize) -> Vec<u8> {
        let (w, h) = (800, 600);
        let mut pixels = vec![255; w * h * 4];
        for y in 0..h {
            let line = y + scroll;
            for x in 0..w {
                // Each row of a glyph is different, as in real text: a hash of the column block and the row.
                let cell = u64::try_from((x / 3) * 4096 + line)
                    .unwrap()
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    >> 59;
                if (line % 20) < 13 && cell < 9 {
                    pixels[(y * w + x) * 4..][..3].copy_from_slice(&[25, 25, 25]);
                }
            }
        }
        pixels
    }

    fn bytes_of_scroll(scroll: bool) -> (usize, Vec<u8>, Vec<u8>) {
        let mut encoder = Encoder {
            scroll,
            ..Encoder::new()
        };
        let mut canvas = Canvas::default();
        canvas.apply(&hello(SLOTS));
        let full = [Rect {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        }];
        canvas.apply(
            &encoder
                .encode(
                    &Image {
                        width: 800,
                        height: 600,
                        pixels: &page(0),
                    },
                    &full,
                )
                .unwrap()
                .bytes,
        );
        let after = page(53);
        let encoded = encoder
            .encode(
                &Image {
                    width: 800,
                    height: 600,
                    pixels: &after,
                },
                &full,
            )
            .unwrap();
        canvas.apply(&encoded.bytes);
        (encoded.bytes.len(), canvas.pixels, after)
    }

    #[test]
    fn a_scroll_sends_a_move_and_the_new_strip_only() {
        let (with, pixels, after) = bytes_of_scroll(true);
        assert_eq!(pixels, after, "the canvas has the exact new frame");
        let (without, pixels, after) = bytes_of_scroll(false);
        assert_eq!(pixels, after);
        eprintln!(
            "a scroll of 53 rows of an 800 x 600 text page: {with} bytes with the move, {without} bytes without"
        );
        assert!(
            with * 3 < without,
            "{with} bytes with the move, {without} without"
        );
    }
}

#[cfg(test)]
mod lossy_tests {
    use super::tests::Canvas;
    use super::*;

    /// A photo: noise in each pixel, on a background with text.
    fn frame(seed: u32) -> Vec<u8> {
        let (w, h) = (256, 128);
        let mut pixels = vec![240; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let p = &mut pixels[(y * w + x) * 4..][..4];
                if x < 128 {
                    let v = u32::try_from(y * w + x)
                        .unwrap()
                        .wrapping_mul(2_654_435_761)
                        .wrapping_add(seed)
                        >> 8;
                    p.copy_from_slice(&[
                        (v & 255) as u8,
                        ((v >> 8) & 255) as u8,
                        ((v >> 16) & 255) as u8,
                        255,
                    ]);
                } else if (x + y) % 9 < 2 {
                    p.copy_from_slice(&[10, 10, 10, 255]);
                }
            }
        }
        pixels
    }

    #[test]
    fn image_tiles_go_lossy_and_become_exact_when_they_stop_changing() {
        let mut encoder = Encoder::new();
        let mut canvas = Canvas::default();
        canvas.apply(&hello(SLOTS));
        let image = frame(0);
        let full = [Rect {
            x: 0,
            y: 0,
            width: 256,
            height: 128,
        }];
        let first = encoder
            .encode(
                &Image {
                    width: 256,
                    height: 128,
                    pixels: &image,
                },
                &full,
            )
            .unwrap();
        canvas.apply(&first.bytes);
        assert_eq!(first.stats.lossy, 4, "the 4 photo tiles go as JPEG");
        assert_eq!(canvas.lossy_tiles, 4);
        // The text tiles are exact at once.
        assert_eq!(
            canvas.pixels[(10 * 256 + 200) * 4..][..4],
            image[(10 * 256 + 200) * 4..][..4]
        );
        let due = encoder.next_refine().unwrap();
        assert!(
            encoder.refine(due - Duration::from_millis(1)).is_none(),
            "not before 300 ms"
        );
        let refined = encoder.refine(due).unwrap();
        canvas.apply(&refined.bytes);
        assert_eq!(refined.stats.refined, 4);
        assert_eq!(
            canvas.pixels, image,
            "after the lossless pass, the canvas is exact"
        );
        assert!(encoder.next_refine().is_none());

        let mut lossless = Encoder {
            lossy: false,
            ..Encoder::new()
        };
        let exact = lossless
            .encode(
                &Image {
                    width: 256,
                    height: 128,
                    pixels: &image,
                },
                &full,
            )
            .unwrap();
        eprintln!(
            "4 photo tiles and 4 text tiles: {} bytes with JPEG, {} bytes lossless",
            first.bytes.len(),
            exact.bytes.len()
        );
        assert!(first.bytes.len() * 2 < exact.bytes.len());
    }

    #[test]
    fn a_tile_that_changes_again_waits_for_its_lossless_pass() {
        let mut encoder = Encoder::new();
        let full = [Rect {
            x: 0,
            y: 0,
            width: 256,
            height: 128,
        }];
        let first = encoder
            .encode(
                &Image {
                    width: 256,
                    height: 128,
                    pixels: &frame(0),
                },
                &full,
            )
            .unwrap();
        let start = encoder.next_refine().unwrap();
        std::thread::sleep(Duration::from_millis(5));
        encoder
            .encode(
                &Image {
                    width: 256,
                    height: 128,
                    pixels: &frame(7),
                },
                &full,
            )
            .unwrap();
        assert!(
            encoder.next_refine().unwrap() > start,
            "the new lossy send starts the time again"
        );
        assert!(first.stats.lossy > 0);
    }
}
