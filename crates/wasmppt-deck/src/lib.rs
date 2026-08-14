//! Host-neutral contracts between authoring adapters and the wasmppt deck engine.
//!
//! `DeckSpec` describes source-backed semantic content. `DeckTemplatePlan` describes
//! template-owned regions, and `DeckPlan` assigns every renderable source fragment to
//! physical pages. The contracts contain no host APIs and use bounded, versioned binary
//! encodings for native, Wasm, browser Worker, and workerd boundaries.

mod media;
mod validate;
mod wire;

use sha2::{Digest, Sha256};
use std::fmt;

pub use media::{inspect_jpeg_size, inspect_media_size};
pub use validate::{validate_deck_plan, validate_deck_spec};
pub use wire::{WireError, WireErrorKind};

/// English Metric Units; 914,400 EMU equal one inch.
pub type Emu = i64;

/// Stable 128-bit identity derived from source or another stable identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StableId([u8; 16]);

impl StableId {
    pub const NIL: Self = Self([0; 16]);

    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Derive an identity from a document identity, exact source span, and semantic role.
    #[must_use]
    pub fn from_source(document_identity: &[u8], source: &SourceRange, role: SemanticRole) -> Self {
        let mut digest = Sha256::new();
        digest.update(b"wasmppt/deck/source-id/v1\0");
        update_len_prefixed(&mut digest, document_identity);
        update_len_prefixed(&mut digest, source.source.as_bytes());
        digest.update(source.start.to_le_bytes());
        digest.update(source.end.to_le_bytes());
        digest.update(role.code().to_le_bytes());
        Self::from_digest(digest.finalize())
    }

    /// Derive a child identity without depending on its position in the complete deck.
    #[must_use]
    pub fn derive(&self, domain: &[u8], ordinal: u32) -> Self {
        let mut digest = Sha256::new();
        digest.update(b"wasmppt/deck/derived-id/v1\0");
        digest.update(self.0);
        update_len_prefixed(&mut digest, domain);
        digest.update(ordinal.to_le_bytes());
        Self::from_digest(digest.finalize())
    }

    fn from_digest(digest: impl AsRef<[u8]>) -> Self {
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&digest.as_ref()[..16]);
        Self(bytes)
    }
}

impl fmt::Display for StableId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

fn update_len_prefixed(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_le_bytes());
    digest.update(value);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRange {
    /// Host-owned source identity, usually a project-relative Markdown path.
    pub source: String,
    /// Inclusive UTF-8 byte offset.
    pub start: u32,
    /// Exclusive UTF-8 byte offset.
    pub end: u32,
}

impl SourceRange {
    #[must_use]
    pub fn new(source: impl Into<String>, start: u32, end: u32) -> Self {
        Self {
            source: source.into(),
            start,
            end,
        }
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.source.is_empty() && self.start <= self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum SemanticRole {
    Title = 1,
    Subtitle = 2,
    Prose = 3,
    Section = 4,
    List = 5,
    ListItem = 6,
    Figure = 7,
    Caption = 8,
    Gallery = 9,
    Table = 10,
    Chart = 11,
    Code = 12,
    Diagram = 13,
    DisplayMath = 14,
    Quote = 15,
    Credit = 16,
    Definition = 17,
    DefinitionTerm = 18,
    DefinitionDescription = 19,
    Statement = 20,
    TableRow = 21,
    TableCell = 22,
    TableColumn = 23,
}

impl SemanticRole {
    #[must_use]
    pub const fn code(self) -> u16 {
        self as u16
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalSlideKind {
    Title,
    Content,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SplitPolicy {
    Never,
    Text,
    ListItems,
    TableRows,
    CodeLines,
    Children,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckSpec {
    pub id: StableId,
    pub logical_slides: Vec<LogicalSlide>,
    pub resources: Vec<DeckResource>,
}

impl DeckSpec {
    pub const SCHEMA_VERSION: u32 = 3;

    pub fn encode(&self, limits: &DeckLimits) -> Result<Vec<u8>, WireError> {
        wire::encode_spec(self, limits)
    }

    pub fn decode(bytes: &[u8], limits: &DeckLimits) -> Result<Self, WireError> {
        wire::decode_spec(bytes, limits)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogicalSlide {
    pub id: StableId,
    pub source: SourceRange,
    pub kind: LogicalSlideKind,
    pub hidden: bool,
    pub nodes: Vec<SemanticNode>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticNode {
    pub id: StableId,
    pub source: SourceRange,
    pub role: SemanticRole,
    pub split: SplitPolicy,
    pub content: SemanticContent,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticContent {
    Text(RichText),
    Children(Vec<SemanticNode>),
    Image(ImageContent),
    List(ListContent),
    Table(TableContent),
    Chart(ChartContent),
    Code(CodeContent),
    Svg(SvgContent),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RichText {
    pub runs: Vec<RichTextRun>,
}

impl RichText {
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RichTextRun {
    pub text: String,
    pub marks: TextMarks,
    pub hyperlink: Option<SafeHyperlink>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextMarks {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub inline_code: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeHyperlink {
    pub kind: HyperlinkKind,
    pub target: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HyperlinkKind {
    Web,
    Email,
    Telephone,
    SourceAnchor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageContent {
    pub resource_id: StableId,
    pub alt_text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListContent {
    pub ordered: bool,
    pub start: u32,
    pub items: Vec<ListItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    pub id: StableId,
    pub source: SourceRange,
    pub blocks: Vec<SemanticNode>,
    pub children: Vec<ListContent>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableContent {
    pub columns: Vec<TableColumn>,
    pub header_rows: u32,
    pub rows: Vec<TableRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableColumn {
    pub id: StableId,
    pub source: SourceRange,
    pub alignment: TableColumnAlignment,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TableColumnAlignment {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableRow {
    pub id: StableId,
    pub source: SourceRange,
    pub cells: Vec<TableCell>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableCell {
    pub id: StableId,
    pub source: SourceRange,
    pub content: RichText,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartContent {
    pub kind: ChartKind,
    pub categories: Vec<String>,
    pub series: Vec<ChartSeries>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChartKind {
    Bar,
    Column,
    Line,
    Area,
    Pie,
    Doughnut,
    Scatter,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeContent {
    pub language: Option<String>,
    pub code: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SvgContent {
    pub resource_id: StableId,
    /// Original source, retained for math and diagrams when available.
    pub source_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckResource {
    pub id: StableId,
    pub kind: ResourceKind,
    pub media_type: String,
    pub bytes: Vec<u8>,
    /// Optional host-observed hint. Layout validates or derives canonical dimensions from bytes.
    pub intrinsic_size: Option<PixelSize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    RasterImage,
    Svg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EmuSize {
    pub width: Emu,
    pub height: Emu,
}

impl EmuSize {
    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EmuRect {
    pub x: Emu,
    pub y: Emu,
    pub width: Emu,
    pub height: Emu,
}

impl EmuRect {
    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.width > 0 && self.height > 0
    }

    #[must_use]
    pub fn is_within(self, outer: Self) -> bool {
        if !self.is_positive() || !outer.is_positive() {
            return false;
        }
        let Some(right) = self.x.checked_add(self.width) else {
            return false;
        };
        let Some(bottom) = self.y.checked_add(self.height) else {
            return false;
        };
        let Some(outer_right) = outer.x.checked_add(outer.width) else {
            return false;
        };
        let Some(outer_bottom) = outer.y.checked_add(outer.height) else {
            return false;
        };
        self.x >= outer.x && self.y >= outer.y && right <= outer_right && bottom <= outer_bottom
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckTemplatePlan {
    pub id: StableId,
    pub template_hash: [u8; 32],
    /// Hash over the template bytes and every compiler input that can change the plan.
    pub cache_key: [u8; 32],
    pub validator_version: u32,
    pub compiler_policy: String,
    pub page_size: EmuSize,
    pub theme: TemplateTheme,
    pub layouts: Vec<TemplateLayout>,
    pub regions: Vec<TemplateRegion>,
    pub assets: Vec<TemplateAsset>,
    pub diagnostics: Vec<DeckDiagnostic>,
}

impl DeckTemplatePlan {
    pub const SCHEMA_VERSION: u32 = 3;

    pub fn encode(&self, limits: &DeckLimits) -> Result<Vec<u8>, WireError> {
        wire::encode_template_plan(self, limits)
    }

    pub fn decode(bytes: &[u8], limits: &DeckLimits) -> Result<Self, WireError> {
        wire::decode_template_plan(bytes, limits)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TemplateTheme {
    pub major_fonts: ThemeFontSet,
    pub minor_fonts: ThemeFontSet,
    pub colors: Vec<ThemeColor>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThemeFontSet {
    pub latin: Option<String>,
    pub east_asian: Option<String>,
    pub complex_script: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeColor {
    /// DrawingML color-scheme slot such as `accent1` or `dk1`.
    pub slot: String,
    /// Resolved sRGB value. The high byte is red; alpha is intentionally excluded.
    pub rgb: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateLayout {
    pub id: StableId,
    /// Semantic and geometric capability exposed by this compiled layout.
    pub capability: TemplateLayoutCapability,
    pub matching_name: String,
    pub source_part: String,
    pub master_part: String,
    pub region_ids: Vec<StableId>,
    pub asset_ids: Vec<StableId>,
    /// Exact XML range for the effective layout/master background, when present.
    pub background: Option<SourceRange>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateLayoutCapability {
    Title,
    Statement,
    ContentFlow,
    ContentSplit,
    MediaStart,
    MediaEnd,
    Gallery,
    Table,
    Comparison,
}

impl TemplateLayoutCapability {
    /// Capability used when a specialized layout is absent from a valid v2 starter.
    #[must_use]
    pub const fn procedural_fallback(self) -> Self {
        match self {
            Self::Title | Self::Statement | Self::ContentFlow => self,
            Self::ContentSplit
            | Self::MediaStart
            | Self::MediaEnd
            | Self::Gallery
            | Self::Table
            | Self::Comparison => Self::ContentFlow,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateRegion {
    pub id: StableId,
    pub layout_id: StableId,
    pub role: RegionRole,
    pub placeholder: PlaceholderIdentity,
    pub frame: EmuRect,
    pub margins: TextMargins,
    pub text_levels: Vec<TemplateTextLevel>,
    pub accepts: Vec<SemanticRole>,
    pub required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaceholderIdentity {
    /// Standard PresentationML `p:ph/@type`, normalized to its schema default.
    pub kind: String,
    /// Standard PresentationML `p:ph/@idx`, normalized to its schema default.
    pub index: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextMargins {
    pub left: Emu,
    pub top: Emu,
    pub right: Emu,
    pub bottom: Emu,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TemplateTextLevel {
    /// Zero-based list/text hierarchy level.
    pub level: u8,
    /// DrawingML font size in hundredths of a point.
    pub font_size: Option<u32>,
    pub latin_typeface: Option<String>,
    pub east_asian_typeface: Option<String>,
    pub complex_script_typeface: Option<String>,
    pub color: Option<TemplateTextColor>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub margin_left: Option<Emu>,
    pub indent: Option<Emu>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateTextColor {
    pub scheme: Option<String>,
    pub rgb: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateAsset {
    pub id: StableId,
    pub layout_id: StableId,
    pub kind: TemplateAssetKind,
    /// Part containing the exact shape or background XML.
    pub source_part: String,
    /// Exact source range copied from the original POTX when composing output.
    pub source_xml: SourceRange,
    pub frame: Option<EmuRect>,
    pub z_order: u32,
    /// Relationship targets needed by the preserved XML, in deterministic part-name order.
    pub related_parts: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateAssetKind {
    Decoration,
    Logo,
    Footer,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RegionRole {
    Title,
    Subtitle,
    Body,
    Statement,
    Media,
    Caption,
    Table,
    Chart,
    Code,
    Footer,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckPlan {
    pub id: StableId,
    pub spec_id: StableId,
    pub template_id: StableId,
    pub page_size: EmuSize,
    pub pages: Vec<PhysicalPage>,
    pub diagnostics: Vec<DeckDiagnostic>,
}

impl DeckPlan {
    pub const SCHEMA_VERSION: u32 = 4;

    pub fn encode(&self, limits: &DeckLimits) -> Result<Vec<u8>, WireError> {
        wire::encode_plan(self, limits)
    }

    pub fn decode(bytes: &[u8], limits: &DeckLimits) -> Result<Self, WireError> {
        wire::decode_plan(bytes, limits)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalPage {
    pub id: StableId,
    pub logical_slide_id: StableId,
    pub template_layout_id: StableId,
    pub topology: TopologyChoice,
    pub hidden: bool,
    pub continuation: Continuation,
    pub regions: Vec<PlannedRegion>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TopologyChoice {
    pub kind: LayoutTopology,
    pub slot_count: u16,
}

impl TopologyChoice {
    #[must_use]
    pub const fn stack() -> Self {
        Self {
            kind: LayoutTopology::Stack,
            slot_count: 1,
        }
    }

    #[must_use]
    pub const fn is_valid(self) -> bool {
        match self.kind {
            LayoutTopology::Stack | LayoutTopology::TableWide => self.slot_count == 1,
            LayoutTopology::FlowColumns => self.slot_count >= 2 && self.slot_count <= 3,
            LayoutTopology::WeightedSplit
            | LayoutTopology::MediaStart
            | LayoutTopology::MediaEnd
            | LayoutTopology::Comparison => self.slot_count == 2,
            LayoutTopology::PeerGrid | LayoutTopology::Gallery => {
                self.slot_count >= 2 && self.slot_count <= 6
            }
            LayoutTopology::LeadSupporting => self.slot_count == 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutTopology {
    Stack,
    FlowColumns,
    WeightedSplit,
    PeerGrid,
    LeadSupporting,
    MediaStart,
    MediaEnd,
    Gallery,
    TableWide,
    Comparison,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Continuation {
    /// One-based page ordinal within one logical slide.
    pub ordinal: u32,
    /// Total physical pages owned by the same logical slide.
    pub total: u32,
    /// H2/title source repeated as chrome on derived pages; not another source fragment.
    pub repeated_heading_node_id: Option<StableId>,
    /// Minimal derived-page marker, exactly `n/total` when present.
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedRegion {
    pub template_region_id: StableId,
    pub placement: RegionPlacement,
    pub frame: EmuRect,
    pub fragments: Vec<PlannedFragment>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegionPlacement {
    Fixed,
    Slot(u16),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedFragment {
    pub id: StableId,
    pub source_node_id: StableId,
    pub slice: FragmentSlice,
    pub frame: EmuRect,
    pub type_choice: TypeChoice,
    /// Fully resolved picture geometry. Present only for raster-image and SVG fragments.
    pub media: Option<MediaPlacement>,
    /// Header rows repeated before this table continuation fragment.
    pub repeat_table_header_rows: u32,
}

impl PlannedFragment {
    #[must_use]
    pub fn expected_id(source_node_id: StableId, slice: FragmentSlice) -> StableId {
        let mut digest = Sha256::new();
        digest.update(b"wasmppt/deck/planned-fragment/v1\0");
        digest.update(source_node_id.0);
        match slice {
            FragmentSlice::Whole => digest.update([0]),
            FragmentSlice::Text { start, end } => {
                digest.update([1]);
                digest.update(start.to_le_bytes());
                digest.update(end.to_le_bytes());
            }
            FragmentSlice::ListItems { start, end } => {
                digest.update([2]);
                digest.update(start.to_le_bytes());
                digest.update(end.to_le_bytes());
            }
            FragmentSlice::TableRows { start, end } => {
                digest.update([3]);
                digest.update(start.to_le_bytes());
                digest.update(end.to_le_bytes());
            }
            FragmentSlice::CodeLines { start, end } => {
                digest.update([4]);
                digest.update(start.to_le_bytes());
                digest.update(end.to_le_bytes());
            }
        }
        StableId::from_digest(digest.finalize())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FragmentSlice {
    Whole,
    Text { start: u32, end: u32 },
    ListItems { start: u32, end: u32 },
    TableRows { start: u32, end: u32 },
    CodeLines { start: u32, end: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeChoice {
    /// DrawingML font size in hundredths of a point, or zero for non-text content.
    pub font_size: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentFit {
    None,
    Contain,
    Cover,
}

/// Host-neutral picture geometry settled by the planner and consumed verbatim by renderers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaPlacement {
    /// Space allocated by the selected topology before aspect fitting.
    pub slot: EmuRect,
    /// Bounds of the emitted picture shape after aspect fitting.
    pub visible_frame: EmuRect,
    /// Semantic fit policy selected by the planner.
    pub fit: ContentFit,
    /// Canonical display-axis dimensions derived from the source resource.
    pub source_size: PixelSize,
    /// DrawingML source crop in 1/1000 percent units; absent means the complete source is visible.
    pub crop: Option<SourceCrop>,
}

impl MediaPlacement {
    /// Resolve centered contain geometry. Returns `None` for invalid source or slot dimensions.
    #[must_use]
    pub fn contain(slot: EmuRect, source_size: PixelSize) -> Option<Self> {
        valid_media_inputs(slot, source_size).then(|| Self {
            slot,
            visible_frame: contain_media_frame(slot, source_size),
            fit: ContentFit::Contain,
            source_size,
            crop: None,
        })
    }

    /// Resolve centered cover geometry and its exact normalized source crop.
    #[must_use]
    pub fn cover(slot: EmuRect, source_size: PixelSize) -> Option<Self> {
        valid_media_inputs(slot, source_size).then(|| Self {
            slot,
            visible_frame: slot,
            fit: ContentFit::Cover,
            source_size,
            crop: centered_cover_crop(slot, source_size),
        })
    }

    /// Whether every derived value is the canonical result for the recorded inputs.
    #[must_use]
    pub fn is_canonical(self) -> bool {
        let expected = match self.fit {
            ContentFit::Contain => Self::contain(self.slot, self.source_size),
            ContentFit::Cover => Self::cover(self.slot, self.source_size),
            ContentFit::None => None,
        };
        expected == Some(self)
    }
}

/// Centered source crop using DrawingML's 0..100000 coordinate space.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SourceCrop {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

fn valid_media_inputs(slot: EmuRect, source_size: PixelSize) -> bool {
    slot.is_positive() && source_size.width > 0 && source_size.height > 0
}

fn contain_media_frame(slot: EmuRect, source_size: PixelSize) -> EmuRect {
    let slot_width = i128::from(slot.width);
    let slot_height = i128::from(slot.height);
    let source_width = i128::from(source_size.width);
    let source_height = i128::from(source_size.height);
    let (width, height) =
        if source_width.saturating_mul(slot_height) > slot_width.saturating_mul(source_height) {
            (
                slot.width,
                i64::try_from(
                    slot_width
                        .saturating_mul(source_height)
                        .checked_div(source_width)
                        .unwrap_or(1),
                )
                .unwrap_or(1)
                .max(1),
            )
        } else {
            (
                i64::try_from(
                    slot_height
                        .saturating_mul(source_width)
                        .checked_div(source_height)
                        .unwrap_or(1),
                )
                .unwrap_or(1)
                .max(1),
                slot.height,
            )
        };
    EmuRect {
        x: slot.x.saturating_add(slot.width.saturating_sub(width) / 2),
        y: slot
            .y
            .saturating_add(slot.height.saturating_sub(height) / 2),
        width,
        height,
    }
}

fn centered_cover_crop(slot: EmuRect, source_size: PixelSize) -> Option<SourceCrop> {
    let source_width = u128::from(source_size.width);
    let source_height = u128::from(source_size.height);
    let slot_width = u128::try_from(slot.width).ok()?;
    let slot_height = u128::try_from(slot.height).ok()?;
    let source_cross = source_width.saturating_mul(slot_height);
    let slot_cross = slot_width.saturating_mul(source_height);
    if source_cross == slot_cross {
        return None;
    }
    let normalized_side = |lost: u128, total: u128| {
        let rounded = lost
            .saturating_mul(100_000)
            .saturating_add(total)
            .checked_div(total.saturating_mul(2))
            .unwrap_or(0);
        u32::try_from(rounded.min(49_999)).unwrap_or(49_999)
    };
    if source_cross > slot_cross {
        let side = normalized_side(source_cross.saturating_sub(slot_cross), source_cross);
        Some(SourceCrop {
            left: side,
            right: side,
            ..SourceCrop::default()
        })
    } else {
        let side = normalized_side(slot_cross.saturating_sub(source_cross), slot_cross);
        Some(SourceCrop {
            top: side,
            bottom: side,
            ..SourceCrop::default()
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

/// Open diagnostic code wrapper. Unknown future numeric values remain round-trippable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeckDiagnosticCode(pub u16);

impl DeckDiagnosticCode {
    pub const INVALID_SOURCE_RANGE: Self = Self(1);
    pub const DUPLICATE_ID: Self = Self(2);
    pub const MISSING_RESOURCE: Self = Self(3);
    pub const UNSAFE_HYPERLINK: Self = Self(4);
    pub const INVALID_SEMANTIC_CONTENT: Self = Self(5);
    pub const NON_FINITE_CHART_VALUE: Self = Self(6);
    pub const PLAN_SOURCE_LOSS: Self = Self(100);
    pub const PLAN_SOURCE_DUPLICATION: Self = Self(101);
    pub const PLAN_SOURCE_REORDERED: Self = Self(102);
    pub const PLAN_TARGET_DRIFT: Self = Self(103);
    pub const PLAN_INVALID_GEOMETRY: Self = Self(104);
    pub const PLAN_INVALID_CONTINUATION: Self = Self(105);
    pub const PLAN_UNSTABLE_ID: Self = Self(106);
    pub const TEMPLATE_INVALID_PACKAGE: Self = Self(200);
    pub const TEMPLATE_WRONG_CONTENT_TYPE: Self = Self(201);
    pub const TEMPLATE_UNSAFE_CONTENT: Self = Self(202);
    pub const TEMPLATE_INVALID_GRAPH: Self = Self(203);
    pub const TEMPLATE_MISSING_LAYOUT: Self = Self(204);
    pub const TEMPLATE_DUPLICATE_LAYOUT: Self = Self(205);
    pub const TEMPLATE_INVALID_PLACEHOLDER: Self = Self(206);
    pub const TEMPLATE_DUPLICATE_PLACEHOLDER: Self = Self(207);
    pub const TEMPLATE_INVALID_PAGE_SIZE: Self = Self(208);
    pub const TEMPLATE_MISSING_THEME: Self = Self(209);
    pub const TEMPLATE_INVALID_XML: Self = Self(210);
    pub const PLAN_FONT_RISK: Self = Self(300);
    pub const PLAN_ATOMIC_OVERFLOW: Self = Self(301);
    pub const PLAN_WORK_LIMIT: Self = Self(302);
    pub const PLAN_MISSING_LAYOUT: Self = Self(303);

    #[must_use]
    pub const fn known_name(self) -> Option<&'static str> {
        match self.0 {
            1 => Some("invalid-source-range"),
            2 => Some("duplicate-id"),
            3 => Some("missing-resource"),
            4 => Some("unsafe-hyperlink"),
            5 => Some("invalid-semantic-content"),
            6 => Some("non-finite-chart-value"),
            100 => Some("plan-source-loss"),
            101 => Some("plan-source-duplication"),
            102 => Some("plan-source-reordered"),
            103 => Some("plan-target-drift"),
            104 => Some("plan-invalid-geometry"),
            105 => Some("plan-invalid-continuation"),
            106 => Some("plan-unstable-id"),
            200 => Some("template-invalid-package"),
            201 => Some("template-wrong-content-type"),
            202 => Some("template-unsafe-content"),
            203 => Some("template-invalid-graph"),
            204 => Some("template-missing-layout"),
            205 => Some("template-duplicate-layout"),
            206 => Some("template-invalid-placeholder"),
            207 => Some("template-duplicate-placeholder"),
            208 => Some("template-invalid-page-size"),
            209 => Some("template-missing-theme"),
            210 => Some("template-invalid-xml"),
            300 => Some("plan-font-risk"),
            301 => Some("plan-atomic-overflow"),
            302 => Some("plan-work-limit"),
            303 => Some("plan-missing-layout"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeckDiagnostic {
    pub code: DeckDiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: Option<SourceRange>,
    pub node_id: Option<StableId>,
    pub page_id: Option<StableId>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeckLimitCode(pub u16);

impl DeckLimitCode {
    pub const PAYLOAD_BYTES: Self = Self(1);
    pub const STRING_BYTES: Self = Self(2);
    pub const COLLECTION_ITEMS: Self = Self(3);
    pub const SEMANTIC_NODES: Self = Self(4);
    pub const NESTING_DEPTH: Self = Self(5);
    pub const RESOURCE_BYTES: Self = Self(6);
    pub const TOTAL_RESOURCE_BYTES: Self = Self(7);
    pub const PHYSICAL_PAGES: Self = Self(8);
    pub const PLANNED_FRAGMENTS: Self = Self(9);

    #[must_use]
    pub const fn known_name(self) -> Option<&'static str> {
        match self.0 {
            1 => Some("payload-bytes"),
            2 => Some("string-bytes"),
            3 => Some("collection-items"),
            4 => Some("semantic-nodes"),
            5 => Some("nesting-depth"),
            6 => Some("resource-bytes"),
            7 => Some("total-resource-bytes"),
            8 => Some("physical-pages"),
            9 => Some("planned-fragments"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeckLimits {
    pub max_payload_bytes: usize,
    pub max_string_bytes: usize,
    pub max_collection_items: usize,
    pub max_semantic_nodes: usize,
    pub max_nesting_depth: usize,
    pub max_resource_bytes: usize,
    pub max_total_resource_bytes: usize,
    pub max_physical_pages: usize,
    pub max_planned_fragments: usize,
}

impl Default for DeckLimits {
    fn default() -> Self {
        Self {
            max_payload_bytes: 64 * 1024 * 1024,
            max_string_bytes: 4 * 1024 * 1024,
            max_collection_items: 100_000,
            max_semantic_nodes: 100_000,
            max_nesting_depth: 64,
            max_resource_bytes: 32 * 1024 * 1024,
            max_total_resource_bytes: 64 * 1024 * 1024,
            max_physical_pages: 10_000,
            max_planned_fragments: 1_000_000,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ValidationReport {
    pub diagnostics: Vec<DeckDiagnostic>,
}

impl ValidationReport {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }
}
