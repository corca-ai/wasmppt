use std::{collections::BTreeMap, ops::Range};

use wasmppt_xml::{TokenKind, XmlDocument};

#[derive(Clone, Debug)]
pub(crate) struct Element {
    pub(crate) local: String,
    pub(crate) namespace: Option<String>,
    pub(crate) attributes: BTreeMap<String, String>,
    pub(crate) range: Range<usize>,
    pub(crate) depth: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Elements {
    values: Vec<Element>,
}

impl Elements {
    pub(crate) fn parse(source: Vec<u8>) -> Result<Self, wasmppt_xml::XmlError> {
        let document = XmlDocument::parse(source)?;
        let mut values = Vec::<Element>::new();
        let mut open = Vec::<usize>::new();
        for token in document.tokens() {
            match &token.kind {
                TokenKind::Start {
                    name,
                    attributes,
                    empty,
                } => {
                    let index = values.len();
                    values.push(Element {
                        local: name.local.clone(),
                        namespace: name
                            .namespace
                            .map(|namespace| document.namespace(namespace).to_owned()),
                        attributes: attributes
                            .iter()
                            .map(|attribute| {
                                (attribute.name.local.clone(), attribute.value.clone())
                            })
                            .collect(),
                        range: token.range.clone(),
                        depth: token.depth,
                    });
                    if !empty {
                        open.push(index);
                    }
                }
                TokenKind::End { .. } => {
                    if let Some(index) = open.pop() {
                        values[index].range.end = token.range.end;
                    }
                }
                _ => {}
            }
        }
        Ok(Self { values })
    }

    pub(crate) fn root(&self) -> Option<&Element> {
        self.values.first()
    }

    pub(crate) fn values(&self) -> &[Element] {
        &self.values
    }

    pub(crate) fn descendants<'a>(
        &'a self,
        parent: &'a Element,
    ) -> impl Iterator<Item = &'a Element> {
        self.values.iter().filter(move |element| {
            element.depth > parent.depth
                && element.range.start >= parent.range.start
                && element.range.end <= parent.range.end
        })
    }

    pub(crate) fn first_descendant<'a>(
        &'a self,
        parent: &'a Element,
        local: &str,
    ) -> Option<&'a Element> {
        self.descendants(parent)
            .find(|element| element.local == local)
    }
}

pub(crate) fn attr<'a>(element: &'a Element, name: &str) -> Option<&'a str> {
    element.attributes.get(name).map(String::as_str)
}
