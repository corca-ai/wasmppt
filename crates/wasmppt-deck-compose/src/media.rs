use std::{io::Cursor, num::NonZeroU64, sync::Arc};

use gif::{ColorOutput, DecodeOptions, MemoryLimit};
use wasmppt_deck::{DeckResource, PixelSize, ResourceKind, StableId, inspect_media_size};
use wasmppt_xml::{TokenKind, XmlDocument};

use crate::{ComposeError, ComposeErrorCode, ComposeLimits, stable_id_hex};

#[derive(Clone, Debug)]
pub(crate) struct PreparedMedia {
    pub(crate) part_name: String,
    pub(crate) content_type: &'static str,
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) size: Option<PixelSize>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum PreparedMediaKey {
    Formula(StableId),
    Resource(StableId),
}

pub(crate) fn prepare_media(
    resource: &DeckResource,
    limits: &ComposeLimits,
) -> Result<PreparedMedia, ComposeError> {
    if resource.bytes.is_empty() || resource.bytes.len() > limits.max_media_bytes {
        return Err(media_error(
            "media is empty or exceeds the configured byte bound",
        ));
    }
    let stem = stable_id_hex(resource.id);
    match (resource.kind, resource.media_type.as_str()) {
        (ResourceKind::RasterImage, "image/png") => Ok(PreparedMedia {
            part_name: format!("ppt/media/deck-{stem}.png"),
            content_type: "image/png",
            bytes: resource.bytes.clone().into(),
            size: inspect_media_size(resource),
        }),
        (ResourceKind::RasterImage, "image/jpeg" | "image/jpg") => Ok(PreparedMedia {
            part_name: format!("ppt/media/deck-{stem}.jpg"),
            content_type: "image/jpeg",
            bytes: resource.bytes.clone().into(),
            size: inspect_media_size(resource),
        }),
        (ResourceKind::RasterImage, "image/gif") => {
            let (bytes, size) = gif_first_frame(&resource.bytes, limits)?;
            Ok(PreparedMedia {
                part_name: format!("ppt/media/deck-{stem}-first-frame.png"),
                content_type: "image/png",
                bytes: bytes.into(),
                size: Some(size),
            })
        }
        (ResourceKind::Svg, "image/svg+xml") => {
            validate_svg(&resource.bytes)?;
            Ok(PreparedMedia {
                part_name: format!("ppt/media/deck-{stem}.svg"),
                content_type: "image/svg+xml",
                bytes: resource.bytes.clone().into(),
                size: inspect_media_size(resource),
            })
        }
        _ => Err(media_error("unsupported deck media kind or content type")),
    }
}

pub(crate) fn prepare_formula_media(
    resource: &DeckResource,
    node_id: StableId,
    color: u32,
    limits: &ComposeLimits,
) -> Result<PreparedMedia, ComposeError> {
    let mut prepared = prepare_media(resource, limits)?;
    if prepared.content_type != "image/svg+xml" {
        return Err(media_error("formula media must be SVG"));
    }
    let bytes = resolve_svg_current_color(&prepared.bytes, color)?;
    if bytes.len() > limits.max_media_bytes {
        return Err(media_error(
            "resolved formula SVG exceeds the configured byte bound",
        ));
    }
    prepared.part_name = format!("ppt/media/deck-{}-formula.svg", stable_id_hex(node_id));
    prepared.bytes = bytes.into();
    Ok(prepared)
}

fn resolve_svg_current_color(source: &[u8], color: u32) -> Result<Vec<u8>, ComposeError> {
    let document = XmlDocument::parse(source.to_vec())
        .map_err(|error| media_error(format!("invalid SVG XML: {error}")))?;
    let replacement = format!("#{:06X}", color & 0x00ff_ffff);
    let mut patches = Vec::new();
    for token in document.tokens() {
        let TokenKind::Start { attributes, .. } = &token.kind else {
            continue;
        };
        for attribute in attributes {
            if !matches!(
                attribute.name.local.as_str(),
                "color"
                    | "fill"
                    | "flood-color"
                    | "lighting-color"
                    | "stop-color"
                    | "stroke"
                    | "style"
            ) {
                continue;
            }
            if let Some(value) = replace_ascii_case_insensitive(
                attribute.value.as_bytes(),
                b"currentColor",
                replacement.as_bytes(),
            ) {
                patches.push((attribute.value_range.clone(), value));
            }
        }
    }
    let mut output = source.to_vec();
    for (range, value) in patches.into_iter().rev() {
        output.splice(range, value);
    }
    Ok(output)
}

fn replace_ascii_case_insensitive(
    source: &[u8],
    needle: &[u8],
    replacement: &[u8],
) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(source.len());
    let mut cursor = 0;
    let mut replaced = false;
    while let Some(relative) = source[cursor..]
        .windows(needle.len())
        .position(|candidate| candidate.eq_ignore_ascii_case(needle))
    {
        let start = cursor + relative;
        output.extend_from_slice(&source[cursor..start]);
        output.extend_from_slice(replacement);
        cursor = start + needle.len();
        replaced = true;
    }
    if !replaced {
        return None;
    }
    output.extend_from_slice(&source[cursor..]);
    Some(output)
}

fn gif_first_frame(
    source: &[u8],
    limits: &ComposeLimits,
) -> Result<(Vec<u8>, PixelSize), ComposeError> {
    let memory_bytes = limits
        .max_decoded_pixels
        .checked_mul(4)
        .and_then(|bytes| u64::try_from(bytes).ok())
        .and_then(NonZeroU64::new)
        .ok_or_else(|| media_error("GIF decoded-pixel bound is zero or overflows"))?;
    let mut options = DecodeOptions::new();
    options.set_color_output(ColorOutput::RGBA);
    options.set_memory_limit(MemoryLimit::Bytes(memory_bytes));
    options.check_frame_consistency(true);
    options.check_lzw_end_code(true);
    let mut decoder = options
        .read_info(Cursor::new(source))
        .map_err(|error| media_error(format!("cannot decode GIF header: {error}")))?;
    let width = u32::from(decoder.width());
    let height = u32::from(decoder.height());
    let pixels = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| media_error("GIF dimensions overflow"))?;
    if pixels == 0 || pixels > limits.max_decoded_pixels {
        return Err(media_error("GIF dimensions exceed the decoded-pixel bound"));
    }
    let frame = decoder
        .read_next_frame()
        .map_err(|error| media_error(format!("cannot decode GIF first frame: {error}")))?
        .ok_or_else(|| media_error("GIF has no image frame"))?;
    let output_bytes = pixels
        .checked_mul(4)
        .ok_or_else(|| media_error("GIF output dimensions overflow"))?;
    let mut rgba = vec![0; output_bytes];
    let frame_width = usize::from(frame.width);
    let frame_height = usize::from(frame.height);
    let left = usize::from(frame.left);
    let top = usize::from(frame.top);
    let canvas_width = width as usize;
    for row in 0..frame_height {
        let source_start = row
            .checked_mul(frame_width)
            .and_then(|offset| offset.checked_mul(4))
            .ok_or_else(|| media_error("GIF frame row overflows"))?;
        let target_start = top
            .checked_add(row)
            .and_then(|row| row.checked_mul(canvas_width))
            .and_then(|offset| offset.checked_add(left))
            .and_then(|offset| offset.checked_mul(4))
            .ok_or_else(|| media_error("GIF canvas row overflows"))?;
        let amount = frame_width
            .checked_mul(4)
            .ok_or_else(|| media_error("GIF frame width overflows"))?;
        let source_end = source_start
            .checked_add(amount)
            .ok_or_else(|| media_error("GIF source row overflows"))?;
        let target_end = target_start
            .checked_add(amount)
            .ok_or_else(|| media_error("GIF target row overflows"))?;
        let source_row = frame
            .buffer
            .get(source_start..source_end)
            .ok_or_else(|| media_error("GIF frame buffer is truncated"))?;
        let target_row = rgba
            .get_mut(target_start..target_end)
            .ok_or_else(|| media_error("GIF frame exceeds its logical canvas"))?;
        target_row.copy_from_slice(source_row);
    }

    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| media_error(format!("cannot encode GIF still header: {error}")))?;
        writer
            .write_image_data(&rgba)
            .map_err(|error| media_error(format!("cannot encode GIF still pixels: {error}")))?;
    }
    if png.len() > limits.max_media_bytes {
        return Err(media_error(
            "encoded GIF first frame exceeds the media byte bound",
        ));
    }
    Ok((png, PixelSize { width, height }))
}

fn validate_svg(source: &[u8]) -> Result<(), ComposeError> {
    let lowercase = String::from_utf8_lossy(source).to_ascii_lowercase();
    if [
        "javascript:",
        "@import",
        "url(http:",
        "url(https:",
        "url(//",
    ]
    .iter()
    .any(|needle| lowercase.contains(needle))
    {
        return Err(media_error("SVG contains active or external CSS"));
    }
    let document = XmlDocument::parse(source.to_vec())
        .map_err(|error| media_error(format!("invalid SVG XML: {error}")))?;
    let mut saw_root = false;
    for token in document.tokens() {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        if !saw_root {
            if name.local != "svg" {
                return Err(media_error("SVG root element is not svg"));
            }
            saw_root = true;
        }
        if matches!(name.local.as_str(), "script" | "foreignObject") {
            return Err(media_error("SVG contains active content"));
        }
        for attribute in attributes {
            let local = attribute.name.local.to_ascii_lowercase();
            let value = attribute.value.trim().to_ascii_lowercase();
            if local.starts_with("on")
                || (matches!(local.as_str(), "href" | "src")
                    && !value.is_empty()
                    && !value.starts_with('#'))
                || value.contains("javascript:")
                || value.contains("@import")
            {
                return Err(media_error("SVG contains an active or external reference"));
            }
        }
    }
    if !saw_root {
        return Err(media_error("SVG has no root element"));
    }
    Ok(())
}

fn media_error(message: impl Into<String>) -> ComposeError {
    ComposeError::new(ComposeErrorCode::InvalidMedia, message)
}

#[cfg(test)]
mod tests {
    use wasmppt_deck::{DeckResource, ResourceKind, StableId};

    use super::*;

    fn svg(bytes: &[u8]) -> DeckResource {
        DeckResource {
            id: StableId::from_bytes([1; 16]),
            kind: ResourceKind::Svg,
            media_type: "image/svg+xml".to_owned(),
            bytes: bytes.to_vec(),
            intrinsic_size: None,
        }
    }

    #[test]
    fn rejects_active_external_and_oversized_svg_before_composition() {
        for bytes in [
            br#"<svg><script>alert(1)</script></svg>"#.as_slice(),
            br#"<svg><style>@import url(https://example.com/x)</style></svg>"#.as_slice(),
            br#"<svg><image href="https://example.com/x.png"/></svg>"#.as_slice(),
        ] {
            assert_eq!(
                prepare_media(&svg(bytes), &ComposeLimits::default())
                    .unwrap_err()
                    .code(),
                ComposeErrorCode::InvalidMedia
            );
        }
        let limits = ComposeLimits {
            max_media_bytes: 4,
            ..ComposeLimits::default()
        };
        assert_eq!(
            prepare_media(&svg(b"<svg/>"), &limits).unwrap_err().code(),
            ComposeErrorCode::InvalidMedia
        );
    }

    #[test]
    fn jpeg_preparation_replaces_a_raw_hint_with_oriented_dimensions() {
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xe1, 0x00, 0x22];
        bytes.extend_from_slice(b"Exif\0\0II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&0x0112u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&6u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0xff, 0xc0, 0x00, 0x07, 0x08, 0x01, 0x2c, 0x03, 0x20]);
        let resource = DeckResource {
            id: StableId::from_bytes([2; 16]),
            kind: ResourceKind::RasterImage,
            media_type: "image/jpeg".to_owned(),
            bytes,
            intrinsic_size: Some(PixelSize {
                width: 800,
                height: 300,
            }),
        };

        assert_eq!(
            prepare_media(&resource, &ComposeLimits::default())
                .unwrap()
                .size,
            Some(PixelSize {
                width: 300,
                height: 800,
            })
        );
    }
}
