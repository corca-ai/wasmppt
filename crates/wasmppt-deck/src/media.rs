use crate::PixelSize;

/// Reads bounded JPEG frame dimensions and applies the display aspect implied by EXIF orientation.
#[must_use]
pub fn inspect_jpeg_size(bytes: &[u8]) -> Option<PixelSize> {
    if bytes.get(..2)? != [0xff, 0xd8] {
        return None;
    }
    let mut offset = 2usize;
    let mut orientation = 1u16;
    while offset.saturating_add(4) <= bytes.len() {
        while bytes.get(offset) == Some(&0xff) {
            offset = offset.saturating_add(1);
        }
        let marker = *bytes.get(offset)?;
        offset = offset.saturating_add(1);
        if matches!(marker, 0x01 | 0xd8 | 0xd9) || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        if marker == 0xda {
            break;
        }
        let length = usize::from(u16::from_be_bytes(
            bytes
                .get(offset..offset.saturating_add(2))?
                .try_into()
                .ok()?,
        ));
        let end = offset.checked_add(length)?;
        if length < 2 || end > bytes.len() {
            return None;
        }
        if marker == 0xe1 {
            orientation = exif_orientation(bytes.get(offset.saturating_add(2)..end)?);
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            let height = u32::from(u16::from_be_bytes(
                bytes
                    .get(offset.saturating_add(3)..offset.saturating_add(5))?
                    .try_into()
                    .ok()?,
            ));
            let width = u32::from(u16::from_be_bytes(
                bytes
                    .get(offset.saturating_add(5)..offset.saturating_add(7))?
                    .try_into()
                    .ok()?,
            ));
            return if (5..=8).contains(&orientation) {
                pixel_size(height, width)
            } else {
                pixel_size(width, height)
            };
        }
        offset = end;
    }
    None
}

fn exif_orientation(bytes: &[u8]) -> u16 {
    if bytes.get(..6) != Some(b"Exif\0\0") {
        return 1;
    }
    let Some(tiff) = bytes.get(6..) else {
        return 1;
    };
    let little_endian = match tiff.get(..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return 1,
    };
    let Some(directory) =
        read_u32(tiff, 4, little_endian).and_then(|value| usize::try_from(value).ok())
    else {
        return 1;
    };
    let Some(entries) = read_u16(tiff, directory, little_endian).map(usize::from) else {
        return 1;
    };
    let available = tiff.len().saturating_sub(directory.saturating_add(2)) / 12;
    for index in 0..entries.min(available).min(256) {
        let entry = directory
            .saturating_add(2)
            .saturating_add(index.saturating_mul(12));
        if read_u16(tiff, entry, little_endian) == Some(0x0112)
            && read_u16(tiff, entry.saturating_add(2), little_endian) == Some(3)
            && read_u32(tiff, entry.saturating_add(4), little_endian) == Some(1)
        {
            return read_u16(tiff, entry.saturating_add(8), little_endian)
                .filter(|value| (1..=8).contains(value))
                .unwrap_or(1);
        }
    }
    1
}

fn read_u16(bytes: &[u8], offset: usize, little_endian: bool) -> Option<u16> {
    let value = bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?;
    Some(if little_endian {
        u16::from_le_bytes(value)
    } else {
        u16::from_be_bytes(value)
    })
}

fn read_u32(bytes: &[u8], offset: usize, little_endian: bool) -> Option<u32> {
    let value = bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(if little_endian {
        u32::from_le_bytes(value)
    } else {
        u32::from_be_bytes(value)
    })
}

const fn pixel_size(width: u32, height: u32) -> Option<PixelSize> {
    if width > 0 && height > 0 {
        Some(PixelSize { width, height })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(orientation: u16) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xe1, 0x00, 0x22];
        bytes.extend_from_slice(b"Exif\0\0II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&0x0112u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&orientation.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0xff, 0xc0, 0x00, 0x07, 0x08, 0x01, 0x2c, 0x03, 0x20]);
        bytes
    }

    #[test]
    fn applies_bounded_exif_display_orientation_to_jpeg_dimensions() {
        for orientation in 1..=8 {
            let expected = if orientation >= 5 {
                PixelSize {
                    width: 300,
                    height: 800,
                }
            } else {
                PixelSize {
                    width: 800,
                    height: 300,
                }
            };
            assert_eq!(inspect_jpeg_size(&jpeg(orientation)), Some(expected));
        }
        let mut truncated = jpeg(6);
        truncated.truncate(18);
        assert_eq!(inspect_jpeg_size(&truncated), None);
    }
}
