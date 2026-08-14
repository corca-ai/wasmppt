use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs, io,
    path::PathBuf,
    time::Instant,
};

use wasmppt_deck::{
    ContentFit, DeckDiagnostic, DeckDiagnosticCode, DeckLimits, DeckPlan, DeckResource, DeckSpec,
    DiagnosticSeverity, EmuRect, FragmentSlice, LayoutTopology, RegionPlacement, SemanticContent,
    SemanticNode, SemanticRole, StableId,
};
use wasmppt_deck_layout::{DeckPlanner, FontCatalog, PlannerPolicy};
use wasmppt_deck_template::ThemeTemplateCompiler;
use wasmppt_wasm::WasmpptEngine;

struct Evidence {
    template_plan: Vec<u8>,
    plan: Vec<u8>,
    slides: Vec<Vec<u8>>,
    pptx: Vec<u8>,
    timings: Timings,
}

struct Timings {
    plan_ms: f64,
    resolve_all_ms: f64,
    export_ms: f64,
}

#[derive(Clone)]
struct SemanticMetadata {
    role: SemanticRole,
    alternative_text: Option<String>,
}

struct MediaEvidence {
    semantic_id: String,
    case: String,
    item: u32,
    page_index: usize,
    expected_width: u32,
    expected_height: u32,
    source_width: u32,
    source_height: u32,
    frame_width: i64,
    frame_height: i64,
    fit: &'static str,
    aspect_error_per_million: u64,
    crop_loss_per_mille: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture_directory = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: deck_gate_native FIXTURE_DIRECTORY OUTPUT_DIRECTORY");
    let output_directory = env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .expect("usage: deck_gate_native FIXTURE_DIRECTORY OUTPUT_DIRECTORY");
    fs::create_dir_all(&output_directory)?;
    let template = fs::read(fixture_directory.join("starter.potx"))?;
    let spec = fs::read(fixture_directory.join("deck-spec.wdsf"))?;
    let atomic = fs::read(fixture_directory.join("atomic-overflow.wdsf"))?;
    assert_atomic_overflow(&template, &atomic)?;

    let mut engine = WasmpptEngine::new();
    let first = execute(&mut engine, &template, &spec)?;
    let mut plan_samples_ms = vec![first.timings.plan_ms];
    let mut resolve_all_samples_ms = vec![first.timings.resolve_all_ms];
    let mut export_samples_ms = vec![first.timings.export_ms];
    for _ in 1..7 {
        let repeated = execute(&mut engine, &template, &spec)?;
        if first.template_plan != repeated.template_plan
            || first.plan != repeated.plan
            || first.slides != repeated.slides
            || first.pptx != repeated.pptx
        {
            return Err("native deck gate is not byte deterministic".into());
        }
        plan_samples_ms.push(repeated.timings.plan_ms);
        resolve_all_samples_ms.push(repeated.timings.resolve_all_ms);
        export_samples_ms.push(repeated.timings.export_ms);
    }
    assert_role_mutations(&mut engine, &template, &spec, &first)?;

    fs::write(output_directory.join("native.wdtp"), &first.template_plan)?;
    fs::write(output_directory.join("native.wdpl"), &first.plan)?;
    fs::write(output_directory.join("native.pptx"), &first.pptx)?;
    for (index, display_list) in first.slides.iter().enumerate() {
        fs::write(
            output_directory.join(format!("native-{index:04}.wpdl")),
            display_list,
        )?;
    }
    let plan = DeckPlan::decode(&first.plan, &DeckLimits::default())?;
    let decoded_spec = DeckSpec::decode(&spec, &DeckLimits::default())?;
    let quality = assert_layout_quality(&template, &decoded_spec, &plan)?;
    fs::write(
        output_directory.join("native-topology.json"),
        topology_json(&plan),
    )?;
    fs::write(
        output_directory.join("native-timings.json"),
        timings_json(
            &plan_samples_ms,
            &resolve_all_samples_ms,
            &export_samples_ms,
        ),
    )?;
    fs::write(output_directory.join("native-quality.json"), quality)?;
    Ok(())
}

fn assert_layout_quality(
    template: &[u8],
    spec: &DeckSpec,
    plan: &DeckPlan,
) -> Result<String, Box<dyn std::error::Error>> {
    let semantics = semantic_metadata(spec);
    let policy = PlannerPolicy::default();
    let mut media_fragments = 0usize;
    let mut table_fragments = 0usize;
    let mut flow_pages = 0usize;
    let mut gallery_pages = 0usize;
    let mut table_slices = BTreeMap::<StableId, Vec<(u32, u32)>>::new();
    let mut fragment_ids = BTreeSet::new();
    let mut locations = BTreeMap::<StableId, Vec<(usize, RegionPlacement, EmuRect)>>::new();
    let mut media_evidence = Vec::new();

    for (page_index, page) in plan.pages.iter().enumerate() {
        if page.topology.kind == LayoutTopology::Gallery {
            gallery_pages += 1;
            if !(2..=6).contains(&page.topology.slot_count) {
                return Err("gallery topology has an invalid slot count".into());
            }
        }

        let mut slot_loads = BTreeMap::<u16, u32>::new();
        let mut tables_on_page = BTreeSet::new();
        let mut page_frames = Vec::new();
        for region in &page.regions {
            for fragment in &region.fragments {
                if !fragment_ids.insert(fragment.id) {
                    return Err("planned fragment identity is not unique".into());
                }
                let metadata = semantics
                    .get(&fragment.source_node_id)
                    .ok_or("planned fragment lost its semantic source")?;
                let role = metadata.role;
                if is_text_role(role) && fragment.type_choice.font_size < policy.readable_floor {
                    return Err(
                        format!("{role:?} fragment fell below the readable type floor").into(),
                    );
                }
                locations.entry(fragment.source_node_id).or_default().push((
                    page_index,
                    region.placement,
                    fragment.frame,
                ));
                page_frames.push((fragment.id, fragment.frame));
                if role == SemanticRole::Figure {
                    media_fragments += 1;
                    if fragment.media.is_none()
                        || fragment.frame.width.min(fragment.frame.height)
                            < policy.readable_media_floor
                    {
                        return Err("media fragment is undersized or lacks an aspect fit".into());
                    }
                    if let Some(raw) = metadata
                        .alternative_text
                        .as_deref()
                        .and_then(parse_gate_media_alternative_text)
                    {
                        let media = fragment.media.expect("checked media placement");
                        if media.source_size.width != raw.expected_width
                            || media.source_size.height != raw.expected_height
                        {
                            return Err(format!(
                                "{} used source axes {}x{} instead of {}x{}",
                                raw.case,
                                media.source_size.width,
                                media.source_size.height,
                                raw.expected_width,
                                raw.expected_height
                            )
                            .into());
                        }
                        let aspect_error = aspect_error_per_million(media);
                        let crop_loss = crop_loss_per_mille(media);
                        if aspect_error > 25 {
                            return Err(format!(
                                "{} distorted its source aspect by {aspect_error} ppm",
                                raw.case
                            )
                            .into());
                        }
                        if crop_loss > u64::from(policy.max_cover_crop_per_mille) {
                            return Err(format!(
                                "{} cropped {crop_loss} per mille of its source",
                                raw.case
                            )
                            .into());
                        }
                        if media.fit == ContentFit::Contain && crop_loss != 0 {
                            return Err("contain placement unexpectedly cropped its source".into());
                        }
                        media_evidence.push(MediaEvidence {
                            semantic_id: fragment.id.to_string(),
                            case: raw.case,
                            item: raw.item,
                            page_index,
                            expected_width: raw.expected_width,
                            expected_height: raw.expected_height,
                            source_width: media.source_size.width,
                            source_height: media.source_size.height,
                            frame_width: media.visible_frame.width,
                            frame_height: media.visible_frame.height,
                            fit: match media.fit {
                                ContentFit::Contain => "contain",
                                ContentFit::Cover => "cover",
                                ContentFit::None => "none",
                            },
                            aspect_error_per_million: aspect_error,
                            crop_loss_per_mille: crop_loss,
                        });
                    }
                }
                if role == SemanticRole::Table {
                    table_fragments += 1;
                    if !tables_on_page.insert(fragment.source_node_id) {
                        return Err(
                            "one table produced multiple editable fragments on one page".into()
                        );
                    }
                    let FragmentSlice::TableRows { start, end } = fragment.slice else {
                        return Err("table fragment does not retain an editable row slice".into());
                    };
                    table_slices
                        .entry(fragment.source_node_id)
                        .or_default()
                        .push((start, end));
                }
                if let RegionPlacement::Slot(slot) = region.placement {
                    *slot_loads.entry(slot).or_default() += fragment_units(fragment.slice);
                }
            }
        }

        for (index, (left_id, left)) in page_frames.iter().enumerate() {
            for (right_id, right) in &page_frames[index + 1..] {
                if rectangles_overlap(*left, *right) {
                    return Err(format!(
                        "source fragments {left_id} and {right_id} overlap on page {}",
                        page.id
                    )
                    .into());
                }
            }
        }

        if page.topology.kind == LayoutTopology::FlowColumns {
            flow_pages += 1;
            if slot_loads.len() != usize::from(page.topology.slot_count) {
                return Err("flow-column page left a selected column empty".into());
            }
            let minimum = slot_loads.values().copied().min().unwrap_or_default();
            let maximum = slot_loads.values().copied().max().unwrap_or_default();
            if minimum == 0 || maximum > minimum.saturating_mul(2).saturating_add(1) {
                return Err(format!("flow columns are visibly unbalanced: {slot_loads:?}").into());
            }
        }
    }

    for slices in table_slices.values_mut() {
        slices.sort_unstable();
        if slices.first().is_none_or(|slice| slice.0 != 0)
            || slices.windows(2).any(|pair| pair[0].1 != pair[1].0)
        {
            return Err(format!("table continuation slices are fragmented: {slices:?}").into());
        }
    }

    for page in plan.pages.iter().filter(|page| {
        page.continuation.total > 1 && page.continuation.ordinal == page.continuation.total
    }) {
        let final_units = page
            .regions
            .iter()
            .flat_map(|region| &region.fragments)
            .filter(|fragment| {
                !matches!(
                    semantics
                        .get(&fragment.source_node_id)
                        .map(|metadata| metadata.role),
                    Some(SemanticRole::Title | SemanticRole::Subtitle)
                )
            })
            .map(|fragment| fragment_units(fragment.slice))
            .sum::<u32>();
        if final_units < 2 {
            return Err(format!(
                "continuation for logical slide {} produced a singleton final-page orphan",
                page.logical_slide_id
            )
            .into());
        }
    }

    for slide in &spec.logical_slides {
        for relation in &slide.media_text_relations {
            let media = locations
                .get(&relation.media_node_id)
                .ok_or("media/text relation lost its media")?;
            let text = locations
                .get(&relation.text_node_id)
                .ok_or("media/text relation lost its text")?;
            let shared = media.iter().find_map(|media_location| {
                text.iter()
                    .find(|text_location| text_location.0 == media_location.0)
                    .map(|text_location| (media_location, text_location))
            });
            let Some((media_location, text_location)) = shared else {
                return Err(format!(
                    "related media {} and text {} never share a page",
                    relation.media_node_id, relation.text_node_id
                )
                .into());
            };
            if relation.explicit_caption && media_location.1 != text_location.1 {
                return Err("an explicit caption escaped its media topology slot".into());
            }
        }
        let pair_media = slide
            .media_text_relations
            .iter()
            .filter(|relation| relation.explicit_caption)
            .map(|relation| relation.media_node_id)
            .collect::<Vec<_>>();
        if pair_media.len() >= 7 {
            let mut loads = BTreeMap::<usize, usize>::new();
            for media_id in pair_media {
                let page_index = locations
                    .get(&media_id)
                    .and_then(|items| items.first())
                    .map(|location| location.0)
                    .ok_or("gallery pair lost its media page")?;
                *loads.entry(page_index).or_default() += 1;
            }
            let minimum = loads.values().copied().min().unwrap_or_default();
            let maximum = loads.values().copied().max().unwrap_or_default();
            if minimum < 2 || maximum.saturating_sub(minimum) > 1 {
                return Err(
                    format!("media/text continuation is visibly unbalanced: {loads:?}").into(),
                );
            }
        }
    }

    if flow_pages == 0
        || gallery_pages == 0
        || media_fragments < 127
        || media_evidence.len() != 117
        || table_fragments < 2
    {
        return Err("canonical corpus did not exercise every required layout family".into());
    }
    assert_single_slide_invalidation(template, spec, plan)?;

    media_evidence.sort_by(|left, right| left.semantic_id.cmp(&right.semantic_id));
    let images = media_evidence
        .iter()
        .map(media_evidence_json)
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        concat!(
            "{{\"schema\":2,\"corpus\":\"autolayout-v3\",\"counts\":{{",
            "\"logicalSlides\":{},\"physicalPages\":{},\"flowPages\":{},",
            "\"galleryPages\":{},\"mediaFragments\":{},\"qualityImages\":{},",
            "\"tableFragments\":{}}},\"images\":[{}],",
            "\"contracts\":{{\"exactSourceCoverage\":true,\"noOverlap\":true,",
            "\"readableType\":true,\"balancedFlow\":true,",
            "\"noSingletonFinalOrphan\":true,\"boundedMedia\":true,",
            "\"aspectFidelity\":true,\"boundedCrop\":true,\"displayAxisFidelity\":true,",
            "\"mediaTextCohesion\":true,\"balancedMediaPagination\":true,",
            "\"singleEditableTablePerSlice\":true,\"singleSlideInvalidation\":true}}}}\n"
        ),
        spec.logical_slides.len(),
        plan.pages.len(),
        flow_pages,
        gallery_pages,
        media_fragments,
        media_evidence.len(),
        table_fragments,
        images,
    ))
}

struct ParsedGateMedia {
    case: String,
    item: u32,
    expected_width: u32,
    expected_height: u32,
}

fn parse_gate_media_alternative_text(value: &str) -> Option<ParsedGateMedia> {
    let mut parts = value.split('|');
    (parts.next()? == "gate-media").then_some(())?;
    let case = parts.next()?.to_owned();
    let item = parts.next()?.parse().ok()?;
    let expected_width = parts.next()?.parse().ok()?;
    let expected_height = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some(ParsedGateMedia {
        case,
        item,
        expected_width,
        expected_height,
    })
}

fn aspect_error_per_million(media: wasmppt_deck::MediaPlacement) -> u64 {
    let crop = media.crop.unwrap_or_default();
    let remaining_x = 100_000u128.saturating_sub(u128::from(crop.left + crop.right));
    let remaining_y = 100_000u128.saturating_sub(u128::from(crop.top + crop.bottom));
    let left = u128::try_from(media.visible_frame.width.max(0))
        .unwrap_or(u128::MAX)
        .saturating_mul(u128::from(media.source_size.height))
        .saturating_mul(remaining_y);
    let right = u128::try_from(media.visible_frame.height.max(0))
        .unwrap_or(u128::MAX)
        .saturating_mul(u128::from(media.source_size.width))
        .saturating_mul(remaining_x);
    u64::try_from(
        left.abs_diff(right)
            .saturating_mul(1_000_000)
            .checked_div(left.max(right).max(1))
            .unwrap_or(u128::MAX),
    )
    .unwrap_or(u64::MAX)
}

fn crop_loss_per_mille(media: wasmppt_deck::MediaPlacement) -> u64 {
    let crop = media.crop.unwrap_or_default();
    let remaining_x = 100_000u64.saturating_sub(u64::from(crop.left + crop.right));
    let remaining_y = 100_000u64.saturating_sub(u64::from(crop.top + crop.bottom));
    let total = 100_000u64.saturating_mul(100_000);
    total
        .saturating_sub(remaining_x.saturating_mul(remaining_y))
        .saturating_mul(1_000)
        / total
}

fn rectangles_overlap(left: EmuRect, right: EmuRect) -> bool {
    left.x < right.x.saturating_add(right.width)
        && right.x < left.x.saturating_add(left.width)
        && left.y < right.y.saturating_add(right.height)
        && right.y < left.y.saturating_add(left.height)
}

fn media_evidence_json(evidence: &MediaEvidence) -> String {
    format!(
        concat!(
            "{{\"semanticId\":\"{}\",\"case\":\"{}\",\"item\":{},",
            "\"pageIndex\":{},\"expectedWidth\":{},\"expectedHeight\":{},",
            "\"sourceWidth\":{},\"sourceHeight\":{},\"frameWidth\":{},",
            "\"frameHeight\":{},\"fit\":\"{}\",\"aspectErrorPerMillion\":{},",
            "\"cropLossPerMille\":{}}}"
        ),
        evidence.semantic_id,
        evidence.case,
        evidence.item,
        evidence.page_index,
        evidence.expected_width,
        evidence.expected_height,
        evidence.source_width,
        evidence.source_height,
        evidence.frame_width,
        evidence.frame_height,
        evidence.fit,
        evidence.aspect_error_per_million,
        evidence.crop_loss_per_mille,
    )
}

fn assert_single_slide_invalidation(
    template: &[u8],
    previous_spec: &DeckSpec,
    previous_plan: &DeckPlan,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut next_spec = previous_spec.clone();
    let target = next_spec.logical_slides[1].id;
    let node = next_spec.logical_slides[1]
        .nodes
        .iter_mut()
        .find(|node| node.role == SemanticRole::Prose)
        .ok_or("invalidation fixture lost its prose node")?;
    mutate_content(&mut node.content, &mut next_spec.resources);
    let compiled = ThemeTemplateCompiler::default().compile(template.to_vec())?;
    let update = DeckPlanner::default().replan(
        previous_spec,
        previous_plan,
        &next_spec,
        &compiled.plan,
        &FontCatalog::default(),
        &DeckLimits::default(),
    )?;
    let previous_target_pages = previous_plan
        .pages
        .iter()
        .filter(|page| page.logical_slide_id == target)
        .count();
    if update.invalidated_logical_slides != vec![target]
        || update.reused_pages != previous_plan.pages.len() - previous_target_pages
    {
        return Err(format!(
            "incremental replan escaped one logical slide: invalidated={:?}, reused={}",
            update.invalidated_logical_slides, update.reused_pages
        )
        .into());
    }
    Ok(())
}

fn semantic_metadata(spec: &DeckSpec) -> BTreeMap<StableId, SemanticMetadata> {
    fn collect(nodes: &[SemanticNode], metadata: &mut BTreeMap<StableId, SemanticMetadata>) {
        for node in nodes {
            metadata.insert(
                node.id,
                SemanticMetadata {
                    role: node.role,
                    alternative_text: match &node.content {
                        SemanticContent::Image(image) => Some(image.alt_text.clone()),
                        _ => None,
                    },
                },
            );
            match &node.content {
                SemanticContent::Children(children) => collect(children, metadata),
                SemanticContent::List(list) => {
                    for item in &list.items {
                        collect(&item.blocks, metadata);
                        for child in &item.children {
                            for nested in &child.items {
                                collect(&nested.blocks, metadata);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut metadata = BTreeMap::new();
    for slide in &spec.logical_slides {
        collect(&slide.nodes, &mut metadata);
    }
    metadata
}

const fn is_text_role(role: SemanticRole) -> bool {
    !matches!(
        role,
        SemanticRole::Figure
            | SemanticRole::Gallery
            | SemanticRole::Chart
            | SemanticRole::Diagram
            | SemanticRole::DisplayMath
    )
}

const fn fragment_units(slice: FragmentSlice) -> u32 {
    match slice {
        FragmentSlice::Whole => 1,
        FragmentSlice::Text { start, end }
        | FragmentSlice::ListItems { start, end }
        | FragmentSlice::TableRows { start, end }
        | FragmentSlice::CodeLines { start, end } => end.saturating_sub(start),
    }
}

fn assert_role_mutations(
    engine: &mut WasmpptEngine,
    template: &[u8],
    encoded_spec: &[u8],
    baseline: &Evidence,
) -> Result<(), Box<dyn std::error::Error>> {
    let limits = DeckLimits::default();
    let roles = [
        SemanticRole::Title,
        SemanticRole::Subtitle,
        SemanticRole::Prose,
        SemanticRole::Section,
        SemanticRole::List,
        SemanticRole::ListItem,
        SemanticRole::Figure,
        SemanticRole::Caption,
        SemanticRole::Gallery,
        SemanticRole::Table,
        SemanticRole::Chart,
        SemanticRole::Code,
        SemanticRole::Diagram,
        SemanticRole::DisplayMath,
        SemanticRole::Quote,
        SemanticRole::Credit,
        SemanticRole::Definition,
        SemanticRole::DefinitionTerm,
        SemanticRole::DefinitionDescription,
        SemanticRole::Statement,
    ];
    for role in roles {
        let mut mutated = DeckSpec::decode(encoded_spec, &limits)?;
        let (slides, resources) = (&mut mutated.logical_slides, &mut mutated.resources);
        let found = slides
            .iter_mut()
            .any(|slide| mutate_nodes_for_role(&mut slide.nodes, resources, role));
        if !found {
            return Err(format!("deck gate does not contain role {role:?}").into());
        }
        let encoded = mutated.encode(&limits)?;
        let evidence = execute(engine, template, &encoded)?;
        if evidence.plan == baseline.plan
            && evidence.slides == baseline.slides
            && evidence.pptx == baseline.pptx
        {
            return Err(format!("deck outputs are insensitive to role {role:?}").into());
        }
    }
    Ok(())
}

fn mutate_nodes_for_role(
    nodes: &mut [SemanticNode],
    resources: &mut [DeckResource],
    role: SemanticRole,
) -> bool {
    for node in nodes {
        if node.role == role {
            mutate_content(&mut node.content, resources);
            return true;
        }
        match &mut node.content {
            SemanticContent::Children(children) => {
                if mutate_nodes_for_role(children, resources, role) {
                    return true;
                }
            }
            SemanticContent::List(list) => {
                for item in &mut list.items {
                    if mutate_nodes_for_role(&mut item.blocks, resources, role) {
                        return true;
                    }
                    for child in &mut item.children {
                        if mutate_list_for_role(child, resources, role) {
                            return true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    false
}

fn mutate_list_for_role(
    list: &mut wasmppt_deck::ListContent,
    resources: &mut [DeckResource],
    role: SemanticRole,
) -> bool {
    for item in &mut list.items {
        if mutate_nodes_for_role(&mut item.blocks, resources, role) {
            return true;
        }
        for child in &mut item.children {
            if mutate_list_for_role(child, resources, role) {
                return true;
            }
        }
    }
    false
}

fn mutate_content(content: &mut SemanticContent, resources: &mut [DeckResource]) {
    match content {
        SemanticContent::Text(text) => text.runs[0].text.push_str(" mutation"),
        SemanticContent::Children(children) => mutate_content(&mut children[0].content, resources),
        SemanticContent::Image(image) => image.alt_text.push_str(" mutation"),
        SemanticContent::List(list) => list.ordered = !list.ordered,
        SemanticContent::Table(table) => {
            table.rows[0].cells[0].content.runs[0]
                .text
                .push_str(" mutation");
        }
        SemanticContent::Chart(chart) => chart.series[0].values[0] += 1.0,
        SemanticContent::Code(code) => code.code.push_str("// mutation\n"),
        SemanticContent::Svg(svg) => {
            svg.source_text
                .get_or_insert_default()
                .push_str(" mutation");
            let resource = resources
                .iter_mut()
                .find(|resource| resource.id == svg.resource_id)
                .expect("SVG mutation resource must exist");
            resource.bytes.extend_from_slice(b"<!-- mutation -->");
        }
    }
}

fn assert_atomic_overflow(template: &[u8], spec: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let limits = DeckLimits::default();
    let decoded = DeckSpec::decode(spec, &limits)?;
    let compiled = ThemeTemplateCompiler::default().compile(template.to_vec())?;
    let failure = DeckPlanner::default()
        .plan(&decoded, &compiled.plan, &FontCatalog::default(), &limits)
        .expect_err("atomic overflow fixture unexpectedly produced a partial plan");
    if failure.code != DeckDiagnosticCode::PLAN_ATOMIC_OVERFLOW {
        return Err(format!("atomic overflow returned {:?}", failure.code).into());
    }
    Ok(())
}

fn execute(
    engine: &mut WasmpptEngine,
    template: &[u8],
    spec: &[u8],
) -> Result<Evidence, Box<dyn std::error::Error>> {
    let plan_started = Instant::now();
    let limits = DeckLimits::default();
    let decoded_spec = DeckSpec::decode(spec, &limits)?;
    let compiled = ThemeTemplateCompiler::default().compile(template.to_vec())?;
    if !compiled.cacheable {
        return Err(format!(
            "deck gate Starter diagnostics: {:?}",
            compiled.plan.diagnostics
        )
        .into());
    }
    let native_plan = DeckPlanner::default().plan(
        &decoded_spec,
        &compiled.plan,
        &FontCatalog::default(),
        &limits,
    )?;
    let native_plan_bytes = native_plan.encode(&limits)?;
    let template_handle = engine
        .prepare_deck_template(template)
        .map_err(|_| io::Error::other("native deck template compilation failed"))?;
    if !engine
        .deck_template_cacheable(template_handle)
        .map_err(|_| io::Error::other("native deck template cache query failed"))?
    {
        return Err("deck gate Starter is not cacheable".into());
    }
    let template_plan = engine
        .deck_template_plan(template_handle)
        .map_err(|_| io::Error::other("native template plan encoding failed"))?;
    let session = engine
        .create_deck_session_with_plan(template_handle, spec, &native_plan_bytes)
        .map_err(|_| io::Error::other("native deck planning failed"))?;
    let revision = engine
        .deck_session_revision(session)
        .map_err(|_| io::Error::other("native deck revision query failed"))?;
    let plan = engine
        .deck_session_plan(session, revision)
        .map_err(|_| io::Error::other("native deck plan encoding failed"))?;
    let slide_count = engine
        .deck_session_slide_count(session)
        .map_err(|_| io::Error::other("native deck slide count failed"))?;
    let plan_ms = plan_started.elapsed().as_secs_f64() * 1_000.0;
    let resolve_started = Instant::now();
    let slides = (0..slide_count)
        .map(|slide_index| {
            engine
                .resolve_deck_session_slide(session, revision, slide_index)
                .map_err(|_| io::Error::other("native deck display-list resolution failed"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let resolve_all_ms = resolve_started.elapsed().as_secs_f64() * 1_000.0;
    let export_started = Instant::now();
    let generation = engine
        .start_deck_session_generation(session, revision)
        .map_err(|_| io::Error::other("native deck export failed"))?;
    let mut pptx = Vec::new();
    while !engine
        .generation_done(generation)
        .map_err(|_| io::Error::other("native deck export state failed"))?
    {
        pptx.extend(
            engine
                .generation_pull(generation, 64 * 1024)
                .map_err(|_| io::Error::other("native deck export pull failed"))?,
        );
    }
    let export_ms = export_started.elapsed().as_secs_f64() * 1_000.0;
    if !engine.release_generation(generation)
        || !engine.release_deck_session(session)
        || !engine.release_deck_template(template_handle)
    {
        return Err("native deck gate leaked an engine handle".into());
    }
    Ok(Evidence {
        template_plan,
        plan,
        slides,
        pptx,
        timings: Timings {
            plan_ms,
            resolve_all_ms,
            export_ms,
        },
    })
}

fn timings_json(plan: &[f64], resolve_all: &[f64], export: &[f64]) -> String {
    let numbers = |samples: &[f64]| {
        samples
            .iter()
            .map(|sample| format!("{sample:.6}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        concat!(
            "{{\"planSamplesMs\":[{}],\"resolveAllSamplesMs\":[{}],",
            "\"exportSamplesMs\":[{}],\"summary\":{{",
            "\"coldPlanMs\":{:.6},\"warmPlanP50Ms\":{:.6},",
            "\"warmPlanP95Ms\":{:.6},\"resolveAllP50Ms\":{:.6},",
            "\"resolveAllP95Ms\":{:.6},\"exportP50Ms\":{:.6},",
            "\"exportP95Ms\":{:.6}}}}}\n"
        ),
        numbers(plan),
        numbers(resolve_all),
        numbers(export),
        plan[0],
        percentile(&plan[1..], 0.5),
        percentile(&plan[1..], 0.95),
        percentile(resolve_all, 0.5),
        percentile(resolve_all, 0.95),
        percentile(export, 0.5),
        percentile(export, 0.95),
    )
}

fn percentile(samples: &[f64], quantile: f64) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() as f64 * quantile).ceil() as usize).saturating_sub(1)]
}

fn topology_json(plan: &DeckPlan) -> String {
    let pages = plan
        .pages
        .iter()
        .enumerate()
        .map(|(index, page)| {
            format!(
                concat!(
                    "{{\"slideIndex\":{},\"pageId\":\"{}\",",
                    "\"logicalSlideId\":\"{}\",\"hidden\":{},",
                    "\"continuationOrdinal\":{},\"continuationTotal\":{},",
                    "\"continuationLabel\":{}}}"
                ),
                index,
                page.id,
                page.logical_slide_id,
                page.hidden,
                page.continuation.ordinal,
                page.continuation.total,
                page.continuation
                    .label
                    .as_ref()
                    .map_or_else(|| "null".to_owned(), |label| format!("\"{label}\"")),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let presentable = plan
        .pages
        .iter()
        .enumerate()
        .filter_map(|(index, page)| (!page.hidden).then_some(index.to_string()))
        .collect::<Vec<_>>()
        .join(",");
    let diagnostics = plan
        .diagnostics
        .iter()
        .map(diagnostic_json)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"slideCount\":{},\"presentableSlides\":[{presentable}],",
            "\"pages\":[{pages}],\"diagnostics\":[{diagnostics}]}}\n"
        ),
        plan.pages.len(),
        presentable = presentable,
        pages = pages,
        diagnostics = diagnostics,
    )
}

fn diagnostic_json(diagnostic: &DeckDiagnostic) -> String {
    let mut fields = vec![
        format!("\"code\":{}", diagnostic.code.0),
        diagnostic
            .code
            .known_name()
            .map(|name| format!("\"name\":{}", json_string(name)))
            .unwrap_or_default(),
        format!(
            "\"severity\":{}",
            json_string(match diagnostic.severity {
                DiagnosticSeverity::Info => "info",
                DiagnosticSeverity::Warning => "warning",
                DiagnosticSeverity::Error => "error",
            })
        ),
        format!("\"message\":{}", json_string(&diagnostic.message)),
    ];
    fields.retain(|field| !field.is_empty());
    if let Some(source) = &diagnostic.source {
        fields.push(format!(
            concat!("\"source\":{{\"source\":{},\"start\":{},\"end\":{}}}"),
            json_string(&source.source),
            source.start,
            source.end,
        ));
    }
    if let Some(node_id) = diagnostic.node_id {
        fields.push(format!("\"nodeId\":{}", json_string(&node_id.to_string())));
    }
    if let Some(page_id) = diagnostic.page_id {
        fields.push(format!("\"pageId\":{}", json_string(&page_id.to_string())));
    }
    format!("{{{}}}", fields.join(","))
}

fn json_string(value: &str) -> String {
    let mut escaped = String::from("\"");
    for character in value.chars() {
        match character {
            '\"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('\"');
    escaped
}
