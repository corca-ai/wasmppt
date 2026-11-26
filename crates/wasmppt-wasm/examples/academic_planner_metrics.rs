//! Reproducible native work counters for the academic aggregate, without adapter/ZIP costs.
use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf, time::Instant};
use wasmppt_deck::{DeckLimits, DeckSpec, PresentationPurpose};
use wasmppt_deck_layout::{DeckPlanner, FontCatalog, PlannerPolicy};
use wasmppt_deck_template::ThemeTemplateCompiler;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = PathBuf::from(env::args_os().nth(1).ok_or("expected fixture directory")?);
    let output = PathBuf::from(env::args_os().nth(2).ok_or("expected output JSON path")?);
    let spec_bytes = fs::read(input.join("deck-spec.wdsf"))?;
    let limits = DeckLimits::default();
    let source = DeckSpec::decode(&spec_bytes, &limits)?;
    let policy = PlannerPolicy::default();
    let mut rows = Vec::new();
    for template_name in ["starter.potx", "custom.potx"] {
        let bytes = fs::read(input.join(template_name))?;
        let compiled = ThemeTemplateCompiler::default().compile(bytes.clone())?;
        if !compiled.cacheable {
            return Err("invalid metrics template".into());
        }
        for (purpose, label) in [
            (PresentationPurpose::Conference, "conference"),
            (PresentationPurpose::Seminar, "seminar"),
            (PresentationPurpose::Handout, "handout"),
        ] {
            let mut spec = source.clone();
            spec.layout.presentation = purpose;
            let planner = DeckPlanner::new(policy.clone());
            let fonts = FontCatalog::default();
            let mut samples = Vec::new();
            let mut metrics_rows = Vec::new();
            let mut previous = None;
            let mut plan_bytes = Vec::new();
            let mut pages = 0;
            let mut expanded_pages = String::new();
            for _ in 0..7 {
                let start = Instant::now();
                let (plan, metrics) =
                    planner.plan_with_metrics(&spec, &compiled.plan, &fonts, &limits)?;
                samples.push(start.elapsed().as_secs_f64() * 1_000.0);
                if !plan.failed_slides.is_empty() {
                    return Err("incomplete metrics plan".into());
                }
                plan_bytes = plan.encode(&limits)?;
                if previous.as_ref().is_some_and(|value| value != &plan_bytes) {
                    return Err("metrics plans differ".into());
                }
                previous = Some(plan_bytes.clone());
                pages = plan.pages.len();
                expanded_pages = plan.pages.iter().filter_map(|page| {
                    let logical = spec.logical_slides.iter().find(|slide| slide.id == page.logical_slide_id)?;
                    if !logical.source.source.starts_with("expanded-academic/") { return None; }
                    let fragments = page.regions.iter().flat_map(|region| &region.fragments).map(|fragment| format!(
                        "{{\"node\":\"{}\",\"fontSize\":{},\"frame\":[{},{},{},{}]}}", fragment.source_node_id, fragment.type_choice.font_size,
                        fragment.frame.x, fragment.frame.y, fragment.frame.width, fragment.frame.height)).collect::<Vec<_>>();
                    Some(format!("{{\"case\":{:?},\"ordinal\":{},\"total\":{},\"slots\":{},\"fragments\":[{}]}}", logical.source.source, page.continuation.ordinal, page.continuation.total, page.topology.slot_count, fragments.join(",")))
                }).collect::<Vec<_>>().join(",");
                metrics_rows.push(format!(
                    "{{\"candidateAssignments\":{},\"measurements\":{}}}",
                    metrics.candidate_assignments, metrics.measurements
                ));
            }
            rows.push(format!("{{\"template\":\"{template_name}\",\"templateSha256\":\"{:x}\",\"purpose\":\"{label}\",\"pages\":{pages},\"planBytes\":{},\"planSha256\":\"{:x}\",\"planMs\":{samples:?},\"work\":[{}],\"expandedPages\":[{expanded_pages}]}}", Sha256::digest(&bytes), plan_bytes.len(), Sha256::digest(&plan_bytes), metrics_rows.join(",")));
        }
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        output,
        format!(
            "{{\"schema\":1,\"scope\":\"native complete planning only; no composition, rendering, or export; deterministic fallback fonts\",\"sourceSha256\":\"{:x}\",\"limits\":{{\"candidateAssignmentsPerSlideAttempt\":{},\"measurementsPerPass\":{}}},\"rows\":[{}]}}\n",
            Sha256::digest(&spec_bytes),
            policy.limits.max_candidate_assignments,
            policy.limits.max_measurements,
            rows.join(",")
        ),
    )?;
    Ok(())
}
