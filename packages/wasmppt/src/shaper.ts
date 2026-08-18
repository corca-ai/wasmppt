/** Optional exact OpenType shaping contract. The Wasm implementation is loaded by the host. */
export interface FontByteShaper {
  shape(request: FontShapeRequest): Promise<ShapedFontRun>
  breakText(text: string, language?: string): Promise<readonly string[]>
}

export interface FontShapeRequest {
  readonly fontBytes: Uint8Array
  readonly faceIndex?: number | undefined
  readonly text: string
  readonly direction: 'ltr' | 'rtl' | 'ttb' | 'btt'
  readonly language?: string | undefined
  readonly script?: string | undefined
  readonly features?: readonly string[] | undefined
  readonly variations?: readonly string[] | undefined
}

export interface ShapedFontGlyph {
  readonly glyphId: number
  /** UTF-8 source byte offset using HarfBuzz cluster semantics. */
  readonly cluster: number
  readonly xAdvance: number
  readonly yAdvance: number
  readonly xOffset: number
  readonly yOffset: number
  readonly safeToBreak: boolean
}

export interface ShapedFontRun {
  readonly unitsPerEm: number
  readonly glyphs: readonly ShapedFontGlyph[]
}

export interface WasmFontShaperModule {
  line_breaks(text: string, maxTextBytes: number): Uint8Array
  shape_font(
    fontBytes: Uint8Array,
    faceIndex: number,
    text: string,
    direction: number,
    language: string,
    script: string,
    features: string,
    variations: string,
    maxFontBytes: number,
    maxTextBytes: number,
    maxGlyphs: number,
  ): Uint8Array
}

export interface WasmFontShaperOptions {
  readonly maxFontBytes?: number | undefined
  readonly maxTextBytes?: number | undefined
  readonly maxGlyphs?: number | undefined
  readonly maxCacheBytes?: number | undefined
}

const SHAPE_MAGIC = 'WPSH'
const SHAPE_VERSION = 1
const GLYPH_BYTES = 25

/** Bounded adapter around the independently emitted `wasmppt-shaper-wasm` module. */
export class WasmFontShaper implements FontByteShaper {
  readonly #module: WasmFontShaperModule
  readonly #maxFontBytes: number
  readonly #maxTextBytes: number
  readonly #maxGlyphs: number
  readonly #maxCacheBytes: number
  readonly #cache = new Map<string, { readonly run: ShapedFontRun; readonly weight: number }>()
  readonly #breakCache = new Map<string, { readonly tokens: readonly string[]; readonly weight: number }>()
  #cacheBytes = 0
  #breakCacheBytes = 0
  #fontIds = new WeakMap<ArrayBufferLike, { readonly id: number; readonly fingerprint: number }>()
  #nextFontId = 1

  constructor(module: WasmFontShaperModule, options: WasmFontShaperOptions = {}) {
    this.#module = module
    this.#maxFontBytes = positiveInteger(options.maxFontBytes ?? 32 * 1024 * 1024, 'maxFontBytes')
    this.#maxTextBytes = positiveInteger(options.maxTextBytes ?? 1024 * 1024, 'maxTextBytes')
    this.#maxGlyphs = positiveInteger(options.maxGlyphs ?? 1_048_576, 'maxGlyphs')
    this.#maxCacheBytes = positiveInteger(options.maxCacheBytes ?? 16 * 1024 * 1024, 'maxCacheBytes')
  }

  async shape(request: FontShapeRequest): Promise<ShapedFontRun> {
    if (request.fontBytes.byteLength === 0 || request.fontBytes.byteLength > this.#maxFontBytes) {
      throw new RangeError('font bytes exceed maxFontBytes')
    }
    const textBytes = new TextEncoder().encode(request.text).byteLength
    if (textBytes > this.#maxTextBytes) throw new RangeError('text bytes exceed maxTextBytes')
    const faceIndex = request.faceIndex ?? 0
    if (!Number.isSafeInteger(faceIndex) || faceIndex < 0 || faceIndex > 63) {
      throw new RangeError('faceIndex must be an integer between 0 and 63')
    }
    const language = shapeProperty(request.language ?? '', 'language')
    const script = shapeProperty(request.script ?? '', 'script')
    const features = shapePropertyList(request.features ?? [], 'features')
    const variations = shapePropertyList(request.variations ?? [], 'variations')
    const fontId = this.#fontIdentity(request.fontBytes)
    const key = `${fontId}:${request.fontBytes.byteOffset}:${request.fontBytes.byteLength}\0${faceIndex}\0${request.direction}\0${language}\0${script}\0${features}\0${variations}\0${request.text}`
    const cached = this.#cache.get(key)
    if (cached !== undefined) {
      this.#cache.delete(key)
      this.#cache.set(key, cached)
      return cached.run
    }
    const encoded = this.#module.shape_font(
      request.fontBytes,
      faceIndex,
      request.text,
      directionCode(request.direction),
      language,
      script,
      features,
      variations,
      this.#maxFontBytes,
      this.#maxTextBytes,
      this.#maxGlyphs,
    )
    const run = decodeShapedFontRun(encoded, this.#maxGlyphs)
    const weight = encoded.byteLength + key.length * 2
    if (weight <= this.#maxCacheBytes) {
      this.#cache.set(key, { run, weight })
      this.#cacheBytes += weight
      this.#evictToBudget()
    }
    return run
  }

  async breakText(text: string, language = ''): Promise<readonly string[]> {
    const textBytes = new TextEncoder().encode(text).byteLength
    if (textBytes > this.#maxTextBytes) throw new RangeError('text bytes exceed maxTextBytes')
    const key = `uax14-v1\0${shapeProperty(language, 'language')}\0${text}`
    const cached = this.#breakCache.get(key)
    if (cached !== undefined) {
      this.#breakCache.delete(key)
      this.#breakCache.set(key, cached)
      return cached.tokens
    }
    const encoded = this.#module.line_breaks(text, this.#maxTextBytes)
    const tokens = decodeLineBreakTokens(text, encoded)
    const weight = encoded.byteLength + key.length * 2 + tokens.length * 8
    if (weight <= this.#maxCacheBytes) {
      this.#breakCache.set(key, { tokens, weight })
      this.#breakCacheBytes += weight
      this.#evictToBudget()
    }
    return tokens
  }

  clear(): void {
    this.#cache.clear()
    this.#breakCache.clear()
    this.#cacheBytes = 0
    this.#breakCacheBytes = 0
  }

  #evictToBudget(): void {
    while (this.#cacheBytes + this.#breakCacheBytes > this.#maxCacheBytes) {
      const oldestShape = this.#cache.entries().next().value as
        | [string, { readonly weight: number }]
        | undefined
      if (oldestShape !== undefined) {
        this.#cache.delete(oldestShape[0])
        this.#cacheBytes -= oldestShape[1].weight
        continue
      }
      const oldestBreak = this.#breakCache.entries().next().value as
        | [string, { readonly weight: number }]
        | undefined
      if (oldestBreak === undefined) break
      this.#breakCache.delete(oldestBreak[0])
      this.#breakCacheBytes -= oldestBreak[1].weight
    }
  }

  #fontIdentity(bytes: Uint8Array): number {
    const fingerprint = byteFingerprint(bytes)
    let identity = this.#fontIds.get(bytes.buffer)
    if (identity === undefined || identity.fingerprint !== fingerprint) {
      identity = { id: this.#nextFontId, fingerprint }
      this.#nextFontId += 1
      this.#fontIds.set(bytes.buffer, identity)
    }
    return identity.id
  }
}

export function decodeLineBreakTokens(text: string, input: Uint8Array): readonly string[] {
  if (input.byteLength < 10) throw new Error('line-break plan is truncated')
  if (new TextDecoder('ascii').decode(input.subarray(0, 4)) !== 'WPLB') {
    throw new Error('line-break plan has invalid magic')
  }
  const view = new DataView(input.buffer, input.byteOffset, input.byteLength)
  if (view.getUint16(4, true) !== 1) throw new Error('line-break plan version is unsupported')
  const count = view.getUint32(6, true)
  if (10 + count * 5 !== input.byteLength || count > text.length + 1) {
    throw new Error('line-break plan bounds are invalid')
  }
  const byteToUtf16 = new Map([[0, 0]])
  let byteOffset = 0
  let utf16Offset = 0
  for (const character of text) {
    byteOffset += new TextEncoder().encode(character).byteLength
    utf16Offset += character.length
    byteToUtf16.set(byteOffset, utf16Offset)
  }
  const tokens: string[] = []
  let start = 0
  for (let index = 0; index < count; index += 1) {
    const offset = view.getUint32(10 + index * 5, true)
    const end = byteToUtf16.get(offset)
    if (end === undefined || end < start) throw new Error('line-break offset is not a text boundary')
    const segment = text.slice(start, end)
    const mandatory = input[10 + index * 5 + 4] === 1
    if (mandatory && segment.endsWith('\r\n')) {
      if (segment.length > 2) tokens.push(segment.slice(0, -2))
      tokens.push('\n')
    } else if (mandatory && (segment.endsWith('\n') || segment.endsWith('\r'))) {
      if (segment.length > 1) tokens.push(segment.slice(0, -1))
      tokens.push('\n')
    } else if (segment !== '') pushBreakSegment(tokens, segment)
    start = end
  }
  if (start !== text.length) throw new Error('line-break plan does not terminate at text end')
  return Object.freeze(tokens)
}

function pushBreakSegment(tokens: string[], segment: string): void {
  const whitespace = /^(.*?)(\p{White_Space}+)$/u.exec(segment)
  if (whitespace === null) {
    tokens.push(segment)
    return
  }
  if (whitespace[1] !== '') tokens.push(whitespace[1]!)
  tokens.push(whitespace[2]!)
}

export function decodeShapedFontRun(input: Uint8Array, maxGlyphs = 1_048_576): ShapedFontRun {
  if (input.byteLength < 12) throw new Error('shaped run is truncated')
  const magic = new TextDecoder('ascii').decode(input.subarray(0, 4))
  if (magic !== SHAPE_MAGIC) throw new Error('shaped run has invalid magic')
  const view = new DataView(input.buffer, input.byteOffset, input.byteLength)
  if (view.getUint16(4, true) !== SHAPE_VERSION) throw new Error('shaped run version is unsupported')
  const unitsPerEm = view.getUint16(6, true)
  const count = view.getUint32(8, true)
  if (unitsPerEm === 0 || count > maxGlyphs || 12 + count * GLYPH_BYTES !== input.byteLength) {
    throw new Error('shaped run bounds are invalid')
  }
  const glyphs: ShapedFontGlyph[] = []
  let offset = 12
  for (let index = 0; index < count; index += 1) {
    glyphs.push(Object.freeze({
      glyphId: view.getUint32(offset, true),
      cluster: view.getUint32(offset + 4, true),
      xAdvance: view.getInt32(offset + 8, true),
      yAdvance: view.getInt32(offset + 12, true),
      xOffset: view.getInt32(offset + 16, true),
      yOffset: view.getInt32(offset + 20, true),
      safeToBreak: input[offset + 24] !== 0,
    }))
    offset += GLYPH_BYTES
  }
  return Object.freeze({ unitsPerEm, glyphs: Object.freeze(glyphs) })
}

function directionCode(direction: FontShapeRequest['direction']): number {
  if (direction === 'ltr') return 0
  if (direction === 'rtl') return 1
  if (direction === 'ttb') return 2
  return 3
}

function positiveInteger(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value <= 0) throw new RangeError(`${name} must be positive`)
  return value
}

function byteFingerprint(bytes: Uint8Array): number {
  let hash = 0x811c9dc5
  for (const byte of bytes) {
    hash ^= byte
    hash = Math.imul(hash, 0x01000193)
  }
  return hash >>> 0
}

function shapeProperty(value: string, name: string): string {
  if (value.includes('\0') || new TextEncoder().encode(value).byteLength > 128) {
    throw new RangeError(`${name} is invalid or too long`)
  }
  return value
}

function shapePropertyList(values: readonly string[], name: string): string {
  if (values.length > 64) throw new RangeError(`${name} exceeds 64 entries`)
  return values.map((value) => {
    if (value === '') throw new RangeError(`${name} contains an empty entry`)
    return shapeProperty(value, name)
  }).join('\0')
}
