use std::collections::BTreeMap;

use wasmppt_xml::{Attribute, TokenKind, XmlDocument};

use crate::RgbaColor;

pub(super) const WHITE: RgbaColor = RgbaColor {
    red: 255,
    green: 255,
    blue: 255,
    alpha: 255,
};
pub(super) const BLACK: RgbaColor = RgbaColor {
    red: 0,
    green: 0,
    blue: 0,
    alpha: 255,
};

#[derive(Clone, Debug)]
pub(super) struct Theme {
    pub(super) colors: BTreeMap<String, RgbaColor>,
    pub(super) mapping: BTreeMap<String, String>,
    pub(super) major_latin: String,
    pub(super) minor_latin: String,
    pub(super) major_east_asian: String,
    pub(super) minor_east_asian: String,
    pub(super) major_complex_script: String,
    pub(super) minor_complex_script: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            colors: BTreeMap::from([
                ("dk1".to_owned(), BLACK),
                ("lt1".to_owned(), WHITE),
                (
                    "accent1".to_owned(),
                    RgbaColor {
                        red: 68,
                        green: 114,
                        blue: 196,
                        alpha: 255,
                    },
                ),
            ]),
            mapping: BTreeMap::from([
                ("bg1".to_owned(), "lt1".to_owned()),
                ("tx1".to_owned(), "dk1".to_owned()),
                ("bg2".to_owned(), "lt2".to_owned()),
                ("tx2".to_owned(), "dk2".to_owned()),
            ]),
            major_latin: "Arial".to_owned(),
            minor_latin: "Arial".to_owned(),
            major_east_asian: "Arial".to_owned(),
            minor_east_asian: "Arial".to_owned(),
            major_complex_script: "Arial".to_owned(),
            minor_complex_script: "Arial".to_owned(),
        }
    }
}

pub(super) fn parse_theme(document: &XmlDocument) -> Result<Theme, String> {
    let mut theme = Theme::default();
    let mut scheme_depth = None;
    let mut slot: Option<(usize, String)> = None;
    for token in document.tokens() {
        match &token.kind {
            TokenKind::Start { name, .. } if name.local == "clrScheme" => {
                scheme_depth = Some(token.depth)
            }
            TokenKind::Start {
                name, attributes, ..
            } if scheme_depth.is_some_and(|depth| token.depth == depth + 1) => {
                slot = Some((token.depth, name.local.clone()));
                if let Some(value) =
                    color_attribute(name.local.as_str(), attributes).and_then(parse_hex_color)
                {
                    theme.colors.insert(name.local.clone(), value);
                }
            }
            TokenKind::Start {
                name, attributes, ..
            } if slot.is_some() && matches!(name.local.as_str(), "srgbClr" | "sysClr") => {
                let value = if name.local == "sysClr" {
                    plain(attributes, "lastClr").or_else(|| plain(attributes, "val"))
                } else {
                    plain(attributes, "val")
                };
                if let (Some((_, slot_name)), Some(color)) =
                    (&slot, value.and_then(parse_hex_color))
                {
                    theme.colors.insert(slot_name.clone(), color);
                }
            }
            TokenKind::End { name } if name.local == "clrScheme" => scheme_depth = None,
            TokenKind::End { .. }
                if slot
                    .as_ref()
                    .is_some_and(|(depth, _)| *depth == token.depth) =>
            {
                slot = None
            }
            _ => {}
        }
    }
    let mut font_group: Option<(usize, bool)> = None;
    for token in document.tokens() {
        match &token.kind {
            TokenKind::Start { name, .. } if name.local == "majorFont" => {
                font_group = Some((token.depth, true));
            }
            TokenKind::Start { name, .. } if name.local == "minorFont" => {
                font_group = Some((token.depth, false));
            }
            TokenKind::Start {
                name, attributes, ..
            } if matches!(name.local.as_str(), "latin" | "ea" | "cs")
                && font_group.is_some_and(|(depth, _)| token.depth == depth + 1) =>
            {
                if let (Some((_, major)), Some(family)) =
                    (font_group, plain(attributes, "typeface"))
                {
                    match (major, name.local.as_str()) {
                        (true, "latin") => theme.major_latin = family.to_owned(),
                        (false, "latin") => theme.minor_latin = family.to_owned(),
                        (true, "ea") => theme.major_east_asian = family.to_owned(),
                        (false, "ea") => theme.minor_east_asian = family.to_owned(),
                        (true, "cs") => theme.major_complex_script = family.to_owned(),
                        (false, "cs") => theme.minor_complex_script = family.to_owned(),
                        _ => {}
                    }
                }
            }
            TokenKind::End { name } if matches!(name.local.as_str(), "majorFont" | "minorFont") => {
                font_group = None;
            }
            _ => {}
        }
    }
    Ok(theme)
}

pub(super) fn apply_color_map(document: &XmlDocument, theme: &mut Theme) {
    for token in document.tokens() {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        if matches!(name.local.as_str(), "clrMap" | "overrideClrMapping") {
            for attribute in attributes
                .iter()
                .filter(|attribute| attribute.name.namespace.is_none())
            {
                theme
                    .mapping
                    .insert(attribute.name.local.clone(), attribute.value.clone());
            }
        }
    }
}

pub(super) fn parse_background(document: &XmlDocument, theme: &Theme) -> Option<RgbaColor> {
    let background = document.tokens().iter().position(
        |token| matches!(&token.kind, TokenKind::Start { name, .. } if name.local == "bg"),
    )?;
    let end = element_end(document, background)?;
    let fill = (background..=end).find(|index| {
        matches!(&document.tokens()[*index].kind, TokenKind::Start { name, .. } if matches!(name.local.as_str(), "solidFill" | "bgRef"))
    })?;
    parse_color(
        document,
        fill,
        element_end(document, fill).unwrap_or(fill).min(end),
        theme,
    )
}

pub(super) fn parse_color(
    document: &XmlDocument,
    start: usize,
    end: usize,
    theme: &Theme,
) -> Option<RgbaColor> {
    let mut color = None;
    let mut transforms = Vec::<(String, i32)>::new();
    for token in &document.tokens()[start..=end] {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        match name.local.as_str() {
            "srgbClr" => color = plain(attributes, "val").and_then(parse_hex_color),
            "sysClr" => {
                color = plain(attributes, "lastClr")
                    .or_else(|| plain(attributes, "val"))
                    .and_then(parse_hex_color)
            }
            "schemeClr" => {
                if let Some(slot) = plain(attributes, "val") {
                    let mapped = theme.mapping.get(slot).map_or(slot, String::as_str);
                    color = theme.colors.get(mapped).copied();
                }
            }
            "tint" | "shade" | "lumMod" | "lumOff" | "satMod" | "satOff" | "hueMod" | "hueOff"
            | "alpha" | "alphaMod" | "alphaOff" | "redMod" | "redOff" | "greenMod" | "greenOff"
            | "blueMod" | "blueOff" => {
                if let Some(value) = plain_i32(attributes, "val") {
                    transforms.push((name.local.clone(), value));
                }
            }
            "comp" | "inv" | "gray" => transforms.push((name.local.clone(), 0)),
            _ => {}
        }
    }
    let mut color = color?;
    for (kind, value) in transforms {
        color = apply_color_transform(color, &kind, value);
    }
    Some(color)
}

pub(super) fn apply_color_transform(mut color: RgbaColor, kind: &str, value: i32) -> RgbaColor {
    let scale = value.clamp(0, 100_000) as i64;
    let channel = |component: u8, operation: &str| -> u8 {
        let component = component as i64;
        let output = match operation {
            "tint" => component + (255 - component) * scale / 100_000,
            "shade" => component * scale / 100_000,
            _ => component,
        };
        output.clamp(0, 255) as u8
    };
    match kind {
        "alpha" => color.alpha = ((255_i64 * scale) / 100_000) as u8,
        "alphaMod" => color.alpha = ((i64::from(color.alpha) * scale) / 100_000) as u8,
        "alphaOff" => {
            color.alpha =
                (i64::from(color.alpha) + 255 * value as i64 / 100_000).clamp(0, 255) as u8;
        }
        "inv" | "comp" => {
            color.red = 255 - color.red;
            color.green = 255 - color.green;
            color.blue = 255 - color.blue;
        }
        "gray" => {
            let gray = (u32::from(color.red) * 21
                + u32::from(color.green) * 72
                + u32::from(color.blue) * 7)
                / 100;
            color.red = gray as u8;
            color.green = gray as u8;
            color.blue = gray as u8;
        }
        "redMod" => color.red = channel(color.red, "shade"),
        "greenMod" => color.green = channel(color.green, "shade"),
        "blueMod" => color.blue = channel(color.blue, "shade"),
        "redOff" => color.red = offset_channel(color.red, value),
        "greenOff" => color.green = offset_channel(color.green, value),
        "blueOff" => color.blue = offset_channel(color.blue, value),
        "lumMod" | "lumOff" | "satMod" | "satOff" | "hueMod" | "hueOff" => {
            let (mut hue, mut saturation, mut lightness) = rgb_to_hsl(color);
            match kind {
                "lumMod" => lightness *= value as f64 / 100_000.0,
                "lumOff" => lightness += value as f64 / 100_000.0,
                "satMod" => saturation *= value as f64 / 100_000.0,
                "satOff" => saturation += value as f64 / 100_000.0,
                "hueMod" => hue *= value as f64 / 100_000.0,
                "hueOff" => hue += value as f64 / 60_000.0,
                _ => {}
            }
            let alpha = color.alpha;
            color = hsl_to_rgb(hue, saturation.clamp(0.0, 1.0), lightness.clamp(0.0, 1.0));
            color.alpha = alpha;
        }
        _ => {
            color.red = channel(color.red, kind);
            color.green = channel(color.green, kind);
            color.blue = channel(color.blue, kind);
        }
    }
    color
}

fn offset_channel(channel: u8, value: i32) -> u8 {
    (i64::from(channel) + 255 * value as i64 / 100_000).clamp(0, 255) as u8
}

fn rgb_to_hsl(color: RgbaColor) -> (f64, f64, f64) {
    let red = f64::from(color.red) / 255.0;
    let green = f64::from(color.green) / 255.0;
    let blue = f64::from(color.blue) / 255.0;
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let lightness = (maximum + minimum) / 2.0;
    if (maximum - minimum).abs() < f64::EPSILON {
        return (0.0, 0.0, lightness);
    }
    let delta = maximum - minimum;
    let saturation = delta / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if maximum == red {
        60.0 * ((green - blue) / delta).rem_euclid(6.0)
    } else if maximum == green {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };
    (hue, saturation, lightness)
}

fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> RgbaColor {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue.rem_euclid(360.0) / 60.0;
    let secondary = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match sector as u8 {
        0 => (chroma, secondary, 0.0),
        1 => (secondary, chroma, 0.0),
        2 => (0.0, chroma, secondary),
        3 => (0.0, secondary, chroma),
        4 => (secondary, 0.0, chroma),
        _ => (chroma, 0.0, secondary),
    };
    let match_value = lightness - chroma / 2.0;
    RgbaColor {
        red: ((red + match_value) * 255.0).round().clamp(0.0, 255.0) as u8,
        green: ((green + match_value) * 255.0).round().clamp(0.0, 255.0) as u8,
        blue: ((blue + match_value) * 255.0).round().clamp(0.0, 255.0) as u8,
        alpha: 255,
    }
}

fn element_end(document: &XmlDocument, start: usize) -> Option<usize> {
    let token = document.tokens().get(start)?;
    let TokenKind::Start { name, empty, .. } = &token.kind else {
        return None;
    };
    if *empty {
        return Some(start);
    }
    document
        .tokens()
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(index, candidate)| {
            matches!(&candidate.kind, TokenKind::End { name: end_name }
                if candidate.depth == token.depth && end_name.local == name.local)
            .then_some(index)
        })
}

fn color_attribute<'a>(local: &str, attributes: &'a [Attribute]) -> Option<&'a str> {
    matches!(local, "srgbClr" | "sysClr")
        .then(|| plain(attributes, "val"))
        .flatten()
}

pub(super) fn parse_hex_color(value: &str) -> Option<RgbaColor> {
    if value.len() != 6 {
        return None;
    }
    Some(RgbaColor {
        red: u8::from_str_radix(&value[0..2], 16).ok()?,
        green: u8::from_str_radix(&value[2..4], 16).ok()?,
        blue: u8::from_str_radix(&value[4..6], 16).ok()?,
        alpha: 255,
    })
}

fn plain<'a>(attributes: &'a [Attribute], local: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|attribute| attribute.name.namespace.is_none() && attribute.name.local == local)
        .map(|attribute| attribute.value.as_str())
}

fn plain_i32(attributes: &[Attribute], local: &str) -> Option<i32> {
    plain(attributes, local)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_theme_slots_and_font_groups() {
        let document = XmlDocument::parse(
            &br#"<a:theme xmlns:a="urn:a"><a:themeElements><a:clrScheme><a:dk1><a:srgbClr val="112233"/></a:dk1></a:clrScheme><a:fontScheme><a:majorFont><a:latin typeface="Major"/></a:majorFont><a:minorFont><a:latin typeface="Minor"/></a:minorFont></a:fontScheme></a:themeElements></a:theme>"#[..],
        )
        .unwrap();
        let theme = parse_theme(&document).unwrap();
        assert_eq!(theme.colors["dk1"], parse_hex_color("112233").unwrap());
        assert_eq!(theme.major_latin, "Major");
        assert_eq!(theme.minor_latin, "Minor");
    }

    #[test]
    fn applies_color_transforms_in_document_order() {
        let document = XmlDocument::parse(
            &br#"<a:solidFill xmlns:a="urn:a"><a:srgbClr val="204060"><a:tint val="50000"/><a:alpha val="25000"/></a:srgbClr></a:solidFill>"#[..],
        )
        .unwrap();
        let color =
            parse_color(&document, 0, document.tokens().len() - 1, &Theme::default()).unwrap();
        assert_eq!(color.red, 143);
        assert_eq!(color.green, 159);
        assert_eq!(color.blue, 175);
        assert_eq!(color.alpha, 63);
    }
}
