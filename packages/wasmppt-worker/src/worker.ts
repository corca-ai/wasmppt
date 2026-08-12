import wasmModule from './generated/wasmppt_wasm_bg.wasm'
import { initSync, WasmpptEngine } from './generated/wasmppt_wasm.js'
import { createWasmpptWorker } from './index.js'

initSync({ module: wasmModule })

export default createWasmpptWorker(new WasmpptEngine())
