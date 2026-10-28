import {
  CanvasDisplayListRenderer, FontResolver, decodeRasterImage, decodeSvgImage,
  hitTestDisplaySceneAtCanvasPoint, type DisplayScene, type RenderTelemetry, type RichTextLayoutPlan,
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


export interface CanvasTextGeometry {
  readonly semanticId: string
  readonly nodeKey: string
  readonly source: NonNullable<ReturnType<DeckSnapshot['sourceIdentity']>>['source']
  readonly text: string
  readonly ranges: readonly { readonly start: number; readonly end: number; readonly bounds: { readonly x: number; readonly y: number; readonly width: number; readonly height: number } }[]
}

function canvasTextGeometry(scene: DisplayScene, layouts: ReadonlyMap<number, RichTextLayoutPlan>, snapshot: DeckSnapshot): readonly CanvasTextGeometry[] {
  const grouped = new Set<number>()
  let depth = 0
  scene.commands.forEach((command, index) => {
    if (command.kind === 'push-group') depth++
    if (depth > 0) grouped.add(index)
    if (command.kind === 'pop-group') depth--
  })
  const result: CanvasTextGeometry[] = []
  let count = 0
  for (const semantic of scene.semantics) {
    if (semantic.source === undefined) continue
    const identity = snapshot.sourceIdentity(semantic.source.semanticId)
    if (identity === undefined) continue
    const entries: [number, RichTextLayoutPlan][] = []
    for (let index = semantic.firstCommand; index < semantic.firstCommand + semantic.commandCount; index++) {
      const layout = layouts.get(index)
      if (layout !== undefined) entries.push([index, layout])
    }
    // Keep one untransformed source stream; other shapes retain semantic hit testing.
    if (entries.length !== 1) continue
    const [index, layout] = entries[0]!
    const command = scene.commands[index]
    if (command?.kind !== 'draw-rich-text' || grouped.has(index) || layout.rotationDegrees !== 0 || command.frame.warp !== undefined) continue
    const paragraphs = command.frame.paragraphs.map(paragraph => paragraph.runs.map(run => run.text).join(''))
    const starts: number[] = []
    let offset = 0
    for (const paragraph of paragraphs) { starts.push(offset); offset += paragraph.length + 1 }
    const text = paragraphs.join('\n')
    const ranges = layout.runs.flatMap(run => run.sourceStart === undefined || run.sourceEnd === undefined || !run.text.trim() ? [] : [{
      start: (starts[run.paragraphIndex] ?? 0) + run.sourceStart, end: (starts[run.paragraphIndex] ?? 0) + run.sourceEnd,
      bounds: Object.freeze({ x: run.x * 9525, y: (run.baseline - run.fontSize * 0.82) * 9525, width: run.width * 9525, height: run.fontSize * 1.2 * 9525 }),
    }])
    count += ranges.length
    if (count > 50_000) return []
    result.push(Object.freeze({ semanticId: semantic.source.semanticId, nodeKey: identity.key, source: identity.source === undefined ? undefined : Object.freeze({ ...identity.source }), text, ranges: Object.freeze(ranges.map(item => Object.freeze(item))) }))
  }
  return Object.freeze(result)
}

export interface CanvasViewOptions { readonly pageId: string; readonly width?: number; readonly signal?: AbortSignal }

/** One host-owned canvas. Visibility, scrolling and component mounting stay in the application. */
export class CanvasView {
  #request: AbortController | undefined
  #shown: { readonly identities: ReadonlyMap<string, ReturnType<DeckSnapshot['sourceIdentity']>>; readonly revision: number; readonly scene: DisplayScene; readonly pageId: string; readonly text: readonly CanvasTextGeometry[] } | undefined
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
      const layouts = new Map<number, RichTextLayoutPlan>()
      const telemetry = await browserResources(state.engine).renderer.render(scene, context, {
        signal, fontResolver, onTextLayout: (index, layout) => layouts.set(index, layout),
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
      this.#shown = { scene, text: canvasTextGeometry(scene, layouts, held), pageId: options.pageId, revision: held.revision,
        identities: new Map(scene.semantics.flatMap(item => item.source === undefined ? []
          : [[item.source.semanticId, held.sourceIdentity(item.source.semanticId)]])) }
      return { scene, telemetry }
    } finally {
      fontHost.clear()
      await held.dispose()
    }
  }

  /** Source-backed native text tokens from the displayed frame, in scene EMU. */
  textGeometry(): readonly CanvasTextGeometry[] { return this.#shown?.text ?? [] }

  hitTest(point: { readonly x: number; readonly y: number }) {
    const shown = this.#shown
    if (shown === undefined) return undefined
    const semantic = hitTestDisplaySceneAtCanvasPoint(shown.scene, this.canvas, point.x, point.y)
    if (semantic?.source === undefined) return undefined
    const identity = shown.identities.get(semantic.source.semanticId)
    if (identity === undefined) return undefined
    const rect = this.canvas.getBoundingClientRect()
    const x = (point.x - rect.left) / rect.width * shown.scene.width
    const y = (point.y - rect.top) / rect.height * shown.scene.height
    const text = shown.text.find(item => item.semanticId === semantic.source!.semanticId)
    const token = text?.ranges.find(item => x >= item.bounds.x && x <= item.bounds.x + item.bounds.width
      && y >= item.bounds.y && y <= item.bounds.y + item.bounds.height)
    return { nodeKey: identity.key, source: identity.source, pageId: shown.pageId,
      ...(token === undefined ? {} : { text: { value: text!.text, start: token.start, end: token.end } }),
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
