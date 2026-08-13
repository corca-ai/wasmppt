//! Lazy PresentationML theme, master, layout, and slide resolution.
//!
//! # Resolve one slide
//!
//! ```no_run
//! use wasmppt_layout::PresentationDocument;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let document = PresentationDocument::open(std::fs::read("report.pptx")?)?;
//! let resolved = document.resolve_slide(0)?;
//! println!("{} resolved elements", resolved.slide.elements.len());
//! # Ok(())
//! # }
//! ```

use std::{collections::BTreeMap, sync::Arc};

use sha2::{Digest, Sha256};
use wasmppt_opc::{PackageGraph, PackagePartSource, PartId, RelationshipTarget, ZipArchive};
use wasmppt_pml::{PmlError, PresentationView};

mod resolve;

pub use resolve::resolve_slide_parts;

/// English Metric Units; 914,400 EMU equal one inch.
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
    LinearGradient {
        angle: i32,
        stops: Vec<GradientStop>,
    },
    RadialGradient {
        stops: Vec<GradientStop>,
    },
    Pattern {
        preset: String,
        foreground: RgbaColor,
        background: RgbaColor,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GradientStop {
    /// Position in DrawingML's 0..=100000 range.
    pub position: i32,
    pub color: RgbaColor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnd {
    Triangle,
    Stealth,
    Diamond,
    Oval,
    Arrow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stroke {
    pub color: RgbaColor,
    pub width: Emu,
    pub dash: Option<String>,
    pub head_end: Option<LineEnd>,
    pub tail_end: Option<LineEnd>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathCommand {
    MoveTo(EmuPoint),
    LineTo(EmuPoint),
    QuadraticTo {
        control: EmuPoint,
        end: EmuPoint,
    },
    CubicTo {
        control1: EmuPoint,
        control2: EmuPoint,
        end: EmuPoint,
    },
    ArcTo {
        width_radius: Emu,
        height_radius: Emu,
        start_angle: i32,
        sweep_angle: i32,
    },
    Close,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomPath {
    pub size: EmuSize,
    pub commands: Vec<PathCommand>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OuterShadow {
    pub color: RgbaColor,
    pub blur_radius: Emu,
    pub distance: Emu,
    /// Direction in OOXML 1/60000 degree units.
    pub direction: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextAlignment {
    #[default]
    Left,
    Center,
    Right,
    Justify,
    Distributed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextVerticalAlignment {
    #[default]
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextDirection {
    #[default]
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextFlow {
    #[default]
    Horizontal,
    Vertical,
    Vertical270,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextTabAlignment {
    #[default]
    Left,
    Center,
    Right,
    Decimal,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextFontAlignment {
    #[default]
    Automatic,
    Top,
    Center,
    Baseline,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResolvedTextTab {
    pub position: Emu,
    pub alignment: TextTabAlignment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextSpacing {
    /// DrawingML percentage in thousandths of a percent.
    Percent(i32),
    /// DrawingML point value in hundredths of a point.
    Points(i32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedTextStyle {
    /// DrawingML font size in hundredths of a point.
    pub font_size: i32,
    pub color: RgbaColor,
    pub font_family: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// DrawingML character spacing in hundredths of a point.
    pub character_spacing: i32,
    /// DrawingML baseline shift in thousandths of a percent.
    pub baseline: i32,
    pub outline: Option<Stroke>,
    pub shadow: Option<OuterShadow>,
    pub inner_shadow: Option<OuterShadow>,
    pub fill: Option<Fill>,
    pub glow: Option<TextGlow>,
    pub blur_radius: Emu,
    pub soft_edge_radius: Emu,
    pub reflection: bool,
    pub alignment: TextAlignment,
    pub vertical_alignment: TextVerticalAlignment,
    pub margin_left: Emu,
    pub margin_top: Emu,
    pub margin_right: Emu,
    pub margin_bottom: Emu,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextAutofit {
    #[default]
    None,
    ShrinkText,
    ResizeShape,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextWarp {
    pub preset: String,
    /// First DrawingML adjustment in thousandths of a percent.
    pub adjustment: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedTextRun {
    pub text: String,
    pub style: ResolvedTextStyle,
    pub east_asian_font_family: Option<String>,
    pub complex_script_font_family: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedParagraph {
    pub runs: Vec<ResolvedTextRun>,
    pub alignment: TextAlignment,
    pub bullet: Option<String>,
    pub bullet_image: Option<ResolvedBulletImage>,
    pub bullet_style: Option<ResolvedTextStyle>,
    pub level: u8,
    pub margin_left: Emu,
    pub indent: Emu,
    pub line_spacing: Option<TextSpacing>,
    pub space_before: Option<TextSpacing>,
    pub space_after: Option<TextSpacing>,
    pub direction: TextDirection,
    pub tabs: Vec<ResolvedTextTab>,
    pub font_alignment: TextFontAlignment,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedBulletImage {
    pub relationship_id: String,
    pub part_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedTextFrame {
    pub paragraphs: Vec<ResolvedParagraph>,
    pub vertical_alignment: TextVerticalAlignment,
    pub margin_left: Emu,
    pub margin_top: Emu,
    pub margin_right: Emu,
    pub margin_bottom: Emu,
    pub wrap: bool,
    pub autofit: TextAutofit,
    /// Stored `a:normAutofit` scale in thousandths of a percent.
    pub autofit_font_scale: Option<i32>,
    /// Stored percentage line-spacing reduction in thousandths of a percent.
    pub autofit_line_spacing_reduction: Option<i32>,
    /// Recompute the largest fit because this logical slide revision changed its source text.
    pub autofit_recompute: bool,
    pub flow: TextFlow,
    pub column_count: u8,
    pub column_spacing: Emu,
    pub default_tab_size: Emu,
    pub warp: Option<TextWarp>,
}

impl Default for ResolvedTextStyle {
    fn default() -> Self {
        Self {
            font_size: 1_800,
            color: RgbaColor {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            font_family: None,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            character_spacing: 0,
            baseline: 0,
            outline: None,
            shadow: None,
            inner_shadow: None,
            fill: None,
            glow: None,
            blur_radius: 0,
            soft_edge_radius: 0,
            reflection: false,
            alignment: TextAlignment::Left,
            vertical_alignment: TextVerticalAlignment::Top,
            margin_left: 91_440,
            margin_top: 45_720,
            margin_right: 91_440,
            margin_bottom: 45_720,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextGlow {
    pub color: RgbaColor,
    pub radius: Emu,
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
    Pentagon,
    Octagon,
    Star5,
    Plus,
    Chevron,
    RightArrow,
    LeftArrow,
    UpArrow,
    DownArrow,
    Trapezoid,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropertyProvenance {
    pub property: &'static str,
    pub source: SourceLevel,
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
    Table {
        table: ResolvedTable,
    },
    Chart {
        chart: ResolvedChart,
    },
    PreservedGraphic {
        feature: PreservedFeature,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedTableCell {
    pub text: String,
    pub text_frame: Option<ResolvedTextFrame>,
    pub row_span: u32,
    pub column_span: u32,
    pub horizontal_merge: bool,
    pub vertical_merge: bool,
    pub fill: RgbaColor,
    pub borders: TableCellBorders,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TableCellBorders {
    pub left: Option<Stroke>,
    pub right: Option<Stroke>,
    pub top: Option<Stroke>,
    pub bottom: Option<Stroke>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedTableRow {
    pub height: Emu,
    pub cells: Vec<ResolvedTableCell>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedTable {
    pub column_widths: Vec<Emu>,
    pub rows: Vec<ResolvedTableRow>,
    pub first_row: bool,
    pub first_column: bool,
    pub banded_rows: bool,
    pub banded_columns: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChartKind {
    Column,
    Bar,
    Line,
    Pie,
    Doughnut,
    Area,
    Scatter,
    Bubble,
    Combination,
    Other,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ChartGrouping {
    #[default]
    Standard,
    Stacked,
    PercentStacked,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartSeries {
    pub kind: ChartKind,
    pub name: String,
    pub categories: Vec<String>,
    pub x_values: Vec<f64>,
    pub values: Vec<f64>,
    pub bubble_sizes: Vec<f64>,
    pub color: RgbaColor,
}

impl Eq for ChartSeries {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedChart {
    pub kind: ChartKind,
    pub grouping: ChartGrouping,
    pub series: Vec<ChartSeries>,
    pub title: Option<String>,
    pub show_legend: bool,
    pub embedded_workbook: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreservedFeature {
    SmartArt,
    Metafile,
    OleObject,
    UnknownGraphicFrame,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedElement {
    pub id: u32,
    pub name: String,
    pub source: SourceLevel,
    pub provenance: Vec<PropertyProvenance>,
    pub z_order: u32,
    pub placeholder: Option<Placeholder>,
    pub transform: Transform,
    pub group_transforms: Vec<GroupTransform>,
    pub fill: Fill,
    pub stroke: Option<Stroke>,
    pub custom_path: Option<CustomPath>,
    pub outer_shadow: Option<OuterShadow>,
    pub text: String,
    pub text_style: ResolvedTextStyle,
    /// Paragraph/run-preserving text model used by WPDL v4 renderers.
    pub text_frame: Option<ResolvedTextFrame>,
    pub alternative_text: Option<String>,
    pub hyperlink: Option<String>,
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
    UnsupportedSmartArt,
    UnsupportedMetafile,
    UnsupportedAnimation,
    UnsupportedTransition,
    UnsupportedActiveContent,
    UnsupportedThreeD,
    UnsupportedChartKind,
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
    pub embedded_fonts: Vec<EmbeddedFontResource>,
    pub diagnostics: Vec<ResolveDiagnostic>,
    pub trace: ResolutionTrace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmbeddedFontStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

/// A lazily readable font part referenced by the presentation main part.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddedFontResource {
    pub family: String,
    pub style: EmbeddedFontStyle,
    pub part_name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LayoutErrorCode {
    Package,
    InvalidPresentation,
    MissingPresentation,
    InvalidSlide,
    LimitExceeded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayoutError {
    code: LayoutErrorCode,
    message: String,
    part_name: Option<String>,
    slide_index: Option<usize>,
    cause_code: Option<&'static str>,
    offset: Option<usize>,
}

impl LayoutError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            code: LayoutErrorCode::InvalidPresentation,
            message: message.into(),
            part_name: None,
            slide_index: None,
            cause_code: None,
            offset: None,
        }
    }

    fn missing_presentation() -> Self {
        Self {
            code: LayoutErrorCode::MissingPresentation,
            message: "package has no PresentationML main part".to_owned(),
            part_name: None,
            slide_index: None,
            cause_code: None,
            offset: None,
        }
    }

    fn invalid_slide(index: usize) -> Self {
        Self {
            code: LayoutErrorCode::InvalidSlide,
            message: format!("slide index {index} is out of bounds"),
            part_name: None,
            slide_index: Some(index),
            cause_code: None,
            offset: None,
        }
    }

    fn limit(message: impl Into<String>) -> Self {
        Self {
            code: LayoutErrorCode::LimitExceeded,
            message: message.into(),
            part_name: None,
            slide_index: None,
            cause_code: Some("limit-exceeded"),
            offset: None,
        }
    }

    fn pml(error: PmlError) -> Self {
        Self {
            code: LayoutErrorCode::InvalidPresentation,
            message: error.to_string(),
            part_name: None,
            slide_index: None,
            cause_code: error.cause_code(),
            offset: error.offset(),
        }
    }

    pub(crate) fn xml(error: wasmppt_xml::XmlError, part_name: impl Into<String>) -> Self {
        Self {
            code: LayoutErrorCode::InvalidPresentation,
            message: error.to_string(),
            part_name: Some(part_name.into()),
            slide_index: None,
            cause_code: Some(xml_error_code(error.code())),
            offset: Some(error.offset()),
        }
    }

    pub const fn code(&self) -> LayoutErrorCode {
        self.code
    }

    pub fn part_name(&self) -> Option<&str> {
        self.part_name.as_deref()
    }

    pub const fn slide_index(&self) -> Option<usize> {
        self.slide_index
    }

    pub const fn cause_code(&self) -> Option<&'static str> {
        self.cause_code
    }

    pub const fn offset(&self) -> Option<usize> {
        self.offset
    }

    pub(crate) fn with_part_name(mut self, part_name: impl Into<String>) -> Self {
        self.part_name = Some(part_name.into());
        self
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
    source: Arc<dyn PackagePartSource>,
    graph: PackageGraph,
    presentation_part: PartId,
    slides: Vec<PartId>,
    slide_size: EmuSize,
    reverse_dependencies: BTreeMap<usize, Vec<PartId>>,
    open_trace: ResolutionTrace,
    embedded_fonts: Vec<EmbeddedFontResource>,
}

impl PresentationDocument {
    /// Open in-memory PPTX bytes and index only package topology plus the presentation part.
    pub fn open(bytes: impl Into<Arc<[u8]>>) -> Result<Self, LayoutError> {
        let archive = Arc::new(ZipArchive::from_bytes(bytes).map_err(package_error)?);
        Self::open_source(archive)
    }

    /// Open one immutable logical package revision.
    ///
    /// The source may be a physical ZIP or a virtual package overlay. It must expose
    /// a complete name set and exact bytes for the lifetime of this document.
    pub fn open_source(source: Arc<dyn PackagePartSource>) -> Result<Self, LayoutError> {
        let graph = PackageGraph::build_from(source.as_ref())
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
            .ok_or_else(LayoutError::missing_presentation)?;
        let presentation_name = graph.part_name(graph.part(presentation_part)).to_owned();
        let presentation_bytes = source
            .read_part(&presentation_name)
            .map_err(package_error)?;
        let presentation = PresentationView::parse(presentation_bytes).map_err(LayoutError::pml)?;
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
        let embedded_fonts = resolve::discover_embedded_fonts(source.as_ref(), &graph)?;
        Ok(Self {
            source,
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
            embedded_fonts,
        })
    }

    /// Number of logical slides in presentation order.
    pub fn slide_count(&self) -> usize {
        self.slides.len()
    }

    /// Ordered logical slide part names used to detect presentation-topology changes.
    pub fn slide_part_names(&self) -> Vec<&str> {
        self.slides
            .iter()
            .map(|part| self.graph.part_name(self.graph.part(*part)))
            .collect()
    }

    /// Trace produced by opening the package before any slide is resolved.
    pub fn open_trace(&self) -> &ResolutionTrace {
        &self.open_trace
    }

    /// Resolve one zero-based slide and only its reachable dependency branch.
    pub fn resolve_slide(&self, index: usize) -> Result<ResolveOutput, LayoutError> {
        let slide = *self
            .slides
            .get(index)
            .ok_or_else(|| LayoutError::invalid_slide(index))?;
        resolve::resolve_slide_parts_cached(
            self.source.as_ref(),
            &self.graph,
            slide,
            self.slide_size,
            &self.embedded_fonts,
            u32::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1)),
        )
    }

    /// Inflate one explicitly requested package part for a render-host resource resolver.
    ///
    /// Callers discover resource names from a resolved display list; the presentation remains
    /// indexed and no unrelated media is decoded eagerly.
    pub fn read_part(&self, part_name: &str) -> Result<Vec<u8>, LayoutError> {
        self.source.read_part(part_name).map_err(package_error)
    }

    /// Rebind unchanged graph/index state to a new immutable source revision.
    ///
    /// Callers may use this only when relationship, content-type, and presentation
    /// topology bytes are proven unchanged.
    pub fn with_compatible_source(&self, source: Arc<dyn PackagePartSource>) -> Self {
        Self {
            source,
            graph: self.graph.clone(),
            presentation_part: self.presentation_part,
            slides: self.slides.clone(),
            slide_size: self.slide_size,
            reverse_dependencies: self.reverse_dependencies.clone(),
            open_trace: self.open_trace.clone(),
            embedded_fonts: self.embedded_fonts.clone(),
        }
    }

    /// Slides whose proven relationship graph reaches the changed part.
    pub fn invalidated_slides(&self, changed_part_name: &str) -> Vec<usize> {
        let relationship_owner = relationship_owner(changed_part_name);
        let changed_part_name = relationship_owner.as_deref().unwrap_or(changed_part_name);
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

    /// Union the invalidation result for multiple changed package part names.
    pub fn invalidated_slides_for_parts<'a>(
        &self,
        changed_part_names: impl IntoIterator<Item = &'a str>,
    ) -> Vec<usize> {
        let mut slides = changed_part_names
            .into_iter()
            .flat_map(|name| self.invalidated_slides(name))
            .collect::<Vec<_>>();
        slides.sort_unstable();
        slides.dedup();
        slides
    }

    /// Hash the exact transitive package bytes that can affect one resolved slide.
    /// Relationship parts are included explicitly because OPC models them as graph
    /// edges rather than ordinary `Part` nodes.
    pub fn slide_dependency_fingerprint(&self, index: usize) -> Result<[u8; 32], LayoutError> {
        let slide = *self
            .slides
            .get(index)
            .ok_or_else(|| LayoutError::invalid_slide(index))?;
        let mut names = self
            .graph
            .walk_from(slide, self.graph.parts().len().saturating_add(1))
            .map_err(|limit| {
                LayoutError::limit(format!(
                    "slide dependency traversal exceeds {} parts",
                    limit.maximum
                ))
            })?
            .into_iter()
            .map(|part| self.graph.part_name(self.graph.part(part)).to_owned())
            .collect::<Vec<_>>();
        names.push(self.presentation_part_name().to_owned());
        for global in ["[Content_Types].xml", "_rels/.rels"] {
            if self.source.contains_part(global) {
                names.push(global.to_owned());
            }
        }
        let relationship_names = names
            .iter()
            .map(|name| relationship_part_name(name))
            .filter(|name| self.source.contains_part(name))
            .collect::<Vec<_>>();
        names.extend(relationship_names);
        names.sort_unstable();
        names.dedup();
        let mut hasher = Sha256::new();
        for name in names {
            let bytes = self.source.read_part(&name).map_err(package_error)?;
            hasher.update((name.len() as u64).to_le_bytes());
            hasher.update(name.as_bytes());
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        }
        Ok(hasher.finalize().into())
    }

    /// Hash one exact package part for content-addressed host caches.
    pub fn part_fingerprint(&self, part_name: &str) -> Result<[u8; 32], LayoutError> {
        let bytes = self.source.read_part(part_name).map_err(package_error)?;
        Ok(Sha256::digest(bytes).into())
    }

    /// Name of the package's main PresentationML part.
    pub fn presentation_part_name(&self) -> &str {
        self.graph
            .part_name(self.graph.part(self.presentation_part))
    }
}

fn relationship_part_name(part_name: &str) -> String {
    match part_name.rsplit_once('/') {
        Some((directory, file)) => format!("{directory}/_rels/{file}.rels"),
        None => format!("_rels/{part_name}.rels"),
    }
}

fn relationship_owner(relationship_name: &str) -> Option<String> {
    let (directory, file) = relationship_name.rsplit_once("/_rels/")?;
    let file = file.strip_suffix(".rels")?;
    Some(format!("{directory}/{file}"))
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
    LayoutError {
        code: LayoutErrorCode::Package,
        message: error.to_string(),
        part_name: None,
        slide_index: None,
        cause_code: Some(opc_error_code(error.code())),
        offset: None,
    }
}

const fn opc_error_code(code: wasmppt_opc::ErrorCode) -> &'static str {
    use wasmppt_opc::ErrorCode;
    match code {
        ErrorCode::Io => "io",
        ErrorCode::Truncated => "truncated",
        ErrorCode::InvalidSignature => "invalid-signature",
        ErrorCode::InvalidField => "invalid-field",
        ErrorCode::InvalidPath => "invalid-path",
        ErrorCode::DuplicateEntry => "duplicate-entry",
        ErrorCode::UnsupportedCompression => "unsupported-compression",
        ErrorCode::UnsupportedEncryption => "unsupported-encryption",
        ErrorCode::UnsupportedMultiDisk => "unsupported-multi-disk",
        ErrorCode::UnsupportedZip64 => "unsupported-zip64",
        ErrorCode::LimitExceeded => "limit-exceeded",
        ErrorCode::OverlappingEntries => "overlapping-entries",
        ErrorCode::ChecksumMismatch => "checksum-mismatch",
        ErrorCode::SizeMismatch => "size-mismatch",
        _ => "unknown",
    }
}

const fn xml_error_code(code: wasmppt_xml::XmlErrorCode) -> &'static str {
    use wasmppt_xml::XmlErrorCode;
    match code {
        XmlErrorCode::InvalidUtf8 => "invalid-utf8",
        XmlErrorCode::Truncated => "truncated",
        XmlErrorCode::InvalidSyntax => "invalid-syntax",
        XmlErrorCode::UndeclaredPrefix => "undeclared-prefix",
        XmlErrorCode::MismatchedEndTag => "mismatched-end-tag",
        XmlErrorCode::DtdForbidden => "dtd-forbidden",
        XmlErrorCode::Entity => "entity",
        XmlErrorCode::LimitExceeded => "limit-exceeded",
        _ => "unknown",
    }
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
