//! Lazy PresentationML theme, master, layout, and slide resolution.

use std::{collections::BTreeMap, sync::Arc};

use wasmppt_opc::{MemorySource, PackageGraph, PartId, RelationshipTarget, ZipArchive};
use wasmppt_pml::PresentationView;

mod resolve;

pub use resolve::resolve_slide_parts;

pub type Emu = i64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EmuPoint {
    pub x: Emu,
    pub y: Emu,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EmuSize {
    pub width: Emu,
    pub height: Emu,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EmuRect {
    pub origin: EmuPoint,
    pub size: EmuSize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Transform {
    pub bounds: EmuRect,
    /// Rotation in OOXML 1/60000 degree units.
    pub rotation: i32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GroupTransform {
    pub outer: Transform,
    pub child_origin: EmuPoint,
    pub child_size: EmuSize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RgbaColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Fill {
    None,
    Solid(RgbaColor),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stroke {
    pub color: RgbaColor,
    pub width: Emu,
    pub dash: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PresetGeometry {
    Rect,
    RoundRect,
    Ellipse,
    Line,
    Triangle,
    RightTriangle,
    Diamond,
    Parallelogram,
    Hexagon,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImageCrop {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Placeholder {
    pub kind: String,
    pub index: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceLevel {
    Master,
    Layout,
    Slide,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ElementKind {
    Shape {
        geometry: PresetGeometry,
    },
    Image {
        relationship_id: String,
        part_name: Option<String>,
        crop: ImageCrop,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedElement {
    pub id: u32,
    pub name: String,
    pub source: SourceLevel,
    pub z_order: u32,
    pub placeholder: Option<Placeholder>,
    pub transform: Transform,
    pub group_transforms: Vec<GroupTransform>,
    pub fill: Fill,
    pub stroke: Option<Stroke>,
    pub text: String,
    pub kind: ElementKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedSlide {
    pub part_name: String,
    pub size: EmuSize,
    pub background: RgbaColor,
    pub elements: Vec<ResolvedElement>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ResolveDiagnosticCode {
    MissingDependency,
    InvalidXml,
    InvalidValue,
    UnsupportedGraphicFrame,
    UnsupportedCustomGeometry,
    UnsupportedFill,
    UnsupportedEffect,
    MissingImage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveDiagnostic {
    pub code: ResolveDiagnosticCode,
    pub part_name: String,
    pub shape_id: Option<u32>,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResolutionTrace {
    pub visited_parts: Vec<String>,
    pub parsed_xml_parts: Vec<String>,
    pub decoded_media_parts: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolveOutput {
    pub slide: ResolvedSlide,
    pub diagnostics: Vec<ResolveDiagnostic>,
    pub trace: ResolutionTrace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayoutError {
    message: String,
}

impl LayoutError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for LayoutError {}

/// An indexed deck that parses slide XML only when that slide is resolved.
#[derive(Debug)]
pub struct PresentationDocument {
    archive: ZipArchive<MemorySource>,
    graph: PackageGraph,
    presentation_part: PartId,
    slides: Vec<PartId>,
    slide_size: EmuSize,
    reverse_dependencies: BTreeMap<usize, Vec<PartId>>,
    open_trace: ResolutionTrace,
}

impl PresentationDocument {
    pub fn open(bytes: impl Into<Arc<[u8]>>) -> Result<Self, LayoutError> {
        let archive = ZipArchive::from_bytes(bytes).map_err(package_error)?;
        let graph = PackageGraph::build(&archive)
            .map_err(|error| LayoutError::new(format!("cannot build package graph: {error}")))?;
        let presentation_part = graph
            .package_relationships()
            .iter()
            .find(|relationship| {
                graph
                    .relationship_type(relationship)
                    .ends_with("/officeDocument")
            })
            .and_then(|relationship| match relationship.target {
                RelationshipTarget::Internal(part) => Some(part),
                _ => None,
            })
            .or_else(|| {
                graph
                    .part_by_name("ppt/presentation.xml")
                    .map(|part| part.id)
            })
            .ok_or_else(|| LayoutError::new("package has no PresentationML main part"))?;
        let presentation_name = graph.part_name(graph.part(presentation_part)).to_owned();
        let entry = archive
            .entry(&presentation_name)
            .ok_or_else(|| LayoutError::new("presentation part has no ZIP entry"))?;
        let presentation_bytes = archive.read_entry(entry).map_err(package_error)?;
        let presentation = PresentationView::parse(presentation_bytes)
            .map_err(|error| LayoutError::new(format!("cannot parse presentation: {error}")))?;
        let slides = presentation
            .slide_relationship_ids()
            .iter()
            .filter_map(|id| {
                graph
                    .part(presentation_part)
                    .relationships
                    .iter()
                    .find(|relationship| graph.relationship_id(relationship) == id)
                    .and_then(|relationship| match relationship.target {
                        RelationshipTarget::Internal(part) => Some(part),
                        _ => None,
                    })
            })
            .collect::<Vec<_>>();
        let slide_size = presentation_size(presentation.document()).unwrap_or(EmuSize {
            width: 9_144_000,
            height: 6_858_000,
        });
        let reverse_dependencies = reverse_dependencies(&graph);
        Ok(Self {
            archive,
            graph,
            presentation_part,
            slides,
            slide_size,
            reverse_dependencies,
            open_trace: ResolutionTrace {
                visited_parts: vec![presentation_name.clone()],
                parsed_xml_parts: vec![presentation_name],
                decoded_media_parts: Vec::new(),
            },
        })
    }

    pub fn slide_count(&self) -> usize {
        self.slides.len()
    }

    pub fn open_trace(&self) -> &ResolutionTrace {
        &self.open_trace
    }

    pub fn resolve_slide(&self, index: usize) -> Result<ResolveOutput, LayoutError> {
        let slide = *self
            .slides
            .get(index)
            .ok_or_else(|| LayoutError::new(format!("slide index {index} is out of bounds")))?;
        resolve_slide_parts(&self.archive, &self.graph, slide, self.slide_size)
    }

    /// Slides whose proven relationship graph reaches the changed part.
    pub fn invalidated_slides(&self, changed_part_name: &str) -> Vec<usize> {
        let Some(changed) = self
            .graph
            .part_by_name(changed_part_name)
            .map(|part| part.id)
        else {
            return Vec::new();
        };
        let mut affected = std::collections::HashSet::from([changed]);
        let mut queue = std::collections::VecDeque::from([changed]);
        while let Some(part) = queue.pop_front() {
            for dependent in self
                .reverse_dependencies
                .get(&part.index())
                .into_iter()
                .flatten()
            {
                if affected.insert(*dependent) {
                    queue.push_back(*dependent);
                }
            }
        }
        self.slides
            .iter()
            .enumerate()
            .filter_map(|(index, slide)| affected.contains(slide).then_some(index))
            .collect()
    }

    pub fn presentation_part_name(&self) -> &str {
        self.graph
            .part_name(self.graph.part(self.presentation_part))
    }
}

fn reverse_dependencies(graph: &PackageGraph) -> BTreeMap<usize, Vec<PartId>> {
    let mut reverse = BTreeMap::<usize, Vec<PartId>>::new();
    for part in graph.parts() {
        for relationship in &part.relationships {
            if let RelationshipTarget::Internal(target) = relationship.target {
                reverse.entry(target.index()).or_default().push(part.id);
            }
        }
    }
    reverse
}

fn presentation_size(document: &wasmppt_xml::XmlDocument) -> Option<EmuSize> {
    document.tokens().iter().find_map(|token| {
        let wasmppt_xml::TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            return None;
        };
        if name.local != "sldSz" {
            return None;
        }
        Some(EmuSize {
            width: plain_i64(attributes, "cx")?,
            height: plain_i64(attributes, "cy")?,
        })
    })
}

pub(crate) fn plain_i64(attributes: &[wasmppt_xml::Attribute], local: &str) -> Option<i64> {
    attributes
        .iter()
        .find(|attribute| attribute.name.namespace.is_none() && attribute.name.local == local)
        .and_then(|attribute| attribute.value.parse().ok())
}

fn package_error(error: wasmppt_opc::Error) -> LayoutError {
    LayoutError::new(error.to_string())
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
