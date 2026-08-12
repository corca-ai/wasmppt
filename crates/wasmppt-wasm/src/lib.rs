//! Narrow WebAssembly boundary for the host-agnostic `wasmppt` core.

use std::{collections::HashMap, sync::Arc};

use js_sys::{Array, Error as JavaScriptError};
use wasm_bindgen::prelude::*;
use wasmppt_display::DisplayList;
use wasmppt_layout::PresentationDocument;
use wasmppt_opc::ZipArchive;
use wasmppt_template::{
    BindingDiagnostic, BindingDiagnosticCode, BindingKind, BindingSource, CompatibilityProfile,
    CompilerOptions, CompressionProfile, GenerateErrorCode, GenerationCursor, InjectionData,
    MacroPolicy, PreparedTemplate, TemplateCompiler, TemplatePlan,
};

/// Returns the engine package version embedded in the Wasm module.
#[wasm_bindgen]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Resolve one slide to the compact backend-neutral display-list wire format.
#[wasm_bindgen]
pub fn resolve_display_list(presentation: &[u8], slide_index: u32) -> Result<Vec<u8>, JsError> {
    let deck = PresentationDocument::open(presentation.to_vec()).map_err(js_error)?;
    let resolved = deck.resolve_slide(slide_index as usize).map_err(js_error)?;
    Ok(DisplayList::from_resolve(&resolved).encode())
}

/// Stable signature used to compare native and Wasm display-list structure.
#[wasm_bindgen]
pub fn display_list_signature(presentation: &[u8], slide_index: u32) -> Result<String, JsError> {
    let deck = PresentationDocument::open(presentation.to_vec()).map_err(js_error)?;
    let resolved = deck.resolve_slide(slide_index as usize).map_err(js_error)?;
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
    templates: HashMap<u32, PreparedRecord>,
    presentations: HashMap<u32, PresentationDocument>,
    generations: HashMap<u32, GenerationCursor>,
}

#[derive(Debug)]
struct PreparedRecord {
    template: PreparedTemplate,
    diagnostics: Vec<BindingDiagnostic>,
}

impl Default for WasmpptEngine {
    fn default() -> Self {
        Self {
            next_handle: 1,
            templates: HashMap::new(),
            presentations: HashMap::new(),
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
    pub fn prepare(&mut self, template: &[u8]) -> Result<u32, JsError> {
        self.prepare_default(template).map_err(js_value_as_js_error)
    }

    /// Compile with explicit stable v1 option tags.
    pub fn prepare_with_options(
        &mut self,
        template: &[u8],
        macro_policy: u8,
        compatibility: u8,
        compression: u8,
        allow_visible_tokens: bool,
    ) -> Result<u32, JsValue> {
        let options = CompilerOptions {
            macro_policy: decode_macro_policy(macro_policy)?,
            compatibility: decode_compatibility(compatibility)?,
            compression: decode_compression(compression)?,
            allow_visible_tokens,
        };
        self.compile_template(template, options)
    }

    /// Restore a previously compiled plan after verifying its source identity.
    pub fn prepare_with_plan(&mut self, template: &[u8], plan: &[u8]) -> Result<u32, JsValue> {
        let bytes: Arc<[u8]> = template.to_vec().into();
        let plan =
            TemplatePlan::decode(plan).map_err(|error| coded_error("WasmpptPlanError", error))?;
        let prepared = PreparedTemplate::new(bytes, plan)
            .map_err(|error| coded_error(generate_error_name(error.code()), error))?;
        let handle = self.allocate_handle()?;
        self.templates.insert(
            handle,
            PreparedRecord {
                template: prepared,
                diagnostics: Vec::new(),
            },
        );
        Ok(handle)
    }

    pub fn prepared_weight(&self, handle: u32) -> Result<u64, JsError> {
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
    pub fn open_presentation(&mut self, presentation: &[u8]) -> Result<u32, JsError> {
        let deck = PresentationDocument::open(presentation.to_vec()).map_err(js_error)?;
        let handle = self.allocate_handle()?;
        self.presentations.insert(handle, deck);
        Ok(handle)
    }

    pub fn presentation_slide_count(&self, handle: u32) -> Result<u32, JsError> {
        u32::try_from(self.presentation(handle)?.slide_count())
            .map_err(|_| JsError::new("slide count exceeds the Wasm 32-bit address space"))
    }

    /// Resolve exactly one requested slide to the compact display-list wire format.
    pub fn resolve_presentation_slide(
        &self,
        presentation_handle: u32,
        slide_index: u32,
    ) -> Result<Vec<u8>, JsError> {
        let resolved = self
            .presentation(presentation_handle)?
            .resolve_slide(slide_index as usize)
            .map_err(js_error)?;
        Ok(DisplayList::from_resolve(&resolved).encode())
    }

    /// Read one display-list resource without eagerly decoding unrelated media.
    pub fn presentation_resource(
        &self,
        presentation_handle: u32,
        part_name: &str,
    ) -> Result<Vec<u8>, JsError> {
        self.presentation(presentation_handle)?
            .read_part(part_name)
            .map_err(js_error)
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
            .map_err(|error| coded_error(generate_error_name(error.code()), error))
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

    pub fn release_generation(&mut self, handle: u32) -> bool {
        self.generations.remove(&handle).is_some()
    }
}

impl WasmpptEngine {
    fn allocate_handle(&mut self) -> Result<u32, JsError> {
        for _ in 0..u32::MAX {
            let handle = self.next_handle;
            self.next_handle = self.next_handle.wrapping_add(1).max(1);
            if !self.templates.contains_key(&handle)
                && !self.presentations.contains_key(&handle)
                && !self.generations.contains_key(&handle)
            {
                return Ok(handle);
            }
        }
        Err(JsError::new("opaque handle space is exhausted"))
    }

    fn template(&self, handle: u32) -> Result<&PreparedTemplate, JsError> {
        self.templates
            .get(&handle)
            .map(|record| &record.template)
            .ok_or_else(|| JsError::new("unknown template handle"))
    }

    fn template_record(&self, handle: u32) -> Result<&PreparedRecord, JsValue> {
        self.templates
            .get(&handle)
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
        let archive = ZipArchive::from_bytes(bytes.clone())
            .map_err(|error| coded_error("WasmpptPackageError", error))?;
        let compiled = TemplateCompiler::new(options)
            .compile(&archive)
            .map_err(|error| coded_error("WasmpptCompileError", error))?;
        let prepared = PreparedTemplate::new(bytes, compiled.plan)
            .map_err(|error| coded_error(generate_error_name(error.code()), error))?;
        let handle = self.allocate_handle()?;
        self.templates.insert(
            handle,
            PreparedRecord {
                template: prepared,
                diagnostics: compiled.diagnostics,
            },
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
            .map_err(|error| coded_error(generate_error_name(error.code()), error))?;
        let handle = self.allocate_handle()?;
        self.generations.insert(handle, cursor);
        Ok(handle)
    }

    fn presentation(&self, handle: u32) -> Result<&PresentationDocument, JsError> {
        self.presentations
            .get(&handle)
            .ok_or_else(|| JsError::new("unknown presentation handle"))
    }
}

fn js_error(error: impl std::fmt::Display) -> JsError {
    JsError::new(&error.to_string())
}

fn coded_error(name: &str, error: impl std::fmt::Display) -> JsValue {
    let js_error = JavaScriptError::new(&error.to_string());
    js_error.set_name(name);
    js_error.into()
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
        2 => Ok(MacroPolicy::PreserveAsPptm),
        _ => Err(coded_error(
            "WasmpptOptionError",
            "invalid macro policy tag",
        )),
    }
}

fn decode_compatibility(value: u8) -> Result<CompatibilityProfile, JsValue> {
    match value {
        0 => Ok(CompatibilityProfile::PowerPoint2016),
        1 => Ok(CompatibilityProfile::Microsoft365),
        _ => Err(coded_error(
            "WasmpptOptionError",
            "invalid compatibility profile tag",
        )),
    }
}

fn decode_compression(value: u8) -> Result<CompressionProfile, JsValue> {
    match value {
        0 => Ok(CompressionProfile::BalancedDeflate6),
        1 => Ok(CompressionProfile::StoreMedia),
        _ => Err(coded_error(
            "WasmpptOptionError",
            "invalid compression profile tag",
        )),
    }
}

const fn binding_kind_name(value: BindingKind) -> &'static str {
    match value {
        BindingKind::Text => "text",
        BindingKind::Image => "image",
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
        _ => "WasmpptGenerateError",
    }
}
