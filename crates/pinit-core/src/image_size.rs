//! Read pixel dimensions from image headers without decoding or dependencies.
//! Returns the *displayed* size: JPEG EXIF orientation 5-8 swaps width/height,
//! matching what a browser `<img>` shows.

pub fn image_size(data: &[u8]) -> Option<(u32, u32)> {
    png(data).or_else(|| gif(data)).or_else(|| jpeg(data)).or_else(|| webp(data)).or_else(|| bmp(data))
}

fn be16(d: &[u8], i: usize) -> Option<u32> {
    Some(u16::from_be_bytes(d.get(i..i + 2)?.try_into().ok()?) as u32)
}
fn be32(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(i..i + 4)?.try_into().ok()?))
}
fn le16(d: &[u8], i: usize) -> Option<u32> {
    Some(u16::from_le_bytes(d.get(i..i + 2)?.try_into().ok()?) as u32)
}
fn le32(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(i..i + 4)?.try_into().ok()?))
}
fn nonzero(w: u32, h: u32) -> Option<(u32, u32)> {
    (w > 0 && h > 0).then_some((w, h))
}

fn png(d: &[u8]) -> Option<(u32, u32)> {
    if d.get(..8)? != b"\x89PNG\r\n\x1a\n" || d.get(12..16)? != b"IHDR" {
        return None;
    }
    nonzero(be32(d, 16)?, be32(d, 20)?)
}

fn gif(d: &[u8]) -> Option<(u32, u32)> {
    let h = d.get(..6)?;
    if h != b"GIF87a" && h != b"GIF89a" {
        return None;
    }
    nonzero(le16(d, 6)?, le16(d, 8)?)
}

fn bmp(d: &[u8]) -> Option<(u32, u32)> {
    if d.get(..2)? != b"BM" {
        return None;
    }
    let w = le32(d, 18)? as i32;
    let h = le32(d, 22)? as i32;
    nonzero(w.unsigned_abs(), h.unsigned_abs())
}

fn webp(d: &[u8]) -> Option<(u32, u32)> {
    if d.get(..4)? != b"RIFF" || d.get(8..12)? != b"WEBP" {
        return None;
    }
    match d.get(12..16)? {
        b"VP8 " => nonzero(le16(d, 26)? & 0x3fff, le16(d, 28)? & 0x3fff),
        b"VP8L" => {
            let b = d.get(21..25)?;
            let bits = u32::from_le_bytes(b.try_into().ok()?);
            nonzero((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
        }
        b"VP8X" => {
            let w = 1 + *d.get(24)? as u32 + ((*d.get(25)? as u32) << 8) + ((*d.get(26)? as u32) << 16);
            let h = 1 + *d.get(27)? as u32 + ((*d.get(28)? as u32) << 8) + ((*d.get(29)? as u32) << 16);
            nonzero(w, h)
        }
        _ => None,
    }
}

fn jpeg(d: &[u8]) -> Option<(u32, u32)> {
    if d.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    let mut orientation = 1u32;
    loop {
        // find next marker
        while *d.get(i)? != 0xFF {
            i += 1;
        }
        while *d.get(i)? == 0xFF {
            i += 1;
        }
        let marker = *d.get(i)?;
        i += 1;
        match marker {
            0xD8 | 0x01 | 0xD0..=0xD7 => continue, // standalone markers
            0xD9 | 0xDA => return None,            // EOI / SOS before SOF
            _ => {}
        }
        let len = be16(d, i)? as usize;
        let seg = d.get(i + 2..i + len)?;
        match marker {
            0xE1 if seg.starts_with(b"Exif\0\0") => {
                orientation = exif_orientation(&seg[6..]).unwrap_or(1);
            }
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                let h = be16(seg, 1)?;
                let w = be16(seg, 3)?;
                let (w, h) = nonzero(w, h)?;
                return Some(if orientation >= 5 { (h, w) } else { (w, h) });
            }
            _ => {}
        }
        i += len;
    }
}

fn exif_orientation(t: &[u8]) -> Option<u32> {
    let le = match t.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let r16 = |o: usize| -> Option<u32> {
        let b: [u8; 2] = t.get(o..o + 2)?.try_into().ok()?;
        Some(if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) } as u32)
    };
    let r32 = |o: usize| -> Option<u32> {
        let b: [u8; 4] = t.get(o..o + 4)?.try_into().ok()?;
        Some(if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    };
    let ifd = r32(4)? as usize;
    let n = r16(ifd)? as usize;
    for k in 0..n {
        let e = ifd + 2 + k * 12;
        if r16(e)? == 0x0112 {
            return r16(e + 8);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        v.extend(w.to_be_bytes());
        v.extend(h.to_be_bytes());
        v
    }

    fn jpeg_bytes(w: u16, h: u16, orientation: Option<u16>) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8];
        if let Some(o) = orientation {
            // little-endian TIFF with one IFD entry: tag 0x0112
            let mut tiff = b"II*\0".to_vec();
            tiff.extend(8u32.to_le_bytes());
            tiff.extend(1u16.to_le_bytes());
            tiff.extend(0x0112u16.to_le_bytes());
            tiff.extend(3u16.to_le_bytes());
            tiff.extend(1u32.to_le_bytes());
            tiff.extend(o.to_le_bytes());
            tiff.extend([0, 0]);
            tiff.extend(0u32.to_le_bytes());
            let mut seg = b"Exif\0\0".to_vec();
            seg.extend(tiff);
            v.extend([0xFF, 0xE1]);
            v.extend(((seg.len() + 2) as u16).to_be_bytes());
            v.extend(seg);
        }
        v.extend([0xFF, 0xC0, 0x00, 0x11, 0x08]);
        v.extend(h.to_be_bytes());
        v.extend(w.to_be_bytes());
        v.extend([0x03, 0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01]);
        v
    }

    #[test]
    fn png_dimensions() {
        assert_eq!(image_size(&png_bytes(640, 480)), Some((640, 480)));
    }

    #[test]
    fn gif_dimensions() {
        let mut v = b"GIF89a".to_vec();
        v.extend(320u16.to_le_bytes());
        v.extend(200u16.to_le_bytes());
        assert_eq!(image_size(&v), Some((320, 200)));
    }

    #[test]
    fn bmp_dimensions_with_negative_height() {
        let mut v = b"BM".to_vec();
        v.extend([0u8; 16]);
        v.extend(100i32.to_le_bytes());
        v.extend((-50i32).to_le_bytes());
        assert_eq!(image_size(&v), Some((100, 50)));
    }

    #[test]
    fn jpeg_plain_and_exif_rotated() {
        assert_eq!(image_size(&jpeg_bytes(4000, 3000, None)), Some((4000, 3000)));
        assert_eq!(image_size(&jpeg_bytes(4000, 3000, Some(1))), Some((4000, 3000)));
        assert_eq!(image_size(&jpeg_bytes(4000, 3000, Some(6))), Some((3000, 4000)));
        assert_eq!(image_size(&jpeg_bytes(4000, 3000, Some(3))), Some((4000, 3000)));
    }

    #[test]
    fn webp_lossy_lossless_extended() {
        let mut lossy = b"RIFF\0\0\0\0WEBPVP8 \0\0\0\0\0\0\0\0\0\0\0\0\0\0".to_vec();
        lossy[26..28].copy_from_slice(&800u16.to_le_bytes());
        lossy[28..30].copy_from_slice(&600u16.to_le_bytes());
        assert_eq!(image_size(&lossy), Some((800, 600)));

        let mut ll = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2f\0\0\0\0".to_vec();
        let bits: u32 = (500 - 1) | ((300 - 1) << 14);
        ll[21..25].copy_from_slice(&bits.to_le_bytes());
        assert_eq!(image_size(&ll), Some((500, 300)));

        let mut ext = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\0\0\0\0\0\0\0\0\0\0".to_vec();
        ext[24..27].copy_from_slice(&(1920u32 - 1).to_le_bytes()[..3]);
        ext[27..30].copy_from_slice(&(1080u32 - 1).to_le_bytes()[..3]);
        assert_eq!(image_size(&ext), Some((1920, 1080)));
    }

    #[test]
    fn garbage_and_truncated_input_do_not_panic() {
        assert_eq!(image_size(b""), None);
        assert_eq!(image_size(b"not an image at all"), None);
        assert_eq!(image_size(&png_bytes(1, 1)[..10]), None);
        assert_eq!(image_size(&jpeg_bytes(10, 10, Some(6))[..9]), None);
        assert_eq!(image_size(&png_bytes(0, 5)), None);
    }
}
