use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

use sha2::{Digest, Sha256};
use wasmppt_opc::{
    CompressionMethod, Entry, EntryOptions, OutputSink, PackageGraph, ReadAt, RelationshipTarget,
    RewriteMode, VecSink, WriteStats, ZipArchive, ZipWriter,
};
use wasmppt_xml::{TokenKind, XmlDocument, decode_entities};

use crate::{BindingKind, BindingTarget, RelationshipAction, TemplatePlan};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImageCrop {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageData {
    pub bytes: Vec<u8>,
    pub extension: String,
    pub content_type: String,
    pub crop: Option<ImageCrop>,
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InjectionData {
    text: BTreeMap<String, String>,
    images: BTreeMap<String, ImageData>,
    table_rows: BTreeMap<String, Vec<BTreeMap<String, String>>>,
    slide_copies: BTreeMap<String, usize>,
    charts: BTreeMap<String, ChartData>,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenerateError {
    code: GenerateErrorCode,
    message: String,
}

impl GenerateError {
    fn new(code: GenerateErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub const fn code(&self) -> GenerateErrorCode {
        self.code
    }
}

impl std::fmt::Display for GenerateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for GenerateError {}

#[derive(Clone, Debug)]
struct Patch {
    range: Range<usize>,
    replacement: Vec<u8>,
}

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
    slide_deck: SlideDeckPlan,
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
}

impl PreparedTemplate {
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
        let chart_plans = prepare_chart_plans(&archive)?;
        let slide_deck = prepare_slide_deck(&archive)?;
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
                || chart_parts.contains(entry.name.as_str());
            if !scan {
                continue;
            }
            let source = archive.read_entry(entry).map_err(package_error)?;
            let patches = if entry.name == "[Content_Types].xml"
                || entry.name.ends_with(".rels")
                || entry.name.ends_with(".xml")
            {
                cleanup_patches(&entry.name, &source, &removed_parts)?
            } else {
                Vec::new()
            };
            if !patches.is_empty()
                || binding_parts.contains(entry.name.as_str())
                || image_relationship_parts.contains(entry.name.as_str())
                || entry.name == "[Content_Types].xml"
                || entry.name == slide_deck.presentation_part
                || entry.name == slide_deck.relationship_part
                || chart_parts.contains(entry.name.as_str())
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
            slide_deck,
        })
    }

    pub fn plan(&self) -> &TemplatePlan {
        &self.plan
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

    pub fn generate(&self, data: &InjectionData) -> Result<GenerateOutput, GenerateError> {
        let (sink, stats) = self.generate_to(data, VecSink::new())?;
        Ok(GenerateOutput {
            bytes: sink.into_inner(),
            zip_stats: stats.zip,
            rewritten_entries: stats.rewritten_entries,
            removed_entries: stats.removed_entries,
        })
    }

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
                    if let Some(crop) = image.crop {
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
            let mut replacement = Vec::new();
            for row in rows {
                let mut row_patches = Vec::new();
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
        for (part_name, chart) in &data.charts {
            validate_chart_data(chart)?;
            let chart_plan = self.chart_plans.get(part_name).ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidChart,
                    format!("no supported chart part named {part_name}"),
                )
            })?;
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
            writer
                .write_entry(&entry.name, &rewritten, &options_from_entry(entry))
                .map_err(package_error)?;
            rewritten_entries += 1;
        }
        for (name, (image, options)) in new_media {
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
            },
        ))
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
        let document = XmlDocument::parse(bytes)
            .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, error.to_string()))?;
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
        let document = XmlDocument::parse(bytes)
            .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, error.to_string()))?;
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
        let document = XmlDocument::parse(source)
            .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, error.to_string()))?;
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
    Ok(plans)
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
    let document = XmlDocument::parse(source)
        .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, error.to_string()))?;
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
    let document = XmlDocument::parse(sheet_source.clone())
        .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, error.to_string()))?;
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
    let presentation_document = XmlDocument::parse(presentation_bytes)
        .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, error.to_string()))?;
    let relationships = archive.entry(&relationship_part).ok_or_else(|| {
        GenerateError::new(
            GenerateErrorCode::InvalidTemplate,
            "presentation relationships are missing",
        )
    })?;
    let relationship_bytes = archive.read_entry(relationships).map_err(package_error)?;
    let relationship_document = XmlDocument::parse(relationship_bytes)
        .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, error.to_string()))?;

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
    let content_types_document = XmlDocument::parse(content_types_bytes)
        .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, error.to_string()))?;
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
    let document = XmlDocument::parse(source)
        .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, error.to_string()))?;
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
    let document = XmlDocument::parse(source)
        .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, error.to_string()))?;
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
    let document = XmlDocument::parse(source)
        .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, error.to_string()))?;
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

fn relationship_part_name(source: &str) -> Option<String> {
    let (directory, file) = source.rsplit_once('/').unwrap_or(("", source));
    Some(if directory.is_empty() {
        format!("_rels/{file}.rels")
    } else {
        format!("{directory}/_rels/{file}.rels")
    })
}

fn relationship_source(name: &str) -> Option<String> {
    if name == "_rels/.rels" {
        return None;
    }
    let (directory, file) = name.rsplit_once("/_rels/")?;
    Some(format!("{directory}/{}", file.strip_suffix(".rels")?))
}

fn resolve_target(source: Option<&str>, target: &str) -> Option<String> {
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

fn missing_value(id: &str) -> GenerateError {
    GenerateError::new(
        GenerateErrorCode::MissingValue,
        format!("missing value for binding {id}"),
    )
}

fn text_patches(
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
        let decoded = decode_entities(raw, range.start)
            .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, error.to_string()))?;
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

fn cleanup_patches(
    name: &str,
    source: &[u8],
    removed: &HashSet<String>,
) -> Result<Vec<Patch>, GenerateError> {
    let document = XmlDocument::parse(source)
        .map_err(|error| GenerateError::new(GenerateErrorCode::Xml, format!("{name}: {error}")))?;
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

fn apply_patches(source: &[u8], mut patches: Vec<Patch>) -> Result<Vec<u8>, GenerateError> {
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

fn escape_xml_text(value: &str) -> String {
    escape_xml(value)
}

fn escape_xml_attribute(value: &str) -> String {
    escape_xml(value)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn prohibited_part(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("vbaproject")
        || lower.contains("vbadata")
        || lower.starts_with("_xmlsignatures/")
        || lower.ends_with("origin.sigs")
}

fn prohibited_content_type(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("vba") || lower.contains("digital-signature")
}

fn prohibited_relationship_type(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("vbaproject") || lower.contains("vbadata") || lower.contains("digital-signature")
}

fn is_template_main_type(value: &str) -> bool {
    matches!(
        value,
        "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"
            | "application/vnd.ms-powerpoint.template.macroEnabled.main+xml"
            | "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml"
    )
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
    GenerateError::new(GenerateErrorCode::Package, error.to_string())
}
