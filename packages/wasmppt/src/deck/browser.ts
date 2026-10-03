import {
  CanvasDisplayListRenderer, FontResolver, decodeRasterImage, decodeSvgImage,
  hitTestDisplaySceneAtCanvasPoint, type DisplayScene, type RenderTelemetry,
} from '../canvas.js'
import { serializeOfflineHtmlDocument, ScopedFontLoadingHost, prepareInputFonts,
  type OfflineHtmlOptions, type OfflineHtmlResult, type OfflineDeckPage } from '../offline.js'
import { WasmFontShaper } from '../shaper.js'
import { accessSnapshot, requirePage, type DeckEngine, type DeckSnapshot } from './engine.js'

interface BrowserResources { readonly renderer: CanvasDisplayListRenderer; shaper?: Promise<WasmFontShaper> }
const resources = new WeakMap<DeckEngine, BrowserResources>()
function browserResources(engine: DeckEngine): BrowserResources {
  let value = resources.get(engine)
  if (value === undefined) {
    value = { renderer: new CanvasDisplayListRenderer() }
    const owned = value
    engine.onDispose(() => {
      owned.renderer.clear()
      void owned.shaper?.then(loaded => loaded.clear()).catch(() => {})
      resources.delete(engine)
    })
    resources.set(engine, value)
  }
  return value
}
async function shaper(engine: DeckEngine): Promise<WasmFontShaper> {
  const value = browserResources(engine)
  value.shaper ??= import('../../../wasmppt-worker/src/generated/shaper/wasmppt_shaper_wasm.js').then(async module => {
    await module.default(); return new WasmFontShaper(module)
  })
  return value.shaper
}

export interface CanvasViewOptions { readonly pageId: string; readonly width?: number; readonly signal?: AbortSignal }

/** One host-owned canvas. Visibility, scrolling and component mounting stay in the application. */
export class CanvasView {
  #request: AbortController | undefined
  #shown: { readonly identities: ReadonlyMap<string, ReturnType<DeckSnapshot['sourceIdentity']>>; readonly revision: number; readonly scene: DisplayScene; readonly pageId: string } | undefined
  #disposed = false
  constructor(readonly canvas: HTMLCanvasElement) {}

  async render(snapshot: DeckSnapshot, options: CanvasViewOptions): Promise<{ readonly scene: DisplayScene; readonly telemetry: RenderTelemetry }> {
    if (this.#disposed) throw new Error('CanvasView is disposed')
    this.#request?.abort()
    const request = new AbortController()
    this.#request = request
    const held = snapshot.retain()
    const state = accessSnapshot(held)
    const signal = AbortSignal.any([request.signal, state.engine.signal, ...(options.signal === undefined ? [] : [options.signal])])
    const fontHost = new ScopedFontLoadingHost()
    try {
      const scene = await held.resolvePage(options.pageId, { signal })
      const width = options.width ?? Math.ceil(scene.width / 9_525)
      if (!Number.isSafeInteger(width) || width < 1 || width > 16384) throw new RangeError('canvas width must be between 1 and 16384')
      const height = Math.ceil(width * scene.height / scene.width)
      if (width * height > 64 * 1024 * 1024) throw new RangeError('canvas exceeds 64 megapixels')
      const scratch = this.canvas.ownerDocument.createElement('canvas')
      scratch.width = width; scratch.height = height
      const context = scratch.getContext('2d')
      if (context === null) throw new Error('Canvas 2D is unavailable')
      const fonts = await prepareInputFonts(state.fonts)
      const fontResolver = state.fonts.length === 0 ? undefined : new FontResolver({
        host: fontHost, webFonts: fonts.definitions, substitutions: fonts.substitutions,
        shaper: await shaper(state.engine),
      })
      const client = state.engine.client
      const telemetry = await browserResources(state.engine).renderer.render(scene, context, {
        signal, fontResolver,
        imageCacheKey: async image => {
          if (!image.partName) throw new Error('image has no package part')
          return client.deckSessionResourceFingerprint(state.handle, state.revision, image.partName, { signal })
        },
        imageResolver: async image => {
          if (!image.partName) throw new Error('image has no package part')
          const resource = await client.deckSessionResource(state.handle, state.revision, image.partName, { signal })
          return /\.svg$/i.test(image.partName) ? decodeSvgImage(resource.bytes, signal) : decodeRasterImage(resource.bytes, {}, signal)
        },
      })
      signal.throwIfAborted()
      const target = this.canvas.getContext('2d')
      if (target === null) throw new Error('Canvas 2D is unavailable')
      this.canvas.width = width; this.canvas.height = height
      target.drawImage(scratch, 0, 0)
      this.#shown = { scene, pageId: options.pageId, revision: held.revision,
        identities: new Map(scene.semantics.flatMap(item => item.source === undefined ? []
          : [[item.source.semanticId, held.sourceIdentity(item.source.semanticId)]])) }
      return { scene, telemetry }
    } finally {
      fontHost.clear()
      await held.dispose()
    }
  }

  hitTest(point: { readonly x: number; readonly y: number }) {
    const shown = this.#shown
    if (shown === undefined) return undefined
    const semantic = hitTestDisplaySceneAtCanvasPoint(shown.scene, this.canvas, point.x, point.y)
    if (semantic?.source === undefined) return undefined
    const identity = shown.identities.get(semantic.source.semanticId)
    if (identity === undefined) return undefined
    return { nodeKey: identity.key, source: identity.source, pageId: shown.pageId,
      revision: shown.revision, bounds: semantic.bounds }
  }
  async dispose(): Promise<void> {
    if (this.#disposed) return
    this.#disposed = true; this.#request?.abort()
    this.#shown = undefined
  }
}

export interface DeckHtmlOptions extends Omit<OfflineHtmlOptions, 'fonts' | 'shaper'> {
  /** Explicit order within this snapshot; omitted selects its presentable pages. */
  readonly pageIds?: readonly string[]
}

export async function exportHtml(snapshot: DeckSnapshot, options: DeckHtmlOptions = {}): Promise<OfflineHtmlResult> {
  const held = snapshot.retain()
  try {
    const state = accessSnapshot(held)
    const signal = AbortSignal.any([state.engine.signal, ...(options.signal === undefined ? [] : [options.signal])])
    const selected = options.pageIds ?? state.pages.filter(page => !page.hidden).map(page => page.id)
    if (new Set(selected).size !== selected.length) throw new RangeError('page selection contains duplicates')
    const pages: OfflineDeckPage[] = []
    for (const id of selected) {
      signal.throwIfAborted()
      const page = requirePage(state, id)
      const result = await state.engine.client.resolveDeckSlide(state.handle, state.revision, page.index, { signal })
      pages.push({ slideIndex: page.index, revision: state.revision, displayList: result.displayList, page: state.rawPages[page.index]! })
    }
    return await serializeOfflineHtmlDocument(pages, async ({ partName }, resourceSignal) =>
      (await state.engine.client.deckSessionResource(state.handle, state.revision, partName, { signal: resourceSignal })).bytes,
    { ...options, signal, fonts: state.fonts, ...(state.fonts.length === 0 ? {} : { shaper: await shaper(state.engine) }) })
  } finally { await held.dispose() }
}
