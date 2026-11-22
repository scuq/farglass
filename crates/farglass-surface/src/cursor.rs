//! The pointer images of the clients as PNG files for a CSS cursor (F084).

use std::io::Write as _;

use crate::{CursorImage, Format};

/// Returns the image as a PNG file with straight alpha, or `None` if the pixels do not match the size.
///
/// A Wayland buffer has premultiplied alpha in the byte order blue, green, red, alpha.
/// A PNG file has straight alpha in the order red, green, blue, alpha.
#[must_use]
pub fn png(image: &CursorImage) -> Option<Vec<u8>> {
    let width = usize::try_from(image.width).ok()?;
    let height = usize::try_from(image.height).ok()?;
    if width == 0 || height == 0 || image.pixels.len() != width * height * 4 {
        return None;
    }
    // Each row starts with the filter type 0: the bytes as they are.
    let mut raw = Vec::with_capacity(height * (width * 4 + 1));
    for row in image.pixels.chunks_exact(width * 4) {
        raw.push(0);
        for &[b, g, r, a] in row.as_chunks::<4>().0 {
            let a = match image.format {
                Format::Argb8888 => a,
                Format::Xrgb8888 => 255,
            };
            let straight = |c: u8| match a {
                0 => 0,
                255 => c,
                _ => u8::try_from((u16::from(c) * 255 + u16::from(a) / 2) / u16::from(a))
                    .unwrap_or(255),
            };
            raw.extend_from_slice(&[straight(r), straight(g), straight(b), a]);
        }
    }
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    zlib.write_all(&raw).ok()?;
    let data = zlib.finish().ok()?;

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&image.width.to_be_bytes());
    header.extend_from_slice(&image.height.to_be_bytes());
    // 8 bits for each channel, RGBA, deflate, no interlace.
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &data);
    chunk(&mut out, b"IEND", &[]);
    Some(out)
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.sum().to_be_bytes());
}

#[cfg(test)]
mod tests {
    use std::io::Read as _;
    use std::sync::Arc;

    use super::*;

    #[test]
    fn a_premultiplied_pixel_becomes_a_straight_pixel_in_a_valid_png() {
        // Blue, green, red, alpha: red at half alpha, premultiplied.
        let image = CursorImage {
            width: 2,
            height: 1,
            hotspot_x: 0,
            hotspot_y: 0,
            format: Format::Argb8888,
            pixels: Arc::new(vec![0, 0, 128, 128, 10, 20, 30, 0]),
        };
        let file = png(&image).unwrap();
        assert_eq!(&file[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&file[12..16], b"IHDR");
        assert_eq!(&file[16..24], &[0, 0, 0, 2, 0, 0, 0, 1]);
        // The IDAT data starts after IHDR: 8 + (4 + 4 + 13 + 4).
        let idat = 8 + 25;
        assert_eq!(&file[idat + 4..idat + 8], b"IDAT");
        let len =
            usize::try_from(u32::from_be_bytes(file[idat..idat + 4].try_into().unwrap())).unwrap();
        let mut raw = Vec::new();
        flate2::read::ZlibDecoder::new(&file[idat + 8..idat + 8 + len])
            .read_to_end(&mut raw)
            .unwrap();
        assert_eq!(raw, vec![0, 255, 0, 0, 128, 0, 0, 0, 0]);
        assert!(file.ends_with(&[0xae, 0x42, 0x60, 0x82]), "the CRC of IEND");
    }

    #[test]
    fn pixels_that_do_not_match_the_size_give_no_png() {
        let image = CursorImage {
            width: 4,
            height: 4,
            hotspot_x: 0,
            hotspot_y: 0,
            format: Format::Xrgb8888,
            pixels: Arc::new(vec![0; 8]),
        };
        assert!(png(&image).is_none());
    }
}
