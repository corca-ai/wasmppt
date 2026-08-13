//! Independently loaded Wasm boundary for exact font-byte shaping.

use wasm_bindgen::prelude::*;
use wasmppt_shaper::{Direction, ShapeOptions};

const MAGIC: &[u8; 4] = b"WPSH";
const VERSION: u16 = 1;
const LINE_BREAK_MAGIC: &[u8; 4] = b"WPLB";

#[wasm_bindgen]
// Flat scalar/string parameters keep the optional Wasm ABI allocation-free and backend-neutral.
#[allow(clippy::too_many_arguments)]
pub fn shape_font(
    font_bytes: &[u8],
    face_index: u32,
    text: &str,
    direction: u8,
    language: &str,
    script: &str,
    features: &str,
    variations: &str,
    max_font_bytes: u32,
    max_text_bytes: u32,
    max_glyphs: u32,
) -> Result<Vec<u8>, JsError> {
    let direction = match direction {
        0 => Direction::LeftToRight,
        1 => Direction::RightToLeft,
        2 => Direction::TopToBottom,
        3 => Direction::BottomToTop,
        _ => return Err(JsError::new("text direction is invalid")),
    };
    let features = split_properties(features);
    let variations = split_properties(variations);
    let run = wasmppt_shaper::shape_configured(
        font_bytes,
        text,
        ShapeOptions {
            face_index,
            direction,
            limits: wasmppt_shaper::ShapeLimits {
                max_font_bytes: max_font_bytes as usize,
                max_text_bytes: max_text_bytes as usize,
                max_glyphs: max_glyphs as usize,
            },
        },
        wasmppt_shaper::ShapeProperties {
            language: (!language.is_empty()).then_some(language),
            script: (!script.is_empty()).then_some(script),
            features: &features,
            variations: &variations,
        },
    )
    .map_err(|error| JsError::new(&format!("{:?}: {error}", error.code)))?;
    let glyph_count = u32::try_from(run.glyphs.len())
        .map_err(|_| JsError::new("shaped glyph count cannot be encoded"))?;
    let mut output = Vec::with_capacity(12 + run.glyphs.len() * 25);
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&VERSION.to_le_bytes());
    output.extend_from_slice(&run.units_per_em.to_le_bytes());
    output.extend_from_slice(&glyph_count.to_le_bytes());
    for glyph in run.glyphs {
        output.extend_from_slice(&glyph.glyph_id.to_le_bytes());
        output.extend_from_slice(&glyph.cluster.to_le_bytes());
        output.extend_from_slice(&glyph.x_advance.to_le_bytes());
        output.extend_from_slice(&glyph.y_advance.to_le_bytes());
        output.extend_from_slice(&glyph.x_offset.to_le_bytes());
        output.extend_from_slice(&glyph.y_offset.to_le_bytes());
        output.push(u8::from(glyph.safe_to_break));
    }
    Ok(output)
}

fn split_properties(properties: &str) -> Vec<&str> {
    if properties.is_empty() {
        Vec::new()
    } else {
        properties.split('\0').collect()
    }
}

#[wasm_bindgen]
pub fn line_breaks(text: &str, max_text_bytes: u32) -> Result<Vec<u8>, JsError> {
    let breaks = wasmppt_shaper::line_breaks(text, max_text_bytes as usize)
        .map_err(|error| JsError::new(&format!("{:?}: {error}", error.code)))?;
    let count = u32::try_from(breaks.len())
        .map_err(|_| JsError::new("line-break count cannot be encoded"))?;
    let mut output = Vec::with_capacity(10 + breaks.len() * 5);
    output.extend_from_slice(LINE_BREAK_MAGIC);
    output.extend_from_slice(&VERSION.to_le_bytes());
    output.extend_from_slice(&count.to_le_bytes());
    for opportunity in breaks {
        output.extend_from_slice(&opportunity.offset.to_le_bytes());
        output.push(match opportunity.kind {
            wasmppt_shaper::LineBreakKind::Allowed => 0,
            wasmppt_shaper::LineBreakKind::Mandatory => 1,
        });
    }
    Ok(output)
}
