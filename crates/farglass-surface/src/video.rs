//! Video for a region that changes in each frame (F037, ADR-007).
//!
//! Tiles are exact and cheap for text and interface, but a video player or
//! an animation changes the same tiles in each frame. The detector counts,
//! for each tile, the commits that changed it. A block of tiles that changed
//! in most of the last commits, at a video frame rate, becomes one video
//! region. The encoder then sends that region as H.264, and tiles for the
//! rest of the window. When the region stops changing for [`VIDEO_IDLE`],
//! the video ends, and the region goes again as exact tiles.
//!
//! The encoder is OpenH264 in software. The host of the first test has no
//! VAAPI driver, thus a hardware encoder is a later step.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use openh264::OpenH264API;
use openh264::encoder::{
    BitRate, Complexity, Encoder, EncoderConfig, FrameRate, FrameType, RateControlMode, UsageType,
};
use openh264::formats::{BgraSliceU8, YUVBuffer};

use crate::tiles::TILE;

/// The commits that the detector looks at for each tile.
pub const WINDOW: usize = 16;

/// The commits of the last [`WINDOW`] that must change a tile for video.
pub const HOT_COMMITS: u32 = 12;

/// The longest time of the last [`WINDOW`] commits: a slower change, for example a blinking cursor, is no video.
pub const WINDOW_TIME: Duration = Duration::from_millis(1600);

/// The fewest tiles of a video region: a small animation, for example a spinner, stays as tiles.
pub const MIN_TILES: usize = 6;

/// The time with no change after which the video ends and the region goes as exact tiles.
pub const VIDEO_IDLE: Duration = Duration::from_millis(500);

/// A block of tiles: the first column, the first row, the end column, and the end row.
pub type TileBox = (usize, usize, usize, usize);

/// Finds the region of a surface that changes in each commit.
#[derive(Debug, Default)]
pub struct Detector {
    /// For each tile, one bit for each of the last commits: 1 if the commit changed the tile.
    history: Vec<u16>,
    columns: usize,
    times: VecDeque<Instant>,
}

impl Detector {
    /// Forgets the history, for a surface of `columns` x `rows` tiles.
    pub fn reset(&mut self, columns: usize, rows: usize) {
        self.history = vec![0; columns * rows];
        self.columns = columns;
        self.times.clear();
    }

    /// Forgets the changes of the tiles of `block`, for example after the end of its video.
    pub fn forget(&mut self, block: TileBox) {
        let (c0, r0, c1, r1) = block;
        for row in r0..r1 {
            for column in c0..c1 {
                if let Some(bits) = self.history.get_mut(row * self.columns + column) {
                    *bits = 0;
                }
            }
        }
    }

    /// Takes the tiles that the commit at `now` changed. Returns the block of the tiles that change in most commits, if it is a video.
    pub fn record(&mut self, changed: &[bool], now: Instant) -> Option<TileBox> {
        if changed.len() != self.history.len() || self.columns == 0 {
            return None;
        }
        for (bits, &changed) in self.history.iter_mut().zip(changed) {
            *bits = (*bits << 1) | u16::from(changed);
        }
        self.times.push_back(now);
        if self.times.len() > WINDOW {
            self.times.pop_front();
        }
        let fast = self.times.len() == WINDOW
            && self
                .times
                .front()
                .is_some_and(|first| now.duration_since(*first) <= WINDOW_TIME);
        if !fast {
            return None;
        }
        let mut count = 0;
        let mut found: Option<TileBox> = None;
        for (index, bits) in self.history.iter().enumerate() {
            if bits.count_ones() < HOT_COMMITS {
                continue;
            }
            count += 1;
            let (column, row) = (index % self.columns, index / self.columns);
            found = Some(match found {
                None => (column, row, column + 1, row + 1),
                Some((c0, r0, c1, r1)) => (
                    c0.min(column),
                    r0.min(row),
                    c1.max(column + 1),
                    r1.max(row + 1),
                ),
            });
        }
        found.filter(|_| count >= MIN_TILES)
    }
}

/// Returns the pixel rectangle of a block of tiles in a buffer of `width` x `height`: x, y, width, and height.
/// The width and the height are even, as H.264 with 4:2:0 color needs. `None` if the block is too small.
#[must_use]
pub fn pixel_rect(
    block: TileBox,
    width: usize,
    height: usize,
) -> Option<(usize, usize, usize, usize)> {
    let (c0, r0, c1, r1) = block;
    let (x, y) = (c0 * TILE, r0 * TILE);
    let w = (c1 * TILE).min(width).saturating_sub(x) & !1;
    let h = (r1 * TILE).min(height).saturating_sub(y) & !1;
    (w >= 16 && h >= 16).then_some((x, y, w, h))
}

/// The flags of a video message.
pub mod flag {
    /// The frame decodes with no frame before it.
    pub const KEY: u8 = 1;
    /// The first frame of a new stream: the client starts a new decoder.
    pub const NEW: u8 = 2;
}

/// The H.264 stream of one video region.
pub struct VideoStream {
    /// The region in buffer pixels: x, y, width, and height.
    pub rect: (usize, usize, usize, usize),
    /// The block of tiles of the region.
    pub block: TileBox,
    /// The time of the last change of the region.
    pub last_change: Instant,
    /// The BGRA pixels of the region at its last frame.
    pub last: Vec<u8>,
    encoder: Encoder,
    started: bool,
    /// A rectangle that never goes as video: x, y, width, and height (F040).
    pub exclude: Option<(usize, usize, usize, usize)>,
}

impl std::fmt::Debug for VideoStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoStream")
            .field("rect", &self.rect)
            .finish_non_exhaustive()
    }
}

/// One encoded frame of a video region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoFrame {
    /// [`flag::KEY`] and [`flag::NEW`].
    pub flags: u8,
    /// The H.264 access unit, Annex B.
    pub data: Vec<u8>,
}

impl VideoStream {
    /// Starts a stream for the region `rect` of the block `block`, at `now`.
    ///
    /// # Errors
    ///
    /// The error of OpenH264 if the encoder does not start.
    pub fn new(
        rect: (usize, usize, usize, usize),
        block: TileBox,
        now: Instant,
    ) -> Result<Self, openh264::Error> {
        let (_, _, w, h) = rect;
        // About 0.1 bit for each pixel at 30 frames each second, from 0.3 to 8 Mbit/s.
        let bits = u32::try_from(w * h * 3)
            .unwrap_or(u32::MAX)
            .clamp(300_000, 8_000_000);
        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(bits))
            .max_frame_rate(FrameRate::from_hz(30.0))
            .usage_type(UsageType::CameraVideoRealTime)
            .rate_control_mode(RateControlMode::Bitrate)
            .complexity(Complexity::Low)
            .skip_frames(false)
            .num_threads(1);
        let encoder = Encoder::with_api_config(OpenH264API::from_source(), config)?;
        Ok(Self {
            rect,
            block,
            last_change: now,
            last: Vec::new(),
            encoder,
            started: false,
            exclude: None,
        })
    }

    /// Reports whether the tile at (`column`, `row`) is fully in the region, and not in the excluded rectangle.
    #[must_use]
    pub fn covers(&self, column: usize, row: usize, width: usize, height: usize) -> bool {
        let (x, y, w, h) = self.rect;
        let (tx, ty) = (column * TILE, row * TILE);
        let (tw, th) = (TILE.min(width - tx), TILE.min(height - ty));
        tx >= x
            && ty >= y
            && tx + tw <= x + w
            && ty + th <= y + h
            && !excluded(self.exclude, column, row)
    }
}

/// Reports whether the tile at (`column`, `row`) touches the rectangle `exclude`.
#[must_use]
pub fn excluded(exclude: Option<(usize, usize, usize, usize)>, column: usize, row: usize) -> bool {
    exclude.is_some_and(|(x, y, w, h)| {
        let (tx, ty) = (column * TILE, row * TILE);
        tx < x + w && x < tx + TILE && ty < y + h && y < ty + TILE
    })
}

impl VideoStream {
    /// Encodes the region of the buffer `pixels` with the row length `width` if it changed since the last frame.
    pub fn frame(&mut self, pixels: &[u8], width: usize, now: Instant) -> Option<VideoFrame> {
        let (x, y, w, h) = self.rect;
        let mut region = Vec::with_capacity(w * h * 4);
        for row in y..y + h {
            let start = (row * width + x) * 4;
            region.extend_from_slice(&pixels[start..start + w * 4]);
        }
        if region == self.last {
            return None;
        }
        self.last = region;
        self.last_change = now;
        let yuv = YUVBuffer::from_bgra8_source(BgraSliceU8::new(&self.last, (w, h)));
        let stream = match self.encoder.encode(&yuv) {
            Ok(stream) => stream,
            Err(error) => {
                tracing::warn!(%error, "a video frame did not encode");
                return None;
            }
        };
        let kind = stream.frame_type();
        if matches!(kind, FrameType::Skip | FrameType::Invalid) {
            return None;
        }
        let mut flags = 0;
        if matches!(kind, FrameType::IDR | FrameType::I) {
            flags |= flag::KEY;
        }
        if !self.started {
            flags |= flag::NEW;
            self.started = true;
        }
        Some(VideoFrame {
            flags,
            data: stream.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_that_changes_at_a_video_rate_is_a_video_and_a_slow_change_is_not() {
        let mut detector = Detector::default();
        detector.reset(8, 6);
        let start = Instant::now();
        let mut changed = vec![false; 48];
        // A block of 3 x 2 tiles at columns 2..5 and rows 1..3, and a tile that changes in every second commit only.
        for row in 1..3 {
            for column in 2..5 {
                changed[row * 8 + column] = true;
            }
        }
        let mut found = None;
        for frame in 0..WINDOW as u32 {
            changed[5 * 8 + 7] = frame % 2 == 0;
            found = detector.record(&changed, start + Duration::from_millis(33) * frame);
        }
        assert_eq!(found, Some((2, 1, 5, 3)));
        changed[5 * 8 + 7] = false;

        // The same block, two commits each second: no video.
        detector.reset(8, 6);
        for frame in 0..WINDOW as u32 * 2 {
            found = detector.record(&changed, start + Duration::from_millis(500) * frame);
        }
        assert_eq!(found, None);

        // A small animation at 30 frames each second: no video.
        detector.reset(8, 6);
        let mut small = vec![false; 48];
        small[0] = true;
        small[1] = true;
        for frame in 0..WINDOW as u32 {
            found = detector.record(&small, start + Duration::from_millis(33) * frame);
        }
        assert_eq!(found, None);
    }

    #[test]
    fn the_region_has_an_even_size_in_the_buffer() {
        assert_eq!(
            pixel_rect((1, 1, 3, 2), 1000, 1000),
            Some((64, 64, 128, 64))
        );
        assert_eq!(pixel_rect((0, 0, 2, 2), 101, 77), Some((0, 0, 100, 76)));
        assert_eq!(pixel_rect((1, 0, 2, 1), 70, 64), None);
    }
}
