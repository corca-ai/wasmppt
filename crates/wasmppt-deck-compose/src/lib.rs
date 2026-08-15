//! Loss-aware composition of host-neutral deck plans into editable PresentationML overlays.

mod media;
mod package_xml;
mod slide;

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use media::{PreparedMediaKey, prepare_formula_media, prepare_media};
use package_xml::{patch_content_types, patch_presentation, patch_presentation_relationships};
use sha2::{Digest, Sha256};
pub use slide::{PlannedShape, planned_shapes};
use slide::{compose_slide, formula_svg_node_ids};
use wasmppt_deck::{
    DeckDiagnostic, DeckLimits, DeckPlan, DeckSpec, DeckTemplatePlan, DiagnosticSeverity,
    SemanticContent, SemanticNode, StableId, validate_deck_plan, validate_deck_spec,
};
use wasmppt_opc::{
    OverlayCursor, OverlayLimits, OverlayPart, OverlayStats, PackageGraph, PackageOverlay,
    PackagePartSource,
};

/// Resource and materialization limits for one composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposeLimits {
    pub max_media_bytes: usize,
    pub max_decoded_pixels: usize,
    pub max_overlay_parts: usize,
    pub max_overlay_bytes: usize,
}

impl Default for ComposeLimits {
    fn default() -> Self {
        Self {
            max_media_bytes: 32 * 1024 * 1024,
            max_decoded_pixels: 40_000_000,
            max_overlay_parts: 100_000,
            max_overlay_bytes: 256 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ComposeErrorCode {
    InvalidContract,
    TemplateMismatch,
    InvalidPackage,
    UnsupportedContent,
    InvalidMedia,
    WorkLimit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposeError {
    code: ComposeErrorCode,
    message: String,
}

impl ComposeError {
    fn new(code: ComposeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn code(&self) -> ComposeErrorCode {
        self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ComposeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ComposeError {}

/// An immutable live presentation revision backed by raw template bytes plus changed parts.
#[derive(Clone, Debug)]
pub struct PresentationOverlay {
    package: PackageOverlay,
    revision: [u8; 32],
    diagnostics: Vec<DeckDiagnostic>,
}

impl PresentationOverlay {
    #[must_use]
    pub const fn revision(&self) -> &[u8; 32] {
        &self.revision
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[DeckDiagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub fn stats(&self) -> OverlayStats {
        self.package.stats()
    }

    #[must_use]
    pub fn generation_cursor(&self) -> OverlayCursor {
        self.package.generation_cursor()
    }

    #[must_use]
    pub fn changed_parts_since(&self, previous: &Self) -> Vec<String> {
        self.package.changed_parts_since(&previous.package)
    }
}

impl PackagePartSource for PresentationOverlay {
    fn part_names(&self) -> Vec<String> {
        self.package.part_names()
    }
    fn is_modified(&self, name: &str) -> bool {
        self.package.is_modified(name)
    }
    fn contains_part(&self, name: &str) -> bool {
        self.package.contains_part(name)
    }
    fn read_part(&self, name: &str) -> wasmppt_opc::Result<Vec<u8>> {
        self.package.read_part(name)
    }
}

#[derive(Clone, Debug, Default)]
pub struct DeckComposer;

impl DeckComposer {
    pub fn compose(
        &self,
        template_bytes: impl Into<Arc<[u8]>>,
        spec: &DeckSpec,
        template: &DeckTemplatePlan,
        plan: &DeckPlan,
        deck_limits: &DeckLimits,
        compose_limits: &ComposeLimits,
    ) -> Result<PresentationOverlay, ComposeError> {
        let template_bytes = template_bytes.into();
        let mut diagnostics =
            validate_contracts(spec, template, plan, deck_limits, &template_bytes)?;

        let nodes = index_nodes(spec);
        let resources = spec
            .resources
            .iter()
            .map(|resource| (resource.id, resource))
            .collect::<BTreeMap<_, _>>();
        let layouts = template
            .layouts
            .iter()
            .map(|layout| (layout.id, layout))
            .collect::<BTreeMap<_, _>>();
        let regions = template
            .regions
            .iter()
            .map(|region| (region.id, region))
            .collect::<BTreeMap<_, _>>();
        let formula_svg_nodes = formula_svg_node_ids(&nodes);
        let mut prepared = BTreeMap::new();
        for node in nodes.values() {
            let (resource_id, key) = match &node.content {
                SemanticContent::Image(image) => (
                    image.resource_id,
                    PreparedMediaKey::Resource(image.resource_id),
                ),
                SemanticContent::Svg(svg) if formula_svg_nodes.contains(&node.id) => {
                    (svg.resource_id, PreparedMediaKey::Formula(node.id))
                }
                SemanticContent::Svg(svg) => {
                    (svg.resource_id, PreparedMediaKey::Resource(svg.resource_id))
                }
                _ => continue,
            };
            if prepared.contains_key(&key) {
                continue;
            }
            let resource = resources.get(&resource_id).ok_or_else(|| {
                ComposeError::new(
                    ComposeErrorCode::InvalidContract,
                    format!("node references missing resource {resource_id}"),
                )
            })?;
            let media = if matches!(key, PreparedMediaKey::Formula(_)) {
                let region = plan
                    .pages
                    .iter()
                    .flat_map(|page| &page.regions)
                    .find(|planned_region| {
                        planned_region
                            .fragments
                            .iter()
                            .any(|fragment| fragment.source_node_id == node.id)
                    })
                    .and_then(|planned_region| regions.get(&planned_region.template_region_id))
                    .copied()
                    .ok_or_else(|| {
                        ComposeError::new(
                            ComposeErrorCode::InvalidContract,
                            "formula fragment has no template region",
                        )
                    })?;
                let color = region
                    .text_levels
                    .first()
                    .and_then(|level| level.color.as_ref())
                    .map_or_else(
                        || theme_rgb(&template.theme, "dk1", 0),
                        |color| color.rgb & 0x00ff_ffff,
                    );
                prepare_formula_media(resource, node.id, color, compose_limits)?
            } else {
                prepare_media(resource, compose_limits)?
            };
            prepared.insert(key, media);
        }

        let slide_parts = (1..=plan.pages.len())
            .map(|index| format!("ppt/slides/slide{index}.xml"))
            .collect::<Vec<_>>();
        let mut overrides = BTreeMap::new();
        let mut generated_content_types = BTreeMap::new();
        for (index, page) in plan.pages.iter().enumerate() {
            let layout = layouts.get(&page.template_layout_id).ok_or_else(|| {
                ComposeError::new(
                    ComposeErrorCode::InvalidContract,
                    format!(
                        "page references missing template layout {}",
                        page.template_layout_id
                    ),
                )
            })?;
            let composed = compose_slide(
                page,
                layout,
                template.page_size,
                &template.theme,
                &regions,
                &nodes,
                &prepared,
            )?;
            let slide_part = &slide_parts[index];
            overrides.insert(slide_part.clone(), OverlayPart::deflated(composed.xml));
            overrides.insert(
                format!("ppt/slides/_rels/slide{}.xml.rels", index + 1),
                OverlayPart::deflated(composed.relationships),
            );
            for part in composed.parts {
                if overrides
                    .insert(part.name.clone(), OverlayPart::deflated(part.bytes))
                    .is_some()
                {
                    return Err(ComposeError::new(
                        ComposeErrorCode::InvalidContract,
                        format!("multiple fragments generated part {}", part.name),
                    ));
                }
                if let Some(content_type) = part.content_type {
                    generated_content_types.insert(part.name, content_type);
                }
            }
        }
        for media in prepared.values() {
            overrides.insert(
                media.part_name.clone(),
                OverlayPart::deflated(media.bytes.clone()),
            );
            generated_content_types.insert(media.part_name.clone(), media.content_type);
        }

        let source =
            wasmppt_opc::ZipArchive::from_bytes(template_bytes.clone()).map_err(|error| {
                ComposeError::new(ComposeErrorCode::InvalidPackage, error.to_string())
            })?;
        let removed = source
            .part_names()
            .into_iter()
            .filter(|name| {
                ((name.starts_with("ppt/slides/") || name.starts_with("ppt/slides/_rels/"))
                    && !overrides.contains_key(name))
                    || name.starts_with("ppt/notesSlides/")
                    || name.starts_with("ppt/notesSlides/_rels/")
            })
            .collect::<BTreeSet<_>>();
        let content_types = source
            .read_part("[Content_Types].xml")
            .map_err(package_error)?;
        let presentation = source
            .read_part("ppt/presentation.xml")
            .map_err(package_error)?;
        let presentation_rels = source
            .read_part("ppt/_rels/presentation.xml.rels")
            .map_err(package_error)?;
        let generated_parts = generated_content_types.into_iter().collect::<Vec<_>>();
        overrides.insert(
            "[Content_Types].xml".to_owned(),
            OverlayPart::deflated(patch_content_types(
                content_types,
                &slide_parts,
                &generated_parts,
                &removed,
            )?),
        );
        let (presentation_rels, relationship_ids) =
            patch_presentation_relationships(presentation_rels, &slide_parts)?;
        overrides.insert(
            "ppt/_rels/presentation.xml.rels".to_owned(),
            OverlayPart::deflated(presentation_rels),
        );
        overrides.insert(
            "ppt/presentation.xml".to_owned(),
            OverlayPart::deflated(patch_presentation(presentation, &relationship_ids)?),
        );

        let package = PackageOverlay::new(
            template_bytes,
            overrides,
            removed,
            &OverlayLimits {
                max_materialized_parts: compose_limits.max_overlay_parts,
                max_materialized_bytes: compose_limits.max_overlay_bytes,
            },
        )
        .map_err(package_error)?;
        let graph = PackageGraph::build_from(&package).map_err(|error| {
            ComposeError::new(
                ComposeErrorCode::InvalidPackage,
                format!("composed package graph is invalid: {error}"),
            )
        })?;
        if graph.diagnostics().iter().any(|diagnostic| {
            matches!(
                diagnostic.code,
                wasmppt_opc::DiagnosticCode::MissingContentTypes
                    | wasmppt_opc::DiagnosticCode::InvalidContentTypesXml
                    | wasmppt_opc::DiagnosticCode::InvalidContentTypesRoot
                    | wasmppt_opc::DiagnosticCode::DuplicateContentType
                    | wasmppt_opc::DiagnosticCode::MissingContentType
                    | wasmppt_opc::DiagnosticCode::InvalidRelationshipsXml
                    | wasmppt_opc::DiagnosticCode::InvalidRelationshipsRoot
                    | wasmppt_opc::DiagnosticCode::DuplicateRelationshipId
                    | wasmppt_opc::DiagnosticCode::InvalidRelationshipTarget
                    | wasmppt_opc::DiagnosticCode::MissingRelationshipTarget
                    | wasmppt_opc::DiagnosticCode::MixedConformance
            )
        }) {
            return Err(ComposeError::new(
                ComposeErrorCode::InvalidPackage,
                "composed package graph has structural diagnostics",
            ));
        }
        let revision = revision_id(template.template_hash, spec.id, plan, deck_limits, &package)?;
        diagnostics.extend(plan.diagnostics.clone());
        Ok(PresentationOverlay {
            package,
            revision,
            diagnostics,
        })
    }
}

fn validate_contracts(
    spec: &DeckSpec,
    template: &DeckTemplatePlan,
    plan: &DeckPlan,
    limits: &DeckLimits,
    template_bytes: &[u8],
) -> Result<Vec<DeckDiagnostic>, ComposeError> {
    let spec_report = validate_deck_spec(spec, limits);
    let plan_report = validate_deck_plan(spec, template, plan, limits);
    if !spec_report.is_valid()
        || !plan_report.is_valid()
        || template
            .diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Error)
    {
        return Err(ComposeError::new(
            ComposeErrorCode::InvalidContract,
            "deck, template, or plan validation failed",
        ));
    }
    if Sha256::digest(template_bytes).as_slice() != template.template_hash {
        return Err(ComposeError::new(
            ComposeErrorCode::TemplateMismatch,
            "template bytes do not match the compiled template hash",
        ));
    }
    for node in index_nodes(spec).values() {
        if let SemanticContent::List(list) = &node.content {
            ensure_editable_list(list)?;
        }
    }
    let mut diagnostics = plan_report.diagnostics;
    diagnostics.extend(template.diagnostics.clone());
    Ok(diagnostics)
}

fn ensure_editable_list(list: &wasmppt_deck::ListContent) -> Result<(), ComposeError> {
    for item in &list.items {
        if item
            .blocks
            .iter()
            .any(|block| !matches!(block.content, SemanticContent::Text(_)))
        {
            return Err(ComposeError::new(
                ComposeErrorCode::UnsupportedContent,
                "list items must contain editable text blocks in this composition slice",
            ));
        }
        for child in &item.children {
            ensure_editable_list(child)?;
        }
    }
    Ok(())
}

fn index_nodes(spec: &DeckSpec) -> BTreeMap<StableId, &SemanticNode> {
    fn insert_list<'a>(
        list: &'a wasmppt_deck::ListContent,
        output: &mut BTreeMap<StableId, &'a SemanticNode>,
    ) {
        for item in &list.items {
            item.blocks.iter().for_each(|node| insert(node, output));
            item.children
                .iter()
                .for_each(|child| insert_list(child, output));
        }
    }

    fn insert<'a>(node: &'a SemanticNode, output: &mut BTreeMap<StableId, &'a SemanticNode>) {
        output.insert(node.id, node);
        match &node.content {
            SemanticContent::Children(children) => {
                children.iter().for_each(|child| insert(child, output))
            }
            SemanticContent::List(list) => insert_list(list, output),
            _ => {}
        }
    }
    let mut output = BTreeMap::new();
    for slide in &spec.logical_slides {
        for node in &slide.nodes {
            insert(node, &mut output);
        }
    }
    output
}

fn theme_rgb(theme: &wasmppt_deck::TemplateTheme, slot: &str, fallback: u32) -> u32 {
    theme
        .colors
        .iter()
        .find(|color| color.slot == slot)
        .map_or(fallback, |color| color.rgb & 0x00ff_ffff)
}

fn revision_id(
    template_hash: [u8; 32],
    spec_id: StableId,
    plan: &DeckPlan,
    limits: &DeckLimits,
    package: &PackageOverlay,
) -> Result<[u8; 32], ComposeError> {
    let mut digest = Sha256::new();
    digest.update(b"wasmppt/presentation-overlay/v1\0");
    digest.update(template_hash);
    digest.update(spec_id.as_bytes());
    digest.update(plan.encode(limits).map_err(|error| {
        ComposeError::new(ComposeErrorCode::InvalidContract, error.to_string())
    })?);
    for name in package
        .part_names()
        .into_iter()
        .filter(|name| package.is_modified(name))
    {
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update(package.read_part(&name).map_err(package_error)?);
    }
    Ok(digest.finalize().into())
}

fn package_error(error: wasmppt_opc::Error) -> ComposeError {
    ComposeError::new(ComposeErrorCode::InvalidPackage, error.to_string())
}

fn stable_id_hex(id: StableId) -> String {
    id.to_string()
}

fn xml_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
