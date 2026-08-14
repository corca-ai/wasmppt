#!/usr/bin/env sh
set -eu

cargo build --profile wasm-release --locked --target wasm32-unknown-unknown \
  -p wasmppt-wasm -p wasmppt-metafile-wasm -p wasmppt-shaper-wasm
wasm-bindgen \
  --target web \
  --out-dir packages/wasmppt-worker/src/generated \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_wasm.wasm
cp tools/wasm-module.d.ts packages/wasmppt-worker/src/generated/wasmppt_wasm_bg.wasm.d.ts
mkdir -p packages/wasmppt-worker/src/generated/metafile
wasm-bindgen \
  --target web \
  --out-dir packages/wasmppt-worker/src/generated/metafile \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_metafile_wasm.wasm
cp tools/wasm-module.d.ts \
  packages/wasmppt-worker/src/generated/metafile/wasmppt_metafile_wasm_bg.wasm.d.ts
mkdir -p packages/wasmppt-worker/src/generated/shaper
wasm-bindgen \
  --target web \
  --out-dir packages/wasmppt-worker/src/generated/shaper \
  target/wasm32-unknown-unknown/wasm-release/wasmppt_shaper_wasm.wasm
cp tools/wasm-module.d.ts \
  packages/wasmppt-worker/src/generated/shaper/wasmppt_shaper_wasm_bg.wasm.d.ts
node scripts/check-wasm-artifact-manifest.mjs --write
