import type {
  DeckAssetInput, DeckHyperlink, DeckListContent, DeckLogicalSlide, DeckRichText,
  DeckSemanticNode, DeckSemanticRole, DeckSourceRange, DeckSpec, DeckTableContent,
} from './model.js'

// WDSF tags are private transport details, pinned against Rust by the SDK integration tests.
export const ROLE_CODES = {
  title: 1, subtitle: 2, prose: 3, section: 4, list: 5, listItem: 6,
  figure: 7, caption: 8, gallery: 9, table: 10, chart: 11, code: 12,
  diagram: 13, displayMath: 14, quote: 15, credit: 16, definition: 17,
  definitionTerm: 18, definitionDescription: 19, statement: 20,
} as const satisfies Record<DeckSemanticRole, number>

const encoder = new TextEncoder()
const UNTRACKED: DeckSourceRange = { source: 'wasmppt:untracked', start: 0, end: 0 }

export interface SourceIdentity { readonly key: string; readonly source?: DeckSourceRange }
export interface EncodedDeck {
  readonly bytes: Uint8Array<ArrayBuffer>
  readonly identities: ReadonlyMap<string, SourceIdentity>
  readonly assetIds: readonly string[]
}

export async function digestHex(bytes: Uint8Array): Promise<string> {
  return hex(new Uint8Array(await crypto.subtle.digest('SHA-256', ownedBuffer(bytes))))
}

export function ownedBuffer(bytes: Uint8Array): ArrayBuffer {
  return new Uint8Array(bytes).buffer
}

export function hex(bytes: Uint8Array): string {
  return Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('')
}

function idBytes(id: string): Uint8Array {
  if (!/^[a-f0-9]{32}$/.test(id)) throw new TypeError('invalid registered asset identity')
  return Uint8Array.from(id.match(/../g)!, byte => Number.parseInt(byte, 16))
}

class Writer {
  readonly chunks: Uint8Array[] = []
  length = 0
  readonly ids: ReadonlyMap<string, Uint8Array>
  readonly assetIds = new Set<string>()
  constructor(ids: ReadonlyMap<string, Uint8Array> = new Map(), readonly assets: ReadonlyMap<string, string> = new Map()) { this.ids = ids }
  raw(bytes: Uint8Array): void {
    this.length += bytes.byteLength
    if (this.length > 64 * 1024 * 1024) throw new RangeError('deck payload exceeds 64 MiB')
    this.chunks.push(bytes)
  }
  integer(value: number, size: 1 | 2 | 4): void {
    if (!Number.isInteger(value) || value < 0 || value > 2 ** (8 * size) - 1) {
      throw new RangeError('invalid deck integer')
    }
    const bytes = new Uint8Array(size)
    const view = new DataView(bytes.buffer)
    if (size === 1) view.setUint8(0, value)
    else if (size === 2) view.setUint16(0, value, true)
    else view.setUint32(0, value, true)
    this.raw(bytes)
  }
  byte(value: number): void { this.integer(value, 1) }
  u32(value: number): void { this.integer(value, 4) }
  bool(value: boolean): void { this.byte(value ? 1 : 0) }
  string(value: string): void {
    const bytes = encoder.encode(value)
    if (bytes.length > 4 * 1024 * 1024) throw new RangeError('deck string exceeds 4 MiB')
    this.u32(bytes.length); this.raw(bytes)
  }
  optionalString(value: string | null | undefined): void {
    this.bool(value != null)
    if (value != null) this.string(value)
  }
  vector<T>(values: readonly T[], write: (value: T) => void): void {
    if (values.length > 100_000) throw new RangeError('too many deck collection items')
    this.u32(values.length)
    for (const value of values) write(value)
  }
  id(key: string): void {
    const value = this.ids.get(key)
    if (value === undefined) throw new TypeError(`unknown semantic key: ${key}`)
    this.raw(value)
  }
  asset(key: string): void { const id = this.assets.get(key) ?? key; this.raw(idBytes(id)); this.assetIds.add(id) }
  source(value: DeckSourceRange): void {
    this.string(value.source); this.u32(value.start); this.u32(value.end)
  }
  richText(value: DeckRichText): void {
    this.vector(value.runs, run => {
      this.string(run.text)
      this.byte(Number(run.marks?.bold ?? false) | (Number(run.marks?.italic ?? false) << 1)
        | (Number(run.marks?.strikethrough ?? false) << 2) | (Number(run.marks?.inlineCode ?? false) << 3))
      this.bool(run.hyperlink != null)
      if (run.hyperlink != null) {
        const tags: Record<DeckHyperlink['kind'], number> = { web: 1, email: 2, telephone: 3, 'source-anchor': 4 }
        this.byte(tags[run.hyperlink.kind]); this.string(run.hyperlink.target)
      }
    })
  }
  list(value: DeckListContent, parent: DeckSourceRange): void {
    this.bool(value.ordered); this.u32(value.start)
    this.vector(value.items, item => {
      const source = item.source ?? parent
      this.id(item.key); this.source(source)
      this.vector(item.blocks, block => this.node(block, source))
      this.vector(item.children, list => this.list(list, source))
    })
  }
  table(value: DeckTableContent, parent: DeckSourceRange): void {
    this.vector(value.columns, column => {
      this.id(column.key); this.source(column.source ?? parent)
      this.byte({ start: 0, center: 1, end: 2 }[column.alignment])
    })
    this.u32(value.headerRows)
    this.vector(value.rows, row => {
      this.id(row.key); this.source(row.source ?? parent)
      this.vector(row.cells, cell => {
        this.id(cell.key); this.source(cell.source ?? row.source ?? parent); this.richText(cell.content)
      })
    })
  }
  node(node: DeckSemanticNode, parent: DeckSourceRange): void {
    const source = node.source ?? parent
    this.id(node.key); this.source(source); this.integer(ROLE_CODES[node.role], 2)
    this.byte({ never: 0, text: 1, 'list-items': 2, 'table-rows': 3, 'code-lines': 4, children: 5 }[node.split ?? 'never'])
    const content = node.content
    switch (content.kind) {
      case 'text': this.byte(1); this.richText(content.text); break
      case 'children': this.byte(2); this.vector(content.nodes, child => this.node(child, source)); break
      case 'image': this.byte(3); this.asset(content.assetId); this.string(content.altText); break
      case 'list': this.byte(4); this.list(content.list, source); break
      case 'table': this.byte(5); this.table(content.table, source); break
      case 'chart':
        this.byte(6)
        this.byte({ bar: 1, column: 2, line: 3, area: 4, pie: 5, doughnut: 6, scatter: 7 }[content.chart.kind])
        this.vector(content.chart.categories, category => this.string(category))
        this.vector(content.chart.series, series => {
          this.string(series.name)
          this.vector(series.values, value => {
            if (!Number.isFinite(value)) throw new TypeError('chart values must be finite')
            const bytes = new Uint8Array(8)
            new DataView(bytes.buffer).setFloat64(0, value, true); this.raw(bytes)
          })
        })
        break
      case 'code': this.byte(7); this.optionalString(content.language); this.string(content.code); break
      case 'svg':
        this.byte(8); this.asset(content.assetId); this.bool(content.fallbackAssetId != null)
        if (content.fallbackAssetId != null) this.asset(content.fallbackAssetId)
        this.optionalString(content.sourceText)
        break
      default: throw new TypeError('unsupported semantic content')
    }
  }
  slide(slide: DeckLogicalSlide): void {
    // Hosts may omit provenance entirely. A partially sourced tree still needs a containing
    // parent span; infer that span from its provided descendants without inventing public source.
    const sources: DeckSourceRange[] = []
    walk(slide, identity => { if (identity.source !== undefined) sources.push(identity.source) })
    const first = sources[0]
    const source = slide.source ?? (first === undefined ? UNTRACKED : {
      source: first.source, start: sources.reduce((minimum, value) => Math.min(minimum, value.start), first.start),
      end: sources.reduce((maximum, value) => Math.max(maximum, value.end), first.end),
    })
    this.id(slide.key); this.source(source); this.byte(slide.kind === 'title' ? 1 : 2)
    this.bool(slide.hidden ?? false)
    this.vector(slide.nodes, node => this.node(node, source))
    this.vector(slide.mediaTextRelations ?? [], relation => {
      this.id(relation.mediaNodeKey); this.id(relation.textNodeKey)
      this.byte({ 'same-paragraph': 1, 'adjacent-blocks': 2, 'blank-separated-blocks': 3 }[relation.proximity])
      this.byte({ 'before-media': 1, 'after-media': 2 }[relation.textSide]); this.bool(relation.explicitCaption)
    })
  }
  finish(): Uint8Array<ArrayBuffer> {
    const output = new Uint8Array(this.length)
    let offset = 0
    for (const chunk of this.chunks) { output.set(chunk, offset); offset += chunk.length }
    return output
  }
}

/** Walk only semantic identities; enforce bounds before recursion/encoding or hashing. */
function walk(value: unknown, visit: (identity: SourceIdentity) => void, depth = 0): void {
  if (depth > 64) throw new RangeError('deck nesting exceeds 64')
  if (value === null || typeof value !== 'object') return
  if (Array.isArray(value)) {
    if (value.length > 100_000) throw new RangeError('too many deck items')
    for (const item of value) walk(item, visit, depth + 1)
    return
  }
  if ('key' in value && typeof value.key === 'string') {
    const identity = value as SourceIdentity
    visit(identity)
  }
  for (const child of Object.values(value)) walk(child, visit, depth + 1)
}

export async function encodeDeck(spec: DeckSpec, assets: ReadonlyMap<string, string> = new Map()): Promise<EncodedDeck> {
  const entries = new Map<string, SourceIdentity>()
  walk(spec, identity => {
    if (!identity.key || entries.has(identity.key)) throw new TypeError(`duplicate or empty semantic key: ${identity.key}`)
    if (entries.size >= 100_000) throw new RangeError('too many semantic identities')
    entries.set(identity.key, { key: identity.key,
      ...(identity.source === undefined ? {} : { source: { ...identity.source } }) })
  })
  const ids = new Map<string, Uint8Array>()
  const identities = new Map<string, SourceIdentity>()
  await Promise.all(Array.from(entries, async ([key, identity]) => {
    const id = (await digestHex(encoder.encode(JSON.stringify(['wasmppt/deck/key/v1', spec.key, key])))).slice(0, 32)
    ids.set(key, idBytes(id)); identities.set(id, identity)
  }))
  const writer = new Writer(ids, assets)
  writer.raw(encoder.encode('WDSF')); writer.u32(5); writer.id(spec.key)
  writer.vector(spec.slides, slide => writer.slide(slide)); writer.u32(0)
  return { bytes: writer.finish(), identities, assetIds: [...writer.assetIds] }
}

export function encodeAsset(id: string, asset: DeckAssetInput): Uint8Array<ArrayBuffer> {
  if (asset.bytes.length === 0 || asset.bytes.length > 32 * 1024 * 1024) throw new RangeError('asset must contain at most 32 MiB')
  const writer = new Writer()
  writer.raw(encoder.encode('WDSF')); writer.u32(5); writer.raw(idBytes(id)); writer.u32(0); writer.u32(1)
  writer.asset(id); writer.byte(asset.mediaType === 'image/svg+xml' ? 2 : 1); writer.string(asset.mediaType)
  writer.u32(asset.bytes.length); writer.raw(asset.bytes); writer.bool(asset.intrinsicSize !== undefined)
  if (asset.intrinsicSize !== undefined) { writer.u32(asset.intrinsicSize.width); writer.u32(asset.intrinsicSize.height) }
  return writer.finish()
}
