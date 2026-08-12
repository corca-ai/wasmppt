//! Narrow WebAssembly boundary for the host-agnostic `wasmppt` core.

use std::{collections::HashMap, sync::Arc};

use js_sys::Array;
use wasm_bindgen::prelude::*;
use wasmppt_display::DisplayList;
use wasmppt_layout::PresentationDocument;
use wasmppt_opc::ZipArchive;
use wasmppt_template::{InjectionData, PreparedTemplate, TemplateCompiler};

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
    Ok(DisplayList::from_slide(&resolved.slide).encode())
}

/// Stable signature used to compare native and Wasm display-list structure.
#[wasm_bindgen]
pub fn display_list_signature(presentation: &[u8], slide_index: u32) -> Result<String, JsError> {
    let deck = PresentationDocument::open(presentation.to_vec()).map_err(js_error)?;
    let resolved = deck.resolve_slide(slide_index as usize).map_err(js_error)?;
    Ok(format!(
        "{:016x}",
        DisplayList::from_slide(&resolved.slide).structural_signature()
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
    templates: HashMap<u32, PreparedTemplate>,
    outputs: HashMap<u32, Vec<u8>>,
}

impl Default for WasmpptEngine {
    fn default() -> Self {
        Self {
            next_handle: 1,
            templates: HashMap::new(),
            outputs: HashMap::new(),
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
        let bytes: Arc<[u8]> = template.to_vec().into();
        let archive = ZipArchive::from_bytes(bytes.clone()).map_err(js_error)?;
        let compiled = TemplateCompiler::new(Default::default())
            .compile(&archive)
            .map_err(js_error)?;
        let prepared = PreparedTemplate::new(bytes, compiled.plan).map_err(js_error)?;
        let handle = self.allocate_handle()?;
        self.templates.insert(handle, prepared);
        Ok(handle)
    }

    pub fn prepared_weight(&self, handle: u32) -> Result<u64, JsError> {
        Ok(self.template(handle)?.estimated_resident_bytes())
    }

    /// Generate into an engine-owned output buffer and return an opaque handle.
    /// Hosts drain that buffer in bounded transferable chunks.
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
        let bytes = self
            .template(template_handle)?
            .generate(&data)
            .map_err(js_error)?
            .bytes;
        let output_handle = self.allocate_handle()?;
        self.outputs.insert(output_handle, bytes);
        Ok(output_handle)
    }

    pub fn output_len(&self, output_handle: u32) -> Result<u32, JsError> {
        let length = self.output(output_handle)?.len();
        u32::try_from(length)
            .map_err(|_| JsError::new("output exceeds the Wasm 32-bit address space"))
    }

    /// Copy one bounded chunk into a JavaScript `Uint8Array`.
    pub fn output_chunk(
        &self,
        output_handle: u32,
        offset: u32,
        length: u32,
    ) -> Result<Vec<u8>, JsError> {
        let output = self.output(output_handle)?;
        let start = offset as usize;
        let end = start
            .checked_add(length as usize)
            .ok_or_else(|| JsError::new("output chunk range overflows"))?;
        let bytes = output
            .get(start..end)
            .ok_or_else(|| JsError::new("output chunk range is out of bounds"))?;
        Ok(bytes.to_vec())
    }

    pub fn release_template(&mut self, handle: u32) -> bool {
        self.templates.remove(&handle).is_some()
    }

    pub fn release_output(&mut self, handle: u32) -> bool {
        self.outputs.remove(&handle).is_some()
    }
}

impl WasmpptEngine {
    fn allocate_handle(&mut self) -> Result<u32, JsError> {
        for _ in 0..u32::MAX {
            let handle = self.next_handle;
            self.next_handle = self.next_handle.wrapping_add(1).max(1);
            if !self.templates.contains_key(&handle) && !self.outputs.contains_key(&handle) {
                return Ok(handle);
            }
        }
        Err(JsError::new("opaque handle space is exhausted"))
    }

    fn template(&self, handle: u32) -> Result<&PreparedTemplate, JsError> {
        self.templates
            .get(&handle)
            .ok_or_else(|| JsError::new("unknown template handle"))
    }

    fn output(&self, handle: u32) -> Result<&[u8], JsError> {
        self.outputs
            .get(&handle)
            .map(Vec::as_slice)
            .ok_or_else(|| JsError::new("unknown output handle"))
    }
}

fn js_error(error: impl std::fmt::Display) -> JsError {
    JsError::new(&error.to_string())
}
