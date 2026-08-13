import type { FontByteShaper, ShapedFontRun } from './shaper.js'
import { ByteBudgetLru } from './cache/byte-budget-lru.js'
import {
  EMU_PER_CSS_PIXEL,
  groupTransformMatrix,
  presetGeometryPath,
  projectPresetGeometryToCanvas,
  shapeTransformMatrix,
  toCssPixels,
} from './scene/geometry.js'

export { ByteBudgetLru } from './cache/byte-budget-lru.js'

export interface RgbaColor {
  readonly red: number
  readonly green: number
  readonly blue: number
  readonly alpha: number
}

export interface EmuRect {
  /** Left edge in English Metric Units (914,400 EMU per inch). */
  readonly x: number
  /** Top edge in English Metric Units (914,400 EMU per inch). */
  readonly y: number
  /** Width in English Metric Units. */
  readonly width: number
  /** Height in English Metric Units. */
  readonly height: number
}

export interface SceneTransform {
  readonly bounds: EmuRect
  /** Clockwise rotation in OOXML 1/60000-degree units. */
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

export interface SceneEmbeddedFont {
  readonly family: string
  readonly style: 'regular' | 'bold' | 'italic' | 'bold-italic'
  readonly partName: string
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
  readonly outline?: { readonly color: RgbaColor; readonly width: number; readonly dash?: string }
  readonly shadow?: { readonly color: RgbaColor; readonly blurRadius: number; readonly distance: number; readonly direction: number }
  readonly innerShadow?: { readonly color: RgbaColor; readonly blurRadius: number; readonly distance: number; readonly direction: number }
  readonly fill?: SceneFill
  readonly glow?: { readonly color: RgbaColor; readonly radius: number }
  readonly blurRadius: number
  readonly softEdgeRadius: number
  readonly reflection: boolean
  readonly alignment: 'left' | 'center' | 'right' | 'justify' | 'distributed'
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
  readonly bulletImageResource?: number
  readonly bulletStyle?: SceneTextStyle
  readonly level: number
  readonly marginLeft: number
  readonly indent: number
  readonly lineSpacing?: SceneTextSpacing
  readonly spaceBefore?: SceneTextSpacing
  readonly spaceAfter?: SceneTextSpacing
  readonly direction: 'ltr' | 'rtl'
  readonly tabs: readonly SceneTextTab[]
  readonly fontAlignment: 'automatic' | 'top' | 'center' | 'baseline' | 'bottom'
}

export interface SceneTextSpacing {
  readonly kind: 'percent' | 'points'
  readonly value: number
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
  readonly autofitFontScale?: number
  readonly autofitLineSpacingReduction?: number
  readonly autofitRecompute: boolean
  readonly flow: 'horizontal' | 'vertical' | 'vertical-270'
  readonly columnCount: number
  readonly columnSpacing: number
  readonly defaultTabSize: number
  readonly warp?: { readonly preset: string; readonly adjustment: number }
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
  readonly embeddedFonts: readonly SceneEmbeddedFont[]
  readonly semantics: readonly SceneSemanticElement[]
  readonly diagnostics: readonly SceneDiagnostic[]
  readonly byteLength: number
}

/**
 * Decode WPDL v1-v9 defensively before touching Canvas or DOM APIs.
 * Counts, references, safe-integer coordinates, group balance, truncation, and trailing bytes are
 * validated before a scene is returned.
 */
export function decodeDisplayList(input: ArrayBuffer | Uint8Array): DisplayScene {
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input)
  const reader = new BinaryReader(bytes)
  if (reader.ascii(4) !== 'WPDL') throw new Error('display list has an invalid magic value')
  const version = reader.u16()
  if (version !== 1 && version !== 2 && version !== 3 && version !== 4 && version !== 5 && version !== 6 && version !== 7 && version !== 8 && version !== 9) {
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
  const embeddedFontCount = version >= 7 ? reader.boundedCount('embedded font') : 0
  const semanticCount = version >= 2 ? reader.boundedCount('semantic element') : 0
  const diagnosticCount = version >= 2 ? reader.boundedCount('diagnostic') : 0
  const commands: SceneCommand[] = []
  const inlineImages: SceneImage[] = []
  for (let index = 0; index < commandCount; index += 1) {
    commands.push(readCommand(reader, version, inlineImages, imageCount))
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
  images.push(...inlineImages)
  const embeddedFonts: SceneEmbeddedFont[] = []
  for (let index = 0; index < embeddedFontCount; index += 1) {
    const family = reader.utf8Blob()
    const style = reader.u8()
    if (family === '' || style > 3) throw new Error('display list contains an invalid embedded font')
    const partName = reader.utf8Blob()
    if (partName === '') throw new Error('display list contains an empty embedded font part name')
    embeddedFonts.push({
      family,
      style: ['regular', 'bold', 'italic', 'bold-italic'][style] as SceneEmbeddedFont['style'],
      partName,
    })
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
    embeddedFonts: Object.freeze(embeddedFonts),
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
  readonly faceIndex?: number
  readonly descriptors?: FontFaceDescriptors
}

export interface ResolvedFont {
  readonly requestedFamily: string
  readonly family: string
  readonly script: FontScript
  readonly exact: boolean
  readonly css: string
  readonly sizePixels: number
  readonly shapingKey?: string
  readonly fontBytes?: Uint8Array
  readonly faceIndex?: number
}

export interface FontResolverOptions {
  readonly theme?: Partial<ThemeFontSet>
  readonly substitutions?: Readonly<Record<string, string>>
  readonly webFonts?: readonly WebFontDefinition[]
  readonly fallback?: Partial<Record<FontScript, string>>
  readonly host?: FontLoadingHost
  readonly shaper?: FontByteShaper
}

export interface FontLoadingHost {
  load(definition: WebFontDefinition): Promise<void>
  check(css: string, text: string): boolean
}

/** Resolves theme font slots deterministically and explicitly reports substitutions. */
export class FontResolver {
  readonly #theme: ThemeFontSet
  readonly #substitutions: Readonly<Record<string, string>>
  readonly #webFonts = new Map<string, WebFontDefinition[]>()
  readonly #fallback: Record<FontScript, string>
  readonly #host: FontLoadingHost
  readonly #shaper?: FontByteShaper
  readonly #loaded = new Map<string, Promise<void>>()
  readonly #resolved = new Map<string, Promise<ResolvedFont>>()

  constructor(options: FontResolverOptions = {}) {
    this.#theme = {
      latin: options.theme?.latin ?? 'Arial',
      eastAsian: options.theme?.eastAsian ?? 'Noto Sans CJK KR',
      complexScript: options.theme?.complexScript ?? 'Noto Sans Arabic',
    }
    this.#substitutions = Object.freeze({ ...options.substitutions })
    for (const font of options.webFonts ?? []) this.registerWebFont(font)
    this.#fallback = {
      latin: options.fallback?.latin ?? 'sans-serif',
      'east-asian': options.fallback?.['east-asian'] ?? 'sans-serif',
      complex: options.fallback?.complex ?? 'sans-serif',
    }
    this.#host = options.host ?? new BrowserFontLoadingHost()
    this.#shaper = options.shaper
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
    const loadedDefinitions = await this.#load(family, emphasis)
    const prefix = `${emphasis.italic === true ? 'italic' : 'normal'} ${emphasis.bold === true ? '700' : '400'}`
    const css = `${prefix} ${sizePixels}px ${quoteFontFamily(family)}`
    const exact = this.#host.check(css, representativeText(script, text))
    const fallbackFamily = this.#fallback[script]
    const resolvedFamily = exact ? family : fallbackFamily
    const cssFamily = exact
      ? `${quoteFontFamily(family)}, ${quoteFontFamily(fallbackFamily)}`
      : quoteFontFamily(fallbackFamily)
    const byteDefinition = exact
      ? loadedDefinitions.find((definition) => definition.source instanceof ArrayBuffer)
      : undefined
    return Object.freeze({
      requestedFamily: requested,
      family: resolvedFamily,
      script,
      exact,
      css: `${prefix} ${sizePixels}px ${cssFamily}`,
      sizePixels,
      ...(byteDefinition?.source instanceof ArrayBuffer
        ? {
            shapingKey: `${family}\0${byteDefinition.faceIndex ?? 0}\0${emphasis.bold === true ? 1 : 0}${emphasis.italic === true ? 1 : 0}`,
            fontBytes: new Uint8Array(byteDefinition.source),
            faceIndex: byteDefinition.faceIndex ?? 0,
          }
        : {}),
    })
  }

  /** Shapes from exact registered font bytes when the optional shaper is configured. */
  async shape(
    text: string,
    font: ResolvedFont,
    direction: 'ltr' | 'rtl' | 'ttb' | 'btt',
  ): Promise<ShapedFontRun | undefined> {
    if (this.#shaper === undefined || font.fontBytes === undefined) return undefined
    return this.#shaper.shape({
      fontBytes: font.fontBytes,
      faceIndex: font.faceIndex,
      text,
      direction,
    })
  }

  /** Returns a UAX #14 break plan when the optional text engine is installed. */
  async breakText(text: string): Promise<readonly string[]> {
    if (this.#shaper === undefined) return lineBreakTokens(text)
    const tokens = await this.#shaper.breakText(text)
    return Object.freeze(tokens.flatMap((token) =>
      /\p{Script=Thai}|\p{Script=Lao}|\p{Script=Khmer}/u.test(token)
        ? lineBreakTokens(token)
        : [token]))
  }

  #themeFamily(script: FontScript): string {
    if (script === 'east-asian') return this.#theme.eastAsian
    if (script === 'complex') return this.#theme.complexScript
    return this.#theme.latin
  }

  /** Adds a font face without rebuilding the resolver; live layouts are invalidated safely. */
  registerWebFont(definition: WebFontDefinition): void {
    const definitions = this.#webFonts.get(definition.family) ?? []
    definitions.push(definition)
    this.#webFonts.set(definition.family, definitions)
    this.#resolved.clear()
  }

  async #load(
    family: string,
    emphasis: { readonly bold?: boolean; readonly italic?: boolean },
  ): Promise<readonly WebFontDefinition[]> {
    const definitions = this.#webFonts.get(family)
    if (definitions === undefined) return []
    const weight = emphasis.bold === true ? '700' : '400'
    const style = emphasis.italic === true ? 'italic' : 'normal'
    const matching = definitions.filter((definition) =>
      (definition.descriptors?.weight ?? '400') === weight &&
      (definition.descriptors?.style ?? 'normal') === style)
    const selected = matching.length === 0 ? definitions : matching
    await Promise.all(selected.map(async (definition, index) => {
      const key = `${family}\0${weight}\0${style}\0${index}`
      let loading = this.#loaded.get(key)
      if (loading === undefined) {
        loading = this.#host.load(definition)
        this.#loaded.set(key, loading)
      }
      await loading
    }))
    return selected
  }
}

export interface EmbeddedFontLoadOptions {
  readonly maxFontBytes?: number
  readonly resource: (partName: string, signal?: AbortSignal) => Promise<ArrayBuffer | Uint8Array>
  readonly signal?: AbortSignal
}

export interface OpenTypeEmbeddingInfo {
  readonly fsType?: number
  readonly permitted: boolean
  readonly reason: 'installable' | 'preview-print' | 'editable' | 'restricted' | 'unknown'
}

/** Loads presentation font parts lazily and enforces the OpenType embedding permission bits. */
export async function registerEmbeddedFonts(
  scene: Pick<DisplayScene, 'embeddedFonts'>,
  resolver: FontResolver,
  options: EmbeddedFontLoadOptions,
): Promise<void> {
  const maximum = options.maxFontBytes ?? 32 * 1024 * 1024
  if (!Number.isSafeInteger(maximum) || maximum <= 0) throw new RangeError('maxFontBytes must be positive')
  for (const font of scene.embeddedFonts) {
    if (options.signal !== undefined) throwIfAborted(options.signal)
    const source = await options.resource(font.partName, options.signal)
    const bytes = source instanceof Uint8Array ? source : new Uint8Array(source)
    if (bytes.byteLength === 0 || bytes.byteLength > maximum) {
      throw new Error(`embedded font ${font.partName} exceeds the byte limit`)
    }
    const decoded = embeddedFontGuid(font.partName) === undefined
      ? bytes.slice()
      : decodeOoxmlObfuscatedFont(bytes, embeddedFontGuid(font.partName)!)
    const permission = inspectOpenTypeEmbedding(decoded)
    if (!permission.permitted) {
      throw new Error(`embedded font ${font.partName} prohibits preview embedding`)
    }
    resolver.registerWebFont({
      family: font.family,
      source: new Uint8Array(decoded).buffer,
      descriptors: {
        weight: font.style === 'bold' || font.style === 'bold-italic' ? '700' : '400',
        style: font.style === 'italic' || font.style === 'bold-italic' ? 'italic' : 'normal',
      },
    })
  }
}

/** Decodes the ECMA-376 first-32-byte GUID XOR font obfuscation in a bounded copy. */
export function decodeOoxmlObfuscatedFont(
  input: ArrayBuffer | Uint8Array,
  guid: string,
): Uint8Array {
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input)
  const hex = guid.replace(/[{}-]/gu, '')
  if (!/^[0-9a-f]{32}$/iu.test(hex)) throw new Error('embedded font GUID is invalid')
  const key = Array.from({ length: 16 }, (_, index) => {
    const sourceIndex = 15 - index
    return Number.parseInt(hex.slice(sourceIndex * 2, sourceIndex * 2 + 2), 16)
  })
  const output = bytes.slice()
  for (let index = 0; index < Math.min(32, output.byteLength); index += 1) {
    output[index] = output[index]! ^ key[index % 16]!
  }
  return output
}

/** Reads OS/2.fsType without allocating tables or trusting font offsets. */
export function inspectOpenTypeEmbedding(input: ArrayBuffer | Uint8Array): OpenTypeEmbeddingInfo {
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input)
  if (bytes.byteLength < 12) return { permitted: false, reason: 'unknown' }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const tag = new TextDecoder('ascii').decode(bytes.subarray(0, 4))
  if (tag === 'ttcf') {
    const faceCount = view.getUint32(8)
    if (faceCount === 0 || faceCount > 64 || 12 + faceCount * 4 > bytes.byteLength) {
      return { permitted: false, reason: 'unknown' }
    }
    const faces = Array.from({ length: faceCount }, (_, index) =>
      inspectSfntEmbedding(bytes, view.getUint32(12 + index * 4)))
    if (faces.some((face) => !face.permitted)) {
      return faces.find((face) => !face.permitted) ?? { permitted: false, reason: 'unknown' }
    }
    return faces[0] ?? { permitted: false, reason: 'unknown' }
  }
  return inspectSfntEmbedding(bytes, 0)
}

function inspectSfntEmbedding(bytes: Uint8Array, faceOffset: number): OpenTypeEmbeddingInfo {
  if (faceOffset < 0 || faceOffset + 12 > bytes.byteLength) return { permitted: false, reason: 'unknown' }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const tableCount = view.getUint16(faceOffset + 4)
  if (tableCount > 4096 || faceOffset + 12 + tableCount * 16 > bytes.byteLength) {
    return { permitted: false, reason: 'unknown' }
  }
  for (let index = 0; index < tableCount; index += 1) {
    const offset = faceOffset + 12 + index * 16
    if (new TextDecoder('ascii').decode(bytes.subarray(offset, offset + 4)) !== 'OS/2') continue
    const tableOffset = view.getUint32(offset + 8)
    const tableLength = view.getUint32(offset + 12)
    if (tableLength < 10 || tableOffset + tableLength > bytes.byteLength) return { permitted: false, reason: 'unknown' }
    const fsType = view.getUint16(tableOffset + 8)
    if ((fsType & 0x0002) !== 0 || (fsType & 0x0200) !== 0) return { fsType, permitted: false, reason: 'restricted' }
    if ((fsType & 0x0008) !== 0) return { fsType, permitted: true, reason: 'editable' }
    if ((fsType & 0x0004) !== 0) return { fsType, permitted: true, reason: 'preview-print' }
    return { fsType, permitted: fsType === 0, reason: fsType === 0 ? 'installable' : 'unknown' }
  }
  return { permitted: false, reason: 'unknown' }
}

function embeddedFontGuid(partName: string): string | undefined {
  return partName.match(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/iu)?.[0]
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

function measureTextBatchCached(
  context: Pick<CanvasRenderingContext2D, 'font' | 'measureText'>,
  requests: readonly TextMeasureRequest[],
  cache: ByteBudgetLru<string, number>,
): readonly number[] {
  const widths = Array.from({ length: requests.length }, () => 0)
  const missing: TextMeasureRequest[] = []
  const missingIndices: number[] = []
  requests.forEach((request, index) => {
    const key = `${request.font}\0${request.text}`
    const cached = cache.get(key)
    if (cached === undefined) {
      missing.push(request)
      missingIndices.push(index)
    } else {
      widths[index] = cached
    }
  })
  const measured = measureTextBatch(context, missing)
  measured.forEach((width, index) => {
    const request = missing[index]!
    const key = `${request.font}\0${request.text}`
    cache.set(key, width, key.length * 2 + 8)
    widths[missingIndices[index]!] = width
  })
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
    if (isSoftWhitespace(token)) {
      if (line === '') continue
      if (measure(line + token) > maxWidth) {
        lines.push(line.trimEnd())
        line = ''
      } else {
        line += token
      }
      continue
    }
    const candidate = line + token
    if (line !== '' && measure(candidate) > maxWidth) {
      lines.push(line.trimEnd())
      line = ''
    }
    if (measure(token) <= maxWidth) {
      line += token
      continue
    }
    const fragments = splitTokenToFit(token, maxWidth, measure)
    for (const [index, fragment] of fragments.entries()) {
      line = fragment
      if (index + 1 < fragments.length) {
        lines.push(line)
        line = ''
      }
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
  readonly outline?: SceneTextStyle['outline']
  readonly shadow?: SceneTextStyle['shadow']
  readonly innerShadow?: SceneTextStyle['innerShadow']
  readonly fill?: SceneFill
  readonly glow?: SceneTextStyle['glow']
  readonly blurRadius: number
  readonly softEdgeRadius: number
  readonly reflection: boolean
  readonly shaped?: ShapedFontRun
  readonly warpRotation: number
  readonly bulletImageResource?: number
  readonly paragraphIndex: number
  readonly sourceStart?: number
  readonly sourceEnd?: number
}

export interface RichTextLayoutPlan {
  readonly runs: readonly RichTextLayoutRun[]
  readonly contentWidth: number
  readonly contentHeight: number
  readonly layoutBounds: EmuRect
  readonly effectiveBounds: EmuRect
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
  const layoutBounds = rotatedLayoutBounds(command.bounds, rotationDegrees)
  const innerWidth = Math.max(
    0,
    toPixels(layoutBounds.width - command.frame.marginLeft - command.frame.marginRight),
  )
  const innerHeight = Math.max(
    0,
    toPixels(layoutBounds.height - command.frame.marginTop - command.frame.marginBottom),
  )
  const columnCount = Math.max(1, Math.min(16, command.frame.columnCount ?? 1))
  const columnSpacing = Math.max(0, toPixels(command.frame.columnSpacing ?? 0))
  const columnWidth = Math.max(
    0,
    (innerWidth - columnSpacing * (columnCount - 1)) / columnCount,
  )
  const resolved = await Promise.all(
    command.frame.paragraphs.flatMap((paragraph) => {
      const first = paragraph.runs[0]
      const marker: SceneTextRun[] = (paragraph.bullet !== undefined || paragraph.bulletImageResource !== undefined) && first !== undefined
        ? [{
            text: paragraph.bullet ?? '◼',
            style: paragraph.bulletStyle ?? first.style,
            eastAsianFontFamily: first.eastAsianFontFamily,
            complexScriptFontFamily: first.complexScriptFontFamily,
          }]
        : []
      return [...marker, ...paragraph.runs].map(async (run) => {
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
      })
    }),
  )
  const breakPlans = new Map<SceneTextRun, readonly string[]>()
  await Promise.all(command.frame.paragraphs.flatMap((paragraph) =>
    paragraph.runs.map(async (run) => {
      breakPlans.set(run, command.frame.wrap ? await resolver.breakText(run.text) : [run.text])
    })))
  const measureRequests: TextMeasureRequest[] = []
  let measureFontIndex = 0
  for (const paragraph of command.frame.paragraphs) {
    if ((paragraph.bullet !== undefined || paragraph.bulletImageResource !== undefined) && paragraph.runs[0] !== undefined) {
      measureRequests.push({
        text: paragraph.bulletImageResource === undefined ? `${paragraph.bullet} ` : '◼ ',
        font: resolved[measureFontIndex]!.css,
      })
      measureFontIndex += 1
    }
    for (const run of paragraph.runs) {
      const font = resolved[measureFontIndex++]!
      for (const token of breakPlans.get(run) ?? [run.text]) {
        if (token !== '\n' && token !== '\t') measureRequests.push({ text: token, font: font.css })
      }
    }
  }
  const measured = measureTextBatch(context, measureRequests)
  const measurementLookup = new Map(
    measureRequests.map((request, index) => [`${request.font}\0${request.text}`, measured[index]!]),
  )
  const shapingLookup = new Map<string, ShapedFontRun>()
  const shapingTasks: Promise<void>[] = []
  let shapingFontIndex = 0
  for (const paragraph of command.frame.paragraphs) {
    const direction = paragraph.direction
    if ((paragraph.bullet !== undefined || paragraph.bulletImageResource !== undefined) && paragraph.runs[0] !== undefined) {
      const font = resolved[shapingFontIndex++]!
      const value = paragraph.bulletImageResource === undefined ? `${paragraph.bullet} ` : '◼ '
      shapingTasks.push(resolver.shape(value, font, direction).then((run) => {
        if (run !== undefined) shapingLookup.set(shapedLookupKey(font, direction, value), run)
      }))
    }
    for (const sourceRun of paragraph.runs) {
      const font = resolved[shapingFontIndex++]!
      for (const token of breakPlans.get(sourceRun) ?? [sourceRun.text]) {
        if (token === '\n' || token === '\t') continue
        const value = token.replaceAll('\u00AD', '')
        shapingTasks.push(resolver.shape(value, font, direction).then((run) => {
          if (run !== undefined) shapingLookup.set(shapedLookupKey(font, direction, value), run)
        }))
      }
    }
  }
  await Promise.all(shapingTasks)
  type LayoutLine = {
    readonly runs: Array<Omit<RichTextLayoutRun, 'x' | 'baseline' | 'warpRotation'>>
    readonly height: number
    readonly alignment: SceneTextStyle['alignment']
    readonly before: number
    readonly after: number
    readonly left: number
    readonly direction: 'ltr' | 'rtl'
    readonly column?: number
    readonly top?: number
    readonly lastInParagraph: boolean
    readonly fontAlignment: SceneParagraph['fontAlignment']
  }
  type ScaledLayout = {
    readonly lines: readonly LayoutLine[]
    readonly contentWidth: number
    readonly contentHeight: number
  }
  const baseTokenWidth = (
    font: ResolvedFont,
    token: string,
    characterSpacing: number,
    direction: 'ltr' | 'rtl',
  ): number => {
    const shaped = shapingLookup.get(shapedLookupKey(font, direction, token))
    if (shaped !== undefined) {
      const advance = Math.abs(shaped.glyphs.reduce((sum, glyph) => sum + glyph.xAdvance, 0))
      return advance / shaped.unitsPerEm * font.sizePixels +
        characterGapCount(token) * characterSpacing
    }
    const key = `${font.css}\0${token}`
    let width = measurementLookup.get(key)
    if (width === undefined) {
      context.font = font.css
      width = context.measureText(token).width
      measurementLookup.set(key, width)
    }
    return width + characterGapCount(token) * characterSpacing
  }
  const layoutAtScale = (scale: number): ScaledLayout => {
    let resolvedIndex = 0
    const lines: LayoutLine[] = []
    for (const [paragraphIndex, paragraph] of command.frame.paragraphs.entries()) {
      const lineRuns: Array<Omit<RichTextLayoutRun, 'x' | 'baseline' | 'warpRotation'>> = []
      let lineWidth = 0
      let lineHeight = 0
      let firstLine = true
      const baseLineHeight = paragraph.runs.reduce(
        (height, run) => Math.max(
          height,
          pointsToCssPixels(run.style.fontSize / 100) * 1.2,
        ),
        1,
      )
      const metricLineHeight = Math.max(
        baseLineHeight,
        paragraph.bulletStyle === undefined
          ? 1
          : pointsToCssPixels(paragraph.bulletStyle.fontSize / 100) * 1.2,
      )
      const paragraphBefore = spacingPixels(paragraph.spaceBefore, metricLineHeight) * scale
      const left = toPixels(paragraph.marginLeft + paragraph.indent)
      const available = Math.max(0, columnWidth - left)
      const firstRun = paragraph.runs[0]
      let inputs: readonly SceneTextRun[] = paragraph.runs
      if (
        firstRun !== undefined &&
        (paragraph.bullet !== undefined || paragraph.bulletImageResource !== undefined)
      ) {
        inputs = [{
          text: paragraph.bullet === undefined ? '◼ ' : `${paragraph.bullet} `,
          style: paragraph.bulletStyle ?? firstRun.style,
          eastAsianFontFamily: firstRun.eastAsianFontFamily,
          complexScriptFontFamily: firstRun.complexScriptFontFamily,
        }, ...paragraph.runs]
      }
      const finishLine = (after: number, lastInParagraph = false): void => {
        lines.push({
          runs: lineRuns.splice(0),
          height: applyScaledLineSpacing(
            lineHeight || scale,
            paragraph.lineSpacing,
            scale,
            command.frame.autofitLineSpacingReduction,
          ),
          alignment: paragraph.alignment,
          before: firstLine ? paragraphBefore : 0,
          after,
          left,
          direction: paragraph.direction,
          lastInParagraph,
          fontAlignment: paragraph.fontAlignment ?? 'automatic',
        })
        firstLine = false
        lineWidth = 0
        lineHeight = 0
      }
      let paragraphSourceOffset = 0
      for (const run of inputs) {
        const isBullet = (paragraph.bullet !== undefined || paragraph.bulletImageResource !== undefined) && run === inputs[0]
        const font = resolved[resolvedIndex++]!
        const baseSpacing = pointsToCssPixels(run.style.characterSpacing / 100)
        const spacing = baseSpacing * scale
        const tokens = breakPlans.get(run) ?? [run.text]
        let runSourceOffset = 0
        for (const [tokenIndex, rawToken] of tokens.entries()) {
          if (rawToken === '\n') {
            finishLine(0)
            if (!isBullet) runSourceOffset += rawToken.length
            continue
          }
          const measureToken = (value: string): number =>
            baseTokenWidth(
              font,
              value.replaceAll('\u00AD', ''),
              baseSpacing,
              paragraph.direction,
            ) * scale
          const followingTokens = rawToken === '\t' ? tokens.slice(tokenIndex + 1) : []
          const followingBoundary = followingTokens.findIndex((value) => value === '\n' || value === '\t')
          const following = followingTokens
            .slice(0, followingBoundary < 0 ? followingTokens.length : followingBoundary)
            .join('')
          let rawWidth = rawToken === '\t'
            ? nextTabWidth(
                lineWidth / scale,
                paragraph.tabs,
                command.frame.defaultTabSize,
                measureToken(following) / scale,
                measureToken(following.split(/[.,]/u)[0] ?? '') / scale,
              ) * scale
            : measureToken(rawToken)
          if (isBullet && rawToken.trim() !== '' && paragraph.indent < 0) {
            rawWidth = Math.max(rawWidth, -toPixels(paragraph.indent) * scale)
          }
          const fragments = command.frame.wrap && !isSoftWhitespace(rawToken) &&
              rawToken !== '\t' && rawWidth > available
            ? splitTokenToFit(rawToken, available, measureToken)
            : [rawToken]
          let fragmentSourceOffset = runSourceOffset
          for (const token of fragments) {
            const width = token === rawToken ? rawWidth : measureToken(token)
            const softWhitespace = isSoftWhitespace(token)
            if (command.frame.wrap && softWhitespace && lineRuns.length === 0) continue
            if (command.frame.wrap && lineRuns.length > 0 && lineWidth + width > available) {
              while (lineRuns.length > 0 && isSoftWhitespace(lineRuns[lineRuns.length - 1]!.text)) {
                lineWidth -= lineRuns.pop()!.width
              }
              lineHeight = lineRuns.reduce(
                (height, existing) => Math.max(height, existing.fontSize * 1.2),
                0,
              )
              const previous = lineRuns[lineRuns.length - 1]
              if (previous?.text.endsWith('\u00AD')) {
                context.font = previous.font.css
                const hyphenWidth = context.measureText('-').width
                lineRuns[lineRuns.length - 1] = {
                  ...previous,
                  text: `${previous.text.slice(0, -1)}-`,
                  width: previous.width + hyphenWidth,
                }
                lineWidth += hyphenWidth
              }
              finishLine(0)
              if (softWhitespace) continue
            }
            const fontSize = pointsToCssPixels(run.style.fontSize / 100) * scale
            lineRuns.push({
              text: token === '\t' ? '' : token,
              width,
              font: scale === 1 ? font : { ...font, css: scaleCssFont(font.css, scale) },
              color: run.style.color,
              underline: run.style.underline,
              strike: run.style.strike,
              characterSpacing: spacing,
              fontSize,
              baselineShift: run.style.baseline / 100_000,
              direction: paragraph.direction,
              outline: run.style.outline,
              shadow: run.style.shadow,
              innerShadow: run.style.innerShadow,
              fill: run.style.fill,
              glow: run.style.glow,
              blurRadius: run.style.blurRadius,
              softEdgeRadius: run.style.softEdgeRadius,
              reflection: run.style.reflection,
              shaped: shapingLookup.get(shapedLookupKey(
                font,
                paragraph.direction,
                token.replaceAll('\u00AD', ''),
              )),
              bulletImageResource: isBullet && rawToken.trim() !== ''
                ? paragraph.bulletImageResource
                : undefined,
              paragraphIndex,
              sourceStart: isBullet ? undefined : paragraphSourceOffset + fragmentSourceOffset,
              sourceEnd: isBullet ? undefined : paragraphSourceOffset + fragmentSourceOffset + token.length,
            })
            lineWidth += width
            lineHeight = Math.max(lineHeight, fontSize * 1.2)
            fragmentSourceOffset += token.length
          }
          if (!isBullet) runSourceOffset += rawToken.length
        }
        if (!isBullet) paragraphSourceOffset += run.text.length
      }
      finishLine(spacingPixels(paragraph.spaceAfter, metricLineHeight) * scale, true)
    }
    let column = 0
    let columnHeight = 0
    let maximumColumnHeight = 0
    const flowedLines = lines.map((line) => {
      const requiredHeight = line.before + line.height + line.after
      if (
        columnHeight > 0 &&
        columnHeight + requiredHeight > innerHeight &&
        column + 1 < columnCount
      ) {
        maximumColumnHeight = Math.max(maximumColumnHeight, columnHeight)
        column += 1
        columnHeight = 0
      }
      const flowed = Object.assign({}, line, { column, top: columnHeight })
      columnHeight += requiredHeight
      maximumColumnHeight = Math.max(maximumColumnHeight, columnHeight)
      return flowed
    })
    return {
      lines: flowedLines,
      contentWidth: flowedLines.reduce(
        (maximum, line) => Math.max(
          maximum,
          (line.column ?? 0) * (columnWidth + columnSpacing) +
            line.left + line.runs.reduce((sum, run) => sum + run.width, 0),
        ),
        0,
      ),
      contentHeight: maximumColumnHeight,
    }
  }
  const fits = (layout: ScaledLayout): boolean =>
    layout.contentWidth <= innerWidth && layout.contentHeight <= innerHeight
  const authoredScale = command.frame.autofit === 'shrink-text'
    ? Math.min(1, Math.max(0.01, (command.frame.autofitFontScale ?? 100_000) / 100_000))
    : 1
  let scaledLayout = layoutAtScale(authoredScale)
  if (command.frame.autofit === 'shrink-text' && command.frame.autofitRecompute) {
    const fullSize = layoutAtScale(1)
    if (fits(fullSize)) {
      scaledLayout = fullSize
    } else {
      let lowerScale = fits(scaledLayout) ? authoredScale : 0.1
      let best = fits(scaledLayout) ? scaledLayout : layoutAtScale(lowerScale)
      let upperScale = 1
      if (fits(best)) {
        for (let iteration = 0; iteration < 10; iteration += 1) {
          const candidateScale = (lowerScale + upperScale) / 2
          const candidate = layoutAtScale(candidateScale)
          if (fits(candidate)) {
            lowerScale = candidateScale
            best = candidate
          } else {
            upperScale = candidateScale
          }
        }
      }
      scaledLayout = best
    }
  } else if (command.frame.autofit === 'shrink-text' && !fits(scaledLayout)) {
    let lowerScale = 0.1
    let upperScale = authoredScale
    let best = layoutAtScale(lowerScale)
    if (fits(best)) {
      for (let iteration = 0; iteration < 12; iteration += 1) {
        const candidateScale = (lowerScale + upperScale) / 2
        const candidate = layoutAtScale(candidateScale)
        if (fits(candidate)) {
          lowerScale = candidateScale
          best = candidate
        } else {
          upperScale = candidateScale
        }
      }
    }
    scaledLayout = best
  }
  const { lines, contentWidth, contentHeight } = scaledLayout
  const effectiveBounds = command.frame.autofit === 'resize-shape'
    ? resizedTextShapeBounds(command, contentWidth, contentHeight, rotationDegrees)
    : command.bounds
  const effectiveLayoutBounds = rotatedLayoutBounds(effectiveBounds, rotationDegrees)
  const effectiveInnerHeight = Math.max(
    0,
    toPixels(
      effectiveLayoutBounds.height - command.frame.marginTop - command.frame.marginBottom,
    ),
  )
  const effectiveInnerX = toPixels(effectiveLayoutBounds.x + command.frame.marginLeft)
  const verticalOffset = command.frame.verticalAlignment === 'center'
    ? Math.max(0, (effectiveInnerHeight - contentHeight) / 2)
    : command.frame.verticalAlignment === 'bottom'
      ? Math.max(0, effectiveInnerHeight - contentHeight)
      : 0
  const output: RichTextLayoutRun[] = []
  for (const line of lines) {
    const y = toPixels(effectiveLayoutBounds.y + command.frame.marginTop) +
      verticalOffset + (line.top ?? 0) + line.before
    let visualRuns = line.direction === 'rtl'
      ? Array.from(line.runs, (_, index) => line.runs[line.runs.length - index - 1]!)
      : line.runs
    let width = visualRuns.reduce((sum, run) => sum + run.width, 0)
    const remaining = Math.max(0, columnWidth - line.left - width)
    if (line.alignment === 'justify' && !line.lastInParagraph && remaining > 0) {
      const spaces = visualRuns.filter((run) => isSoftWhitespace(run.text))
      if (spaces.length > 0) {
        const extra = remaining / spaces.length
        visualRuns = visualRuns.map((run) => isSoftWhitespace(run.text)
          ? { ...run, width: run.width + extra }
          : run)
        width += remaining
      }
    } else if (line.alignment === 'distributed' && remaining > 0) {
      const gaps = visualRuns.reduce((sum, run) => sum + characterGapCount(run.text), 0)
      if (gaps > 0) {
        const extra = remaining / gaps
        visualRuns = visualRuns.map((run) => ({
          ...run,
          width: run.width + characterGapCount(run.text) * extra,
          characterSpacing: run.characterSpacing + extra,
        }))
        width += remaining
      }
    }
    let x = effectiveInnerX + (line.column ?? 0) * (columnWidth + columnSpacing) + line.left
    if (line.alignment === 'center') x += Math.max(0, (columnWidth - line.left - width) / 2)
    if (line.alignment === 'right' || (line.alignment === 'left' && line.direction === 'rtl')) {
      x += Math.max(0, columnWidth - line.left - width)
    }
    for (const run of visualRuns) {
      const naturalBaseline = line.fontAlignment === 'top'
        ? y + run.fontSize * 0.82
        : line.fontAlignment === 'center'
          ? y + line.height / 2 + run.fontSize * 0.32
          : line.fontAlignment === 'bottom'
            ? y + line.height - run.fontSize * 0.18
            : y + line.height * 0.82
      output.push({
        ...run,
        x,
        baseline: naturalBaseline - run.fontSize * run.baselineShift,
        warpRotation: 0,
      })
      x += run.width
    }
  }
  const warpedOutput = command.frame.warp === undefined
    ? output
    : output.map((run) => applyTextWarp(run, command.frame.warp!, effectiveLayoutBounds))
  return Object.freeze({
    runs: Object.freeze(warpedOutput),
    contentWidth,
    contentHeight,
    layoutBounds: effectiveLayoutBounds,
    effectiveBounds,
    rotationDegrees,
  })
}

function applyTextWarp(
  run: RichTextLayoutRun,
  warp: NonNullable<SceneTextFrame['warp']>,
  bounds: EmuRect,
): RichTextLayoutRun {
  const left = toPixels(bounds.x)
  const width = Math.max(1, toPixels(bounds.width))
  const height = Math.max(1, toPixels(bounds.height))
  const normalized = Math.max(-1, Math.min(1, ((run.x + run.width / 2 - left) / width) * 2 - 1))
  const amplitude = Math.min(height / 2, height * warp.adjustment / 200_000)
  let offset = 0
  let rotation = 0
  if (warp.preset === 'textArchUp' || warp.preset === 'textArchUpPour' || warp.preset === 'archUp' || warp.preset === 'archUpPour') {
    offset = -amplitude * (1 - normalized * normalized)
    rotation = normalized * 20
  } else if (warp.preset === 'textArchDown' || warp.preset === 'textArchDownPour' || warp.preset === 'archDown' || warp.preset === 'archDownPour') {
    offset = amplitude * (1 - normalized * normalized)
    rotation = -normalized * 20
  } else if (warp.preset === 'textWave1' || warp.preset === 'textWave2' || warp.preset === 'wave1' || warp.preset === 'wave2') {
    const doubleWave = warp.preset === 'textWave2' || warp.preset === 'wave2'
    offset = Math.sin((normalized + 1) * Math.PI * (doubleWave ? 2 : 1)) * amplitude
    rotation = Math.cos((normalized + 1) * Math.PI * (doubleWave ? 2 : 1)) * 12
  } else if (warp.preset === 'textInflate' || warp.preset === 'inflate') {
    offset = -amplitude * (1 - normalized * normalized)
  } else if (warp.preset === 'textDeflate' || warp.preset === 'deflate') {
    offset = amplitude * (1 - normalized * normalized)
  }
  return { ...run, baseline: run.baseline + offset, warpRotation: rotation }
}

function rotatedLayoutBounds(bounds: EmuRect, rotationDegrees: 0 | 90 | -90): EmuRect {
  return rotationDegrees === 0
    ? bounds
    : {
        x: bounds.x + (bounds.width - bounds.height) / 2,
        y: bounds.y + (bounds.height - bounds.width) / 2,
        width: bounds.height,
        height: bounds.width,
      }
}

function resizedTextShapeBounds(
  command: Extract<SceneCommand, { readonly kind: 'draw-rich-text' }>,
  contentWidth: number,
  contentHeight: number,
  rotationDegrees: 0 | 90 | -90,
): EmuRect {
  const maximumDimension = 91_440_000
  const originalLayout = rotatedLayoutBounds(command.bounds, rotationDegrees)
  const measuredLayoutWidth = command.frame.wrap
    ? originalLayout.width
    : Math.max(
        originalLayout.width,
        contentWidth * EMU_PER_CSS_PIXEL + command.frame.marginLeft + command.frame.marginRight,
      )
  const measuredLayoutHeight = Math.max(
    originalLayout.height,
    contentHeight * EMU_PER_CSS_PIXEL + command.frame.marginTop + command.frame.marginBottom,
  )
  const requiredLayoutWidth = Number.isFinite(measuredLayoutWidth)
    ? Math.min(maximumDimension, Math.max(originalLayout.width, measuredLayoutWidth))
    : originalLayout.width
  const requiredLayoutHeight = Number.isFinite(measuredLayoutHeight)
    ? Math.min(maximumDimension, Math.max(originalLayout.height, measuredLayoutHeight))
    : originalLayout.height
  const width = rotationDegrees === 0 ? requiredLayoutWidth : requiredLayoutHeight
  const height = rotationDegrees === 0 ? requiredLayoutHeight : requiredLayoutWidth
  const deltaWidth = width - command.bounds.width
  const deltaHeight = height - command.bounds.height
  const anchor = command.frame.verticalAlignment === 'center'
    ? 0.5
    : command.frame.verticalAlignment === 'bottom'
      ? 1
      : 0
  return {
    x: command.bounds.x - (rotationDegrees === 0 ? 0 : deltaWidth * anchor),
    y: command.bounds.y - (rotationDegrees === 0 ? deltaHeight * anchor : 0),
    width,
    height,
  }
}

function drawRichTextLayout(
  context: CanvasRenderingContext2D,
  plan: RichTextLayoutPlan,
  images: readonly (DecodedImage | undefined)[] = [],
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
    context.save()
    if (run.warpRotation !== 0) {
      const centerX = run.x + run.width / 2
      const centerY = run.baseline - run.fontSize / 2
      context.translate(centerX, centerY)
      context.rotate(run.warpRotation * Math.PI / 180)
      context.translate(-centerX, -centerY)
    }
    context.font = run.font.css
    context.fillStyle = textFillStyle(context, run)
    context.filter = run.blurRadius > 0 || run.softEdgeRadius > 0
      ? `blur(${toPixels(Math.max(run.blurRadius, run.softEdgeRadius))}px)`
      : 'none'
    context.direction = run.direction
    context.textAlign = run.direction === 'rtl' ? 'right' : 'left'
    const start = run.direction === 'rtl' ? run.x + run.width : run.x
    const spaced = context as CanvasRenderingContext2D & { letterSpacing?: string }
    if (spaced.letterSpacing !== undefined) spaced.letterSpacing = `${run.characterSpacing}px`
    if (run.shadow !== undefined) {
      const radians = run.shadow.direction / 60_000 * Math.PI / 180
      context.shadowColor = cssColor(run.shadow.color)
      context.shadowBlur = toPixels(run.shadow.blurRadius)
      context.shadowOffsetX = Math.cos(radians) * toPixels(run.shadow.distance)
      context.shadowOffsetY = Math.sin(radians) * toPixels(run.shadow.distance)
    } else if (run.glow !== undefined) {
      context.shadowColor = cssColor(run.glow.color)
      context.shadowBlur = toPixels(run.glow.radius)
      context.shadowOffsetX = 0
      context.shadowOffsetY = 0
    }
    const bulletImage = run.bulletImageResource === undefined ? undefined : images[run.bulletImageResource]
    if (bulletImage !== undefined) {
      const size = run.fontSize
      context.drawImage(bulletImage.source, run.x, run.baseline - size * 0.82, size, size)
    } else if (run.fill?.kind !== 'none') context.fillText(run.text, start, run.baseline)
    if (run.reflection && bulletImage === undefined && run.fill?.kind !== 'none') {
      context.save()
      context.globalAlpha *= 0.22
      context.translate(0, 2 * (run.baseline + run.fontSize * 0.18))
      context.scale(1, -1)
      context.fillText(run.text, start, run.baseline)
      context.restore()
    }
    if (run.innerShadow !== undefined && bulletImage === undefined && run.fill?.kind !== 'none') {
      const radians = run.innerShadow.direction / 60_000 * Math.PI / 180
      context.save()
      context.globalCompositeOperation = 'source-atop'
      context.strokeStyle = cssColor(run.innerShadow.color)
      context.lineWidth = Math.max(0.5, run.fontSize / 18)
      context.shadowColor = cssColor(run.innerShadow.color)
      context.shadowBlur = toPixels(run.innerShadow.blurRadius)
      context.shadowOffsetX = Math.cos(radians) * toPixels(run.innerShadow.distance)
      context.shadowOffsetY = Math.sin(radians) * toPixels(run.innerShadow.distance)
      context.strokeText(run.text, start, run.baseline)
      context.restore()
    }
    context.shadowColor = 'transparent'
    context.shadowBlur = 0
    context.shadowOffsetX = 0
    context.shadowOffsetY = 0
    if (run.outline !== undefined) {
      context.strokeStyle = cssColor(run.outline.color)
      context.lineWidth = Math.max(0.5, toPixels(run.outline.width))
      context.setLineDash(dashPattern(run.outline.dash, context.lineWidth))
      context.strokeText(run.text, start, run.baseline)
      context.setLineDash([])
    }
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
    context.restore()
  }
  context.restore()
}

function spacingPixels(value: SceneTextSpacing | undefined, referenceHeight: number): number {
  if (value === undefined) return 0
  return value.kind === 'percent'
    ? referenceHeight * value.value / 100_000
    : pointsToCssPixels(value.value / 100)
}

function applyScaledLineSpacing(
  height: number,
  value: SceneTextSpacing | undefined,
  scale: number,
  reduction = 0,
): number {
  if (value === undefined) return height
  return value.kind === 'percent'
    ? height * Math.max(0, value.value - reduction) / 100_000
    : pointsToCssPixels(value.value / 100) * scale
}

function scaleCssFont(css: string, scale: number): string {
  return css.replace(/([0-9.]+)px/, (_match, size: string) => `${Number(size) * scale}px`)
}

function shapedLookupKey(
  font: ResolvedFont,
  direction: 'ltr' | 'rtl',
  text: string,
): string {
  return `${font.shapingKey ?? font.css}\0${direction}\0${text}`
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

/** Reads bounded raster metadata and JPEG EXIF orientation without decoding pixels. */
export function inspectRasterImageMetadata(input: ArrayBuffer | Uint8Array): RasterImageMetadata {
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input)
  if (
    bytes.byteLength >= 24 &&
    bytes.subarray(0, 8).every((value, index) => value === PNG_SIGNATURE[index]) &&
    new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(8) === 13 &&
    new TextDecoder('ascii').decode(bytes.subarray(12, 16)) === 'IHDR'
  ) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    const width = view.getUint32(16)
    const height = view.getUint32(20)
    if (width === 0 || height === 0) throw new Error('PNG dimensions are missing')
    return { format: 'png', width, height, orientation: 1 }
  }
  if (bytes.byteLength >= 10 && new TextDecoder('ascii').decode(bytes.subarray(0, 6)).match(/^GIF8[79]a$/u)) {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    const width = view.getUint16(6, true)
    const height = view.getUint16(8, true)
    if (width === 0 || height === 0) throw new Error('GIF dimensions are missing')
    return { format: 'gif', width, height, orientation: 1 }
  }
  const source = new TextDecoder('utf-8').decode(bytes).trimStart()
  if (source.startsWith('<svg') || source.startsWith('<?xml') && source.includes('<svg')) {
    assertSafeSvg(source)
    const svg = source.match(/<svg\b[^>]*>/iu)?.[0] ?? ''
    const viewBox = svgViewBoxSize(svg)
    const resolvedWidth = svgLength(svg, 'width') ?? viewBox?.width ?? 0
    const resolvedHeight = svgLength(svg, 'height') ?? viewBox?.height ?? 0
    if (!(resolvedWidth > 0) || !(resolvedHeight > 0)) throw new Error('SVG dimensions are missing')
    return {
      format: 'svg',
      width: Math.ceil(resolvedWidth),
      height: Math.ceil(resolvedHeight),
      orientation: 1,
    }
  }
  if (bytes.byteLength < 4 || bytes[0] !== 0xff || bytes[1] !== 0xd8) {
    throw new Error('resource is not a supported raster image')
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

const PNG_SIGNATURE = Uint8Array.of(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a)

function assertSafeSvg(source: string): void {
  if (
    /<!DOCTYPE\b|<!ENTITY\b|<\?xml-stylesheet\b|<(?:script|foreignObject|iframe|object|embed)\b|\bon[a-z]+\s*=/iu
      .test(source) ||
    /@import\b/iu.test(source)
  ) {
    throw new Error('SVG resource contains active or external content')
  }
  for (const match of source.matchAll(/\b(?:href|src)\s*=\s*["']([^"']*)["']/giu)) {
    if (!match[1]?.startsWith('#')) {
      throw new Error('SVG resource contains active or external content')
    }
  }
  for (const match of source.matchAll(/url\s*\(\s*["']?([^)'"\s]+)["']?\s*\)/giu)) {
    if (!match[1]?.startsWith('#')) {
      throw new Error('SVG resource contains active or external content')
    }
  }
}

function svgLength(root: string, name: 'width' | 'height'): number | undefined {
  const raw = root.match(new RegExp(`(?:^|\\s)${name}\\s*=\\s*["']([^"']+)["']`, 'iu'))?.[1]
  if (raw === undefined) return undefined
  const parsed = raw.trim().match(/^([+]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)(px|in|cm|mm|q|pt|pc)?$/iu)
  if (parsed === null) throw new Error('SVG dimensions use an unsupported length')
  const value = Number(parsed[1])
  const scales: Readonly<Record<string, number>> = {
    px: 1,
    in: 96,
    cm: 96 / 2.54,
    mm: 96 / 25.4,
    q: 96 / 101.6,
    pt: 96 / 72,
    pc: 16,
  }
  const result = value * scales[parsed[2]?.toLowerCase() ?? 'px']!
  if (!Number.isFinite(result) || result <= 0) throw new Error('SVG dimensions are invalid')
  return result
}

function svgViewBoxSize(root: string): { readonly width: number; readonly height: number } | undefined {
  const raw = root.match(/(?:^|\s)viewBox\s*=\s*["']([^"']+)["']/iu)?.[1]
  if (raw === undefined) return undefined
  const values = raw.trim().split(/[\s,]+/u).map(Number)
  if (values.length !== 4 || values.some((value) => !Number.isFinite(value))) {
    throw new Error('SVG dimensions contain an invalid viewBox')
  }
  const width = values[2]!
  const height = values[3]!
  if (width <= 0 || height <= 0) throw new Error('SVG dimensions contain an invalid viewBox')
  return { width, height }
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
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 0) {
    throw new RangeError('image byte limit must be a non-negative safe integer')
  }
  if (!Number.isSafeInteger(maxPixels) || maxPixels < 0) {
    throw new RangeError('image pixel limit must be a non-negative safe integer')
  }
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
  if (signal.aborted) {
    source.close()
    throwIfAborted(signal)
  }
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

export type ImageCacheKeyResolver = (
  image: SceneImage,
  signal: AbortSignal,
) => Promise<string>

export interface RenderTelemetry {
  readonly resolutionMs: number
  readonly fontMeasurementMs: number
  readonly displayExecutionMs: number
  readonly mediaDecodeMs: number
  readonly commandCount: number
  readonly cacheBytes: {
    readonly decodedImages: number
    readonly textMeasurements: number
    readonly richTextLayouts: number
  }
  readonly cacheHitRate: {
    readonly decodedImages: number
    readonly textMeasurements: number
    readonly richTextLayouts: number
  }
}

export interface CanvasRenderOptions {
  readonly signal?: AbortSignal
  readonly fontResolver?: FontResolver
  readonly imageResolver?: ImageResolver
  readonly imageCacheKey?: ImageCacheKeyResolver
  readonly imageCacheBytes?: number
  readonly resolutionMs?: number
  readonly scale?: number
}

/** Executes one compact scene and owns bounded image, measurement, and layout caches. */
export class CanvasDisplayListRenderer {
  readonly #images: ByteBudgetLru<string, DecodedImage>
  readonly #textMeasurements: ByteBudgetLru<string, number>
  readonly #richTextLayouts: ByteBudgetLru<string, RichTextLayoutPlan>
  readonly #defaultFontResolver = new FontResolver()
  readonly #fontResolverIds = new WeakMap<FontResolver, number>()
  readonly #imageInflight = new Map<string, Promise<DecodedImage | undefined>>()
  #imageAbort = new AbortController()
  #imageRevision = 0
  #nextFontResolverId = 1

  constructor(
    imageCacheBytes = 32 * 1024 * 1024,
    textMeasurementCacheBytes = 4 * 1024 * 1024,
    richTextLayoutCacheBytes = 8 * 1024 * 1024,
  ) {
    this.#images = new ByteBudgetLru(imageCacheBytes, (image) => image.close?.())
    this.#textMeasurements = new ByteBudgetLru(textMeasurementCacheBytes)
    this.#richTextLayouts = new ByteBudgetLru(richTextLayoutCacheBytes)
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
    const fontResolver = options.fontResolver ?? this.#defaultFontResolver
    const textCommands = scene.commands.filter(
      (command): command is Extract<SceneCommand, { readonly kind: 'draw-text' }> =>
        command.kind === 'draw-text',
    )
    const richTextEntries = scene.commands.flatMap((command, commandIndex) =>
      command.kind === 'draw-rich-text' ? [{ command, commandIndex }] : [],
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
    const measurements = measureTextBatchCached(
      context,
      textCommands.map((command, index) => ({
        text: scene.strings[command.text] ?? '',
        font: resolvedFonts[index]!.css,
      })),
      this.#textMeasurements,
    )
    const richTextLayouts = await Promise.all(
      richTextEntries.map(async ({ command }) => {
        const key = `${this.#fontResolverId(fontResolver)}\0${JSON.stringify(command)}`
        const cached = this.#richTextLayouts.get(key)
        if (cached !== undefined) return cached
        const layout = await buildRichTextLayout(context, command, fontResolver)
        this.#richTextLayouts.set(key, layout, key.length * 2 + layout.runs.length * 128)
        return layout
      }),
    )
    const richTextLayoutsByCommand = new Map(
      richTextEntries.map((entry, index) => [entry.commandIndex, richTextLayouts[index]!] as const),
    )
    const resizedBoundsByCommand = new Map<number, EmuRect>()
    for (const semantic of scene.semantics) {
      const richCommandIndex = Array.from(
        { length: semantic.commandCount },
        (_, offset) => semantic.firstCommand + offset,
      ).find((index) => scene.commands[index]?.kind === 'draw-rich-text')
      if (richCommandIndex === undefined) continue
      const richCommand = scene.commands[richCommandIndex]
      const layout = richTextLayoutsByCommand.get(richCommandIndex)
      if (richCommand?.kind !== 'draw-rich-text' || richCommand.frame.autofit !== 'resize-shape' || layout === undefined) {
        continue
      }
      for (let index = semantic.firstCommand; index < semantic.firstCommand + semantic.commandCount; index += 1) {
        resizedBoundsByCommand.set(index, layout.effectiveBounds)
      }
    }
    const fontMeasurementMs = performance.now() - fontStart
    const mediaStart = performance.now()
    const decodedImages = await this.#resolveImages(
      scene,
      options.imageResolver,
      options.imageCacheKey,
      signal,
    )
    const mediaDecodeMs = performance.now() - mediaStart
    throwIfAborted(signal)
    const executionStart = performance.now()
    context.save()
    try {
      context.setTransform(rootScale, 0, 0, rootScale, 0, 0)
      let textIndex = 0
      for (const [commandIndex, command] of scene.commands.entries()) {
        throwIfAborted(signal)
        const resizedBounds = resizedBoundsByCommand.get(commandIndex)
        const commandTransform = 'transform' in command && typeof command.transform === 'object'
          ? command.transform
          : undefined
        const transform = commandTransform !== undefined && resizedBounds !== undefined
          ? { ...commandTransform, bounds: resizedBounds }
          : commandTransform
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
            drawPreset(context, command.geometry, transform!, () => {
              context.fillStyle = cssColor(command.color)
              context.fill()
            })
            break
          case 'stroke-preset':
            drawPreset(context, command.geometry, transform!, () => {
              context.strokeStyle = cssColor(command.color)
              context.lineWidth = toPixels(command.width)
              context.setLineDash(dashPattern(command.dash, toPixels(command.width)))
              context.stroke()
            })
            if (command.geometry === 4) drawLineEnds(context, transform!, command)
            break
          case 'fill-gradient-preset':
            drawPreset(context, command.geometry, transform!, () => {
              context.fillStyle = canvasGradient(context, transform!.bounds, command.angle, command.stops)
              context.fill()
            })
            break
          case 'fill-radial-gradient-preset':
            drawPreset(context, command.geometry, transform!, () => {
              const width = toPixels(transform!.bounds.width)
              const height = toPixels(transform!.bounds.height)
              const radius = Math.max(width, height) / 2
              const gradient = context.createRadialGradient(width / 2, height / 2, 0, width / 2, height / 2, radius)
              for (const stop of command.stops) gradient.addColorStop(stop.position / 100_000, cssColor(stop.color))
              context.fillStyle = gradient
              context.fill()
            })
            break
          case 'fill-pattern-preset':
            drawPreset(context, command.geometry, transform!, () => {
              drawPatternFill(
                context,
                toPixels(transform!.bounds.width),
                toPixels(transform!.bounds.height),
                command.preset,
                command.foreground,
                command.background,
              )
            })
            break
          case 'draw-custom-path':
            drawCustomPath(context, transform === command.transform ? command : { ...command, transform: transform! })
            break
          case 'draw-outer-shadow':
            drawOuterShadow(context, transform === command.transform ? command : { ...command, transform: transform! })
            break
          case 'draw-image': {
            const image = decodedImages[command.resource]
            if (image !== undefined) drawImage(context, image.source, transform!, command.crop)
            else drawUnsupportedGraphic(context, transform!, 'Image unavailable')
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
            drawRichTextLayout(
              context,
              requiredMap(richTextLayoutsByCommand, commandIndex, 'rich-text layout'),
              decodedImages,
            )
            break
          case 'draw-unsupported':
            drawUnsupportedGraphic(context, transform!, unsupportedLabel(command.feature))
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
      cacheBytes: Object.freeze({
        decodedImages: this.#images.residentBytes,
        textMeasurements: this.#textMeasurements.residentBytes,
        richTextLayouts: this.#richTextLayouts.residentBytes,
      }),
      cacheHitRate: Object.freeze({
        decodedImages: this.#images.hitRate,
        textMeasurements: this.#textMeasurements.hitRate,
        richTextLayouts: this.#richTextLayouts.hitRate,
      }),
    })
  }

  clear(): void {
    this.#imageRevision += 1
    this.#imageAbort.abort()
    this.#imageAbort = new AbortController()
    this.#imageInflight.clear()
    this.#images.clear()
    this.#textMeasurements.clear()
    this.#richTextLayouts.clear()
  }

  #fontResolverId(resolver: FontResolver): number {
    const current = this.#fontResolverIds.get(resolver)
    if (current !== undefined) return current
    const next = this.#nextFontResolverId
    this.#nextFontResolverId += 1
    this.#fontResolverIds.set(resolver, next)
    return next
  }

  async #resolveImages(
    scene: DisplayScene,
    resolver: ImageResolver | undefined,
    cacheKeyResolver: ImageCacheKeyResolver | undefined,
    signal: AbortSignal,
  ): Promise<readonly (DecodedImage | undefined)[]> {
    if (resolver === undefined) return scene.images.map(() => undefined)
    return Promise.all(
      scene.images.map(async (image) => {
        const key = cacheKeyResolver === undefined
          ? `${image.partName ?? ''}\0${image.relationshipId}`
          : await cacheKeyResolver(image, signal)
        throwIfAborted(signal)
        const cached = this.#images.get(key)
        if (cached !== undefined) return cached
        let loading = this.#imageInflight.get(key)
        if (loading === undefined) {
          const revision = this.#imageRevision
          const sharedSignal = this.#imageAbort.signal
          loading = resolver(image, sharedSignal).then((decoded) => {
            if (revision !== this.#imageRevision) {
              decoded?.close?.()
              return undefined
            }
            if (decoded !== undefined) this.#images.set(key, decoded, decoded.residentBytes)
            return decoded
          }).finally(() => {
            if (this.#imageInflight.get(key) === loading) this.#imageInflight.delete(key)
          })
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
  if (!Number.isFinite(maximumWidth) || maximumWidth <= 0) {
    throw new RangeError('maximum width must be positive and finite')
  }
  if (!Number.isFinite(scene.width) || scene.width <= 0 || !Number.isFinite(scene.height) || scene.height <= 0) {
    throw new RangeError('scene dimensions must be positive and finite')
  }
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

function readCommand(
  reader: BinaryReader,
  version: number,
  inlineImages: SceneImage[] = [],
  declaredImageCount = 0,
): SceneCommand {
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
      const autofitFontScale = version >= 6 ? optionalI32(reader) : undefined
      const autofitLineSpacingReduction = version >= 6 ? optionalI32(reader) : undefined
      const autofitRecompute = version >= 9 && reader.u8() !== 0
      const flowCode = version >= 5 ? reader.u8() : 0
      if (flowCode > 2) throw new Error('display list contains an unknown text flow')
      const columnCount = version >= 6 ? reader.u8() : 1
      if (columnCount < 1 || columnCount > 16) {
        throw new Error('display list contains an invalid text column count')
      }
      const columnSpacing = version >= 6 ? reader.safeI64('text column spacing') : 0
      if (columnSpacing < 0) throw new Error('display list contains negative text column spacing')
      const defaultTabSize = version >= 7 ? reader.safeI64('default text tab size') : 457_200
      if (defaultTabSize <= 0) throw new Error('display list contains an invalid default text tab size')
      const warpPreset = version >= 7 ? reader.utf8Blob() : ''
      const warpAdjustment = version >= 7 ? optionalI32(reader) : undefined
      const warp = warpPreset === '' ? undefined : {
        preset: warpPreset,
        adjustment: warpAdjustment ?? 25_000,
      }
      const paragraphs: SceneParagraph[] = []
      const paragraphCount = reader.boundedCount('paragraph')
      for (let paragraphIndex = 0; paragraphIndex < paragraphCount; paragraphIndex += 1) {
        const alignment = textAlignment(reader.u8())
        const rawBullet = reader.utf8Blob()
        const bulletImagePartName = version >= 7 ? reader.utf8Blob() : ''
        const bulletImageRelationshipId = version >= 7 ? reader.utf8Blob() : ''
        const bulletImageResource = bulletImagePartName === '' && bulletImageRelationshipId === ''
          ? undefined
          : declaredImageCount + inlineImages.push({
              partName: bulletImagePartName || undefined,
              relationshipId: bulletImageRelationshipId,
            }) - 1
        const bulletStyle = version >= 7 && reader.u8() !== 0
          ? readTextStyle(reader, version)
          : undefined
        const level = reader.u8()
        const paragraphMarginLeft = reader.safeI64('paragraph left margin')
        const indent = reader.safeI64('paragraph indent')
        const lineSpacing = version >= 6 ? readTextSpacing(reader) : legacyTextSpacing(reader)
        const spaceBefore = version >= 6 ? readTextSpacing(reader) : legacyTextSpacing(reader)
        const spaceAfter = version >= 6 ? readTextSpacing(reader) : legacyTextSpacing(reader)
        const directionCode = version >= 5 ? reader.u8() : 0
        if (directionCode > 1) throw new Error('display list contains an unknown text direction')
        const fontAlignmentCode = version >= 7 ? reader.u8() : 0
        if (fontAlignmentCode > 4) throw new Error('display list contains an unknown text font alignment')
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
          bulletImageResource,
          bulletStyle,
          level,
          marginLeft: paragraphMarginLeft,
          indent,
          lineSpacing,
          spaceBefore,
          spaceAfter,
          direction: directionCode === 1 ? 'rtl' : 'ltr',
          tabs,
          fontAlignment: ['automatic', 'top', 'center', 'baseline', 'bottom'][fontAlignmentCode] as SceneParagraph['fontAlignment'],
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
          autofitFontScale,
          autofitLineSpacingReduction,
          autofitRecompute,
          flow: flowCode === 1 ? 'vertical' : flowCode === 2 ? 'vertical-270' : 'horizontal',
          columnCount,
          columnSpacing,
          defaultTabSize,
          warp,
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
  const base = {
    ...style,
    underline: version >= 5 ? reader.u8() !== 0 : false,
    strike: version >= 5 ? reader.u8() !== 0 : false,
    characterSpacing: version >= 5 ? reader.i32() : 0,
    baseline: version >= 5 ? reader.i32() : 0,
    blurRadius: 0,
    softEdgeRadius: 0,
    reflection: false,
  }
  if (version < 7) return base
  const outline = reader.u8() === 0 ? undefined : {
    color: readColor(reader),
    width: reader.safeI64('text outline width'),
    dash: reader.utf8Blob() || undefined,
  }
  if (outline !== undefined) { lineEnd(reader.u8()); lineEnd(reader.u8()) }
  const shadow = reader.u8() === 0 ? undefined : {
    color: readColor(reader),
    blurRadius: reader.safeI64('text shadow blur radius'),
    distance: reader.safeI64('text shadow distance'),
    direction: reader.i32(),
  }
  const innerShadow = version < 8 || reader.u8() === 0 ? undefined : {
      color: readColor(reader),
      blurRadius: reader.safeI64('text inner shadow blur radius'),
      distance: reader.safeI64('text inner shadow distance'),
      direction: reader.i32(),
    }
  const fill = reader.u8() === 0 ? undefined : readFill(reader)
  const glow = reader.u8() === 0 ? undefined : {
    color: readColor(reader),
    radius: reader.safeI64('text glow radius'),
  }
  const blurRadius = reader.safeI64('text blur radius')
  const softEdgeRadius = reader.safeI64('text soft-edge radius')
  const reflection = reader.u8() !== 0
  return { ...base, outline, shadow, innerShadow, fill, glow, blurRadius, softEdgeRadius, reflection }
}

function textFillStyle(
  context: CanvasRenderingContext2D,
  run: RichTextLayoutRun,
): string | CanvasGradient {
  const fill = run.fill
  if (fill === undefined || fill.kind === 'none') return cssColor(run.color)
  if (fill.kind === 'solid') return cssColor(fill.color)
  if (fill.kind === 'pattern') return cssColor(fill.foreground)
  if (fill.kind === 'linear-gradient') {
    const radians = fill.angle / 60_000 * Math.PI / 180
    const dx = Math.cos(radians) * run.width / 2
    const dy = Math.sin(radians) * run.fontSize / 2
    const gradient = context.createLinearGradient(
      run.x + run.width / 2 - dx,
      run.baseline - run.fontSize / 2 - dy,
      run.x + run.width / 2 + dx,
      run.baseline - run.fontSize / 2 + dy,
    )
    for (const stop of fill.stops) gradient.addColorStop(Math.max(0, Math.min(1, stop.position / 100_000)), cssColor(stop.color))
    return gradient
  }
  const gradient = context.createRadialGradient(
    run.x + run.width / 2, run.baseline - run.fontSize / 2, 0,
    run.x + run.width / 2, run.baseline - run.fontSize / 2, Math.max(run.width, run.fontSize) / 2,
  )
  for (const stop of fill.stops) gradient.addColorStop(Math.max(0, Math.min(1, stop.position / 100_000)), cssColor(stop.color))
  return gradient
}

function optionalI32(reader: BinaryReader): number | undefined {
  const value = reader.i32()
  return value === -0x8000_0000 ? undefined : value
}

function legacyTextSpacing(reader: BinaryReader): SceneTextSpacing | undefined {
  const value = optionalI32(reader)
  if (value === undefined) return undefined
  return { kind: value >= 10_000 ? 'percent' : 'points', value }
}

function readTextSpacing(reader: BinaryReader): SceneTextSpacing | undefined {
  const kind = reader.u8()
  if (kind === 0) return undefined
  if (kind === 1) return { kind: 'percent', value: reader.i32() }
  if (kind === 2) return { kind: 'points', value: reader.i32() }
  throw new Error('display list contains an unknown text spacing kind')
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
    blurRadius: 0,
    softEdgeRadius: 0,
    reflection: false,
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
  if (value === 5) return 'distributed'
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
    } else if (command.kind === 'draw-rich-text') {
      for (const paragraph of command.frame.paragraphs) {
        if (
          paragraph.bulletImageResource !== undefined &&
          paragraph.bulletImageResource >= imageCount
        ) throw new Error('display list references an unknown picture bullet')
      }
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
  if (/\p{Script=Arabic}|\p{Script=Hebrew}|\p{Script=Devanagari}|\p{Script=Thai}|\p{Script=Lao}|\p{Script=Khmer}/u.test(text)) {
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
  let whitespace = ''
  let opening = ''
  const flushLatin = (): void => {
    if (latin !== '') tokens.push(latin)
    latin = ''
  }
  const flushWhitespace = (): void => {
    if (whitespace !== '') tokens.push(whitespace)
    whitespace = ''
  }
  let wordJoin = false
  for (const character of segmentGraphemes(text)) {
    if (character === '\n') {
      flushLatin()
      flushWhitespace()
      if (opening !== '') tokens.push(opening)
      opening = ''
      tokens.push(character)
    } else if (character === '\t') {
      flushLatin()
      flushWhitespace()
      if (opening !== '') tokens.push(opening)
      opening = ''
      tokens.push(character)
    } else if (character === '\u2060') {
      wordJoin = true
    } else if (character === '\u200B') {
      flushLatin()
      flushWhitespace()
      tokens.push(character)
      wordJoin = false
    } else if (character === '\u00AD') {
      latin += character
      flushLatin()
      wordJoin = false
    } else if (isBreakableWhitespace(character)) {
      flushLatin()
      whitespace += character
      wordJoin = false
    } else if (/[（［｛〈《「『【〔〖〘〚]/u.test(character)) {
      flushLatin()
      flushWhitespace()
      opening += character
    } else if (/[）］｝〉》」』】〕〗〙〛、。，．？！：；]/u.test(character)) {
      flushLatin()
      flushWhitespace()
      if (tokens.length > 0) tokens[tokens.length - 1] += opening + character
      else opening += character
      opening = ''
    } else if (
      !wordJoin &&
      /\p{Script=Han}|\p{Script=Hangul}|\p{Script=Hiragana}|\p{Script=Katakana}|\p{Script=Thai}|\p{Script=Lao}|\p{Script=Khmer}/u.test(character)
    ) {
      flushLatin()
      flushWhitespace()
      tokens.push(opening + character)
      opening = ''
    } else {
      flushWhitespace()
      latin += opening + character
      opening = ''
      wordJoin = false
    }
  }
  if (opening !== '') latin += opening
  flushLatin()
  flushWhitespace()
  return tokens
}

function isSoftWhitespace(token: string): boolean {
  return token === '\u200B' || (
    token !== '\n' && token !== '\t' && [...token].every(isBreakableWhitespace)
  )
}

function splitTokenToFit(
  token: string,
  maxWidth: number,
  measure: (candidate: string) => number,
): readonly string[] {
  if (!(maxWidth > 0)) return [token]
  const fragments: string[] = []
  let fragment = ''
  for (const character of segmentGraphemes(token)) {
    const candidate = fragment + character
    if (fragment !== '' && measure(candidate) > maxWidth) {
      fragments.push(fragment)
      fragment = character
    } else {
      fragment = candidate
    }
  }
  if (fragment !== '') fragments.push(fragment)
  return fragments
}

function characterGapCount(text: string): number {
  return Math.max(0, segmentGraphemes(text).length - 1)
}

function isBreakableWhitespace(value: string): boolean {
  return value !== '\u00A0' && value !== '\u202F' && /^\s+$/u.test(value)
}

function segmentGraphemes(text: string): readonly string[] {
  const output: string[] = []
  let joinNext = false
  let regionalCount = 0
  for (const character of text) {
    const combining = character === '\u200D' || /\p{Mark}/u.test(character) ||
      /[\uFE00-\uFE0F\u{E0100}-\u{E01EF}]/u.test(character) ||
      /\p{Emoji_Modifier}/u.test(character)
    const regional = /\p{Regional_Indicator}/u.test(character)
    if (
      output.length === 0 ||
      (!joinNext && !combining && !(regional && regionalCount % 2 === 1))
    ) {
      output.push(character)
    } else {
      output[output.length - 1] += character
    }
    joinNext = character === '\u200D'
    regionalCount = regional ? regionalCount + 1 : 0
  }
  return output
}

function nextTabWidth(
  current: number,
  tabs: readonly SceneTextTab[],
  defaultTabSize = 457_200,
  followingWidth = 0,
  decimalPrefixWidth = 0,
): number {
  let tab: SceneTextTab | undefined
  for (const candidate of tabs) {
    if (
      toPixels(candidate.position) > current &&
      (tab === undefined || candidate.position < tab.position)
    ) tab = candidate
  }
  if (tab !== undefined) {
    const stop = toPixels(tab.position)
    const offset = tab.alignment === 'center' ? followingWidth / 2
      : tab.alignment === 'right' ? followingWidth
        : tab.alignment === 'decimal' ? decimalPrefixWidth
          : 0
    return Math.max(1, stop - current - offset)
  }
  const defaultInterval = toPixels(defaultTabSize)
  return Math.max(1, (Math.floor(current / defaultInterval) + 1) * defaultInterval - current)
}

function required<Value>(values: readonly Value[], index: number, label: string): Value {
  const value = values[index]
  if (value === undefined) throw new Error(`display list references an unknown ${label}`)
  return value
}

function requiredMap<Key, Value>(values: ReadonlyMap<Key, Value>, key: Key, label: string): Value {
  const value = values.get(key)
  if (value === undefined) throw new Error(`${label} is missing`)
  return value
}

function cssColor(color: RgbaColor): string {
  return `rgba(${color.red}, ${color.green}, ${color.blue}, ${color.alpha / 255})`
}

function applyGroup(context: CanvasRenderingContext2D, group: SceneGroupTransform): void {
  const matrix = toCssPixels(groupTransformMatrix(group))
  context.transform(matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f)
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
  const matrix = toCssPixels(shapeTransformMatrix(transform))
  context.transform(matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f)
  return { x: 0, y: 0, width: bounds.width, height: bounds.height }
}

function presetPath(
  context: CanvasRenderingContext2D,
  geometry: number,
  width: number,
  height: number,
): void {
  projectPresetGeometryToCanvas(presetGeometryPath(geometry, width, height), context)
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
  const alignment = style.alignment === 'justify' || style.alignment === 'distributed'
    ? 'left'
    : style.alignment
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
