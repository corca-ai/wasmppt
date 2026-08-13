use std::{collections::BTreeSet, ops::Range};

use wasmppt_xml::{TokenKind, XmlDocument};

use crate::{ComposeError, ComposeErrorCode, xml_attr};

const PPTX_MAIN_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";
const SLIDE_TYPE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
const OFFICE_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

#[derive(Debug)]
struct Patch {
    range: Range<usize>,
    replacement: Vec<u8>,
}

pub(crate) fn patch_content_types(
    source: Vec<u8>,
    slide_parts: &[String],
    generated_parts: &[(String, &'static str)],
    removed_parts: &BTreeSet<String>,
) -> Result<Vec<u8>, ComposeError> {
    let document = parse(source, "[Content_Types].xml")?;
    let mut patches = Vec::new();
    let generated = generated_parts
        .iter()
        .map(|(name, _)| format!("/{name}"))
        .collect::<BTreeSet<_>>();
    let mut patched_main_type = false;
    for (index, token) in document.tokens().iter().enumerate() {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        if name.local != "Override" {
            continue;
        }
        let part_name = document
            .attribute(attributes, None, "PartName")
            .map(|attribute| attribute.value.as_str());
        if part_name == Some("/ppt/presentation.xml") {
            patched_main_type = true;
            let content_type = document
                .attribute(attributes, None, "ContentType")
                .ok_or_else(|| invalid_package("presentation content type has no value"))?;
            patches.push(Patch {
                range: content_type.value_range.clone(),
                replacement: PPTX_MAIN_TYPE.as_bytes().to_vec(),
            });
        } else if part_name.is_some_and(|name| {
            name.starts_with("/ppt/slides/")
                || generated.contains(name)
                || removed_parts.contains(name.trim_start_matches('/'))
        }) {
            patches.push(Patch {
                range: element_range(&document, index)?,
                replacement: Vec::new(),
            });
        }
    }
    if !patched_main_type {
        return Err(invalid_package(
            "content types have no presentation main-part override",
        ));
    }

    let mut addition = String::new();
    for part in slide_parts {
        addition.push_str(&format!(
            "<Override PartName=\"/{}\" ContentType=\"{SLIDE_TYPE}\"/>",
            xml_attr(part)
        ));
    }
    for (part, content_type) in generated_parts {
        addition.push_str(&format!(
            "<Override PartName=\"/{}\" ContentType=\"{}\"/>",
            xml_attr(part),
            xml_attr(content_type)
        ));
    }
    patches.push(Patch {
        range: root_end(&document)?.start..root_end(&document)?.start,
        replacement: addition.into_bytes(),
    });
    apply(document.source(), patches)
}

pub(crate) fn patch_presentation_relationships(
    source: Vec<u8>,
    slide_parts: &[String],
) -> Result<(Vec<u8>, Vec<String>), ComposeError> {
    let document = parse(source, "ppt/_rels/presentation.xml.rels")?;
    let mut patches = Vec::new();
    let mut used = BTreeSet::new();
    for (index, token) in document.tokens().iter().enumerate() {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        if name.local != "Relationship" {
            continue;
        }
        if let Some(id) = document.attribute(attributes, None, "Id") {
            used.insert(id.value.clone());
        }
        if document
            .attribute(attributes, None, "Type")
            .is_some_and(|attribute| attribute.value.ends_with("/slide"))
        {
            patches.push(Patch {
                range: element_range(&document, index)?,
                replacement: Vec::new(),
            });
        }
    }
    let mut ids = Vec::with_capacity(slide_parts.len());
    let mut addition = String::new();
    for (index, part) in slide_parts.iter().enumerate() {
        let mut ordinal = index + 1;
        let id = loop {
            let candidate = format!("rIdDeck{ordinal}");
            if used.insert(candidate.clone()) {
                break candidate;
            }
            ordinal += slide_parts.len().max(1);
        };
        let target = part.strip_prefix("ppt/").unwrap_or(part);
        addition.push_str(&format!(
            "<Relationship Id=\"{}\" Type=\"{OFFICE_REL}/slide\" Target=\"{}\"/>",
            xml_attr(&id),
            xml_attr(target)
        ));
        ids.push(id);
    }
    let end = root_end(&document)?;
    patches.push(Patch {
        range: end.start..end.start,
        replacement: addition.into_bytes(),
    });
    Ok((apply(document.source(), patches)?, ids))
}

pub(crate) fn patch_presentation(
    source: Vec<u8>,
    slide_relationship_ids: &[String],
) -> Result<Vec<u8>, ComposeError> {
    let document = parse(source, "ppt/presentation.xml")?;
    let root = document
        .tokens()
        .iter()
        .find_map(|token| match &token.kind {
            TokenKind::Start { name, .. } if token.depth == 0 => Some(name),
            _ => None,
        })
        .ok_or_else(|| invalid_package("presentation XML has no root"))?;
    let prefix = root.prefix.as_deref().unwrap_or("p");
    let mut list = format!(
        "<{prefix}:sldIdLst xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">"
    );
    for (index, relationship_id) in slide_relationship_ids.iter().enumerate() {
        let id = 256u64
            .checked_add(index as u64)
            .ok_or_else(|| invalid_package("slide identifier overflow"))?;
        list.push_str(&format!(
            "<{prefix}:sldId id=\"{id}\" r:id=\"{}\"/>",
            xml_attr(relationship_id)
        ));
    }
    list.push_str(&format!("</{prefix}:sldIdLst>"));

    for (index, token) in document.tokens().iter().enumerate() {
        if matches!(&token.kind, TokenKind::Start { name, .. } if name.local == "sldIdLst") {
            return apply(
                document.source(),
                vec![Patch {
                    range: element_range(&document, index)?,
                    replacement: list.into_bytes(),
                }],
            );
        }
    }
    let insertion = document
        .tokens()
        .iter()
        .find(|token| {
            matches!(&token.kind, TokenKind::Start { name, .. } if token.depth == 1 && name.local == "sldSz")
        })
        .map_or(root_end(&document)?.start, |token| token.range.start);
    apply(
        document.source(),
        vec![Patch {
            range: insertion..insertion,
            replacement: list.into_bytes(),
        }],
    )
}

fn parse(source: Vec<u8>, part: &str) -> Result<XmlDocument, ComposeError> {
    XmlDocument::parse(source)
        .map_err(|error| invalid_package(format!("cannot parse {part}: {error}")))
}

fn root_end(document: &XmlDocument) -> Result<Range<usize>, ComposeError> {
    document
        .tokens()
        .iter()
        .rev()
        .find(|token| matches!(token.kind, TokenKind::End { .. }) && token.depth == 0)
        .map(|token| token.range.clone())
        .ok_or_else(|| invalid_package("XML root has no closing tag"))
}

fn element_range(document: &XmlDocument, start: usize) -> Result<Range<usize>, ComposeError> {
    let token = &document.tokens()[start];
    let TokenKind::Start { name, empty, .. } = &token.kind else {
        return Err(invalid_package(
            "element range does not start at an element",
        ));
    };
    if *empty {
        return Ok(token.range.clone());
    }
    document.tokens()[start + 1..]
        .iter()
        .find(|candidate| {
            candidate.depth == token.depth
                && matches!(&candidate.kind, TokenKind::End { name: end } if end.local == name.local && end.namespace == name.namespace)
        })
        .map(|end| token.range.start..end.range.end)
        .ok_or_else(|| invalid_package("element has no matching closing tag"))
}

fn apply(source: &[u8], mut patches: Vec<Patch>) -> Result<Vec<u8>, ComposeError> {
    patches.sort_by_key(|patch| patch.range.start);
    let mut cursor = 0;
    let mut output = Vec::with_capacity(source.len());
    for patch in patches {
        if patch.range.start < cursor
            || patch.range.end < patch.range.start
            || patch.range.end > source.len()
        {
            return Err(invalid_package("overlapping or invalid XML patch"));
        }
        output.extend_from_slice(&source[cursor..patch.range.start]);
        output.extend_from_slice(&patch.replacement);
        cursor = patch.range.end;
    }
    output.extend_from_slice(&source[cursor..]);
    Ok(output)
}

fn invalid_package(message: impl Into<String>) -> ComposeError {
    ComposeError::new(ComposeErrorCode::InvalidPackage, message)
}
