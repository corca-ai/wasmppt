import {
  decodeOoxmlObfuscatedFont,
  decodeDisplayList,
  decodeRasterImage,
  FontResolver,
  inspectOpenTypeEmbedding,
  inspectRasterImageMetadata,
  type FontLoadingHost,
  type SceneImage,
  type WebFontDefinition,
} from './canvas.js'
import { DomSvgRenderer } from './dom-svg.js'
import type { DeckPageMetadata } from './protocol.js'
import type {
  OpenedDeckSession,
  ResolvedDeckSlide,
  WasmpptWorkerClient,
} from './worker-client.js'

const DEFAULT_RESOURCE_BYTES = 32 * 1024 * 1024
const DEFAULT_TOTAL_RESOURCE_BYTES = 128 * 1024 * 1024
const DEFAULT_RESOURCE_PIXELS = 64 * 1024 * 1024

export interface OfflineDeckPage {
  readonly slideIndex: number
  readonly revision: number
  readonly displayList: ArrayBuffer | Uint8Array
  readonly page: DeckPageMetadata
}

export interface OfflineResourceRequest {
  readonly partName: string
  readonly kind: 'image' | 'font'
  readonly slideIndex: number
}

export type OfflineResourceResolver = (
  request: OfflineResourceRequest,
  signal: AbortSignal,
) => Promise<ArrayBuffer | Uint8Array>

export interface OfflineHtmlOptions {
  readonly title?: string
  readonly language?: string
  readonly signal?: AbortSignal
  readonly maxResourceBytes?: number
  readonly maxTotalResourceBytes?: number
  readonly maxResourcePixels?: number
}

export interface OfflineHtmlResult {
  readonly html: string
  readonly bytes: Uint8Array
  readonly revision: number
  readonly pageCount: number
  readonly widthEmu: number
  readonly heightEmu: number
  readonly pageIds: readonly string[]
  readonly resourceCount: number
  readonly resourceBytes: number
}

export class OfflineDocumentError extends Error {
  readonly code:
    | 'invalid-document'
    | 'resource-limit'
    | 'unresolved-resource'
    | 'unsafe-resource'

  constructor(code: OfflineDocumentError['code'], message: string, options?: ErrorOptions) {
    super(message, options)
    this.name = 'OfflineDocumentError'
    this.code = code
  }
}

/**
 * Resolves the exact presentable page set from one browser deck-session revision and emits a
 * network-closed HTML document. Hidden authoring pages remain in the PPTX overlay but are omitted
 * from this presentation/PDF surface through the session's engine-owned presentable index set.
 */
export async function serializeDeckSessionToHtml(
  client: WasmpptWorkerClient,
  session: OpenedDeckSession,
  options: OfflineHtmlOptions = {},
): Promise<OfflineHtmlResult> {
  const signal = options.signal ?? new AbortController().signal
  const pages: OfflineDeckPage[] = []
  for (const slideIndex of session.presentableSlides) {
    throwIfAborted(signal)
    const resolved = await client.resolveDeckSlide(
      session.handle,
      session.revision,
      slideIndex,
      { signal },
    )
    pages.push(deckPage(resolved))
  }
  return serializeOfflineHtmlDocument(
    pages,
    async ({ partName }, resourceSignal) => {
      const resource = await client.deckSessionResource(
        session.handle,
        session.revision,
        partName,
        { signal: resourceSignal },
      )
      return resource.bytes
    },
    options,
  )
}

/** Projects already resolved WPDL pages into one deterministic standalone HTML document. */
export async function serializeOfflineHtmlDocument(
  pages: readonly OfflineDeckPage[],
  resolveResource: OfflineResourceResolver,
  options: OfflineHtmlOptions = {},
): Promise<OfflineHtmlResult> {
  if (globalThis.document === undefined) {
    throw new OfflineDocumentError('invalid-document', 'offline HTML serialization requires a browser Document')
  }
  if (pages.length === 0) {
    throw new OfflineDocumentError('invalid-document', 'offline HTML requires at least one presentable page')
  }
  const signal = options.signal ?? new AbortController().signal
  throwIfAborted(signal)
  const maximumResourceBytes = positiveLimit(options.maxResourceBytes, DEFAULT_RESOURCE_BYTES, 'resource')
  const maximumTotalResourceBytes = positiveLimit(
    options.maxTotalResourceBytes,
    DEFAULT_TOTAL_RESOURCE_BYTES,
    'total resource',
  )
  const maximumResourcePixels = positiveLimit(
    options.maxResourcePixels,
    DEFAULT_RESOURCE_PIXELS,
    'resource pixel',
  )
  const revision = pages[0]!.revision
  const decodedScenes = pages.map((page) => {
    if (page.revision !== revision) {
      throw new OfflineDocumentError('invalid-document', 'offline HTML pages must use one exact revision')
    }
    validatePage(page)
    return decodeDisplayList(page.displayList)
  })
  const widthEmu = decodedScenes[0]!.width
  const heightEmu = decodedScenes[0]!.height
  for (const scene of decodedScenes) {
    if (scene.width !== widthEmu || scene.height !== heightEmu) {
      throw new OfflineDocumentError(
        'invalid-document',
        'every offline HTML page must use the selected POTX page size',
      )
    }
  }

  const resources = new Map<string, Uint8Array>()
  let resourceBytes = 0
  const loadResource = async (request: OfflineResourceRequest): Promise<Uint8Array> => {
    throwIfAborted(signal)
    const existing = resources.get(request.partName)
    if (existing !== undefined) return existing
    let value: ArrayBuffer | Uint8Array
    try {
      value = await resolveResource(request, signal)
    } catch (error) {
      throw new OfflineDocumentError(
        'unresolved-resource',
        `required ${request.kind} resource could not be resolved: ${request.partName}`,
        { cause: error },
      )
    }
    const bytes = value instanceof Uint8Array ? value.slice() : new Uint8Array(value.slice(0))
    if (bytes.byteLength === 0 || bytes.byteLength > maximumResourceBytes) {
      throw new OfflineDocumentError(
        'resource-limit',
        `resource ${request.partName} exceeds the ${maximumResourceBytes}-byte limit`,
      )
    }
    if (resourceBytes + bytes.byteLength > maximumTotalResourceBytes) {
      throw new OfflineDocumentError(
        'resource-limit',
        `offline HTML resources exceed the ${maximumTotalResourceBytes}-byte total limit`,
      )
    }
    resources.set(request.partName, bytes)
    resourceBytes += bytes.byteLength
    return bytes
  }

  const fontHost = new ScopedFontLoadingHost()
  const fontResolver = new FontResolver({ host: fontHost })
  const fontRules: string[] = []
  const seenFonts = new Set<string>()
  for (let pageOffset = 0; pageOffset < pages.length; pageOffset += 1) {
    const page = pages[pageOffset]!
    const scene = decodedScenes[pageOffset]!
    for (const font of scene.embeddedFonts) {
      const key = `${font.partName}\0${font.family}\0${font.style}`
      if (seenFonts.has(key)) continue
      seenFonts.add(key)
      const source = await loadResource({
        partName: font.partName,
        kind: 'font',
        slideIndex: page.slideIndex,
      })
      const guid = embeddedFontGuid(font.partName)
      const decoded = guid === undefined ? source.slice() : decodeOoxmlObfuscatedFont(source, guid)
      const permission = inspectOpenTypeEmbedding(decoded)
      if (!permission.permitted) {
        throw new OfflineDocumentError(
          'unsafe-resource',
          `embedded font ${font.partName} does not permit offline preview/print embedding`,
        )
      }
      const descriptors = fontDescriptors(font.style)
      fontResolver.registerWebFont({
        family: font.family,
        source: exactArrayBuffer(decoded),
        descriptors,
      })
      fontRules.push(
        `@font-face{font-family:${cssString(font.family)};src:url("${dataUrl(fontMediaType(decoded), decoded)}");` +
        `font-style:${descriptors.style};font-weight:${descriptors.weight};font-display:block}`,
      )
    }
  }

  const imageUrls = new Map<string, string>()
  const imageUrl = async (image: SceneImage, slideIndex: number): Promise<string> => {
    if (image.partName === undefined || image.partName.length === 0) {
      throw new OfflineDocumentError(
        'unresolved-resource',
        `slide ${slideIndex} contains an image without a package part`,
      )
    }
    const cached = imageUrls.get(image.partName)
    if (cached !== undefined) return cached
    const bytes = await loadResource({ partName: image.partName, kind: 'image', slideIndex })
    let metadata: ReturnType<typeof inspectRasterImageMetadata>
    try {
      metadata = inspectRasterImageMetadata(bytes)
    } catch (error) {
      throw new OfflineDocumentError(
        'unsafe-resource',
        `image resource is unsupported or unsafe: ${image.partName}`,
        { cause: error },
      )
    }
    if (metadata.width * metadata.height > maximumResourcePixels) {
      throw new OfflineDocumentError(
        'resource-limit',
        `image ${image.partName} exceeds the ${maximumResourcePixels}-pixel limit`,
      )
    }
    const url = metadata.format === 'gif'
      ? await gifFirstFrameDataUrl(bytes, maximumResourceBytes, maximumResourcePixels, signal)
      : dataUrl(imageMediaType(metadata.format), bytes)
    imageUrls.set(image.partName, url)
    return url
  }

  const renderer = new DomSvgRenderer()
  const renderedPages: string[] = []
  try {
    for (let pageOffset = 0; pageOffset < pages.length; pageOffset += 1) {
      throwIfAborted(signal)
      const page = pages[pageOffset]!
      const scene = decodedScenes[pageOffset]!
      const host = document.createElement('div')
      const rendered = await renderer.render(scene, host, {
        revision,
        slideIndex: page.slideIndex,
        signal,
        fontResolver,
        imageResolver: (image) => imageUrl(image, page.slideIndex),
      })
      const root = rendered.root
      root.classList.add('wasmppt-offline-page')
      root.dataset['revision'] = String(revision)
      root.dataset['pageId'] = page.page.pageId
      root.dataset['logicalSlideId'] = page.page.logicalSlideId
      root.dataset['physicalSlideIndex'] = String(page.slideIndex)
      root.dataset['continuationOrdinal'] = String(page.page.continuationOrdinal)
      root.dataset['continuationTotal'] = String(page.page.continuationTotal)
      root.dataset['hidden'] = String(page.page.hidden)
      if (page.page.continuationLabel !== undefined) {
        root.dataset['continuationLabel'] = page.page.continuationLabel
      }
      root.setAttribute('aria-posinset', String(pageOffset + 1))
      root.setAttribute('aria-setsize', String(pages.length))
      renderedPages.push(root.outerHTML)
    }
  } finally {
    fontHost.clear()
  }

  const html = standaloneDocument({
    title: options.title ?? 'Presentation',
    language: options.language ?? 'en',
    widthEmu,
    heightEmu,
    fontRules,
    pages: renderedPages,
  })
  const bytes = new TextEncoder().encode(html)
  return Object.freeze({
    html,
    bytes,
    revision,
    pageCount: pages.length,
    widthEmu,
    heightEmu,
    pageIds: Object.freeze(pages.map((page) => page.page.pageId)),
    resourceCount: resources.size,
    resourceBytes,
  })
}

function deckPage(resolved: ResolvedDeckSlide): OfflineDeckPage {
  return {
    slideIndex: resolved.slideIndex,
    revision: resolved.revision,
    displayList: resolved.displayList,
    page: resolved.page,
  }
}

function validatePage(page: OfflineDeckPage): void {
  if (!Number.isSafeInteger(page.slideIndex) || page.slideIndex < 0 ||
    !Number.isSafeInteger(page.revision) || page.revision < 0 ||
    !/^[0-9a-f]{32}$/u.test(page.page.pageId) ||
    !/^[0-9a-f]{32}$/u.test(page.page.logicalSlideId) ||
    !Number.isSafeInteger(page.page.continuationOrdinal) || page.page.continuationOrdinal <= 0 ||
    !Number.isSafeInteger(page.page.continuationTotal) ||
    page.page.continuationTotal < page.page.continuationOrdinal) {
    throw new OfflineDocumentError('invalid-document', 'offline HTML page metadata is invalid')
  }
}

function standaloneDocument(input: {
  readonly title: string
  readonly language: string
  readonly widthEmu: number
  readonly heightEmu: number
  readonly fontRules: readonly string[]
  readonly pages: readonly string[]
}): string {
  const width = emuToCssPixels(input.widthEmu)
  const height = emuToCssPixels(input.heightEmu)
  const style = [
    '@page{margin:0;size:' + width + 'px ' + height + 'px}',
    'html,body{margin:0;padding:0;background:#fff}',
    '.wasmppt-offline-deck{margin:0;padding:0}',
    '.wasmppt-offline-page{break-after:page;page-break-after:always;contain:layout paint size}',
    '.wasmppt-offline-page:last-child{break-after:auto;page-break-after:auto}',
    '@media screen{.wasmppt-offline-page{margin:0 auto}}',
    ...input.fontRules,
  ].join('')
  return '<!doctype html>\n' +
    `<html lang="${escapeAttribute(input.language)}"><head><meta charset="utf-8">` +
    '<meta http-equiv="Content-Security-Policy" content="default-src \'none\'; img-src data:; ' +
    'font-src data:; style-src \'unsafe-inline\'; script-src \'none\'; connect-src \'none\'; ' +
    'media-src \'none\'; object-src \'none\'; frame-src \'none\'; base-uri \'none\'; ' +
    'form-action \'none\'">' +
    `<meta name="wasmppt-page-size" content="${input.widthEmu}x${input.heightEmu}">` +
    `<title>${escapeText(input.title)}</title><style>${style}</style></head><body>` +
    `<main class="wasmppt-offline-deck" data-page-count="${input.pages.length}">` +
    input.pages.join('') +
    '</main></body></html>\n'
}

async function gifFirstFrameDataUrl(
  bytes: Uint8Array,
  maxBytes: number,
  maxPixels: number,
  signal: AbortSignal,
): Promise<string> {
  const decoded = await decodeRasterImage(bytes, { maxBytes, maxPixels }, signal)
  const metadata = inspectRasterImageMetadata(bytes)
  try {
    const canvas = document.createElement('canvas')
    canvas.width = metadata.width
    canvas.height = metadata.height
    const context = canvas.getContext('2d')
    if (context === null) {
      throw new OfflineDocumentError('invalid-document', 'Canvas 2D is required to freeze GIF output')
    }
    context.drawImage(decoded.source, 0, 0, metadata.width, metadata.height)
    return canvas.toDataURL('image/png')
  } finally {
    decoded.close?.()
  }
}

function positiveLimit(value: number | undefined, fallback: number, label: string): number {
  const result = value ?? fallback
  if (!Number.isSafeInteger(result) || result <= 0) {
    throw new RangeError(`${label} limit must be a positive safe integer`)
  }
  return result
}

function exactArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  return bytes.slice().buffer
}

function embeddedFontGuid(partName: string): string | undefined {
  return partName.match(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/iu)?.[0]
}

function fontDescriptors(style: 'regular' | 'bold' | 'italic' | 'bold-italic'): {
  readonly weight: string
  readonly style: string
} {
  return {
    weight: style === 'bold' || style === 'bold-italic' ? '700' : '400',
    style: style === 'italic' || style === 'bold-italic' ? 'italic' : 'normal',
  }
}

function fontMediaType(bytes: Uint8Array): string {
  const tag = new TextDecoder('ascii').decode(bytes.subarray(0, 4))
  if (tag === 'OTTO') return 'font/otf'
  if (tag === 'ttcf') return 'font/collection'
  return 'font/ttf'
}

function imageMediaType(format: 'png' | 'jpeg' | 'gif' | 'svg'): string {
  if (format === 'jpeg') return 'image/jpeg'
  if (format === 'svg') return 'image/svg+xml'
  if (format === 'gif') return 'image/gif'
  return 'image/png'
}

function dataUrl(mediaType: string, bytes: Uint8Array): string {
  return `data:${mediaType};base64,${base64(bytes)}`
}

function base64(bytes: Uint8Array): string {
  const output: string[] = []
  const chunkBytes = 24_576
  for (let offset = 0; offset < bytes.byteLength; offset += chunkBytes) {
    const chunk = bytes.subarray(offset, Math.min(bytes.byteLength, offset + chunkBytes))
    output.push(btoa(String.fromCharCode(...chunk)))
  }
  return output.join('')
}

class ScopedFontLoadingHost implements FontLoadingHost {
  readonly #faces: FontFace[] = []

  async load(definition: WebFontDefinition): Promise<void> {
    const face = new FontFace(definition.family, definition.source, definition.descriptors)
    const loaded = await face.load()
    document.fonts.add(loaded)
    this.#faces.push(loaded)
  }

  check(css: string, text: string): boolean {
    return document.fonts.check(css, text)
  }

  clear(): void {
    for (const face of this.#faces) document.fonts.delete(face)
    this.#faces.length = 0
  }
}

function cssString(value: string): string {
  return JSON.stringify(value).replaceAll('<', '\\3c ')
}

function emuToCssPixels(value: number): string {
  const pixels = value / 9_525
  return Number.isInteger(pixels) ? String(pixels) : String(Number(pixels.toFixed(8)))
}

function escapeText(value: string): string {
  return value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')
}

function escapeAttribute(value: string): string {
  return escapeText(value).replaceAll('"', '&quot;')
}

function throwIfAborted(signal: AbortSignal): void {
  if (signal.aborted) throw new DOMException('offline document serialization was cancelled', 'AbortError')
}
