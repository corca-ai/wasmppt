import wasmModule from './generated/wasmppt_wasm_bg.wasm'
import { display_list_signature, initSync, WasmpptEngine } from './generated/wasmppt_wasm.js'
import { createWasmpptWorker } from './index.js'

initSync({ module: wasmModule })

const generation = createWasmpptWorker(new WasmpptEngine())

export default {
  async fetch(request, env, context): Promise<Response> {
    const url = new URL(request.url)
    if (url.pathname === '/v1/display-signature' && request.method === 'POST') {
      const bytes = new Uint8Array(await request.arrayBuffer())
      return Response.json({ signature: display_list_signature(bytes, 0) })
    }
    return generation.fetch!(request, env, context)
  },
} satisfies ExportedHandler<Env>
