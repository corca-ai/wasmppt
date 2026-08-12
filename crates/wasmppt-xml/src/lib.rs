//! Loss-aware, namespace-aware XML tokens with exact source byte ranges.

use std::{collections::HashMap, ops::Range, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Symbol(u32);

#[derive(Clone, Debug, Default)]
pub struct Interner {
    values: Vec<String>,
    ids: HashMap<String, Symbol>,
}

impl Interner {
    pub fn intern(&mut self, value: &str) -> Symbol {
        if let Some(symbol) = self.ids.get(value) {
            return *symbol;
        }
        let symbol = Symbol(u32::try_from(self.values.len()).expect("symbol table exceeds u32"));
        let owned = value.to_owned();
        self.values.push(owned.clone());
        self.ids.insert(owned, symbol);
        symbol
    }

    pub fn resolve(&self, symbol: Symbol) -> &str {
        &self.values[symbol.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum XmlErrorCode {
    InvalidUtf8,
    Truncated,
    InvalidSyntax,
    UndeclaredPrefix,
    MismatchedEndTag,
    DtdForbidden,
    Entity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlError {
    code: XmlErrorCode,
    offset: usize,
    message: String,
}

impl XmlError {
    fn new(code: XmlErrorCode, offset: usize, message: impl Into<String>) -> Self {
        Self {
            code,
            offset,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> XmlErrorCode {
        self.code
    }

    pub const fn offset(&self) -> usize {
        self.offset
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} at XML byte {}", self.message, self.offset)
    }
}

impl std::error::Error for XmlError {}

pub type Result<T> = std::result::Result<T, XmlError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedName {
    pub namespace: Option<Symbol>,
    pub prefix: Option<String>,
    pub local: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attribute {
    pub name: ExpandedName,
    pub value: String,
    pub range: Range<usize>,
    pub value_range: Range<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Start {
        name: ExpandedName,
        attributes: Vec<Attribute>,
        empty: bool,
    },
    End {
        name: ExpandedName,
    },
    Text,
    Cdata,
    Comment,
    ProcessingInstruction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub range: Range<usize>,
    pub depth: usize,
}

/// Parsed tokens retain the complete source so unsupported markup can be copied exactly.
#[derive(Clone, Debug)]
pub struct XmlDocument {
    source: Arc<[u8]>,
    tokens: Vec<Token>,
    namespaces: Interner,
}

impl XmlDocument {
    pub fn parse(bytes: impl Into<Arc<[u8]>>) -> Result<Self> {
        let source = bytes.into();
        std::str::from_utf8(&source).map_err(|error| {
            XmlError::new(
                XmlErrorCode::InvalidUtf8,
                error.valid_up_to(),
                "XML is not UTF-8",
            )
        })?;
        Parser::new(source).parse()
    }

    pub fn source(&self) -> &[u8] {
        &self.source
    }

    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    pub fn namespace(&self, symbol: Symbol) -> &str {
        self.namespaces.resolve(symbol)
    }

    pub fn source_range(&self, range: Range<usize>) -> &[u8] {
        &self.source[range]
    }

    pub fn attribute<'a>(
        &self,
        attributes: &'a [Attribute],
        namespace: Option<&str>,
        local: &str,
    ) -> Option<&'a Attribute> {
        attributes.iter().find(|attribute| {
            attribute.name.local == local
                && match (attribute.name.namespace, namespace) {
                    (None, None) => true,
                    (Some(symbol), Some(expected)) => self.namespace(symbol) == expected,
                    _ => false,
                }
        })
    }
}

struct Parser {
    source: Arc<[u8]>,
    cursor: usize,
    tokens: Vec<Token>,
    namespaces: Interner,
    scopes: Vec<HashMap<String, String>>,
    open_names: Vec<String>,
}

impl Parser {
    fn new(source: Arc<[u8]>) -> Self {
        let mut root = HashMap::new();
        root.insert(
            "xml".to_owned(),
            "http://www.w3.org/XML/1998/namespace".to_owned(),
        );
        Self {
            source,
            cursor: 0,
            tokens: Vec::new(),
            namespaces: Interner::default(),
            scopes: vec![root],
            open_names: Vec::new(),
        }
    }

    fn parse(mut self) -> Result<XmlDocument> {
        while self.cursor < self.source.len() {
            if self.source[self.cursor] != b'<' {
                let end = self.source[self.cursor..]
                    .iter()
                    .position(|byte| *byte == b'<')
                    .map_or(self.source.len(), |relative| self.cursor + relative);
                self.tokens.push(Token {
                    kind: TokenKind::Text,
                    range: self.cursor..end,
                    depth: self.open_names.len(),
                });
                self.cursor = end;
                continue;
            }
            if self.starts_with(b"<!--") {
                self.special(b"-->", 4, TokenKind::Comment)?;
            } else if self.starts_with(b"<![CDATA[") {
                self.special(b"]]>", 9, TokenKind::Cdata)?;
            } else if self.starts_with(b"<?") {
                self.special(b"?>", 2, TokenKind::ProcessingInstruction)?;
            } else if self.starts_with_case_insensitive(b"<!DOCTYPE") {
                return Err(XmlError::new(
                    XmlErrorCode::DtdForbidden,
                    self.cursor,
                    "DTD declarations are forbidden",
                ));
            } else if self.starts_with(b"</") {
                self.end_tag()?;
            } else if self.starts_with(b"<!") {
                return Err(XmlError::new(
                    XmlErrorCode::InvalidSyntax,
                    self.cursor,
                    "unsupported XML declaration",
                ));
            } else {
                self.start_tag()?;
            }
        }
        if let Some(name) = self.open_names.last() {
            return Err(XmlError::new(
                XmlErrorCode::Truncated,
                self.source.len(),
                format!("unclosed element {name}"),
            ));
        }
        Ok(XmlDocument {
            source: self.source,
            tokens: self.tokens,
            namespaces: self.namespaces,
        })
    }

    fn special(&mut self, terminator: &[u8], prefix: usize, kind: TokenKind) -> Result<()> {
        let search = self.cursor + prefix;
        let relative = find_bytes(&self.source[search..], terminator).ok_or_else(|| {
            XmlError::new(
                XmlErrorCode::Truncated,
                self.cursor,
                "unterminated XML token",
            )
        })?;
        let end = search + relative + terminator.len();
        self.tokens.push(Token {
            kind,
            range: self.cursor..end,
            depth: self.open_names.len(),
        });
        self.cursor = end;
        Ok(())
    }

    fn end_tag(&mut self) -> Result<()> {
        let end = self.source[self.cursor + 2..]
            .iter()
            .position(|byte| *byte == b'>')
            .map(|relative| self.cursor + 2 + relative)
            .ok_or_else(|| {
                XmlError::new(XmlErrorCode::Truncated, self.cursor, "unterminated end tag")
            })?;
        let raw = str_trim(&self.source[self.cursor + 2..end]);
        validate_xml_name(raw, self.cursor + 2)?;
        let expected = self.open_names.pop().ok_or_else(|| {
            XmlError::new(
                XmlErrorCode::MismatchedEndTag,
                self.cursor,
                "unexpected end tag",
            )
        })?;
        if raw != expected {
            return Err(XmlError::new(
                XmlErrorCode::MismatchedEndTag,
                self.cursor,
                format!("expected </{expected}> but found </{raw}>"),
            ));
        }
        let scope = self.scopes.last().expect("root namespace scope").clone();
        let name = resolve_name(raw, true, &scope, &mut self.namespaces, self.cursor)?;
        self.scopes.pop();
        let range = self.cursor..end + 1;
        self.cursor = end + 1;
        self.tokens.push(Token {
            kind: TokenKind::End { name },
            range,
            depth: self.open_names.len(),
        });
        Ok(())
    }

    fn start_tag(&mut self) -> Result<()> {
        let end = find_tag_end(&self.source, self.cursor + 1)?;
        let mut body_end = end;
        while body_end > self.cursor + 1 && self.source[body_end - 1].is_ascii_whitespace() {
            body_end -= 1;
        }
        let empty = body_end > self.cursor + 1 && self.source[body_end - 1] == b'/';
        if empty {
            body_end -= 1;
        }
        let mut position = self.cursor + 1;
        skip_space(&self.source, &mut position, body_end);
        let name_start = position;
        scan_name(&self.source, &mut position, body_end);
        let raw_name = utf8(&self.source[name_start..position], name_start)?;
        validate_xml_name(raw_name, name_start)?;
        let mut raw_attributes = Vec::new();
        while position < body_end {
            skip_space(&self.source, &mut position, body_end);
            if position == body_end {
                break;
            }
            let attr_start = position;
            scan_name(&self.source, &mut position, body_end);
            let attr_name = utf8(&self.source[attr_start..position], attr_start)?;
            validate_xml_name(attr_name, attr_start)?;
            skip_space(&self.source, &mut position, body_end);
            if self.source.get(position) != Some(&b'=') {
                return Err(XmlError::new(
                    XmlErrorCode::InvalidSyntax,
                    position,
                    "attribute is missing '='",
                ));
            }
            position += 1;
            skip_space(&self.source, &mut position, body_end);
            let quote = *self.source.get(position).ok_or_else(|| {
                XmlError::new(
                    XmlErrorCode::Truncated,
                    position,
                    "attribute value is missing",
                )
            })?;
            if quote != b'\'' && quote != b'"' {
                return Err(XmlError::new(
                    XmlErrorCode::InvalidSyntax,
                    position,
                    "attribute value must be quoted",
                ));
            }
            position += 1;
            let value_start = position;
            while position < body_end && self.source[position] != quote {
                position += 1;
            }
            if position == body_end {
                return Err(XmlError::new(
                    XmlErrorCode::Truncated,
                    value_start,
                    "unterminated attribute value",
                ));
            }
            let value_end = position;
            position += 1;
            let value = decode_entities(
                utf8(&self.source[value_start..value_end], value_start)?,
                value_start,
            )?;
            raw_attributes.push((
                attr_name.to_owned(),
                value,
                attr_start..position,
                value_start..value_end,
            ));
        }

        let mut scope = self.scopes.last().expect("root namespace scope").clone();
        for (name, value, _, _) in &raw_attributes {
            if name == "xmlns" {
                scope.insert(String::new(), value.clone());
            } else if let Some(prefix) = name.strip_prefix("xmlns:") {
                scope.insert(prefix.to_owned(), value.clone());
            }
        }
        let name = resolve_name(raw_name, true, &scope, &mut self.namespaces, name_start)?;
        let mut attributes = Vec::with_capacity(raw_attributes.len());
        for (raw, value, range, value_range) in raw_attributes {
            let attr_name = if raw == "xmlns" || raw.starts_with("xmlns:") {
                let local = raw.strip_prefix("xmlns:").unwrap_or("xmlns").to_owned();
                ExpandedName {
                    namespace: Some(self.namespaces.intern("http://www.w3.org/2000/xmlns/")),
                    prefix: Some("xmlns".to_owned()),
                    local,
                }
            } else {
                resolve_name(&raw, false, &scope, &mut self.namespaces, range.start)?
            };
            attributes.push(Attribute {
                name: attr_name,
                value,
                range,
                value_range,
            });
        }
        let depth = self.open_names.len();
        self.tokens.push(Token {
            kind: TokenKind::Start {
                name,
                attributes,
                empty,
            },
            range: self.cursor..end + 1,
            depth,
        });
        if !empty {
            self.open_names.push(raw_name.to_owned());
            self.scopes.push(scope);
        }
        self.cursor = end + 1;
        Ok(())
    }

    fn starts_with(&self, bytes: &[u8]) -> bool {
        self.source[self.cursor..].starts_with(bytes)
    }

    fn starts_with_case_insensitive(&self, bytes: &[u8]) -> bool {
        self.source[self.cursor..]
            .get(..bytes.len())
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(bytes))
    }
}

fn resolve_name(
    raw: &str,
    use_default: bool,
    scope: &HashMap<String, String>,
    interner: &mut Interner,
    offset: usize,
) -> Result<ExpandedName> {
    let (prefix, local) = raw
        .split_once(':')
        .map_or((None, raw), |(prefix, local)| (Some(prefix), local));
    let namespace = match prefix {
        Some(prefix) => Some(scope.get(prefix).ok_or_else(|| {
            XmlError::new(
                XmlErrorCode::UndeclaredPrefix,
                offset,
                format!("undeclared namespace prefix {prefix}"),
            )
        })?),
        None if use_default => scope.get(""),
        None => None,
    }
    .map(|uri| interner.intern(uri));
    Ok(ExpandedName {
        namespace,
        prefix: prefix.map(str::to_owned),
        local: local.to_owned(),
    })
}

pub fn decode_entities(value: &str, offset: usize) -> Result<String> {
    if !value.contains('&') {
        return Ok(value.to_owned());
    }
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    while let Some(relative) = value[cursor..].find('&') {
        let start = cursor + relative;
        output.push_str(&value[cursor..start]);
        let semicolon = value[start..].find(';').ok_or_else(|| {
            XmlError::new(
                XmlErrorCode::Entity,
                offset + start,
                "unterminated XML entity",
            )
        })? + start;
        let entity = &value[start + 1..semicolon];
        match entity {
            "amp" => output.push('&'),
            "lt" => output.push('<'),
            "gt" => output.push('>'),
            "quot" => output.push('"'),
            "apos" => output.push('\''),
            numeric if numeric.starts_with("#x") => {
                push_codepoint(&mut output, &numeric[2..], 16, offset + start)?
            }
            numeric if numeric.starts_with('#') => {
                push_codepoint(&mut output, &numeric[1..], 10, offset + start)?
            }
            _ => {
                return Err(XmlError::new(
                    XmlErrorCode::Entity,
                    offset + start,
                    format!("unsupported XML entity &{entity};"),
                ));
            }
        }
        cursor = semicolon + 1;
    }
    output.push_str(&value[cursor..]);
    Ok(output)
}

fn push_codepoint(output: &mut String, digits: &str, radix: u32, offset: usize) -> Result<()> {
    let value = u32::from_str_radix(digits, radix)
        .ok()
        .and_then(char::from_u32)
        .ok_or_else(|| XmlError::new(XmlErrorCode::Entity, offset, "invalid numeric XML entity"))?;
    output.push(value);
    Ok(())
}

fn find_tag_end(source: &[u8], mut position: usize) -> Result<usize> {
    let mut quote = None;
    while position < source.len() {
        match (source[position], quote) {
            (b'\'' | b'"', None) => quote = Some(source[position]),
            (byte, Some(expected)) if byte == expected => quote = None,
            (b'>', None) => return Ok(position),
            _ => {}
        }
        position += 1;
    }
    Err(XmlError::new(
        XmlErrorCode::Truncated,
        position,
        "unterminated start tag",
    ))
}

fn scan_name(source: &[u8], position: &mut usize, end: usize) {
    while *position < end
        && !source[*position].is_ascii_whitespace()
        && !matches!(source[*position], b'=' | b'/' | b'>')
    {
        *position += 1;
    }
}

fn skip_space(source: &[u8], position: &mut usize, end: usize) {
    while *position < end && source[*position].is_ascii_whitespace() {
        *position += 1;
    }
}

fn validate_xml_name(name: &str, offset: usize) -> Result<()> {
    if name.is_empty() || name.chars().any(char::is_whitespace) || name.matches(':').count() > 1 {
        return Err(XmlError::new(
            XmlErrorCode::InvalidSyntax,
            offset,
            "invalid XML name",
        ));
    }
    Ok(())
}

fn str_trim(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes)
        .expect("document UTF-8 checked")
        .trim()
}

fn utf8(bytes: &[u8], offset: usize) -> Result<&str> {
    std::str::from_utf8(bytes)
        .map_err(|_| XmlError::new(XmlErrorCode::InvalidUtf8, offset, "XML is not UTF-8"))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
