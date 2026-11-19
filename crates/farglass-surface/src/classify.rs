//! The class of a tile, and the lossy codec for image tiles (F028).
//!
//! Text and interface tiles have few colors, and LZ4 keeps them small and
//! exact. A photo or a gradient has many colors: LZ4 gives little, and a JPEG
//! is much smaller. The browser decodes a JPEG natively. A lossy tile becomes
//! exact later, when it stops changing.

use std::collections::HashSet;

/// A tile with more distinct colors than this is an image.
pub const IMAGE_COLORS: usize = 256;

/// The JPEG quality of a lossy tile. The lossless pass follows, thus a low value is good enough.
pub const JPEG_QUALITY: u8 = 70;

/// Reports whether a tile of BGRA pixels is an image: it has more than [`IMAGE_COLORS`] distinct colors.
#[must_use]
pub fn is_image(pixels: &[u8]) -> bool {
    let mut colors = HashSet::with_capacity(IMAGE_COLORS + 1);
    for pixel in pixels.as_chunks::<4>().0 {
        colors.insert(*pixel);
        if colors.len() > IMAGE_COLORS {
            return true;
        }
    }
    false
}

/// Returns the JPEG of a tile of BGRA pixels, or `None` if the encoder fails.
#[must_use]
pub fn jpeg(pixels: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let encoder = jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY);
    encoder
        .encode(
            pixels,
            u16::try_from(width).ok()?,
            u16::try_from(height).ok()?,
            jpeg_encoder::ColorType::Bgra,
        )
        .ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_has_few_colors_and_a_photo_has_many() {
        let text: Vec<u8> = (0..64 * 64)
            .flat_map(|i| {
                if i % 7 < 2 {
                    [20, 20, 20, 255]
                } else {
                    [250, 250, 250, 255]
                }
            })
            .collect();
        assert!(!is_image(&text));
        let photo: Vec<u8> = (0..64 * 64_u32)
            .flat_map(|i| {
                let v = i.wrapping_mul(2_654_435_761) >> 8;
                [
                    (v & 255) as u8,
                    ((v >> 8) & 255) as u8,
                    ((v >> 16) & 255) as u8,
                    255,
                ]
            })
            .collect();
        assert!(is_image(&photo));
        let encoded = jpeg(&photo, 64, 64).unwrap();
        assert_eq!(&encoded[..2], &[0xFF, 0xD8], "a JPEG starts with SOI");
    }
}
