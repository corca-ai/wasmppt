use crate::*;
use std::fmt;

const SPEC_MAGIC: &[u8; 4] = b"WDSF";
const TEMPLATE_MAGIC: &[u8; 4] = b"WDTP";
const PLAN_MAGIC: &[u8; 4] = b"WDPL";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireErrorKind {
    InvalidMagic,
    UnsupportedVersion,
    Truncated,
    InvalidTag,
    InvalidUtf8,
    LimitExceeded,
    TrailingBytes,
    InvalidData,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WireError {
    kind: WireErrorKind,
    message: String,
    limit_code: Option<DeckLimitCode>,
    limit: Option<u64>,
    actual: Option<u64>,
}

impl WireError {
    fn new(kind: WireErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            limit_code: None,
            limit: None,
            actual: None,
        }
    }

    fn limit(code: DeckLimitCode, label: &str, limit: usize, actual: usize) -> Self {
        Self {
            kind: WireErrorKind::LimitExceeded,
            message: format!("{label} exceeds the configured limit"),
            limit_code: Some(code),
            limit: Some(limit as u64),
            actual: Some(actual as u64),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> WireErrorKind {
        self.kind
    }

    #[must_use]
    pub const fn limit_code(&self) -> Option<DeckLimitCode> {
        self.limit_code
    }

    #[must_use]
    pub const fn limit_value(&self) -> Option<u64> {
        self.limit
    }

    #[must_use]
    pub const fn actual_value(&self) -> Option<u64> {
        self.actual
    }
}

impl fmt::Display for WireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for WireError {}

pub(crate) fn encode_spec(spec: &DeckSpec, limits: &DeckLimits) -> Result<Vec<u8>, WireError> {
    let mut writer = Writer::new(limits, SPEC_MAGIC, DeckSpec::SCHEMA_VERSION)?;
    writer.id(spec.id)?;
    writer.vec(&spec.logical_slides, |writer, slide| {
        writer.logical_slide(slide)
    })?;
    writer.vec(&spec.resources, |writer, resource| {
        writer.resource(resource)
    })?;
    writer.finish()
}

pub(crate) fn decode_spec(bytes: &[u8], limits: &DeckLimits) -> Result<DeckSpec, WireError> {
    let mut reader = Reader::new(bytes, limits, SPEC_MAGIC, DeckSpec::SCHEMA_VERSION)?;
    let spec = DeckSpec {
        id: reader.id()?,
        logical_slides: reader.vec("logical slides", |reader| reader.logical_slide(1))?,
        resources: reader.vec("resources", Reader::resource)?,
    };
    reader.finish()?;
    Ok(spec)
}

pub(crate) fn encode_template_plan(
    plan: &DeckTemplatePlan,
    limits: &DeckLimits,
) -> Result<Vec<u8>, WireError> {
    let mut writer = Writer::new(limits, TEMPLATE_MAGIC, DeckTemplatePlan::SCHEMA_VERSION)?;
    writer.id(plan.id)?;
    writer.raw(&plan.template_hash)?;
    writer.raw(&plan.cache_key)?;
    writer.u32(plan.validator_version)?;
    writer.string(&plan.compiler_policy)?;
    writer.emu_size(plan.page_size)?;
    writer.template_theme(&plan.theme)?;
    writer.vec(&plan.layouts, |writer, layout| {
        writer.template_layout(layout)
    })?;
    writer.vec(&plan.regions, |writer, region| {
        writer.template_region(region)
    })?;
    writer.vec(&plan.assets, |writer, asset| writer.template_asset(asset))?;
    writer.vec(&plan.diagnostics, |writer, diagnostic| {
        writer.diagnostic(diagnostic)
    })?;
    writer.finish()
}

pub(crate) fn decode_template_plan(
    bytes: &[u8],
    limits: &DeckLimits,
) -> Result<DeckTemplatePlan, WireError> {
    let mut reader = Reader::new(
        bytes,
        limits,
        TEMPLATE_MAGIC,
        DeckTemplatePlan::SCHEMA_VERSION,
    )?;
    let id = reader.id()?;
    let mut template_hash = [0; 32];
    template_hash.copy_from_slice(reader.take(32)?);
    let mut cache_key = [0; 32];
    cache_key.copy_from_slice(reader.take(32)?);
    let plan = DeckTemplatePlan {
        id,
        template_hash,
        cache_key,
        validator_version: reader.u32()?,
        compiler_policy: reader.string()?,
        page_size: reader.emu_size()?,
        theme: reader.template_theme()?,
        layouts: reader.vec("template layouts", Reader::template_layout)?,
        regions: reader.vec("template regions", Reader::template_region)?,
        assets: reader.vec("template assets", Reader::template_asset)?,
        diagnostics: reader.vec("template diagnostics", Reader::diagnostic)?,
    };
    reader.finish()?;
    Ok(plan)
}

pub(crate) fn encode_plan(plan: &DeckPlan, limits: &DeckLimits) -> Result<Vec<u8>, WireError> {
    if plan.pages.len() > limits.max_physical_pages {
        return Err(WireError::limit(
            DeckLimitCode::PHYSICAL_PAGES,
            "physical page count",
            limits.max_physical_pages,
            plan.pages.len(),
        ));
    }
    let mut writer = Writer::new(limits, PLAN_MAGIC, DeckPlan::SCHEMA_VERSION)?;
    writer.id(plan.id)?;
    writer.id(plan.spec_id)?;
    writer.id(plan.template_id)?;
    writer.emu_size(plan.page_size)?;
    writer.vec(&plan.pages, |writer, page| writer.physical_page(page))?;
    writer.vec(&plan.diagnostics, |writer, diagnostic| {
        writer.diagnostic(diagnostic)
    })?;
    writer.finish()
}

pub(crate) fn decode_plan(bytes: &[u8], limits: &DeckLimits) -> Result<DeckPlan, WireError> {
    let mut reader = Reader::new(bytes, limits, PLAN_MAGIC, DeckPlan::SCHEMA_VERSION)?;
    let id = reader.id()?;
    let spec_id = reader.id()?;
    let template_id = reader.id()?;
    let page_size = reader.emu_size()?;
    let page_count = reader.count("physical pages")?;
    if page_count > limits.max_physical_pages {
        return Err(WireError::limit(
            DeckLimitCode::PHYSICAL_PAGES,
            "physical page count",
            limits.max_physical_pages,
            page_count,
        ));
    }
    let mut pages = Vec::with_capacity(page_count);
    for _ in 0..page_count {
        pages.push(reader.physical_page()?);
    }
    let plan = DeckPlan {
        id,
        spec_id,
        template_id,
        page_size,
        pages,
        diagnostics: reader.vec("plan diagnostics", Reader::diagnostic)?,
    };
    reader.finish()?;
    Ok(plan)
}

struct Writer<'a> {
    bytes: Vec<u8>,
    limits: &'a DeckLimits,
    semantic_nodes: usize,
    resource_bytes: usize,
    fragments: usize,
}

impl<'a> Writer<'a> {
    fn new(limits: &'a DeckLimits, magic: &[u8; 4], version: u32) -> Result<Self, WireError> {
        let mut writer = Self {
            bytes: Vec::new(),
            limits,
            semantic_nodes: 0,
            resource_bytes: 0,
            fragments: 0,
        };
        writer.raw(magic)?;
        writer.u32(version)?;
        Ok(writer)
    }

    fn finish(self) -> Result<Vec<u8>, WireError> {
        if self.bytes.len() > self.limits.max_payload_bytes {
            return Err(WireError::limit(
                DeckLimitCode::PAYLOAD_BYTES,
                "encoded payload",
                self.limits.max_payload_bytes,
                self.bytes.len(),
            ));
        }
        Ok(self.bytes)
    }

    fn raw(&mut self, value: &[u8]) -> Result<(), WireError> {
        let actual = self.bytes.len().saturating_add(value.len());
        if actual > self.limits.max_payload_bytes {
            return Err(WireError::limit(
                DeckLimitCode::PAYLOAD_BYTES,
                "encoded payload",
                self.limits.max_payload_bytes,
                actual,
            ));
        }
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    fn byte(&mut self, value: u8) -> Result<(), WireError> {
        self.raw(&[value])
    }

    fn bool(&mut self, value: bool) -> Result<(), WireError> {
        self.byte(u8::from(value))
    }

    fn u16(&mut self, value: u16) -> Result<(), WireError> {
        self.raw(&value.to_le_bytes())
    }

    fn u32(&mut self, value: u32) -> Result<(), WireError> {
        self.raw(&value.to_le_bytes())
    }

    fn i64(&mut self, value: i64) -> Result<(), WireError> {
        self.raw(&value.to_le_bytes())
    }

    fn f64(&mut self, value: f64) -> Result<(), WireError> {
        self.raw(&value.to_bits().to_le_bytes())
    }

    fn id(&mut self, value: StableId) -> Result<(), WireError> {
        self.raw(value.as_bytes())
    }

    fn string(&mut self, value: &str) -> Result<(), WireError> {
        if value.len() > self.limits.max_string_bytes {
            return Err(WireError::limit(
                DeckLimitCode::STRING_BYTES,
                "string",
                self.limits.max_string_bytes,
                value.len(),
            ));
        }
        self.u32(length_u32(value.len(), "string")?)?;
        self.raw(value.as_bytes())
    }

    fn optional_string(&mut self, value: Option<&str>) -> Result<(), WireError> {
        self.bool(value.is_some())?;
        if let Some(value) = value {
            self.string(value)?;
        }
        Ok(())
    }

    fn bytes(&mut self, value: &[u8]) -> Result<(), WireError> {
        self.u32(length_u32(value.len(), "byte sequence")?)?;
        self.raw(value)
    }

    fn vec<T>(
        &mut self,
        values: &[T],
        mut encode: impl FnMut(&mut Self, &T) -> Result<(), WireError>,
    ) -> Result<(), WireError> {
        if values.len() > self.limits.max_collection_items {
            return Err(WireError::limit(
                DeckLimitCode::COLLECTION_ITEMS,
                "collection",
                self.limits.max_collection_items,
                values.len(),
            ));
        }
        self.u32(length_u32(values.len(), "collection")?)?;
        for value in values {
            encode(self, value)?;
        }
        Ok(())
    }

    fn source(&mut self, source: &SourceRange) -> Result<(), WireError> {
        self.string(&source.source)?;
        self.u32(source.start)?;
        self.u32(source.end)
    }

    fn logical_slide(&mut self, slide: &LogicalSlide) -> Result<(), WireError> {
        self.id(slide.id)?;
        self.source(&slide.source)?;
        self.byte(logical_slide_kind_tag(slide.kind))?;
        self.bool(slide.hidden)?;
        self.vec(&slide.nodes, |writer, node| writer.semantic_node(node, 1))?;
        self.vec(&slide.media_text_relations, |writer, relation| {
            writer.media_text_relation(relation)
        })
    }

    fn media_text_relation(&mut self, relation: &MediaTextRelation) -> Result<(), WireError> {
        self.id(relation.media_node_id)?;
        self.id(relation.text_node_id)?;
        self.byte(media_text_proximity_tag(relation.proximity))?;
        self.byte(media_text_side_tag(relation.text_side))?;
        self.bool(relation.explicit_caption)
    }

    fn semantic_node(&mut self, node: &SemanticNode, depth: usize) -> Result<(), WireError> {
        self.semantic_nodes = self.semantic_nodes.saturating_add(1);
        if self.semantic_nodes > self.limits.max_semantic_nodes {
            return Err(WireError::limit(
                DeckLimitCode::SEMANTIC_NODES,
                "semantic node count",
                self.limits.max_semantic_nodes,
                self.semantic_nodes,
            ));
        }
        if depth > self.limits.max_nesting_depth {
            return Err(WireError::limit(
                DeckLimitCode::NESTING_DEPTH,
                "semantic nesting depth",
                self.limits.max_nesting_depth,
                depth,
            ));
        }
        self.id(node.id)?;
        self.source(&node.source)?;
        self.u16(node.role.code())?;
        self.byte(split_policy_tag(node.split))?;
        self.semantic_content(&node.content, depth)
    }

    fn semantic_content(
        &mut self,
        content: &SemanticContent,
        depth: usize,
    ) -> Result<(), WireError> {
        match content {
            SemanticContent::Text(text) => {
                self.byte(1)?;
                self.rich_text(text)
            }
            SemanticContent::Children(children) => {
                self.byte(2)?;
                self.vec(children, |writer, child| {
                    writer.semantic_node(child, depth + 1)
                })
            }
            SemanticContent::Image(image) => {
                self.byte(3)?;
                self.id(image.resource_id)?;
                self.string(&image.alt_text)
            }
            SemanticContent::List(list) => {
                self.byte(4)?;
                self.list(list, depth + 1)
            }
            SemanticContent::Table(table) => {
                self.byte(5)?;
                self.table(table)
            }
            SemanticContent::Chart(chart) => {
                self.byte(6)?;
                self.chart(chart)
            }
            SemanticContent::Code(code) => {
                self.byte(7)?;
                self.optional_string(code.language.as_deref())?;
                self.string(&code.code)
            }
            SemanticContent::Svg(svg) => {
                self.byte(8)?;
                self.id(svg.resource_id)?;
                self.optional_id(svg.fallback_resource_id)?;
                self.optional_string(svg.source_text.as_deref())
            }
        }
    }

    fn rich_text(&mut self, text: &RichText) -> Result<(), WireError> {
        self.vec(&text.runs, |writer, run| {
            writer.string(&run.text)?;
            let marks = u8::from(run.marks.bold)
                | (u8::from(run.marks.italic) << 1)
                | (u8::from(run.marks.strikethrough) << 2)
                | (u8::from(run.marks.inline_code) << 3);
            writer.byte(marks)?;
            writer.bool(run.hyperlink.is_some())?;
            if let Some(link) = &run.hyperlink {
                writer.byte(hyperlink_kind_tag(link.kind))?;
                writer.string(&link.target)?;
            }
            Ok(())
        })
    }

    fn list(&mut self, list: &ListContent, depth: usize) -> Result<(), WireError> {
        if depth > self.limits.max_nesting_depth {
            return Err(WireError::limit(
                DeckLimitCode::NESTING_DEPTH,
                "list nesting depth",
                self.limits.max_nesting_depth,
                depth,
            ));
        }
        self.bool(list.ordered)?;
        self.u32(list.start)?;
        self.vec(&list.items, |writer, item| {
            writer.id(item.id)?;
            writer.source(&item.source)?;
            writer.vec(&item.blocks, |writer, block| {
                writer.semantic_node(block, depth + 1)
            })?;
            writer.vec(&item.children, |writer, child| {
                writer.list(child, depth + 1)
            })
        })
    }

    fn table(&mut self, table: &TableContent) -> Result<(), WireError> {
        self.vec(&table.columns, |writer, column| {
            writer.id(column.id)?;
            writer.source(&column.source)?;
            writer.byte(table_column_alignment_tag(column.alignment))
        })?;
        self.u32(table.header_rows)?;
        self.vec(&table.rows, |writer, row| {
            writer.id(row.id)?;
            writer.source(&row.source)?;
            writer.vec(&row.cells, |writer, cell| {
                writer.id(cell.id)?;
                writer.source(&cell.source)?;
                writer.rich_text(&cell.content)
            })
        })
    }

    fn chart(&mut self, chart: &ChartContent) -> Result<(), WireError> {
        self.byte(chart_kind_tag(chart.kind))?;
        self.vec(&chart.categories, |writer, category| {
            writer.string(category)
        })?;
        self.vec(&chart.series, |writer, series| {
            writer.string(&series.name)?;
            writer.vec(&series.values, |writer, value| writer.f64(*value))
        })
    }

    fn resource(&mut self, resource: &DeckResource) -> Result<(), WireError> {
        if resource.bytes.len() > self.limits.max_resource_bytes {
            return Err(WireError::limit(
                DeckLimitCode::RESOURCE_BYTES,
                "resource",
                self.limits.max_resource_bytes,
                resource.bytes.len(),
            ));
        }
        self.resource_bytes = self.resource_bytes.saturating_add(resource.bytes.len());
        if self.resource_bytes > self.limits.max_total_resource_bytes {
            return Err(WireError::limit(
                DeckLimitCode::TOTAL_RESOURCE_BYTES,
                "total resource bytes",
                self.limits.max_total_resource_bytes,
                self.resource_bytes,
            ));
        }
        self.id(resource.id)?;
        self.byte(resource_kind_tag(resource.kind))?;
        self.string(&resource.media_type)?;
        self.bytes(&resource.bytes)?;
        self.bool(resource.intrinsic_size.is_some())?;
        if let Some(size) = resource.intrinsic_size {
            self.u32(size.width)?;
            self.u32(size.height)?;
        }
        Ok(())
    }

    fn emu_size(&mut self, size: EmuSize) -> Result<(), WireError> {
        self.i64(size.width)?;
        self.i64(size.height)
    }

    fn rect(&mut self, rect: EmuRect) -> Result<(), WireError> {
        self.i64(rect.x)?;
        self.i64(rect.y)?;
        self.i64(rect.width)?;
        self.i64(rect.height)
    }

    fn template_region(&mut self, region: &TemplateRegion) -> Result<(), WireError> {
        self.id(region.id)?;
        self.id(region.layout_id)?;
        self.byte(region_role_tag(region.role))?;
        self.string(&region.placeholder.kind)?;
        self.u32(region.placeholder.index)?;
        self.rect(region.frame)?;
        self.bool(region.bleed_frame.is_some())?;
        if let Some(frame) = region.bleed_frame {
            self.rect(frame)?;
        }
        self.text_margins(region.margins)?;
        self.vec(&region.text_levels, |writer, level| {
            writer.template_text_level(level)
        })?;
        self.vec(&region.accepts, |writer, role| writer.u16(role.code()))?;
        self.bool(region.required)
    }

    fn template_theme(&mut self, theme: &TemplateTheme) -> Result<(), WireError> {
        self.theme_fonts(&theme.major_fonts)?;
        self.theme_fonts(&theme.minor_fonts)?;
        self.vec(&theme.colors, |writer, color| {
            writer.string(&color.slot)?;
            writer.u32(color.rgb)
        })
    }

    fn theme_fonts(&mut self, fonts: &ThemeFontSet) -> Result<(), WireError> {
        self.optional_string(fonts.latin.as_deref())?;
        self.optional_string(fonts.east_asian.as_deref())?;
        self.optional_string(fonts.complex_script.as_deref())
    }

    fn template_layout(&mut self, layout: &TemplateLayout) -> Result<(), WireError> {
        self.id(layout.id)?;
        self.byte(template_layout_capability_tag(layout.capability))?;
        self.string(&layout.matching_name)?;
        self.string(&layout.source_part)?;
        self.string(&layout.master_part)?;
        self.vec(&layout.region_ids, |writer, id| writer.id(*id))?;
        self.vec(&layout.asset_ids, |writer, id| writer.id(*id))?;
        self.bool(layout.background.is_some())?;
        if let Some(background) = &layout.background {
            self.source(background)?;
        }
        Ok(())
    }

    fn text_margins(&mut self, margins: TextMargins) -> Result<(), WireError> {
        self.i64(margins.left)?;
        self.i64(margins.top)?;
        self.i64(margins.right)?;
        self.i64(margins.bottom)
    }

    fn template_text_level(&mut self, level: &TemplateTextLevel) -> Result<(), WireError> {
        self.byte(level.level)?;
        self.optional_u32(level.font_size)?;
        self.optional_string(level.latin_typeface.as_deref())?;
        self.optional_string(level.east_asian_typeface.as_deref())?;
        self.optional_string(level.complex_script_typeface.as_deref())?;
        self.bool(level.color.is_some())?;
        if let Some(color) = &level.color {
            self.optional_string(color.scheme.as_deref())?;
            self.u32(color.rgb)?;
        }
        self.optional_bool(level.bold)?;
        self.optional_bool(level.italic)?;
        self.optional_i64(level.margin_left)?;
        self.optional_i64(level.indent)
    }

    fn template_asset(&mut self, asset: &TemplateAsset) -> Result<(), WireError> {
        self.id(asset.id)?;
        self.id(asset.layout_id)?;
        self.byte(template_asset_kind_tag(asset.kind))?;
        self.string(&asset.source_part)?;
        self.source(&asset.source_xml)?;
        self.bool(asset.frame.is_some())?;
        if let Some(frame) = asset.frame {
            self.rect(frame)?;
        }
        self.u32(asset.z_order)?;
        self.vec(&asset.related_parts, |writer, part| writer.string(part))
    }

    fn optional_u32(&mut self, value: Option<u32>) -> Result<(), WireError> {
        self.bool(value.is_some())?;
        if let Some(value) = value {
            self.u32(value)?;
        }
        Ok(())
    }

    fn optional_i64(&mut self, value: Option<i64>) -> Result<(), WireError> {
        self.bool(value.is_some())?;
        if let Some(value) = value {
            self.i64(value)?;
        }
        Ok(())
    }

    fn optional_bool(&mut self, value: Option<bool>) -> Result<(), WireError> {
        self.bool(value.is_some())?;
        if let Some(value) = value {
            self.bool(value)?;
        }
        Ok(())
    }

    fn diagnostic(&mut self, diagnostic: &DeckDiagnostic) -> Result<(), WireError> {
        self.u16(diagnostic.code.0)?;
        self.byte(diagnostic_severity_tag(diagnostic.severity))?;
        self.string(&diagnostic.message)?;
        self.bool(diagnostic.source.is_some())?;
        if let Some(source) = &diagnostic.source {
            self.source(source)?;
        }
        self.optional_id(diagnostic.node_id)?;
        self.optional_id(diagnostic.page_id)
    }

    fn optional_id(&mut self, id: Option<StableId>) -> Result<(), WireError> {
        self.bool(id.is_some())?;
        if let Some(id) = id {
            self.id(id)?;
        }
        Ok(())
    }

    fn physical_page(&mut self, page: &PhysicalPage) -> Result<(), WireError> {
        self.id(page.id)?;
        self.id(page.logical_slide_id)?;
        self.id(page.template_layout_id)?;
        self.byte(layout_topology_tag(page.topology.kind))?;
        self.u16(page.topology.slot_count)?;
        self.bool(page.hidden)?;
        self.u32(page.continuation.ordinal)?;
        self.u32(page.continuation.total)?;
        self.optional_id(page.continuation.repeated_heading_node_id)?;
        self.optional_string(page.continuation.label.as_deref())?;
        self.vec(&page.regions, |writer, region| {
            writer.planned_region(region)
        })
    }

    fn planned_region(&mut self, region: &PlannedRegion) -> Result<(), WireError> {
        self.id(region.template_region_id)?;
        self.region_placement(region.placement)?;
        self.rect(region.frame)?;
        self.vec(&region.fragments, |writer, fragment| {
            writer.fragments = writer.fragments.saturating_add(1);
            if writer.fragments > writer.limits.max_planned_fragments {
                return Err(WireError::limit(
                    DeckLimitCode::PLANNED_FRAGMENTS,
                    "planned fragment count",
                    writer.limits.max_planned_fragments,
                    writer.fragments,
                ));
            }
            writer.id(fragment.id)?;
            writer.id(fragment.source_node_id)?;
            writer.fragment_slice(fragment.slice)?;
            writer.rect(fragment.frame)?;
            writer.u32(fragment.type_choice.font_size)?;
            writer.bool(fragment.media.is_some())?;
            if let Some(media) = fragment.media {
                writer.rect(media.slot)?;
                writer.rect(media.visible_frame)?;
                writer.byte(content_fit_tag(media.fit))?;
                writer.u32(media.source_size.width)?;
                writer.u32(media.source_size.height)?;
                writer.bool(media.crop.is_some())?;
                if let Some(crop) = media.crop {
                    writer.u32(crop.left)?;
                    writer.u32(crop.top)?;
                    writer.u32(crop.right)?;
                    writer.u32(crop.bottom)?;
                }
            }
            writer.u32(fragment.repeat_table_header_rows)
        })
    }

    fn region_placement(&mut self, placement: RegionPlacement) -> Result<(), WireError> {
        match placement {
            RegionPlacement::Fixed => self.byte(0),
            RegionPlacement::Slot(index) => {
                self.byte(1)?;
                self.u16(index)
            }
        }
    }

    fn fragment_slice(&mut self, slice: FragmentSlice) -> Result<(), WireError> {
        match slice {
            FragmentSlice::Whole => self.byte(0),
            FragmentSlice::Text { start, end } => {
                self.byte(1)?;
                self.u32(start)?;
                self.u32(end)
            }
            FragmentSlice::ListItems { start, end } => {
                self.byte(2)?;
                self.u32(start)?;
                self.u32(end)
            }
            FragmentSlice::TableRows { start, end } => {
                self.byte(3)?;
                self.u32(start)?;
                self.u32(end)
            }
            FragmentSlice::CodeLines { start, end } => {
                self.byte(4)?;
                self.u32(start)?;
                self.u32(end)
            }
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
    limits: &'a DeckLimits,
    semantic_nodes: usize,
    resource_bytes: usize,
    fragments: usize,
}

impl<'a> Reader<'a> {
    fn new(
        bytes: &'a [u8],
        limits: &'a DeckLimits,
        magic: &[u8; 4],
        supported_version: u32,
    ) -> Result<Self, WireError> {
        if bytes.len() > limits.max_payload_bytes {
            return Err(WireError::limit(
                DeckLimitCode::PAYLOAD_BYTES,
                "payload",
                limits.max_payload_bytes,
                bytes.len(),
            ));
        }
        let mut reader = Self {
            bytes,
            cursor: 0,
            limits,
            semantic_nodes: 0,
            resource_bytes: 0,
            fragments: 0,
        };
        if reader.take(4)? != magic {
            return Err(WireError::new(
                WireErrorKind::InvalidMagic,
                "deck payload has an invalid magic value",
            ));
        }
        let version = reader.u32()?;
        if version != supported_version {
            return Err(WireError::new(
                WireErrorKind::UnsupportedVersion,
                format!("unsupported deck payload schema version {version}"),
            ));
        }
        Ok(reader)
    }

    fn finish(&self) -> Result<(), WireError> {
        if self.cursor != self.bytes.len() {
            return Err(WireError::new(
                WireErrorKind::TrailingBytes,
                "deck payload contains trailing bytes",
            ));
        }
        Ok(())
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], WireError> {
        let end = self.cursor.checked_add(length).ok_or_else(|| {
            WireError::new(WireErrorKind::Truncated, "deck payload offset overflow")
        })?;
        let value = self
            .bytes
            .get(self.cursor..end)
            .ok_or_else(|| WireError::new(WireErrorKind::Truncated, "deck payload is truncated"))?;
        self.cursor = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, WireError> {
        Ok(self.take(1)?[0])
    }

    fn bool(&mut self) -> Result<bool, WireError> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid_tag("boolean")),
        }
    }

    fn u16(&mut self) -> Result<u16, WireError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("two bytes"),
        ))
    }

    fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }

    fn i64(&mut self) -> Result<i64, WireError> {
        Ok(i64::from_le_bytes(
            self.take(8)?.try_into().expect("eight bytes"),
        ))
    }

    fn f64(&mut self) -> Result<f64, WireError> {
        Ok(f64::from_bits(u64::from_le_bytes(
            self.take(8)?.try_into().expect("eight bytes"),
        )))
    }

    fn id(&mut self) -> Result<StableId, WireError> {
        Ok(StableId::from_bytes(
            self.take(16)?.try_into().expect("sixteen bytes"),
        ))
    }

    fn string(&mut self) -> Result<String, WireError> {
        let length = self.u32()? as usize;
        if length > self.limits.max_string_bytes {
            return Err(WireError::limit(
                DeckLimitCode::STRING_BYTES,
                "string",
                self.limits.max_string_bytes,
                length,
            ));
        }
        let value = std::str::from_utf8(self.take(length)?).map_err(|_| {
            WireError::new(WireErrorKind::InvalidUtf8, "deck string is not valid UTF-8")
        })?;
        Ok(value.to_owned())
    }

    fn optional_string(&mut self) -> Result<Option<String>, WireError> {
        self.bool()?.then(|| self.string()).transpose()
    }

    fn bytes(&mut self, maximum: usize, code: DeckLimitCode) -> Result<Vec<u8>, WireError> {
        let length = self.u32()? as usize;
        if length > maximum {
            return Err(WireError::limit(code, "byte sequence", maximum, length));
        }
        Ok(self.take(length)?.to_vec())
    }

    fn count(&mut self, label: &str) -> Result<usize, WireError> {
        let count = self.u32()? as usize;
        if count > self.limits.max_collection_items {
            return Err(WireError::limit(
                DeckLimitCode::COLLECTION_ITEMS,
                label,
                self.limits.max_collection_items,
                count,
            ));
        }
        Ok(count)
    }

    fn vec<T>(
        &mut self,
        label: &str,
        mut decode: impl FnMut(&mut Self) -> Result<T, WireError>,
    ) -> Result<Vec<T>, WireError> {
        let count = self.count(label)?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(decode(self)?);
        }
        Ok(values)
    }

    fn source(&mut self) -> Result<SourceRange, WireError> {
        Ok(SourceRange {
            source: self.string()?,
            start: self.u32()?,
            end: self.u32()?,
        })
    }

    fn logical_slide(&mut self, depth: usize) -> Result<LogicalSlide, WireError> {
        Ok(LogicalSlide {
            id: self.id()?,
            source: self.source()?,
            kind: logical_slide_kind(self.byte()?)?,
            hidden: self.bool()?,
            nodes: self.vec("semantic nodes", |reader| reader.semantic_node(depth))?,
            media_text_relations: self.vec("media-text relations", Reader::media_text_relation)?,
        })
    }

    fn media_text_relation(&mut self) -> Result<MediaTextRelation, WireError> {
        Ok(MediaTextRelation {
            media_node_id: self.id()?,
            text_node_id: self.id()?,
            proximity: media_text_proximity(self.byte()?)?,
            text_side: media_text_side(self.byte()?)?,
            explicit_caption: self.bool()?,
        })
    }

    fn semantic_node(&mut self, depth: usize) -> Result<SemanticNode, WireError> {
        self.semantic_nodes = self.semantic_nodes.saturating_add(1);
        if self.semantic_nodes > self.limits.max_semantic_nodes {
            return Err(WireError::limit(
                DeckLimitCode::SEMANTIC_NODES,
                "semantic node count",
                self.limits.max_semantic_nodes,
                self.semantic_nodes,
            ));
        }
        if depth > self.limits.max_nesting_depth {
            return Err(WireError::limit(
                DeckLimitCode::NESTING_DEPTH,
                "semantic nesting depth",
                self.limits.max_nesting_depth,
                depth,
            ));
        }
        Ok(SemanticNode {
            id: self.id()?,
            source: self.source()?,
            role: semantic_role(self.u16()?)?,
            split: split_policy(self.byte()?)?,
            content: self.semantic_content(depth)?,
        })
    }

    fn semantic_content(&mut self, depth: usize) -> Result<SemanticContent, WireError> {
        match self.byte()? {
            1 => Ok(SemanticContent::Text(self.rich_text()?)),
            2 => Ok(SemanticContent::Children(
                self.vec("semantic children", |reader| {
                    reader.semantic_node(depth + 1)
                })?,
            )),
            3 => Ok(SemanticContent::Image(ImageContent {
                resource_id: self.id()?,
                alt_text: self.string()?,
            })),
            4 => Ok(SemanticContent::List(self.list(depth + 1)?)),
            5 => Ok(SemanticContent::Table(self.table()?)),
            6 => Ok(SemanticContent::Chart(self.chart()?)),
            7 => Ok(SemanticContent::Code(CodeContent {
                language: self.optional_string()?,
                code: self.string()?,
            })),
            8 => Ok(SemanticContent::Svg(SvgContent {
                resource_id: self.id()?,
                fallback_resource_id: self.optional_id()?,
                source_text: self.optional_string()?,
            })),
            _ => Err(invalid_tag("semantic content")),
        }
    }

    fn rich_text(&mut self) -> Result<RichText, WireError> {
        Ok(RichText {
            runs: self.vec("rich-text runs", |reader| {
                let text = reader.string()?;
                let marks = reader.byte()?;
                if marks & !0x0f != 0 {
                    return Err(invalid_tag("rich-text marks"));
                }
                let hyperlink = if reader.bool()? {
                    Some(SafeHyperlink {
                        kind: hyperlink_kind(reader.byte()?)?,
                        target: reader.string()?,
                    })
                } else {
                    None
                };
                Ok(RichTextRun {
                    text,
                    marks: TextMarks {
                        bold: marks & 1 != 0,
                        italic: marks & 2 != 0,
                        strikethrough: marks & 4 != 0,
                        inline_code: marks & 8 != 0,
                    },
                    hyperlink,
                })
            })?,
        })
    }

    fn list(&mut self, depth: usize) -> Result<ListContent, WireError> {
        if depth > self.limits.max_nesting_depth {
            return Err(WireError::limit(
                DeckLimitCode::NESTING_DEPTH,
                "list nesting depth",
                self.limits.max_nesting_depth,
                depth,
            ));
        }
        Ok(ListContent {
            ordered: self.bool()?,
            start: self.u32()?,
            items: self.vec("list items", |reader| {
                Ok(ListItem {
                    id: reader.id()?,
                    source: reader.source()?,
                    blocks: reader
                        .vec("list item blocks", |reader| reader.semantic_node(depth + 1))?,
                    children: reader.vec("nested lists", |reader| reader.list(depth + 1))?,
                })
            })?,
        })
    }

    fn table(&mut self) -> Result<TableContent, WireError> {
        let columns = self.vec("table columns", |reader| {
            Ok(TableColumn {
                id: reader.id()?,
                source: reader.source()?,
                alignment: table_column_alignment(reader.byte()?)?,
            })
        })?;
        let header_rows = self.u32()?;
        let rows = self.vec("table rows", |reader| {
            Ok(TableRow {
                id: reader.id()?,
                source: reader.source()?,
                cells: reader.vec("table cells", |reader| {
                    Ok(TableCell {
                        id: reader.id()?,
                        source: reader.source()?,
                        content: reader.rich_text()?,
                    })
                })?,
            })
        })?;
        Ok(TableContent {
            columns,
            header_rows,
            rows,
        })
    }

    fn chart(&mut self) -> Result<ChartContent, WireError> {
        Ok(ChartContent {
            kind: chart_kind(self.byte()?)?,
            categories: self.vec("chart categories", Reader::string)?,
            series: self.vec("chart series", |reader| {
                Ok(ChartSeries {
                    name: reader.string()?,
                    values: reader.vec("chart values", Reader::f64)?,
                })
            })?,
        })
    }

    fn resource(&mut self) -> Result<DeckResource, WireError> {
        let id = self.id()?;
        let kind = resource_kind(self.byte()?)?;
        let media_type = self.string()?;
        let bytes = self.bytes(
            self.limits.max_resource_bytes,
            DeckLimitCode::RESOURCE_BYTES,
        )?;
        self.resource_bytes = self.resource_bytes.saturating_add(bytes.len());
        if self.resource_bytes > self.limits.max_total_resource_bytes {
            return Err(WireError::limit(
                DeckLimitCode::TOTAL_RESOURCE_BYTES,
                "total resource bytes",
                self.limits.max_total_resource_bytes,
                self.resource_bytes,
            ));
        }
        let intrinsic_size = if self.bool()? {
            Some(PixelSize {
                width: self.u32()?,
                height: self.u32()?,
            })
        } else {
            None
        };
        Ok(DeckResource {
            id,
            kind,
            media_type,
            bytes,
            intrinsic_size,
        })
    }

    fn emu_size(&mut self) -> Result<EmuSize, WireError> {
        Ok(EmuSize {
            width: self.i64()?,
            height: self.i64()?,
        })
    }

    fn rect(&mut self) -> Result<EmuRect, WireError> {
        Ok(EmuRect {
            x: self.i64()?,
            y: self.i64()?,
            width: self.i64()?,
            height: self.i64()?,
        })
    }

    fn template_region(&mut self) -> Result<TemplateRegion, WireError> {
        Ok(TemplateRegion {
            id: self.id()?,
            layout_id: self.id()?,
            role: region_role(self.byte()?)?,
            placeholder: PlaceholderIdentity {
                kind: self.string()?,
                index: self.u32()?,
            },
            frame: self.rect()?,
            bleed_frame: if self.bool()? {
                Some(self.rect()?)
            } else {
                None
            },
            margins: self.text_margins()?,
            text_levels: self.vec("template text levels", Reader::template_text_level)?,
            accepts: self.vec("accepted semantic roles", |reader| {
                semantic_role(reader.u16()?)
            })?,
            required: self.bool()?,
        })
    }

    fn template_theme(&mut self) -> Result<TemplateTheme, WireError> {
        Ok(TemplateTheme {
            major_fonts: self.theme_fonts()?,
            minor_fonts: self.theme_fonts()?,
            colors: self.vec("theme colors", |reader| {
                Ok(ThemeColor {
                    slot: reader.string()?,
                    rgb: reader.u32()?,
                })
            })?,
        })
    }

    fn theme_fonts(&mut self) -> Result<ThemeFontSet, WireError> {
        Ok(ThemeFontSet {
            latin: self.optional_string()?,
            east_asian: self.optional_string()?,
            complex_script: self.optional_string()?,
        })
    }

    fn template_layout(&mut self) -> Result<TemplateLayout, WireError> {
        Ok(TemplateLayout {
            id: self.id()?,
            capability: template_layout_capability(self.byte()?)?,
            matching_name: self.string()?,
            source_part: self.string()?,
            master_part: self.string()?,
            region_ids: self.vec("template layout region IDs", Reader::id)?,
            asset_ids: self.vec("template layout asset IDs", Reader::id)?,
            background: if self.bool()? {
                Some(self.source()?)
            } else {
                None
            },
        })
    }

    fn text_margins(&mut self) -> Result<TextMargins, WireError> {
        Ok(TextMargins {
            left: self.i64()?,
            top: self.i64()?,
            right: self.i64()?,
            bottom: self.i64()?,
        })
    }

    fn template_text_level(&mut self) -> Result<TemplateTextLevel, WireError> {
        let level = self.byte()?;
        let font_size = self.optional_u32()?;
        let latin_typeface = self.optional_string()?;
        let east_asian_typeface = self.optional_string()?;
        let complex_script_typeface = self.optional_string()?;
        let color = if self.bool()? {
            Some(TemplateTextColor {
                scheme: self.optional_string()?,
                rgb: self.u32()?,
            })
        } else {
            None
        };
        Ok(TemplateTextLevel {
            level,
            font_size,
            latin_typeface,
            east_asian_typeface,
            complex_script_typeface,
            color,
            bold: self.optional_bool()?,
            italic: self.optional_bool()?,
            margin_left: self.optional_i64()?,
            indent: self.optional_i64()?,
        })
    }

    fn template_asset(&mut self) -> Result<TemplateAsset, WireError> {
        Ok(TemplateAsset {
            id: self.id()?,
            layout_id: self.id()?,
            kind: template_asset_kind(self.byte()?)?,
            source_part: self.string()?,
            source_xml: self.source()?,
            frame: if self.bool()? {
                Some(self.rect()?)
            } else {
                None
            },
            z_order: self.u32()?,
            related_parts: self.vec("template asset relationship parts", Reader::string)?,
        })
    }

    fn optional_u32(&mut self) -> Result<Option<u32>, WireError> {
        self.bool()?.then(|| self.u32()).transpose()
    }

    fn optional_i64(&mut self) -> Result<Option<i64>, WireError> {
        self.bool()?.then(|| self.i64()).transpose()
    }

    fn optional_bool(&mut self) -> Result<Option<bool>, WireError> {
        self.bool()?.then(|| self.bool()).transpose()
    }

    fn diagnostic(&mut self) -> Result<DeckDiagnostic, WireError> {
        Ok(DeckDiagnostic {
            code: DeckDiagnosticCode(self.u16()?),
            severity: diagnostic_severity(self.byte()?)?,
            message: self.string()?,
            source: if self.bool()? {
                Some(self.source()?)
            } else {
                None
            },
            node_id: self.optional_id()?,
            page_id: self.optional_id()?,
        })
    }

    fn optional_id(&mut self) -> Result<Option<StableId>, WireError> {
        self.bool()?.then(|| self.id()).transpose()
    }

    fn physical_page(&mut self) -> Result<PhysicalPage, WireError> {
        Ok(PhysicalPage {
            id: self.id()?,
            logical_slide_id: self.id()?,
            template_layout_id: self.id()?,
            topology: TopologyChoice {
                kind: layout_topology(self.byte()?)?,
                slot_count: self.u16()?,
            },
            hidden: self.bool()?,
            continuation: Continuation {
                ordinal: self.u32()?,
                total: self.u32()?,
                repeated_heading_node_id: self.optional_id()?,
                label: self.optional_string()?,
            },
            regions: self.vec("planned regions", Reader::planned_region)?,
        })
    }

    fn planned_region(&mut self) -> Result<PlannedRegion, WireError> {
        let template_region_id = self.id()?;
        let placement = self.region_placement()?;
        let frame = self.rect()?;
        let fragment_count = self.count("planned fragments")?;
        self.fragments = self.fragments.saturating_add(fragment_count);
        if self.fragments > self.limits.max_planned_fragments {
            return Err(WireError::limit(
                DeckLimitCode::PLANNED_FRAGMENTS,
                "planned fragment count",
                self.limits.max_planned_fragments,
                self.fragments,
            ));
        }
        let mut fragments = Vec::with_capacity(fragment_count);
        for _ in 0..fragment_count {
            let id = self.id()?;
            let source_node_id = self.id()?;
            let slice = self.fragment_slice()?;
            fragments.push(PlannedFragment {
                id,
                source_node_id,
                slice,
                frame: self.rect()?,
                type_choice: TypeChoice {
                    font_size: self.u32()?,
                },
                media: if self.bool()? {
                    let slot = self.rect()?;
                    let visible_frame = self.rect()?;
                    let fit = content_fit(self.byte()?)?;
                    let source_size = PixelSize {
                        width: self.u32()?,
                        height: self.u32()?,
                    };
                    let crop = if self.bool()? {
                        Some(SourceCrop {
                            left: self.u32()?,
                            top: self.u32()?,
                            right: self.u32()?,
                            bottom: self.u32()?,
                        })
                    } else {
                        None
                    };
                    Some(MediaPlacement {
                        slot,
                        visible_frame,
                        fit,
                        source_size,
                        crop,
                    })
                } else {
                    None
                },
                repeat_table_header_rows: self.u32()?,
            });
        }
        Ok(PlannedRegion {
            template_region_id,
            placement,
            frame,
            fragments,
        })
    }

    fn region_placement(&mut self) -> Result<RegionPlacement, WireError> {
        match self.byte()? {
            0 => Ok(RegionPlacement::Fixed),
            1 => Ok(RegionPlacement::Slot(self.u16()?)),
            _ => Err(invalid_tag("region placement")),
        }
    }

    fn fragment_slice(&mut self) -> Result<FragmentSlice, WireError> {
        match self.byte()? {
            0 => Ok(FragmentSlice::Whole),
            1 => Ok(FragmentSlice::Text {
                start: self.u32()?,
                end: self.u32()?,
            }),
            2 => Ok(FragmentSlice::ListItems {
                start: self.u32()?,
                end: self.u32()?,
            }),
            3 => Ok(FragmentSlice::TableRows {
                start: self.u32()?,
                end: self.u32()?,
            }),
            4 => Ok(FragmentSlice::CodeLines {
                start: self.u32()?,
                end: self.u32()?,
            }),
            _ => Err(invalid_tag("fragment slice")),
        }
    }
}

fn invalid_tag(label: &str) -> WireError {
    WireError::new(WireErrorKind::InvalidTag, format!("invalid {label} tag"))
}

fn length_u32(length: usize, label: &str) -> Result<u32, WireError> {
    u32::try_from(length).map_err(|_| {
        WireError::new(
            WireErrorKind::LimitExceeded,
            format!("{label} cannot be represented by the wire format"),
        )
    })
}

const fn logical_slide_kind_tag(value: LogicalSlideKind) -> u8 {
    match value {
        LogicalSlideKind::Title => 1,
        LogicalSlideKind::Content => 2,
    }
}

fn logical_slide_kind(value: u8) -> Result<LogicalSlideKind, WireError> {
    match value {
        1 => Ok(LogicalSlideKind::Title),
        2 => Ok(LogicalSlideKind::Content),
        _ => Err(invalid_tag("logical slide kind")),
    }
}

const fn media_text_proximity_tag(value: MediaTextProximity) -> u8 {
    match value {
        MediaTextProximity::SameParagraph => 1,
        MediaTextProximity::AdjacentBlocks => 2,
        MediaTextProximity::BlankSeparatedBlocks => 3,
    }
}

fn media_text_proximity(value: u8) -> Result<MediaTextProximity, WireError> {
    match value {
        1 => Ok(MediaTextProximity::SameParagraph),
        2 => Ok(MediaTextProximity::AdjacentBlocks),
        3 => Ok(MediaTextProximity::BlankSeparatedBlocks),
        _ => Err(invalid_tag("media-text proximity")),
    }
}

const fn media_text_side_tag(value: MediaTextSide) -> u8 {
    match value {
        MediaTextSide::BeforeMedia => 1,
        MediaTextSide::AfterMedia => 2,
    }
}

fn media_text_side(value: u8) -> Result<MediaTextSide, WireError> {
    match value {
        1 => Ok(MediaTextSide::BeforeMedia),
        2 => Ok(MediaTextSide::AfterMedia),
        _ => Err(invalid_tag("media-text side")),
    }
}

fn semantic_role(value: u16) -> Result<SemanticRole, WireError> {
    match value {
        1 => Ok(SemanticRole::Title),
        2 => Ok(SemanticRole::Subtitle),
        3 => Ok(SemanticRole::Prose),
        4 => Ok(SemanticRole::Section),
        5 => Ok(SemanticRole::List),
        6 => Ok(SemanticRole::ListItem),
        7 => Ok(SemanticRole::Figure),
        8 => Ok(SemanticRole::Caption),
        9 => Ok(SemanticRole::Gallery),
        10 => Ok(SemanticRole::Table),
        11 => Ok(SemanticRole::Chart),
        12 => Ok(SemanticRole::Code),
        13 => Ok(SemanticRole::Diagram),
        14 => Ok(SemanticRole::DisplayMath),
        15 => Ok(SemanticRole::Quote),
        16 => Ok(SemanticRole::Credit),
        17 => Ok(SemanticRole::Definition),
        18 => Ok(SemanticRole::DefinitionTerm),
        19 => Ok(SemanticRole::DefinitionDescription),
        20 => Ok(SemanticRole::Statement),
        21 => Ok(SemanticRole::TableRow),
        22 => Ok(SemanticRole::TableCell),
        23 => Ok(SemanticRole::TableColumn),
        _ => Err(invalid_tag("semantic role")),
    }
}

const fn split_policy_tag(value: SplitPolicy) -> u8 {
    match value {
        SplitPolicy::Never => 0,
        SplitPolicy::Text => 1,
        SplitPolicy::ListItems => 2,
        SplitPolicy::TableRows => 3,
        SplitPolicy::CodeLines => 4,
        SplitPolicy::Children => 5,
    }
}

fn split_policy(value: u8) -> Result<SplitPolicy, WireError> {
    match value {
        0 => Ok(SplitPolicy::Never),
        1 => Ok(SplitPolicy::Text),
        2 => Ok(SplitPolicy::ListItems),
        3 => Ok(SplitPolicy::TableRows),
        4 => Ok(SplitPolicy::CodeLines),
        5 => Ok(SplitPolicy::Children),
        _ => Err(invalid_tag("split policy")),
    }
}

const fn hyperlink_kind_tag(value: HyperlinkKind) -> u8 {
    match value {
        HyperlinkKind::Web => 1,
        HyperlinkKind::Email => 2,
        HyperlinkKind::Telephone => 3,
        HyperlinkKind::SourceAnchor => 4,
    }
}

fn hyperlink_kind(value: u8) -> Result<HyperlinkKind, WireError> {
    match value {
        1 => Ok(HyperlinkKind::Web),
        2 => Ok(HyperlinkKind::Email),
        3 => Ok(HyperlinkKind::Telephone),
        4 => Ok(HyperlinkKind::SourceAnchor),
        _ => Err(invalid_tag("hyperlink kind")),
    }
}

const fn chart_kind_tag(value: ChartKind) -> u8 {
    match value {
        ChartKind::Bar => 1,
        ChartKind::Column => 2,
        ChartKind::Line => 3,
        ChartKind::Area => 4,
        ChartKind::Pie => 5,
        ChartKind::Doughnut => 6,
        ChartKind::Scatter => 7,
    }
}

fn chart_kind(value: u8) -> Result<ChartKind, WireError> {
    match value {
        1 => Ok(ChartKind::Bar),
        2 => Ok(ChartKind::Column),
        3 => Ok(ChartKind::Line),
        4 => Ok(ChartKind::Area),
        5 => Ok(ChartKind::Pie),
        6 => Ok(ChartKind::Doughnut),
        7 => Ok(ChartKind::Scatter),
        _ => Err(invalid_tag("chart kind")),
    }
}

const fn table_column_alignment_tag(value: TableColumnAlignment) -> u8 {
    match value {
        TableColumnAlignment::Start => 0,
        TableColumnAlignment::Center => 1,
        TableColumnAlignment::End => 2,
    }
}

fn table_column_alignment(value: u8) -> Result<TableColumnAlignment, WireError> {
    match value {
        0 => Ok(TableColumnAlignment::Start),
        1 => Ok(TableColumnAlignment::Center),
        2 => Ok(TableColumnAlignment::End),
        _ => Err(invalid_tag("table column alignment")),
    }
}

const fn resource_kind_tag(value: ResourceKind) -> u8 {
    match value {
        ResourceKind::RasterImage => 1,
        ResourceKind::Svg => 2,
    }
}

fn resource_kind(value: u8) -> Result<ResourceKind, WireError> {
    match value {
        1 => Ok(ResourceKind::RasterImage),
        2 => Ok(ResourceKind::Svg),
        _ => Err(invalid_tag("resource kind")),
    }
}

const fn region_role_tag(value: RegionRole) -> u8 {
    match value {
        RegionRole::Title => 1,
        RegionRole::Subtitle => 2,
        RegionRole::Body => 3,
        RegionRole::Statement => 4,
        RegionRole::Media => 5,
        RegionRole::Caption => 6,
        RegionRole::Table => 7,
        RegionRole::Chart => 8,
        RegionRole::Code => 9,
        RegionRole::Footer => 10,
    }
}

fn region_role(value: u8) -> Result<RegionRole, WireError> {
    match value {
        1 => Ok(RegionRole::Title),
        2 => Ok(RegionRole::Subtitle),
        3 => Ok(RegionRole::Body),
        4 => Ok(RegionRole::Statement),
        5 => Ok(RegionRole::Media),
        6 => Ok(RegionRole::Caption),
        7 => Ok(RegionRole::Table),
        8 => Ok(RegionRole::Chart),
        9 => Ok(RegionRole::Code),
        10 => Ok(RegionRole::Footer),
        _ => Err(invalid_tag("region role")),
    }
}

const fn template_layout_capability_tag(value: TemplateLayoutCapability) -> u8 {
    match value {
        TemplateLayoutCapability::Title => 1,
        TemplateLayoutCapability::Statement => 3,
        TemplateLayoutCapability::ContentEnvelope => 4,
    }
}

fn template_layout_capability(value: u8) -> Result<TemplateLayoutCapability, WireError> {
    match value {
        1 => Ok(TemplateLayoutCapability::Title),
        3 => Ok(TemplateLayoutCapability::Statement),
        4 => Ok(TemplateLayoutCapability::ContentEnvelope),
        _ => Err(invalid_tag("template layout role")),
    }
}

const fn layout_topology_tag(value: LayoutTopology) -> u8 {
    match value {
        LayoutTopology::Stack => 0,
        LayoutTopology::FlowColumns => 1,
        LayoutTopology::WeightedSplit => 2,
        LayoutTopology::PeerGrid => 3,
        LayoutTopology::LeadSupporting => 4,
        LayoutTopology::MediaStart => 5,
        LayoutTopology::MediaEnd => 6,
        LayoutTopology::Gallery => 7,
        LayoutTopology::TableWide => 8,
        LayoutTopology::Comparison => 9,
    }
}

fn layout_topology(value: u8) -> Result<LayoutTopology, WireError> {
    match value {
        0 => Ok(LayoutTopology::Stack),
        1 => Ok(LayoutTopology::FlowColumns),
        2 => Ok(LayoutTopology::WeightedSplit),
        3 => Ok(LayoutTopology::PeerGrid),
        4 => Ok(LayoutTopology::LeadSupporting),
        5 => Ok(LayoutTopology::MediaStart),
        6 => Ok(LayoutTopology::MediaEnd),
        7 => Ok(LayoutTopology::Gallery),
        8 => Ok(LayoutTopology::TableWide),
        9 => Ok(LayoutTopology::Comparison),
        _ => Err(invalid_tag("layout topology")),
    }
}

const fn template_asset_kind_tag(value: TemplateAssetKind) -> u8 {
    match value {
        TemplateAssetKind::Decoration => 1,
        TemplateAssetKind::Logo => 2,
        TemplateAssetKind::Footer => 3,
    }
}

fn template_asset_kind(value: u8) -> Result<TemplateAssetKind, WireError> {
    match value {
        1 => Ok(TemplateAssetKind::Decoration),
        2 => Ok(TemplateAssetKind::Logo),
        3 => Ok(TemplateAssetKind::Footer),
        _ => Err(invalid_tag("template asset kind")),
    }
}

const fn diagnostic_severity_tag(value: DiagnosticSeverity) -> u8 {
    match value {
        DiagnosticSeverity::Info => 1,
        DiagnosticSeverity::Warning => 2,
        DiagnosticSeverity::Error => 3,
    }
}

fn diagnostic_severity(value: u8) -> Result<DiagnosticSeverity, WireError> {
    match value {
        1 => Ok(DiagnosticSeverity::Info),
        2 => Ok(DiagnosticSeverity::Warning),
        3 => Ok(DiagnosticSeverity::Error),
        _ => Err(invalid_tag("diagnostic severity")),
    }
}

const fn content_fit_tag(value: ContentFit) -> u8 {
    match value {
        ContentFit::None => 0,
        ContentFit::Contain => 1,
        ContentFit::Cover => 2,
    }
}

fn content_fit(value: u8) -> Result<ContentFit, WireError> {
    match value {
        0 => Ok(ContentFit::None),
        1 => Ok(ContentFit::Contain),
        2 => Ok(ContentFit::Cover),
        _ => Err(invalid_tag("content fit")),
    }
}
