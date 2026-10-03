import { readFile } from 'node:fs/promises'
import { WasmpptWorkerClient, type WorkerLike } from './worker-client.js'
import { installWorkerRuntime } from './worker-runtime.js'
import type { WorkerRequest, WorkerResponse } from './protocol.js'
import { initializeDeckEngine, type DeckEngine, type DeckEngineOptions } from './deck/engine.js'
import { initSync, WasmpptEngine } from '../../wasmppt-worker/src/generated/wasmppt_wasm.js'

export type * from './deck.js'

/** Headless Node adapter. Executes the same ABI and transport without DOM or browser globals. */
export async function createNodeEngine(options: DeckEngineOptions = {}): Promise<DeckEngine> {
  const bytes = await readFile(new URL('../../wasmppt-worker/src/generated/wasmppt_wasm_bg.wasm', import.meta.url))
  initSync({ module: bytes })
  const wasm = new WasmpptEngine()
  const incoming = new Set<(event: MessageEvent<unknown>) => void>()
  const outgoing = new Set<(event: MessageEvent<unknown>) => void>()
  let disposed = false
  const dispatch = (listeners: typeof incoming, data: WorkerRequest | WorkerResponse): void => {
    queueMicrotask(() => {
      if (!disposed) for (const listener of listeners) listener({ data } as MessageEvent<unknown>)
    })
  }
  const worker: WorkerLike = {
    postMessage: message => dispatch(incoming, message),
    addEventListener(type, listener) { if (type === 'message') outgoing.add(listener as (event: MessageEvent<unknown>) => void) },
    removeEventListener(type, listener) { if (type === 'message') outgoing.delete(listener as (event: MessageEvent<unknown>) => void) },
    terminate() { if (!disposed) { disposed = true; incoming.clear(); outgoing.clear(); wasm.free() } },
  }
  installWorkerRuntime({ postMessage: message => dispatch(outgoing, message), addEventListener: (_, listener) => incoming.add(listener) }, wasm)
  return initializeDeckEngine(new WasmpptWorkerClient(worker), options)
}
