use std::collections::{BTreeMap, BTreeSet};

use crate::{
    DeckDiagnostic, DeckDiagnosticCode, DeckLimits, DeckPlan, DeckSpec, DeckTemplatePlan,
    DiagnosticSeverity, EmuRect, FragmentSlice, HyperlinkKind, LogicalSlide, MediaTextSide,
    PlannedFragment, RegionPlacement, SemanticContent, SemanticNode, SemanticRole, SplitPolicy,
    StableId, ValidationReport,
};

#[derive(Clone, Copy)]
enum CoverageDomain {
    Whole,
    Text(u32),
    ListItems(u32),
    TableRows(u32),
    CodeLines(u32),
}

struct IndexedNode<'a> {
    node: &'a SemanticNode,
    slide_id: StableId,
    order: usize,
    domain: CoverageDomain,
}

/// Validate source-backed semantic content and resource ownership.
#[must_use]
pub fn validate_deck_spec(spec: &DeckSpec, limits: &DeckLimits) -> ValidationReport {
    let mut validator = SpecValidator {
        limits,
        report: ValidationReport::default(),
        ids: BTreeSet::new(),
        resources: spec.resources.iter().map(|resource| resource.id).collect(),
        semantic_nodes: 0,
    };

    validator.id(spec.id, None, "deck");
    validator.count(
        spec.logical_slides.len(),
        limits.max_collection_items,
        None,
        "logical slide count exceeds the configured collection limit",
    );
    validator.count(
        spec.resources.len(),
        limits.max_collection_items,
        None,
        "resource count exceeds the configured collection limit",
    );

    let mut total_resource_bytes = 0usize;
    for resource in &spec.resources {
        validator.id(resource.id, None, "resource");
        if resource.media_type.is_empty() || resource.media_type.len() > limits.max_string_bytes {
            validator.error(
                DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                None,
                Some(resource.id),
                "resource media type is empty or exceeds the configured string limit",
            );
        }
        if resource.bytes.len() > limits.max_resource_bytes {
            validator.error(
                DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                None,
                Some(resource.id),
                "resource exceeds the configured per-resource byte limit",
            );
        }
        total_resource_bytes = total_resource_bytes.saturating_add(resource.bytes.len());
        if let Some(size) = resource.intrinsic_size {
            if size.width == 0 || size.height == 0 {
                validator.error(
                    DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                    None,
                    Some(resource.id),
                    "resource intrinsic dimensions must be positive",
                );
            }
        }
    }
    if total_resource_bytes > limits.max_total_resource_bytes {
        validator.error(
            DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
            None,
            None,
            "resource bytes exceed the configured deck total",
        );
    }

    for slide in &spec.logical_slides {
        validator.slide(slide);
    }
    validator.report
}

struct SpecValidator<'a> {
    limits: &'a DeckLimits,
    report: ValidationReport,
    ids: BTreeSet<StableId>,
    resources: BTreeSet<StableId>,
    semantic_nodes: usize,
}

impl SpecValidator<'_> {
    fn slide(&mut self, slide: &LogicalSlide) {
        self.id(slide.id, Some(&slide.source), "logical slide");
        self.source(&slide.source, Some(slide.id));
        self.count(
            slide.nodes.len(),
            self.limits.max_collection_items,
            Some(&slide.source),
            "logical slide node count exceeds the configured collection limit",
        );
        for node in &slide.nodes {
            self.node(node, Some(&slide.source), 1);
        }
        self.count(
            slide.media_text_relations.len(),
            self.limits.max_collection_items,
            Some(&slide.source),
            "media-text relation count exceeds the configured collection limit",
        );
        let mut nodes = BTreeMap::new();
        index_semantic_nodes(&slide.nodes, &mut nodes);
        let mut relations = BTreeSet::new();
        for relation in &slide.media_text_relations {
            let media = nodes.get(&relation.media_node_id).copied();
            let text = nodes.get(&relation.text_node_id).copied();
            if media.is_none_or(|node| !is_media_relation_node(node))
                || text.is_none_or(|node| !is_text_relation_node(node))
                || relation.media_node_id == relation.text_node_id
                || !relations.insert((relation.media_node_id, relation.text_node_id))
            {
                self.error(
                    DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                    Some(&slide.source),
                    Some(slide.id),
                    "media-text relation endpoints must be unique compatible nodes on one slide",
                );
                continue;
            }
            let (Some(media), Some(text)) = (media, text) else {
                continue;
            };
            if media.source.source == text.source.source {
                let observed_side = if text.source.start <= media.source.start {
                    MediaTextSide::BeforeMedia
                } else {
                    MediaTextSide::AfterMedia
                };
                if observed_side != relation.text_side {
                    self.error(
                        DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                        Some(&text.source),
                        Some(text.id),
                        "media-text relation side disagrees with source order",
                    );
                }
            }
            if relation.explicit_caption && text.role != SemanticRole::Caption {
                self.error(
                    DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                    Some(&text.source),
                    Some(text.id),
                    "an explicit media caption relation must target a caption node",
                );
            }
        }
    }

    fn node(&mut self, node: &SemanticNode, parent: Option<&crate::SourceRange>, depth: usize) {
        self.semantic_nodes = self.semantic_nodes.saturating_add(1);
        self.id(node.id, Some(&node.source), "semantic node");
        self.source(&node.source, Some(node.id));
        if self.semantic_nodes > self.limits.max_semantic_nodes {
            self.error(
                DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                Some(&node.source),
                Some(node.id),
                "semantic node count exceeds the configured limit",
            );
        }
        if depth > self.limits.max_nesting_depth {
            self.error(
                DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                Some(&node.source),
                Some(node.id),
                "semantic nesting exceeds the configured depth limit",
            );
            return;
        }
        if let Some(parent) = parent {
            if !range_contains(parent, &node.source) {
                self.error(
                    DeckDiagnosticCode::INVALID_SOURCE_RANGE,
                    Some(&node.source),
                    Some(node.id),
                    "semantic node source is outside its parent source range",
                );
            }
        }
        if !split_matches(node) || !role_matches(node) {
            self.error(
                DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                Some(&node.source),
                Some(node.id),
                "semantic role, content, and split policy are inconsistent",
            );
        }

        match &node.content {
            SemanticContent::Text(text) => {
                self.count(
                    text.runs.len(),
                    self.limits.max_collection_items,
                    Some(&node.source),
                    "rich-text run count exceeds the configured collection limit",
                );
                if text.runs.is_empty() || text.runs.iter().all(|run| run.text.is_empty()) {
                    self.error(
                        DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                        Some(&node.source),
                        Some(node.id),
                        "renderable rich text must contain text",
                    );
                }
                for run in &text.runs {
                    if run.text.len() > self.limits.max_string_bytes {
                        self.error(
                            DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                            Some(&node.source),
                            Some(node.id),
                            "rich-text run exceeds the configured string limit",
                        );
                    }
                    if let Some(link) = &run.hyperlink {
                        if !safe_hyperlink(link.kind, &link.target) {
                            self.error(
                                DeckDiagnosticCode::UNSAFE_HYPERLINK,
                                Some(&node.source),
                                Some(node.id),
                                "hyperlink target does not match its declared safe kind",
                            );
                        }
                    }
                }
            }
            SemanticContent::Children(children) => {
                self.count(
                    children.len(),
                    self.limits.max_collection_items,
                    Some(&node.source),
                    "semantic child count exceeds the configured collection limit",
                );
                for child in children {
                    self.node(child, Some(&node.source), depth + 1);
                }
            }
            SemanticContent::Image(image) => {
                self.resource(image.resource_id, node);
            }
            SemanticContent::List(list) => {
                if list.items.is_empty() || (list.ordered && list.start == 0) {
                    self.error(
                        DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                        Some(&node.source),
                        Some(node.id),
                        "list must contain an item and ordered list start must be one or greater",
                    );
                }
                self.list(list, &node.source, depth + 1);
            }
            SemanticContent::Table(table) => {
                if table.columns.is_empty()
                    || table.rows.is_empty()
                    || table.header_rows as usize > table.rows.len()
                {
                    self.error(
                        DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                        Some(&node.source),
                        Some(node.id),
                        "table dimensions or header row count are invalid",
                    );
                }
                for column in &table.columns {
                    self.id(column.id, Some(&column.source), "table column");
                    self.source(&column.source, Some(column.id));
                }
                for row in &table.rows {
                    self.id(row.id, Some(&row.source), "table row");
                    self.source(&row.source, Some(row.id));
                    if row.cells.len() != table.columns.len() {
                        self.error(
                            DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                            Some(&row.source),
                            Some(row.id),
                            "table row cell count does not match the column count",
                        );
                    }
                    for cell in &row.cells {
                        self.id(cell.id, Some(&cell.source), "table cell");
                        self.source(&cell.source, Some(cell.id));
                        if cell.content.runs.is_empty() {
                            self.error(
                                DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                                Some(&cell.source),
                                Some(cell.id),
                                "table cell rich text must contain at least one run",
                            );
                        }
                    }
                }
            }
            SemanticContent::Chart(chart) => {
                for series in &chart.series {
                    if series.values.len() != chart.categories.len() {
                        self.error(
                            DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                            Some(&node.source),
                            Some(node.id),
                            "chart category and series value counts differ",
                        );
                    }
                    if series.values.iter().any(|value| !value.is_finite()) {
                        self.error(
                            DeckDiagnosticCode::NON_FINITE_CHART_VALUE,
                            Some(&node.source),
                            Some(node.id),
                            "chart values must be finite",
                        );
                    }
                }
            }
            SemanticContent::Code(code) => {
                if code.code.len() > self.limits.max_string_bytes {
                    self.error(
                        DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                        Some(&node.source),
                        Some(node.id),
                        "code block exceeds the configured string limit",
                    );
                }
            }
            SemanticContent::Svg(svg) => self.resource(svg.resource_id, node),
        }
    }

    fn list(&mut self, list: &crate::ListContent, parent: &crate::SourceRange, depth: usize) {
        if depth > self.limits.max_nesting_depth {
            self.error(
                DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                Some(parent),
                None,
                "list nesting exceeds the configured depth limit",
            );
            return;
        }
        for item in &list.items {
            self.id(item.id, Some(&item.source), "list item");
            self.source(&item.source, Some(item.id));
            if !range_contains(parent, &item.source) {
                self.error(
                    DeckDiagnosticCode::INVALID_SOURCE_RANGE,
                    Some(&item.source),
                    Some(item.id),
                    "list item source is outside the list source range",
                );
            }
            for block in &item.blocks {
                self.node(block, Some(&item.source), depth + 1);
            }
            for child in &item.children {
                self.list(child, &item.source, depth + 1);
            }
        }
    }

    fn resource(&mut self, resource_id: StableId, node: &SemanticNode) {
        if !self.resources.contains(&resource_id) {
            self.error(
                DeckDiagnosticCode::MISSING_RESOURCE,
                Some(&node.source),
                Some(node.id),
                "semantic content references a missing resource",
            );
        }
    }

    fn id(&mut self, id: StableId, source: Option<&crate::SourceRange>, label: &str) {
        if id == StableId::NIL || !self.ids.insert(id) {
            self.error(
                DeckDiagnosticCode::DUPLICATE_ID,
                source,
                Some(id),
                &format!("{label} identity is nil or duplicated"),
            );
        }
    }

    fn source(&mut self, source: &crate::SourceRange, node_id: Option<StableId>) {
        if !source.is_valid() || source.source.len() > self.limits.max_string_bytes {
            self.error(
                DeckDiagnosticCode::INVALID_SOURCE_RANGE,
                Some(source),
                node_id,
                "source range is invalid or exceeds the configured string limit",
            );
        }
    }

    fn count(
        &mut self,
        actual: usize,
        maximum: usize,
        source: Option<&crate::SourceRange>,
        message: &str,
    ) {
        if actual > maximum {
            self.error(
                DeckDiagnosticCode::INVALID_SEMANTIC_CONTENT,
                source,
                None,
                message,
            );
        }
    }

    fn error(
        &mut self,
        code: DeckDiagnosticCode,
        source: Option<&crate::SourceRange>,
        node_id: Option<StableId>,
        message: &str,
    ) {
        self.report.diagnostics.push(DeckDiagnostic {
            code,
            severity: DiagnosticSeverity::Error,
            message: message.to_owned(),
            source: source.cloned(),
            node_id,
            page_id: None,
        });
    }
}

/// Validate exact source ownership, target regions, geometry, and continuation metadata.
#[must_use]
pub fn validate_deck_plan(
    spec: &DeckSpec,
    template: &DeckTemplatePlan,
    plan: &DeckPlan,
    limits: &DeckLimits,
) -> ValidationReport {
    let mut report = validate_deck_spec(spec, limits);
    let mut nodes = BTreeMap::new();
    let mut coverage = Vec::new();
    let mut node_ids = BTreeSet::new();
    for slide in &spec.logical_slides {
        for node in &slide.nodes {
            index_node(node, slide.id, &mut nodes, &mut coverage, &mut node_ids);
        }
    }

    if plan.spec_id != spec.id
        || plan.template_id != template.id
        || plan.page_size != template.page_size
    {
        plan_error(
            &mut report,
            DeckDiagnosticCode::PLAN_TARGET_DRIFT,
            None,
            None,
            "plan identity or page size does not match its spec and template",
        );
    }
    if !plan.page_size.is_positive() || plan.pages.len() > limits.max_physical_pages {
        plan_error(
            &mut report,
            DeckDiagnosticCode::PLAN_INVALID_GEOMETRY,
            None,
            None,
            "plan page size is invalid or physical page count exceeds the configured limit",
        );
    }

    let page_bounds = EmuRect {
        x: 0,
        y: 0,
        width: plan.page_size.width,
        height: plan.page_size.height,
    };
    let regions = template
        .regions
        .iter()
        .map(|region| (region.id, region))
        .collect::<BTreeMap<_, _>>();
    let layouts = template
        .layouts
        .iter()
        .map(|layout| layout.id)
        .collect::<BTreeSet<_>>();
    let resources = spec
        .resources
        .iter()
        .map(|resource| (resource.id, resource))
        .collect::<BTreeMap<_, _>>();
    for region in &template.regions {
        let valid_bleed = region
            .bleed_frame
            .is_none_or(|bleed| bleed.is_within(page_bounds) && region.frame.is_within(bleed));
        if !region.frame.is_within(page_bounds) || !valid_bleed || region.accepts.is_empty() {
            plan_error(
                &mut report,
                DeckDiagnosticCode::PLAN_INVALID_GEOMETRY,
                None,
                None,
                "template region or bleed is outside the page, the bleed does not contain the safe frame, or the region accepts no semantic roles",
            );
        }
    }

    validate_pages(
        spec,
        plan,
        &PlanValidationContext {
            nodes: &nodes,
            regions: &regions,
            layouts: &layouts,
            resources: &resources,
            page_bounds,
            limits,
        },
        &mut report,
    );
    validate_coverage(&coverage, plan, &nodes, &mut report);
    report
}

fn index_node<'a>(
    node: &'a SemanticNode,
    slide_id: StableId,
    nodes: &mut BTreeMap<StableId, IndexedNode<'a>>,
    coverage: &mut Vec<StableId>,
    ids: &mut BTreeSet<StableId>,
) {
    if !ids.insert(node.id) {
        return;
    }
    if let SemanticContent::Children(children) = &node.content {
        for child in children {
            index_node(child, slide_id, nodes, coverage, ids);
        }
        return;
    }
    let domain = match (&node.content, node.split) {
        (SemanticContent::Text(text), SplitPolicy::Text) => {
            CoverageDomain::Text(text.plain_text().len() as u32)
        }
        (SemanticContent::List(list), SplitPolicy::ListItems) => {
            CoverageDomain::ListItems(list.items.len() as u32)
        }
        (SemanticContent::Table(table), SplitPolicy::TableRows) => {
            CoverageDomain::TableRows(table.rows.len() as u32)
        }
        (SemanticContent::Code(code), SplitPolicy::CodeLines) => {
            CoverageDomain::CodeLines(logical_line_count(&code.code))
        }
        _ => CoverageDomain::Whole,
    };
    let order = coverage.len();
    coverage.push(node.id);
    nodes.insert(
        node.id,
        IndexedNode {
            node,
            slide_id,
            order,
            domain,
        },
    );
}

struct PlanValidationContext<'a> {
    nodes: &'a BTreeMap<StableId, IndexedNode<'a>>,
    regions: &'a BTreeMap<StableId, &'a crate::TemplateRegion>,
    layouts: &'a BTreeSet<StableId>,
    resources: &'a BTreeMap<StableId, &'a crate::DeckResource>,
    page_bounds: EmuRect,
    limits: &'a DeckLimits,
}

fn validate_pages(
    spec: &DeckSpec,
    plan: &DeckPlan,
    context: &PlanValidationContext<'_>,
    report: &mut ValidationReport,
) {
    let slides = spec
        .logical_slides
        .iter()
        .map(|slide| (slide.id, slide))
        .collect::<BTreeMap<_, _>>();
    let mut grouped = BTreeMap::<StableId, Vec<&crate::PhysicalPage>>::new();
    let mut observed_slide_order = Vec::new();
    let mut previous_slide = None;
    let mut fragment_count = 0usize;

    for page in &plan.pages {
        let mut fragment_frames = Vec::new();
        if previous_slide != Some(page.logical_slide_id) {
            observed_slide_order.push(page.logical_slide_id);
            previous_slide = Some(page.logical_slide_id);
        }
        grouped.entry(page.logical_slide_id).or_default().push(page);
        if !context.layouts.contains(&page.template_layout_id) {
            plan_error(
                report,
                DeckDiagnosticCode::PLAN_TARGET_DRIFT,
                Some(page.id),
                None,
                "physical page references an unknown template layout",
            );
        }
        let Some(slide) = slides.get(&page.logical_slide_id) else {
            plan_error(
                report,
                DeckDiagnosticCode::PLAN_TARGET_DRIFT,
                Some(page.id),
                None,
                "physical page references an unknown logical slide",
            );
            continue;
        };
        if page.hidden != slide.hidden {
            plan_error(
                report,
                DeckDiagnosticCode::PLAN_TARGET_DRIFT,
                Some(page.id),
                None,
                "physical page hidden state differs from its logical slide",
            );
        }
        let mut tables_seen_on_page = BTreeSet::new();
        if !page.topology.is_valid() {
            plan_error(
                report,
                DeckDiagnosticCode::PLAN_INVALID_GEOMETRY,
                Some(page.id),
                None,
                "page topology kind and slot count are inconsistent",
            );
        }
        for planned_region in &page.regions {
            let Some(template_region) = context.regions.get(&planned_region.template_region_id)
            else {
                plan_error(
                    report,
                    DeckDiagnosticCode::PLAN_TARGET_DRIFT,
                    Some(page.id),
                    None,
                    "planned region references an unknown template region",
                );
                continue;
            };
            if template_region.layout_id != page.template_layout_id {
                plan_error(
                    report,
                    DeckDiagnosticCode::PLAN_TARGET_DRIFT,
                    Some(page.id),
                    None,
                    "planned region belongs to a different template layout",
                );
            }
            let composition_frame = template_region.bleed_frame.unwrap_or(template_region.frame);
            if !planned_region.frame.is_within(composition_frame)
                || !planned_region.frame.is_within(context.page_bounds)
            {
                plan_error(
                    report,
                    DeckDiagnosticCode::PLAN_INVALID_GEOMETRY,
                    Some(page.id),
                    None,
                    "planned region is outside its template composition frame or page",
                );
            }
            if !planned_region.frame.is_within(template_region.frame)
                && planned_region.fragments.iter().any(|fragment| {
                    context
                        .nodes
                        .get(&fragment.source_node_id)
                        .is_none_or(|indexed| !is_media_role(indexed.node.role))
                })
            {
                plan_error(
                    report,
                    DeckDiagnosticCode::PLAN_INVALID_GEOMETRY,
                    Some(page.id),
                    None,
                    "only media fragments may use space outside the template safe frame",
                );
            }
            if let RegionPlacement::Slot(index) = planned_region.placement {
                if index >= page.topology.slot_count {
                    plan_error(
                        report,
                        DeckDiagnosticCode::PLAN_INVALID_GEOMETRY,
                        Some(page.id),
                        None,
                        "planned region references a slot outside its page topology",
                    );
                }
            }
            for fragment in &planned_region.fragments {
                fragment_count = fragment_count.saturating_add(1);
                let expected_repeated_header_rows = match (
                    &context
                        .nodes
                        .get(&fragment.source_node_id)
                        .map(|indexed| &indexed.node.content),
                    fragment.slice,
                ) {
                    (
                        Some(SemanticContent::Table(table)),
                        FragmentSlice::TableRows { start, .. },
                    ) if tables_seen_on_page.insert(fragment.source_node_id)
                        && table.header_rows > 0
                        && start >= table.header_rows =>
                    {
                        table.header_rows
                    }
                    _ => 0,
                };
                validate_fragment(
                    fragment,
                    &FragmentTarget {
                        page_id: page.id,
                        slide_id: page.logical_slide_id,
                        region_frame: planned_region.frame,
                        template_region,
                        expected_repeated_header_rows,
                    },
                    context.nodes,
                    context.resources,
                    report,
                );
                fragment_frames.push((fragment.source_node_id, fragment.frame));
            }
        }
        if let Some((left_id, right_id)) = first_overlapping_frames(&fragment_frames) {
            plan_error(
                report,
                DeckDiagnosticCode::PLAN_INVALID_GEOMETRY,
                Some(page.id),
                Some(right_id),
                &format!("source-owned fragment frames overlap ({left_id} and {right_id})"),
            );
        }
    }

    let expected_slide_order = spec
        .logical_slides
        .iter()
        .map(|slide| slide.id)
        .collect::<Vec<_>>();
    if observed_slide_order != expected_slide_order {
        plan_error(
            report,
            DeckDiagnosticCode::PLAN_SOURCE_REORDERED,
            None,
            None,
            "physical page groups do not follow logical slide source order",
        );
    }
    if fragment_count > context.limits.max_planned_fragments {
        plan_error(
            report,
            DeckDiagnosticCode::PLAN_SOURCE_DUPLICATION,
            None,
            None,
            "planned fragment count exceeds the configured limit",
        );
    }

    for slide in &spec.logical_slides {
        let Some(pages) = grouped.get(&slide.id) else {
            plan_error(
                report,
                DeckDiagnosticCode::PLAN_SOURCE_LOSS,
                None,
                Some(slide.id),
                "logical slide has no physical page",
            );
            continue;
        };
        let total = pages.len() as u32;
        let heading = context
            .nodes
            .values()
            .filter(|indexed| {
                indexed.slide_id == slide.id
                    && matches!(
                        indexed.node.role,
                        SemanticRole::Title | SemanticRole::Section
                    )
            })
            .min_by_key(|indexed| indexed.order)
            .map(|indexed| indexed.node.id);
        for (index, page) in pages.iter().enumerate() {
            let ordinal = index as u32 + 1;
            if page.continuation.ordinal != ordinal
                || page.continuation.total != total
                || page.id != slide.id.derive(b"physical-page", ordinal)
            {
                plan_error(
                    report,
                    DeckDiagnosticCode::PLAN_INVALID_CONTINUATION,
                    Some(page.id),
                    Some(slide.id),
                    "continuation ordinal, total, or stable page identity is inconsistent",
                );
            }
            let expected_label = (total > 1).then(|| format!("{ordinal}/{total}"));
            if page.continuation.label != expected_label {
                plan_error(
                    report,
                    DeckDiagnosticCode::PLAN_INVALID_CONTINUATION,
                    Some(page.id),
                    Some(slide.id),
                    "continuation label is not the minimal n/total marker",
                );
            }
            let expected_heading = (ordinal > 1).then_some(heading).flatten();
            if page.continuation.repeated_heading_node_id != expected_heading {
                plan_error(
                    report,
                    DeckDiagnosticCode::PLAN_INVALID_CONTINUATION,
                    Some(page.id),
                    Some(slide.id),
                    "derived page repeated-heading metadata is inconsistent",
                );
            }
        }
    }
}

const fn is_media_role(role: SemanticRole) -> bool {
    matches!(
        role,
        SemanticRole::Figure | SemanticRole::Gallery | SemanticRole::Chart | SemanticRole::Diagram
    )
}

struct FragmentTarget<'a> {
    page_id: StableId,
    slide_id: StableId,
    region_frame: EmuRect,
    template_region: &'a crate::TemplateRegion,
    expected_repeated_header_rows: u32,
}

fn validate_fragment(
    fragment: &PlannedFragment,
    target: &FragmentTarget<'_>,
    nodes: &BTreeMap<StableId, IndexedNode<'_>>,
    resources: &BTreeMap<StableId, &crate::DeckResource>,
    report: &mut ValidationReport,
) {
    let Some(indexed) = nodes.get(&fragment.source_node_id) else {
        plan_error(
            report,
            DeckDiagnosticCode::PLAN_TARGET_DRIFT,
            Some(target.page_id),
            Some(fragment.source_node_id),
            "planned fragment references an unknown source node",
        );
        return;
    };
    if indexed.slide_id != target.slide_id
        || !target.template_region.accepts.contains(&indexed.node.role)
    {
        plan_error(
            report,
            DeckDiagnosticCode::PLAN_TARGET_DRIFT,
            Some(target.page_id),
            Some(fragment.source_node_id),
            "planned fragment moved to another logical slide or incompatible template region",
        );
    }
    if !fragment.frame.is_within(target.region_frame) {
        plan_error(
            report,
            DeckDiagnosticCode::PLAN_INVALID_GEOMETRY,
            Some(target.page_id),
            Some(fragment.source_node_id),
            "fragment frame is outside its planned region",
        );
    }
    if fragment.id != PlannedFragment::expected_id(fragment.source_node_id, fragment.slice) {
        plan_error(
            report,
            DeckDiagnosticCode::PLAN_UNSTABLE_ID,
            Some(target.page_id),
            Some(fragment.source_node_id),
            "fragment identity is not derived from its source node and slice",
        );
    }
    if fragment.repeat_table_header_rows != target.expected_repeated_header_rows {
        plan_error(
            report,
            DeckDiagnosticCode::PLAN_TARGET_DRIFT,
            Some(target.page_id),
            Some(fragment.source_node_id),
            "table continuation header metadata is inconsistent",
        );
    }
    if !valid_fragment_choice_kind(indexed.node, fragment) {
        plan_error(
            report,
            DeckDiagnosticCode::PLAN_TARGET_DRIFT,
            Some(target.page_id),
            Some(fragment.source_node_id),
            "fragment font or resolved media choices do not match their semantic content",
        );
    } else if let Some((resource_id, allow_cover)) = media_resource(indexed.node) {
        if !valid_media_geometry(
            fragment,
            target.region_frame,
            resource_id,
            resources,
            allow_cover,
        ) {
            plan_error(
                report,
                DeckDiagnosticCode::PLAN_INVALID_GEOMETRY,
                Some(target.page_id),
                Some(fragment.source_node_id),
                "resolved media placement is not canonical for its source and allocated slot",
            );
        }
    }
}

fn valid_fragment_choice_kind(node: &SemanticNode, fragment: &PlannedFragment) -> bool {
    match &node.content {
        SemanticContent::Text(_)
        | SemanticContent::List(_)
        | SemanticContent::Table(_)
        | SemanticContent::Code(_) => {
            fragment.type_choice.font_size > 0 && fragment.media.is_none()
        }
        SemanticContent::Image(_) | SemanticContent::Svg(_) => {
            fragment.type_choice.font_size == 0 && fragment.media.is_some()
        }
        SemanticContent::Chart(_) => {
            fragment.type_choice.font_size == 0 && fragment.media.is_none()
        }
        SemanticContent::Children(_) => false,
    }
}

fn media_resource(node: &SemanticNode) -> Option<(StableId, bool)> {
    match &node.content {
        SemanticContent::Image(image) => Some((image.resource_id, true)),
        SemanticContent::Svg(svg) => Some((svg.resource_id, false)),
        _ => None,
    }
}

fn valid_media_geometry(
    fragment: &PlannedFragment,
    region_frame: EmuRect,
    resource_id: StableId,
    resources: &BTreeMap<StableId, &crate::DeckResource>,
    allow_cover: bool,
) -> bool {
    let Some(media) = fragment.media else {
        return false;
    };
    let Some(source_size) = resources
        .get(&resource_id)
        .and_then(|resource| crate::inspect_media_size(resource))
    else {
        return false;
    };
    media.source_size == source_size
        && media.visible_frame == fragment.frame
        && media.slot.is_within(region_frame)
        && media.visible_frame.is_within(media.slot)
        && media.is_canonical()
        && (allow_cover || media.fit == crate::ContentFit::Contain)
}

fn first_overlapping_frames(frames: &[(StableId, EmuRect)]) -> Option<(StableId, StableId)> {
    let mut events = Vec::with_capacity(frames.len().saturating_mul(2));
    for (index, (_, frame)) in frames.iter().enumerate() {
        let right = frame.x.checked_add(frame.width)?;
        if !frame.is_positive() {
            continue;
        }
        // End events sort before starts, so touching edges remain legal.
        events.push((frame.x, 1u8, index));
        events.push((right, 0u8, index));
    }
    events.sort_unstable();

    // Until a collision is found, every active x-overlapping rectangle has a disjoint y interval.
    // Its immediate y neighbors are therefore sufficient for bounded O(n log n) detection.
    let mut active = BTreeMap::<(i64, usize), (i64, StableId)>::new();
    for (_, kind, index) in events {
        let (id, frame) = frames[index];
        let key = (frame.y, index);
        if kind == 0 {
            active.remove(&key);
            continue;
        }
        let bottom = frame.y.checked_add(frame.height)?;
        if let Some((_, (other_bottom, other_id))) = active.range(..key).next_back() {
            if *other_bottom > frame.y {
                return Some((*other_id, id));
            }
        }
        if let Some(((other_y, _), (_, other_id))) = active.range(key..).next() {
            if *other_y < bottom {
                return Some((*other_id, id));
            }
        }
        active.insert(key, (bottom, id));
    }
    None
}

fn validate_coverage(
    expected: &[StableId],
    plan: &DeckPlan,
    nodes: &BTreeMap<StableId, IndexedNode<'_>>,
    report: &mut ValidationReport,
) {
    let fragments = plan
        .pages
        .iter()
        .flat_map(|page| page.regions.iter())
        .flat_map(|region| region.fragments.iter())
        .filter(|fragment| nodes.contains_key(&fragment.source_node_id))
        .collect::<Vec<_>>();
    let observed_order = fragments
        .iter()
        .map(|fragment| nodes[&fragment.source_node_id].order)
        .collect::<Vec<_>>();
    if observed_order.windows(2).any(|pair| pair[0] > pair[1]) {
        plan_error(
            report,
            DeckDiagnosticCode::PLAN_SOURCE_REORDERED,
            None,
            None,
            "planned fragments do not follow semantic source order",
        );
    }

    let mut by_node = BTreeMap::<StableId, Vec<FragmentSlice>>::new();
    for fragment in fragments {
        by_node
            .entry(fragment.source_node_id)
            .or_default()
            .push(fragment.slice);
    }
    for id in expected {
        let slices = by_node.remove(id).unwrap_or_default();
        match nodes[id].domain {
            CoverageDomain::Whole => validate_whole(*id, &slices, report),
            CoverageDomain::Text(end) => {
                let text = match &nodes[id].node.content {
                    SemanticContent::Text(text) => Some(text.plain_text()),
                    _ => None,
                };
                validate_intervals(*id, &slices, end, SliceKind::Text, text.as_deref(), report)
            }
            CoverageDomain::ListItems(end) => {
                validate_intervals(*id, &slices, end, SliceKind::ListItems, None, report)
            }
            CoverageDomain::TableRows(end) => {
                validate_intervals(*id, &slices, end, SliceKind::TableRows, None, report)
            }
            CoverageDomain::CodeLines(end) => {
                validate_intervals(*id, &slices, end, SliceKind::CodeLines, None, report)
            }
        }
    }
}

fn validate_whole(id: StableId, slices: &[FragmentSlice], report: &mut ValidationReport) {
    if slices.is_empty() {
        coverage_error(
            report,
            DeckDiagnosticCode::PLAN_SOURCE_LOSS,
            id,
            "source node is absent",
        );
    } else if slices.len() > 1 {
        coverage_error(
            report,
            DeckDiagnosticCode::PLAN_SOURCE_DUPLICATION,
            id,
            "atomic source node appears more than once",
        );
    }
    if slices.iter().any(|slice| *slice != FragmentSlice::Whole) {
        coverage_error(
            report,
            DeckDiagnosticCode::PLAN_SOURCE_LOSS,
            id,
            "atomic source node uses an incompatible fragment slice",
        );
    }
}

#[derive(Clone, Copy)]
enum SliceKind {
    Text,
    ListItems,
    TableRows,
    CodeLines,
}

fn validate_intervals(
    id: StableId,
    slices: &[FragmentSlice],
    expected_end: u32,
    kind: SliceKind,
    text: Option<&str>,
    report: &mut ValidationReport,
) {
    if slices == [FragmentSlice::Whole] {
        return;
    }
    let intervals = slices
        .iter()
        .filter_map(|slice| match (kind, slice) {
            (SliceKind::Text, FragmentSlice::Text { start, end })
            | (SliceKind::ListItems, FragmentSlice::ListItems { start, end })
            | (SliceKind::TableRows, FragmentSlice::TableRows { start, end }) => {
                Some((*start, *end))
            }
            (SliceKind::CodeLines, FragmentSlice::CodeLines { start, end }) => Some((*start, *end)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if intervals.len() != slices.len() {
        coverage_error(
            report,
            DeckDiagnosticCode::PLAN_SOURCE_LOSS,
            id,
            "source node uses an incompatible fragment slice kind",
        );
    }
    let mut cursor = 0;
    for (start, end) in intervals {
        if let Some(text) = text {
            if !text.is_char_boundary(start as usize) || !text.is_char_boundary(end as usize) {
                coverage_error(
                    report,
                    DeckDiagnosticCode::PLAN_SOURCE_LOSS,
                    id,
                    "text fragment slice does not end on UTF-8 boundaries",
                );
            }
        }
        if start < cursor || end <= start {
            coverage_error(
                report,
                DeckDiagnosticCode::PLAN_SOURCE_DUPLICATION,
                id,
                "fragment slices overlap or have an empty range",
            );
        }
        if start > cursor {
            coverage_error(
                report,
                DeckDiagnosticCode::PLAN_SOURCE_LOSS,
                id,
                "fragment slices leave a source gap",
            );
        }
        cursor = cursor.max(end);
    }
    if cursor != expected_end {
        coverage_error(
            report,
            DeckDiagnosticCode::PLAN_SOURCE_LOSS,
            id,
            "fragment slices do not cover the complete source extent",
        );
    }
}

fn coverage_error(
    report: &mut ValidationReport,
    code: DeckDiagnosticCode,
    node_id: StableId,
    message: &str,
) {
    plan_error(report, code, None, Some(node_id), message);
}

fn plan_error(
    report: &mut ValidationReport,
    code: DeckDiagnosticCode,
    page_id: Option<StableId>,
    node_id: Option<StableId>,
    message: &str,
) {
    report.diagnostics.push(DeckDiagnostic {
        code,
        severity: DiagnosticSeverity::Error,
        message: message.to_owned(),
        source: None,
        node_id,
        page_id,
    });
}

fn split_matches(node: &SemanticNode) -> bool {
    matches!(node.split, SplitPolicy::Never)
        || matches!(
            (&node.content, node.split),
            (SemanticContent::Text(_), SplitPolicy::Text)
                | (SemanticContent::List(_), SplitPolicy::ListItems)
                | (SemanticContent::Table(_), SplitPolicy::TableRows)
                | (SemanticContent::Code(_), SplitPolicy::CodeLines)
                | (SemanticContent::Children(_), SplitPolicy::Children)
        )
}

fn logical_line_count(text: &str) -> u32 {
    u32::try_from(text.split_inclusive('\n').count().max(1)).unwrap_or(u32::MAX)
}

fn index_semantic_nodes<'a>(
    nodes: &'a [SemanticNode],
    output: &mut BTreeMap<StableId, &'a SemanticNode>,
) {
    for node in nodes {
        output.insert(node.id, node);
        match &node.content {
            SemanticContent::Children(children) => index_semantic_nodes(children, output),
            SemanticContent::List(list) => {
                for item in &list.items {
                    index_semantic_nodes(&item.blocks, output);
                    for children in &item.children {
                        index_list_nodes(children, output);
                    }
                }
            }
            _ => {}
        }
    }
}

fn index_list_nodes<'a>(
    list: &'a crate::ListContent,
    output: &mut BTreeMap<StableId, &'a SemanticNode>,
) {
    for item in &list.items {
        index_semantic_nodes(&item.blocks, output);
        for children in &item.children {
            index_list_nodes(children, output);
        }
    }
}

fn is_media_relation_node(node: &SemanticNode) -> bool {
    matches!(
        node.content,
        SemanticContent::Image(_) | SemanticContent::Svg(_)
    )
}

fn is_text_relation_node(node: &SemanticNode) -> bool {
    matches!(node.content, SemanticContent::Text(_))
}

fn role_matches(node: &SemanticNode) -> bool {
    match node.role {
        SemanticRole::List => matches!(node.content, SemanticContent::List(_)),
        SemanticRole::Table => matches!(node.content, SemanticContent::Table(_)),
        SemanticRole::Chart => matches!(node.content, SemanticContent::Chart(_)),
        SemanticRole::Code => matches!(node.content, SemanticContent::Code(_)),
        SemanticRole::Diagram | SemanticRole::DisplayMath => {
            matches!(node.content, SemanticContent::Svg(_))
        }
        SemanticRole::Gallery | SemanticRole::Quote | SemanticRole::Definition => {
            matches!(node.content, SemanticContent::Children(_))
        }
        SemanticRole::Figure => matches!(
            node.content,
            SemanticContent::Image(_) | SemanticContent::Children(_)
        ),
        SemanticRole::TableRow | SemanticRole::TableCell | SemanticRole::TableColumn => false,
        _ => matches!(
            node.content,
            SemanticContent::Text(_) | SemanticContent::Children(_)
        ),
    }
}

fn safe_hyperlink(kind: HyperlinkKind, target: &str) -> bool {
    if target.trim() != target || target.chars().any(char::is_control) {
        return false;
    }
    match kind {
        HyperlinkKind::Web => target.starts_with("https://") || target.starts_with("http://"),
        HyperlinkKind::Email => target.starts_with("mailto:"),
        HyperlinkKind::Telephone => target.starts_with("tel:"),
        HyperlinkKind::SourceAnchor => target.starts_with('#') && target.len() > 1,
    }
}

fn range_contains(parent: &crate::SourceRange, child: &crate::SourceRange) -> bool {
    parent.source == child.source && parent.start <= child.start && child.end <= parent.end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_is_complete_coverage_for_every_interval_domain() {
        for kind in [
            SliceKind::Text,
            SliceKind::ListItems,
            SliceKind::TableRows,
            SliceKind::CodeLines,
        ] {
            let mut report = ValidationReport::default();

            validate_intervals(
                StableId::from_bytes([1; 16]),
                &[FragmentSlice::Whole],
                4,
                kind,
                None,
                &mut report,
            );

            assert!(report.is_valid(), "{:?}", report.diagnostics);
        }
    }

    #[test]
    fn whole_cannot_be_combined_with_partial_coverage() {
        let mut report = ValidationReport::default();

        validate_intervals(
            StableId::from_bytes([2; 16]),
            &[
                FragmentSlice::Whole,
                FragmentSlice::Text { start: 0, end: 4 },
            ],
            4,
            SliceKind::Text,
            Some("test"),
            &mut report,
        );

        assert!(!report.is_valid());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DeckDiagnosticCode::PLAN_SOURCE_LOSS)
        );
    }

    #[test]
    fn overlap_sweep_detects_nested_frames_but_allows_touching_edges() {
        let id = |value| StableId::from_bytes([value; 16]);
        let frame = |x, y, width, height| EmuRect {
            x,
            y,
            width,
            height,
        };

        assert_eq!(
            first_overlapping_frames(&[
                (id(1), frame(0, 0, 100, 100)),
                (id(2), frame(100, 0, 100, 100)),
                (id(3), frame(0, 100, 100, 100)),
            ]),
            None
        );
        assert_eq!(
            first_overlapping_frames(&[
                (id(1), frame(0, 0, 300, 300)),
                (id(2), frame(50, 100, 100, 100)),
            ]),
            Some((id(1), id(2)))
        );
    }
}
