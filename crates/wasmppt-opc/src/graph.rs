use std::collections::{HashMap, HashSet, VecDeque};

use wasmppt_xml::{Interner, TokenKind, XmlDocument};

use crate::{ReadAt, ZipArchive};

const CONTENT_TYPES: &str = "[Content_Types].xml";
const PACKAGE_RELS: &str = "_rels/.rels";
const TRANSITIONAL_CONTENT_TYPES_NS: &str =
    "http://schemas.openxmlformats.org/package/2006/content-types";
const STRICT_CONTENT_TYPES_NS: &str = "http://purl.oclc.org/ooxml/package/content-types";
const TRANSITIONAL_RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const STRICT_RELS_NS: &str = "http://purl.oclc.org/ooxml/package/relationships";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PartId(u32);

impl PartId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Conformance {
    Transitional,
    Strict,
    Mixed,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DiagnosticCode {
    MissingContentTypes,
    InvalidContentTypesXml,
    InvalidContentTypesRoot,
    DuplicateContentType,
    MissingContentType,
    InvalidRelationshipsXml,
    InvalidRelationshipsRoot,
    OrphanRelationshipPart,
    DuplicateRelationshipId,
    InvalidRelationshipTarget,
    MissingRelationshipTarget,
    RelationshipCycle,
    OrphanedPart,
    MixedConformance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub part: Option<PartId>,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphError {
    message: String,
}

impl GraphError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for GraphError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RelationshipTarget {
    Internal(PartId),
    Missing(String),
    External(String),
}

#[derive(Clone, Debug)]
pub struct Relationship {
    id: wasmppt_xml::Symbol,
    relationship_type: wasmppt_xml::Symbol,
    pub target: RelationshipTarget,
}

#[derive(Clone, Debug)]
pub struct Part {
    pub id: PartId,
    name: wasmppt_xml::Symbol,
    content_type: Option<wasmppt_xml::Symbol>,
    pub relationships: Vec<Relationship>,
    pub orphaned: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraversalLimit {
    pub maximum: usize,
}

#[derive(Clone, Debug)]
pub struct PackageGraph {
    parts: Vec<Part>,
    package_relationships: Vec<Relationship>,
    names: Interner,
    relationship_ids: Interner,
    relationship_types: Interner,
    content_types: Interner,
    namespace_uris: Interner,
    name_to_id: HashMap<String, PartId>,
    diagnostics: Vec<Diagnostic>,
    conformance: Conformance,
}

impl PackageGraph {
    pub fn build<S: ReadAt>(archive: &ZipArchive<S>) -> Result<Self, GraphError> {
        let mut graph = Self {
            parts: Vec::new(),
            package_relationships: Vec::new(),
            names: Interner::default(),
            relationship_ids: Interner::default(),
            relationship_types: Interner::default(),
            content_types: Interner::default(),
            namespace_uris: Interner::default(),
            name_to_id: HashMap::new(),
            diagnostics: Vec::new(),
            conformance: Conformance::Unknown,
        };

        for entry in archive.entries().iter().filter(|entry| {
            !entry.name.ends_with('/')
                && entry.name != CONTENT_TYPES
                && !is_relationship_part(&entry.name)
        }) {
            let id = PartId(
                u32::try_from(graph.parts.len())
                    .map_err(|_| GraphError::new("too many OPC parts"))?,
            );
            let name = graph.names.intern(&entry.name);
            graph.name_to_id.insert(entry.name.clone(), id);
            graph.parts.push(Part {
                id,
                name,
                content_type: None,
                relationships: Vec::new(),
                orphaned: false,
            });
        }

        let mut evidence = ConformanceEvidence::default();
        graph.load_content_types(archive, &mut evidence)?;
        graph.load_relationships(archive, &mut evidence)?;
        graph.inspect_main_namespaces(archive, &mut evidence)?;
        graph.conformance = evidence.finish();
        if graph.conformance == Conformance::Mixed {
            graph.diagnostics.push(Diagnostic {
                code: DiagnosticCode::MixedConformance,
                part: None,
                message: "package mixes Transitional and Strict OOXML namespaces".to_owned(),
            });
        }
        graph.detect_cycles();
        graph.mark_orphans();
        Ok(graph)
    }

    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    pub fn package_relationships(&self) -> &[Relationship] {
        &self.package_relationships
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub const fn conformance(&self) -> Conformance {
        self.conformance
    }

    pub fn part(&self, id: PartId) -> &Part {
        &self.parts[id.index()]
    }

    pub fn part_by_name(&self, name: &str) -> Option<&Part> {
        self.name_to_id.get(name).map(|id| self.part(*id))
    }

    pub fn part_name(&self, part: &Part) -> &str {
        self.names.resolve(part.name)
    }

    pub fn content_type(&self, part: &Part) -> Option<&str> {
        part.content_type
            .map(|symbol| self.content_types.resolve(symbol))
    }

    pub fn relationship_id(&self, relationship: &Relationship) -> &str {
        self.relationship_ids.resolve(relationship.id)
    }

    pub fn relationship_type(&self, relationship: &Relationship) -> &str {
        self.relationship_types
            .resolve(relationship.relationship_type)
    }

    pub fn interned_namespace_count(&self) -> usize {
        self.namespace_uris.len()
    }

    /// Breadth-first traversal with a hard visit limit; cycles never recurse.
    pub fn walk_from(&self, start: PartId, maximum: usize) -> Result<Vec<PartId>, TraversalLimit> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::from([start]);
        let mut output = Vec::new();
        while let Some(id) = queue.pop_front() {
            if !visited.insert(id) {
                continue;
            }
            if output.len() == maximum {
                return Err(TraversalLimit { maximum });
            }
            output.push(id);
            for relationship in &self.part(id).relationships {
                if let RelationshipTarget::Internal(target) = relationship.target {
                    queue.push_back(target);
                }
            }
        }
        Ok(output)
    }

    fn load_content_types<S: ReadAt>(
        &mut self,
        archive: &ZipArchive<S>,
        evidence: &mut ConformanceEvidence,
    ) -> Result<(), GraphError> {
        let Some(entry) = archive.entry(CONTENT_TYPES) else {
            self.diagnostics.push(Diagnostic {
                code: DiagnosticCode::MissingContentTypes,
                part: None,
                message: "package has no [Content_Types].xml".to_owned(),
            });
            return Ok(());
        };
        let bytes = archive
            .read_entry(entry)
            .map_err(|error| GraphError::new(format!("cannot read {CONTENT_TYPES}: {error}")))?;
        let document = match XmlDocument::parse(bytes) {
            Ok(document) => document,
            Err(error) => {
                self.diagnostics.push(Diagnostic {
                    code: DiagnosticCode::InvalidContentTypesXml,
                    part: None,
                    message: error.to_string(),
                });
                return Ok(());
            }
        };
        self.collect_namespaces(&document, evidence);
        if !root_matches(
            &document,
            "Types",
            &[TRANSITIONAL_CONTENT_TYPES_NS, STRICT_CONTENT_TYPES_NS],
        ) {
            self.diagnostics.push(Diagnostic {
                code: DiagnosticCode::InvalidContentTypesRoot,
                part: None,
                message: "content-types root has an unexpected namespace or local name".to_owned(),
            });
        }

        let mut defaults = HashMap::<String, String>::new();
        let mut overrides = HashMap::<String, String>::new();
        for token in document.tokens() {
            let TokenKind::Start {
                name, attributes, ..
            } = &token.kind
            else {
                continue;
            };
            if name.local == "Default" {
                let extension = attribute_value(&document, attributes, "Extension");
                let content_type = attribute_value(&document, attributes, "ContentType");
                if let (Some(extension), Some(content_type)) = (extension, content_type) {
                    if defaults
                        .insert(extension.to_ascii_lowercase(), content_type.to_owned())
                        .is_some()
                    {
                        self.diagnostics.push(duplicate_content_type(format!(
                            "duplicate Default for .{extension}"
                        )));
                    }
                }
            } else if name.local == "Override" {
                let part_name = attribute_value(&document, attributes, "PartName")
                    .map(|name| name.trim_start_matches('/'));
                let content_type = attribute_value(&document, attributes, "ContentType");
                if let (Some(part_name), Some(content_type)) = (part_name, content_type) {
                    if overrides
                        .insert(part_name.to_owned(), content_type.to_owned())
                        .is_some()
                    {
                        self.diagnostics.push(duplicate_content_type(format!(
                            "duplicate Override for /{part_name}"
                        )));
                    }
                }
            }
        }
        for part in &mut self.parts {
            let name = self.names.resolve(part.name);
            let content_type = overrides.get(name).or_else(|| {
                name.rsplit_once('.')
                    .and_then(|(_, extension)| defaults.get(&extension.to_ascii_lowercase()))
            });
            if let Some(content_type) = content_type {
                part.content_type = Some(self.content_types.intern(content_type));
            } else {
                self.diagnostics.push(Diagnostic {
                    code: DiagnosticCode::MissingContentType,
                    part: Some(part.id),
                    message: format!("no content type resolves for {name}"),
                });
            }
        }
        Ok(())
    }

    fn load_relationships<S: ReadAt>(
        &mut self,
        archive: &ZipArchive<S>,
        evidence: &mut ConformanceEvidence,
    ) -> Result<(), GraphError> {
        for entry in archive
            .entries()
            .iter()
            .filter(|entry| is_relationship_part(&entry.name))
        {
            let source = relationship_source(&entry.name);
            let source_id = source
                .as_deref()
                .and_then(|name| self.name_to_id.get(name).copied());
            if source.is_some() && source_id.is_none() {
                self.diagnostics.push(Diagnostic {
                    code: DiagnosticCode::OrphanRelationshipPart,
                    part: None,
                    message: format!("relationship part {} has no source part", entry.name),
                });
            }
            let bytes = archive
                .read_entry(entry)
                .map_err(|error| GraphError::new(format!("cannot read {}: {error}", entry.name)))?;
            let document = match XmlDocument::parse(bytes) {
                Ok(document) => document,
                Err(error) => {
                    self.diagnostics.push(Diagnostic {
                        code: DiagnosticCode::InvalidRelationshipsXml,
                        part: source_id,
                        message: format!("{}: {error}", entry.name),
                    });
                    continue;
                }
            };
            self.collect_namespaces(&document, evidence);
            if !root_matches(
                &document,
                "Relationships",
                &[TRANSITIONAL_RELS_NS, STRICT_RELS_NS],
            ) {
                self.diagnostics.push(Diagnostic {
                    code: DiagnosticCode::InvalidRelationshipsRoot,
                    part: source_id,
                    message: format!("{} has an unexpected Relationships root", entry.name),
                });
            }
            let mut ids = HashSet::new();
            let mut relationships = Vec::new();
            for token in document.tokens() {
                let TokenKind::Start {
                    name, attributes, ..
                } = &token.kind
                else {
                    continue;
                };
                if name.local != "Relationship" {
                    continue;
                }
                let (Some(id), Some(kind), Some(target)) = (
                    attribute_value(&document, attributes, "Id"),
                    attribute_value(&document, attributes, "Type"),
                    attribute_value(&document, attributes, "Target"),
                ) else {
                    self.diagnostics.push(Diagnostic {
                        code: DiagnosticCode::InvalidRelationshipTarget,
                        part: source_id,
                        message: format!(
                            "{} contains a Relationship missing Id, Type, or Target",
                            entry.name
                        ),
                    });
                    continue;
                };
                if !ids.insert(id.to_owned()) {
                    self.diagnostics.push(Diagnostic {
                        code: DiagnosticCode::DuplicateRelationshipId,
                        part: source_id,
                        message: format!("duplicate relationship ID {id} in {}", entry.name),
                    });
                    continue;
                }
                let external = attribute_value(&document, attributes, "TargetMode")
                    .is_some_and(|mode| mode.eq_ignore_ascii_case("External"));
                let resolved = if external {
                    RelationshipTarget::External(target.to_owned())
                } else {
                    match resolve_target(source.as_deref(), target) {
                        Some(name) => match self.name_to_id.get(&name) {
                            Some(id) => RelationshipTarget::Internal(*id),
                            None => {
                                self.diagnostics.push(Diagnostic {
                                    code: DiagnosticCode::MissingRelationshipTarget,
                                    part: source_id,
                                    message: format!(
                                        "relationship {id} targets missing part {name}"
                                    ),
                                });
                                RelationshipTarget::Missing(name)
                            }
                        },
                        None => {
                            self.diagnostics.push(Diagnostic {
                                code: DiagnosticCode::InvalidRelationshipTarget,
                                part: source_id,
                                message: format!("relationship {id} has unsafe target {target:?}"),
                            });
                            RelationshipTarget::Missing(target.to_owned())
                        }
                    }
                };
                evidence.observe_uri(kind);
                relationships.push(Relationship {
                    id: self.relationship_ids.intern(id),
                    relationship_type: self.relationship_types.intern(kind),
                    target: resolved,
                });
            }
            if entry.name == PACKAGE_RELS {
                self.package_relationships = relationships;
            } else if let Some(source_id) = source_id {
                self.parts[source_id.index()].relationships = relationships;
            }
        }
        Ok(())
    }

    fn inspect_main_namespaces<S: ReadAt>(
        &mut self,
        archive: &ZipArchive<S>,
        evidence: &mut ConformanceEvidence,
    ) -> Result<(), GraphError> {
        let candidates = self
            .parts
            .iter()
            .filter_map(|part| {
                let content_type = self.content_type(part)?;
                (content_type.contains("presentationml") && content_type.contains("main+xml"))
                    .then(|| (part.id, self.part_name(part).to_owned()))
            })
            .collect::<Vec<_>>();
        for (_, name) in candidates {
            let Some(entry) = archive.entry(&name) else {
                continue;
            };
            let bytes = archive
                .read_entry(entry)
                .map_err(|error| GraphError::new(format!("cannot read {name}: {error}")))?;
            if let Ok(document) = XmlDocument::parse(bytes) {
                self.collect_namespaces(&document, evidence);
            }
        }
        Ok(())
    }

    fn collect_namespaces(&mut self, document: &XmlDocument, evidence: &mut ConformanceEvidence) {
        for token in document.tokens() {
            let (name, attributes) = match &token.kind {
                TokenKind::Start {
                    name, attributes, ..
                } => (Some(name), attributes.as_slice()),
                TokenKind::End { name } => (Some(name), &[][..]),
                _ => (None, &[][..]),
            };
            if let Some(symbol) = name.and_then(|name| name.namespace) {
                let uri = document.namespace(symbol);
                self.namespace_uris.intern(uri);
                evidence.observe_uri(uri);
            }
            for attribute in attributes {
                if let Some(symbol) = attribute.name.namespace {
                    let uri = document.namespace(symbol);
                    self.namespace_uris.intern(uri);
                    evidence.observe_uri(uri);
                }
            }
        }
    }

    fn detect_cycles(&mut self) {
        let mut colors = vec![0u8; self.parts.len()];
        let mut reported = HashSet::new();
        for start in 0..self.parts.len() {
            if colors[start] != 0 {
                continue;
            }
            let root = PartId(start as u32);
            colors[start] = 1;
            let mut stack = vec![(root, 0usize)];
            while let Some((id, edge_index)) = stack.last_mut() {
                let edges = &self.parts[id.index()].relationships;
                if *edge_index == edges.len() {
                    colors[id.index()] = 2;
                    stack.pop();
                    continue;
                }
                let relationship = &edges[*edge_index];
                *edge_index += 1;
                let RelationshipTarget::Internal(target) = relationship.target else {
                    continue;
                };
                match colors[target.index()] {
                    0 => {
                        colors[target.index()] = 1;
                        stack.push((target, 0));
                    }
                    1 if reported.insert(target) => self.diagnostics.push(Diagnostic {
                        code: DiagnosticCode::RelationshipCycle,
                        part: Some(target),
                        message: "relationship graph contains a cycle".to_owned(),
                    }),
                    _ => {}
                }
            }
        }
    }

    fn mark_orphans(&mut self) {
        let mut reachable = HashSet::new();
        let mut queue = VecDeque::new();
        for relationship in &self.package_relationships {
            if let RelationshipTarget::Internal(id) = relationship.target {
                queue.push_back(id);
            }
        }
        while let Some(id) = queue.pop_front() {
            if !reachable.insert(id) {
                continue;
            }
            for relationship in &self.parts[id.index()].relationships {
                if let RelationshipTarget::Internal(target) = relationship.target {
                    queue.push_back(target);
                }
            }
        }
        for part in &mut self.parts {
            part.orphaned = !reachable.contains(&part.id);
            if part.orphaned {
                self.diagnostics.push(Diagnostic {
                    code: DiagnosticCode::OrphanedPart,
                    part: Some(part.id),
                    message: format!(
                        "part {} is unreachable from package relationships",
                        self.names.resolve(part.name)
                    ),
                });
            }
        }
    }
}

#[derive(Default)]
struct ConformanceEvidence {
    transitional: bool,
    strict: bool,
}

impl ConformanceEvidence {
    fn observe_uri(&mut self, uri: &str) {
        if uri.contains("purl.oclc.org/ooxml") {
            self.strict = true;
        }
        if uri.contains("schemas.openxmlformats.org/package/2006")
            || uri.contains("schemas.openxmlformats.org/officeDocument/2006")
            || uri.contains("schemas.openxmlformats.org/presentationml/2006")
            || uri.contains("schemas.openxmlformats.org/drawingml/2006")
        {
            self.transitional = true;
        }
    }

    fn finish(self) -> Conformance {
        match (self.transitional, self.strict) {
            (true, false) => Conformance::Transitional,
            (false, true) => Conformance::Strict,
            (true, true) => Conformance::Mixed,
            (false, false) => Conformance::Unknown,
        }
    }
}

fn duplicate_content_type(message: String) -> Diagnostic {
    Diagnostic {
        code: DiagnosticCode::DuplicateContentType,
        part: None,
        message,
    }
}

fn attribute_value<'a>(
    document: &XmlDocument,
    attributes: &'a [wasmppt_xml::Attribute],
    local: &str,
) -> Option<&'a str> {
    document
        .attribute(attributes, None, local)
        .map(|attribute| attribute.value.as_str())
}

fn root_matches(document: &XmlDocument, local: &str, namespaces: &[&str]) -> bool {
    document
        .tokens()
        .iter()
        .find_map(|token| {
            let TokenKind::Start { name, .. } = &token.kind else {
                return None;
            };
            Some(
                name.local == local
                    && name
                        .namespace
                        .is_some_and(|symbol| namespaces.contains(&document.namespace(symbol))),
            )
        })
        .unwrap_or(false)
}

fn is_relationship_part(name: &str) -> bool {
    name == PACKAGE_RELS
        || (name.ends_with(".rels") && name.split('/').any(|segment| segment == "_rels"))
}

fn relationship_source(name: &str) -> Option<String> {
    if name == PACKAGE_RELS {
        return None;
    }
    let (directory, file) = name.rsplit_once("/_rels/")?;
    let base = file.strip_suffix(".rels")?;
    Some(if directory.is_empty() {
        base.to_owned()
    } else {
        format!("{directory}/{base}")
    })
}

fn resolve_target(source: Option<&str>, target: &str) -> Option<String> {
    let target = target.split('#').next().unwrap_or(target);
    let mut segments = Vec::new();
    if !target.starts_with('/') {
        if let Some((directory, _)) = source.and_then(|source| source.rsplit_once('/')) {
            segments.extend(
                directory
                    .split('/')
                    .filter(|segment| !segment.is_empty())
                    .map(str::to_owned),
            );
        }
    }
    for segment in target.trim_start_matches('/').split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            segment if segment.contains('\\') || segment.contains('\0') => return None,
            segment => segments.push(segment.to_owned()),
        }
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}
