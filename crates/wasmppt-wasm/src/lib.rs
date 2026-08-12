//! Narrow WebAssembly boundary for the host-agnostic `wasmppt` core.

use wasm_bindgen::prelude::*;

/// Returns the engine package version embedded in the Wasm module.
#[wasm_bindgen]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
