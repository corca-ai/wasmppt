const EMU_PER_CSS_PIXEL = 9_525

export interface RgbaColor {
  readonly red: number
  readonly green: number
  readonly blue: number
  readonly alpha: number
}

export interface EmuRect {
  readonly x: number
  readonly y: number
  readonly width: number
  readonly height: number
}

export interface SceneTransform {
  readonly bounds: EmuRect
  readonly rotation: number
  readonly flipHorizontal: boolean
  readonly flipVertical: boolean
}

export interface SceneGroupTransform {
  readonly outer: SceneTransform
  readonly childX: number
  readonly childY: number
  readonly childWidth: number
  readonly childHeight: number
}

export interface SceneImage {
  readonly partName?: string
  readonly relationshipId: string
}

export interface SceneTextStyle {
  readonly fontSize: number
  readonly color: RgbaColor
  readonly fontFamily?: string
  readonly bold: boolean
  readonly italic: boolean
  readonly alignment: 'left' | 'center' | 'right' | 'justify'
  readonly verticalAlignment: 'top' | 'center' | 'bottom'
  readonly marginLeft: number
  readonly marginTop: number
  readonly marginRight: number
  readonly marginBottom: number
}

export interface SceneSemanticElement {
  readonly firstCommand: number
  readonly commandCount: number
  readonly shapeId: number
  readonly zOrder: number
  readonly kind: 'shape' | 'image' | 'table' | 'chart' | 'preserved-graphic'
  readonly bounds: EmuRect
  readonly name: string
  readonly alternativeText?: string
  readonly hyperlink?: string
}

export type DisplayDiagnosticCode =
  | 'missing-dependency'
  | 'invalid-xml'
  | 'invalid-value'
  | 'unsupported-graphic-frame'
  | 'unsupported-custom-geometry'
  | 'unsupported-fill'
  | 'unsupported-effect'
  | 'missing-image'
  | 'unsupported-smartart'
  | 'unsupported-metafile'
  | 'unsupported-animation'
  | 'unsupported-transition'
  | 'unsupported-active-content'
  | 'unsupported-3d'
  | 'unsupported-chart-kind'
  | 'unknown'

export interface SceneDiagnostic {
  readonly code: DisplayDiagnosticCode
  readonly partName: string
  readonly shapeId?: number
  readonly message: string
}

export type SceneCommand =
  | { readonly kind: 'clear'; readonly color: RgbaColor }
  | { readonly kind: 'push-group'; readonly transform: number }
  | { readonly kind: 'pop-group' }
  | {
      readonly kind: 'fill-preset'
      readonly geometry: number
      readonly transform: SceneTransform
      readonly color: RgbaColor
    }
  | {
      readonly kind: 'stroke-preset'
      readonly geometry: number
      readonly transform: SceneTransform
      readonly color: RgbaColor
      readonly width: number
      readonly dash?: string
    }
  | {
      readonly kind: 'draw-image'
      readonly resource: number
      readonly transform: SceneTransform
      readonly crop: readonly [number, number, number, number]
    }
  | {
      readonly kind: 'draw-text'
      readonly text: number
      readonly bounds: EmuRect
      readonly style: SceneTextStyle
    }
  | {
      readonly kind: 'draw-unsupported'
      readonly transform: SceneTransform
      readonly feature: 'smartart' | 'metafile' | 'ole-object' | 'graphic-frame'
    }

export interface DisplayScene {
  readonly version: number
  readonly width: number
  readonly height: number
  readonly commands: readonly SceneCommand[]
  readonly groups: readonly SceneGroupTransform[]
  readonly strings: readonly string[]
  readonly images: readonly SceneImage[]
  readonly semantics: readonly SceneSemanticElement[]
  readonly diagnostics: readonly SceneDiagnostic[]
  readonly byteLength: number
}

/** Decode the stable WPDL boundary defensively before touching a canvas. */
export function decodeDisplayList(input: ArrayBuffer | Uint8Array): DisplayScene {
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input)
  const reader = new BinaryReader(bytes)
  if (reader.ascii(4) !== 'WPDL') throw new Error('display list has an invalid magic value')
  const version = reader.u16()
  if (version !== 1 && version !== 2 && version !== 3) {
    throw new Error(`unsupported display-list version ${version}`)
  }
  if (reader.u16() !== 0) throw new Error('display-list reserved flags are non-zero')
  const width = reader.safeI64('slide width')
  const height = reader.safeI64('slide height')
  if (width <= 0 || height <= 0) throw new Error('display-list slide size must be positive')
  const commandCount = reader.boundedCount('command')
  const groupCount = reader.boundedCount('group')
  const stringCount = reader.boundedCount('string')
  const imageCount = reader.boundedCount('image')
  const semanticCount = version >= 2 ? reader.boundedCount('semantic element') : 0
  const diagnosticCount = version >= 2 ? reader.boundedCount('diagnostic') : 0
  const commands: SceneCommand[] = []
  for (let index = 0; index < commandCount; index += 1) {
    commands.push(readCommand(reader, version))
  }
  const groups: SceneGroupTransform[] = []
  for (let index = 0; index < groupCount; index += 1) {
    groups.push({
      outer: readTransform(reader),
      childX: reader.safeI64('group child x'),
      childY: reader.safeI64('group child y'),
      childWidth: reader.safeI64('group child width'),
      childHeight: reader.safeI64('group child height'),
    })
  }
  const strings: string[] = []
  for (let index = 0; index < stringCount; index += 1) strings.push(reader.utf8Blob())
  const images: SceneImage[] = []
  for (let index = 0; index < imageCount; index += 1) {
    const partName = reader.utf8Blob()
    images.push({ partName: partName === '' ? undefined : partName, relationshipId: reader.utf8Blob() })
  }
  const semantics: SceneSemanticElement[] = []
  for (let index = 0; index < semanticCount; index += 1) {
    const firstCommand = reader.u32()
    const commandCount = reader.u32()
    const shapeId = reader.u32()
    const zOrder = reader.u32()
    const kindCode = reader.u8()
    if (kindCode < 1 || kindCode > 5) throw new Error('semantic element has an unknown kind')
    const bounds = readRect(reader)
    const name = reader.utf8Blob()
    const alternativeText = reader.utf8Blob()
    const hyperlink = reader.utf8Blob()
    if (firstCommand + commandCount > commands.length) {
      throw new Error('semantic element command range is out of bounds')
    }
    semantics.push({
      firstCommand,
      commandCount,
      shapeId,
      zOrder,
      kind: semanticKind(kindCode),
      bounds,
      name,
      alternativeText: alternativeText === '' ? undefined : alternativeText,
      hyperlink: hyperlink === '' ? undefined : hyperlink,
    })
  }
  const diagnostics: SceneDiagnostic[] = []
  for (let index = 0; index < diagnosticCount; index += 1) {
    const code = diagnosticCode(reader.u8())
    const rawShapeId = reader.u32()
    diagnostics.push({
      code,
      shapeId: rawShapeId === 0xffff_ffff ? undefined : rawShapeId,
      partName: reader.utf8Blob(),
      message: reader.utf8Blob(),
    })
  }
  if (!reader.done) throw new Error('display list has trailing bytes')
  validateReferences(commands, groups.length, strings.length, images.length)
  return Object.freeze({
    version,
    width,
    height,
    commands: Object.freeze(commands),
    groups: Object.freeze(groups),
    strings: Object.freeze(strings),
    images: Object.freeze(images),
    semantics: Object.freeze(semantics),
    diagnostics: Object.freeze(diagnostics),
    byteLength: bytes.byteLength,
  })
}

export type FontScript = 'latin' | 'east-asian' | 'complex'

export interface ThemeFontSet {
  readonly latin: string
  readonly eastAsian: string
  readonly complexScript: string
}

export interface WebFontDefinition {
  readonly family: string
  readonly source: string | ArrayBuffer
  readonly descriptors?: FontFaceDescriptors
}

export interface ResolvedFont {
  readonly requestedFamily: string
  readonly family: string
  readonly script: FontScript
  readonly exact: boolean
  readonly css: string
}

export interface FontResolverOptions {
  readonly theme?: Partial<ThemeFontSet>
  readonly substitutions?: Readonly<Record<string, string>>
  readonly webFonts?: readonly WebFontDefinition[]
  readonly fallback?: Partial<Record<FontScript, string>>
  readonly host?: FontLoadingHost
}

export interface FontLoadingHost {
  load(definition: WebFontDefinition): Promise<void>
  check(css: string, text: string): boolean
}

/** Resolves theme font slots deterministically and explicitly reports substitutions. */
export class FontResolver {
  readonly #theme: ThemeFontSet
  readonly #substitutions: Readonly<Record<string, string>>
  readonly #webFonts: ReadonlyMap<string, WebFontDefinition>
  readonly #fallback: Record<FontScript, string>
  readonly #host: FontLoadingHost
  readonly #loaded = new Map<string, Promise<void>>()

  constructor(options: FontResolverOptions = {}) {
    this.#theme = {
      latin: options.theme?.latin ?? 'Arial',
      eastAsian: options.theme?.eastAsian ?? 'Noto Sans CJK KR',
      complexScript: options.theme?.complexScript ?? 'Noto Sans Arabic',
    }
    this.#substitutions = Object.freeze({ ...(options.substitutions ?? {}) })
    this.#webFonts = new Map((options.webFonts ?? []).map((font) => [font.family, font]))
    this.#fallback = {
      latin: options.fallback?.latin ?? 'sans-serif',
      'east-asian': options.fallback?.['east-asian'] ?? 'sans-serif',
      complex: options.fallback?.complex ?? 'sans-serif',
    }
    this.#host = options.host ?? new BrowserFontLoadingHost()
  }

  async resolve(
    text: string,
    sizePixels = 18,
    requestedFamily?: string,
    emphasis: { readonly bold?: boolean; readonly italic?: boolean } = {},
  ): Promise<ResolvedFont> {
    const script = detectFontScript(text)
    const requested = requestedFamily ?? this.#themeFamily(script)
    const family = this.#substitutions[requested] ?? requested
    await this.#load(family)
    const prefix = `${emphasis.italic === true ? 'italic' : 'normal'} ${emphasis.bold === true ? '700' : '400'}`
    const css = `${prefix} ${sizePixels}px ${quoteFontFamily(family)}`
    const exact = this.#host.check(css, representativeText(script, text))
    const fallbackFamily = this.#fallback[script]
    const resolvedFamily = exact ? family : fallbackFamily
    const cssFamily = exact
      ? `${quoteFontFamily(family)}, ${quoteFontFamily(fallbackFamily)}`
      : quoteFontFamily(fallbackFamily)
    return Object.freeze({
      requestedFamily: requested,
      family: resolvedFamily,
      script,
      exact,
      css: `${prefix} ${sizePixels}px ${cssFamily}`,
    })
  }

  #themeFamily(script: FontScript): string {
    if (script === 'east-asian') return this.#theme.eastAsian
    if (script === 'complex') return this.#theme.complexScript
    return this.#theme.latin
  }

  async #load(family: string): Promise<void> {
    const definition = this.#webFonts.get(family)
    if (definition === undefined) return
    let loading = this.#loaded.get(family)
    if (loading === undefined) {
      loading = this.#host.load(definition)
      this.#loaded.set(family, loading)
    }
    await loading
  }
}

class BrowserFontLoadingHost implements FontLoadingHost {
  async load(definition: WebFontDefinition): Promise<void> {
    if (typeof FontFace === 'undefined' || globalThis.document === undefined) return
    const face = new FontFace(definition.family, definition.source, definition.descriptors)
    document.fonts.add(await face.load())
  }

  check(css: string, text: string): boolean {
    return globalThis.document?.fonts.check(css, text) ?? false
  }
}

export interface TextMeasureRequest {
  readonly text: string
  readonly font: string
}

/** Measures in font batches on the target context, avoiding per-run host round trips. */
export function measureTextBatch(
  context: Pick<CanvasRenderingContext2D, 'font' | 'measureText'>,
  requests: readonly TextMeasureRequest[],
): readonly number[] {
  const widths = new Array<number>(requests.length)
  const batches = new Map<string, number[]>()
  requests.forEach((request, index) => {
    const indices = batches.get(request.font)
    if (indices === undefined) batches.set(request.font, [index])
    else indices.push(index)
  })
  for (const [font, indices] of batches) {
    context.font = font
    for (const index of indices) widths[index] = context.measureText(requests[index]!.text).width
  }
  return Object.freeze(widths)
}

export function wrapText(
  text: string,
  maxWidth: number,
  measure: (candidate: string) => number,
): readonly string[] {
  if (!(maxWidth > 0)) return Object.freeze([text])
  const tokens = lineBreakTokens(text)
  const lines: string[] = []
  let line = ''
  for (const token of tokens) {
    if (token === '\n') {
      lines.push(line.trimEnd())
      line = ''
      continue
    }
    const candidate = line + token
    if (line !== '' && measure(candidate) > maxWidth) {
      lines.push(line.trimEnd())
      line = token.trimStart()
    } else {
      line = candidate
    }
  }
  if (line !== '' || lines.length === 0) lines.push(line.trimEnd())
  return Object.freeze(lines)
}

export interface DecodedImage {
  readonly source: CanvasImageSource
  readonly residentBytes: number
  close?(): void
}

/** Decode SVG bytes through an HTML image, including Chrome builds that reject SVG ImageBitmap. */
export async function decodeSvgImage(
  input: ArrayBuffer | Uint8Array,
  signal: AbortSignal = new AbortController().signal,
): Promise<DecodedImage> {
  throwIfAborted(signal)
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input)
  const owned = new Uint8Array(bytes.byteLength)
  owned.set(bytes)
  const url = URL.createObjectURL(new Blob([owned.buffer], { type: 'image/svg+xml' }))
  const source = new Image()
  try {
    await new Promise<void>((resolve, reject) => {
      const cleanup = (): void => {
        source.removeEventListener('load', loaded)
        source.removeEventListener('error', failed)
        signal.removeEventListener('abort', aborted)
      }
      const loaded = (): void => {
        cleanup()
        resolve()
      }
      const failed = (): void => {
        cleanup()
        reject(new Error('browser could not decode converted metafile SVG'))
      }
      const aborted = (): void => {
        cleanup()
        reject(new DOMException('image decoding was cancelled', 'AbortError'))
      }
      source.addEventListener('load', loaded, { once: true })
      source.addEventListener('error', failed, { once: true })
      signal.addEventListener('abort', aborted, { once: true })
      source.src = url
    })
  } catch (error) {
    URL.revokeObjectURL(url)
    throw error
  }
  return {
    source,
    residentBytes: source.naturalWidth * source.naturalHeight * 4,
    close: () => URL.revokeObjectURL(url),
  }
}

export type ImageResolver = (
  image: SceneImage,
  signal: AbortSignal,
) => Promise<DecodedImage | undefined>

export interface RenderTelemetry {
  readonly resolutionMs: number
  readonly fontMeasurementMs: number
  readonly displayExecutionMs: number
  readonly mediaDecodeMs: number
  readonly commandCount: number
}

export interface CanvasRenderOptions {
  readonly signal?: AbortSignal
  readonly fontResolver?: FontResolver
  readonly imageResolver?: ImageResolver
  readonly imageCacheBytes?: number
  readonly resolutionMs?: number
  readonly scale?: number
}

/** Executes one compact scene and owns a bounded decoded-image cache. */
export class CanvasDisplayListRenderer {
  readonly #images: ByteBudgetLru<string, DecodedImage>

  constructor(imageCacheBytes = 32 * 1024 * 1024) {
    this.#images = new ByteBudgetLru(imageCacheBytes, (image) => image.close?.())
  }

  get decodedImageBytes(): number {
    return this.#images.residentBytes
  }

  async render(
    scene: DisplayScene,
    context: CanvasRenderingContext2D,
    options: CanvasRenderOptions = {},
  ): Promise<RenderTelemetry> {
    const signal = options.signal ?? new AbortController().signal
    throwIfAborted(signal)
    const scale = options.scale ?? 1
    const widthPixels = scene.width / EMU_PER_CSS_PIXEL
    const rootScale = (widthPixels === 0 ? 1 : context.canvas.width / widthPixels) * scale
    const fontResolver = options.fontResolver ?? new FontResolver()
    const textCommands = scene.commands.filter(
      (command): command is Extract<SceneCommand, { readonly kind: 'draw-text' }> =>
        command.kind === 'draw-text',
    )
    const fontStart = performance.now()
    const resolvedFonts = await Promise.all(
      textCommands.map((command) =>
        fontResolver.resolve(
          scene.strings[command.text] ?? '',
          pointsToCssPixels(command.style.fontSize / 100),
          command.style.fontFamily,
          command.style,
        ),
      ),
    )
    const measurements = measureTextBatch(
      context,
      textCommands.map((command, index) => ({
        text: scene.strings[command.text] ?? '',
        font: resolvedFonts[index]!.css,
      })),
    )
    const fontMeasurementMs = performance.now() - fontStart
    const mediaStart = performance.now()
    const decodedImages = await this.#resolveImages(scene, options.imageResolver, signal)
    const mediaDecodeMs = performance.now() - mediaStart
    throwIfAborted(signal)
    const executionStart = performance.now()
    context.save()
    try {
      context.setTransform(rootScale, 0, 0, rootScale, 0, 0)
      let textIndex = 0
      for (const command of scene.commands) {
        throwIfAborted(signal)
        switch (command.kind) {
          case 'clear':
            context.save()
            context.setTransform(1, 0, 0, 1, 0, 0)
            context.fillStyle = cssColor(command.color)
            context.fillRect(0, 0, context.canvas.width, context.canvas.height)
            context.restore()
            break
          case 'push-group':
            context.save()
            applyGroup(context, required(scene.groups, command.transform, 'group'))
            break
          case 'pop-group':
            context.restore()
            break
          case 'fill-preset':
            drawPreset(context, command.geometry, command.transform, () => {
              context.fillStyle = cssColor(command.color)
              context.fill()
            })
            break
          case 'stroke-preset':
            drawPreset(context, command.geometry, command.transform, () => {
              context.strokeStyle = cssColor(command.color)
              context.lineWidth = toPixels(command.width)
              context.setLineDash(dashPattern(command.dash, toPixels(command.width)))
              context.stroke()
            })
            break
          case 'draw-image': {
            const image = decodedImages[command.resource]
            if (image !== undefined) drawImage(context, image.source, command.transform, command.crop)
            else drawUnsupportedGraphic(context, command.transform, 'Image unavailable')
            break
          }
          case 'draw-text': {
            const text = required(scene.strings, command.text, 'string')
            const font = resolvedFonts[textIndex]!
            const measured = measurements[textIndex]!
            textIndex += 1
            drawText(context, text, command.bounds, command.style, font, measured)
            break
          }
          case 'draw-unsupported':
            drawUnsupportedGraphic(context, command.transform, unsupportedLabel(command.feature))
            break
        }
      }
    } finally {
      context.restore()
    }
    return Object.freeze({
      resolutionMs: options.resolutionMs ?? 0,
      fontMeasurementMs,
      displayExecutionMs: performance.now() - executionStart,
      mediaDecodeMs,
      commandCount: scene.commands.length,
    })
  }

  clear(): void {
    this.#images.clear()
  }

  async #resolveImages(
    scene: DisplayScene,
    resolver: ImageResolver | undefined,
    signal: AbortSignal,
  ): Promise<readonly (DecodedImage | undefined)[]> {
    if (resolver === undefined) return scene.images.map(() => undefined)
    return Promise.all(
      scene.images.map(async (image) => {
        const key = `${image.partName ?? ''}\0${image.relationshipId}`
        const cached = this.#images.get(key)
        if (cached !== undefined) return cached
        const decoded = await resolver(image, signal)
        throwIfAborted(signal)
        if (decoded === undefined) return undefined
        this.#images.set(key, decoded, decoded.residentBytes)
        return decoded
      }),
    )
  }
}

export interface SceneResolver {
  resolveSlide(
    presentationHandle: number,
    slideIndex: number,
    options?: { readonly signal?: AbortSignal },
  ): Promise<ArrayBuffer>
}

export interface VirtualizedViewerOptions {
  readonly sceneCacheBytes?: number
  readonly prefetchNeighbors?: number
  readonly devicePixelRatio?: number
  readonly onTelemetry?: (slideIndex: number, telemetry: RenderTelemetry) => void
}

/** Keeps DOM canvases limited to visible slides while prefetching bounded neighbor scenes. */
export class VirtualizedCanvasViewer {
  readonly #resolver: SceneResolver
  readonly #presentationHandle: number
  readonly #root: HTMLElement
  readonly #renderer: CanvasDisplayListRenderer
  readonly #sceneCache: ByteBudgetLru<number, DisplayScene>
  readonly #prefetchNeighbors: number
  readonly #devicePixelRatio: number
  readonly #onTelemetry: VirtualizedViewerOptions['onTelemetry']
  readonly #mounted = new Map<number, HTMLCanvasElement>()
  #revision = 0
  #abort = new AbortController()
  #disposed = false

  constructor(
    resolver: SceneResolver,
    presentationHandle: number,
    root: HTMLElement,
    renderer = new CanvasDisplayListRenderer(),
    options: VirtualizedViewerOptions = {},
  ) {
    this.#resolver = resolver
    this.#presentationHandle = presentationHandle
    this.#root = root
    this.#renderer = renderer
    this.#sceneCache = new ByteBudgetLru(options.sceneCacheBytes ?? 16 * 1024 * 1024)
    this.#prefetchNeighbors = options.prefetchNeighbors ?? 1
    this.#devicePixelRatio = options.devicePixelRatio ?? globalThis.devicePixelRatio ?? 1
    this.#onTelemetry = options.onTelemetry
  }

  get mountedSlideCount(): number {
    return this.#mounted.size
  }

  get cachedSceneBytes(): number {
    return this.#sceneCache.residentBytes
  }

  async setVisibleSlides(indices: readonly number[]): Promise<void> {
    if (this.#disposed) throw new Error('viewer is disposed')
    const visible = new Set(indices)
    for (const [index, canvas] of this.#mounted) {
      if (!visible.has(index)) {
        canvas.remove()
        this.#mounted.delete(index)
      }
    }
    this.#abort.abort()
    this.#abort = new AbortController()
    const signal = this.#abort.signal
    const revision = ++this.#revision
    const renderTasks = indices.map(async (index) => {
      const started = performance.now()
      const scene = await this.#scene(index, signal)
      if (this.#stale(revision, signal)) return
      const canvas = this.#mounted.get(index) ?? this.#mount(index, scene)
      const context = canvas.getContext('2d')
      if (context === null) throw new Error('Canvas 2D is unavailable')
      const telemetry = await this.#renderer.render(scene, context, {
        signal,
        resolutionMs: performance.now() - started,
      })
      if (!this.#stale(revision, signal)) this.#onTelemetry?.(index, telemetry)
    })
    const neighbors = new Set<number>()
    for (const index of indices) {
      for (let distance = 1; distance <= this.#prefetchNeighbors; distance += 1) {
        if (index - distance >= 0) neighbors.add(index - distance)
        neighbors.add(index + distance)
      }
    }
    for (const index of visible) neighbors.delete(index)
    const prefetchTasks = [...neighbors].map((index) => this.#scene(index, signal).catch(() => undefined))
    await Promise.all(renderTasks)
    void Promise.all(prefetchTasks)
  }

  dispose(): void {
    if (this.#disposed) return
    this.#disposed = true
    this.#abort.abort()
    for (const canvas of this.#mounted.values()) canvas.remove()
    this.#mounted.clear()
    this.#sceneCache.clear()
    this.#renderer.clear()
  }

  async #scene(index: number, signal: AbortSignal): Promise<DisplayScene> {
    const cached = this.#sceneCache.get(index)
    if (cached !== undefined) return cached
    const bytes = await this.#resolver.resolveSlide(this.#presentationHandle, index, { signal })
    throwIfAborted(signal)
    const scene = decodeDisplayList(bytes)
    this.#sceneCache.set(index, scene, scene.byteLength)
    return scene
  }

  #mount(index: number, scene: DisplayScene): HTMLCanvasElement {
    const canvas = document.createElement('canvas')
    const cssWidth = scene.width / EMU_PER_CSS_PIXEL
    const cssHeight = scene.height / EMU_PER_CSS_PIXEL
    canvas.dataset['slideIndex'] = String(index)
    canvas.width = Math.max(1, Math.round(cssWidth * this.#devicePixelRatio))
    canvas.height = Math.max(1, Math.round(cssHeight * this.#devicePixelRatio))
    canvas.style.width = `${cssWidth}px`
    canvas.style.height = `${cssHeight}px`
    this.#root.append(canvas)
    this.#mounted.set(index, canvas)
    return canvas
  }

  #stale(revision: number, signal: AbortSignal): boolean {
    return this.#disposed || signal.aborted || revision !== this.#revision
  }
}

export class ByteBudgetLru<Key, Value> {
  readonly #entries = new Map<Key, { readonly value: Value; readonly weight: number }>()
  readonly #maxBytes: number
  readonly #dispose?: (value: Value) => void
  #residentBytes = 0

  constructor(maxBytes: number, dispose?: (value: Value) => void) {
    if (!Number.isSafeInteger(maxBytes) || maxBytes < 0) {
      throw new RangeError('byte budget must be a non-negative safe integer')
    }
    this.#maxBytes = maxBytes
    this.#dispose = dispose
  }

  get residentBytes(): number {
    return this.#residentBytes
  }

  get size(): number {
    return this.#entries.size
  }

  get(key: Key): Value | undefined {
    const entry = this.#entries.get(key)
    if (entry === undefined) return undefined
    this.#entries.delete(key)
    this.#entries.set(key, entry)
    return entry.value
  }

  set(key: Key, value: Value, weight: number): boolean {
    if (!Number.isSafeInteger(weight) || weight < 0) throw new RangeError('cache weight is invalid')
    const previous = this.#entries.get(key)
    if (previous !== undefined) this.#remove(key, previous)
    if (weight > this.#maxBytes) {
      this.#dispose?.(value)
      return false
    }
    this.#entries.set(key, { value, weight })
    this.#residentBytes += weight
    while (this.#residentBytes > this.#maxBytes) {
      const oldest = this.#entries.entries().next().value as
        | [Key, { readonly value: Value; readonly weight: number }]
        | undefined
      if (oldest === undefined) break
      this.#remove(oldest[0], oldest[1])
    }
    return this.#entries.get(key)?.value === value
  }

  clear(): void {
    for (const entry of this.#entries.values()) this.#dispose?.(entry.value)
    this.#entries.clear()
    this.#residentBytes = 0
  }

  #remove(key: Key, entry: { readonly value: Value; readonly weight: number }): void {
    this.#entries.delete(key)
    this.#residentBytes -= entry.weight
    this.#dispose?.(entry.value)
  }
}

class BinaryReader {
  readonly #bytes: Uint8Array
  readonly #view: DataView
  #offset = 0

  constructor(bytes: Uint8Array) {
    this.#bytes = bytes
    this.#view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  }

  get done(): boolean {
    return this.#offset === this.#bytes.byteLength
  }

  ascii(length: number): string {
    return String.fromCharCode(...this.take(length))
  }

  u8(): number {
    this.#ensure(1)
    return this.#view.getUint8(this.#offset++)
  }

  u16(): number {
    this.#ensure(2)
    const value = this.#view.getUint16(this.#offset, true)
    this.#offset += 2
    return value
  }

  u32(): number {
    this.#ensure(4)
    const value = this.#view.getUint32(this.#offset, true)
    this.#offset += 4
    return value
  }

  i32(): number {
    this.#ensure(4)
    const value = this.#view.getInt32(this.#offset, true)
    this.#offset += 4
    return value
  }

  safeI64(label: string): number {
    this.#ensure(8)
    const value = this.#view.getBigInt64(this.#offset, true)
    this.#offset += 8
    const number = Number(value)
    if (!Number.isSafeInteger(number)) throw new Error(`${label} exceeds JavaScript safe integers`)
    return number
  }

  boundedCount(label: string): number {
    const count = this.u32()
    if (count > this.#bytes.byteLength) throw new Error(`display-list ${label} count is implausible`)
    return count
  }

  utf8Blob(): string {
    const length = this.u32()
    return new TextDecoder('utf-8', { fatal: true }).decode(this.take(length))
  }

  take(length: number): Uint8Array {
    this.#ensure(length)
    const output = this.#bytes.subarray(this.#offset, this.#offset + length)
    this.#offset += length
    return output
  }

  #ensure(length: number): void {
    if (!Number.isSafeInteger(length) || length < 0 || this.#offset + length > this.#bytes.byteLength) {
      throw new Error('display list is truncated')
    }
  }
}

function readCommand(reader: BinaryReader, version: number): SceneCommand {
  switch (reader.u8()) {
    case 1:
      return { kind: 'clear', color: readColor(reader) }
    case 2:
      return { kind: 'push-group', transform: reader.u32() }
    case 3:
      return { kind: 'pop-group' }
    case 4:
      return {
        kind: 'fill-preset',
        geometry: reader.u8(),
        transform: readTransform(reader),
        color: readColor(reader),
      }
    case 5: {
      const command = {
        kind: 'stroke-preset' as const,
        geometry: reader.u8(),
        transform: readTransform(reader),
        color: readColor(reader),
        width: reader.safeI64('stroke width'),
        dash: reader.utf8Blob(),
      }
      return { ...command, dash: command.dash === '' ? undefined : command.dash }
    }
    case 6:
      return {
        kind: 'draw-image',
        resource: reader.u32(),
        transform: readTransform(reader),
        crop: [reader.i32(), reader.i32(), reader.i32(), reader.i32()],
      }
    case 7: {
      const text = reader.u32()
      const bounds = readRect(reader)
      if (version < 3) return { kind: 'draw-text', text, bounds, style: defaultTextStyle() }
      const fontSize = reader.i32()
      const color = readColor(reader)
      const fontFamily = reader.utf8Blob()
      const bold = reader.u8() !== 0
      const italic = reader.u8() !== 0
      const alignment = textAlignment(reader.u8())
      const verticalAlignment = textVerticalAlignment(reader.u8())
      return {
        kind: 'draw-text',
        text,
        bounds,
        style: {
          fontSize,
          color,
          fontFamily: fontFamily === '' ? undefined : fontFamily,
          bold,
          italic,
          alignment,
          verticalAlignment,
          marginLeft: reader.safeI64('text left margin'),
          marginTop: reader.safeI64('text top margin'),
          marginRight: reader.safeI64('text right margin'),
          marginBottom: reader.safeI64('text bottom margin'),
        },
      }
    }
    case 8:
      return {
        kind: 'draw-unsupported',
        transform: readTransform(reader),
        feature: unsupportedFeature(reader.u8()),
      }
    default:
      throw new Error('display list contains an unknown command')
  }
}

function unsupportedFeature(
  value: number,
): Extract<SceneCommand, { readonly kind: 'draw-unsupported' }>['feature'] {
  if (value === 1) return 'smartart'
  if (value === 2) return 'metafile'
  if (value === 3) return 'ole-object'
  if (value === 4) return 'graphic-frame'
  throw new Error('display list contains an unknown preserved graphic feature')
}

function defaultTextStyle(): SceneTextStyle {
  return {
    fontSize: 1_800,
    color: { red: 0, green: 0, blue: 0, alpha: 255 },
    bold: false,
    italic: false,
    alignment: 'left',
    verticalAlignment: 'top',
    marginLeft: 91_440,
    marginTop: 45_720,
    marginRight: 91_440,
    marginBottom: 45_720,
  }
}

function textAlignment(value: number): SceneTextStyle['alignment'] {
  if (value === 1) return 'left'
  if (value === 2) return 'center'
  if (value === 3) return 'right'
  if (value === 4) return 'justify'
  throw new Error('display list contains an unknown text alignment')
}

function textVerticalAlignment(value: number): SceneTextStyle['verticalAlignment'] {
  if (value === 1) return 'top'
  if (value === 2) return 'center'
  if (value === 3) return 'bottom'
  throw new Error('display list contains an unknown vertical text alignment')
}

function readTransform(reader: BinaryReader): SceneTransform {
  return {
    bounds: readRect(reader),
    rotation: reader.i32(),
    flipHorizontal: reader.u8() !== 0,
    flipVertical: reader.u8() !== 0,
  }
}

function readRect(reader: BinaryReader): EmuRect {
  return {
    x: reader.safeI64('rectangle x'),
    y: reader.safeI64('rectangle y'),
    width: reader.safeI64('rectangle width'),
    height: reader.safeI64('rectangle height'),
  }
}

function readColor(reader: BinaryReader): RgbaColor {
  return { red: reader.u8(), green: reader.u8(), blue: reader.u8(), alpha: reader.u8() }
}

function validateReferences(
  commands: readonly SceneCommand[],
  groupCount: number,
  stringCount: number,
  imageCount: number,
): void {
  let depth = 0
  for (const command of commands) {
    if (command.kind === 'push-group') {
      if (command.transform >= groupCount) throw new Error('display list references an unknown group')
      depth += 1
    } else if (command.kind === 'pop-group') {
      depth -= 1
      if (depth < 0) throw new Error('display list group stack underflows')
    } else if (command.kind === 'draw-text' && command.text >= stringCount) {
      throw new Error('display list references an unknown string')
    } else if (command.kind === 'draw-image' && command.resource >= imageCount) {
      throw new Error('display list references an unknown image')
    }
  }
  if (depth !== 0) throw new Error('display list group stack is unbalanced')
}

function diagnosticCode(code: number): DisplayDiagnosticCode {
  if (code === 1) return 'missing-dependency'
  if (code === 2) return 'invalid-xml'
  if (code === 3) return 'invalid-value'
  if (code === 4) return 'unsupported-graphic-frame'
  if (code === 5) return 'unsupported-custom-geometry'
  if (code === 6) return 'unsupported-fill'
  if (code === 7) return 'unsupported-effect'
  if (code === 8) return 'missing-image'
  if (code === 9) return 'unsupported-smartart'
  if (code === 10) return 'unsupported-metafile'
  if (code === 11) return 'unsupported-animation'
  if (code === 12) return 'unsupported-transition'
  if (code === 13) return 'unsupported-active-content'
  if (code === 14) return 'unsupported-3d'
  if (code === 15) return 'unsupported-chart-kind'
  return 'unknown'
}

function semanticKind(code: number): SceneSemanticElement['kind'] {
  if (code === 1) return 'shape'
  if (code === 2) return 'image'
  if (code === 3) return 'table'
  if (code === 4) return 'chart'
  return 'preserved-graphic'
}

function detectFontScript(text: string): FontScript {
  if (/\p{Script=Hangul}|\p{Script=Han}|\p{Script=Hiragana}|\p{Script=Katakana}/u.test(text)) {
    return 'east-asian'
  }
  if (/\p{Script=Arabic}|\p{Script=Hebrew}|\p{Script=Devanagari}|\p{Script=Thai}/u.test(text)) {
    return 'complex'
  }
  return 'latin'
}

function representativeText(script: FontScript, text: string): string {
  if (text !== '') return text
  if (script === 'east-asian') return '한국語'
  if (script === 'complex') return 'العربية'
  return 'Aa'
}

function quoteFontFamily(family: string): string {
  if (/^(serif|sans-serif|monospace|cursive|fantasy|system-ui)$/i.test(family)) return family
  return `"${family.replaceAll('"', '\\"')}"`
}

function lineBreakTokens(text: string): readonly string[] {
  const tokens: string[] = []
  let latin = ''
  for (const character of text) {
    if (character === '\n') {
      if (latin !== '') tokens.push(latin)
      latin = ''
      tokens.push(character)
    } else if (/\s/u.test(character)) {
      latin += character
    } else if (/\p{Script=Han}|\p{Script=Hangul}|\p{Script=Hiragana}|\p{Script=Katakana}/u.test(character)) {
      if (latin !== '') tokens.push(latin)
      latin = ''
      tokens.push(character)
    } else {
      latin += character
    }
  }
  if (latin !== '') tokens.push(latin)
  return tokens
}

function required<Value>(values: readonly Value[], index: number, label: string): Value {
  const value = values[index]
  if (value === undefined) throw new Error(`display list references an unknown ${label}`)
  return value
}

function cssColor(color: RgbaColor): string {
  return `rgba(${color.red}, ${color.green}, ${color.blue}, ${color.alpha / 255})`
}

function applyGroup(context: CanvasRenderingContext2D, group: SceneGroupTransform): void {
  const bounds = pixelRect(group.outer.bounds)
  context.translate(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2)
  context.rotate((group.outer.rotation / 60_000) * (Math.PI / 180))
  context.scale(group.outer.flipHorizontal ? -1 : 1, group.outer.flipVertical ? -1 : 1)
  context.translate(-bounds.width / 2, -bounds.height / 2)
  context.scale(
    group.childWidth === 0 ? 1 : bounds.width / toPixels(group.childWidth),
    group.childHeight === 0 ? 1 : bounds.height / toPixels(group.childHeight),
  )
  context.translate(-toPixels(group.childX), -toPixels(group.childY))
}

function drawPreset(
  context: CanvasRenderingContext2D,
  geometry: number,
  transform: SceneTransform,
  paint: () => void,
): void {
  context.save()
  const bounds = applyShapeTransform(context, transform)
  context.beginPath()
  presetPath(context, geometry, bounds.width, bounds.height)
  paint()
  context.restore()
}

function applyShapeTransform(context: CanvasRenderingContext2D, transform: SceneTransform): EmuRect {
  const bounds = pixelRect(transform.bounds)
  context.translate(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2)
  context.rotate((transform.rotation / 60_000) * (Math.PI / 180))
  context.scale(transform.flipHorizontal ? -1 : 1, transform.flipVertical ? -1 : 1)
  context.translate(-bounds.width / 2, -bounds.height / 2)
  return { x: 0, y: 0, width: bounds.width, height: bounds.height }
}

function presetPath(
  context: CanvasRenderingContext2D,
  geometry: number,
  width: number,
  height: number,
): void {
  if (geometry === 3) {
    context.ellipse(width / 2, height / 2, Math.abs(width / 2), Math.abs(height / 2), 0, 0, Math.PI * 2)
  } else if (geometry === 4) {
    context.moveTo(0, 0)
    context.lineTo(width, height)
  } else if (geometry === 5 || geometry === 6) {
    context.moveTo(geometry === 5 ? width / 2 : 0, 0)
    context.lineTo(width, height)
    context.lineTo(0, height)
    context.closePath()
  } else if (geometry === 7) {
    context.moveTo(width / 2, 0)
    context.lineTo(width, height / 2)
    context.lineTo(width / 2, height)
    context.lineTo(0, height / 2)
    context.closePath()
  } else if (geometry === 8) {
    context.moveTo(width / 4, 0)
    context.lineTo(width, 0)
    context.lineTo((width * 3) / 4, height)
    context.lineTo(0, height)
    context.closePath()
  } else if (geometry === 9) {
    context.moveTo(width / 4, 0)
    context.lineTo((width * 3) / 4, 0)
    context.lineTo(width, height / 2)
    context.lineTo((width * 3) / 4, height)
    context.lineTo(width / 4, height)
    context.lineTo(0, height / 2)
    context.closePath()
  } else if (geometry === 2) {
    context.roundRect(0, 0, width, height, Math.min(Math.abs(width), Math.abs(height)) / 8)
  } else {
    context.rect(0, 0, width, height)
  }
}

function dashPattern(dash: string | undefined, width: number): number[] {
  const unit = Math.max(1, width)
  if (dash === 'dash') return [4 * unit, 3 * unit]
  if (dash === 'dot') return [unit, 2 * unit]
  if (dash === 'dashDot') return [4 * unit, 2 * unit, unit, 2 * unit]
  return []
}

function drawImage(
  context: CanvasRenderingContext2D,
  image: CanvasImageSource,
  transform: SceneTransform,
  crop: readonly [number, number, number, number],
): void {
  context.save()
  const bounds = applyShapeTransform(context, transform)
  const sourceWidth = 'width' in image ? Number(image.width) : bounds.width
  const sourceHeight = 'height' in image ? Number(image.height) : bounds.height
  const left = Math.max(0, crop[0] / 100_000)
  const top = Math.max(0, crop[1] / 100_000)
  const right = Math.max(0, crop[2] / 100_000)
  const bottom = Math.max(0, crop[3] / 100_000)
  context.drawImage(
    image,
    sourceWidth * left,
    sourceHeight * top,
    sourceWidth * Math.max(0, 1 - left - right),
    sourceHeight * Math.max(0, 1 - top - bottom),
    0,
    0,
    bounds.width,
    bounds.height,
  )
  context.restore()
}

function unsupportedLabel(
  feature: Extract<SceneCommand, { readonly kind: 'draw-unsupported' }>['feature'],
): string {
  if (feature === 'smartart') return 'SmartArt preview unavailable'
  if (feature === 'metafile') return 'EMF/WMF preview unavailable'
  if (feature === 'ole-object') return 'Embedded object preview unavailable'
  return 'Graphic preview unavailable'
}

function drawUnsupportedGraphic(
  context: CanvasRenderingContext2D,
  transform: SceneTransform,
  label: string,
): void {
  context.save()
  const bounds = applyShapeTransform(context, transform)
  context.fillStyle = 'rgba(104, 116, 129, 0.08)'
  context.strokeStyle = 'rgba(104, 116, 129, 0.65)'
  context.lineWidth = 1
  context.setLineDash([6, 4])
  context.fillRect(0, 0, bounds.width, bounds.height)
  context.strokeRect(0, 0, bounds.width, bounds.height)
  context.setLineDash([])
  context.fillStyle = 'rgba(70, 79, 89, 0.9)'
  context.font = '12px sans-serif'
  context.textAlign = 'center'
  context.textBaseline = 'middle'
  context.fillText(label, bounds.width / 2, bounds.height / 2, Math.max(0, bounds.width - 16))
  context.restore()
}

function drawText(
  context: CanvasRenderingContext2D,
  text: string,
  bounds: EmuRect,
  style: SceneTextStyle,
  font: ResolvedFont,
  measuredWidth: number,
): void {
  context.save()
  context.font = font.css
  context.textBaseline = 'top'
  context.fillStyle = cssColor(style.color)
  const pixelBounds = pixelRect(bounds)
  const left = pixelBounds.x + toPixels(style.marginLeft)
  const top = pixelBounds.y + toPixels(style.marginTop)
  const width = Math.max(
    0,
    pixelBounds.width - toPixels(style.marginLeft) - toPixels(style.marginRight),
  )
  const height = Math.max(
    0,
    pixelBounds.height - toPixels(style.marginTop) - toPixels(style.marginBottom),
  )
  const lines = wrapText(text, width, (candidate) => {
    if (candidate === text) return measuredWidth
    return context.measureText(candidate).width
  })
  const lineHeight = pointsToCssPixels(style.fontSize / 100) * 1.2
  const blockHeight = lines.length * lineHeight
  const y =
    style.verticalAlignment === 'center'
      ? top + Math.max(0, height - blockHeight) / 2
      : style.verticalAlignment === 'bottom'
        ? top + Math.max(0, height - blockHeight)
        : top
  const alignment = style.alignment === 'justify' ? 'left' : style.alignment
  context.textAlign = alignment
  const x = alignment === 'center' ? left + width / 2 : alignment === 'right' ? left + width : left
  for (let index = 0; index < lines.length; index += 1) {
    context.fillText(lines[index]!, x, y + index * lineHeight)
  }
  context.restore()
}

function pointsToCssPixels(points: number): number {
  return (points * 96) / 72
}

function throwIfAborted(signal: AbortSignal): void {
  if (signal.aborted) throw new DOMException('slide rendering was cancelled', 'AbortError')
}

function toPixels(value: number): number {
  return value / EMU_PER_CSS_PIXEL
}

function pixelRect(bounds: EmuRect): EmuRect {
  return {
    x: toPixels(bounds.x),
    y: toPixels(bounds.y),
    width: toPixels(bounds.width),
    height: toPixels(bounds.height),
  }
}
