//! Loss-aware typed views over the PresentationML subset used by template injection.

use std::{ops::Range, sync::Arc};

use wasmppt_xml::{TokenKind, XmlDocument, decode_entities};

const PML_TRANSITIONAL: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const PML_STRICT: &str = "http://purl.oclc.org/ooxml/presentationml/main";
const DML_TRANSITIONAL: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const DML_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/main";
const REL_TRANSITIONAL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PmlErrorCode {
    Xml,
    UnexpectedRoot,
    InvalidId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PmlError {
    code: PmlErrorCode,
    message: String,
}

impl PmlError {
    fn new(code: PmlErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> PmlErrorCode {
        self.code
    }
}

impl std::fmt::Display for PmlError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PmlError {}

pub type Result<T> = std::result::Result<T, PmlError>;

#[derive(Clone, Debug)]
pub struct PresentationView {
    document: XmlDocument,
    slide_relationship_ids: Vec<String>,
}

impl PresentationView {
    pub fn parse(bytes: impl Into<Arc<[u8]>>) -> Result<Self> {
        let document = XmlDocument::parse(bytes)
            .map_err(|error| PmlError::new(PmlErrorCode::Xml, error.to_string()))?;
        ensure_root(&document, "presentation", &[PML_TRANSITIONAL, PML_STRICT])?;
        let mut slide_relationship_ids = Vec::new();
        for token in document.tokens() {
            let TokenKind::Start {
                name, attributes, ..
            } = &token.kind
            else {
                continue;
            };
            if name.local != "sldId"
                || !namespace_is(&document, name.namespace, &[PML_TRANSITIONAL, PML_STRICT])
            {
                continue;
            }
            if let Some(attribute) = attributes.iter().find(|attribute| {
                attribute.name.local == "id"
                    && namespace_is(
                        &document,
                        attribute.name.namespace,
                        &[REL_TRANSITIONAL, REL_STRICT],
                    )
            }) {
                slide_relationship_ids.push(attribute.value.clone());
            }
        }
        Ok(Self {
            document,
            slide_relationship_ids,
        })
    }

    pub fn document(&self) -> &XmlDocument {
        &self.document
    }

    pub fn slide_relationship_ids(&self) -> &[String] {
        &self.slide_relationship_ids
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextRun {
    pub text: String,
    /// Exact byte range of the text content, excluding `<a:t>` tags.
    pub source_range: Range<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShapeView {
    pub id: Option<u32>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub text_runs: Vec<TextRun>,
}

#[derive(Clone, Debug)]
pub struct SlideView {
    document: XmlDocument,
    shapes: Vec<ShapeView>,
}

impl SlideView {
    pub fn parse(bytes: impl Into<Arc<[u8]>>) -> Result<Self> {
        let document = XmlDocument::parse(bytes)
            .map_err(|error| PmlError::new(PmlErrorCode::Xml, error.to_string()))?;
        ensure_root(&document, "sld", &[PML_TRANSITIONAL, PML_STRICT])?;
        let mut shapes = Vec::new();
        let mut shape: Option<(usize, ShapeView)> = None;
        let mut text_depth = None;
        for token in document.tokens() {
            match &token.kind {
                TokenKind::Start {
                    name,
                    attributes,
                    empty,
                } => {
                    if name.local == "sp"
                        && namespace_is(&document, name.namespace, &[PML_TRANSITIONAL, PML_STRICT])
                    {
                        shape = Some((
                            token.depth,
                            ShapeView {
                                id: None,
                                name: None,
                                description: None,
                                text_runs: Vec::new(),
                            },
                        ));
                    } else if name.local == "cNvPr" {
                        if let Some((_, current)) = &mut shape {
                            current.id = plain_attribute(attributes, "id")
                                .map(|value| {
                                    value.parse::<u32>().map_err(|_| {
                                        PmlError::new(
                                            PmlErrorCode::InvalidId,
                                            format!("invalid cNvPr id {value:?}"),
                                        )
                                    })
                                })
                                .transpose()?;
                            current.name = plain_attribute(attributes, "name").map(str::to_owned);
                            current.description = plain_attribute(attributes, "descr")
                                .or_else(|| plain_attribute(attributes, "title"))
                                .map(str::to_owned);
                        }
                    } else if name.local == "t"
                        && namespace_is(&document, name.namespace, &[DML_TRANSITIONAL, DML_STRICT])
                        && shape.is_some()
                        && !empty
                    {
                        text_depth = Some(token.depth);
                    }
                }
                TokenKind::Text | TokenKind::Cdata if text_depth.is_some() => {
                    if let Some((_, current)) = &mut shape {
                        let source_range = if matches!(&token.kind, TokenKind::Cdata) {
                            token.range.start + 9..token.range.end - 3
                        } else {
                            token.range.clone()
                        };
                        let raw = std::str::from_utf8(document.source_range(source_range.clone()))
                            .expect("XML source was validated as UTF-8");
                        let text = if matches!(&token.kind, TokenKind::Cdata) {
                            raw.to_owned()
                        } else {
                            decode_entities(raw, token.range.start).map_err(|error| {
                                PmlError::new(PmlErrorCode::Xml, error.to_string())
                            })?
                        };
                        current.text_runs.push(TextRun { text, source_range });
                    }
                }
                TokenKind::End { name } => {
                    if name.local == "t"
                        && namespace_is(&document, name.namespace, &[DML_TRANSITIONAL, DML_STRICT])
                    {
                        text_depth = None;
                    }
                    if let Some((depth, _)) = &shape {
                        if token.depth == *depth
                            && name.local == "sp"
                            && namespace_is(
                                &document,
                                name.namespace,
                                &[PML_TRANSITIONAL, PML_STRICT],
                            )
                        {
                            shapes.push(shape.take().expect("shape exists").1);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(Self { document, shapes })
    }

    pub fn document(&self) -> &XmlDocument {
        &self.document
    }

    pub fn shapes(&self) -> &[ShapeView] {
        &self.shapes
    }
}

fn ensure_root(document: &XmlDocument, local: &str, namespaces: &[&str]) -> Result<()> {
    let matches = document.tokens().iter().find_map(|token| {
        let TokenKind::Start { name, .. } = &token.kind else {
            return None;
        };
        Some(name.local == local && namespace_is(document, name.namespace, namespaces))
    });
    if matches == Some(true) {
        Ok(())
    } else {
        Err(PmlError::new(
            PmlErrorCode::UnexpectedRoot,
            format!("expected PresentationML root {local}"),
        ))
    }
}

fn namespace_is(
    document: &XmlDocument,
    symbol: Option<wasmppt_xml::Symbol>,
    expected: &[&str],
) -> bool {
    symbol.is_some_and(|symbol| expected.contains(&document.namespace(symbol)))
}

fn plain_attribute<'a>(attributes: &'a [wasmppt_xml::Attribute], local: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|attribute| attribute.name.namespace.is_none() && attribute.name.local == local)
        .map(|attribute| attribute.value.as_str())
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
