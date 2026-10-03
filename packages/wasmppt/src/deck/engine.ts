import { decodeDisplayList, type DisplayScene } from '../canvas.js'
import type { WasmpptWorkerClient } from '../worker-client.js'
import type { DeckPageMetadata } from '../protocol.js'
import { diagnostics, inputFailure, invalidInput } from './diagnostics.js'
import { digestHex, encodeAsset, encodeDeck, ownedBuffer, type SourceIdentity } from './encode.js'
import type {
  DeckAssetInput, DeckDiagnostic, DeckFontInput, DeckResult, DeckSourceRange, DeckSpec,
  DeckTemplateDescription,
} from './model.js'

export interface DeckEngineOptions { readonly maxRetainedBytes?: number }
export interface DeckOperationOptions { readonly signal?: AbortSignal }
export interface DeckInput { readonly template: DeckTemplate; readonly spec: DeckSpec; readonly assets?: ReadonlyMap<string, DeckAsset>; readonly fonts?: DeckFonts }
export interface DeckPage {
  readonly id: string
  readonly logicalSlideKey: string
  readonly index: number
  readonly hidden: boolean
  readonly continuationOrdinal: number
  readonly continuationTotal: number
  readonly continuationLabel?: string
}

interface Resource<T> { readonly value: T; refs: number; readonly release: () => Promise<void> }
function hold<T>(resource: Resource<T>): () => Promise<void> {
  resource.refs += 1
  let released = false
  return () => {
    if (released) return Promise.resolve()
    released = true
    return release(resource)
  }
}
function release<T>(resource: Resource<T>): Promise<void> {
  resource.refs -= 1
  return resource.refs === 0 ? resource.release() : Promise.resolve()
}
function abort(signal?: AbortSignal): void { signal?.throwIfAborted() }
function closed(): Error { return new Error('wasmppt object is disposed') }

const templates = new WeakMap<DeckTemplate, { engine: DeckEngine; resource: Resource<number>; disposed: boolean }>()
const fontSets = new WeakMap<DeckFonts, { engine: DeckEngine; resource: Resource<readonly RegisteredFont[]>; disposed: boolean }>()
interface RegisteredFont { readonly handle: number; readonly family: string; readonly bytes: Uint8Array }

export class DeckTemplate {
  /** @internal */
  constructor(engine: DeckEngine, handle: number, readonly description: DeckTemplateDescription) {
    templates.set(this, { engine, disposed: false, resource: { value: handle, refs: 1,
      release: () => engine.isDisposed ? Promise.resolve() : engine.client.releaseDeckTemplate(handle) } })
  }
  describe(): DeckTemplateDescription { return this.description }
  async dispose(): Promise<void> {
    const state = templates.get(this)!
    if (state.disposed) return
    state.disposed = true
    await release(state.resource)
  }
}

const assetLeases = new WeakMap<DeckAsset, { engine: DeckEngine; disposed: boolean }>()

/** One caller-owned registration lease. Equal inputs share bytes and the same content ID. */
export class DeckAsset {
  /** @internal */
  constructor(engine: DeckEngine, readonly id: string, private readonly releaseLease: () => Promise<void>) {
    assetLeases.set(this, { engine, disposed: false })
  }
  async dispose(): Promise<void> {
    const state = assetLeases.get(this)!
    if (state.disposed) return
    state.disposed = true
    await this.releaseLease()
  }
}

export class DeckFonts {
  /** @internal */
  constructor(engine: DeckEngine, values: readonly RegisteredFont[]) {
    fontSets.set(this, { engine, disposed: false, resource: { value: values, refs: 1,
      release: async () => {
        if (!engine.isDisposed) await Promise.all(values.map(font => engine.client.deckOperation({ kind: 'release-input', handle: font.handle })))
      } } })
  }
  async dispose(): Promise<void> {
    const state = fontSets.get(this)!
    if (state.disposed) return
    state.disposed = true
    await release(state.resource)
  }
}

/** Owns one host runtime. Inputs are copied by default; no caller buffer is detached. */
export class DeckEngine {
  readonly #disposal = new Set<() => void>()
  readonly #lifetime = new AbortController()
  readonly #assets = new Map<string, Resource<number>>()
  readonly #registering = new Map<string, Promise<Resource<number>>>()
  #disposed = false
  /** @internal */
  constructor(readonly client: WasmpptWorkerClient) {}
  get isDisposed(): boolean { return this.#disposed }
  get signal(): AbortSignal { return this.#lifetime.signal }
  /** @internal */
  onDispose(cleanup: () => void): void { this.assertOpen(); this.#disposal.add(cleanup) }
  assertOpen(): void { if (this.#disposed) throw closed() }

  async prepareDeckTemplate(bytes: Uint8Array | ArrayBuffer): Promise<DeckResult<DeckTemplate>> {
    this.assertOpen()
    let handle: number | undefined
    try {
      const prepared = await this.client.prepareDeckTemplate(bytes instanceof Uint8Array ? ownedBuffer(bytes) : bytes.slice(0))
      handle = prepared.handle
      const result = await this.client.deckOperation({ kind: 'describe-template', handle })
      if (result.kind !== 'description') throw new Error('invalid template description response')
      const report = diagnostics(result.diagnostics)
      if (!prepared.cacheable || report.some(item => item.severity === 'error')) {
        await this.client.releaseDeckTemplate(handle)
        return { ok: false, diagnostics: report }
      }
      const description = deepFreeze({ ...result.description, diagnostics: report })
      return { ok: true, value: new DeckTemplate(this, handle, description), diagnostics: report }
    } catch (error) {
      if (handle !== undefined && !this.#disposed) await this.client.releaseDeckTemplate(handle)
      return { ok: false, diagnostics: inputFailure(error) }
    }
  }

  async registerAsset(input: DeckAssetInput): Promise<DeckAsset> {
    this.assertOpen()
    const asset = { ...input, bytes: new Uint8Array(input.bytes),
      ...(input.intrinsicSize === undefined ? {} : { intrinsicSize: { ...input.intrinsicSize } }) }
    const canonical = encodeAsset('00000000000000000000000000000001', asset)
    const id = (await digestHex(canonical)).slice(0, 32)
    this.assertOpen()
    let resource = this.#assets.get(id)
    if (resource === undefined) {
      let pending = this.#registering.get(id)
      if (pending === undefined) {
        pending = this.#registerAsset(id, asset)
        this.#registering.set(id, pending)
      }
      try { resource = await pending } finally { this.#registering.delete(id) }
    }
    resource.refs += 1
    return new DeckAsset(this, id, () => release(resource))
  }
  async #registerAsset(id: string, input: DeckAssetInput): Promise<Resource<number>> {
    const result = await this.client.deckOperation({ kind: 'register-asset', bytes: encodeAsset(id, input).buffer })
    if (result.kind !== 'handle') throw new Error('invalid asset registration response')
    const handle = result.handle
    const resource: Resource<number> = { value: handle, refs: 0, release: async () => {
      if (this.#assets.get(id) === resource) this.#assets.delete(id)
      if (!this.#disposed) await this.client.deckOperation({ kind: 'release-input', handle })
    } }
    this.#assets.set(id, resource)
    return resource
  }

  async registerFonts(inputs: readonly DeckFontInput[]): Promise<DeckFonts> {
    this.assertOpen()
    const owned = inputs.map(input => ({ family: input.family, bytes: new Uint8Array(input.bytes) }))
    if (new Set(owned.map(font => font.family.toLowerCase())).size !== owned.length) throw new TypeError('font families must be unique')
    const registered: RegisteredFont[] = []
    try {
      for (const font of owned) {
        const result = await this.client.deckOperation({ kind: 'register-font', family: font.family, bytes: ownedBuffer(font.bytes) })
        if (result.kind !== 'handle') throw new Error('invalid font registration response')
        registered.push({ ...font, handle: result.handle })
      }
      return new DeckFonts(this, registered)
    } catch (error) {
      if (!this.#disposed) await Promise.all(registered.map(font => this.client.deckOperation({ kind: 'release-input', handle: font.handle })))
      throw error
    }
  }

  createDeck(): Deck { this.assertOpen(); return new Deck(this) }
  async accountedBytes(): Promise<number> {
    const result = await this.client.deckOperation({ kind: 'accounting' })
    if (result.kind !== 'accounting') throw new Error('invalid memory accounting response')
    return result.bytes
  }
  dispose(): void {
    if (this.#disposed) return
    this.#disposed = true; this.#lifetime.abort();
    for (const cleanup of this.#disposal) cleanup()
    this.#disposal.clear(); this.#assets.clear(); this.#registering.clear(); this.client.terminate()
  }

  /** @internal Capture registration leases before asynchronous encoding or queued replacement. */
  capture(input: DeckInput) {
    this.assertOpen()
    const template = templates.get(input.template)
    if (template === undefined || template.disposed || template.engine !== this) throw new Error('template is not live on this engine')
    const fonts = input.fonts === undefined ? undefined : fontSets.get(input.fonts)
    if (input.fonts !== undefined && (fonts === undefined || fonts.disposed || fonts.engine !== this)) throw new Error('fonts are not live on this engine')
    const bindings = new Map<string, string>()
    for (const [key, asset] of input.assets ?? []) {
      const registered = assetLeases.get(asset)
      if (registered === undefined || registered.disposed || registered.engine !== this) throw new Error('asset is not live on this engine')
      bindings.set(key, asset.id)
    }
    const spec = structuredClone(input.spec)
    // Capturing the registry is cheap: its entries are handles, not media copies. Only the
    // referenced handles enter the wire request; these short leases end when replace settles.
    const assets = new Map(this.#assets)
    const releases = [hold(template.resource), ...Array.from(assets.values(), hold)]
    if (fonts !== undefined) releases.push(hold(fonts.resource))
    return { spec, bindings, template: template.resource.value, assets,
      fonts: fonts?.resource.value ?? [],
      fontResource: fonts?.resource,
      release: () => Promise.all(releases.map(releaseLease => releaseLease())) }
  }
}

export class Deck {
  #current: DeckSnapshot | undefined
  #queue: Promise<unknown> = Promise.resolve()
  #disposed = false
  /** @internal */
  constructor(readonly engine: DeckEngine) {}

  replace(input: DeckInput, options: DeckOperationOptions = {}): Promise<DeckResult<DeckSnapshot>> {
    if (this.#disposed) return Promise.reject(closed())
    const captured = this.engine.capture(input)
    const operation = this.#queue.then(async (): Promise<DeckResult<DeckSnapshot>> => {
      this.engine.assertOpen()
      if (this.#disposed) throw closed()
      abort(options.signal)
      try {
        let encoded: Awaited<ReturnType<typeof encodeDeck>>
        try { encoded = await encodeDeck(captured.spec, captured.bindings) }
        catch (error) {
          if (error instanceof TypeError || error instanceof RangeError) return { ok: false, diagnostics: invalidInput(error) }
          throw error
        }
        const assets = encoded.assetIds.map(id => {
          const asset = captured.assets.get(id)
          if (asset === undefined) throw new Error(`unregistered asset: ${id}`)
          return asset.value
        })
        abort(options.signal)
        const result = await this.engine.client.deckOperation({ kind: 'snapshot', template: captured.template,
          spec: encoded.bytes.buffer, assets, fonts: captured.fonts.map(font => font.handle),
          previous: this.#current === undefined ? 0 : accessSnapshot(this.#current).handle })
        if (result.kind === 'rejected') return { ok: false, diagnostics: diagnostics(result.diagnostics, encoded.identities) }
        if (result.kind !== 'snapshot') throw new Error('invalid snapshot response')
        // Cancellation before publication discards the speculative revision. Once published,
        // success stays success; a late abort never pretends to roll back an accepted revision.
        if (options.signal?.aborted || this.#disposed) {
          await this.engine.client.releaseDeckSession(result.handle)
          abort(options.signal); throw closed()
        }
        const report = diagnostics(result.diagnostics, encoded.identities)
        const releaseFonts = captured.fontResource === undefined ? () => Promise.resolve() : hold(captured.fontResource)
        const snapshot = new DeckSnapshot({ engine: this.engine, handle: result.handle, revision: result.revision,
          identities: new Map([...encoded.identities, ...result.sources.flatMap(([fragment, node]) => {
            const identity = encoded.identities.get(node)
            return identity === undefined ? [] : [[fragment, identity] as const]
          })]), fonts: captured.fonts, releaseFonts, diagnostics: report,
          rawPages: result.pages,
          pages: Object.freeze(result.pages.map((page, index) => Object.freeze({ index, id: page.pageId,
            hidden: page.hidden, continuationOrdinal: page.continuationOrdinal, continuationTotal: page.continuationTotal,
            ...(page.continuationLabel === undefined ? {} : { continuationLabel: page.continuationLabel }),
            logicalSlideKey: encoded.identities.get(page.logicalSlideId)!.key }))) })
        const previous = this.#current
        this.#current = snapshot.retain()
        await previous?.dispose()
        return { ok: true, value: snapshot, diagnostics: report }
      } catch (error) { return { ok: false, diagnostics: inputFailure(error) } }
    }).finally(() => captured.release())
    this.#queue = operation.catch(() => {})
    return operation
  }
  async dispose(): Promise<void> {
    if (this.#disposed) return
    this.#disposed = true
    await this.#queue
    await this.#current?.dispose(); this.#current = undefined
  }
}

interface SnapshotData {
  readonly engine: DeckEngine
  readonly handle: number
  readonly revision: number
  readonly rawPages: readonly DeckPageMetadata[]
  readonly pages: readonly DeckPage[]
  readonly diagnostics: readonly DeckDiagnostic[]
  readonly identities: ReadonlyMap<string, SourceIdentity>
  readonly fonts: readonly RegisteredFont[]
  readonly releaseFonts: () => Promise<void>
}
const snapshots = new WeakMap<DeckSnapshot, { readonly resource: Resource<SnapshotData>; disposed: boolean }>()

/** Immutable accepted revision. Retain an independent lease when sharing it across owners. */
export class DeckSnapshot {
  /** @internal */
  constructor(data: SnapshotData, shared?: Resource<SnapshotData>) {
    const resource = shared ?? { value: data, refs: 1, release: async () => {
      try { if (!data.engine.isDisposed) await data.engine.client.releaseDeckSession(data.handle) }
      finally { await data.releaseFonts() }
    } }
    snapshots.set(this, { resource, disposed: false })
  }
  get revision(): number { return accessSnapshot(this).revision }
  get pages(): readonly DeckPage[] { return accessSnapshot(this).pages }
  get diagnostics(): readonly DeckDiagnostic[] { return accessSnapshot(this).diagnostics }
  retain(): DeckSnapshot {
    const data = accessSnapshot(this)
    const resource = snapshots.get(this)!.resource
    resource.refs += 1
    return new DeckSnapshot(data, resource)
  }
  async dispose(): Promise<void> {
    const state = snapshots.get(this)!
    if (state.disposed) return
    state.disposed = true; await release(state.resource)
  }
  sourceIdentity(id: string): { readonly key: string; readonly source?: DeckSourceRange } | undefined {
    const value = accessSnapshot(this).identities.get(id)
    return value === undefined ? undefined : { key: value.key,
      ...(value.source === undefined ? {} : { source: { ...value.source } }) }
  }
  async resolvePage(pageId: string, options: DeckOperationOptions = {}): Promise<DisplayScene> {
    const held = this.retain()
    try {
      const state = accessSnapshot(held)
      const page = requirePage(state, pageId)
      abort(options.signal)
      const result = await state.engine.client.resolveDeckSlide(state.handle, state.revision, page.index, options)
      abort(options.signal)
      return decodeDisplayList(result.displayList)
    } finally { await held.dispose() }
  }
  /** Pull-driven output holds its own lease until the stream is consumed or cancelled. */
  exportPptx(options: DeckOperationOptions = {}): ReadableStream<Uint8Array> {
    const held = this.retain()
    const state = accessSnapshot(held)
    const signal = AbortSignal.any([state.engine.signal, ...(options.signal === undefined ? [] : [options.signal])])
    let cursor: number | undefined
    let finished = false
    let pending: Promise<void> = Promise.resolve()
    const finish = async (): Promise<void> => {
      if (finished) return
      finished = true
      signal?.removeEventListener('abort', onAbort)
      try {
        if (cursor !== undefined && !state.engine.isDisposed) await state.engine.client.deckOperation({ kind: 'output-release', handle: cursor })
      } finally { await held.dispose() }
    }
    let controller: ReadableStreamDefaultController<Uint8Array>
    const onAbort = (): void => {
      controller.error(signal?.reason)
      void pending.finally(finish).catch(() => {})
    }
    return new ReadableStream<Uint8Array>({
      start(value) {
        controller = value
        signal?.addEventListener('abort', onAbort, { once: true })
        if (signal?.aborted) onAbort()
      },
      pull(value) {
        pending = (async () => {
          try {
            abort(signal)
            if (cursor === undefined) {
              const started = await state.engine.client.deckOperation({ kind: 'output-start', handle: state.handle, revision: state.revision })
              if (started.kind !== 'handle') throw new Error('invalid output cursor response')
              cursor = started.handle
            }
            const result = await state.engine.client.deckOperation({ kind: 'output-pull', handle: cursor, chunkBytes: 256 * 1024 })
            if (result.kind !== 'chunk') throw new Error('invalid output chunk response')
            abort(signal)
            if (result.bytes.byteLength > 0) value.enqueue(new Uint8Array(result.bytes))
            if (result.done) { value.close(); await finish() }
          } catch (error) { value.error(error); await finish() }
        })()
        return pending
      },
      async cancel() { await pending; await finish() },
    })
  }
}

/** @internal Borrow only while the caller holds a live snapshot lease. */
export function accessSnapshot(snapshot: DeckSnapshot): SnapshotData {
  const state = snapshots.get(snapshot)
  if (state === undefined || state.disposed) throw closed()
  state.resource.value.engine.assertOpen()
  return state.resource.value
}
export function requirePage(state: SnapshotData, id: string): DeckPage {
  const page = state.pages.find(candidate => candidate.id === id)
  if (page === undefined) throw new RangeError('page does not belong to this snapshot')
  return page
}
function deepFreeze<T>(value: T): T {
  if (value !== null && typeof value === 'object' && !Object.isFrozen(value)) {
    for (const child of Object.values(value)) deepFreeze(child)
    Object.freeze(value)
  }
  return value
}

/** @internal */
export async function initializeDeckEngine(client: WasmpptWorkerClient, options: DeckEngineOptions): Promise<DeckEngine> {
  try {
    if (options.maxRetainedBytes !== undefined) {
      if (!Number.isSafeInteger(options.maxRetainedBytes) || options.maxRetainedBytes < 1 || options.maxRetainedBytes > 256 * 1024 * 1024) {
        throw new RangeError('maxRetainedBytes must be an integer between 1 and 256 MiB')
      }
      await client.deckOperation({ kind: 'configure', maxRetainedBytes: options.maxRetainedBytes })
    }
    return new DeckEngine(client)
  } catch (error) { client.terminate(); throw error }
}
