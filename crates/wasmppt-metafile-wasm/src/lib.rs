//! Optional, independently loaded Wasm boundary for EMF/WMF previews.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn convert_metafile_to_svg(input: &[u8]) -> Result<Vec<u8>, JsError> {
    wasmppt_metafile::convert_to_svg(input).map_err(|error| JsError::new(&error.to_string()))
}
