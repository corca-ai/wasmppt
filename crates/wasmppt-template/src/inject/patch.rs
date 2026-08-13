use std::{collections::HashSet, ops::Range};

use wasmppt_xml::{TokenKind, XmlDocument, decode_entities};

use crate::{
    BindingTarget,
    policy::{is_template_main_type, prohibited_content_type, prohibited_relationship_type},
};

use super::{GenerateError, GenerateErrorCode};

/// A bounded replacement against the original, unmodified part bytes.
#[derive(Clone, Debug)]
pub(super) struct Patch {
    pub(super) range: Range<usize>,
    pub(super) replacement: Vec<u8>,
}

pub(super) fn relative_patches(
    patches: Vec<Patch>,
    offset: usize,
) -> Result<Vec<Patch>, GenerateError> {
    patches
        .into_iter()
        .map(|patch| {
            let start = patch.range.start.checked_sub(offset).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidBindingRange,
                    "patch precedes shape",
                )
            })?;
            let end = patch.range.end.checked_sub(offset).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidBindingRange,
                    "patch precedes shape",
                )
            })?;
            Ok(Patch {
                range: start..end,
                replacement: patch.replacement,
            })
        })
        .collect()
}

pub(super) fn relationship_part_name(source: &str) -> Option<String> {
    let (directory, file) = source.rsplit_once('/').unwrap_or(("", source));
    Some(if directory.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{directory}/_rels/{file}.rels")
    })
}

pub(super) fn relationship_source(name: &str) -> Option<String> {
    if name == "_rels/.rels" {
        return None;
    }
    let (directory, file) = name.rsplit_once("/_rels/")?;
    Some(format!("{directory}/{}", file.strip_suffix(".rels")?))
}

pub(super) fn resolve_target(source: Option<&str>, target: &str) -> Option<String> {
    let mut segments = Vec::new();
    if !target.starts_with('/') {
        if let Some((directory, _)) = source.and_then(|source| source.rsplit_once('/')) {
            segments.extend(
                directory
                    .split('/')
                    .filter(|part| !part.is_empty())
                    .map(str::to_owned),
            );
        }
    }
    for part in target.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            part if part.contains('\\') || part.contains('\0') => return None,
            part => segments.push(part.to_owned()),
        }
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

pub(super) fn missing_value(id: &str) -> GenerateError {
    GenerateError::new(
        GenerateErrorCode::MissingValue,
        format!("missing value for binding {id}"),
    )
}

pub(super) fn text_patches(
    binding: &BindingTarget,
    value: &str,
    source: &[u8],
) -> Result<Vec<Patch>, GenerateError> {
    let mut patches = Vec::new();
    for (index, span) in binding.text_spans.iter().enumerate() {
        let range = span.source_range.start as usize..span.source_range.end as usize;
        let raw = std::str::from_utf8(source.get(range.clone()).ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidBindingRange,
                "binding range is outside part",
            )
        })?)
        .map_err(|_| {
            GenerateError::new(
                GenerateErrorCode::InvalidBindingRange,
                "binding text is not UTF-8",
            )
        })?;
        let decoded = decode_entities(raw, range.start).map_err(GenerateError::xml)?;
        let start = span.decoded_start as usize;
        let end = span.decoded_end as usize;
        if start > end
            || !decoded.is_char_boundary(start)
            || !decoded.is_char_boundary(end)
            || end > decoded.len()
        {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidBindingRange,
                "binding offsets are invalid",
            ));
        }
        let mut replacement = String::new();
        replacement.push_str(&decoded[..start]);
        if index == 0 {
            replacement.push_str(value);
        }
        replacement.push_str(&decoded[end..]);
        patches.push(Patch {
            range,
            replacement: escape_xml(&replacement).into_bytes(),
        });
    }
    Ok(patches)
}

pub(super) fn cleanup_patches(
    name: &str,
    source: &[u8],
    removed: &HashSet<String>,
) -> Result<Vec<Patch>, GenerateError> {
    let document =
        XmlDocument::parse(source).map_err(|error| GenerateError::xml_in_part(error, name))?;
    let mut patches = Vec::new();
    for token in document.tokens() {
        let TokenKind::Start {
            name: element,
            attributes,
            empty,
        } = &token.kind
        else {
            continue;
        };
        if name == "[Content_Types].xml" && element.local == "Override" && *empty {
            let part = document
                .attribute(attributes, None, "PartName")
                .map(|attribute| attribute.value.trim_start_matches('/'));
            let content = document
                .attribute(attributes, None, "ContentType")
                .map(|attribute| attribute.value.as_str());
            if part.is_some_and(|part| removed.contains(part))
                || content.is_some_and(prohibited_content_type)
            {
                patches.push(Patch {
                    range: token.range.clone(),
                    replacement: Vec::new(),
                });
                continue;
            }
        }
        if name == "[Content_Types].xml" {
            if let Some(attribute) = document.attribute(attributes, None, "ContentType") {
                if is_template_main_type(&attribute.value) {
                    patches.push(Patch {
                        range: attribute.value_range.clone(),
                        replacement: b"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml".to_vec(),
                    });
                }
            }
        }
        if name.ends_with(".rels") && element.local == "Relationship" && *empty {
            let kind = document
                .attribute(attributes, None, "Type")
                .map(|attribute| attribute.value.as_str());
            if kind.is_some_and(prohibited_relationship_type) {
                patches.push(Patch {
                    range: token.range.clone(),
                    replacement: Vec::new(),
                });
                continue;
            }
        }
        for attribute in attributes {
            if attribute.name.local == "action"
                && attribute.value.to_ascii_lowercase().contains("macro")
            {
                patches.push(Patch {
                    range: attribute.range.clone(),
                    replacement: Vec::new(),
                });
            }
        }
    }
    Ok(patches)
}

pub(super) fn apply_patches(
    source: &[u8],
    mut patches: Vec<Patch>,
) -> Result<Vec<u8>, GenerateError> {
    patches.sort_unstable_by_key(|patch| std::cmp::Reverse(patch.range.start));
    let mut output = source.to_vec();
    let mut previous = source.len();
    for patch in patches {
        if patch.range.start > patch.range.end || patch.range.end > previous {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidBindingRange,
                "overlapping or invalid patches",
            ));
        }
        output.splice(patch.range.clone(), patch.replacement);
        previous = patch.range.start;
    }
    Ok(output)
}

fn escape_xml(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(character),
        }
    }
    output
}

pub(super) fn escape_xml_text(value: &str) -> String {
    escape_xml(value)
}

pub(super) fn escape_xml_attribute(value: &str) -> String {
    escape_xml(value)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_non_overlapping_patches_against_original_offsets() {
        let output = apply_patches(
            b"abcdef",
            vec![
                Patch {
                    range: 1..3,
                    replacement: b"X".to_vec(),
                },
                Patch {
                    range: 4..6,
                    replacement: b"YZ".to_vec(),
                },
            ],
        )
        .unwrap();
        assert_eq!(output, b"aXdYZ");
    }

    #[test]
    fn rejects_overlapping_patches() {
        let error = apply_patches(
            b"abcdef",
            vec![
                Patch {
                    range: 1..4,
                    replacement: Vec::new(),
                },
                Patch {
                    range: 3..5,
                    replacement: Vec::new(),
                },
            ],
        )
        .unwrap_err();
        assert_eq!(error.code(), GenerateErrorCode::InvalidBindingRange);
    }

    #[test]
    fn resolves_relationship_targets_without_escaping_the_package_root() {
        assert_eq!(
            resolve_target(Some("ppt/slides/slide1.xml"), "../media/image1.png"),
            Some("ppt/media/image1.png".to_owned())
        );
        assert_eq!(resolve_target(Some("ppt/slide.xml"), "../../escape"), None);
    }

    #[test]
    fn escapes_text_and_attribute_contexts() {
        assert_eq!(escape_xml_text("<&>\"'"), "&lt;&amp;&gt;\"'");
        assert_eq!(escape_xml_attribute("<&>\"'"), "&lt;&amp;&gt;&quot;&apos;");
    }
}
