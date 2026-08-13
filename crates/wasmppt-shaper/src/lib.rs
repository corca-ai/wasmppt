//! Optional, host-agnostic OpenType shaping over exact font bytes.
//!
//! This crate is deliberately absent from the generation engine dependency graph. Browser and
//! native rendering hosts may load it independently when exact embedded or supplied font bytes are
//! available.

use std::fmt;
use std::str::FromStr;

const DEFAULT_MAX_FONT_BYTES: usize = 32 * 1024 * 1024;
const DEFAULT_MAX_TEXT_BYTES: usize = 1024 * 1024;
const DEFAULT_MAX_GLYPHS: usize = 1_048_576;
const MAX_FACE_INDEX: u32 = 63;
const MAX_SHAPE_PROPERTIES: usize = 64;
const MAX_SHAPE_PROPERTY_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShapeLimits {
    pub max_font_bytes: usize,
    pub max_text_bytes: usize,
    pub max_glyphs: usize,
}

impl Default for ShapeLimits {
    fn default() -> Self {
        Self {
            max_font_bytes: DEFAULT_MAX_FONT_BYTES,
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
            max_glyphs: DEFAULT_MAX_GLYPHS,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShapeOptions {
    pub face_index: u32,
    pub direction: Direction,
    pub limits: ShapeLimits,
}

impl Default for ShapeOptions {
    fn default() -> Self {
        Self {
            face_index: 0,
            direction: Direction::LeftToRight,
            limits: ShapeLimits::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShapedGlyph {
    pub glyph_id: u32,
    /// UTF-8 byte offset in the original input, as defined by HarfBuzz cluster semantics.
    pub cluster: u32,
    pub x_advance: i32,
    pub y_advance: i32,
    pub x_offset: i32,
    pub y_offset: i32,
    pub safe_to_break: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShapedRun {
    pub units_per_em: u16,
    pub glyphs: Vec<ShapedGlyph>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ShapeProperties<'a> {
    pub language: Option<&'a str>,
    pub script: Option<&'a str>,
    pub features: &'a [&'a str],
    pub variations: &'a [&'a str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineBreakKind {
    Allowed,
    Mandatory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineBreakOpportunity {
    /// UTF-8 byte offset immediately after the breakable segment.
    pub offset: u32,
    pub kind: LineBreakKind,
}

/// Returns the default UAX #14 break opportunities, including the mandatory end-of-text break.
pub fn line_breaks(
    text: &str,
    max_text_bytes: usize,
) -> Result<Vec<LineBreakOpportunity>, ShapeError> {
    if text.len() > max_text_bytes {
        return Err(ShapeError::new(
            ShapeErrorCode::TextLimitExceeded,
            "text bytes exceed the configured limit",
        ));
    }
    unicode_linebreak::linebreaks(text)
        .map(|(offset, opportunity)| {
            Ok(LineBreakOpportunity {
                offset: u32::try_from(offset).map_err(|_| {
                    ShapeError::new(
                        ShapeErrorCode::TextLimitExceeded,
                        "line-break offset cannot be encoded",
                    )
                })?,
                kind: match opportunity {
                    unicode_linebreak::BreakOpportunity::Allowed => LineBreakKind::Allowed,
                    unicode_linebreak::BreakOpportunity::Mandatory => LineBreakKind::Mandatory,
                },
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ShapeErrorCode {
    FontLimitExceeded,
    TextLimitExceeded,
    GlyphLimitExceeded,
    InvalidFaceIndex,
    InvalidFont,
    InvalidProperties,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShapeError {
    pub code: ShapeErrorCode,
    message: &'static str,
}

impl ShapeError {
    fn new(code: ShapeErrorCode, message: &'static str) -> Self {
        Self { code, message }
    }
}

impl fmt::Display for ShapeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for ShapeError {}

/// Shapes `text` with the selected face, returning unscaled font-unit positions and clusters.
pub fn shape(
    font_bytes: &[u8],
    text: &str,
    options: ShapeOptions,
) -> Result<ShapedRun, ShapeError> {
    shape_configured(font_bytes, text, options, ShapeProperties::default())
}

/// Shapes with optional OpenType language, script, feature, and variation settings.
pub fn shape_configured(
    font_bytes: &[u8],
    text: &str,
    options: ShapeOptions,
    properties: ShapeProperties<'_>,
) -> Result<ShapedRun, ShapeError> {
    validate_limits(font_bytes, text, options)?;
    let language = properties
        .language
        .map(rustybuzz::Language::from_str)
        .transpose()
        .map_err(|_| invalid_properties())?;
    let script = properties
        .script
        .map(rustybuzz::Script::from_str)
        .transpose()
        .map_err(|_| invalid_properties())?;
    let features = parse_shape_properties(properties.features, rustybuzz::Feature::from_str)?;
    let variations = parse_shape_properties(properties.variations, rustybuzz::Variation::from_str)?;
    let mut face = rustybuzz::Face::from_slice(font_bytes, options.face_index)
        .ok_or_else(|| ShapeError::new(ShapeErrorCode::InvalidFont, "font face is invalid"))?;
    face.set_variations(&variations);
    let units_per_em = u16::try_from(face.units_per_em())
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            ShapeError::new(ShapeErrorCode::InvalidFont, "font units per em are invalid")
        })?;
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.set_direction(match options.direction {
        Direction::LeftToRight => rustybuzz::Direction::LeftToRight,
        Direction::RightToLeft => rustybuzz::Direction::RightToLeft,
        Direction::TopToBottom => rustybuzz::Direction::TopToBottom,
        Direction::BottomToTop => rustybuzz::Direction::BottomToTop,
    });
    if let Some(language) = language {
        buffer.set_language(language);
    }
    if let Some(script) = script {
        buffer.set_script(script);
    }
    buffer.guess_segment_properties();
    let shaped = rustybuzz::shape(&face, &features, buffer);
    if shaped.len() > options.limits.max_glyphs {
        return Err(ShapeError::new(
            ShapeErrorCode::GlyphLimitExceeded,
            "shaped glyph count exceeds the configured limit",
        ));
    }
    let glyphs = shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .map(|(info, position)| ShapedGlyph {
            glyph_id: info.glyph_id,
            cluster: info.cluster,
            x_advance: position.x_advance,
            y_advance: position.y_advance,
            x_offset: position.x_offset,
            y_offset: position.y_offset,
            safe_to_break: !info.unsafe_to_break(),
        })
        .collect();
    Ok(ShapedRun {
        units_per_em,
        glyphs,
    })
}

fn parse_shape_properties<T>(
    values: &[&str],
    parse: impl Fn(&str) -> Result<T, &'static str>,
) -> Result<Vec<T>, ShapeError> {
    if values.len() > MAX_SHAPE_PROPERTIES
        || values
            .iter()
            .any(|value| value.is_empty() || value.len() > MAX_SHAPE_PROPERTY_BYTES)
    {
        return Err(invalid_properties());
    }
    values
        .iter()
        .map(|value| parse(value).map_err(|_| invalid_properties()))
        .collect()
}

fn invalid_properties() -> ShapeError {
    ShapeError::new(
        ShapeErrorCode::InvalidProperties,
        "font shaping properties are invalid or exceed their configured limits",
    )
}

fn validate_limits(font_bytes: &[u8], text: &str, options: ShapeOptions) -> Result<(), ShapeError> {
    if font_bytes.is_empty() || font_bytes.len() > options.limits.max_font_bytes {
        return Err(ShapeError::new(
            ShapeErrorCode::FontLimitExceeded,
            "font bytes exceed the configured limit",
        ));
    }
    if text.len() > options.limits.max_text_bytes {
        return Err(ShapeError::new(
            ShapeErrorCode::TextLimitExceeded,
            "text bytes exceed the configured limit",
        ));
    }
    if options.face_index > MAX_FACE_INDEX {
        return Err(ShapeError::new(
            ShapeErrorCode::InvalidFaceIndex,
            "font face index exceeds the configured limit",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_untrusted_inputs_before_font_parsing() {
        let tiny_limits = ShapeLimits {
            max_font_bytes: 2,
            max_text_bytes: 2,
            max_glyphs: 1,
        };
        assert_eq!(
            shape(
                &[0, 1, 2],
                "a",
                ShapeOptions {
                    limits: tiny_limits,
                    ..ShapeOptions::default()
                },
            )
            .unwrap_err()
            .code,
            ShapeErrorCode::FontLimitExceeded
        );
        assert_eq!(
            shape(
                &[0],
                "abc",
                ShapeOptions {
                    limits: tiny_limits,
                    ..ShapeOptions::default()
                },
            )
            .unwrap_err()
            .code,
            ShapeErrorCode::TextLimitExceeded
        );
        assert_eq!(
            shape(
                &[0],
                "a",
                ShapeOptions {
                    face_index: 64,
                    limits: tiny_limits,
                    ..ShapeOptions::default()
                },
            )
            .unwrap_err()
            .code,
            ShapeErrorCode::InvalidFaceIndex
        );
    }

    #[test]
    fn rejects_malformed_fonts_with_a_stable_error() {
        assert_eq!(
            shape(&[0, 1, 2, 3], "office", ShapeOptions::default())
                .unwrap_err()
                .code,
            ShapeErrorCode::InvalidFont
        );
    }

    #[test]
    fn rejects_untrusted_shape_properties_before_font_parsing() {
        assert_eq!(
            shape_configured(
                &[0, 1, 2, 3],
                "office",
                ShapeOptions::default(),
                ShapeProperties {
                    features: &["not a feature"],
                    ..ShapeProperties::default()
                },
            )
            .unwrap_err()
            .code,
            ShapeErrorCode::InvalidProperties
        );
    }

    #[test]
    fn computes_bounded_uax14_opportunities() {
        assert_eq!(
            line_breaks("a b\n日", 32).unwrap(),
            [
                LineBreakOpportunity {
                    offset: 2,
                    kind: LineBreakKind::Allowed,
                },
                LineBreakOpportunity {
                    offset: 4,
                    kind: LineBreakKind::Mandatory,
                },
                LineBreakOpportunity {
                    offset: 7,
                    kind: LineBreakKind::Mandatory,
                },
            ]
        );
        assert_eq!(
            line_breaks("abc", 2).unwrap_err().code,
            ShapeErrorCode::TextLimitExceeded
        );
    }
}
