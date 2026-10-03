//! Immutable semantic revisions and registered inputs for the host SDK.

use super::*;
use sha2::{Digest, Sha256};
use wasmppt_deck::{DeckResource, TemplateLayoutCapability};
use wasmppt_deck_layout::FontFace;

const DEFAULT_RETAINED_BYTES: usize = 256 * 1024 * 1024;

pub(super) struct DeckInputs {
    assets: HashMap<u32, DeckResource>,
    fonts: HashMap<u32, FontFace>,
    maximum_bytes: usize,
}

impl Default for DeckInputs {
    fn default() -> Self {
        Self {
            assets: HashMap::new(),
            fonts: HashMap::new(),
            maximum_bytes: DEFAULT_RETAINED_BYTES,
        }
    }
}

impl DeckInputs {
    pub(super) fn contains(&self, handle: u32) -> bool {
        self.assets.contains_key(&handle) || self.fonts.contains_key(&handle)
    }
}

#[wasm_bindgen]
impl WasmpptEngine {
    /// Tighten the SDK's conservative retained-state budget. Transient parser/composer limits
    /// remain independently bounded; this is not a measurement of the process heap.
    pub fn configure_deck_budget(&mut self, maximum_bytes: u32) -> Result<(), JsValue> {
        if maximum_bytes == 0 || maximum_bytes as usize > DEFAULT_RETAINED_BYTES {
            return Err(coded_error(
                "WasmpptOptionError",
                "invalid deck retained-byte budget",
            ));
        }
        if self.deck_retained_bytes() > maximum_bytes as usize {
            return Err(coded_error(
                "WasmpptLimitError",
                "existing deck state exceeds the budget",
            ));
        }
        self.deck_inputs.maximum_bytes = maximum_bytes as usize;
        Ok(())
    }

    /// Register one bounded WDSF resource. Subsequent revisions send handles, never its bytes.
    pub fn register_deck_asset(&mut self, payload: &[u8]) -> Result<u32, JsValue> {
        let mut spec = DeckSpec::decode(payload, &DeckLimits::default())
            .map_err(|error| coded_error("WasmpptDeckSpecError", error))?;
        if !spec.logical_slides.is_empty() || spec.resources.len() != 1 {
            return Err(coded_error(
                "WasmpptDeckSpecError",
                "asset registration requires one resource",
            ));
        }
        let resource = spec.resources.remove(0);
        self.check_deck_capacity(resource.bytes.len().saturating_add(1024))?;
        let handle = self.allocate_handle()?;
        self.deck_inputs.assets.insert(handle, resource);
        Ok(handle)
    }

    pub fn register_deck_font(
        &mut self,
        family: &str,
        face_index: u32,
        bytes: &[u8],
    ) -> Result<u32, JsValue> {
        if family.is_empty() || family.len() > 1024 {
            return Err(coded_error("WasmpptOptionError", "invalid font family"));
        }
        wasmppt_shaper::shape(
            bytes,
            "",
            wasmppt_shaper::ShapeOptions {
                face_index,
                ..wasmppt_shaper::ShapeOptions::default()
            },
        )
        .map_err(|error| coded_error("WasmpptDeckSpecError", error))?;
        self.check_deck_capacity(bytes.len().saturating_add(1024))?;
        let handle = self.allocate_handle()?;
        self.deck_inputs.fonts.insert(
            handle,
            FontFace {
                family: family.to_owned(),
                face_index,
                bytes: bytes.into(),
            },
        );
        Ok(handle)
    }

    /// Existing snapshots own shared bytes independently of these registration handles.
    pub fn release_deck_input(&mut self, handle: u32) -> bool {
        self.deck_inputs.assets.remove(&handle).is_some()
            || self.deck_inputs.fonts.remove(&handle).is_some()
    }

    /// Fork from a previous accepted snapshot without ever modifying it. The new handle is
    /// published only after validation, planning, composition and retained-budget checks pass.
    pub fn create_deck_snapshot(
        &mut self,
        template_handle: u32,
        spec: &[u8],
        assets: &[u32],
        fonts: &[u32],
        previous_handle: u32,
    ) -> Result<u32, JsValue> {
        let limits = DeckLimits::default();
        let mut spec = DeckSpec::decode(spec, &limits)
            .map_err(|error| coded_error("WasmpptDeckSpecError", error))?;
        if !spec.resources.is_empty() || assets.len() > limits.max_collection_items {
            return Err(coded_error(
                "WasmpptDeckSpecError",
                "snapshot resources must be registered",
            ));
        }
        for handle in assets {
            spec.resources.push(
                self.deck_inputs
                    .assets
                    .get(handle)
                    .ok_or_else(|| coded_error("WasmpptHandleError", "unknown asset handle"))?
                    .clone(),
            );
        }
        let fonts = self.deck_font_catalog(fonts)?;
        let template = self.deck_template_record(template_handle)?.clone();
        let previous = if previous_handle == 0 {
            None
        } else {
            Some(self.deck_session(previous_handle)?)
        };
        let plan = plan_snapshot(&spec, &template, &fonts, previous)?;
        let overlay = DeckComposer
            .compose(
                template.bytes.clone(),
                &spec,
                &template.plan,
                &plan,
                &limits,
                &ComposeLimits::default(),
            )
            .map_err(|error| coded_error("WasmpptDeckComposeError", error))?;
        let document = match previous {
            Some(previous) if previous.template.plan.id == template.plan.id => {
                let changed = overlay.changed_parts_since(&previous.overlay);
                open_deck_revision_document(
                    &previous.document,
                    &previous.overlay,
                    &overlay,
                    &changed,
                    previous.plan.pages.len() != plan.pages.len(),
                )?
            }
            _ => PresentationDocument::open_source(Arc::new(overlay.clone()))
                .map_err(layout_error)?,
        };
        // Conservatively charge shared inputs again per retained revision. Reserving the complete
        // scene-cache allowance ensures a later lazy resolve cannot overrun this account.
        let retained_weight = spec
            .encode(&limits)
            .map_err(|error| coded_error("WasmpptDeckSpecError", error))?
            .len()
            .saturating_add(
                plan.encode(&limits)
                    .map_err(|error| coded_error("WasmpptDeckPlanError", error))?
                    .len(),
            )
            .saturating_mul(2)
            .saturating_add(template.bytes.len())
            .saturating_add(
                usize::try_from(overlay.stats().materialized_bytes).unwrap_or(usize::MAX),
            )
            .saturating_add(
                fonts
                    .faces
                    .iter()
                    .map(|face| face.bytes.len())
                    .sum::<usize>(),
            )
            .saturating_add(SESSION_SCENE_CACHE_BYTES);
        self.check_deck_capacity(retained_weight)?;
        let revision = previous.map_or(Ok(0), |record| {
            record
                .revision
                .checked_add(1)
                .ok_or_else(|| coded_error("WasmpptRevisionError", "snapshot revision exhausted"))
        })?;
        let scenes = previous.map_or_else(
            || SceneCache::new(SESSION_SCENE_CACHE_BYTES),
            |record| record.scenes.clone(),
        );
        let handle = self.allocate_handle()?;
        self.deck_sessions.insert(
            handle,
            DeckSessionRecord {
                revision,
                template,
                spec,
                plan,
                overlay,
                document,
                scenes,
                fonts,
                retained_weight,
            },
        );
        Ok(handle)
    }

    /// Structured template profile; no consumer decodes a binary plan or searches its bytes.
    pub fn describe_deck_template(&self, handle: u32) -> Result<Array, JsValue> {
        let plan = &self.deck_template(handle)?.plan;
        let result = Array::new();
        result.push(&JsValue::from_f64(plan.page_size.width as f64));
        result.push(&JsValue::from_f64(plan.page_size.height as f64));
        let layouts = Array::new();
        for layout in &plan.layouts {
            let row = Array::new();
            row.push(&JsValue::from(layout.id.to_string()));
            row.push(&JsValue::from(match layout.capability {
                TemplateLayoutCapability::Title => "title",
                TemplateLayoutCapability::Statement => "statement",
                TemplateLayoutCapability::ContentEnvelope => "content-envelope",
            }));
            row.push(&JsValue::from(layout.matching_name.as_str()));
            let regions = Array::new();
            for region in plan
                .regions
                .iter()
                .filter(|region| region.layout_id == layout.id)
            {
                let entry = Array::new();
                entry.push(&JsValue::from(region.id.to_string()));
                entry.push(&JsValue::from(format!("{:?}", region.role).to_lowercase()));
                let frame = Array::new();
                for coordinate in [
                    region.frame.x,
                    region.frame.y,
                    region.frame.width,
                    region.frame.height,
                ] {
                    frame.push(&JsValue::from_f64(coordinate as f64));
                }
                entry.push(&frame);
                let accepts = Array::new();
                for role in &region.accepts {
                    accepts.push(&JsValue::from(role.code()));
                }
                entry.push(&accepts);
                regions.push(&entry);
            }
            row.push(&regions);
            layouts.push(&row);
        }
        result.push(&layouts);
        let fonts = Array::new();
        for set in [&plan.theme.major_fonts, &plan.theme.minor_fonts] {
            for family in [&set.latin, &set.east_asian, &set.complex_script]
                .into_iter()
                .flatten()
            {
                fonts.push(&JsValue::from(family.as_str()));
            }
        }
        for region in &plan.regions {
            for level in &region.text_levels {
                for family in [
                    &level.latin_typeface,
                    &level.east_asian_typeface,
                    &level.complex_script_typeface,
                ]
                .into_iter()
                .flatten()
                {
                    fonts.push(&JsValue::from(family.as_str()));
                }
            }
        }
        result.push(&fonts);
        result.push(&deck_diagnostics_array(&plan.diagnostics));
        Ok(result)
    }

    pub fn deck_accounted_bytes(&self) -> f64 {
        self.deck_retained_bytes() as f64
    }

    /// WPDL semantic IDs identify physical fragments; retain their semantic-node ownership.
    pub fn deck_snapshot_sources(&self, handle: u32, revision: u32) -> Result<Array, JsValue> {
        let record = self.deck_session(handle)?;
        require_deck_revision(record, revision)?;
        let result = Array::new();
        for page in &record.plan.pages {
            for region in &page.regions {
                for fragment in &region.fragments {
                    let row = Array::new();
                    row.push(&JsValue::from(fragment.id.to_string()));
                    row.push(&JsValue::from(fragment.source_node_id.to_string()));
                    result.push(&row);
                }
            }
        }
        Ok(result)
    }
}

impl WasmpptEngine {
    fn deck_font_catalog(&self, handles: &[u32]) -> Result<FontCatalog, JsValue> {
        if handles.len() > 256 {
            return Err(coded_error("WasmpptLimitError", "too many font faces"));
        }
        let mut digest = Sha256::new();
        let mut faces = Vec::with_capacity(handles.len());
        for handle in handles {
            let face = self
                .deck_inputs
                .fonts
                .get(handle)
                .ok_or_else(|| coded_error("WasmpptHandleError", "unknown font handle"))?;
            digest.update((face.family.len() as u64).to_le_bytes());
            digest.update(face.family.as_bytes());
            digest.update(face.face_index.to_le_bytes());
            digest.update((face.bytes.len() as u64).to_le_bytes());
            digest.update(&face.bytes);
            faces.push(face.clone());
        }
        Ok(FontCatalog {
            identity: digest.finalize().into(),
            default_family: None,
            faces,
        })
    }

    fn deck_retained_bytes(&self) -> usize {
        self.deck_inputs
            .assets
            .values()
            .map(|asset| asset.bytes.len().saturating_add(1024))
            .chain(
                self.deck_inputs
                    .fonts
                    .values()
                    .map(|font| font.bytes.len().saturating_add(1024)),
            )
            .chain(
                self.deck_templates
                    .values()
                    .map(|template| template.bytes.len()),
            )
            .chain(
                self.deck_sessions
                    .values()
                    .map(|record| record.retained_weight),
            )
            .fold(0, usize::saturating_add)
    }

    pub(super) fn check_deck_capacity(&self, additional: usize) -> Result<(), JsValue> {
        if self.deck_retained_bytes().saturating_add(additional) > self.deck_inputs.maximum_bytes {
            Err(coded_error(
                "WasmpptLimitError",
                "deck retained-byte budget exceeded",
            ))
        } else {
            Ok(())
        }
    }
}

fn plan_snapshot(
    spec: &DeckSpec,
    template: &DeckTemplateRecord,
    fonts: &FontCatalog,
    previous: Option<&DeckSessionRecord>,
) -> Result<DeckPlan, JsValue> {
    let planner = DeckPlanner::default();
    let limits = DeckLimits::default();
    if let Some(previous) = previous.filter(|previous| {
        previous.template.plan.id == template.plan.id && previous.fonts == *fonts
    }) {
        planner
            .replan(
                &previous.spec,
                &previous.plan,
                spec,
                &template.plan,
                fonts,
                &limits,
            )
            .map(|update| update.plan)
            .map_err(deck_layout_error)
    } else {
        planner
            .plan(spec, &template.plan, fonts, &limits)
            .map_err(deck_layout_error)
    }
}

pub(super) fn scene_fingerprint(
    record: &DeckSessionRecord,
    index: usize,
) -> Result<[u8; 32], JsValue> {
    let mut digest = Sha256::new();
    digest.update(
        record
            .document
            .slide_dependency_fingerprint(index)
            .map_err(layout_error)?,
    );
    let page = record
        .plan
        .pages
        .get(index)
        .ok_or_else(|| coded_error("WasmpptDeckPlanError", "page index out of bounds"))?;
    let logical = record
        .spec
        .logical_slides
        .iter()
        .find(|slide| slide.id == page.logical_slide_id)
        .ok_or_else(|| coded_error("WasmpptDeckPlanError", "logical slide is missing"))?;
    // OOXML does not contain authoring spans. Include their current metadata in the scene cache
    // identity so moving source text cannot serve stale hit-test ranges from an unchanged package.
    let source = DeckSpec {
        id: record.spec.id,
        logical_slides: vec![logical.clone()],
        resources: vec![],
    };
    digest.update(
        source
            .encode(&DeckLimits::default())
            .map_err(|error| coded_error("WasmpptDeckSpecError", error))?,
    );
    Ok(digest.finalize().into())
}
