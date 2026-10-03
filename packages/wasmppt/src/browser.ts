import { connectWasmpptBrowserWorker } from './browser-worker-client.js'
import { initializeDeckEngine, type DeckEngine, type DeckEngineOptions } from './deck/engine.js'

export type * from './deck.js'
export { CanvasView, exportHtml } from './deck/browser.js'
export type { CanvasViewOptions, DeckHtmlOptions } from './deck/browser.js'
export type { OfflineHtmlResult } from './offline.js'

export async function createBrowserEngine(
  options: DeckEngineOptions & { readonly worker: Worker; readonly startupTimeoutMs?: number },
): Promise<DeckEngine> {
  return initializeDeckEngine(await connectWasmpptBrowserWorker(options.worker, options.startupTimeoutMs), options)
}
