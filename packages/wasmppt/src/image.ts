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
  readonly maxBytes?: number | undefined
  readonly maxPixels?: number | undefined
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

function throwIfAborted(signal: AbortSignal): void {
  if (signal.aborted) throw new DOMException('slide rendering was cancelled', 'AbortError')
}
