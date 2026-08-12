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
  readonly underline: boolean
  readonly strike: boolean
  readonly characterSpacing: number
  readonly baseline: number
  readonly alignment: 'left' | 'center' | 'right' | 'justify'
  readonly verticalAlignment: 'top' | 'center' | 'bottom'
  readonly marginLeft: number
  readonly marginTop: number
  readonly marginRight: number
  readonly marginBottom: number
}

export interface SceneTextRun {
  readonly text: string
  readonly style: SceneTextStyle
  readonly eastAsianFontFamily?: string
  readonly complexScriptFontFamily?: string
}

export interface SceneParagraph {
  readonly runs: readonly SceneTextRun[]
  readonly alignment: SceneTextStyle['alignment']
  readonly bullet?: string
  readonly level: number
  readonly marginLeft: number
  readonly indent: number
  readonly lineSpacing?: number
  readonly spaceBefore?: number
  readonly spaceAfter?: number
  readonly direction: 'ltr' | 'rtl'
  readonly tabs: readonly SceneTextTab[]
}

export interface SceneTextTab {
  readonly position: number
  readonly alignment: 'left' | 'center' | 'right' | 'decimal'
}

export interface SceneTextFrame {
  readonly paragraphs: readonly SceneParagraph[]
  readonly verticalAlignment: SceneTextStyle['verticalAlignment']
  readonly marginLeft: number
  readonly marginTop: number
  readonly marginRight: number
  readonly marginBottom: number
  readonly wrap: boolean
  readonly autofit: 'none' | 'shrink-text' | 'resize-shape'
  readonly flow: 'horizontal' | 'vertical' | 'vertical-270'
}

export interface SceneGradientStop {
  readonly position: number
  readonly color: RgbaColor
}

export type SceneLineEnd = 'triangle' | 'stealth' | 'diamond' | 'oval' | 'arrow'

export type SceneFill =
  | { readonly kind: 'none' }
  | { readonly kind: 'solid'; readonly color: RgbaColor }
  | { readonly kind: 'linear-gradient'; readonly angle: number; readonly stops: readonly SceneGradientStop[] }
  | { readonly kind: 'radial-gradient'; readonly stops: readonly SceneGradientStop[] }
  | { readonly kind: 'pattern'; readonly preset: string; readonly foreground: RgbaColor; readonly background: RgbaColor }

export type ScenePathCommand =
  | { readonly kind: 'move-to'; readonly x: number; readonly y: number }
  | { readonly kind: 'line-to'; readonly x: number; readonly y: number }
  | { readonly kind: 'quadratic-to'; readonly controlX: number; readonly controlY: number; readonly x: number; readonly y: number }
  | { readonly kind: 'cubic-to'; readonly control1X: number; readonly control1Y: number; readonly control2X: number; readonly control2Y: number; readonly x: number; readonly y: number }
  | { readonly kind: 'arc-to'; readonly widthRadius: number; readonly heightRadius: number; readonly startAngle: number; readonly sweepAngle: number }
  | { readonly kind: 'close' }

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
      readonly headEnd?: SceneLineEnd
      readonly tailEnd?: SceneLineEnd
    }
  | {
      readonly kind: 'fill-gradient-preset'
      readonly geometry: number
      readonly transform: SceneTransform
      readonly angle: number
      readonly stops: readonly SceneGradientStop[]
    }
  | {
      readonly kind: 'fill-radial-gradient-preset'
      readonly geometry: number
      readonly transform: SceneTransform
      readonly stops: readonly SceneGradientStop[]
    }
  | {
      readonly kind: 'fill-pattern-preset'
      readonly geometry: number
      readonly transform: SceneTransform
      readonly preset: string
      readonly foreground: RgbaColor
      readonly background: RgbaColor
    }
  | {
      readonly kind: 'draw-custom-path'
      readonly transform: SceneTransform
      readonly pathWidth: number
      readonly pathHeight: number
      readonly path: readonly ScenePathCommand[]
      readonly fill: SceneFill
      readonly stroke?: {
        readonly color: RgbaColor
        readonly width: number
        readonly dash?: string
        readonly headEnd?: SceneLineEnd
        readonly tailEnd?: SceneLineEnd
      }
    }
  | {
      readonly kind: 'draw-outer-shadow'
      readonly geometry: number
      readonly transform: SceneTransform
      readonly color: RgbaColor
      readonly blurRadius: number
      readonly distance: number
      readonly direction: number
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
      readonly kind: 'draw-rich-text'
      readonly bounds: EmuRect
      readonly frame: SceneTextFrame
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
  if (version !== 1 && version !== 2 && version !== 3 && version !== 4 && version !== 5) {
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
    const semanticCommandCount = reader.u32()
    const shapeId = reader.u32()
    const zOrder = reader.u32()
    const kindCode = reader.u8()
    if (kindCode < 1 || kindCode > 5) throw new Error('semantic element has an unknown kind')
    const bounds = readRect(reader)
    const name = reader.utf8Blob()
    const alternativeText = reader.utf8Blob()
    const hyperlink = reader.utf8Blob()
    if (firstCommand + semanticCommandCount > commands.length) {
      throw new Error('semantic element command range is out of bounds')
    }
    semantics.push({
      firstCommand,
      commandCount: semanticCommandCount,
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
  readonly #resolved = new Map<string, Promise<ResolvedFont>>()

  constructor(options: FontResolverOptions = {}) {
    this.#theme = {
      latin: options.theme?.latin ?? 'Arial',
      eastAsian: options.theme?.eastAsian ?? 'Noto Sans CJK KR',
      complexScript: options.theme?.complexScript ?? 'Noto Sans Arabic',
    }
    this.#substitutions = Object.freeze({ ...options.substitutions })
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
    const key = `${requestedFamily ?? ''}\0${sizePixels}\0${emphasis.bold === true ? 1 : 0}${emphasis.italic === true ? 1 : 0}\0${text}`
    const cached = this.#resolved.get(key)
    if (cached !== undefined) {
      this.#resolved.delete(key)
      this.#resolved.set(key, cached)
      return cached
    }
    const resolving = this.#resolveUncached(text, sizePixels, requestedFamily, emphasis)
    this.#resolved.set(key, resolving)
    while (this.#resolved.size > 512) {
      const oldest = this.#resolved.keys().next().value as string | undefined
      if (oldest === undefined) break
      this.#resolved.delete(oldest)
    }
    return resolving
  }

  async #resolveUncached(
    text: string,
    sizePixels: number,
    requestedFamily: string | undefined,
    emphasis: { readonly bold?: boolean; readonly italic?: boolean },
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
  const widths = Array.from({ length: requests.length }, () => 0)
  const batches = new Map<string, Map<string, number[]>>()
  requests.forEach((request, index) => {
    let texts = batches.get(request.font)
    if (texts === undefined) {
      texts = new Map()
      batches.set(request.font, texts)
    }
    const indices = texts.get(request.text)
    if (indices === undefined) texts.set(request.text, [index])
    else indices.push(index)
  })
  for (const [font, texts] of batches) {
    context.font = font
    for (const [value, indices] of texts) {
      const width = context.measureText(value).width
      for (const index of indices) widths[index] = width
    }
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

export interface RichTextLayoutRun {
  readonly text: string
  readonly x: number
  readonly baseline: number
  readonly width: number
  readonly font: ResolvedFont
  readonly color: RgbaColor
  readonly underline: boolean
  readonly strike: boolean
  readonly characterSpacing: number
  readonly fontSize: number
  readonly baselineShift: number
  readonly direction: 'ltr' | 'rtl'
}

export interface RichTextLayoutPlan {
  readonly runs: readonly RichTextLayoutRun[]
  readonly contentWidth: number
  readonly contentHeight: number
  readonly layoutBounds: EmuRect
  readonly rotationDegrees: 0 | 90 | -90
}

/** Builds the backend-neutral positioned run plan shared by Canvas and DOM/SVG renderers. */
export async function buildRichTextLayout(
  context: Pick<CanvasRenderingContext2D, 'font' | 'measureText'>,
  command: Extract<SceneCommand, { readonly kind: 'draw-rich-text' }>,
  resolver = new FontResolver(),
): Promise<RichTextLayoutPlan> {
  const rotationDegrees = command.frame.flow === 'vertical'
    ? 90
    : command.frame.flow === 'vertical-270'
      ? -90
      : 0
  const layoutBounds = rotationDegrees === 0
    ? command.bounds
    : {
        x: command.bounds.x + (command.bounds.width - command.bounds.height) / 2,
        y: command.bounds.y + (command.bounds.height - command.bounds.width) / 2,
        width: command.bounds.height,
        height: command.bounds.width,
      }
  const innerX = toPixels(layoutBounds.x + command.frame.marginLeft)
  const innerY = toPixels(layoutBounds.y + command.frame.marginTop)
  const innerWidth = Math.max(
    0,
    toPixels(layoutBounds.width - command.frame.marginLeft - command.frame.marginRight),
  )
  const innerHeight = Math.max(
    0,
    toPixels(layoutBounds.height - command.frame.marginTop - command.frame.marginBottom),
  )
  const resolved = await Promise.all(
    command.frame.paragraphs.flatMap((paragraph) =>
      paragraph.runs.map(async (run) => {
        const script = detectFontScript(run.text)
        const requested =
          script === 'east-asian'
            ? run.eastAsianFontFamily ?? run.style.fontFamily
            : script === 'complex'
              ? run.complexScriptFontFamily ?? run.style.fontFamily
              : run.style.fontFamily
        return resolver.resolve(
          run.text,
          pointsToCssPixels(run.style.fontSize / 100),
          requested,
          run.style,
        )
      }),
    ),
  )
  const measureRequests: TextMeasureRequest[] = []
  let measureFontIndex = 0
  for (const paragraph of command.frame.paragraphs) {
    if (paragraph.bullet !== undefined && paragraph.runs[0] !== undefined) {
      measureRequests.push({ text: `${paragraph.bullet} `, font: resolved[measureFontIndex]!.css })
    }
    for (const run of paragraph.runs) {
      const font = resolved[measureFontIndex++]!
      for (const token of command.frame.wrap ? lineBreakTokens(run.text) : [run.text]) {
        if (token !== '\n' && token !== '\t') measureRequests.push({ text: token, font: font.css })
      }
    }
  }
  const measured = measureTextBatch(context, measureRequests)
  const measurementLookup = new Map(
    measureRequests.map((request, index) => [`${request.font}\0${request.text}`, measured[index]!]),
  )
  let resolvedIndex = 0
  const lines: Array<{
    readonly runs: Array<Omit<RichTextLayoutRun, 'x' | 'baseline'>>
    readonly height: number
    readonly alignment: SceneTextStyle['alignment']
    readonly before: number
    readonly after: number
    readonly left: number
    readonly direction: 'ltr' | 'rtl'
  }> = []
  for (const paragraph of command.frame.paragraphs) {
    const lineRuns: Array<Omit<RichTextLayoutRun, 'x' | 'baseline'>> = []
    let lineWidth = 0
    let lineHeight = 0
    const left = toPixels(paragraph.marginLeft + paragraph.indent)
    const available = Math.max(0, innerWidth - left)
    const inputs = paragraph.bullet === undefined || paragraph.runs[0] === undefined
      ? paragraph.runs
      : [{ ...paragraph.runs[0]!, text: `${paragraph.bullet} ` }, ...paragraph.runs]
    for (const run of inputs) {
      const font = paragraph.bullet !== undefined && run === inputs[0]
        ? resolved[resolvedIndex] ?? await resolver.resolve(run.text)
        : resolved[resolvedIndex++]!
      const tokens = command.frame.wrap ? lineBreakTokens(run.text) : [run.text]
      for (const token of tokens) {
        if (token === '\n') {
          lines.push({ runs: lineRuns.splice(0), height: lineHeight || 1, alignment: paragraph.alignment, before: 0, after: 0, left, direction: paragraph.direction })
          lineWidth = 0
          lineHeight = 0
          continue
        }
        const spacing = pointsToCssPixels(run.style.characterSpacing / 100)
        const width = token === '\t'
          ? nextTabWidth(lineWidth, paragraph.tabs)
          : (measurementLookup.get(`${font.css}\0${token}`) ?? 0) + characterGapCount(token) * spacing
        if (command.frame.wrap && lineRuns.length > 0 && lineWidth + width > available) {
          lines.push({ runs: lineRuns.splice(0), height: lineHeight || 1, alignment: paragraph.alignment, before: 0, after: 0, left, direction: paragraph.direction })
          lineWidth = 0
          lineHeight = 0
        }
        lineRuns.push({
          text: token === '\t' ? '' : token,
          width,
          font,
          color: run.style.color,
          underline: run.style.underline,
          strike: run.style.strike,
          characterSpacing: spacing,
          fontSize: pointsToCssPixels(run.style.fontSize / 100),
          baselineShift: run.style.baseline / 100_000,
          direction: paragraph.direction,
        })
        lineWidth += width
        lineHeight = Math.max(lineHeight, pointsToCssPixels(run.style.fontSize / 100) * 1.2)
      }
    }
    lines.push({
      runs: lineRuns,
      height: applyLineSpacing(lineHeight || 1, paragraph.lineSpacing),
      alignment: paragraph.alignment,
      before: spacingPixels(paragraph.spaceBefore),
      after: spacingPixels(paragraph.spaceAfter),
      left,
      direction: paragraph.direction,
    })
  }
  const rawHeight = lines.reduce((sum, line) => sum + line.before + line.height + line.after, 0)
  const shrink = command.frame.autofit === 'shrink-text' && rawHeight > innerHeight
    ? Math.max(0.1, innerHeight / rawHeight)
    : 1
  const contentHeight = rawHeight * shrink
  let y = innerY
  if (command.frame.verticalAlignment === 'center') y += Math.max(0, (innerHeight - contentHeight) / 2)
  if (command.frame.verticalAlignment === 'bottom') y += Math.max(0, innerHeight - contentHeight)
  const output: RichTextLayoutRun[] = []
  let contentWidth = 0
  for (const line of lines) {
    y += line.before * shrink
    const visualRuns = line.direction === 'rtl'
      ? Array.from(line.runs, (_, index) => line.runs[line.runs.length - index - 1]!)
      : line.runs
    const width = visualRuns.reduce((sum, run) => sum + run.width, 0) * shrink
    let x = innerX + line.left
    if (line.alignment === 'center') x += Math.max(0, (innerWidth - line.left - width) / 2)
    if (line.alignment === 'right' || (line.alignment === 'left' && line.direction === 'rtl')) {
      x += Math.max(0, innerWidth - line.left - width)
    }
    for (const run of visualRuns) {
      output.push({
        ...run,
        x,
        baseline: y + line.height * 0.82 * shrink - run.fontSize * run.baselineShift * shrink,
        width: run.width * shrink,
        characterSpacing: run.characterSpacing * shrink,
        fontSize: run.fontSize * shrink,
        font: shrink === 1 ? run.font : { ...run.font, css: scaleCssFont(run.font.css, shrink) },
      })
      x += run.width * shrink
    }
    y += (line.height + line.after) * shrink
    contentWidth = Math.max(contentWidth, width + line.left)
  }
  return Object.freeze({
    runs: Object.freeze(output),
    contentWidth,
    contentHeight,
    layoutBounds,
    rotationDegrees,
  })
}

function drawRichTextLayout(
  context: CanvasRenderingContext2D,
  plan: RichTextLayoutPlan,
): void {
  context.save()
  if (plan.rotationDegrees !== 0) {
    const centerX = toPixels(plan.layoutBounds.x + plan.layoutBounds.width / 2)
    const centerY = toPixels(plan.layoutBounds.y + plan.layoutBounds.height / 2)
    context.translate(centerX, centerY)
    context.rotate(plan.rotationDegrees * Math.PI / 180)
    context.translate(-centerX, -centerY)
  }
  context.textAlign = 'left'
  context.textBaseline = 'alphabetic'
  for (const run of plan.runs) {
    context.font = run.font.css
    context.fillStyle = cssColor(run.color)
    context.direction = run.direction
    context.textAlign = run.direction === 'rtl' ? 'right' : 'left'
    const start = run.direction === 'rtl' ? run.x + run.width : run.x
    const spaced = context as CanvasRenderingContext2D & { letterSpacing?: string }
    if (spaced.letterSpacing !== undefined) spaced.letterSpacing = `${run.characterSpacing}px`
    context.fillText(run.text, start, run.baseline)
    if (spaced.letterSpacing !== undefined) spaced.letterSpacing = '0px'
    if (run.underline || run.strike) {
      context.strokeStyle = cssColor(run.color)
      context.lineWidth = Math.max(1, run.fontSize / 16)
      if (run.underline) {
        context.beginPath()
        context.moveTo(run.x, run.baseline + run.fontSize * 0.1)
        context.lineTo(run.x + run.width, run.baseline + run.fontSize * 0.1)
        context.stroke()
      }
      if (run.strike) {
        context.beginPath()
        context.moveTo(run.x, run.baseline - run.fontSize * 0.3)
        context.lineTo(run.x + run.width, run.baseline - run.fontSize * 0.3)
        context.stroke()
      }
    }
  }
  context.restore()
}

function spacingPixels(value: number | undefined): number {
  if (value === undefined) return 0
  return pointsToCssPixels(value / 100)
}

function applyLineSpacing(height: number, value: number | undefined): number {
  if (value === undefined) return height
  return value >= 10_000 ? height * value / 100_000 : pointsToCssPixels(value / 100)
}

function scaleCssFont(css: string, scale: number): string {
  return css.replace(/([0-9.]+)px/, (_match, size: string) => `${Number(size) * scale}px`)
}

export interface DecodedImage {
  readonly source: CanvasImageSource
  readonly residentBytes: number
  close?(): void
}

export interface RasterImageMetadata {
  readonly format: 'png' | 'jpeg' | 'gif' | 'svg'
  readonly width: number
  readonly height: number
  readonly orientation: number
}

export interface RasterDecodeLimits {
  readonly maxBytes?: number
  readonly maxPixels?: number
}

/** Reads PNG/JPEG dimensions and JPEG EXIF orientation without decoding pixels. */
export function inspectRasterImageMetadata(input: ArrayBuffer | Uint8Array): RasterImageMetadata {
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input)
  if (bytes.byteLength >= 24 && bytes[0] === 0x89 && String.fromCharCode(...bytes.subarray(1, 4)) === 'PNG') {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    return { format: 'png', width: view.getUint32(16), height: view.getUint32(20), orientation: 1 }
  }
  if (bytes.byteLength >= 10 && new TextDecoder('ascii').decode(bytes.subarray(0, 6)).match(/^GIF8[79]a$/u)) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    return { format: 'gif', width: view.getUint16(6, true), height: view.getUint16(8, true), orientation: 1 }
  }
  const prefix = new TextDecoder('utf-8').decode(bytes.subarray(0, Math.min(bytes.byteLength, 1024 * 1024))).trimStart()
  if (prefix.startsWith('<svg') || prefix.startsWith('<?xml') && prefix.includes('<svg')) {
    if (/<(?:script|foreignObject)\b|\b(?:href|src)\s*=\s*["'](?:https?:|data:|javascript:)/iu.test(prefix)) {
      throw new Error('SVG resource contains active or external content')
    }
    const svg = prefix.match(/<svg\b[^>]*>/iu)?.[0] ?? ''
    const width = svg.match(/\bwidth\s*=\s*["']([0-9.]+)/iu)?.[1]
    const height = svg.match(/\bheight\s*=\s*["']([0-9.]+)/iu)?.[1]
    const viewBox = svg.match(/\bviewBox\s*=\s*["'][^"']*?([0-9.]+)[ ,]+([0-9.]+)["']/iu)
    const resolvedWidth = Number(width ?? viewBox?.[1] ?? 0)
    const resolvedHeight = Number(height ?? viewBox?.[2] ?? 0)
    if (!(resolvedWidth > 0) || !(resolvedHeight > 0)) throw new Error('SVG dimensions are missing')
    return { format: 'svg', width: resolvedWidth, height: resolvedHeight, orientation: 1 }
  }
  if (bytes.byteLength < 4 || bytes[0] !== 0xff || bytes[1] !== 0xd8) {
    throw new Error('resource is not a supported PNG or JPEG image')
  }
  let offset = 2
  let width = 0
  let height = 0
  let orientation = 1
  while (offset + 4 <= bytes.byteLength) {
    if (bytes[offset] !== 0xff) { offset += 1; continue }
    const marker = bytes[offset + 1]!
    offset += 2
    if (marker === 0xd9 || marker === 0xda) break
    const length = (bytes[offset]! << 8) | bytes[offset + 1]!
    if (length < 2 || offset + length > bytes.byteLength) throw new Error('JPEG segment is truncated')
    if (marker === 0xe1) orientation = jpegExifOrientation(bytes.subarray(offset + 2, offset + length))
    if ((marker >= 0xc0 && marker <= 0xc3) || (marker >= 0xc5 && marker <= 0xc7) || (marker >= 0xc9 && marker <= 0xcb) || (marker >= 0xcd && marker <= 0xcf)) {
      height = (bytes[offset + 3]! << 8) | bytes[offset + 4]!
      width = (bytes[offset + 5]! << 8) | bytes[offset + 6]!
    }
    offset += length
  }
  if (width <= 0 || height <= 0) throw new Error('JPEG dimensions are missing')
  return { format: 'jpeg', width, height, orientation }
}

/** Bounded raster decoder; browsers apply EXIF orientation while creating the bitmap. */
export async function decodeRasterImage(
  input: ArrayBuffer | Uint8Array,
  limits: RasterDecodeLimits = {},
  signal: AbortSignal = new AbortController().signal,
): Promise<DecodedImage> {
  throwIfAborted(signal)
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input)
  const maxBytes = limits.maxBytes ?? 32 * 1024 * 1024
  const maxPixels = limits.maxPixels ?? 64 * 1024 * 1024
  if (bytes.byteLength > maxBytes) throw new RangeError(`image exceeds the ${maxBytes}-byte decode limit`)
  const metadata = inspectRasterImageMetadata(bytes)
  if (metadata.width * metadata.height > maxPixels) {
    throw new RangeError(`image exceeds the ${maxPixels}-pixel decode limit`)
  }
  const owned = bytes.slice()
  const source = await createImageBitmap(new Blob([owned], {
    type: metadata.format === 'png'
      ? 'image/png'
      : metadata.format === 'jpeg'
        ? 'image/jpeg'
        : metadata.format === 'gif'
          ? 'image/gif'
          : 'image/svg+xml',
  }), { imageOrientation: 'from-image' })
  throwIfAborted(signal)
  return {
    source,
    residentBytes: metadata.width * metadata.height * 4,
    close: () => source.close(),
  }
}

function jpegExifOrientation(bytes: Uint8Array): number {
  if (bytes.byteLength < 14 || new TextDecoder().decode(bytes.subarray(0, 6)) !== 'Exif\0\0') return 1
  const little = bytes[6] === 0x49 && bytes[7] === 0x49
  if (!little && !(bytes[6] === 0x4d && bytes[7] === 0x4d)) return 1
  const view = new DataView(bytes.buffer, bytes.byteOffset + 6, bytes.byteLength - 6)
  const u16 = (offset: number): number => view.getUint16(offset, little)
  const u32 = (offset: number): number => view.getUint32(offset, little)
  const directory = u32(4)
  if (directory + 2 > view.byteLength) return 1
  const entries = u16(directory)
  for (let index = 0; index < entries; index += 1) {
    const entry = directory + 2 + index * 12
    if (entry + 12 > view.byteLength) break
    if (u16(entry) === 0x0112) return Math.max(1, Math.min(8, u16(entry + 8)))
  }
  return 1
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
  readonly cacheBytes: { readonly decodedImages: number }
  readonly cacheHitRate: { readonly decodedImages: number }
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
  readonly #imageInflight = new Map<string, Promise<DecodedImage | undefined>>()

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
    const richTextCommands = scene.commands.filter(
      (command): command is Extract<SceneCommand, { readonly kind: 'draw-rich-text' }> =>
        command.kind === 'draw-rich-text',
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
    const richTextLayouts = await Promise.all(
      richTextCommands.map((command) => buildRichTextLayout(context, command, fontResolver)),
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
      let richTextIndex = 0
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
            if (command.geometry === 4) drawLineEnds(context, command.transform, command)
            break
          case 'fill-gradient-preset':
            drawPreset(context, command.geometry, command.transform, () => {
              context.fillStyle = canvasGradient(context, command.transform.bounds, command.angle, command.stops)
              context.fill()
            })
            break
          case 'fill-radial-gradient-preset':
            drawPreset(context, command.geometry, command.transform, () => {
              const width = toPixels(command.transform.bounds.width)
              const height = toPixels(command.transform.bounds.height)
              const radius = Math.max(width, height) / 2
              const gradient = context.createRadialGradient(width / 2, height / 2, 0, width / 2, height / 2, radius)
              for (const stop of command.stops) gradient.addColorStop(stop.position / 100_000, cssColor(stop.color))
              context.fillStyle = gradient
              context.fill()
            })
            break
          case 'fill-pattern-preset':
            drawPreset(context, command.geometry, command.transform, () => {
              drawPatternFill(
                context,
                toPixels(command.transform.bounds.width),
                toPixels(command.transform.bounds.height),
                command.preset,
                command.foreground,
                command.background,
              )
            })
            break
          case 'draw-custom-path':
            drawCustomPath(context, command)
            break
          case 'draw-outer-shadow':
            drawOuterShadow(context, command)
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
          case 'draw-rich-text':
            drawRichTextLayout(context, richTextLayouts[richTextIndex]!)
            richTextIndex += 1
            break
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
      cacheBytes: Object.freeze({ decodedImages: this.#images.residentBytes }),
      cacheHitRate: Object.freeze({ decodedImages: this.#images.hitRate }),
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
        let loading = this.#imageInflight.get(key)
        if (loading === undefined) {
          loading = resolver(image, signal).then((decoded) => {
            throwIfAborted(signal)
            if (decoded !== undefined) this.#images.set(key, decoded, decoded.residentBytes)
            return decoded
          }).finally(() => this.#imageInflight.delete(key))
          this.#imageInflight.set(key, loading)
        }
        const decoded = await loading
        throwIfAborted(signal)
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

export interface OffscreenThumbnailResult {
  readonly bitmap: ImageBitmap
  readonly telemetry: RenderTelemetry
  readonly width: number
  readonly height: number
}

/** Worker-safe thumbnail path with a scalar fallback signal for hosts without OffscreenCanvas. */
export async function renderOffscreenThumbnail(
  scene: DisplayScene,
  maximumWidth = 320,
  options: CanvasRenderOptions = {},
  renderer = new CanvasDisplayListRenderer(),
): Promise<OffscreenThumbnailResult> {
  if (typeof OffscreenCanvas === 'undefined') {
    throw new Error('OffscreenCanvas is unavailable; render the same scene on the main thread')
  }
  const ratio = Math.min(1, maximumWidth / (scene.width / EMU_PER_CSS_PIXEL))
  const width = Math.max(1, Math.round(scene.width / EMU_PER_CSS_PIXEL * ratio))
  const height = Math.max(1, Math.round(scene.height / EMU_PER_CSS_PIXEL * ratio))
  const canvas = new OffscreenCanvas(width, height)
  const offscreen = canvas.getContext('2d')
  if (offscreen === null) throw new Error('OffscreenCanvas 2D is unavailable')
  const telemetry = await renderer.render(
    scene,
    offscreen as unknown as CanvasRenderingContext2D,
    options,
  )
  return Object.freeze({ bitmap: canvas.transferToImageBitmap(), telemetry, width, height })
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

  get sceneCacheTelemetry(): Readonly<{ residentBytes: number; entries: number; hitRate: number }> {
    return Object.freeze({
      residentBytes: this.#sceneCache.residentBytes,
      entries: this.#sceneCache.size,
      hitRate: this.#sceneCache.hitRate,
    })
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
    await Promise.all(renderTasks)
    if (!this.#stale(revision, signal)) {
      void Promise.all([...neighbors].map((index) => this.#scene(index, signal).catch(() => undefined)))
    }
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
  #hits = 0
  #misses = 0

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

  get hits(): number { return this.#hits }

  get misses(): number { return this.#misses }

  get hitRate(): number {
    const requests = this.#hits + this.#misses
    return requests === 0 ? 0 : this.#hits / requests
  }

  get(key: Key): Value | undefined {
    const entry = this.#entries.get(key)
    if (entry === undefined) {
      this.#misses += 1
      return undefined
    }
    this.#hits += 1
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
    this.#hits = 0
    this.#misses = 0
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
      const headEnd = version >= 4 ? lineEnd(reader.u8()) : undefined
      const tailEnd = version >= 4 ? lineEnd(reader.u8()) : undefined
      return { ...command, dash: command.dash === '' ? undefined : command.dash, headEnd, tailEnd }
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
      return { kind: 'draw-text', text, bounds, style: readTextStyle(reader, version) }
    }
    case 8:
      return {
        kind: 'draw-unsupported',
        transform: readTransform(reader),
        feature: unsupportedFeature(reader.u8()),
      }
    case 9: {
      if (version < 4) throw new Error('rich text requires display-list version 4')
      const bounds = readRect(reader)
      const verticalAlignment = textVerticalAlignment(reader.u8())
      const marginLeft = reader.safeI64('text-frame left margin')
      const marginTop = reader.safeI64('text-frame top margin')
      const marginRight = reader.safeI64('text-frame right margin')
      const marginBottom = reader.safeI64('text-frame bottom margin')
      const wrap = reader.u8() !== 0
      const autofitCode = reader.u8()
      if (autofitCode > 2) throw new Error('display list contains an unknown text autofit mode')
      const flowCode = version >= 5 ? reader.u8() : 0
      if (flowCode > 2) throw new Error('display list contains an unknown text flow')
      const paragraphs: SceneParagraph[] = []
      const paragraphCount = reader.boundedCount('paragraph')
      for (let paragraphIndex = 0; paragraphIndex < paragraphCount; paragraphIndex += 1) {
        const alignment = textAlignment(reader.u8())
        const rawBullet = reader.utf8Blob()
        const level = reader.u8()
        const paragraphMarginLeft = reader.safeI64('paragraph left margin')
        const indent = reader.safeI64('paragraph indent')
        const lineSpacing = optionalI32(reader)
        const spaceBefore = optionalI32(reader)
        const spaceAfter = optionalI32(reader)
        const directionCode = version >= 5 ? reader.u8() : 0
        if (directionCode > 1) throw new Error('display list contains an unknown text direction')
        const tabs: SceneTextTab[] = []
        const tabCount = version >= 5 ? reader.boundedCount('text tab') : 0
        for (let tabIndex = 0; tabIndex < tabCount; tabIndex += 1) {
          const position = reader.safeI64('text tab position')
          const alignmentCode = reader.u8()
          if (alignmentCode > 3) throw new Error('display list contains an unknown tab alignment')
          tabs.push({
            position,
            alignment: ['left', 'center', 'right', 'decimal'][alignmentCode] as SceneTextTab['alignment'],
          })
        }
        const runs: SceneTextRun[] = []
        const runCount = reader.boundedCount('text run')
        for (let runIndex = 0; runIndex < runCount; runIndex += 1) {
          const text = reader.utf8Blob()
          const style = readTextStyle(reader, version)
          const eastAsianFontFamily = reader.utf8Blob()
          const complexScriptFontFamily = reader.utf8Blob()
          runs.push({
            text,
            style,
            eastAsianFontFamily: eastAsianFontFamily || undefined,
            complexScriptFontFamily: complexScriptFontFamily || undefined,
          })
        }
        paragraphs.push({
          runs,
          alignment,
          bullet: rawBullet || undefined,
          level,
          marginLeft: paragraphMarginLeft,
          indent,
          lineSpacing,
          spaceBefore,
          spaceAfter,
          direction: directionCode === 1 ? 'rtl' : 'ltr',
          tabs,
        })
      }
      return {
        kind: 'draw-rich-text',
        bounds,
        frame: {
          paragraphs,
          verticalAlignment,
          marginLeft,
          marginTop,
          marginRight,
          marginBottom,
          wrap,
          autofit: autofitCode === 1 ? 'shrink-text' : autofitCode === 2 ? 'resize-shape' : 'none',
          flow: flowCode === 1 ? 'vertical' : flowCode === 2 ? 'vertical-270' : 'horizontal',
        },
      }
    }
    case 10:
      return {
        kind: 'fill-gradient-preset',
        geometry: reader.u8(),
        transform: readTransform(reader),
        angle: reader.i32(),
        stops: readGradientStops(reader),
      }
    case 11: {
      const transform = readTransform(reader)
      const pathWidth = reader.safeI64('custom path width')
      const pathHeight = reader.safeI64('custom path height')
      const commandCount = reader.boundedCount('custom path command')
      const path: ScenePathCommand[] = []
      for (let index = 0; index < commandCount; index += 1) {
        const kind = reader.u8()
        if (kind === 3) path.push({ kind: 'close' })
        else if (kind === 1 || kind === 2) {
          path.push({
            kind: kind === 1 ? 'move-to' : 'line-to',
            x: reader.safeI64('custom path x'),
            y: reader.safeI64('custom path y'),
          })
        } else if (version >= 5 && kind === 4) {
          path.push({
            kind: 'quadratic-to',
            controlX: reader.safeI64('quadratic control x'),
            controlY: reader.safeI64('quadratic control y'),
            x: reader.safeI64('quadratic end x'),
            y: reader.safeI64('quadratic end y'),
          })
        } else if (version >= 5 && kind === 5) {
          path.push({
            kind: 'cubic-to',
            control1X: reader.safeI64('cubic control 1 x'),
            control1Y: reader.safeI64('cubic control 1 y'),
            control2X: reader.safeI64('cubic control 2 x'),
            control2Y: reader.safeI64('cubic control 2 y'),
            x: reader.safeI64('cubic end x'),
            y: reader.safeI64('cubic end y'),
          })
        } else if (version >= 5 && kind === 6) {
          path.push({
            kind: 'arc-to',
            widthRadius: reader.safeI64('arc width radius'),
            heightRadius: reader.safeI64('arc height radius'),
            startAngle: reader.i32(),
            sweepAngle: reader.i32(),
          })
        } else throw new Error('display list contains an unknown custom path command')
      }
      const fill = readFill(reader)
      const stroke = reader.u8() === 0 ? undefined : readStroke(reader)
      return { kind: 'draw-custom-path', transform, pathWidth, pathHeight, path, fill, stroke }
    }
    case 12:
      return {
        kind: 'draw-outer-shadow',
        geometry: reader.u8(),
        transform: readTransform(reader),
        color: readColor(reader),
        blurRadius: reader.safeI64('shadow blur radius'),
        distance: reader.safeI64('shadow distance'),
        direction: reader.i32(),
      }
    case 13:
      if (version < 5) throw new Error('radial gradient requires display-list version 5')
      return {
        kind: 'fill-radial-gradient-preset',
        geometry: reader.u8(),
        transform: readTransform(reader),
        stops: readGradientStops(reader),
      }
    case 14:
      if (version < 5) throw new Error('pattern fill requires display-list version 5')
      return {
        kind: 'fill-pattern-preset',
        geometry: reader.u8(),
        transform: readTransform(reader),
        preset: reader.utf8Blob(),
        foreground: readColor(reader),
        background: readColor(reader),
      }
    default:
      throw new Error('display list contains an unknown command')
  }
}

function readGradientStops(reader: BinaryReader): readonly SceneGradientStop[] {
  const stops: SceneGradientStop[] = []
  const count = reader.boundedCount('gradient stop')
  for (let index = 0; index < count; index += 1) {
    stops.push({ position: reader.i32(), color: readColor(reader) })
  }
  return stops
}

function readFill(reader: BinaryReader): SceneFill {
  const kind = reader.u8()
  if (kind === 0) return { kind: 'none' }
  if (kind === 1) return { kind: 'solid', color: readColor(reader) }
  if (kind === 2) return { kind: 'linear-gradient', angle: reader.i32(), stops: readGradientStops(reader) }
  if (kind === 3) return { kind: 'radial-gradient', stops: readGradientStops(reader) }
  if (kind === 4) return {
    kind: 'pattern',
    preset: reader.utf8Blob(),
    foreground: readColor(reader),
    background: readColor(reader),
  }
  throw new Error('display list contains an unknown fill')
}

function readStroke(reader: BinaryReader): NonNullable<Extract<SceneCommand, { readonly kind: 'draw-custom-path' }>['stroke']> {
  const color = readColor(reader)
  const width = reader.safeI64('stroke width')
  const dash = reader.utf8Blob()
  return {
    color,
    width,
    dash: dash || undefined,
    headEnd: lineEnd(reader.u8()),
    tailEnd: lineEnd(reader.u8()),
  }
}

function lineEnd(value: number): SceneLineEnd | undefined {
  if (value === 0) return undefined
  if (value === 1) return 'triangle'
  if (value === 2) return 'stealth'
  if (value === 3) return 'diamond'
  if (value === 4) return 'oval'
  if (value === 5) return 'arrow'
  throw new Error('display list contains an unknown line end')
}

function readTextStyle(reader: BinaryReader, version: number): SceneTextStyle {
  const fontSize = reader.i32()
  const color = readColor(reader)
  const fontFamily = reader.utf8Blob()
  const style = {
    fontSize,
    color,
    fontFamily: fontFamily || undefined,
    bold: reader.u8() !== 0,
    italic: reader.u8() !== 0,
    alignment: textAlignment(reader.u8()),
    verticalAlignment: textVerticalAlignment(reader.u8()),
    marginLeft: reader.safeI64('text left margin'),
    marginTop: reader.safeI64('text top margin'),
    marginRight: reader.safeI64('text right margin'),
    marginBottom: reader.safeI64('text bottom margin'),
  }
  return {
    ...style,
    underline: version >= 5 ? reader.u8() !== 0 : false,
    strike: version >= 5 ? reader.u8() !== 0 : false,
    characterSpacing: version >= 5 ? reader.i32() : 0,
    baseline: version >= 5 ? reader.i32() : 0,
  }
}

function optionalI32(reader: BinaryReader): number | undefined {
  const value = reader.i32()
  return value === -0x8000_0000 ? undefined : value
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
    underline: false,
    strike: false,
    characterSpacing: 0,
    baseline: 0,
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
  let opening = ''
  for (const character of text) {
    if (character === '\n') {
      if (latin !== '') tokens.push(latin)
      latin = ''
      if (opening !== '') tokens.push(opening)
      opening = ''
      tokens.push(character)
    } else if (character === '\t') {
      if (latin !== '') tokens.push(latin)
      latin = ''
      if (opening !== '') tokens.push(opening)
      opening = ''
      tokens.push(character)
    } else if (/\s/u.test(character)) {
      latin += character
    } else if (/\p{Script=Han}|\p{Script=Hangul}|\p{Script=Hiragana}|\p{Script=Katakana}/u.test(character)) {
      if (latin !== '') tokens.push(latin)
      latin = ''
      if (/[（［｛〈《「『【〔〖〘〚]/u.test(character)) {
        opening += character
      } else if (/[）］｝〉》」』】〕〗〙〛、。，．？！：；]/u.test(character) && tokens.length > 0) {
        tokens[tokens.length - 1] += opening + character
        opening = ''
      } else {
        tokens.push(opening + character)
        opening = ''
      }
    } else {
      latin += opening + character
      opening = ''
    }
  }
  if (opening !== '') latin += opening
  if (latin !== '') tokens.push(latin)
  return tokens
}

function characterGapCount(text: string): number {
  return Math.max(0, Array.from(text).length - 1)
}

function nextTabWidth(current: number, tabs: readonly SceneTextTab[]): number {
  const stop = tabs
    .map((tab) => toPixels(tab.position))
    .filter((position) => position > current)
    .sort((left, right) => left - right)[0]
  if (stop !== undefined) return stop - current
  const defaultInterval = toPixels(457_200)
  return Math.max(1, (Math.floor(current / defaultInterval) + 1) * defaultInterval - current)
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

function canvasGradient(
  context: CanvasRenderingContext2D,
  bounds: EmuRect,
  angle: number,
  stops: readonly SceneGradientStop[],
): CanvasGradient {
  const radians = angle / 60_000 * Math.PI / 180
  const centerX = toPixels(bounds.x + bounds.width / 2)
  const centerY = toPixels(bounds.y + bounds.height / 2)
  const radius = Math.hypot(toPixels(bounds.width), toPixels(bounds.height)) / 2
  const dx = Math.cos(radians) * radius
  const dy = Math.sin(radians) * radius
  const gradient = context.createLinearGradient(centerX - dx, centerY - dy, centerX + dx, centerY + dy)
  for (const stop of stops) gradient.addColorStop(Math.max(0, Math.min(1, stop.position / 100_000)), cssColor(stop.color))
  return gradient
}

function drawCustomPath(
  context: CanvasRenderingContext2D,
  command: Extract<SceneCommand, { readonly kind: 'draw-custom-path' }>,
): void {
  const bounds = command.transform.bounds
  context.save()
  context.translate(toPixels(bounds.x), toPixels(bounds.y))
  context.scale(toPixels(bounds.width) / command.pathWidth, toPixels(bounds.height) / command.pathHeight)
  context.beginPath()
  let currentX = 0
  let currentY = 0
  for (const part of command.path) {
    if (part.kind === 'move-to') {
      context.moveTo(part.x, part.y)
      currentX = part.x
      currentY = part.y
    } else if (part.kind === 'line-to') {
      context.lineTo(part.x, part.y)
      currentX = part.x
      currentY = part.y
    } else if (part.kind === 'quadratic-to') {
      context.quadraticCurveTo(part.controlX, part.controlY, part.x, part.y)
      currentX = part.x
      currentY = part.y
    } else if (part.kind === 'cubic-to') {
      context.bezierCurveTo(part.control1X, part.control1Y, part.control2X, part.control2Y, part.x, part.y)
      currentX = part.x
      currentY = part.y
    } else if (part.kind === 'arc-to') {
      const start = part.startAngle / 60_000 * Math.PI / 180
      const sweep = part.sweepAngle / 60_000 * Math.PI / 180
      const centerX = currentX - part.widthRadius * Math.cos(start)
      const centerY = currentY - part.heightRadius * Math.sin(start)
      context.ellipse(centerX, centerY, part.widthRadius, part.heightRadius, 0, start, start + sweep, sweep < 0)
      currentX = centerX + part.widthRadius * Math.cos(start + sweep)
      currentY = centerY + part.heightRadius * Math.sin(start + sweep)
    } else context.closePath()
  }
  if (command.fill.kind !== 'none') {
    let patternPainted = false
    if (command.fill.kind === 'solid') context.fillStyle = cssColor(command.fill.color)
    else if (command.fill.kind === 'linear-gradient') {
      const gradient = context.createLinearGradient(0, 0, command.pathWidth, 0)
      for (const stop of command.fill.stops) gradient.addColorStop(stop.position / 100_000, cssColor(stop.color))
      context.fillStyle = gradient
    } else if (command.fill.kind === 'radial-gradient') {
      const radius = Math.max(command.pathWidth, command.pathHeight) / 2
      const gradient = context.createRadialGradient(command.pathWidth / 2, command.pathHeight / 2, 0, command.pathWidth / 2, command.pathHeight / 2, radius)
      for (const stop of command.fill.stops) gradient.addColorStop(stop.position / 100_000, cssColor(stop.color))
      context.fillStyle = gradient
    } else {
      drawPatternFill(
        context,
        command.pathWidth,
        command.pathHeight,
        command.fill.preset,
        command.fill.foreground,
        command.fill.background,
      )
      patternPainted = true
    }
    if (!patternPainted) context.fill()
  }
  if (command.stroke !== undefined) {
    context.strokeStyle = cssColor(command.stroke.color)
    context.lineWidth = command.stroke.width * command.pathWidth / Math.max(1, bounds.width)
    context.setLineDash(dashPattern(command.stroke.dash, context.lineWidth))
    context.stroke()
  }
  context.restore()
}

function drawPatternFill(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
  preset: string,
  foreground: RgbaColor,
  background: RgbaColor,
): void {
  context.fillStyle = cssColor(background)
  context.fill()
  context.save()
  context.clip()
  context.strokeStyle = cssColor(foreground)
  context.lineWidth = Math.max(1, Math.min(Math.abs(width), Math.abs(height)) / 160)
  const step = Math.max(4, Math.min(Math.abs(width), Math.abs(height)) / 12)
  const vertical = /Vert|vert/u.test(preset)
  const horizontal = /Horz|horz/u.test(preset)
  const reverse = /DnDiag|dnDiag/u.test(preset)
  if (vertical || horizontal) {
    if (vertical) for (let x = 0; x <= width; x += step) { context.beginPath(); context.moveTo(x, 0); context.lineTo(x, height); context.stroke() }
    if (horizontal) for (let y = 0; y <= height; y += step) { context.beginPath(); context.moveTo(0, y); context.lineTo(width, y); context.stroke() }
  } else {
    for (let offset = -height; offset <= width + height; offset += step) {
      context.beginPath()
      context.moveTo(offset, reverse ? 0 : height)
      context.lineTo(offset + height, reverse ? height : 0)
      context.stroke()
    }
  }
  context.restore()
}

function drawOuterShadow(
  context: CanvasRenderingContext2D,
  command: Extract<SceneCommand, { readonly kind: 'draw-outer-shadow' }>,
): void {
  const radians = command.direction / 60_000 * Math.PI / 180
  context.save()
  context.translate(
    Math.cos(radians) * toPixels(command.distance),
    Math.sin(radians) * toPixels(command.distance),
  )
  context.filter = `blur(${Math.max(0, toPixels(command.blurRadius))}px)`
  drawPreset(context, command.geometry, command.transform, () => {
    context.fillStyle = cssColor(command.color)
    context.fill()
  })
  context.restore()
}

function drawLineEnds(
  context: CanvasRenderingContext2D,
  transform: SceneTransform,
  command: Extract<SceneCommand, { readonly kind: 'stroke-preset' }>,
): void {
  const start = { x: toPixels(transform.bounds.x), y: toPixels(transform.bounds.y) }
  const end = {
    x: toPixels(transform.bounds.x + transform.bounds.width),
    y: toPixels(transform.bounds.y + transform.bounds.height),
  }
  const angle = Math.atan2(end.y - start.y, end.x - start.x)
  if (command.headEnd !== undefined) drawLineEnd(context, start.x, start.y, angle + Math.PI, command.headEnd, command)
  if (command.tailEnd !== undefined) drawLineEnd(context, end.x, end.y, angle, command.tailEnd, command)
}

function drawLineEnd(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  angle: number,
  kind: SceneLineEnd,
  command: Extract<SceneCommand, { readonly kind: 'stroke-preset' }>,
): void {
  const size = Math.max(4, toPixels(command.width) * 4)
  context.save()
  context.translate(x, y)
  context.rotate(angle)
  context.fillStyle = cssColor(command.color)
  context.beginPath()
  if (kind === 'oval') context.ellipse(-size / 2, 0, size / 2, size / 3, 0, 0, Math.PI * 2)
  else if (kind === 'diamond') {
    context.moveTo(0, 0); context.lineTo(-size / 2, size / 3); context.lineTo(-size, 0); context.lineTo(-size / 2, -size / 3); context.closePath()
  } else {
    context.moveTo(0, 0); context.lineTo(-size, size / 2); context.lineTo(-size * 0.7, 0); context.lineTo(-size, -size / 2); context.closePath()
  }
  context.fill()
  context.restore()
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
  } else if (geometry === 10 || geometry === 11 || geometry === 12) {
    const points = geometry === 10 ? 5 : geometry === 11 ? 8 : 10
    for (let index = 0; index < points; index += 1) {
      const angle = -Math.PI / 2 + index * Math.PI * 2 / points
      const radius = geometry === 12 && index % 2 === 1 ? 0.22 : 0.5
      const x = width / 2 + Math.cos(angle) * width * radius
      const y = height / 2 + Math.sin(angle) * height * radius
      if (index === 0) context.moveTo(x, y)
      else context.lineTo(x, y)
    }
    context.closePath()
  } else if (geometry === 13) {
    context.moveTo(width * 0.35, 0); context.lineTo(width * 0.65, 0)
    context.lineTo(width * 0.65, height * 0.35); context.lineTo(width, height * 0.35)
    context.lineTo(width, height * 0.65); context.lineTo(width * 0.65, height * 0.65)
    context.lineTo(width * 0.65, height); context.lineTo(width * 0.35, height)
    context.lineTo(width * 0.35, height * 0.65); context.lineTo(0, height * 0.65)
    context.lineTo(0, height * 0.35); context.lineTo(width * 0.35, height * 0.35); context.closePath()
  } else if (geometry === 14) {
    context.moveTo(0, 0); context.lineTo(width * 0.65, 0); context.lineTo(width, height / 2)
    context.lineTo(width * 0.65, height); context.lineTo(0, height); context.lineTo(width * 0.35, height / 2); context.closePath()
  } else if (geometry === 15 || geometry === 16) {
    const direction = geometry === 15 ? 1 : -1
    context.translate(direction === 1 ? 0 : width, 0); context.scale(direction, 1)
    context.moveTo(0, height * 0.3); context.lineTo(width * 0.6, height * 0.3)
    context.lineTo(width * 0.6, 0); context.lineTo(width, height / 2)
    context.lineTo(width * 0.6, height); context.lineTo(width * 0.6, height * 0.7)
    context.lineTo(0, height * 0.7); context.closePath()
  } else if (geometry === 17 || geometry === 18) {
    const direction = geometry === 18 ? -1 : 1
    context.translate(0, direction === 1 ? 0 : height); context.scale(1, direction)
    context.moveTo(width * 0.3, height); context.lineTo(width * 0.3, height * 0.4)
    context.lineTo(0, height * 0.4); context.lineTo(width / 2, 0)
    context.lineTo(width, height * 0.4); context.lineTo(width * 0.7, height * 0.4)
    context.lineTo(width * 0.7, height); context.closePath()
  } else if (geometry === 19) {
    context.moveTo(width * 0.2, 0); context.lineTo(width * 0.8, 0)
    context.lineTo(width, height); context.lineTo(0, height); context.closePath()
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
