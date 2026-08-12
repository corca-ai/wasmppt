import init, { WasmpptEngine } from './wasm/wasmppt_wasm.js'
import { installWorkerRuntime } from './lib/worker-runtime.js'

let metafileModule

async function metafileToSvg(input) {
  metafileModule ??= import('./wasm/metafile/wasmppt_metafile_wasm.js').then(async (module) => {
    await module.default({
      module_or_path: new URL(
        './wasm/metafile/wasmppt_metafile_wasm_bg.wasm',
        self.location.href,
      ),
    })
    return module
  })
  return (await metafileModule).convert_metafile_to_svg(input)
}

try {
  await init({ module_or_path: new URL('./wasm/wasmppt_wasm_bg.wasm', self.location.href) })
  installWorkerRuntime(self, new WasmpptEngine(), { metafileToSvg })
  self.postMessage({ type: 'host-ready' })
} catch (error) {
  self.postMessage({
    type: 'host-init-error',
    message: error instanceof Error ? error.stack ?? error.message : String(error),
  })
  throw error
}
