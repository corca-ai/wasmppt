use std::{env, fs, io, path::PathBuf, time::Instant};

use wasmppt_deck::{
    DeckDiagnosticCode, DeckLimits, DeckPlan, DeckResource, DeckSpec, SemanticContent,
    SemanticNode, SemanticRole,
};
use wasmppt_deck_layout::{DeckPlanner, FontCatalog};
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
    Ok(())
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
    format!(
        "{{\"slideCount\":{},\"presentableSlides\":[{presentable}],\"pages\":[{pages}]}}\n",
        plan.pages.len()
    )
}
