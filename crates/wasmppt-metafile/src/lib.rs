//! Bounded EMF/WMF-to-SVG conversion shared by native and Wasm hosts.

use std::{error::Error, fmt};

/// Maximum compressed media-part bytes accepted by the preview converter.
pub const MAX_METAFILE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum generated SVG bytes returned across a host boundary.
pub const MAX_SVG_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetafileError {
    InputTooLarge { actual: usize, maximum: usize },
    Conversion(String),
    OutputTooLarge { actual: usize, maximum: usize },
    InvalidSvg,
}

impl fmt::Display for MetafileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "metafile is {actual} bytes; maximum is {maximum}"
                )
            }
            Self::Conversion(message) => write!(formatter, "cannot convert metafile: {message}"),
            Self::OutputTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "generated SVG is {actual} bytes; maximum is {maximum}"
                )
            }
            Self::InvalidSvg => formatter.write_str("metafile converter returned invalid SVG"),
        }
    }
}

impl Error for MetafileError {}

/// Convert EMF or WMF bytes to SVG without using filesystem, browser, or OS APIs.
pub fn convert_to_svg(input: &[u8]) -> Result<Vec<u8>, MetafileError> {
    if input.len() > MAX_METAFILE_BYTES {
        return Err(MetafileError::InputTooLarge {
            actual: input.len(),
            maximum: MAX_METAFILE_BYTES,
        });
    }
    let svg = emf_core::converter::convert_to_svg(input)
        .map_err(|error| MetafileError::Conversion(error.to_string()))?;
    let svg = add_intrinsic_dimensions(svg)?;
    if svg.len() > MAX_SVG_BYTES {
        return Err(MetafileError::OutputTooLarge {
            actual: svg.len(),
            maximum: MAX_SVG_BYTES,
        });
    }
    if !svg.starts_with(b"<svg ") || !svg.ends_with(b"</svg>") {
        return Err(MetafileError::InvalidSvg);
    }
    Ok(svg)
}

fn add_intrinsic_dimensions(svg: Vec<u8>) -> Result<Vec<u8>, MetafileError> {
    let text = std::str::from_utf8(&svg).map_err(|_| MetafileError::InvalidSvg)?;
    let view_box = text
        .strip_prefix("<svg viewBox=\"")
        .and_then(|rest| rest.split_once('"').map(|(value, _)| value))
        .ok_or(MetafileError::InvalidSvg)?;
    let mut values = view_box.split_ascii_whitespace();
    let _x = values.next().ok_or(MetafileError::InvalidSvg)?;
    let _y = values.next().ok_or(MetafileError::InvalidSvg)?;
    let width = values.next().ok_or(MetafileError::InvalidSvg)?;
    let height = values.next().ok_or(MetafileError::InvalidSvg)?;
    if values.next().is_some()
        || width
            .parse::<f64>()
            .ok()
            .filter(|value| *value > 0.0)
            .is_none()
        || height
            .parse::<f64>()
            .ok()
            .filter(|value| *value > 0.0)
            .is_none()
    {
        return Err(MetafileError::InvalidSvg);
    }
    let attributes = format!("<svg width=\"{width}\" height=\"{height}\" ");
    let mut output = Vec::with_capacity(svg.len() + attributes.len());
    output.extend_from_slice(attributes.as_bytes());
    output.extend_from_slice(&svg[b"<svg ".len()..]);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMR_EOF: u32 = 0x0000_000E;
    const EMR_RECTANGLE: u32 = 0x0000_002B;

    #[test]
    fn converts_emf_rectangle_to_svg() {
        let input = emf_binary(200, 200, &[record(EMR_RECTANGLE, &[10, 20, 150, 120])]);
        let svg = String::from_utf8(convert_to_svg(&input).unwrap()).unwrap();
        assert!(svg.contains("viewBox=\"0 0 200 200\""));
        assert!(svg.starts_with("<svg width=\"200\" height=\"200\" "));
        assert!(svg.contains("<rect "));
    }

    #[test]
    fn converts_wmf_rectangle_to_svg() {
        let input = wmf_binary();
        let svg = String::from_utf8(convert_to_svg(&input).unwrap()).unwrap();
        assert!(svg.contains("viewBox=\"0 0 200 200\""));
        assert!(svg.starts_with("<svg width=\"200\" height=\"200\" "));
        assert!(svg.contains("<rect "));
    }

    #[test]
    fn rejects_oversized_input_before_parsing() {
        let error = convert_to_svg(&vec![0; MAX_METAFILE_BYTES + 1]).unwrap_err();
        assert!(matches!(error, MetafileError::InputTooLarge { .. }));
    }

    fn emf_binary(width: i32, height: i32, records: &[Vec<u8>]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&1_u32.to_le_bytes());
        data.extend_from_slice(&88_u32.to_le_bytes());
        for value in [0_i32, 0, width, height, 0, 0, width, height] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        data.extend_from_slice(&0x464D_4520_u32.to_le_bytes());
        data.extend_from_slice(&0x0001_0000_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&1_u16.to_le_bytes());
        data.extend_from_slice(&0_u16.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        for value in [100_u32, 100, 100, 100] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        for item in records {
            data.extend_from_slice(item);
        }
        data.extend_from_slice(&record(EMR_EOF, &[0, 0]));
        let total = u32::try_from(data.len()).unwrap();
        data[48..52].copy_from_slice(&total.to_le_bytes());
        data
    }

    fn record(record_type: u32, params: &[i32]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&record_type.to_le_bytes());
        data.extend_from_slice(&u32::try_from(8 + params.len() * 4).unwrap().to_le_bytes());
        for param in params {
            data.extend_from_slice(&param.to_le_bytes());
        }
        data
    }

    fn wmf_binary() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&1_u16.to_le_bytes());
        data.extend_from_slice(&9_u16.to_le_bytes());
        data.extend_from_slice(&0x0300_u16.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0_u16.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0_u16.to_le_bytes());
        for (function, params) in [
            (0x020C_u16, &[200_i16, 200_i16][..]),
            (0x041B_u16, &[120_i16, 150_i16, 20_i16, 10_i16][..]),
            (0_u16, &[][..]),
        ] {
            data.extend_from_slice(&u32::try_from(3 + params.len()).unwrap().to_le_bytes());
            data.extend_from_slice(&function.to_le_bytes());
            for param in params {
                data.extend_from_slice(&param.to_le_bytes());
            }
        }
        data
    }
}
