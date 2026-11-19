//! Scroll detection (F027).
//!
//! When a client scrolls, most rows of the new frame are rows of the old
//! frame at a different height. The detector hashes each row of a region in
//! the two frames, finds the vertical offset that most rows agree on, and
//! returns the largest block of rows that moved with that offset. The client
//! copies the block on its canvas, and only the new strip goes as tiles.

use std::collections::HashMap;

/// A block of rows that moved: the rows `y .. y + height` of the old frame are at `y + dy` in the new frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    /// The left edge.
    pub x: usize,
    /// The top edge in the old frame.
    pub y: usize,
    /// The width.
    pub width: usize,
    /// The height.
    pub height: usize,
    /// The vertical offset: negative is up.
    pub dy: isize,
}

/// The fewest rows that a move must have. A smaller move costs more than the tiles that it saves.
const MIN_ROWS: usize = 24;

/// A row hash that more rows than this have is a row of one color: it gives no vote.
const COMMON: usize = 8;

fn row_hash(pixels: &[u8], width: usize, y: usize, x0: usize, x1: usize) -> u64 {
    let start = (y * width + x0) * 4;
    xxhash_rust::xxh3::xxh3_64(&pixels[start..start + (x1 - x0) * 4])
}

/// Finds a vertical move in the columns `x0 .. x1` and the rows `y0 .. y1` of two frames of the same size `width`.
#[must_use]
pub fn detect(
    before: &[u8],
    after: &[u8],
    width: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
) -> Option<Move> {
    if y1.saturating_sub(y0) < MIN_ROWS * 2 || x1.saturating_sub(x0) < 16 {
        return None;
    }
    let old: Vec<u64> = (y0..y1)
        .map(|y| row_hash(before, width, y, x0, x1))
        .collect();
    let new: Vec<u64> = (y0..y1)
        .map(|y| row_hash(after, width, y, x0, x1))
        .collect();
    let mut rows: HashMap<u64, Vec<usize>> = HashMap::new();
    for (index, hash) in old.iter().enumerate() {
        rows.entry(*hash).or_default().push(index);
    }
    let mut votes: HashMap<isize, usize> = HashMap::new();
    for (index, hash) in new.iter().enumerate() {
        let Some(matches) = rows.get(hash) else {
            continue;
        };
        if matches.len() > COMMON {
            continue;
        }
        for &from in matches {
            let dy = isize::try_from(index).ok()? - isize::try_from(from).ok()?;
            if dy != 0 {
                *votes.entry(dy).or_default() += 1;
            }
        }
    }
    let (&dy, &count) = votes
        .iter()
        .max_by_key(|(dy, count)| (**count, -dy.abs()))?;
    if count < MIN_ROWS {
        return None;
    }
    // The largest block of rows that equal the old rows at the offset.
    let (mut best_start, mut best_len, mut start, mut len) = (0, 0, 0, 0);
    for index in 0..new.len() {
        let from = isize::try_from(index).ok()? - dy;
        let same = usize::try_from(from)
            .ok()
            .is_some_and(|from| from < old.len() && old[from] == new[index]);
        if same {
            if len == 0 {
                start = index;
            }
            len += 1;
            if len > best_len {
                (best_start, best_len) = (start, len);
            }
        } else {
            len = 0;
        }
    }
    if best_len < MIN_ROWS {
        return None;
    }
    let dest = y0 + best_start;
    Some(Move {
        x: x0,
        y: usize::try_from(isize::try_from(dest).ok()? - dy).ok()?,
        width: x1 - x0,
        height: best_len,
        dy,
    })
}

/// Applies `step` to a frame, as the client does on its canvas.
pub fn apply(pixels: &mut [u8], width: usize, step: Move) {
    let row = step.width * 4;
    let block: Vec<u8> = (step.y..step.y + step.height)
        .flat_map(|y| pixels[(y * width + step.x) * 4..][..row].to_vec())
        .collect();
    for (index, line) in block.chunks_exact(row).enumerate() {
        let Some(to) = (step.y + index).checked_add_signed(step.dy) else {
            continue;
        };
        pixels[(to * width + step.x) * 4..][..row].copy_from_slice(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(width: usize, height: usize, scroll: usize) -> Vec<u8> {
        let mut pixels = vec![255; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let line = y + scroll;
                if (line % 18) < 12 && ((x * 7 + line / 18 * 13) % 23) < 9 {
                    pixels[(y * width + x) * 4..][..3].copy_from_slice(&[
                        30,
                        30,
                        u8::try_from(line % 200).unwrap(),
                    ]);
                }
            }
        }
        pixels
    }

    #[test]
    fn a_scroll_is_one_move_and_applying_it_gives_the_new_rows() {
        let (w, h) = (200, 300);
        let before = page(w, h, 0);
        let after = page(w, h, 37);
        let step = detect(&before, &after, w, 0, 0, w, h).expect("a move");
        assert_eq!(step.dy, -37);
        assert_eq!((step.y, step.height), (37, 263));
        let mut moved = before.clone();
        apply(&mut moved, w, step);
        assert_eq!(moved[..263 * w * 4], after[..263 * w * 4]);
    }

    #[test]
    fn no_move_for_a_frame_with_no_scroll_or_a_small_region() {
        let (w, h) = (200, 300);
        let before = page(w, h, 0);
        assert_eq!(detect(&before, &before, w, 0, 0, w, h), None);
        assert_eq!(detect(&before, &page(w, h, 5), w, 0, 0, w, 30), None);
    }
}
