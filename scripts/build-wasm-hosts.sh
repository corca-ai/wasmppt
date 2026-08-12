#!/usr/bin/env sh
set -eu

cargo build --profile wasm-release --locked --target wasm32-unknown-unknown -p wasmppt-wasm
wasm-bindgen \
  --target web \
  --out-dir packages/wasmppt-worker/src/generated \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_wasm.wasm
cp tools/wasm-module.d.ts packages/wasmppt-worker/src/generated/wasmppt_wasm_bg.wasm.d.ts
