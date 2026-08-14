use crate::{DeckResource, PixelSize, ResourceKind};
use wasmppt_xml::{TokenKind, XmlDocument};

/// Derives canonical display-axis dimensions from bounded resource bytes, falling back to a
/// positive host hint only when the supported byte format has no usable dimensions.
#[must_use]
pub fn inspect_media_size(resource: &DeckResource) -> Option<PixelSize> {
    let derived = match (resource.kind, resource.media_type.as_str()) {
        (ResourceKind::RasterImage, "image/png") => png_size(&resource.bytes),
        (ResourceKind::RasterImage, "image/jpeg" | "image/jpg") => {
            inspect_jpeg_size(&resource.bytes)
        }
        (ResourceKind::RasterImage, "image/gif") => gif_size(&resource.bytes),
        (ResourceKind::Svg, "image/svg+xml") => svg_size(&resource.bytes),
        _ => None,
    };
    derived.or_else(|| {
        resource
            .intrinsic_size
            .filter(|size| size.width > 0 && size.height > 0)
    })
}

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

fn png_size(bytes: &[u8]) -> Option<PixelSize> {
    if bytes.get(..8)? != b"\x89PNG\r\n\x1a\n" || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    pixel_size(
        u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?),
        u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?),
    )
}

fn gif_size(bytes: &[u8]) -> Option<PixelSize> {
    if !matches!(bytes.get(..6)?, b"GIF87a" | b"GIF89a") {
        return None;
    }
    pixel_size(
        u32::from(u16::from_le_bytes(bytes.get(6..8)?.try_into().ok()?)),
        u32::from(u16::from_le_bytes(bytes.get(8..10)?.try_into().ok()?)),
    )
}

fn svg_size(bytes: &[u8]) -> Option<PixelSize> {
    let document = XmlDocument::parse(bytes.to_vec()).ok()?;
    let (width, height, view_box) = document.tokens().iter().find_map(|token| {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            return None;
        };
        (name.local == "svg").then(|| {
            let attribute = |name: &str| {
                attributes
                    .iter()
                    .find(|attribute| attribute.name.local == name)
                    .map(|attribute| attribute.value.as_str())
            };
            (
                attribute("width"),
                attribute("height"),
                attribute("viewBox"),
            )
        })
    })?;
    if let (Some(width), Some(height)) = (width.and_then(svg_length), height.and_then(svg_length)) {
        return pixel_size(width, height);
    }
    let values = view_box?
        .split(|character: char| character.is_ascii_whitespace() || character == ',')
        .filter(|value| !value.is_empty())
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (values.len() == 4).then_some(())?;
    pixel_size(f64_to_dimension(values[2])?, f64_to_dimension(values[3])?)
}

fn svg_length(value: &str) -> Option<u32> {
    let number = value
        .trim()
        .strip_suffix("px")
        .unwrap_or(value.trim())
        .parse::<f64>()
        .ok()?;
    f64_to_dimension(number)
}

fn f64_to_dimension(value: f64) -> Option<u32> {
    (value.is_finite() && value > 0.0 && value <= f64::from(u32::MAX)).then(|| value.ceil() as u32)
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
