import init, { WasmpptEngine } from './wasm/wasmppt_wasm.js'
import { installWorkerRuntime } from './lib/worker-runtime.js'

try {
  await init({ module_or_path: new URL('./wasm/wasmppt_wasm_bg.wasm', self.location.href) })
  installWorkerRuntime(self, new WasmpptEngine())
  self.postMessage({ type: 'host-ready' })
} catch (error) {
  self.postMessage({
    type: 'host-init-error',
    message: error instanceof Error ? error.stack ?? error.message : String(error),
  })
  throw error
}
