use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    ops::Range,
    sync::Arc,
};

use sha2::{Digest, Sha256};
use wasmppt_opc::{
    CompressionMethod, Entry, EntryOptions, Error as PackageError, ErrorCode as PackageErrorCode,
    MemorySource, OutputSink, PackageGraph, PackagePartSource, PartId, ReadAt, RelationshipTarget,
    RewriteMode, StreamingZipWriter, VecSink, WriteStats, ZipArchive, ZipWriter,
};
use wasmppt_pml::SlideView;
use wasmppt_xml::{TokenKind, XmlDocument};

use crate::{
    BindingKind, BindingTarget, MacroPolicy, RelationshipAction, TemplatePlan,
    policy::{is_template_main_type, prohibited_content, prohibited_part},
};

mod patch;
mod table;

use patch::{
    Patch, apply_patches, cleanup_patches, escape_xml_attribute, escape_xml_text, missing_value,
    relationship_part_name, relationship_source, relative_patches, resolve_target, text_patches,
};
use table::{apply_table_policy, table_row_height_patch};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImageCrop {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageData {
    pub bytes: Arc<[u8]>,
    pub extension: String,
    pub content_type: String,
    pub crop: Option<ImageCrop>,
    pub fit: ImageFitPolicy,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImageFitPolicy {
    #[default]
    Preserve,
    Cover,
    Contain,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RichTextRunData {
    pub text: String,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub font_size: Option<i32>,
    pub color: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SemanticShapeData {
    pub visible: Option<bool>,
    pub copies: Option<u32>,
    pub rich_text: Option<Vec<RichTextRunData>>,
    pub hyperlink: Option<String>,
    pub fill_color: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartSeriesData {
    pub name: String,
    pub values: Vec<f64>,
}

impl Eq for ChartSeriesData {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChartData {
    pub categories: Vec<String>,
    pub series: Vec<ChartSeriesData>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableOverflowPolicy {
    Fail,
    Clip,
    Shrink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TablePolicyData {
    pub maximum_rows: u32,
    pub overflow: TableOverflowPolicy,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// Complete logical values for one generation, or a partial delta for a live session.
pub struct InjectionData {
    text: BTreeMap<String, String>,
    images: BTreeMap<String, ImageData>,
    table_rows: BTreeMap<String, Vec<BTreeMap<String, String>>>,
    table_policies: BTreeMap<String, TablePolicyData>,
    slide_copies: BTreeMap<String, usize>,
    charts: BTreeMap<String, ChartData>,
    semantic_shapes: BTreeMap<String, SemanticShapeData>,
    notes: BTreeMap<String, String>,
}

impl InjectionData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_text(&mut self, id: impl Into<String>, value: impl Into<String>) {
        self.text.insert(id.into(), value.into());
    }

    pub fn with_text(mut self, id: impl Into<String>, value: impl Into<String>) -> Self {
        self.insert_text(id, value);
        self
    }

    pub fn insert_image(&mut self, id: impl Into<String>, image: ImageData) {
        self.images.insert(id.into(), image);
    }

    pub fn with_image(mut self, id: impl Into<String>, image: ImageData) -> Self {
        self.insert_image(id, image);
        self
    }

    pub fn set_table_rows(&mut self, id: impl Into<String>, rows: Vec<BTreeMap<String, String>>) {
        self.table_rows.insert(id.into(), rows);
    }

    pub fn with_table_rows(
        mut self,
        id: impl Into<String>,
        rows: Vec<BTreeMap<String, String>>,
    ) -> Self {
        self.set_table_rows(id, rows);
        self
    }

    pub fn set_table_policy(&mut self, id: impl Into<String>, policy: TablePolicyData) {
        self.table_policies.insert(id.into(), policy);
    }

    pub fn with_table_policy(mut self, id: impl Into<String>, policy: TablePolicyData) -> Self {
        self.set_table_policy(id, policy);
        self
    }

    /// Set the number of copies of a source slide. Zero excludes it.
    pub fn set_slide_copies(&mut self, part_name: impl Into<String>, copies: usize) {
        self.slide_copies.insert(part_name.into(), copies);
    }

    pub fn with_slide_copies(mut self, part_name: impl Into<String>, copies: usize) -> Self {
        self.set_slide_copies(part_name, copies);
        self
    }

    /// Replace a supported chart cache and its related embedded workbook atomically.
    pub fn set_chart(&mut self, chart_part_name: impl Into<String>, chart: ChartData) {
        self.charts.insert(chart_part_name.into(), chart);
    }

    pub fn with_chart(mut self, chart_part_name: impl Into<String>, chart: ChartData) -> Self {
        self.set_chart(chart_part_name, chart);
        self
    }

    pub fn set_semantic_shape(&mut self, id: impl Into<String>, shape: SemanticShapeData) {
        self.semantic_shapes.insert(id.into(), shape);
    }

    pub fn with_semantic_shape(mut self, id: impl Into<String>, shape: SemanticShapeData) -> Self {
        self.set_semantic_shape(id, shape);
        self
    }

    pub fn set_notes(&mut self, slide_part_name: impl Into<String>, text: impl Into<String>) {
        self.notes.insert(slide_part_name.into(), text.into());
    }

    pub fn with_notes(
        mut self,
        slide_part_name: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        self.set_notes(slide_part_name, text);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
/// Stable machine category for generation and live-session failures.
pub enum GenerateErrorCode {
    InvalidTemplate,
    IncompletePlan,
    PlanMismatch,
    MissingValue,
    InvalidBindingRange,
    Package,
    Xml,
    InvalidImage,
    InvalidChart,
    InvalidTable,
    InvalidRevision,
    MacroPresent,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Generation failure with an optional stable lower-level cause code.
pub struct GenerateError {
    code: GenerateErrorCode,
    message: String,
    cause_code: Option<&'static str>,
}

impl GenerateError {
    fn new(code: GenerateErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            cause_code: None,
        }
    }

    fn xml(error: wasmppt_xml::XmlError) -> Self {
        Self {
            code: GenerateErrorCode::Xml,
            message: error.to_string(),
            cause_code: Some(super::xml_error_code(error.code())),
        }
    }

    fn xml_in_part(error: wasmppt_xml::XmlError, name: &str) -> Self {
        let mut output = Self::xml(error);
        output.message = format!("{name}: {}", output.message);
        output
    }

    fn pml(error: wasmppt_pml::PmlError) -> Self {
        Self {
            code: GenerateErrorCode::Xml,
            message: error.to_string(),
            cause_code: error.cause_code(),
        }
    }

    pub const fn code(&self) -> GenerateErrorCode {
        self.code
    }

    pub const fn cause_code(&self) -> Option<&'static str> {
        self.cause_code
    }
}

impl std::fmt::Display for GenerateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for GenerateError {}

#[derive(Clone, Debug)]
struct ImagePlan {
    relationship_part: String,
    relationship_target_range: Range<usize>,
    original_media_part: String,
    original_reference_count: usize,
    crop: CropPlan,
}

#[derive(Clone, Debug)]
struct TablePlan {
    part_name: String,
    row_range: Range<usize>,
    bindings: Vec<BindingTarget>,
}

#[derive(Clone, Debug)]
struct ChartPlan {
    chart_part: String,
    workbook_part: Option<String>,
}

#[derive(Clone, Debug)]
struct SemanticShapePlan {
    part_name: String,
    shape_range: Range<usize>,
    shape_id_range: Option<Range<usize>>,
    paragraph_content_range: Option<Range<usize>>,
    fill_color_range: Option<Range<usize>>,
    hyperlink_target: Option<(String, Range<usize>)>,
}

#[derive(Clone, Debug)]
struct NotesPlan {
    part_name: String,
    text_ranges: Vec<Range<usize>>,
}

#[derive(Clone, Debug)]
struct SlideRecord {
    part_name: String,
    slide_id: u32,
    list_range: Range<usize>,
    list_prefix: String,
    list_relationship_prefix: String,
    relationship_range: Range<usize>,
    relationship_type: String,
}

#[derive(Clone, Debug)]
struct SlideDeckPlan {
    presentation_part: String,
    relationship_part: String,
    relationship_insert_offset: usize,
    slides: Vec<SlideRecord>,
    used_relationship_ids: HashSet<String>,
    used_slide_parts: HashSet<String>,
    content_type_insert_offset: usize,
    content_types: HashMap<String, (String, Range<usize>)>,
}

type PreparedSemanticShapes = (HashMap<String, SemanticShapePlan>, HashMap<String, u32>);

#[derive(Clone, Debug, Default)]
struct SlideOperations {
    presentation_patches: Vec<Patch>,
    relationship_patches: Vec<Patch>,
    content_type_patches: Vec<Patch>,
    removed_parts: HashSet<String>,
    clones: Vec<SlideClone>,
}

#[derive(Clone, Debug)]
struct SlideClone {
    source_part: String,
    part_name: String,
    source_relationship_part: Option<String>,
    relationship_part: Option<String>,
}

#[derive(Clone, Debug)]
enum CropPlan {
    Existing {
        left: Option<Range<usize>>,
        top: Option<Range<usize>>,
        right: Option<Range<usize>>,
        bottom: Option<Range<usize>>,
        element_range: Range<usize>,
        prefix: String,
    },
    Insert {
        offset: usize,
        prefix: String,
    },
    None,
}

/// Immutable compiled template and cached dirty-part planning state.
#[derive(Debug)]
pub struct PreparedTemplate {
    archive: ZipArchive<wasmppt_opc::MemorySource>,
    plan: TemplatePlan,
    cached_parts: HashMap<String, Vec<u8>>,
    static_patches: HashMap<String, Vec<Patch>>,
    removed_parts: HashSet<String>,
    image_plans: HashMap<String, ImagePlan>,
    table_plans: HashMap<String, TablePlan>,
    chart_plans: HashMap<String, ChartPlan>,
    semantic_shape_plans: HashMap<String, SemanticShapePlan>,
    notes_plans: HashMap<String, NotesPlan>,
    maximum_shape_ids: HashMap<String, u32>,
    slide_deck: SlideDeckPlan,
}

#[derive(Clone, Debug)]
struct OverlayEntry {
    bytes: Arc<[u8]>,
    options: EntryOptions,
}

/// One immutable logical package revision prepared from a compiled template.
///
/// Unchanged parts stay in the shared source ZIP. Only rewritten or new parts are
/// retained here, so layout resolution can consume this view without serializing and
/// reopening a complete PPTX.
#[derive(Clone, Debug)]
pub struct PreparedOverlay {
    archive: ZipArchive<MemorySource>,
    order: Vec<String>,
    names: HashSet<String>,
    overrides: BTreeMap<String, OverlayEntry>,
    rewritten_entries: u64,
    removed_entries: u64,
    dirty_uncompressed_bytes: u64,
    peak_dirty_entry_bytes: u64,
    graph_fingerprint: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverlayStats {
    pub logical_parts: u64,
    pub materialized_parts: u64,
    pub materialized_bytes: u64,
    pub reused_source_bytes: u64,
    pub removed_parts: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveSessionUpdate {
    pub revision: u32,
    pub changed_bindings: Vec<String>,
    pub changed_parts: Vec<String>,
    pub graph_changed: bool,
    pub reused_materialized_parts: u64,
    pub overlay_stats: OverlayStats,
}

#[derive(Debug)]
pub struct LiveSession {
    prepared: Arc<PreparedTemplate>,
    data: InjectionData,
    revision: u32,
    overlay: Arc<PreparedOverlay>,
}

#[derive(Debug, Default)]
struct InjectionUndo {
    text: BTreeMap<String, Option<String>>,
    images: BTreeMap<String, Option<ImageData>>,
    table_rows: BTreeMap<String, Option<Vec<BTreeMap<String, String>>>>,
    table_policies: BTreeMap<String, Option<TablePolicyData>>,
    slide_copies: BTreeMap<String, Option<usize>>,
    charts: BTreeMap<String, Option<ChartData>>,
    semantic_shapes: BTreeMap<String, Option<SemanticShapeData>>,
    notes: BTreeMap<String, Option<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerateOutput {
    pub bytes: Vec<u8>,
    pub zip_stats: WriteStats,
    pub rewritten_entries: u64,
    pub removed_entries: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerateStats {
    pub zip: WriteStats,
    pub rewritten_entries: u64,
    pub removed_entries: u64,
    pub dirty_uncompressed_bytes: u64,
    pub peak_dirty_entry_bytes: u64,
    pub maximum_output_chunk_bytes: u64,
}

#[derive(Debug)]
pub struct GenerationCursor {
    writer: StreamingZipWriter<wasmppt_opc::MemorySource>,
    entries: VecDeque<GenerationEntry>,
    rewritten_entries: u64,
    removed_entries: u64,
    finish_started: bool,
    dirty_uncompressed_bytes: u64,
    peak_dirty_entry_bytes: u64,
    maximum_output_chunk_bytes: u64,
}

#[derive(Debug)]
enum GenerationEntry {
    Raw(Entry),
    Owned {
        name: String,
        bytes: Arc<[u8]>,
        options: EntryOptions,
    },
}

impl GenerationCursor {
    pub fn pull(&mut self, maximum_bytes: usize) -> Result<Vec<u8>, GenerateError> {
        if maximum_bytes == 0 {
            return Err(GenerateError::new(
                GenerateErrorCode::Package,
                "generation chunk size must be positive",
            ));
        }
        let mut output = Vec::with_capacity(maximum_bytes);
        while output.len() < maximum_bytes && !self.writer.is_done() {
            if !self.writer.entry_active() && !self.finish_started {
                match self.entries.pop_front() {
                    Some(GenerationEntry::Raw(entry)) => self
                        .writer
                        .start_raw_copy(&entry, RewriteMode::Preserve)
                        .map_err(package_error)?,
                    Some(GenerationEntry::Owned {
                        name,
                        bytes,
                        options,
                    }) => self
                        .writer
                        .start_shared_entry(name, bytes, options)
                        .map_err(package_error)?,
                    None => {
                        self.writer.start_finish().map_err(package_error)?;
                        self.finish_started = true;
                    }
                }
            }
            let chunk = self
                .writer
                .pull(maximum_bytes - output.len())
                .map_err(package_error)?;
            if chunk.is_empty() && self.writer.is_done() {
                break;
            }
            output.extend(chunk);
        }
        self.maximum_output_chunk_bytes = self.maximum_output_chunk_bytes.max(output.len() as u64);
        Ok(output)
    }

    pub fn is_done(&self) -> bool {
        self.writer.is_done()
    }

    pub fn stats(&self) -> Option<GenerateStats> {
        self.is_done().then(|| GenerateStats {
            zip: self.writer.stats(),
            rewritten_entries: self.rewritten_entries,
            removed_entries: self.removed_entries,
            dirty_uncompressed_bytes: self.dirty_uncompressed_bytes,
            peak_dirty_entry_bytes: self.peak_dirty_entry_bytes,
            maximum_output_chunk_bytes: self.maximum_output_chunk_bytes,
        })
    }
}

impl PreparedOverlay {
    #[allow(clippy::too_many_arguments)]
    fn from_entries(
        archive: ZipArchive<MemorySource>,
        entries: VecDeque<GenerationEntry>,
        rewritten_entries: u64,
        removed_entries: u64,
        dirty_uncompressed_bytes: u64,
        peak_dirty_entry_bytes: u64,
    ) -> Result<Self, GenerateError> {
        let mut order = Vec::with_capacity(entries.len());
        let mut overrides = BTreeMap::new();
        for entry in entries {
            match entry {
                GenerationEntry::Raw(entry) => order.push(entry.name),
                GenerationEntry::Owned {
                    name,
                    bytes,
                    options,
                } => {
                    order.push(name.clone());
                    if overrides
                        .insert(name.clone(), OverlayEntry { bytes, options })
                        .is_some()
                    {
                        return Err(GenerateError::new(
                            GenerateErrorCode::InvalidTemplate,
                            format!("duplicate logical package part {name}"),
                        ));
                    }
                }
            }
        }
        let names = order.iter().cloned().collect();
        let mut overlay = Self {
            archive,
            order,
            names,
            overrides,
            rewritten_entries,
            removed_entries,
            dirty_uncompressed_bytes,
            peak_dirty_entry_bytes,
            graph_fingerprint: [0; 32],
        };
        overlay.graph_fingerprint = overlay.compute_graph_fingerprint()?;
        Ok(overlay)
    }

    pub fn generation_cursor(&self) -> GenerationCursor {
        let mut entries = VecDeque::with_capacity(self.order.len());
        for name in &self.order {
            if let Some(entry) = self.overrides.get(name) {
                entries.push_back(GenerationEntry::Owned {
                    name: name.clone(),
                    bytes: entry.bytes.clone(),
                    options: entry.options.clone(),
                });
            } else if let Some(entry) = self.archive.entry(name) {
                entries.push_back(GenerationEntry::Raw(entry.clone()));
            }
        }
        GenerationCursor {
            writer: StreamingZipWriter::new(self.archive.source().clone()),
            entries,
            rewritten_entries: self.rewritten_entries,
            removed_entries: self.removed_entries,
            finish_started: false,
            dirty_uncompressed_bytes: self.dirty_uncompressed_bytes,
            peak_dirty_entry_bytes: self.peak_dirty_entry_bytes,
            maximum_output_chunk_bytes: 0,
        }
    }

    pub fn stats(&self) -> OverlayStats {
        let reused_source_bytes = self
            .order
            .iter()
            .filter(|name| !self.overrides.contains_key(*name))
            .filter_map(|name| self.archive.entry(name))
            .map(|entry| entry.compressed_size)
            .sum();
        OverlayStats {
            logical_parts: self.order.len() as u64,
            materialized_parts: self.overrides.len() as u64,
            materialized_bytes: self.dirty_uncompressed_bytes,
            reused_source_bytes,
            removed_parts: self.removed_entries,
        }
    }

    pub const fn graph_fingerprint(&self) -> [u8; 32] {
        self.graph_fingerprint
    }

    pub fn part_fingerprint(&self, name: &str) -> Result<[u8; 32], GenerateError> {
        let bytes = self.read_part(name).map_err(package_error)?;
        Ok(Sha256::digest(bytes).into())
    }

    /// Exact logical parts whose bytes or presence differ from another revision of
    /// the same prepared template. Source-only parts need no comparison because their
    /// shared immutable ZIP bytes cannot change.
    pub fn changed_parts_since(&self, previous: &Self) -> Vec<String> {
        let mut candidates = self
            .overrides
            .keys()
            .chain(previous.overrides.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let current_names = self.order.iter().cloned().collect::<BTreeSet<_>>();
        let previous_names = previous.order.iter().cloned().collect::<BTreeSet<_>>();
        candidates.extend(current_names.symmetric_difference(&previous_names).cloned());
        candidates
            .into_iter()
            .filter(|name| {
                if let (Some(current), Some(old)) =
                    (self.overrides.get(name), previous.overrides.get(name))
                {
                    if Arc::ptr_eq(&current.bytes, &old.bytes) {
                        return false;
                    }
                }
                match (self.logical_bytes(name), previous.logical_bytes(name)) {
                    (Some(current), Some(old)) => current != old,
                    (None, None) => false,
                    _ => true,
                }
            })
            .collect()
    }

    fn generation_entry(&self, name: &str) -> Option<GenerationEntry> {
        if !self.names.contains(name) {
            return None;
        }
        self.overrides.get(name).map_or_else(
            || self.archive.entry(name).cloned().map(GenerationEntry::Raw),
            |entry| {
                Some(GenerationEntry::Owned {
                    name: name.to_owned(),
                    bytes: entry.bytes.clone(),
                    options: entry.options.clone(),
                })
            },
        )
    }

    fn shared_materialized_parts_with(&self, previous: &Self) -> u64 {
        self.overrides
            .iter()
            .filter(|(name, current)| {
                previous
                    .overrides
                    .get(*name)
                    .is_some_and(|old| Arc::ptr_eq(&current.bytes, &old.bytes))
            })
            .count() as u64
    }

    fn logical_bytes(&self, name: &str) -> Option<&[u8]> {
        if !self.names.contains(name) {
            return None;
        }
        self.overrides
            .get(name)
            .map(|entry| entry.bytes.as_ref())
            .or_else(|| {
                self.archive.entry(name).and_then(|entry| {
                    let range = entry.compressed_range();
                    (entry.compression == CompressionMethod::Stored)
                        .then(|| {
                            self.archive
                                .source()
                                .as_bytes()
                                .get(range.start as usize..range.end as usize)
                        })
                        .flatten()
                })
            })
    }

    fn compute_graph_fingerprint(&self) -> Result<[u8; 32], GenerateError> {
        let mut hasher = Sha256::new();
        for name in self.order.iter().filter(|name| graph_identity_part(name)) {
            let bytes = self.read_part(name).map_err(package_error)?;
            hasher.update((name.len() as u64).to_le_bytes());
            hasher.update(name.as_bytes());
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        }
        Ok(hasher.finalize().into())
    }
}

impl PackagePartSource for PreparedOverlay {
    fn part_names(&self) -> Vec<String> {
        self.order.clone()
    }

    fn contains_part(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    fn is_modified(&self, name: &str) -> bool {
        self.overrides.contains_key(name)
    }

    fn read_part(&self, name: &str) -> wasmppt_opc::Result<Vec<u8>> {
        if !self.names.contains(name) {
            return Err(PackageError::new(
                PackageErrorCode::InvalidField,
                format!("logical package part not found: {name}"),
            ));
        }
        if let Some(entry) = self.overrides.get(name) {
            return Ok(entry.bytes.to_vec());
        }
        let entry = self.archive.entry(name).ok_or_else(|| {
            PackageError::new(
                PackageErrorCode::InvalidField,
                format!("source package part not found: {name}"),
            )
        })?;
        self.archive.read_entry(entry)
    }
}

impl LiveSession {
    fn new(prepared: Arc<PreparedTemplate>, data: InjectionData) -> Result<Self, GenerateError> {
        let overlay = Arc::new(prepared.prepare_overlay(&data)?);
        PackageGraph::build_from(overlay.as_ref()).map_err(|error| {
            GenerateError::new(
                GenerateErrorCode::Package,
                format!("invalid initial live package graph: {error}"),
            )
        })?;
        Ok(Self {
            prepared,
            data,
            revision: 0,
            overlay,
        })
    }

    pub const fn revision(&self) -> u32 {
        self.revision
    }

    pub fn overlay(&self) -> Arc<PreparedOverlay> {
        self.overlay.clone()
    }

    pub fn estimated_resident_bytes(&self) -> u64 {
        self.prepared
            .estimated_resident_bytes()
            .saturating_add(self.overlay.stats().materialized_bytes)
            .saturating_add(injection_data_weight(&self.data))
    }

    /// Atomically merge a compact partial data update into the current revision.
    ///
    /// Only keys present in `delta` are replaced. Optional values are reset by sending
    /// their explicit empty/default representation. The next revision must be exactly
    /// one greater than the expected current revision.
    pub fn apply_delta(
        &mut self,
        expected_revision: u32,
        next_revision: u32,
        delta: InjectionData,
    ) -> Result<LiveSessionUpdate, GenerateError> {
        self.apply_delta_validated(expected_revision, next_revision, delta, |_, _| Ok(()))
    }

    /// Apply a delta only if a host-specific logical-view validator also succeeds.
    ///
    /// The validator runs after package-graph validation but before commit. Its error
    /// triggers the same undo log as an injection failure, allowing a Wasm host to
    /// validate a new layout index without cloning complete session data.
    pub fn apply_delta_validated(
        &mut self,
        expected_revision: u32,
        next_revision: u32,
        delta: InjectionData,
        validate: impl FnOnce(Arc<PreparedOverlay>, bool) -> Result<(), String>,
    ) -> Result<LiveSessionUpdate, GenerateError> {
        if expected_revision != self.revision
            || next_revision
                != expected_revision.checked_add(1).ok_or_else(|| {
                    GenerateError::new(
                        GenerateErrorCode::InvalidRevision,
                        "live session revision is exhausted",
                    )
                })?
        {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidRevision,
                format!(
                    "live delta expected revision {expected_revision} -> {next_revision}, current revision is {}",
                    self.revision
                ),
            ));
        }

        let changed_bindings = injection_delta_keys(&delta);
        let affected_parts = self.prepared.incremental_affected_parts(&delta);
        let changed_binding_set = changed_bindings.iter().cloned().collect::<BTreeSet<_>>();
        let undo = merge_injection_data(&mut self.data, delta);
        let next_overlay = match self.prepared.prepare_overlay_reusing(
            &self.data,
            &self.overlay,
            affected_parts.as_ref(),
            &changed_binding_set,
        ) {
            Ok(overlay) => Arc::new(overlay),
            Err(error) => {
                rollback_injection_data(&mut self.data, undo);
                return Err(error);
            }
        };
        if let Err(error) = PackageGraph::build_from(next_overlay.as_ref()) {
            rollback_injection_data(&mut self.data, undo);
            return Err(GenerateError::new(
                GenerateErrorCode::Package,
                format!("invalid live package graph: {error}"),
            ));
        }
        let changed_parts = next_overlay.changed_parts_since(&self.overlay);
        let graph_changed = next_overlay.graph_fingerprint() != self.overlay.graph_fingerprint();
        let reused_materialized_parts = next_overlay.shared_materialized_parts_with(&self.overlay);
        if let Err(error) = validate(next_overlay.clone(), graph_changed) {
            rollback_injection_data(&mut self.data, undo);
            return Err(GenerateError::new(
                GenerateErrorCode::Package,
                format!("live overlay validation failed: {error}"),
            ));
        }
        self.revision = next_revision;
        self.overlay = next_overlay;
        Ok(LiveSessionUpdate {
            revision: self.revision,
            changed_bindings,
            changed_parts,
            graph_changed,
            reused_materialized_parts,
            overlay_stats: self.overlay.stats(),
        })
    }

    pub fn generation_cursor(&self) -> GenerationCursor {
        self.overlay.generation_cursor()
    }
}

fn injection_delta_keys(delta: &InjectionData) -> Vec<String> {
    delta
        .text
        .keys()
        .chain(delta.images.keys())
        .chain(delta.table_rows.keys())
        .chain(delta.table_policies.keys())
        .chain(delta.slide_copies.keys())
        .chain(delta.charts.keys())
        .chain(delta.semantic_shapes.keys())
        .chain(delta.notes.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn merge_injection_data(target: &mut InjectionData, delta: InjectionData) -> InjectionUndo {
    InjectionUndo {
        text: merge_map(&mut target.text, delta.text),
        images: merge_map(&mut target.images, delta.images),
        table_rows: merge_map(&mut target.table_rows, delta.table_rows),
        table_policies: merge_map(&mut target.table_policies, delta.table_policies),
        slide_copies: merge_map(&mut target.slide_copies, delta.slide_copies),
        charts: merge_map(&mut target.charts, delta.charts),
        semantic_shapes: merge_map(&mut target.semantic_shapes, delta.semantic_shapes),
        notes: merge_map(&mut target.notes, delta.notes),
    }
}

fn merge_map<Value>(
    target: &mut BTreeMap<String, Value>,
    updates: BTreeMap<String, Value>,
) -> BTreeMap<String, Option<Value>> {
    updates
        .into_iter()
        .map(|(key, value)| {
            let previous = target.insert(key.clone(), value);
            (key, previous)
        })
        .collect()
}

fn rollback_injection_data(target: &mut InjectionData, undo: InjectionUndo) {
    rollback_map(&mut target.text, undo.text);
    rollback_map(&mut target.images, undo.images);
    rollback_map(&mut target.table_rows, undo.table_rows);
    rollback_map(&mut target.table_policies, undo.table_policies);
    rollback_map(&mut target.slide_copies, undo.slide_copies);
    rollback_map(&mut target.charts, undo.charts);
    rollback_map(&mut target.semantic_shapes, undo.semantic_shapes);
    rollback_map(&mut target.notes, undo.notes);
}

fn rollback_map<Value>(
    target: &mut BTreeMap<String, Value>,
    previous: BTreeMap<String, Option<Value>>,
) {
    for (key, value) in previous {
        if let Some(value) = value {
            target.insert(key, value);
        } else {
            target.remove(&key);
        }
    }
}

fn injection_data_weight(data: &InjectionData) -> u64 {
    fn strings(map: &BTreeMap<String, String>) -> u64 {
        map.iter()
            .map(|(key, value)| (key.len() + value.len()) as u64)
            .sum()
    }
    let images = data
        .images
        .iter()
        .map(|(key, image)| {
            (key.len() + image.extension.len() + image.content_type.len() + image.bytes.len())
                as u64
        })
        .sum::<u64>();
    strings(&data.text)
        .saturating_add(strings(&data.notes))
        .saturating_add(images)
}

fn graph_identity_part(name: &str) -> bool {
    name == "[Content_Types].xml"
        || name.ends_with(".rels")
        || name.ends_with("/presentation.xml")
        || name == "ppt/presentation.xml"
}

impl PreparedTemplate {
    /// Validate that `plan` belongs to the exact template bytes and prepare warm generation state.
    pub fn new(bytes: impl Into<Arc<[u8]>>, plan: TemplatePlan) -> Result<Self, GenerateError> {
        if !plan.completeness.graph_valid
            || !plan.completeness.bindings_unambiguous
            || !plan.completeness.raw_copy_partition_complete
            || !plan.completeness.unknown_markup_preserved
        {
            return Err(GenerateError::new(
                GenerateErrorCode::IncompletePlan,
                "TemplatePlan completeness proof is false",
            ));
        }
        let bytes = bytes.into();
        let actual_hash: [u8; 32] = Sha256::digest(&bytes).into();
        if actual_hash != plan.identity.template_sha256 {
            return Err(GenerateError::new(
                GenerateErrorCode::PlanMismatch,
                "TemplatePlan source hash does not match template bytes",
            ));
        }
        let archive = ZipArchive::from_bytes(bytes).map_err(package_error)?;
        if plan.identity.macro_policy == MacroPolicy::Reject {
            if let Some(reason) = prohibited_content(&archive).map_err(|message| {
                GenerateError::new(GenerateErrorCode::InvalidTemplate, message)
            })? {
                return Err(GenerateError::new(GenerateErrorCode::MacroPresent, reason));
            }
        }
        let removed_parts = archive
            .entries()
            .iter()
            .filter(|entry| prohibited_part(&entry.name))
            .map(|entry| entry.name.clone())
            .collect::<HashSet<_>>();
        let binding_parts = plan
            .bindings
            .iter()
            .map(|binding| binding.part_name.as_str())
            .collect::<HashSet<_>>();
        let image_plans = prepare_image_plans(&archive, &plan)?;
        let table_plans = prepare_table_plans(&archive, &plan)?;
        let chart_plans = prepare_chart_plans(&archive, &plan)?;
        let slide_deck = prepare_slide_deck(&archive)?;
        let graph = PackageGraph::build(&archive).map_err(|error| {
            GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                format!("cannot build package graph: {error}"),
            )
        })?;
        let default_background_patches = graph
            .part_by_name(&slide_deck.presentation_part)
            .and_then(|part| graph.content_type(part))
            .filter(|content_type| is_template_main_type(content_type))
            .map(|_| prepare_default_background_patches(&archive, &graph, &slide_deck))
            .transpose()?
            .unwrap_or_default();
        let (semantic_shape_plans, maximum_shape_ids) =
            prepare_semantic_shape_plans(&archive, &plan)?;
        let notes_plans = prepare_notes_plans(&archive)?;
        let image_relationship_parts = image_plans
            .values()
            .map(|plan| plan.relationship_part.as_str())
            .collect::<HashSet<_>>();
        let chart_parts = chart_plans
            .values()
            .flat_map(|plan| {
                std::iter::once(plan.chart_part.as_str()).chain(plan.workbook_part.as_deref())
            })
            .collect::<HashSet<_>>();
        let semantic_parts = semantic_shape_plans
            .values()
            .flat_map(|plan| {
                std::iter::once(plan.part_name.as_str()).chain(
                    plan.hyperlink_target
                        .as_ref()
                        .map(|(part, _)| part.as_str()),
                )
            })
            .chain(notes_plans.values().map(|plan| plan.part_name.as_str()))
            .collect::<HashSet<_>>();
        let mut cached_parts = HashMap::new();
        let mut static_patches = HashMap::new();
        for entry in archive.entries() {
            if removed_parts.contains(&entry.name) {
                continue;
            }
            let scan = entry.name == "[Content_Types].xml"
                || entry.name.ends_with(".rels")
                || entry.name.ends_with(".xml")
                || binding_parts.contains(entry.name.as_str())
                || chart_parts.contains(entry.name.as_str())
                || semantic_parts.contains(entry.name.as_str());
            if !scan {
                continue;
            }
            let source = archive.read_entry(entry).map_err(package_error)?;
            let mut patches = if entry.name == "[Content_Types].xml"
                || entry.name.ends_with(".rels")
                || entry.name.ends_with(".xml")
            {
                cleanup_patches(&entry.name, &source, &removed_parts)?
            } else {
                Vec::new()
            };
            if let Some(patch) = default_background_patches.get(&entry.name) {
                patches.push(patch.clone());
            }
            if !patches.is_empty()
                || binding_parts.contains(entry.name.as_str())
                || image_relationship_parts.contains(entry.name.as_str())
                || entry.name == "[Content_Types].xml"
                || entry.name == slide_deck.presentation_part
                || entry.name == slide_deck.relationship_part
                || chart_parts.contains(entry.name.as_str())
                || semantic_parts.contains(entry.name.as_str())
                || slide_deck.used_slide_parts.contains(&entry.name)
                || slide_deck.used_slide_parts.iter().any(|part| {
                    relationship_part_name(part).as_deref() == Some(entry.name.as_str())
                })
            {
                cached_parts.insert(entry.name.clone(), source);
            }
            if !patches.is_empty() {
                static_patches.insert(entry.name.clone(), patches);
            }
        }
        Ok(Self {
            archive,
            plan,
            cached_parts,
            static_patches,
            removed_parts,
            image_plans,
            table_plans,
            chart_plans,
            semantic_shape_plans,
            notes_plans,
            maximum_shape_ids,
            slide_deck,
        })
    }

    /// Return the validated immutable plan owned by this prepared template.
    pub fn plan(&self) -> &TemplatePlan {
        &self.plan
    }

    /// Start a revision-zero live session. The session retains this prepared template by `Arc`.
    pub fn start_live_session(
        self: &Arc<Self>,
        data: InjectionData,
    ) -> Result<LiveSession, GenerateError> {
        LiveSession::new(self.clone(), data)
    }

    fn incremental_affected_parts(&self, delta: &InjectionData) -> Option<HashSet<String>> {
        if !delta.slide_copies.is_empty() {
            return None;
        }
        let mut affected = HashSet::new();
        let direct_ids = delta
            .text
            .keys()
            .chain(delta.images.keys())
            .collect::<HashSet<_>>();
        for binding in self
            .plan
            .bindings
            .iter()
            .filter(|binding| direct_ids.contains(&binding.id))
        {
            affected.insert(binding.part_name.clone());
            if binding.kind == BindingKind::Image {
                affected.insert("[Content_Types].xml".to_owned());
                if let Some(plan) = self.image_plans.get(&binding.id) {
                    affected.insert(plan.relationship_part.clone());
                    affected.insert(plan.original_media_part.clone());
                }
            }
        }
        for id in delta.table_rows.keys().chain(delta.table_policies.keys()) {
            if let Some(plan) = self.table_plans.get(id) {
                affected.insert(plan.part_name.clone());
            }
        }
        for id in delta.charts.keys() {
            if let Some(plan) = self.chart_plans.get(id) {
                affected.insert(plan.chart_part.clone());
                if let Some(workbook) = &plan.workbook_part {
                    affected.insert(workbook.clone());
                }
            }
        }
        for id in delta.semantic_shapes.keys() {
            if let Some(plan) = self.semantic_shape_plans.get(id) {
                affected.insert(plan.part_name.clone());
                if let Some((relationship_part, _)) = &plan.hyperlink_target {
                    affected.insert(relationship_part.clone());
                }
            }
        }
        for slide in delta.notes.keys() {
            if let Some(plan) = self.notes_plans.get(slide) {
                affected.insert(plan.part_name.clone());
            }
        }
        Some(affected)
    }

    /// Conservative byte weight used by host-owned eviction policies.
    ///
    /// This is advisory: hosts must treat eviction as a performance decision,
    /// never as part of generation correctness.
    pub fn estimated_resident_bytes(&self) -> u64 {
        let source = self.archive.source().len();
        let cached = self
            .cached_parts
            .values()
            .map(|bytes| bytes.len() as u64)
            .sum::<u64>();
        source.saturating_add(cached)
    }

    fn append_v2_patches(
        &self,
        data: &InjectionData,
        dynamic: &mut HashMap<String, Vec<Patch>>,
    ) -> Result<(), GenerateError> {
        let mut next_ids = self
            .maximum_shape_ids
            .iter()
            .map(|(part, value)| (part.clone(), value.saturating_add(1)))
            .collect::<HashMap<_, _>>();
        for (id, value) in &data.semantic_shapes {
            let plan = self.semantic_shape_plans.get(id).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    format!("no semantic shape binding named {id}"),
                )
            })?;
            validate_semantic_shape(value)?;
            let source = self.cached_part(&plan.part_name)?;
            let mut local = Vec::new();
            if let Some(runs) = &value.rich_text {
                let range = plan.paragraph_content_range.clone().ok_or_else(|| {
                    GenerateError::new(
                        GenerateErrorCode::InvalidTemplate,
                        format!("semantic shape {id} has no writable paragraph"),
                    )
                })?;
                local.push(Patch {
                    range,
                    replacement: rich_text_xml(runs).into_bytes(),
                });
            }
            if let Some(color) = &value.fill_color {
                let range = plan.fill_color_range.clone().ok_or_else(|| {
                    GenerateError::new(
                        GenerateErrorCode::InvalidTemplate,
                        format!("semantic shape {id} has no writable solid fill"),
                    )
                })?;
                local.push(Patch {
                    range,
                    replacement: color.as_bytes().to_vec(),
                });
            }
            let copies = value.copies.unwrap_or(1) as usize;
            if value.visible == Some(false) || copies == 0 {
                dynamic
                    .entry(plan.part_name.clone())
                    .or_default()
                    .push(Patch {
                        range: plan.shape_range.clone(),
                        replacement: Vec::new(),
                    });
            } else if copies > 1 {
                let base = source.get(plan.shape_range.clone()).ok_or_else(|| {
                    GenerateError::new(
                        GenerateErrorCode::InvalidBindingRange,
                        "semantic shape range is invalid",
                    )
                })?;
                let relative = relative_patches(local, plan.shape_range.start)?;
                let first = apply_patches(base, relative.clone())?;
                let mut replacement = first;
                let id_range = plan.shape_id_range.clone().ok_or_else(|| {
                    GenerateError::new(
                        GenerateErrorCode::InvalidTemplate,
                        "repeated semantic shape has no cNvPr id",
                    )
                })?;
                let relative_id =
                    id_range.start - plan.shape_range.start..id_range.end - plan.shape_range.start;
                let next = next_ids.entry(plan.part_name.clone()).or_insert(1);
                for _ in 1..copies {
                    let mut patches = relative.clone();
                    patches.push(Patch {
                        range: relative_id.clone(),
                        replacement: next.to_string().into_bytes(),
                    });
                    replacement.extend(apply_patches(base, patches)?);
                    *next = next.checked_add(1).ok_or_else(|| {
                        GenerateError::new(GenerateErrorCode::InvalidTemplate, "shape ID exhausted")
                    })?;
                }
                dynamic
                    .entry(plan.part_name.clone())
                    .or_default()
                    .push(Patch {
                        range: plan.shape_range.clone(),
                        replacement,
                    });
            } else {
                dynamic
                    .entry(plan.part_name.clone())
                    .or_default()
                    .extend(local);
            }
            if let Some(hyperlink) = &value.hyperlink {
                let (relationship_part, target_range) =
                    plan.hyperlink_target.as_ref().ok_or_else(|| {
                        GenerateError::new(
                            GenerateErrorCode::InvalidTemplate,
                            format!("semantic shape {id} has no writable hyperlink relationship"),
                        )
                    })?;
                dynamic
                    .entry(relationship_part.clone())
                    .or_default()
                    .push(Patch {
                        range: target_range.clone(),
                        replacement: escape_xml_attribute(hyperlink).into_bytes(),
                    });
            }
        }
        for (slide, notes) in &data.notes {
            let plan = self.notes_plans.get(slide).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    format!("slide {slide} has no writable notes part"),
                )
            })?;
            let Some(first) = plan.text_ranges.first() else {
                return Err(GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    format!("notes part {} has no text range", plan.part_name),
                ));
            };
            let patches = dynamic.entry(plan.part_name.clone()).or_default();
            patches.push(Patch {
                range: first.clone(),
                replacement: escape_xml_text(notes).into_bytes(),
            });
            patches.extend(plan.text_ranges.iter().skip(1).cloned().map(|range| Patch {
                range,
                replacement: Vec::new(),
            }));
        }
        Ok(())
    }

    /// Generate a complete caller-owned PPTX buffer.
    pub fn generate(&self, data: &InjectionData) -> Result<GenerateOutput, GenerateError> {
        let (sink, stats) = self.generate_to(data, VecSink::new())?;
        Ok(GenerateOutput {
            bytes: sink.into_inner(),
            zip_stats: stats.zip,
            rewritten_entries: stats.rewritten_entries,
            removed_entries: stats.removed_entries,
        })
    }

    /// Generate into a caller-provided forward-only sink and return the sink plus exact statistics.
    pub fn generate_to<S: OutputSink>(
        &self,
        data: &InjectionData,
        sink: S,
    ) -> Result<(S, GenerateStats), GenerateError> {
        let slide_operations = self.prepare_slide_operations(&data.slide_copies)?;
        let mut dynamic = HashMap::<String, Vec<Patch>>::new();
        let mut new_media = BTreeMap::<String, (&ImageData, EntryOptions)>::new();
        let mut replaced_media = HashSet::new();
        let mut image_types = BTreeMap::<String, String>::new();
        self.append_v2_patches(data, &mut dynamic)?;
        let active_table_bindings = self
            .table_plans
            .iter()
            .filter(|(id, _)| data.table_rows.contains_key(*id))
            .flat_map(|(_, plan)| plan.bindings.iter().map(|binding| binding.id.as_str()))
            .collect::<HashSet<_>>();
        for binding in &self.plan.bindings {
            if data.slide_copies.get(&binding.part_name) == Some(&0) {
                continue;
            }
            if active_table_bindings.contains(binding.id.as_str()) {
                continue;
            }
            if data.semantic_shapes.get(&binding.id).is_some_and(|shape| {
                shape.visible == Some(false)
                    || shape.copies == Some(0)
                    || shape.copies.is_some_and(|copies| copies > 1)
                    || (binding.kind == BindingKind::Text && shape.rich_text.is_some())
            }) {
                continue;
            }
            match binding.kind {
                BindingKind::Text => {
                    let value = data
                        .text
                        .get(&binding.id)
                        .ok_or_else(|| missing_value(&binding.id))?;
                    dynamic
                        .entry(binding.part_name.clone())
                        .or_default()
                        .extend(text_patches(
                            binding,
                            value,
                            self.cached_part(&binding.part_name)?,
                        )?);
                }
                BindingKind::Image => {
                    let image = data
                        .images
                        .get(&binding.id)
                        .ok_or_else(|| missing_value(&binding.id))?;
                    validate_image(image)?;
                    let image_plan = self.image_plans.get(&binding.id).ok_or_else(|| {
                        GenerateError::new(
                            GenerateErrorCode::InvalidTemplate,
                            format!("image plan missing for {}", binding.id),
                        )
                    })?;
                    let media_name = format!(
                        "ppt/media/wasmppt-{}.{}",
                        binding.id,
                        image.extension.to_ascii_lowercase()
                    );
                    let relative_target = format!(
                        "../media/wasmppt-{}.{}",
                        binding.id,
                        image.extension.to_ascii_lowercase()
                    );
                    dynamic
                        .entry(image_plan.relationship_part.clone())
                        .or_default()
                        .push(Patch {
                            range: image_plan.relationship_target_range.clone(),
                            replacement: escape_xml_attribute(&relative_target).into_bytes(),
                        });
                    let crop = image.crop.or(match image.fit {
                        ImageFitPolicy::Contain => Some(ImageCrop::default()),
                        ImageFitPolicy::Preserve | ImageFitPolicy::Cover => None,
                    });
                    if let Some(crop) = crop {
                        dynamic
                            .entry(binding.part_name.clone())
                            .or_default()
                            .extend(crop_patches(&image_plan.crop, crop));
                    }
                    image_types.insert(
                        image.extension.to_ascii_lowercase(),
                        image.content_type.clone(),
                    );
                    if image_plan.original_reference_count == 1 {
                        replaced_media.insert(image_plan.original_media_part.clone());
                    }
                    new_media.insert(
                        media_name,
                        (
                            image,
                            EntryOptions::deterministic(CompressionMethod::Stored),
                        ),
                    );
                }
                BindingKind::Chart => {
                    if !data.charts.contains_key(&binding.id) {
                        return Err(missing_value(&binding.id));
                    }
                }
            }
        }
        for (id, rows) in &data.table_rows {
            let table = self.table_plans.get(id).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    format!("no repeated table row named {id}"),
                )
            })?;
            let source = self.cached_part(&table.part_name)?;
            let template_row = source.get(table.row_range.clone()).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidBindingRange,
                    "table row range is invalid",
                )
            })?;
            let (rows, shrink) = apply_table_policy(id, rows, data.table_policies.get(id))?;
            let mut replacement = Vec::new();
            for row in rows {
                let mut row_patches = Vec::new();
                if let Some((numerator, denominator)) = shrink {
                    if let Some(patch) =
                        table_row_height_patch(template_row, numerator as u64, denominator as u64)?
                    {
                        row_patches.push(patch);
                    }
                }
                for binding in &table.bindings {
                    let field = binding
                        .id
                        .strip_prefix(id)
                        .and_then(|value| value.strip_prefix('.'))
                        .ok_or_else(|| {
                            GenerateError::new(
                                GenerateErrorCode::InvalidTemplate,
                                "table binding prefix mismatch",
                            )
                        })?;
                    let value = row.get(field).ok_or_else(|| missing_value(&binding.id))?;
                    for mut patch in text_patches(binding, value, source)? {
                        patch.range = patch.range.start - table.row_range.start
                            ..patch.range.end - table.row_range.start;
                        row_patches.push(patch);
                    }
                }
                replacement.extend(apply_patches(template_row, row_patches)?);
            }
            dynamic
                .entry(table.part_name.clone())
                .or_default()
                .push(Patch {
                    range: table.row_range.clone(),
                    replacement,
                });
        }
        let mut updated_chart_parts = HashSet::new();
        for (part_name, chart) in &data.charts {
            validate_chart_data(chart)?;
            let chart_plan = self.chart_plans.get(part_name).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidChart,
                    format!("no supported chart part named {part_name}"),
                )
            })?;
            if !updated_chart_parts.insert(chart_plan.chart_part.as_str()) {
                return Err(GenerateError::new(
                    GenerateErrorCode::InvalidChart,
                    format!(
                        "multiple chart keys target the same chart part {}",
                        chart_plan.chart_part
                    ),
                ));
            }
            let chart_source = self.cached_part(&chart_plan.chart_part)?;
            let rewritten_chart = rewrite_chart_cache(chart_source, chart)?;
            dynamic
                .entry(chart_plan.chart_part.clone())
                .or_default()
                .push(Patch {
                    range: 0..chart_source.len(),
                    replacement: rewritten_chart,
                });
            if let Some(workbook_part) = &chart_plan.workbook_part {
                let workbook_source = self.cached_part(workbook_part)?;
                let rewritten_workbook = rewrite_embedded_workbook(workbook_source, chart)?;
                dynamic
                    .entry(workbook_part.clone())
                    .or_default()
                    .push(Patch {
                        range: 0..workbook_source.len(),
                        replacement: rewritten_workbook,
                    });
            }
        }
        if !image_types.is_empty() {
            dynamic
                .entry("[Content_Types].xml".to_owned())
                .or_default()
                .extend(content_type_patches(
                    self.cached_part("[Content_Types].xml")?,
                    &image_types,
                )?);
        }
        if !slide_operations.presentation_patches.is_empty() {
            dynamic.insert(
                self.slide_deck.presentation_part.clone(),
                slide_operations.presentation_patches.clone(),
            );
        }
        if !slide_operations.relationship_patches.is_empty() {
            dynamic.insert(
                self.slide_deck.relationship_part.clone(),
                slide_operations.relationship_patches.clone(),
            );
        }
        if !slide_operations.content_type_patches.is_empty() {
            dynamic
                .entry("[Content_Types].xml".to_owned())
                .or_default()
                .extend(slide_operations.content_type_patches.clone());
        }

        let mut writer = ZipWriter::new(sink);
        let mut rewritten_entries = 0;
        let mut removed_entries = 0;
        let mut dirty_uncompressed_bytes = 0_u64;
        let mut peak_dirty_entry_bytes = 0_u64;
        for entry in self.archive.entries() {
            if self.removed_parts.contains(&entry.name)
                || replaced_media.contains(&entry.name)
                || slide_operations.removed_parts.contains(&entry.name)
            {
                removed_entries += 1;
                continue;
            }
            let static_edits = self.static_patches.get(&entry.name);
            let dynamic_edits = dynamic.get(&entry.name);
            if static_edits.is_none() && dynamic_edits.is_none() {
                writer
                    .raw_copy(&self.archive, entry, RewriteMode::Preserve)
                    .map_err(package_error)?;
                continue;
            }
            let mut patches = static_edits
                .into_iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>();
            patches.extend(dynamic_edits.into_iter().flatten().cloned());
            let rewritten = apply_patches(self.cached_part(&entry.name)?, patches)?;
            record_dirty_bytes(
                &mut dirty_uncompressed_bytes,
                &mut peak_dirty_entry_bytes,
                rewritten.len(),
            );
            writer
                .write_entry(&entry.name, &rewritten, &options_from_entry(entry))
                .map_err(package_error)?;
            rewritten_entries += 1;
        }
        for (name, (image, options)) in new_media {
            record_dirty_bytes(
                &mut dirty_uncompressed_bytes,
                &mut peak_dirty_entry_bytes,
                image.bytes.len(),
            );
            writer
                .write_entry(&name, &image.bytes, &options)
                .map_err(package_error)?;
            rewritten_entries += 1;
        }
        for clone in slide_operations.clones {
            let source_entry = self.archive.entry(&clone.source_part).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    "clone source slide is missing",
                )
            })?;
            let mut patches = self
                .static_patches
                .get(&clone.source_part)
                .into_iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>();
            patches.extend(
                dynamic
                    .get(&clone.source_part)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            let bytes = apply_patches(self.cached_part(&clone.source_part)?, patches)?;
            record_dirty_bytes(
                &mut dirty_uncompressed_bytes,
                &mut peak_dirty_entry_bytes,
                bytes.len(),
            );
            writer
                .write_entry(&clone.part_name, &bytes, &options_from_entry(source_entry))
                .map_err(package_error)?;
            rewritten_entries += 1;
            if let (Some(source_rels), Some(clone_rels)) =
                (clone.source_relationship_part, clone.relationship_part)
            {
                let entry = self.archive.entry(&source_rels).ok_or_else(|| {
                    GenerateError::new(
                        GenerateErrorCode::InvalidTemplate,
                        "clone source relationships are missing",
                    )
                })?;
                let mut patches = self
                    .static_patches
                    .get(&source_rels)
                    .into_iter()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>();
                patches.extend(dynamic.get(&source_rels).into_iter().flatten().cloned());
                let bytes = apply_patches(self.cached_part(&source_rels)?, patches)?;
                let bytes = strip_notes_relationships(&bytes)?;
                record_dirty_bytes(
                    &mut dirty_uncompressed_bytes,
                    &mut peak_dirty_entry_bytes,
                    bytes.len(),
                );
                writer
                    .write_entry(&clone_rels, &bytes, &options_from_entry(entry))
                    .map_err(package_error)?;
                rewritten_entries += 1;
            }
        }
        let (sink, zip_stats) = writer.finish().map_err(package_error)?;
        Ok((
            sink,
            GenerateStats {
                zip: zip_stats,
                rewritten_entries,
                removed_entries,
                dirty_uncompressed_bytes,
                peak_dirty_entry_bytes,
                maximum_output_chunk_bytes: 0,
            },
        ))
    }

    /// Prepare one immutable logical package revision without serializing a PPTX.
    pub fn prepare_overlay(&self, data: &InjectionData) -> Result<PreparedOverlay, GenerateError> {
        self.prepare_overlay_internal(data, None, None, &BTreeSet::new())
    }

    fn prepare_overlay_reusing(
        &self,
        data: &InjectionData,
        previous: &PreparedOverlay,
        affected_parts: Option<&HashSet<String>>,
        changed_bindings: &BTreeSet<String>,
    ) -> Result<PreparedOverlay, GenerateError> {
        self.prepare_overlay_internal(data, Some(previous), affected_parts, changed_bindings)
    }

    fn prepare_overlay_internal(
        &self,
        data: &InjectionData,
        previous: Option<&PreparedOverlay>,
        affected_parts: Option<&HashSet<String>>,
        changed_bindings: &BTreeSet<String>,
    ) -> Result<PreparedOverlay, GenerateError> {
        let slide_operations = self.prepare_slide_operations(&data.slide_copies)?;
        let mut dynamic = HashMap::<String, Vec<Patch>>::new();
        let mut new_media = BTreeMap::<String, (String, ImageData, EntryOptions)>::new();
        let mut replaced_media = HashSet::new();
        let mut image_types = BTreeMap::<String, String>::new();
        self.append_v2_patches(data, &mut dynamic)?;
        let active_table_bindings = self
            .table_plans
            .iter()
            .filter(|(id, _)| data.table_rows.contains_key(*id))
            .flat_map(|(_, plan)| plan.bindings.iter().map(|binding| binding.id.as_str()))
            .collect::<HashSet<_>>();
        for binding in &self.plan.bindings {
            if data.slide_copies.get(&binding.part_name) == Some(&0)
                || active_table_bindings.contains(binding.id.as_str())
            {
                continue;
            }
            if data.semantic_shapes.get(&binding.id).is_some_and(|shape| {
                shape.visible == Some(false)
                    || shape.copies == Some(0)
                    || shape.copies.is_some_and(|copies| copies > 1)
                    || (binding.kind == BindingKind::Text && shape.rich_text.is_some())
            }) {
                continue;
            }
            match binding.kind {
                BindingKind::Text => {
                    let value = data
                        .text
                        .get(&binding.id)
                        .ok_or_else(|| missing_value(&binding.id))?;
                    dynamic
                        .entry(binding.part_name.clone())
                        .or_default()
                        .extend(text_patches(
                            binding,
                            value,
                            self.cached_part(&binding.part_name)?,
                        )?);
                }
                BindingKind::Image => {
                    let image = data
                        .images
                        .get(&binding.id)
                        .ok_or_else(|| missing_value(&binding.id))?;
                    validate_image(image)?;
                    let image_plan = self.image_plans.get(&binding.id).ok_or_else(|| {
                        GenerateError::new(
                            GenerateErrorCode::InvalidTemplate,
                            format!("image plan missing for {}", binding.id),
                        )
                    })?;
                    let extension = image.extension.to_ascii_lowercase();
                    let media_name = format!("ppt/media/wasmppt-{}.{}", binding.id, extension);
                    let relative_target = format!("../media/wasmppt-{}.{}", binding.id, extension);
                    dynamic
                        .entry(image_plan.relationship_part.clone())
                        .or_default()
                        .push(Patch {
                            range: image_plan.relationship_target_range.clone(),
                            replacement: escape_xml_attribute(&relative_target).into_bytes(),
                        });
                    let crop = image.crop.or(match image.fit {
                        ImageFitPolicy::Contain => Some(ImageCrop::default()),
                        ImageFitPolicy::Preserve | ImageFitPolicy::Cover => None,
                    });
                    if let Some(crop) = crop {
                        dynamic
                            .entry(binding.part_name.clone())
                            .or_default()
                            .extend(crop_patches(&image_plan.crop, crop));
                    }
                    image_types.insert(extension, image.content_type.clone());
                    if image_plan.original_reference_count == 1 {
                        replaced_media.insert(image_plan.original_media_part.clone());
                    }
                    new_media.insert(
                        media_name,
                        (
                            binding.id.clone(),
                            image.clone(),
                            EntryOptions::deterministic(CompressionMethod::Stored),
                        ),
                    );
                }
                BindingKind::Chart => {
                    if !data.charts.contains_key(&binding.id) {
                        return Err(missing_value(&binding.id));
                    }
                }
            }
        }
        for (id, rows) in &data.table_rows {
            let table = self.table_plans.get(id).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    format!("no repeated table row named {id}"),
                )
            })?;
            let source = self.cached_part(&table.part_name)?;
            let template_row = source.get(table.row_range.clone()).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidBindingRange,
                    "table row range is invalid",
                )
            })?;
            let (rows, shrink) = apply_table_policy(id, rows, data.table_policies.get(id))?;
            let mut replacement = Vec::new();
            for row in rows {
                let mut row_patches = Vec::new();
                if let Some((numerator, denominator)) = shrink {
                    if let Some(patch) =
                        table_row_height_patch(template_row, numerator as u64, denominator as u64)?
                    {
                        row_patches.push(patch);
                    }
                }
                for binding in &table.bindings {
                    let field = binding
                        .id
                        .strip_prefix(id)
                        .and_then(|value| value.strip_prefix('.'))
                        .ok_or_else(|| {
                            GenerateError::new(
                                GenerateErrorCode::InvalidTemplate,
                                "table binding prefix mismatch",
                            )
                        })?;
                    let value = row.get(field).ok_or_else(|| missing_value(&binding.id))?;
                    for mut patch in text_patches(binding, value, source)? {
                        patch.range = patch.range.start - table.row_range.start
                            ..patch.range.end - table.row_range.start;
                        row_patches.push(patch);
                    }
                }
                replacement.extend(apply_patches(template_row, row_patches)?);
            }
            dynamic
                .entry(table.part_name.clone())
                .or_default()
                .push(Patch {
                    range: table.row_range.clone(),
                    replacement,
                });
        }
        let mut updated_chart_parts = HashSet::new();
        for (part_name, chart) in &data.charts {
            validate_chart_data(chart)?;
            let chart_plan = self.chart_plans.get(part_name).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidChart,
                    format!("no supported chart part named {part_name}"),
                )
            })?;
            if !updated_chart_parts.insert(chart_plan.chart_part.as_str()) {
                return Err(GenerateError::new(
                    GenerateErrorCode::InvalidChart,
                    format!(
                        "multiple chart keys target the same chart part {}",
                        chart_plan.chart_part
                    ),
                ));
            }
            let chart_source = self.cached_part(&chart_plan.chart_part)?;
            dynamic
                .entry(chart_plan.chart_part.clone())
                .or_default()
                .push(Patch {
                    range: 0..chart_source.len(),
                    replacement: rewrite_chart_cache(chart_source, chart)?,
                });
            if let Some(workbook_part) = &chart_plan.workbook_part {
                let workbook_source = self.cached_part(workbook_part)?;
                dynamic
                    .entry(workbook_part.clone())
                    .or_default()
                    .push(Patch {
                        range: 0..workbook_source.len(),
                        replacement: rewrite_embedded_workbook(workbook_source, chart)?,
                    });
            }
        }
        if !image_types.is_empty() {
            dynamic
                .entry("[Content_Types].xml".to_owned())
                .or_default()
                .extend(content_type_patches(
                    self.cached_part("[Content_Types].xml")?,
                    &image_types,
                )?);
        }
        if !slide_operations.presentation_patches.is_empty() {
            dynamic.insert(
                self.slide_deck.presentation_part.clone(),
                slide_operations.presentation_patches.clone(),
            );
        }
        if !slide_operations.relationship_patches.is_empty() {
            dynamic.insert(
                self.slide_deck.relationship_part.clone(),
                slide_operations.relationship_patches.clone(),
            );
        }
        if !slide_operations.content_type_patches.is_empty() {
            dynamic
                .entry("[Content_Types].xml".to_owned())
                .or_default()
                .extend(slide_operations.content_type_patches.clone());
        }

        let mut entries = VecDeque::new();
        let mut rewritten_entries = 0;
        let mut removed_entries = 0;
        for entry in self.archive.entries() {
            if self.removed_parts.contains(&entry.name)
                || replaced_media.contains(&entry.name)
                || slide_operations.removed_parts.contains(&entry.name)
            {
                removed_entries += 1;
                continue;
            }
            let static_edits = self.static_patches.get(&entry.name);
            let dynamic_edits = dynamic.get(&entry.name);
            if static_edits.is_none() && dynamic_edits.is_none() {
                entries.push_back(GenerationEntry::Raw(entry.clone()));
                continue;
            }
            if affected_parts.is_some_and(|parts| !parts.contains(&entry.name)) {
                if let Some(reused) =
                    previous.and_then(|overlay| overlay.generation_entry(&entry.name))
                {
                    entries.push_back(reused);
                    rewritten_entries += 1;
                    continue;
                }
            }
            let mut patches = static_edits
                .into_iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>();
            patches.extend(dynamic_edits.into_iter().flatten().cloned());
            entries.push_back(GenerationEntry::Owned {
                name: entry.name.clone(),
                bytes: apply_patches(self.cached_part(&entry.name)?, patches)?.into(),
                options: options_from_entry(entry),
            });
            rewritten_entries += 1;
        }
        for (name, (binding_id, image, options)) in new_media {
            if !changed_bindings.contains(&binding_id) {
                if let Some(reused) = previous.and_then(|overlay| overlay.generation_entry(&name)) {
                    entries.push_back(reused);
                    rewritten_entries += 1;
                    continue;
                }
            }
            entries.push_back(GenerationEntry::Owned {
                name,
                bytes: image.bytes,
                options,
            });
            rewritten_entries += 1;
        }
        for clone in slide_operations.clones {
            let source_entry = self.archive.entry(&clone.source_part).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    "clone source slide is missing",
                )
            })?;
            if affected_parts.is_some_and(|parts| !parts.contains(&clone.source_part)) {
                let reused_slide =
                    previous.and_then(|overlay| overlay.generation_entry(&clone.part_name));
                let reused_relationships = clone
                    .relationship_part
                    .as_ref()
                    .map(|name| previous.and_then(|overlay| overlay.generation_entry(name)));
                if let Some(reused_slide) = reused_slide {
                    if reused_relationships.as_ref().is_none_or(Option::is_some) {
                        entries.push_back(reused_slide);
                        rewritten_entries += 1;
                        if let Some(Some(reused)) = reused_relationships {
                            entries.push_back(reused);
                            rewritten_entries += 1;
                        }
                        continue;
                    }
                }
            }
            let mut patches = self
                .static_patches
                .get(&clone.source_part)
                .into_iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>();
            patches.extend(
                dynamic
                    .get(&clone.source_part)
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
            entries.push_back(GenerationEntry::Owned {
                name: clone.part_name,
                bytes: apply_patches(self.cached_part(&clone.source_part)?, patches)?.into(),
                options: options_from_entry(source_entry),
            });
            rewritten_entries += 1;
            if let (Some(source_rels), Some(clone_rels)) =
                (clone.source_relationship_part, clone.relationship_part)
            {
                let entry = self.archive.entry(&source_rels).ok_or_else(|| {
                    GenerateError::new(
                        GenerateErrorCode::InvalidTemplate,
                        "clone source relationships are missing",
                    )
                })?;
                let mut patches = self
                    .static_patches
                    .get(&source_rels)
                    .into_iter()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>();
                patches.extend(dynamic.get(&source_rels).into_iter().flatten().cloned());
                let bytes = strip_notes_relationships(&apply_patches(
                    self.cached_part(&source_rels)?,
                    patches,
                )?)?;
                entries.push_back(GenerationEntry::Owned {
                    name: clone_rels,
                    bytes: bytes.into(),
                    options: options_from_entry(entry),
                });
                rewritten_entries += 1;
            }
        }
        let (dirty_uncompressed_bytes, peak_dirty_entry_bytes) =
            entries
                .iter()
                .fold((0_u64, 0_u64), |(total, peak), entry| match entry {
                    GenerationEntry::Raw(_) => (total, peak),
                    GenerationEntry::Owned { bytes, .. } => (
                        total.saturating_add(bytes.len() as u64),
                        peak.max(bytes.len() as u64),
                    ),
                });
        PreparedOverlay::from_entries(
            self.archive.clone(),
            entries,
            rewritten_entries,
            removed_entries,
            dirty_uncompressed_bytes,
            peak_dirty_entry_bytes,
        )
    }

    /// Prepare a resumable package cursor. No complete output buffer is retained.
    pub fn generate_cursor(&self, data: &InjectionData) -> Result<GenerationCursor, GenerateError> {
        self.prepare_overlay(data)
            .map(|overlay| overlay.generation_cursor())
    }

    fn cached_part(&self, name: &str) -> Result<&[u8], GenerateError> {
        self.cached_parts
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    format!("prepared bytes missing for dirty part {name}"),
                )
            })
    }

    fn prepare_slide_operations(
        &self,
        requested: &BTreeMap<String, usize>,
    ) -> Result<SlideOperations, GenerateError> {
        if requested.is_empty() {
            return Ok(SlideOperations::default());
        }
        for part in requested.keys() {
            if !self.slide_deck.used_slide_parts.contains(part) {
                return Err(GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    format!("slide copy request targets unknown slide {part}"),
                ));
            }
        }
        let presentation = self.cached_part(&self.slide_deck.presentation_part)?;
        let mut operations = SlideOperations::default();
        let mut next_slide_id = self
            .slide_deck
            .slides
            .iter()
            .map(|slide| slide.slide_id)
            .max()
            .unwrap_or(255)
            .checked_add(1)
            .ok_or_else(|| {
                GenerateError::new(GenerateErrorCode::InvalidTemplate, "slide ID exhausted")
            })?;
        let mut used_relationships = self.slide_deck.used_relationship_ids.clone();
        let mut next_relationship = 1u32;
        let mut used_parts = self.slide_deck.used_slide_parts.clone();
        let mut next_part = 1u32;
        let mut relationship_insertion = String::new();
        let mut content_type_insertion = String::new();

        for slide in &self.slide_deck.slides {
            let copies = requested.get(&slide.part_name).copied().unwrap_or(1);
            if copies == 0 {
                operations.presentation_patches.push(Patch {
                    range: slide.list_range.clone(),
                    replacement: Vec::new(),
                });
                operations.relationship_patches.push(Patch {
                    range: slide.relationship_range.clone(),
                    replacement: Vec::new(),
                });
                operations.removed_parts.insert(slide.part_name.clone());
                if let Some(rels) = relationship_part_name(&slide.part_name) {
                    if self.archive.entry(&rels).is_some() {
                        operations.removed_parts.insert(rels);
                    }
                }
                if let Some((_, range)) = self.slide_deck.content_types.get(&slide.part_name) {
                    operations.content_type_patches.push(Patch {
                        range: range.clone(),
                        replacement: Vec::new(),
                    });
                }
                continue;
            }
            if copies == 1 {
                continue;
            }
            let original = presentation.get(slide.list_range.clone()).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    "slide list range is invalid",
                )
            })?;
            let mut list_replacement = original.to_vec();
            for _ in 1..copies {
                while used_relationships.contains(&format!("rId{next_relationship}")) {
                    next_relationship = next_relationship.checked_add(1).ok_or_else(|| {
                        GenerateError::new(
                            GenerateErrorCode::InvalidTemplate,
                            "relationship ID exhausted",
                        )
                    })?;
                }
                let relationship_id = format!("rId{next_relationship}");
                used_relationships.insert(relationship_id.clone());
                next_relationship += 1;
                while used_parts.contains(&format!("ppt/slides/slide{next_part}.xml")) {
                    next_part = next_part.checked_add(1).ok_or_else(|| {
                        GenerateError::new(
                            GenerateErrorCode::InvalidTemplate,
                            "slide part number exhausted",
                        )
                    })?;
                }
                let part_name = format!("ppt/slides/slide{next_part}.xml");
                used_parts.insert(part_name.clone());
                next_part += 1;
                list_replacement.extend_from_slice(
                    format!(
                        "<{}:sldId id=\"{}\" {}:id=\"{}\"/>",
                        slide.list_prefix,
                        next_slide_id,
                        slide.list_relationship_prefix,
                        relationship_id
                    )
                    .as_bytes(),
                );
                next_slide_id = next_slide_id.checked_add(1).ok_or_else(|| {
                    GenerateError::new(GenerateErrorCode::InvalidTemplate, "slide ID exhausted")
                })?;
                let target = part_name.strip_prefix("ppt/").expect("slide prefix");
                relationship_insertion.push_str(&format!(
                    "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"/>",
                    relationship_id,
                    escape_xml_attribute(&slide.relationship_type),
                    escape_xml_attribute(target)
                ));
                if let Some((content_type, _)) = self.slide_deck.content_types.get(&slide.part_name)
                {
                    content_type_insertion.push_str(&format!(
                        "<Override PartName=\"/{}\" ContentType=\"{}\"/>",
                        part_name,
                        escape_xml_attribute(content_type)
                    ));
                }
                let source_relationship_part = relationship_part_name(&slide.part_name)
                    .filter(|name| self.archive.entry(name).is_some());
                let relationship_part = source_relationship_part
                    .as_ref()
                    .and_then(|_| relationship_part_name(&part_name));
                operations.clones.push(SlideClone {
                    source_part: slide.part_name.clone(),
                    part_name,
                    source_relationship_part,
                    relationship_part,
                });
            }
            operations.presentation_patches.push(Patch {
                range: slide.list_range.clone(),
                replacement: list_replacement,
            });
        }
        if !relationship_insertion.is_empty() {
            operations.relationship_patches.push(Patch {
                range: self.slide_deck.relationship_insert_offset
                    ..self.slide_deck.relationship_insert_offset,
                replacement: relationship_insertion.into_bytes(),
            });
        }
        if !content_type_insertion.is_empty() {
            operations.content_type_patches.push(Patch {
                range: self.slide_deck.content_type_insert_offset
                    ..self.slide_deck.content_type_insert_offset,
                replacement: content_type_insertion.into_bytes(),
            });
        }
        Ok(operations)
    }
}

fn prepare_semantic_shape_plans(
    archive: &ZipArchive<wasmppt_opc::MemorySource>,
    template: &TemplatePlan,
) -> Result<PreparedSemanticShapes, GenerateError> {
    let mut output = HashMap::new();
    let mut maximum_ids = HashMap::new();
    let mut parts = template
        .bindings
        .iter()
        .map(|binding| binding.part_name.as_str())
        .collect::<HashSet<_>>();
    for part_name in parts.drain() {
        let entry = archive.entry(part_name).ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                "binding part is missing",
            )
        })?;
        let bytes = archive.read_entry(entry).map_err(package_error)?;
        let slide = SlideView::parse(bytes.clone()).map_err(GenerateError::pml)?;
        maximum_ids.insert(
            part_name.to_owned(),
            slide
                .shapes()
                .iter()
                .filter_map(|shape| shape.id)
                .max()
                .unwrap_or(0),
        );
        for binding in template
            .bindings
            .iter()
            .filter(|binding| binding.part_name == part_name)
        {
            let Some(shape) = slide.shapes().iter().find(|shape| {
                binding.shape_id.is_some_and(|id| shape.id == Some(id))
                    || binding
                        .shape_name
                        .as_deref()
                        .is_some_and(|name| shape.name.as_deref() == Some(name))
            }) else {
                continue;
            };
            let document = slide.document();
            let within = |token: &wasmppt_xml::Token| {
                token.range.start >= shape.source_range.start
                    && token.range.end <= shape.source_range.end
            };
            let mut shape_id_range = None;
            let mut fill_color_range = None;
            let mut paragraph_content_range = None;
            let mut hyperlink_id = None;
            for (index, token) in document.tokens().iter().enumerate() {
                if !within(token) {
                    continue;
                }
                let TokenKind::Start {
                    name,
                    attributes,
                    empty,
                } = &token.kind
                else {
                    continue;
                };
                if name.local == "cNvPr" {
                    shape_id_range = document
                        .attribute(attributes, None, "id")
                        .map(|value| value.value_range.clone());
                } else if name.local == "srgbClr" && fill_color_range.is_none() {
                    fill_color_range = document
                        .attribute(attributes, None, "val")
                        .map(|value| value.value_range.clone());
                } else if name.local == "hlinkClick" {
                    hyperlink_id = attributes
                        .iter()
                        .find(|attribute| attribute.name.local == "id")
                        .map(|attribute| attribute.value.clone());
                } else if name.local == "p" && paragraph_content_range.is_none() && !empty {
                    if let Some(end) = matching_end(document, index) {
                        paragraph_content_range =
                            Some(token.range.end..document.tokens()[end].range.start);
                    }
                }
            }
            let hyperlink_target = hyperlink_id
                .as_deref()
                .and_then(|id| find_relationship_target_range(archive, part_name, id).transpose())
                .transpose()?;
            output.insert(
                binding.id.clone(),
                SemanticShapePlan {
                    part_name: part_name.to_owned(),
                    shape_range: shape.source_range.clone(),
                    shape_id_range,
                    paragraph_content_range,
                    fill_color_range,
                    hyperlink_target,
                },
            );
        }
    }
    Ok((output, maximum_ids))
}

fn prepare_notes_plans(
    archive: &ZipArchive<wasmppt_opc::MemorySource>,
) -> Result<HashMap<String, NotesPlan>, GenerateError> {
    let mut output = HashMap::new();
    for slide in archive
        .entries()
        .iter()
        .filter(|entry| entry.name.starts_with("ppt/slides/slide") && entry.name.ends_with(".xml"))
    {
        let Some(rels_name) = relationship_part_name(&slide.name) else {
            continue;
        };
        let Some(rels_entry) = archive.entry(&rels_name) else {
            continue;
        };
        let rels = archive.read_entry(rels_entry).map_err(package_error)?;
        let document = XmlDocument::parse(rels).map_err(GenerateError::xml)?;
        let target = document.tokens().iter().find_map(|token| {
            let TokenKind::Start {
                name, attributes, ..
            } = &token.kind
            else {
                return None;
            };
            if name.local != "Relationship" {
                return None;
            }
            let kind = document.attribute(attributes, None, "Type")?;
            kind.value.ends_with("/notesSlide").then(|| {
                document
                    .attribute(attributes, None, "Target")
                    .and_then(|target| resolve_target(Some(&slide.name), &target.value))
            })?
        });
        let Some(part_name) = target else {
            continue;
        };
        let Some(entry) = archive.entry(&part_name) else {
            continue;
        };
        let bytes = archive.read_entry(entry).map_err(package_error)?;
        let document = XmlDocument::parse(bytes).map_err(GenerateError::xml)?;
        let text_ranges = document
            .tokens()
            .iter()
            .filter(|token| matches!(&token.kind, TokenKind::Text | TokenKind::Cdata))
            .map(|token| {
                if matches!(&token.kind, TokenKind::Cdata) {
                    token.range.start + 9..token.range.end - 3
                } else {
                    token.range.clone()
                }
            })
            .collect();
        output.insert(
            slide.name.clone(),
            NotesPlan {
                part_name,
                text_ranges,
            },
        );
    }
    Ok(output)
}

fn find_relationship_target_range(
    archive: &ZipArchive<wasmppt_opc::MemorySource>,
    source_part: &str,
    relationship_id: &str,
) -> Result<Option<(String, Range<usize>)>, GenerateError> {
    let Some(part_name) = relationship_part_name(source_part) else {
        return Ok(None);
    };
    let Some(entry) = archive.entry(&part_name) else {
        return Ok(None);
    };
    let bytes = archive.read_entry(entry).map_err(package_error)?;
    let document = XmlDocument::parse(bytes).map_err(GenerateError::xml)?;
    Ok(document.tokens().iter().find_map(|token| {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            return None;
        };
        (name.local == "Relationship"
            && document
                .attribute(attributes, None, "Id")
                .is_some_and(|id| id.value == relationship_id))
        .then(|| {
            document
                .attribute(attributes, None, "Target")
                .map(|target| (part_name.clone(), target.value_range.clone()))
        })?
    }))
}

fn matching_end(document: &XmlDocument, start: usize) -> Option<usize> {
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
            matches!(&candidate.kind, TokenKind::End { name: end }
            if candidate.depth == token.depth && end.local == name.local)
            .then_some(index)
        })
}

fn prepare_image_plans(
    archive: &ZipArchive<wasmppt_opc::MemorySource>,
    plan: &TemplatePlan,
) -> Result<HashMap<String, ImagePlan>, GenerateError> {
    let mut reference_counts = HashMap::<String, usize>::new();
    for entry in archive
        .entries()
        .iter()
        .filter(|entry| entry.name.ends_with(".rels"))
    {
        let source_part = relationship_source(&entry.name);
        let bytes = archive.read_entry(entry).map_err(package_error)?;
        let document = XmlDocument::parse(bytes).map_err(GenerateError::xml)?;
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
            let external = document
                .attribute(attributes, None, "TargetMode")
                .is_some_and(|attribute| attribute.value.eq_ignore_ascii_case("External"));
            if external {
                continue;
            }
            if let Some(target) = document.attribute(attributes, None, "Target") {
                if let Some(resolved) = resolve_target(source_part.as_deref(), &target.value) {
                    *reference_counts.entry(resolved).or_default() += 1;
                }
            }
        }
    }

    let mut output = HashMap::new();
    for binding in plan
        .bindings
        .iter()
        .filter(|binding| binding.kind == BindingKind::Image)
    {
        let RelationshipAction::ReplaceImage { relationship_id } = &binding.relationship_action
        else {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                format!("image binding {} has no relationship action", binding.id),
            ));
        };
        let relationship_part = relationship_part_name(&binding.part_name).ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                "image binding part has no relationship path",
            )
        })?;
        let entry = archive.entry(&relationship_part).ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                format!("missing relationship part {relationship_part}"),
            )
        })?;
        let bytes = archive.read_entry(entry).map_err(package_error)?;
        let document = XmlDocument::parse(bytes).map_err(GenerateError::xml)?;
        let mut target = None;
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
            if document
                .attribute(attributes, None, "Id")
                .is_some_and(|attribute| attribute.value == *relationship_id)
            {
                let attribute =
                    document
                        .attribute(attributes, None, "Target")
                        .ok_or_else(|| {
                            GenerateError::new(
                                GenerateErrorCode::InvalidTemplate,
                                "image relationship has no Target",
                            )
                        })?;
                let resolved = resolve_target(Some(&binding.part_name), &attribute.value)
                    .ok_or_else(|| {
                        GenerateError::new(
                            GenerateErrorCode::InvalidTemplate,
                            "image relationship target is invalid",
                        )
                    })?;
                target = Some((attribute.value_range.clone(), resolved));
                break;
            }
        }
        let (relationship_target_range, original_media_part) = target.ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                format!("relationship {relationship_id} was not found"),
            )
        })?;
        let slide = archive.entry(&binding.part_name).ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                "image slide part is missing",
            )
        })?;
        let slide_bytes = archive.read_entry(slide).map_err(package_error)?;
        let crop = find_crop_plan(&slide_bytes, relationship_id)?;
        output.insert(
            binding.id.clone(),
            ImagePlan {
                relationship_part,
                relationship_target_range,
                original_reference_count: reference_counts
                    .get(&original_media_part)
                    .copied()
                    .unwrap_or(0),
                original_media_part,
                crop,
            },
        );
    }
    Ok(output)
}

fn prepare_table_plans(
    archive: &ZipArchive<wasmppt_opc::MemorySource>,
    plan: &TemplatePlan,
) -> Result<HashMap<String, TablePlan>, GenerateError> {
    let mut grouped = HashMap::<(String, String), Vec<BindingTarget>>::new();
    for binding in plan
        .bindings
        .iter()
        .filter(|binding| binding.kind == BindingKind::Text)
    {
        let Some((table_id, field)) = binding.id.split_once('.') else {
            continue;
        };
        if table_id.is_empty() || field.is_empty() || binding.text_spans.is_empty() {
            continue;
        }
        grouped
            .entry((binding.part_name.clone(), table_id.to_owned()))
            .or_default()
            .push(binding.clone());
    }
    let mut output = HashMap::new();
    for ((part_name, table_id), bindings) in grouped {
        let entry = archive.entry(&part_name).ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                format!("missing table part {part_name}"),
            )
        })?;
        let source = archive.read_entry(entry).map_err(package_error)?;
        let document = XmlDocument::parse(source).map_err(GenerateError::xml)?;
        let first_offset = bindings
            .iter()
            .flat_map(|binding| binding.text_spans.iter())
            .map(|span| span.source_range.start as usize)
            .min()
            .expect("table bindings have spans");
        let row_range =
            enclosing_element_range(&document, "tr", first_offset).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    format!("binding prefix {table_id} is not inside a DrawingML table row"),
                )
            })?;
        if !bindings
            .iter()
            .flat_map(|binding| binding.text_spans.iter())
            .all(|span| {
                row_range.contains(&(span.source_range.start as usize))
                    && span.source_range.end as usize <= row_range.end
            })
        {
            continue;
        }
        if output
            .insert(
                table_id.clone(),
                TablePlan {
                    part_name,
                    row_range,
                    bindings,
                },
            )
            .is_some()
        {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                format!("table row ID {table_id} is ambiguous across parts"),
            ));
        }
    }
    Ok(output)
}

fn prepare_chart_plans(
    archive: &ZipArchive<wasmppt_opc::MemorySource>,
    template: &TemplatePlan,
) -> Result<HashMap<String, ChartPlan>, GenerateError> {
    let graph = PackageGraph::build(archive).map_err(|error| {
        GenerateError::new(
            GenerateErrorCode::InvalidTemplate,
            format!("cannot build chart relationship graph: {error}"),
        )
    })?;
    let mut plans = HashMap::new();
    for entry in archive.entries().iter().filter(|entry| {
        entry.name.starts_with("ppt/charts/")
            && entry.name.ends_with(".xml")
            && !entry.name.contains("/_rels/")
    }) {
        let workbook_part = graph.part_by_name(&entry.name).and_then(|part| {
            part.relationships.iter().find_map(|relationship| {
                if !graph.relationship_type(relationship).ends_with("/package") {
                    return None;
                }
                match relationship.target {
                    RelationshipTarget::Internal(target) => {
                        Some(graph.part_name(graph.part(target)).to_owned())
                    }
                    _ => None,
                }
            })
        });
        plans.insert(
            entry.name.clone(),
            ChartPlan {
                chart_part: entry.name.clone(),
                workbook_part,
            },
        );
    }
    for binding in template
        .bindings
        .iter()
        .filter(|binding| binding.kind == BindingKind::Chart)
    {
        let RelationshipAction::ReplaceChart { relationship_id } = &binding.relationship_action
        else {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                format!("chart binding {} has no relationship action", binding.id),
            ));
        };
        let chart_part = find_relationship_target(archive, &binding.part_name, relationship_id)?
            .ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    format!("chart relationship {relationship_id} was not found"),
                )
            })?;
        let chart_plan = plans.get(&chart_part).cloned().ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidChart,
                format!(
                    "chart binding {} targets unsupported part {chart_part}",
                    binding.id
                ),
            )
        })?;
        plans.insert(binding.id.clone(), chart_plan);
    }
    Ok(plans)
}

fn find_relationship_target(
    archive: &ZipArchive<wasmppt_opc::MemorySource>,
    source_part: &str,
    relationship_id: &str,
) -> Result<Option<String>, GenerateError> {
    let Some(part_name) = relationship_part_name(source_part) else {
        return Ok(None);
    };
    let Some(entry) = archive.entry(&part_name) else {
        return Ok(None);
    };
    let bytes = archive.read_entry(entry).map_err(package_error)?;
    let document = XmlDocument::parse(bytes).map_err(GenerateError::xml)?;
    Ok(document.tokens().iter().find_map(|token| {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            return None;
        };
        if name.local != "Relationship"
            || document
                .attribute(attributes, None, "Id")
                .is_none_or(|id| id.value != relationship_id)
        {
            return None;
        }
        document
            .attribute(attributes, None, "Target")
            .and_then(|target| resolve_target(Some(source_part), &target.value))
    }))
}

fn validate_chart_data(chart: &ChartData) -> Result<(), GenerateError> {
    if chart.categories.is_empty() || chart.series.is_empty() {
        return Err(GenerateError::new(
            GenerateErrorCode::InvalidChart,
            "chart categories and series must not be empty",
        ));
    }
    for series in &chart.series {
        if series.values.len() != chart.categories.len() {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidChart,
                format!(
                    "chart series {:?} has {} values for {} categories",
                    series.name,
                    series.values.len(),
                    chart.categories.len()
                ),
            ));
        }
        if series.values.iter().any(|value| !value.is_finite()) {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidChart,
                format!(
                    "chart series {:?} contains a non-finite number",
                    series.name
                ),
            ));
        }
    }
    Ok(())
}

fn rewrite_chart_cache(source: &[u8], chart: &ChartData) -> Result<Vec<u8>, GenerateError> {
    let document = XmlDocument::parse(source).map_err(GenerateError::xml)?;
    let series_ranges = document
        .tokens()
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            matches!(&token.kind, TokenKind::Start { name, .. } if name.local == "ser")
                .then(|| element_token_end(&document, index).map(|end| (index, end)))
                .flatten()
        })
        .collect::<Vec<_>>();
    if series_ranges.len() != chart.series.len() {
        return Err(GenerateError::new(
            GenerateErrorCode::InvalidChart,
            format!(
                "chart has {} source series but {} replacements",
                series_ranges.len(),
                chart.series.len()
            ),
        ));
    }
    let mut patches = Vec::new();
    for (series_index, ((start, end), series)) in
        series_ranges.into_iter().zip(&chart.series).enumerate()
    {
        let column = spreadsheet_column(series_index + 2);
        replace_chart_container(
            source,
            &document,
            start,
            end,
            "tx",
            &["strCache"],
            std::slice::from_ref(&series.name),
            false,
            &format!("Sheet1!${column}$1"),
            &mut patches,
        )?;
        replace_chart_container(
            source,
            &document,
            start,
            end,
            "cat",
            &["strCache", "numCache"],
            &chart.categories,
            false,
            &format!("Sheet1!$A$2:$A${}", chart.categories.len() + 1),
            &mut patches,
        )?;
        let values = series
            .values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        replace_chart_container(
            source,
            &document,
            start,
            end,
            "val",
            &["numCache"],
            &values,
            true,
            &format!(
                "Sheet1!${column}$2:${column}${}",
                chart.categories.len() + 1
            ),
            &mut patches,
        )?;
    }
    apply_patches(source, patches)
}

#[allow(clippy::too_many_arguments)]
fn replace_chart_container(
    source: &[u8],
    document: &XmlDocument,
    series_start: usize,
    series_end: usize,
    container_name: &str,
    cache_names: &[&str],
    values: &[String],
    numeric: bool,
    formula: &str,
    patches: &mut Vec<Patch>,
) -> Result<(), GenerateError> {
    let (container_start, container_end) =
        find_element(document, series_start, series_end, &[container_name]).ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidChart,
                format!("chart series has no {container_name} container"),
            )
        })?;
    let (cache_start, cache_end) =
        find_element(document, container_start, container_end, cache_names).ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidChart,
                format!("chart {container_name} has no supported cache"),
            )
        })?;
    let prefix = xml_prefix(source, &document.tokens()[cache_start]);
    let mut replacement = Vec::new();
    if numeric {
        replacement.extend_from_slice(
            format!("<{prefix}:formatCode>General</{prefix}:formatCode>").as_bytes(),
        );
    }
    replacement
        .extend_from_slice(format!("<{prefix}:ptCount val=\"{}\"/>", values.len()).as_bytes());
    for (index, value) in values.iter().enumerate() {
        replacement.extend_from_slice(
            format!(
                "<{prefix}:pt idx=\"{index}\"><{prefix}:v>{}</{prefix}:v></{prefix}:pt>",
                escape_xml_text(value)
            )
            .as_bytes(),
        );
    }
    patches.push(Patch {
        range: element_inner_range(document, cache_start, cache_end)?,
        replacement,
    });
    if let Some((formula_start, formula_end)) =
        find_element(document, container_start, container_end, &["f"])
    {
        patches.push(Patch {
            range: element_inner_range(document, formula_start, formula_end)?,
            replacement: escape_xml_text(formula).into_bytes(),
        });
    }
    Ok(())
}

fn rewrite_embedded_workbook(source: &[u8], chart: &ChartData) -> Result<Vec<u8>, GenerateError> {
    let archive = ZipArchive::from_bytes(source.to_vec()).map_err(package_error)?;
    let sheet = archive.entry("xl/worksheets/sheet1.xml").ok_or_else(|| {
        GenerateError::new(
            GenerateErrorCode::InvalidChart,
            "embedded workbook has no xl/worksheets/sheet1.xml",
        )
    })?;
    let sheet_source = archive.read_entry(sheet).map_err(package_error)?;
    let document = XmlDocument::parse(sheet_source.clone()).map_err(GenerateError::xml)?;
    let (sheet_data_start, sheet_data_end) = find_element(
        &document,
        0,
        document.tokens().len().saturating_sub(1),
        &["sheetData"],
    )
    .ok_or_else(|| {
        GenerateError::new(
            GenerateErrorCode::InvalidChart,
            "embedded workbook sheet has no sheetData",
        )
    })?;
    let mut rows = String::new();
    rows.push_str("<row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>Category</t></is></c>");
    for (series_index, series) in chart.series.iter().enumerate() {
        let column = spreadsheet_column(series_index + 2);
        rows.push_str(&format!(
            "<c r=\"{column}1\" t=\"inlineStr\"><is><t>{}</t></is></c>",
            escape_xml_text(&series.name)
        ));
    }
    rows.push_str("</row>");
    for (category_index, category) in chart.categories.iter().enumerate() {
        let row = category_index + 2;
        rows.push_str(&format!(
            "<row r=\"{row}\"><c r=\"A{row}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
            escape_xml_text(category)
        ));
        for (series_index, series) in chart.series.iter().enumerate() {
            let column = spreadsheet_column(series_index + 2);
            rows.push_str(&format!(
                "<c r=\"{column}{row}\"><v>{}</v></c>",
                series.values[category_index]
            ));
        }
        rows.push_str("</row>");
    }
    let rewritten_sheet = apply_patches(
        &sheet_source,
        vec![Patch {
            range: element_inner_range(&document, sheet_data_start, sheet_data_end)?,
            replacement: rows.into_bytes(),
        }],
    )?;
    let mut writer = ZipWriter::new(VecSink::new());
    for entry in archive.entries() {
        if entry.name == "xl/worksheets/sheet1.xml" {
            writer
                .write_entry(&entry.name, &rewritten_sheet, &options_from_entry(entry))
                .map_err(package_error)?;
        } else {
            writer
                .raw_copy(&archive, entry, RewriteMode::Preserve)
                .map_err(package_error)?;
        }
    }
    Ok(writer.finish().map_err(package_error)?.0.into_inner())
}

fn find_element(
    document: &XmlDocument,
    start: usize,
    end: usize,
    names: &[&str],
) -> Option<(usize, usize)> {
    (start..=end).find_map(|index| {
        let TokenKind::Start { name, .. } = &document.tokens()[index].kind else {
            return None;
        };
        names
            .contains(&name.local.as_str())
            .then(|| element_token_end(document, index).map(|element_end| (index, element_end)))
            .flatten()
    })
}

fn element_token_end(document: &XmlDocument, start: usize) -> Option<usize> {
    let TokenKind::Start { name, empty, .. } = &document.tokens()[start].kind else {
        return None;
    };
    if *empty {
        return Some(start);
    }
    document.tokens()[start + 1..]
        .iter()
        .position(|token| {
            token.depth == document.tokens()[start].depth
                && matches!(&token.kind, TokenKind::End { name: end } if end == name)
        })
        .map(|offset| start + offset + 1)
}

fn element_inner_range(
    document: &XmlDocument,
    start: usize,
    end: usize,
) -> Result<Range<usize>, GenerateError> {
    if start == end {
        return Err(GenerateError::new(
            GenerateErrorCode::InvalidChart,
            "cannot replace the contents of an empty XML element",
        ));
    }
    Ok(document.tokens()[start].range.end..document.tokens()[end].range.start)
}

fn xml_prefix(source: &[u8], token: &wasmppt_xml::Token) -> String {
    let raw = std::str::from_utf8(&source[token.range.clone()]).unwrap_or("<c:");
    raw.trim_start_matches('<')
        .split([':', ' ', '>'])
        .next()
        .filter(|prefix| !prefix.is_empty())
        .unwrap_or("c")
        .to_owned()
}

fn spreadsheet_column(mut number: usize) -> String {
    let mut output = String::new();
    while number > 0 {
        number -= 1;
        output.insert(0, (b'A' + (number % 26) as u8) as char);
        number /= 26;
    }
    output
}

const TRANSITIONAL_PRESENTATION_NS: &str =
    "http://schemas.openxmlformats.org/presentationml/2006/main";
const STRICT_PRESENTATION_NS: &str = "http://purl.oclc.org/ooxml/presentationml/main";
const TRANSITIONAL_DRAWING_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const STRICT_DRAWING_NS: &str = "http://purl.oclc.org/ooxml/drawingml/main";

fn prepare_default_background_patches(
    archive: &ZipArchive<MemorySource>,
    graph: &PackageGraph,
    slide_deck: &SlideDeckPlan,
) -> Result<HashMap<String, Patch>, GenerateError> {
    let mut patches = HashMap::new();
    for slide in &slide_deck.slides {
        let slide_part = graph.part_by_name(&slide.part_name).ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                format!(
                    "slide part is missing from package graph: {}",
                    slide.part_name
                ),
            )
        })?;
        let layout = related_part(graph, slide_part.id, "/slideLayout");
        let master = layout.and_then(|part| related_part(graph, part, "/slideMaster"));
        let mut has_background = false;
        for part in [Some(slide_part.id), layout, master].into_iter().flatten() {
            has_background |= part_has_background(archive, graph, part)?;
        }
        if has_background {
            continue;
        }
        let target = master.or(layout).unwrap_or(slide_part.id);
        let target_name = graph.part_name(graph.part(target));
        if let std::collections::hash_map::Entry::Vacant(entry) =
            patches.entry(target_name.to_owned())
        {
            let source = read_graph_part(archive, graph, target)?;
            entry.insert(default_background_patch(target_name, &source)?);
        }
    }
    Ok(patches)
}

fn related_part(graph: &PackageGraph, source: PartId, suffix: &str) -> Option<PartId> {
    graph
        .part(source)
        .relationships
        .iter()
        .find(|relationship| graph.relationship_type(relationship).ends_with(suffix))
        .and_then(|relationship| match relationship.target {
            RelationshipTarget::Internal(part) => Some(part),
            _ => None,
        })
}

fn read_graph_part(
    archive: &ZipArchive<MemorySource>,
    graph: &PackageGraph,
    part: PartId,
) -> Result<Vec<u8>, GenerateError> {
    let name = graph.part_name(graph.part(part));
    let entry = archive.entry(name).ok_or_else(|| {
        GenerateError::new(
            GenerateErrorCode::InvalidTemplate,
            format!("package graph part is missing from archive: {name}"),
        )
    })?;
    archive.read_entry(entry).map_err(package_error)
}

fn part_has_background(
    archive: &ZipArchive<MemorySource>,
    graph: &PackageGraph,
    part: PartId,
) -> Result<bool, GenerateError> {
    let name = graph.part_name(graph.part(part));
    let source = read_graph_part(archive, graph, part)?;
    let document =
        XmlDocument::parse(source).map_err(|error| GenerateError::xml_in_part(error, name))?;
    Ok(document.tokens().iter().any(|token| {
        matches!(
            &token.kind,
            TokenKind::Start { name, .. }
                if name.local == "bg"
                    && name.namespace.is_some_and(|namespace| matches!(
                        document.namespace(namespace),
                        TRANSITIONAL_PRESENTATION_NS | STRICT_PRESENTATION_NS
                    ))
        )
    }))
}

fn default_background_patch(name: &str, source: &[u8]) -> Result<Patch, GenerateError> {
    let document = XmlDocument::parse(source.to_vec())
        .map_err(|error| GenerateError::xml_in_part(error, name))?;
    let (insertion_offset, presentation_namespace) = document
        .tokens()
        .iter()
        .find_map(|token| {
            let TokenKind::Start {
                name, empty: false, ..
            } = &token.kind
            else {
                return None;
            };
            (name.local == "cSld")
                .then(|| {
                    name.namespace
                        .map(|namespace| (token.range.end, document.namespace(namespace)))
                })
                .flatten()
        })
        .ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                format!("slide common data element is missing from {name}"),
            )
        })?;
    let drawing_namespace = match presentation_namespace {
        TRANSITIONAL_PRESENTATION_NS => TRANSITIONAL_DRAWING_NS,
        STRICT_PRESENTATION_NS => STRICT_DRAWING_NS,
        _ => {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                format!("slide common data has an unsupported namespace in {name}"),
            ));
        }
    };
    let replacement = format!(
        r#"<p:bg xmlns:p="{presentation_namespace}" xmlns:a="{drawing_namespace}"><p:bgPr><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>"#,
    )
    .into_bytes();
    Ok(Patch {
        range: insertion_offset..insertion_offset,
        replacement,
    })
}

fn prepare_slide_deck(
    archive: &ZipArchive<wasmppt_opc::MemorySource>,
) -> Result<SlideDeckPlan, GenerateError> {
    let presentation_part = "ppt/presentation.xml".to_owned();
    let relationship_part = "ppt/_rels/presentation.xml.rels".to_owned();
    let presentation = archive.entry(&presentation_part).ok_or_else(|| {
        GenerateError::new(
            GenerateErrorCode::InvalidTemplate,
            "presentation main part is missing",
        )
    })?;
    let presentation_bytes = archive.read_entry(presentation).map_err(package_error)?;
    let presentation_document =
        XmlDocument::parse(presentation_bytes).map_err(GenerateError::xml)?;
    let relationships = archive.entry(&relationship_part).ok_or_else(|| {
        GenerateError::new(
            GenerateErrorCode::InvalidTemplate,
            "presentation relationships are missing",
        )
    })?;
    let relationship_bytes = archive.read_entry(relationships).map_err(package_error)?;
    let relationship_document =
        XmlDocument::parse(relationship_bytes).map_err(GenerateError::xml)?;

    let mut relationship_map = HashMap::<String, (String, Range<usize>, String)>::new();
    let mut used_relationship_ids = HashSet::new();
    let mut relationship_insert_offset = None;
    for token in relationship_document.tokens() {
        match &token.kind {
            TokenKind::Start {
                name, attributes, ..
            } if name.local == "Relationship" => {
                let id = relationship_document.attribute(attributes, None, "Id");
                let target = relationship_document.attribute(attributes, None, "Target");
                let kind = relationship_document.attribute(attributes, None, "Type");
                if let (Some(id), Some(target), Some(kind)) = (id, target, kind) {
                    used_relationship_ids.insert(id.value.clone());
                    relationship_map.insert(
                        id.value.clone(),
                        (
                            target.value.clone(),
                            token.range.clone(),
                            kind.value.clone(),
                        ),
                    );
                }
            }
            TokenKind::End { name } if name.local == "Relationships" => {
                relationship_insert_offset = Some(token.range.start);
            }
            _ => {}
        }
    }
    let relationship_insert_offset = relationship_insert_offset.ok_or_else(|| {
        GenerateError::new(
            GenerateErrorCode::InvalidTemplate,
            "relationships closing tag is missing",
        )
    })?;
    let mut slides = Vec::new();
    let mut used_slide_parts = HashSet::new();
    for token in presentation_document.tokens() {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        if name.local != "sldId" {
            continue;
        }
        let slide_id = presentation_document
            .attribute(attributes, None, "id")
            .and_then(|attribute| attribute.value.parse::<u32>().ok())
            .ok_or_else(|| {
                GenerateError::new(GenerateErrorCode::InvalidTemplate, "slide ID is invalid")
            })?;
        let relationship_attribute = attributes
            .iter()
            .find(|attribute| attribute.name.local == "id" && attribute.name.namespace.is_some())
            .ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    "slide has no relationship ID",
                )
            })?;
        let (target, relationship_range, relationship_type) = relationship_map
            .get(&relationship_attribute.value)
            .cloned()
            .ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    "slide relationship is missing",
                )
            })?;
        let part_name = resolve_target(Some(&presentation_part), &target).ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                "slide target is invalid",
            )
        })?;
        used_slide_parts.insert(part_name.clone());
        slides.push(SlideRecord {
            part_name,
            slide_id,
            list_range: token.range.clone(),
            list_prefix: name.prefix.clone().unwrap_or_else(|| "p".to_owned()),
            list_relationship_prefix: relationship_attribute
                .name
                .prefix
                .clone()
                .unwrap_or_else(|| "r".to_owned()),
            relationship_range,
            relationship_type,
        });
    }

    let content_types_entry = archive.entry("[Content_Types].xml").ok_or_else(|| {
        GenerateError::new(
            GenerateErrorCode::InvalidTemplate,
            "content types part is missing",
        )
    })?;
    let content_types_bytes = archive
        .read_entry(content_types_entry)
        .map_err(package_error)?;
    let content_types_document =
        XmlDocument::parse(content_types_bytes).map_err(GenerateError::xml)?;
    let mut content_types = HashMap::new();
    let mut content_type_insert_offset = None;
    for token in content_types_document.tokens() {
        match &token.kind {
            TokenKind::Start {
                name, attributes, ..
            } if name.local == "Override" => {
                let part = content_types_document
                    .attribute(attributes, None, "PartName")
                    .map(|attribute| attribute.value.trim_start_matches('/'));
                let kind = content_types_document
                    .attribute(attributes, None, "ContentType")
                    .map(|attribute| attribute.value.as_str());
                if let (Some(part), Some(kind)) = (part, kind) {
                    if used_slide_parts.contains(part) {
                        content_types
                            .insert(part.to_owned(), (kind.to_owned(), token.range.clone()));
                    }
                }
            }
            TokenKind::End { name } if name.local == "Types" => {
                content_type_insert_offset = Some(token.range.start);
            }
            _ => {}
        }
    }
    Ok(SlideDeckPlan {
        presentation_part,
        relationship_part,
        relationship_insert_offset,
        slides,
        used_relationship_ids,
        used_slide_parts,
        content_type_insert_offset: content_type_insert_offset.ok_or_else(|| {
            GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                "content types closing tag is missing",
            )
        })?,
        content_types,
    })
}

fn enclosing_element_range(
    document: &XmlDocument,
    local: &str,
    offset: usize,
) -> Option<Range<usize>> {
    let mut candidates = Vec::<(usize, usize)>::new();
    for token in document.tokens() {
        match &token.kind {
            TokenKind::Start { name, empty, .. } if name.local == local && !empty => {
                candidates.push((token.depth, token.range.start));
            }
            TokenKind::End { name } if name.local == local => {
                if let Some(position) = candidates
                    .iter()
                    .rposition(|(depth, _)| *depth == token.depth)
                {
                    let (_, start) = candidates.remove(position);
                    if start <= offset && offset < token.range.end {
                        return Some(start..token.range.end);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn find_crop_plan(source: &[u8], relationship_id: &str) -> Result<CropPlan, GenerateError> {
    let document = XmlDocument::parse(source).map_err(GenerateError::xml)?;
    let mut found_blip = false;
    let mut insertion_offset = None;
    let mut prefix = "a".to_owned();
    for token in document.tokens() {
        let TokenKind::Start {
            name, attributes, ..
        } = &token.kind
        else {
            continue;
        };
        if name.local == "blip" {
            let matches = attributes.iter().any(|attribute| {
                attribute.name.local == "embed" && attribute.value == relationship_id
            });
            if matches {
                found_blip = true;
                insertion_offset = Some(token.range.end);
                prefix = name.prefix.clone().unwrap_or_else(|| "a".to_owned());
            } else if found_blip {
                break;
            }
        } else if found_blip && name.local == "srcRect" {
            let attr_range = |local: &str| {
                attributes
                    .iter()
                    .find(|attribute| attribute.name.local == local)
                    .map(|attribute| attribute.value_range.clone())
            };
            return Ok(CropPlan::Existing {
                left: attr_range("l"),
                top: attr_range("t"),
                right: attr_range("r"),
                bottom: attr_range("b"),
                element_range: token.range.clone(),
                prefix: name.prefix.clone().unwrap_or(prefix),
            });
        }
    }
    Ok(insertion_offset.map_or(CropPlan::None, |offset| CropPlan::Insert { offset, prefix }))
}

fn crop_patches(plan: &CropPlan, crop: ImageCrop) -> Vec<Patch> {
    let values = [
        ("l", crop.left),
        ("t", crop.top),
        ("r", crop.right),
        ("b", crop.bottom),
    ];
    match plan {
        CropPlan::Existing {
            left,
            top,
            right,
            bottom,
            element_range,
            prefix,
        } => {
            let ranges = [left, top, right, bottom];
            if ranges.iter().all(|range| range.is_some()) {
                ranges
                    .into_iter()
                    .zip(values)
                    .map(|(range, (_, value))| Patch {
                        range: range.clone().expect("all crop ranges present"),
                        replacement: value.to_string().into_bytes(),
                    })
                    .collect()
            } else {
                vec![Patch {
                    range: element_range.clone(),
                    replacement: crop_element(prefix, crop).into_bytes(),
                }]
            }
        }
        CropPlan::Insert { offset, prefix } => vec![Patch {
            range: *offset..*offset,
            replacement: crop_element(prefix, crop).into_bytes(),
        }],
        CropPlan::None => Vec::new(),
    }
}

fn crop_element(prefix: &str, crop: ImageCrop) -> String {
    format!(
        "<{prefix}:srcRect l=\"{}\" t=\"{}\" r=\"{}\" b=\"{}\"/>",
        crop.left, crop.top, crop.right, crop.bottom
    )
}

fn content_type_patches(
    source: &[u8],
    image_types: &BTreeMap<String, String>,
) -> Result<Vec<Patch>, GenerateError> {
    let document = XmlDocument::parse(source).map_err(GenerateError::xml)?;
    let mut patches = Vec::new();
    let mut present = HashSet::new();
    let mut end_offset = None;
    for token in document.tokens() {
        match &token.kind {
            TokenKind::Start {
                name, attributes, ..
            } if name.local == "Default" => {
                let Some(extension) = document.attribute(attributes, None, "Extension") else {
                    continue;
                };
                let extension_lower = extension.value.to_ascii_lowercase();
                let Some(content_type) = image_types.get(&extension_lower) else {
                    continue;
                };
                present.insert(extension_lower);
                if let Some(attribute) = document.attribute(attributes, None, "ContentType") {
                    if attribute.value != *content_type {
                        patches.push(Patch {
                            range: attribute.value_range.clone(),
                            replacement: escape_xml_attribute(content_type).into_bytes(),
                        });
                    }
                }
            }
            TokenKind::End { name } if name.local == "Types" => {
                end_offset = Some(token.range.start)
            }
            _ => {}
        }
    }
    let offset = end_offset.ok_or_else(|| {
        GenerateError::new(
            GenerateErrorCode::InvalidTemplate,
            "content types has no closing Types element",
        )
    })?;
    let mut insertion = String::new();
    for (extension, content_type) in image_types {
        if !present.contains(extension) {
            insertion.push_str(&format!(
                "<Default Extension=\"{}\" ContentType=\"{}\"/>",
                escape_xml_attribute(extension),
                escape_xml_attribute(content_type)
            ));
        }
    }
    if !insertion.is_empty() {
        patches.push(Patch {
            range: offset..offset,
            replacement: insertion.into_bytes(),
        });
    }
    Ok(patches)
}

fn strip_notes_relationships(source: &[u8]) -> Result<Vec<u8>, GenerateError> {
    let document = XmlDocument::parse(source).map_err(GenerateError::xml)?;
    let patches = document
        .tokens()
        .iter()
        .filter_map(|token| {
            let TokenKind::Start {
                name, attributes, ..
            } = &token.kind
            else {
                return None;
            };
            if name.local != "Relationship" {
                return None;
            }
            document
                .attribute(attributes, None, "Type")
                .is_some_and(|attribute| attribute.value.ends_with("/notesSlide"))
                .then(|| Patch {
                    range: token.range.clone(),
                    replacement: Vec::new(),
                })
        })
        .collect();
    apply_patches(source, patches)
}

fn validate_image(image: &ImageData) -> Result<(), GenerateError> {
    let extension_ok = !image.extension.is_empty()
        && image.extension.len() <= 16
        && image
            .extension
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric());
    let content_type_ok = image.content_type.starts_with("image/")
        && !image
            .content_type
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte == b'"');
    if image.bytes.is_empty() || !extension_ok || !content_type_ok {
        return Err(GenerateError::new(
            GenerateErrorCode::InvalidImage,
            "image requires non-empty bytes, a safe extension, and an image/* content type",
        ));
    }
    Ok(())
}

fn validate_semantic_shape(value: &SemanticShapeData) -> Result<(), GenerateError> {
    if value.copies.is_some_and(|copies| copies > 10_000) {
        return Err(GenerateError::new(
            GenerateErrorCode::InvalidTemplate,
            "semantic shape copies exceed 10000",
        ));
    }
    if let Some(runs) = &value.rich_text {
        if runs.len() > 10_000 {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidTemplate,
                "rich text run count exceeds 10000",
            ));
        }
        for run in runs {
            if run
                .font_size
                .is_some_and(|size| !(100..=40_000).contains(&size))
            {
                return Err(GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    "rich text font size is outside 1..400 points",
                ));
            }
            if run
                .color
                .as_ref()
                .is_some_and(|color| !valid_hex_color(color))
            {
                return Err(GenerateError::new(
                    GenerateErrorCode::InvalidTemplate,
                    "rich text color must be six hexadecimal digits",
                ));
            }
        }
    }
    if value
        .fill_color
        .as_ref()
        .is_some_and(|color| !valid_hex_color(color))
    {
        return Err(GenerateError::new(
            GenerateErrorCode::InvalidTemplate,
            "shape fill color must be six hexadecimal digits",
        ));
    }
    if value.hyperlink.as_ref().is_some_and(|link| {
        !(link.starts_with("https://")
            || link.starts_with("http://")
            || link.starts_with("mailto:")
            || link.starts_with("tel:"))
    }) {
        return Err(GenerateError::new(
            GenerateErrorCode::InvalidTemplate,
            "shape hyperlink uses an unsupported scheme",
        ));
    }
    Ok(())
}

fn valid_hex_color(color: &str) -> bool {
    color.len() == 6 && color.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn rich_text_xml(runs: &[RichTextRunData]) -> String {
    let mut output = String::new();
    for run in runs {
        output.push_str("<a:r><a:rPr");
        if let Some(value) = run.bold {
            output.push_str(if value { " b=\"1\"" } else { " b=\"0\"" });
        }
        if let Some(value) = run.italic {
            output.push_str(if value { " i=\"1\"" } else { " i=\"0\"" });
        }
        if let Some(value) = run.underline {
            output.push_str(if value { " u=\"sng\"" } else { " u=\"none\"" });
        }
        if let Some(value) = run.font_size {
            output.push_str(&format!(" sz=\"{value}\""));
        }
        if run.color.is_none() {
            output.push_str("/>");
        } else {
            output.push('>');
            output.push_str("<a:solidFill><a:srgbClr val=\"");
            output.push_str(run.color.as_deref().unwrap_or_default());
            output.push_str("\"/></a:solidFill></a:rPr>");
        }
        output.push_str("<a:t>");
        output.push_str(&escape_xml_text(&run.text));
        output.push_str("</a:t></a:r>");
    }
    output
}

fn record_dirty_bytes(total: &mut u64, peak: &mut u64, length: usize) {
    *total = total.saturating_add(length as u64);
    *peak = (*peak).max(length as u64);
}

fn options_from_entry(entry: &Entry) -> EntryOptions {
    EntryOptions {
        compression: match entry.compression {
            CompressionMethod::Stored => CompressionMethod::Stored,
            _ => CompressionMethod::Deflate,
        },
        modified_time: entry.modified_time,
        modified_date: entry.modified_date,
        local_extra: entry.local_extra.clone(),
        central_extra: entry.central_extra.clone(),
        comment: entry.comment.clone(),
        internal_attributes: entry.internal_attributes,
        external_attributes: entry.external_attributes,
    }
}

fn package_error(error: wasmppt_opc::Error) -> GenerateError {
    GenerateError {
        code: GenerateErrorCode::Package,
        message: error.to_string(),
        cause_code: Some(super::opc_error_code(error.code())),
    }
}
