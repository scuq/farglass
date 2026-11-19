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
//! | 2 | tile | x u16, y u16, w u8, h u8, length u32, LZ4 block of `w * h * 4` BGRA bytes |
//! | 6 | fill | x u16, y u16, w u8, h u8, BGRA u32 |
//! | 8 | end | sequence u32 |
//!
//! A new size in a frame message clears the canvas of the client to
//! transparent black. The client shows the canvas at the end message, and
//! acknowledges the sequence.

use std::time::Instant;

use crate::Rect;

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
    /// A tile of one color.
    pub const FILL: u8 = 6;
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
#[derive(Debug, Default)]
pub struct Encoder {
    /// What the client shows: the size and the pixels.
    shown: Option<(usize, usize, Vec<u8>)>,
    seq: u32,
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

/// Returns the hello message of a stream with `slots` cache slots.
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
        if resized {
            self.shown = Some((width, height, vec![0; width * height * 4]));
        }
        let columns = width.div_ceil(TILE);
        let rows = height.div_ceil(TILE);
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
                if let Some(color) = single_color(&current) {
                    // A cleared canvas is transparent black already.
                    if !(resized && color == [0; 4]) {
                        out.u8(code::FILL);
                        out.u16(x);
                        out.u16(y);
                        out.u8(u8::try_from(w).unwrap_or(64));
                        out.u8(u8::try_from(h).unwrap_or(64));
                        out.0.extend_from_slice(&color);
                    }
                    stats.fills += 1;
                } else {
                    let compressed = lz4_flex::block::compress(&current);
                    out.u8(code::TILE);
                    out.u16(x);
                    out.u16(y);
                    out.u8(u8::try_from(w).unwrap_or(64));
                    out.u8(u8::try_from(h).unwrap_or(64));
                    out.u32(u32::try_from(compressed.len()).unwrap_or(u32::MAX));
                    out.0.extend_from_slice(&compressed);
                    stats.tiles += 1;
                }
                for (line, chunk) in current.chunks_exact(w * 4).enumerate() {
                    let start = ((y + line) * width + x) * 4;
                    shown[start..start + w * 4].copy_from_slice(chunk);
                }
            }
        }
        if stats.tiles + stats.fills == 0 && !resized {
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
    }

    impl Canvas {
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
                    code::HELLO => drop(take(&mut bytes, 4)),
                    code::FRAME => {
                        let f = take(&mut bytes, 8);
                        let (w, h) = (u16_(&f[4..6]), u16_(&f[6..8]));
                        if (w, h) != (self.width, self.height) {
                            *self = Self {
                                width: w,
                                height: h,
                                pixels: vec![0; w * h * 4],
                                acked: std::mem::take(&mut self.acked),
                            };
                        }
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
                            lz4_flex::block::decompress(&take(&mut bytes, len), w * h * 4).unwrap()
                        } else {
                            take(&mut bytes, 4).repeat(w * h)
                        };
                        for line in 0..h {
                            let start = ((y + line) * self.width + x) * 4;
                            self.pixels[start..start + w * 4]
                                .copy_from_slice(&pixels[line * w * 4..(line + 1) * w * 4]);
                        }
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
        let mut encoder = Encoder::new();
        let mut canvas = Canvas::default();
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
        let mut encoder = Encoder::new();
        let mut canvas = Canvas::default();
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
