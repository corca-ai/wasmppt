//! Narrow WebAssembly boundary for the host-agnostic `wasmppt` core.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use js_sys::{Array, Error as JavaScriptError, Object, Reflect};
use wasm_bindgen::prelude::*;
use wasmppt_display::DisplayList;
use wasmppt_layout::{LayoutError, LayoutErrorCode, PresentationDocument};
use wasmppt_opc::{Error as OpcError, ErrorCode as OpcErrorCode, ZipArchive};
use wasmppt_template::{
    BindingDiagnostic, BindingDiagnosticCode, BindingKind, BindingSource, CompileError,
    CompileErrorCode, CompilerOptions, GenerateError, GenerateErrorCode, GenerationCursor,
    InjectionData, LiveSession, LiveSessionUpdate, MacroPolicy, PreparedTemplate, TemplateCompiler,
    TemplatePlan,
};

const SESSION_SCENE_CACHE_BYTES: usize = 16 * 1024 * 1024;

/// Returns the engine package version embedded in the Wasm module.
#[wasm_bindgen]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Resolve one slide to the compact backend-neutral display-list wire format.
#[wasm_bindgen]
pub fn resolve_display_list(presentation: &[u8], slide_index: u32) -> Result<Vec<u8>, JsValue> {
    let deck = PresentationDocument::open(presentation.to_vec()).map_err(layout_error)?;
    let resolved = deck
        .resolve_slide(slide_index as usize)
        .map_err(layout_error)?;
    Ok(DisplayList::from_resolve(&resolved).encode())
}

/// Stable signature used to compare native and Wasm display-list structure.
#[wasm_bindgen]
pub fn display_list_signature(presentation: &[u8], slide_index: u32) -> Result<String, JsValue> {
    let deck = PresentationDocument::open(presentation.to_vec()).map_err(layout_error)?;
    let resolved = deck
        .resolve_slide(slide_index as usize)
        .map_err(layout_error)?;
    Ok(format!(
        "{:016x}",
        DisplayList::from_resolve(&resolved).structural_signature()
    ))
}

/// Runtime-independent capabilities. Correctness always uses the scalar path.
#[wasm_bindgen]
pub struct EngineCapabilities {
    simd: bool,
    threads: bool,
}

#[wasm_bindgen]
impl EngineCapabilities {
    #[wasm_bindgen(getter)]
    pub fn simd(&self) -> bool {
        self.simd
    }

    #[wasm_bindgen(getter)]
    pub fn threads(&self) -> bool {
        self.threads
    }
}

/// Instance-local handle table. No request or document state is process-global.
#[wasm_bindgen]
pub struct WasmpptEngine {
    next_handle: u32,
    templates: HashMap<u32, Arc<PreparedRecord>>,
    presentations: HashMap<u32, PresentationDocument>,
    live_sessions: HashMap<u32, LiveSessionRecord>,
    generations: HashMap<u32, GenerationCursor>,
}

#[derive(Debug)]
struct PreparedRecord {
    template: Arc<PreparedTemplate>,
    diagnostics: Vec<BindingDiagnostic>,
}

#[derive(Debug)]
struct LiveSessionRecord {
    session: LiveSession,
    document: PresentationDocument,
    scenes: SceneCache,
}

#[derive(Debug)]
struct SceneCache {
    maximum_bytes: usize,
    resident_bytes: usize,
    peak_bytes: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
    entries: HashMap<(u32, [u8; 32]), Vec<u8>>,
    order: VecDeque<(u32, [u8; 32])>,
}

impl Default for WasmpptEngine {
    fn default() -> Self {
        Self {
            next_handle: 1,
            templates: HashMap::new(),
            presentations: HashMap::new(),
            live_sessions: HashMap::new(),
            generations: HashMap::new(),
        }
    }
}

#[wasm_bindgen]
impl WasmpptEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Report optional acceleration detected by the JavaScript adapter.
    ///
    /// The current baseline artifact intentionally reports scalar-only support.
    pub fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            simd: false,
            threads: false,
        }
    }

    /// Compile an immutable template and return an opaque instance-local handle.
    pub fn prepare(&mut self, template: &[u8]) -> Result<u32, JsValue> {
        self.prepare_default(template)
    }

    /// Compile with explicit stable v2 option tags.
    pub fn prepare_with_options(
        &mut self,
        template: &[u8],
        macro_policy: u8,
        allow_visible_tokens: bool,
    ) -> Result<u32, JsValue> {
        let options = CompilerOptions {
            macro_policy: decode_macro_policy(macro_policy)?,
            allow_visible_tokens,
        };
        self.compile_template(template, options)
    }

    /// Restore a previously compiled plan after verifying its source identity.
    pub fn prepare_with_plan(&mut self, template: &[u8], plan: &[u8]) -> Result<u32, JsValue> {
        let bytes: Arc<[u8]> = template.to_vec().into();
        let plan =
            TemplatePlan::decode(plan).map_err(|error| coded_error("WasmpptPlanError", error))?;
        let prepared = PreparedTemplate::new(bytes, plan).map_err(generate_error)?;
        let handle = self.allocate_handle()?;
        self.templates.insert(
            handle,
            Arc::new(PreparedRecord {
                template: Arc::new(prepared),
                diagnostics: Vec::new(),
            }),
        );
        Ok(handle)
    }

    pub fn prepared_weight(&self, handle: u32) -> Result<u64, JsValue> {
        Ok(self.template(handle)?.estimated_resident_bytes())
    }

    pub fn prepared_plan(&self, handle: u32) -> Result<Vec<u8>, JsValue> {
        Ok(self.template_record(handle)?.template.plan().encode())
    }

    /// Return compact binding tuples: id, kind, part, source, shape ID, shape name.
    pub fn prepared_bindings(&self, handle: u32) -> Result<Array, JsValue> {
        let output = Array::new();
        for binding in &self.template_record(handle)?.template.plan().bindings {
            let row = Array::new();
            row.push(&binding.id.clone().into());
            row.push(&binding_kind_name(binding.kind).into());
            row.push(&binding.part_name.clone().into());
            row.push(&binding_source_name(binding.source).into());
            row.push(&binding.shape_id.map(JsValue::from).unwrap_or(JsValue::NULL));
            row.push(
                &binding
                    .shape_name
                    .clone()
                    .map(JsValue::from)
                    .unwrap_or(JsValue::NULL),
            );
            output.push(&row);
        }
        Ok(output)
    }

    /// Return compact diagnostic tuples: code, binding ID, part, message.
    pub fn prepared_diagnostics(&self, handle: u32) -> Result<Array, JsValue> {
        let output = Array::new();
        for diagnostic in &self.template_record(handle)?.diagnostics {
            let row = Array::new();
            row.push(&binding_diagnostic_name(diagnostic.code).into());
            row.push(
                &diagnostic
                    .binding_id
                    .clone()
                    .map(JsValue::from)
                    .unwrap_or(JsValue::NULL),
            );
            row.push(
                &diagnostic
                    .part_name
                    .clone()
                    .map(JsValue::from)
                    .unwrap_or(JsValue::NULL),
            );
            row.push(&diagnostic.message.clone().into());
            output.push(&row);
        }
        Ok(output)
    }

    /// Index a presentation once and retain its compressed package behind an opaque handle.
    pub fn open_presentation(&mut self, presentation: &[u8]) -> Result<u32, JsValue> {
        let deck = PresentationDocument::open(presentation.to_vec()).map_err(layout_error)?;
        let handle = self.allocate_handle()?;
        self.presentations.insert(handle, deck);
        Ok(handle)
    }

    pub fn presentation_slide_count(&self, handle: u32) -> Result<u32, JsValue> {
        u32::try_from(self.presentation(handle)?.slide_count())
            .map_err(|_| coded_error("WasmpptLimitError", "slide count exceeds u32"))
    }

    /// Resolve exactly one requested slide to the compact display-list wire format.
    pub fn resolve_presentation_slide(
        &self,
        presentation_handle: u32,
        slide_index: u32,
    ) -> Result<Vec<u8>, JsValue> {
        let resolved = self
            .presentation(presentation_handle)?
            .resolve_slide(slide_index as usize)
            .map_err(layout_error)?;
        Ok(DisplayList::from_resolve(&resolved).encode())
    }

    /// Read one display-list resource without eagerly decoding unrelated media.
    pub fn presentation_resource(
        &self,
        presentation_handle: u32,
        part_name: &str,
    ) -> Result<Vec<u8>, JsValue> {
        self.presentation(presentation_handle)?
            .read_part(part_name)
            .map_err(layout_error)
    }

    /// Create a revision-zero live session from one prepared template and complete
    /// initial generation data. The logical package is opened directly, without a
    /// generated PPTX buffer.
    pub fn create_live_session_payload(
        &mut self,
        template_handle: u32,
        payload: &[u8],
    ) -> Result<u32, JsValue> {
        let data = InjectionData::decode(payload)
            .map_err(|error| coded_error("WasmpptPayloadError", error))?;
        let prepared = self.template_record(template_handle)?.template.clone();
        let session = prepared.start_live_session(data).map_err(generate_error)?;
        let document =
            PresentationDocument::open_source(session.overlay()).map_err(layout_error)?;
        let handle = self.allocate_handle()?;
        self.live_sessions.insert(
            handle,
            LiveSessionRecord {
                session,
                document,
                scenes: SceneCache::new(SESSION_SCENE_CACHE_BYTES),
            },
        );
        Ok(handle)
    }

    pub fn live_session_revision(&self, handle: u32) -> Result<u32, JsValue> {
        Ok(self.live_session(handle)?.session.revision())
    }

    pub fn live_session_slide_count(&self, handle: u32) -> Result<u32, JsValue> {
        u32::try_from(self.live_session(handle)?.document.slide_count())
            .map_err(|_| coded_error("WasmpptLimitError", "slide count exceeds u32"))
    }

    /// Atomically apply a partial WPPD payload and return compact revision metadata.
    pub fn apply_live_session_payload(
        &mut self,
        handle: u32,
        expected_revision: u32,
        next_revision: u32,
        payload: &[u8],
    ) -> Result<Array, JsValue> {
        let delta = InjectionData::decode(payload)
            .map_err(|error| coded_error("WasmpptPayloadError", error))?;
        let record = self.live_session_mut(handle)?;
        let old_count = record.document.slide_count();
        let old_slide_parts = record
            .document
            .slide_part_names()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut candidate_document = None;
        let update = record
            .session
            .apply_delta_validated(
                expected_revision,
                next_revision,
                delta,
                |source, changed| {
                    let document = if changed {
                        PresentationDocument::open_source(source)
                            .map_err(|error| error.to_string())?
                    } else {
                        record.document.with_compatible_source(source)
                    };
                    candidate_document = Some(document);
                    Ok(())
                },
            )
            .map_err(generate_error)?;
        let old_invalidated = record
            .document
            .invalidated_slides_for_parts(update.changed_parts.iter().map(String::as_str));
        let next_document =
            candidate_document.expect("successful live overlay validation produces a document");
        let new_count = next_document.slide_count();
        let mut invalidated = old_invalidated;
        invalidated.extend(
            next_document
                .invalidated_slides_for_parts(update.changed_parts.iter().map(String::as_str)),
        );
        invalidated.sort_unstable();
        invalidated.dedup();
        let full_fallback = old_count != new_count
            || old_slide_parts
                != next_document
                    .slide_part_names()
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
        if full_fallback {
            invalidated = (0..new_count).collect();
        }
        record.document = next_document;
        Ok(live_update_array(
            &update,
            full_fallback,
            new_count,
            &invalidated,
        ))
    }

    pub fn resolve_live_session_slide(
        &mut self,
        handle: u32,
        revision: u32,
        slide_index: u32,
    ) -> Result<Vec<u8>, JsValue> {
        let record = self.live_session_mut(handle)?;
        require_revision(record, revision)?;
        let fingerprint = record
            .document
            .slide_dependency_fingerprint(slide_index as usize)
            .map_err(layout_error)?;
        if let Some(bytes) = record.scenes.get((slide_index, fingerprint)) {
            return Ok(bytes);
        }
        let resolved = record
            .document
            .resolve_slide(slide_index as usize)
            .map_err(layout_error)?;
        let bytes = DisplayList::from_resolve(&resolved).encode();
        record
            .scenes
            .insert((slide_index, fingerprint), bytes.clone());
        Ok(bytes)
    }

    pub fn live_session_slide_fingerprint(
        &self,
        handle: u32,
        revision: u32,
        slide_index: u32,
    ) -> Result<String, JsValue> {
        let record = self.live_session(handle)?;
        require_revision(record, revision)?;
        record
            .document
            .slide_dependency_fingerprint(slide_index as usize)
            .map(fingerprint_hex)
            .map_err(layout_error)
    }

    pub fn live_session_resource(
        &self,
        handle: u32,
        revision: u32,
        part_name: &str,
    ) -> Result<Vec<u8>, JsValue> {
        let record = self.live_session(handle)?;
        require_revision(record, revision)?;
        record.document.read_part(part_name).map_err(layout_error)
    }

    pub fn live_session_resource_fingerprint(
        &self,
        handle: u32,
        revision: u32,
        part_name: &str,
    ) -> Result<String, JsValue> {
        let record = self.live_session(handle)?;
        require_revision(record, revision)?;
        record
            .document
            .part_fingerprint(part_name)
            .map(fingerprint_hex)
            .map_err(layout_error)
    }

    pub fn start_live_session_generation(
        &mut self,
        handle: u32,
        revision: u32,
    ) -> Result<u32, JsValue> {
        let cursor = {
            let record = self.live_session(handle)?;
            require_revision(record, revision)?;
            record.session.generation_cursor()
        };
        let generation_handle = self.allocate_handle()?;
        self.generations.insert(generation_handle, cursor);
        Ok(generation_handle)
    }

    pub fn live_session_cache_telemetry(&self, handle: u32) -> Result<Array, JsValue> {
        let cache = &self.live_session(handle)?.scenes;
        let output = Array::new();
        for value in [
            cache.resident_bytes as u64,
            cache.peak_bytes as u64,
            cache.entries.len() as u64,
            cache.hits,
            cache.misses,
            cache.evictions,
        ] {
            output.push(&JsValue::from_f64(value as f64));
        }
        Ok(output)
    }

    /// Text-only compatibility entry point returning a pull cursor handle.
    pub fn generate_text(
        &mut self,
        template_handle: u32,
        ids: &Array,
        values: &Array,
    ) -> Result<u32, JsError> {
        if ids.length() != values.length() {
            return Err(JsError::new(
                "binding IDs and values have different lengths",
            ));
        }
        let mut data = InjectionData::new();
        for index in 0..ids.length() {
            let id = ids
                .get(index)
                .as_string()
                .ok_or_else(|| JsError::new("binding ID is not a string"))?;
            let value = values
                .get(index)
                .as_string()
                .ok_or_else(|| JsError::new("binding value is not a string"))?;
            data.insert_text(id, value);
        }
        self.start_generation(template_handle, &data)
            .map_err(js_value_as_js_error)
    }

    /// Generate from the versioned binary structured-injection payload.
    pub fn start_generation_payload(
        &mut self,
        template_handle: u32,
        payload: &[u8],
    ) -> Result<u32, JsValue> {
        let data = InjectionData::decode(payload)
            .map_err(|error| coded_error("WasmpptPayloadError", error))?;
        self.start_generation(template_handle, &data)
    }

    pub fn generation_pull(
        &mut self,
        generation_handle: u32,
        maximum_bytes: u32,
    ) -> Result<Vec<u8>, JsValue> {
        self.generations
            .get_mut(&generation_handle)
            .ok_or_else(|| coded_error("WasmpptHandleError", "unknown generation handle"))?
            .pull(maximum_bytes as usize)
            .map_err(generate_error)
    }

    pub fn generation_done(&self, generation_handle: u32) -> Result<bool, JsValue> {
        Ok(self
            .generations
            .get(&generation_handle)
            .ok_or_else(|| coded_error("WasmpptHandleError", "unknown generation handle"))?
            .is_done())
    }

    pub fn release_template(&mut self, handle: u32) -> bool {
        self.templates.remove(&handle).is_some()
    }

    pub fn release_presentation(&mut self, handle: u32) -> bool {
        self.presentations.remove(&handle).is_some()
    }

    pub fn release_live_session(&mut self, handle: u32) -> bool {
        self.live_sessions.remove(&handle).is_some()
    }

    pub fn release_generation(&mut self, handle: u32) -> bool {
        self.generations.remove(&handle).is_some()
    }
}

impl WasmpptEngine {
    fn allocate_handle(&mut self) -> Result<u32, JsValue> {
        for _ in 0..u32::MAX {
            let handle = self.next_handle;
            self.next_handle = self.next_handle.wrapping_add(1).max(1);
            if !self.templates.contains_key(&handle)
                && !self.presentations.contains_key(&handle)
                && !self.live_sessions.contains_key(&handle)
                && !self.generations.contains_key(&handle)
            {
                return Ok(handle);
            }
        }
        Err(coded_error(
            "WasmpptHandleSpaceError",
            "opaque handle space is exhausted",
        ))
    }

    fn template(&self, handle: u32) -> Result<&PreparedTemplate, JsValue> {
        self.templates
            .get(&handle)
            .map(|record| record.template.as_ref())
            .ok_or_else(|| coded_error("WasmpptHandleError", "unknown template handle"))
    }

    fn template_record(&self, handle: u32) -> Result<&PreparedRecord, JsValue> {
        self.templates
            .get(&handle)
            .map(Arc::as_ref)
            .ok_or_else(|| coded_error("WasmpptHandleError", "unknown template handle"))
    }

    fn prepare_default(&mut self, template: &[u8]) -> Result<u32, JsValue> {
        self.compile_template(template, CompilerOptions::default())
    }

    fn compile_template(
        &mut self,
        template: &[u8],
        options: CompilerOptions,
    ) -> Result<u32, JsValue> {
        let bytes: Arc<[u8]> = template.to_vec().into();
        let archive = ZipArchive::from_bytes(bytes.clone()).map_err(opc_error)?;
        let compiled = TemplateCompiler::new(options)
            .compile(&archive)
            .map_err(compile_error)?;
        let prepared = PreparedTemplate::new(bytes, compiled.plan).map_err(generate_error)?;
        let handle = self.allocate_handle()?;
        self.templates.insert(
            handle,
            Arc::new(PreparedRecord {
                template: Arc::new(prepared),
                diagnostics: compiled.diagnostics,
            }),
        );
        Ok(handle)
    }

    fn start_generation(
        &mut self,
        template_handle: u32,
        data: &InjectionData,
    ) -> Result<u32, JsValue> {
        let cursor = self
            .template_record(template_handle)?
            .template
            .generate_cursor(data)
            .map_err(generate_error)?;
        let handle = self.allocate_handle()?;
        self.generations.insert(handle, cursor);
        Ok(handle)
    }

    fn presentation(&self, handle: u32) -> Result<&PresentationDocument, JsValue> {
        self.presentations
            .get(&handle)
            .ok_or_else(|| coded_error("WasmpptHandleError", "unknown presentation handle"))
    }

    fn live_session(&self, handle: u32) -> Result<&LiveSessionRecord, JsValue> {
        self.live_sessions
            .get(&handle)
            .ok_or_else(|| coded_error("WasmpptHandleError", "unknown live session handle"))
    }

    fn live_session_mut(&mut self, handle: u32) -> Result<&mut LiveSessionRecord, JsValue> {
        self.live_sessions
            .get_mut(&handle)
            .ok_or_else(|| coded_error("WasmpptHandleError", "unknown live session handle"))
    }
}

impl SceneCache {
    fn new(maximum_bytes: usize) -> Self {
        Self {
            maximum_bytes,
            resident_bytes: 0,
            peak_bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, key: (u32, [u8; 32])) -> Option<Vec<u8>> {
        let value = self.entries.get(&key).cloned();
        if value.is_some() {
            self.hits = self.hits.saturating_add(1);
            self.order.retain(|candidate| candidate != &key);
            self.order.push_back(key);
        } else {
            self.misses = self.misses.saturating_add(1);
        }
        value
    }

    fn insert(&mut self, key: (u32, [u8; 32]), bytes: Vec<u8>) {
        if bytes.len() > self.maximum_bytes {
            return;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.resident_bytes = self.resident_bytes.saturating_sub(previous.len());
        }
        self.order.retain(|candidate| candidate != &key);
        self.resident_bytes = self.resident_bytes.saturating_add(bytes.len());
        self.entries.insert(key, bytes);
        self.order.push_back(key);
        self.peak_bytes = self.peak_bytes.max(self.resident_bytes);
        while self.resident_bytes > self.maximum_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.resident_bytes = self.resident_bytes.saturating_sub(removed.len());
                self.evictions = self.evictions.saturating_add(1);
            }
        }
    }
}

fn require_revision(record: &LiveSessionRecord, revision: u32) -> Result<(), JsValue> {
    if record.session.revision() == revision {
        Ok(())
    } else {
        Err(coded_error(
            "WasmpptRevisionError",
            format!(
                "requested live revision {revision}, current revision is {}",
                record.session.revision()
            ),
        ))
    }
}

fn fingerprint_hex(fingerprint: [u8; 32]) -> String {
    fingerprint
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn live_update_array(
    update: &LiveSessionUpdate,
    full_fallback: bool,
    slide_count: usize,
    invalidated: &[usize],
) -> Array {
    let output = Array::new();
    output.push(&JsValue::from(update.revision));
    output.push(&JsValue::from(update.graph_changed));
    output.push(&JsValue::from(full_fallback));
    output.push(&JsValue::from(if full_fallback {
        "topology"
    } else if invalidated.is_empty() {
        "none"
    } else {
        "dependency"
    }));
    output.push(&JsValue::from(slide_count as u32));
    let invalidated_rows = Array::new();
    for index in invalidated {
        invalidated_rows.push(&JsValue::from(*index as u32));
    }
    output.push(&invalidated_rows);
    let changed_bindings = Array::new();
    for id in &update.changed_bindings {
        changed_bindings.push(&JsValue::from(id.clone()));
    }
    output.push(&changed_bindings);
    let changed_parts = Array::new();
    for name in &update.changed_parts {
        changed_parts.push(&JsValue::from(name.clone()));
    }
    output.push(&changed_parts);
    output.push(&JsValue::from_f64(update.reused_materialized_parts as f64));
    for value in [
        update.overlay_stats.logical_parts,
        update.overlay_stats.materialized_parts,
        update.overlay_stats.materialized_bytes,
        update.overlay_stats.reused_source_bytes,
        update.overlay_stats.removed_parts,
    ] {
        output.push(&JsValue::from_f64(value as f64));
    }
    output
}

fn coded_error(name: &str, error: impl std::fmt::Display) -> JsValue {
    let (domain, code) = match name {
        "WasmpptPayloadError" => ("payload", "invalid-payload"),
        "WasmpptPlanError" => ("template", "invalid-plan"),
        "WasmpptOptionError" => ("runtime", "invalid-option"),
        "WasmpptHandleError" => ("runtime", "unknown-handle"),
        "WasmpptHandleSpaceError" => ("runtime", "handle-space-exhausted"),
        "WasmpptRevisionError" => ("runtime", "stale-revision"),
        "WasmpptLimitError" => ("runtime", "limit-exceeded"),
        _ => ("runtime", "internal"),
    };
    envelope_error(
        name,
        domain,
        code,
        error.to_string(),
        ErrorContext::default(),
    )
}

fn opc_error(error: OpcError) -> JsValue {
    envelope_error(
        "WasmpptPackageError",
        "package",
        opc_error_code(error.code()),
        error.to_string(),
        ErrorContext::default(),
    )
}

fn compile_error(error: CompileError) -> JsValue {
    envelope_error(
        compile_error_name(error.code()),
        "template",
        compile_error_code(error.code()),
        error.to_string(),
        ErrorContext {
            cause_code: error.cause_code(),
            ..ErrorContext::default()
        },
    )
}

fn generate_error(error: GenerateError) -> JsValue {
    let domain = if error.code() == GenerateErrorCode::InvalidRevision {
        "runtime"
    } else {
        "generation"
    };
    envelope_error(
        generate_error_name(error.code()),
        domain,
        generate_error_code(error.code()),
        error.to_string(),
        ErrorContext {
            cause_code: error.cause_code(),
            ..ErrorContext::default()
        },
    )
}

fn layout_error(error: LayoutError) -> JsValue {
    envelope_error(
        "WasmpptLayoutError",
        "layout",
        layout_error_code(error.code()),
        error.to_string(),
        ErrorContext {
            cause_code: error.cause_code(),
            part_name: error.part_name(),
            offset: error.offset(),
            slide_index: error.slide_index(),
        },
    )
}

#[derive(Default)]
struct ErrorContext<'a> {
    cause_code: Option<&'a str>,
    part_name: Option<&'a str>,
    offset: Option<usize>,
    slide_index: Option<usize>,
}

fn envelope_error(
    name: &str,
    domain: &str,
    code: &str,
    message: String,
    context: ErrorContext<'_>,
) -> JsValue {
    let js_error = JavaScriptError::new(&message);
    js_error.set_name(name);
    let envelope = Object::new();
    set_property(&envelope, "version", &JsValue::from(1));
    set_property(&envelope, "domain", &JsValue::from(domain));
    set_property(&envelope, "code", &JsValue::from(code));
    set_property(&envelope, "message", &JsValue::from(message));
    if let Some(value) = context.cause_code {
        set_property(&envelope, "causeCode", &JsValue::from(value));
    }
    if let Some(value) = context.part_name {
        set_property(&envelope, "partName", &JsValue::from(value));
    }
    if let Some(value) = context.offset {
        set_property(&envelope, "offset", &JsValue::from_f64(value as f64));
    }
    if let Some(value) = context.slide_index {
        set_property(&envelope, "slideIndex", &JsValue::from_f64(value as f64));
    }
    set_property(js_error.as_ref(), "wasmppt", envelope.as_ref());
    js_error.into()
}

fn set_property(target: &JsValue, name: &str, value: &JsValue) {
    let _ = Reflect::set(target, &JsValue::from(name), value);
}

fn js_value_as_js_error(error: JsValue) -> JsError {
    let message = error
        .dyn_ref::<JavaScriptError>()
        .and_then(|error| error.message().as_string())
        .unwrap_or_else(|| "wasmppt preparation failed".to_owned());
    JsError::new(&message)
}

fn decode_macro_policy(value: u8) -> Result<MacroPolicy, JsValue> {
    match value {
        0 => Ok(MacroPolicy::Strip),
        1 => Ok(MacroPolicy::Reject),
        _ => Err(coded_error(
            "WasmpptOptionError",
            "invalid macro policy tag",
        )),
    }
}

const fn compile_error_name(value: CompileErrorCode) -> &'static str {
    match value {
        CompileErrorCode::InvalidTemplate => "WasmpptCompileError",
        CompileErrorCode::MacroPresent => "WasmpptMacroPresentError",
        _ => "WasmpptCompileError",
    }
}

const fn compile_error_code(value: CompileErrorCode) -> &'static str {
    match value {
        CompileErrorCode::InvalidTemplate => "invalid-template",
        CompileErrorCode::MacroPresent => "macro-present",
        _ => "unknown",
    }
}

const fn layout_error_code(value: LayoutErrorCode) -> &'static str {
    match value {
        LayoutErrorCode::Package => "invalid-package",
        LayoutErrorCode::InvalidPresentation => "invalid-presentation",
        LayoutErrorCode::MissingPresentation => "missing-presentation",
        LayoutErrorCode::InvalidSlide => "invalid-slide",
        LayoutErrorCode::LimitExceeded => "limit-exceeded",
        _ => "unknown",
    }
}

const fn opc_error_code(value: OpcErrorCode) -> &'static str {
    match value {
        OpcErrorCode::Io => "io",
        OpcErrorCode::Truncated => "truncated",
        OpcErrorCode::InvalidSignature => "invalid-signature",
        OpcErrorCode::InvalidField => "invalid-field",
        OpcErrorCode::InvalidPath => "invalid-path",
        OpcErrorCode::DuplicateEntry => "duplicate-entry",
        OpcErrorCode::UnsupportedCompression => "unsupported-compression",
        OpcErrorCode::UnsupportedEncryption => "unsupported-encryption",
        OpcErrorCode::UnsupportedMultiDisk => "unsupported-multi-disk",
        OpcErrorCode::UnsupportedZip64 => "unsupported-zip64",
        OpcErrorCode::LimitExceeded => "limit-exceeded",
        OpcErrorCode::OverlappingEntries => "overlapping-entries",
        OpcErrorCode::ChecksumMismatch => "checksum-mismatch",
        OpcErrorCode::SizeMismatch => "size-mismatch",
        _ => "unknown",
    }
}

const fn binding_kind_name(value: BindingKind) -> &'static str {
    match value {
        BindingKind::Text => "text",
        BindingKind::Image => "image",
        BindingKind::Chart => "chart",
    }
}

const fn binding_source_name(value: BindingSource) -> &'static str {
    match value {
        BindingSource::VisibleToken => "visible-token",
        BindingSource::ShapeMetadata => "shape-metadata",
        BindingSource::Manifest => "manifest",
    }
}

const fn binding_diagnostic_name(value: BindingDiagnosticCode) -> &'static str {
    match value {
        BindingDiagnosticCode::MissingTarget => "missing-target",
        BindingDiagnosticCode::DuplicateId => "duplicate-id",
        BindingDiagnosticCode::AmbiguousTarget => "ambiguous-target",
        BindingDiagnosticCode::UnsupportedKind => "unsupported-kind",
        BindingDiagnosticCode::InvalidManifest => "invalid-manifest",
        BindingDiagnosticCode::InvalidSlide => "invalid-slide",
        _ => "unknown",
    }
}

const fn generate_error_name(value: GenerateErrorCode) -> &'static str {
    match value {
        GenerateErrorCode::InvalidTemplate => "WasmpptInvalidTemplateError",
        GenerateErrorCode::IncompletePlan => "WasmpptIncompletePlanError",
        GenerateErrorCode::PlanMismatch => "WasmpptPlanMismatchError",
        GenerateErrorCode::MissingValue => "WasmpptMissingValueError",
        GenerateErrorCode::InvalidBindingRange => "WasmpptBindingRangeError",
        GenerateErrorCode::Package => "WasmpptPackageError",
        GenerateErrorCode::Xml => "WasmpptXmlError",
        GenerateErrorCode::InvalidImage => "WasmpptImageError",
        GenerateErrorCode::InvalidChart => "WasmpptChartError",
        GenerateErrorCode::InvalidTable => "WasmpptTableError",
        GenerateErrorCode::InvalidRevision => "WasmpptRevisionError",
        GenerateErrorCode::MacroPresent => "WasmpptMacroPresentError",
        _ => "WasmpptGenerateError",
    }
}

const fn generate_error_code(value: GenerateErrorCode) -> &'static str {
    match value {
        GenerateErrorCode::InvalidTemplate => "invalid-template",
        GenerateErrorCode::IncompletePlan => "incomplete-plan",
        GenerateErrorCode::PlanMismatch => "plan-mismatch",
        GenerateErrorCode::MissingValue => "missing-value",
        GenerateErrorCode::InvalidBindingRange => "invalid-binding-range",
        GenerateErrorCode::Package => "package",
        GenerateErrorCode::Xml => "xml",
        GenerateErrorCode::InvalidImage => "invalid-image",
        GenerateErrorCode::InvalidChart => "invalid-chart",
        GenerateErrorCode::InvalidTable => "invalid-table",
        GenerateErrorCode::InvalidRevision => "stale-revision",
        GenerateErrorCode::MacroPresent => "macro-present",
        _ => "unknown",
    }
}
