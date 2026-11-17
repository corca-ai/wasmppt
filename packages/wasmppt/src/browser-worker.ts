import init, {
  WasmpptEngine,
} from '../../wasmppt-worker/src/generated/wasmppt_wasm.js'
import {
  type BrowserWorkerStartupMessage,
  WASMPPT_BROWSER_WORKER_ERROR,
  WASMPPT_BROWSER_WORKER_READY,
} from './browser-worker-client.js'
import {
  installWorkerRuntime,
  type WorkerRuntimeScope,
} from './worker-runtime.js'
import type { InlineTextMeasure } from './deck/inline-measure.js'

const scope = self as unknown as WorkerRuntimeScope
const startupScope = self as unknown as {
  postMessage(message: BrowserWorkerStartupMessage): void
}

let metafileModule:
  | Promise<
      typeof import('../../wasmppt-worker/src/generated/metafile/wasmppt_metafile_wasm.js')
    >
  | undefined

async function metafileToSvg(input: Uint8Array): Promise<Uint8Array> {
  metafileModule ??= import(
    '../../wasmppt-worker/src/generated/metafile/wasmppt_metafile_wasm.js'
  ).then(async (module) => {
    await module.default()
    return module
  })
  return (await metafileModule).convert_metafile_to_svg(input)
}

async function loadInlineTextMeasure(): Promise<InlineTextMeasure | undefined> {
  if (typeof OffscreenCanvas === 'undefined') return undefined
  const { createInlineTextMeasurer } = await import('./deck/inline-measure.js')
  const context = new OffscreenCanvas(1, 1).getContext('2d')
  return context === null ? undefined : createInlineTextMeasurer(context, {
    load: async () => {},
    check: (css, text) => (self as unknown as { readonly fonts: FontFaceSet }).fonts.check(css, text),
  })
}

try {
  await init()
  installWorkerRuntime(scope, new WasmpptEngine(), { metafileToSvg, loadInlineTextMeasure })
  startupScope.postMessage({ type: WASMPPT_BROWSER_WORKER_READY })
} catch (error) {
  const message = error instanceof Error ? error.message : String(error)
  startupScope.postMessage({ type: WASMPPT_BROWSER_WORKER_ERROR, message })
  throw error
}
