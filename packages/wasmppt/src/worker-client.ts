import {
  LEGACY_WORKER_PROTOCOL_VERSION,
  WORKER_PROTOCOL_VERSION,
  type DeckSessionUpdate,
  type DeckPageMetadata,
  type TemplateBinding,
  type TemplateCompilerOptions,
  type TemplateDiagnostic,
  type TextBindings,
  type WorkerRequest,
  type WorkerResponse,
} from './protocol.js'
import {
  ERROR_ENVELOPE_VERSION,
  WasmpptError,
  cancellationEnvelope,
  isWasmpptErrorEnvelope,
} from './error.js'
import { encodeInjectionData, type GenerationData } from './injection.js'

export interface WorkerLike {
  postMessage(message: WorkerRequest, transfer?: readonly Transferable[]): void
  addEventListener(type: 'message', listener: (event: MessageEvent<unknown>) => void): void
  addEventListener(type: 'error' | 'messageerror', listener: (event: Event) => void): void
  removeEventListener(type: 'message', listener: (event: MessageEvent<unknown>) => void): void
  removeEventListener(type: 'error' | 'messageerror', listener: (event: Event) => void): void
  terminate(): void
}

export interface PreparedBrowserTemplate {
  /** Worker-owned opaque handle. Release it with `WasmpptWorkerClient.release`. */
  readonly handle: number
  /** Conservative resident-byte weight used for host cache budgets. */
  readonly residentBytes: number
  /** Caller-owned serialized plan bytes suitable for a later `prepare` call. */
  readonly plan: ArrayBuffer
  readonly bindings: readonly TemplateBinding[]
  readonly diagnostics: readonly TemplateDiagnostic[]
}

export interface PreparedDeckTemplate {
  readonly handle: number
  readonly cacheable: boolean
  readonly plan: ArrayBuffer
}

export interface OpenedDeckSession {
  readonly handle: number
  readonly revision: number
  readonly slideCount: number
  readonly presentableSlides: readonly number[]
  readonly plan: ArrayBuffer
}

export interface ResolvedDeckSlide {
  readonly handle: number
  readonly revision: number
  readonly slideIndex: number
  readonly fingerprint: string
  readonly page: DeckPageMetadata
  readonly displayList: ArrayBuffer
}

export interface PrepareOptions extends TemplateCompilerOptions {
  readonly plan?: ArrayBuffer
}

export interface GenerateOptions {
  /** Cooperatively abort work; cancellation releases the generation cursor only. */
  readonly signal?: AbortSignal
  /** Positive maximum bytes requested from the Worker per output chunk. */
  readonly chunkBytes?: number
  readonly onProgress?: (phase: 'generate' | 'stream', completed: number, total: number) => void
}

export interface OpenedBrowserPresentation {
  readonly handle: number
  readonly slideCount: number
}

export interface ResolveSlideOptions {
  readonly signal?: AbortSignal
  readonly onProgress?: (phase: 'open' | 'resolve', completed: number, total: number) => void
}

export interface OpenedLiveSession {
  readonly handle: number
  readonly revision: number
  readonly slideCount: number
}

export interface LiveSessionUpdate {
  readonly handle: number
  readonly revision: number
  readonly graphChanged: boolean
  readonly fullFallback: boolean
  readonly invalidationReason: 'topology' | 'dependency' | 'none'
  readonly slideCount: number
  readonly invalidatedSlides: readonly number[]
  readonly changedBindings: readonly string[]
  readonly changedParts: readonly string[]
  readonly overlay: {
    readonly reusedMaterializedParts: number
    readonly logicalParts: number
    readonly materializedParts: number
    readonly materializedBytes: number
    readonly reusedSourceBytes: number
    readonly removedParts: number
  }
}

export interface ResolvedLiveSlide {
  readonly handle: number
  readonly revision: number
  readonly slideIndex: number
  readonly fingerprint: string
  readonly displayList: ArrayBuffer
}

export interface LiveSessionResource {
  readonly fingerprint: string
  readonly bytes: ArrayBuffer
}

export interface LiveSessionCacheTelemetry {
  readonly residentBytes: number
  readonly peakBytes: number
  readonly entries: number
  readonly hits: number
  readonly misses: number
  readonly evictions: number
}

type Pending =
  | {
      readonly kind: 'prepare' | 'release' | 'open' | 'resolve' | 'resource' | 'metafile' |
        'release-presentation' | 'session' | 'delta' | 'release-session' | 'telemetry'
      readonly resolve: (value: WorkerResponse) => void
      readonly reject: (error: Error) => void
      readonly onProgress?: ResolveSlideOptions['onProgress']
    }
  | {
      readonly kind: 'generate'
      readonly controller: ReadableStreamDefaultController<Uint8Array>
      readonly onProgress: GenerateOptions['onProgress']
      readonly abortCleanup: () => void
    }

/**
 * Main-thread client that settles every request on completion, abort, or Worker crash.
 *
 * Template and presentation handles belong to this client's Worker. Release each handle explicitly
 * in long-lived clients, then call `terminate()` during final teardown.
 */
export class WasmpptWorkerClient {
  readonly #worker: WorkerLike
  readonly #pending = new Map<number, Pending>()
  #nextId = 1
  #closed = false
  readonly #resourceCache = new Map<string, ArrayBuffer>()
  readonly #resourceInflight = new Map<string, Promise<ArrayBuffer>>()
  readonly #releasedPresentations = new Set<number>()
  readonly #releasedLiveSessions = new Set<number>()
  readonly #releasedDeckSessions = new Set<number>()
  readonly #deckRevisions = new Map<number, number>()
  readonly #liveResourceFingerprints = new Map<string, string>()
  readonly #resourceCacheLimit: number
  #resourceCacheBytes = 0

  readonly #onMessage = (event: MessageEvent<unknown>): void => this.#receive(event.data)
  readonly #onCrash = (): void => this.#failAll(new Error('wasmppt Worker terminated unexpectedly'))

  constructor(worker: WorkerLike, resourceCacheBytes = 32 * 1024 * 1024) {
    if (!Number.isSafeInteger(resourceCacheBytes) || resourceCacheBytes < 0) {
      throw new RangeError('resourceCacheBytes must be a non-negative safe integer')
    }
    this.#worker = worker
    this.#resourceCacheLimit = resourceCacheBytes
    worker.addEventListener('message', this.#onMessage)
    worker.addEventListener('error', this.#onCrash)
    worker.addEventListener('messageerror', this.#onCrash)
  }

  get resourceCacheBytes(): number {
    return this.#resourceCacheBytes
  }

  /** Transfer and compile a template. `template` and an optional `plan` are detached immediately. */
  async prepare(template: ArrayBuffer, options: PrepareOptions = {}): Promise<PreparedBrowserTemplate> {
    this.#assertOpen()
    const id = this.#allocateId()
    const result = new Promise<WorkerResponse>((resolve, reject) => {
      this.#pending.set(id, { kind: 'prepare', resolve, reject })
    })
    const { plan, ...compilerOptions } = options
    const transfer: Transferable[] = [template]
    if (plan !== undefined) transfer.push(plan)
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'prepare',
      template,
      options: compilerOptions,
      ...(plan === undefined ? {} : { plan }),
    }, transfer)
    const response = await result
    if (response.type !== 'prepared') throw new Error('invalid prepare response')
    return {
      handle: response.templateHandle,
      residentBytes: response.residentBytes,
      plan: response.plan,
      bindings: response.bindings,
      diagnostics: response.diagnostics,
    }
  }

  /** Transfer and compile a POTX governed by the deck template contract. */
  async prepareDeckTemplate(template: ArrayBuffer, plan?: ArrayBuffer): Promise<PreparedDeckTemplate> {
    this.#assertOpen()
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'prepare')
    const transfer: Transferable[] = [template]
    if (plan !== undefined) transfer.push(plan)
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'prepare-deck-template',
      template,
      ...(plan === undefined ? {} : { plan }),
    }, transfer)
    const response = await result
    if (response.type !== 'deck-template-prepared') {
      throw new Error('invalid deck template response')
    }
    return {
      handle: response.templateHandle,
      cacheable: response.cacheable,
      plan: response.plan,
    }
  }

  /** Create a revisioned authoring session from caller-owned WDSF and optional WDPL bytes. */
  async createDeckSession(
    templateHandle: number,
    spec: ArrayBuffer,
    plan?: ArrayBuffer,
    options: ResolveSlideOptions = {},
  ): Promise<OpenedDeckSession> {
    this.#assertOpen()
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'session', options.signal, options.onProgress)
    const transfer: Transferable[] = [spec]
    if (plan !== undefined) transfer.push(plan)
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'create-deck-session',
      templateHandle,
      spec,
      ...(plan === undefined ? {} : { plan }),
    }, transfer)
    const response = await result
    if (response.type !== 'deck-session-created') throw new Error('invalid deck session response')
    this.#releasedDeckSessions.delete(response.sessionHandle)
    this.#deckRevisions.set(response.sessionHandle, response.revision)
    return {
      handle: response.sessionHandle,
      revision: response.revision,
      slideCount: response.slideCount,
      presentableSlides: response.presentableSlides,
      plan: response.plan,
    }
  }

  async updateDeckSession(
    sessionHandle: number,
    expectedRevision: number,
    spec: ArrayBuffer,
    options: ResolveSlideOptions = {},
  ): Promise<DeckSessionUpdate> {
    this.#assertOpen()
    assertRevision(expectedRevision)
    const nextRevision = expectedRevision + 1
    assertRevision(nextRevision)
    if (this.#deckRevisions.get(sessionHandle) !== expectedRevision) {
      throw new Error('deck session update does not target the current client revision')
    }
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'delta', options.signal, options.onProgress)
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'update-deck-session',
      sessionHandle,
      expectedRevision,
      nextRevision,
      spec,
    }, [spec])
    const response = await result
    if (response.type !== 'deck-session-updated') throw new Error('invalid deck update response')
    this.#deckRevisions.set(sessionHandle, response.revision)
    const prefix = `${sessionHandle}\0`
    if (response.fullFallback) {
      for (const key of this.#liveResourceFingerprints.keys()) {
        if (key.startsWith(prefix)) this.#liveResourceFingerprints.delete(key)
      }
    } else {
      for (const partName of response.changedParts) {
        this.#liveResourceFingerprints.delete(`${prefix}${partName}`)
      }
    }
    return response
  }

  async resolveDeckSlide(
    sessionHandle: number,
    revision: number,
    slideIndex: number,
    options: ResolveSlideOptions = {},
  ): Promise<ResolvedDeckSlide> {
    this.#assertOpen()
    assertRevision(revision)
    if (!Number.isSafeInteger(slideIndex) || slideIndex < 0) {
      throw new RangeError('slideIndex must be a non-negative safe integer')
    }
    if (this.#deckRevisions.get(sessionHandle) !== revision) {
      throw new Error('deck slide request targets a stale revision')
    }
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'resolve', options.signal, options.onProgress)
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'resolve-deck-slide',
      sessionHandle,
      revision,
      slideIndex,
    })
    const response = await result
    if (response.type !== 'deck-slide-resolved') throw new Error('invalid deck resolve response')
    if (this.#deckRevisions.get(sessionHandle) !== response.revision) {
      throw new Error('discarded stale deck slide result')
    }
    return {
      handle: response.sessionHandle,
      revision: response.revision,
      slideIndex: response.slideIndex,
      fingerprint: response.fingerprint,
      page: response.page,
      displayList: response.displayList,
    }
  }

  async deckSessionResource(
    sessionHandle: number,
    revision: number,
    partName: string,
    options: ResolveSlideOptions = {},
  ): Promise<LiveSessionResource> {
    this.#assertOpen()
    const fingerprint = await this.deckSessionResourceFingerprint(
      sessionHandle,
      revision,
      partName,
      options,
    )
    const cacheKey = `content\0deck\0${fingerprint}`
    const cached = this.#resourceCache.get(cacheKey)
    if (cached !== undefined) return { fingerprint, bytes: cached }
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'resource', options.signal, options.onProgress)
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'deck-session-resource',
      sessionHandle,
      revision,
      partName,
    })
    const response = await result
    if (response.type !== 'deck-session-resource' || response.fingerprint !== fingerprint) {
      throw new Error('deck resource changed during one revision')
    }
    if (this.#deckRevisions.get(sessionHandle) !== revision) {
      throw new Error('discarded stale deck resource result')
    }
    if (!this.#releasedDeckSessions.has(sessionHandle)) this.#storeResource(cacheKey, response.bytes)
    return { fingerprint, bytes: response.bytes }
  }

  async deckSessionResourceFingerprint(
    sessionHandle: number,
    revision: number,
    partName: string,
    options: ResolveSlideOptions = {},
  ): Promise<string> {
    this.#assertOpen()
    assertRevision(revision)
    if (partName.length === 0) throw new TypeError('partName must not be empty')
    if (this.#deckRevisions.get(sessionHandle) !== revision) {
      throw new Error('deck resource request targets a stale revision')
    }
    const key = `${sessionHandle}\0${partName}`
    const cached = this.#liveResourceFingerprints.get(key)
    if (cached !== undefined) return cached
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'resource', options.signal, options.onProgress)
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'deck-session-resource-fingerprint',
      sessionHandle,
      revision,
      partName,
    })
    const response = await result
    if (response.type !== 'deck-session-resource-fingerprint') {
      throw new Error('invalid deck resource fingerprint response')
    }
    if (this.#deckRevisions.get(sessionHandle) !== response.revision) {
      throw new Error('discarded stale deck resource fingerprint')
    }
    this.#liveResourceFingerprints.set(key, response.fingerprint)
    return response.fingerprint
  }

  async deckSessionCacheTelemetry(sessionHandle: number): Promise<LiveSessionCacheTelemetry> {
    this.#assertOpen()
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'telemetry')
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'deck-session-cache-telemetry',
      sessionHandle,
    })
    const response = await result
    if (response.type !== 'deck-session-cache-telemetry') {
      throw new Error('invalid deck cache telemetry response')
    }
    return response
  }

  async releaseDeckSession(sessionHandle: number): Promise<void> {
    this.#assertOpen()
    this.#releasedDeckSessions.add(sessionHandle)
    this.#deckRevisions.delete(sessionHandle)
    const prefix = `${sessionHandle}\0`
    for (const key of this.#liveResourceFingerprints.keys()) {
      if (key.startsWith(prefix)) this.#liveResourceFingerprints.delete(key)
    }
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'release-session')
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'release-deck-session',
      sessionHandle,
    })
    const response = await result
    if (response.type !== 'deck-session-released') throw new Error('invalid deck release response')
  }

  async releaseDeckTemplate(templateHandle: number): Promise<void> {
    this.#assertOpen()
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'release')
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'release-deck-template',
      templateHandle,
    })
    const response = await result
    if (response.type !== 'deck-template-released') throw new Error('invalid deck template release response')
  }

  async createLiveSession(
    templateHandle: number,
    initialData: GenerationData | TextBindings,
    options: ResolveSlideOptions = {},
  ): Promise<OpenedLiveSession> {
    this.#assertOpen()
    const id = this.#allocateId()
    const payload = encodeInjectionData(normalizeGenerationData(initialData))
    const result = this.#unaryRequest(id, 'session', options.signal, options.onProgress)
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'create-live-session',
      templateHandle,
      payload,
    }, [payload])
    const response = await result
    if (response.type !== 'live-session-created') throw new Error('invalid live session response')
    this.#releasedLiveSessions.delete(response.sessionHandle)
    return {
      handle: response.sessionHandle,
      revision: response.revision,
      slideCount: response.slideCount,
    }
  }

  async applyLiveDelta(
    sessionHandle: number,
    expectedRevision: number,
    delta: GenerationData | TextBindings,
    options: ResolveSlideOptions = {},
  ): Promise<LiveSessionUpdate> {
    this.#assertOpen()
    assertRevision(expectedRevision)
    const nextRevision = expectedRevision + 1
    if (!Number.isSafeInteger(nextRevision) || nextRevision > 0xffff_ffff) {
      throw new RangeError('live session revision is exhausted')
    }
    const id = this.#allocateId()
    const payload = encodeInjectionData(normalizeGenerationData(delta))
    const result = this.#unaryRequest(id, 'delta', options.signal, options.onProgress)
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'apply-live-delta',
      sessionHandle,
      expectedRevision,
      nextRevision,
      payload,
    }, [payload])
    const response = await result
    if (response.type !== 'live-session-updated') throw new Error('invalid live delta response')
    const prefix = `${sessionHandle}\0`
    if (response.fullFallback) {
      for (const key of this.#liveResourceFingerprints.keys()) {
        if (key.startsWith(prefix)) this.#liveResourceFingerprints.delete(key)
      }
    } else {
      for (const partName of response.changedParts) {
        this.#liveResourceFingerprints.delete(`${prefix}${partName}`)
      }
    }
    return {
      handle: response.sessionHandle,
      revision: response.revision,
      graphChanged: response.graphChanged,
      fullFallback: response.fullFallback,
      invalidationReason: response.invalidationReason,
      slideCount: response.slideCount,
      invalidatedSlides: response.invalidatedSlides,
      changedBindings: response.changedBindings,
      changedParts: response.changedParts,
      overlay: response.overlay,
    }
  }

  async resolveLiveSlide(
    sessionHandle: number,
    revision: number,
    slideIndex: number,
    options: ResolveSlideOptions = {},
  ): Promise<ResolvedLiveSlide> {
    this.#assertOpen()
    assertRevision(revision)
    if (!Number.isSafeInteger(slideIndex) || slideIndex < 0) {
      throw new RangeError('slideIndex must be a non-negative safe integer')
    }
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'resolve', options.signal, options.onProgress)
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'resolve-live-slide',
      sessionHandle,
      revision,
      slideIndex,
    })
    const response = await result
    if (response.type !== 'live-slide-resolved') throw new Error('invalid live resolve response')
    return {
      handle: response.sessionHandle,
      revision: response.revision,
      slideIndex: response.slideIndex,
      fingerprint: response.fingerprint,
      displayList: response.displayList,
    }
  }

  async liveSessionResource(
    sessionHandle: number,
    revision: number,
    partName: string,
    options: ResolveSlideOptions = {},
  ): Promise<LiveSessionResource> {
    return this.#liveResource(sessionHandle, revision, partName, false, options)
  }

  async liveSessionResourceFingerprint(
    sessionHandle: number,
    revision: number,
    partName: string,
    options: ResolveSlideOptions = {},
  ): Promise<string> {
    this.#assertOpen()
    assertRevision(revision)
    if (partName.length === 0) throw new TypeError('partName must not be empty')
    const fingerprintKey = `${sessionHandle}\0${partName}`
    const cached = this.#liveResourceFingerprints.get(fingerprintKey)
    if (cached !== undefined) return cached
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'resource', options.signal, options.onProgress)
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'live-session-resource-fingerprint',
      sessionHandle,
      revision,
      partName,
    })
    const response = await result
    if (response.type !== 'live-session-resource-fingerprint') {
      throw new Error('invalid live resource fingerprint response')
    }
    if (!this.#releasedLiveSessions.has(sessionHandle)) {
      this.#liveResourceFingerprints.set(fingerprintKey, response.fingerprint)
    }
    return response.fingerprint
  }

  async liveSessionMetafileSvg(
    sessionHandle: number,
    revision: number,
    partName: string,
    options: ResolveSlideOptions = {},
  ): Promise<LiveSessionResource> {
    if (!/\.(?:emf|wmf)$/i.test(partName)) {
      throw new TypeError('partName must identify an EMF or WMF resource')
    }
    return this.#liveResource(sessionHandle, revision, partName, true, options)
  }

  async liveSessionCacheTelemetry(sessionHandle: number): Promise<LiveSessionCacheTelemetry> {
    this.#assertOpen()
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'telemetry')
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'live-session-cache-telemetry',
      sessionHandle,
    })
    const response = await result
    if (response.type !== 'live-session-cache-telemetry') {
      throw new Error('invalid live cache telemetry response')
    }
    return response
  }

  async releaseLiveSession(sessionHandle: number): Promise<void> {
    this.#assertOpen()
    this.#releasedLiveSessions.add(sessionHandle)
    const prefix = `${sessionHandle}\0`
    for (const key of this.#liveResourceFingerprints.keys()) {
      if (key.startsWith(prefix)) this.#liveResourceFingerprints.delete(key)
    }
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'release-session')
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'release-live-session',
      sessionHandle,
    })
    const response = await result
    if (response.type !== 'live-session-released') {
      this.#releasedLiveSessions.delete(sessionHandle)
      throw new Error('invalid release-live-session response')
    }
  }

  /** Transfer a PPTX into the Worker and return a handle for lazy slide resolution. */
  async openPresentation(
    presentation: ArrayBuffer,
    options: ResolveSlideOptions = {},
  ): Promise<OpenedBrowserPresentation> {
    this.#assertOpen()
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'open', options.signal, options.onProgress)
    this.#worker.postMessage(
      { version: WORKER_PROTOCOL_VERSION, id, type: 'open-presentation', presentation },
      [presentation],
    )
    const response = await result
    if (response.type !== 'presentation-opened') throw new Error('invalid open response')
    this.#releasedPresentations.delete(response.presentationHandle)
    return { handle: response.presentationHandle, slideCount: response.slideCount }
  }

  /** Resolve one zero-based slide to a caller-owned WPDL `ArrayBuffer`. */
  async resolveSlide(
    presentationHandle: number,
    slideIndex: number,
    options: ResolveSlideOptions = {},
  ): Promise<ArrayBuffer> {
    this.#assertOpen()
    if (!Number.isSafeInteger(slideIndex) || slideIndex < 0) {
      throw new RangeError('slideIndex must be a non-negative safe integer')
    }
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'resolve', options.signal, options.onProgress)
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'resolve-slide',
      presentationHandle,
      slideIndex,
    })
    const response = await result
    if (response.type !== 'slide-resolved') throw new Error('invalid resolve response')
    return response.displayList
  }

  async presentationResource(
    presentationHandle: number,
    partName: string,
    options: ResolveSlideOptions = {},
  ): Promise<ArrayBuffer> {
    this.#assertOpen()
    if (partName.length === 0) throw new TypeError('partName must not be empty')
    return this.#cachedResource(presentationHandle, `${presentationHandle}\0raw\0${partName}`, options.signal, async () => {
      const id = this.#allocateId()
      const result = this.#unaryRequest(id, 'resource', undefined, options.onProgress)
      this.#worker.postMessage({
        version: WORKER_PROTOCOL_VERSION,
        id,
        type: 'presentation-resource',
        presentationHandle,
        partName,
      })
      const response = await result
      if (response.type !== 'presentation-resource') throw new Error('invalid presentation-resource response')
      return response.bytes
    })
  }

  async presentationMetafileSvg(
    presentationHandle: number,
    partName: string,
    options: ResolveSlideOptions = {},
  ): Promise<ArrayBuffer> {
    this.#assertOpen()
    if (!/\.(?:emf|wmf)$/i.test(partName)) {
      throw new TypeError('partName must identify an EMF or WMF resource')
    }
    return this.#cachedResource(presentationHandle, `${presentationHandle}\0svg\0${partName}`, options.signal, async () => {
      const id = this.#allocateId()
      const result = this.#unaryRequest(id, 'metafile', undefined, options.onProgress)
      this.#worker.postMessage({
        version: WORKER_PROTOCOL_VERSION,
        id,
        type: 'presentation-metafile-svg',
        presentationHandle,
        partName,
      })
      const response = await result
      if (response.type !== 'presentation-metafile-svg') {
        throw new Error('invalid presentation-metafile-svg response')
      }
      return response.bytes
    })
  }

  /** Release a presentation and purge its cached resource bytes from this client. */
  async releasePresentation(presentationHandle: number): Promise<void> {
    this.#assertOpen()
    this.#releasedPresentations.add(presentationHandle)
    this.#purgePresentationResources(presentationHandle)
    const id = this.#allocateId()
    const result = this.#unaryRequest(id, 'release-presentation')
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'release-presentation',
      presentationHandle,
    })
    let response: WorkerResponse
    try {
      response = await result
    } catch (error) {
      this.#releasedPresentations.delete(presentationHandle)
      throw error
    }
    if (response.type !== 'presentation-released') {
      throw new Error('invalid release-presentation response')
    }
  }

  async #liveResource(
    sessionHandle: number,
    revision: number,
    partName: string,
    metafile: boolean,
    options: ResolveSlideOptions,
  ): Promise<LiveSessionResource> {
    this.#assertOpen()
    assertRevision(revision)
    if (partName.length === 0) throw new TypeError('partName must not be empty')
    const sourceFingerprint = await this.liveSessionResourceFingerprint(
      sessionHandle,
      revision,
      partName,
      options,
    )
    const fingerprint = metafile ? `${sourceFingerprint}:metafile-svg-v1` : sourceFingerprint
    const cacheKey = `content\0${metafile ? 'svg' : 'raw'}\0${fingerprint}`
    const cached = this.#resourceCache.get(cacheKey)
    if (cached !== undefined) {
      this.#resourceCache.delete(cacheKey)
      this.#resourceCache.set(cacheKey, cached)
      return { fingerprint, bytes: cached }
    }
    let loading = this.#resourceInflight.get(cacheKey)
    if (loading === undefined) {
      loading = (async () => {
        const id = this.#allocateId()
        const result = this.#unaryRequest(id, metafile ? 'metafile' : 'resource')
        this.#worker.postMessage({
          version: WORKER_PROTOCOL_VERSION,
          id,
          type: metafile ? 'live-session-metafile-svg' : 'live-session-resource',
          sessionHandle,
          revision,
          partName,
        })
        const response = await result
        if (response.type !== 'live-session-resource' &&
          response.type !== 'live-session-metafile-svg') {
          throw new Error('invalid live resource response')
        }
        if (response.fingerprint !== fingerprint) {
          throw new Error('live resource fingerprint changed during one revision')
        }
        if (!this.#releasedLiveSessions.has(sessionHandle)) {
          this.#storeResource(cacheKey, response.bytes)
        }
        return response.bytes
      })().finally(() => this.#resourceInflight.delete(cacheKey))
      this.#resourceInflight.set(cacheKey, loading)
    }
    return { fingerprint, bytes: await abortable(loading, options.signal) }
  }

  async #cachedResource(
    presentationHandle: number,
    key: string,
    signal: AbortSignal | undefined,
    load: () => Promise<ArrayBuffer>,
  ): Promise<ArrayBuffer> {
    if (signal?.aborted === true) throw abortError()
    const cached = this.#resourceCache.get(key)
    if (cached !== undefined) {
      this.#resourceCache.delete(key)
      this.#resourceCache.set(key, cached)
      return cached
    }
    let loading = this.#resourceInflight.get(key)
    if (loading === undefined) {
      loading = load().then((bytes) => {
        if (!this.#releasedPresentations.has(presentationHandle)) this.#storeResource(key, bytes)
        return bytes
      }).finally(() => this.#resourceInflight.delete(key))
      this.#resourceInflight.set(key, loading)
    }
    const bytes = await abortable(loading, signal)
    return bytes
  }

  #storeResource(key: string, bytes: ArrayBuffer): void {
    if (bytes.byteLength > this.#resourceCacheLimit) return
    const previous = this.#resourceCache.get(key)
    if (previous !== undefined) this.#resourceCacheBytes -= previous.byteLength
    this.#resourceCache.delete(key)
    this.#resourceCache.set(key, bytes)
    this.#resourceCacheBytes += bytes.byteLength
    while (this.#resourceCacheBytes > this.#resourceCacheLimit) {
      const oldest = this.#resourceCache.entries().next().value as [string, ArrayBuffer] | undefined
      if (oldest === undefined) break
      this.#resourceCache.delete(oldest[0])
      this.#resourceCacheBytes -= oldest[1].byteLength
    }
  }

  #purgePresentationResources(handle: number): void {
    const prefix = `${handle}\0`
    for (const [key, bytes] of this.#resourceCache) {
      if (!key.startsWith(prefix)) continue
      this.#resourceCache.delete(key)
      this.#resourceCacheBytes -= bytes.byteLength
    }
  }

  /**
   * Stream caller-owned PPTX chunks. Reading to completion or cancelling releases the cursor;
   * the prepared template handle remains live until `release`.
   */
  generateStream(
    templateHandle: number,
    data: GenerationData | TextBindings = {},
    options: GenerateOptions = {},
  ): ReadableStream<Uint8Array> {
    this.#assertOpen()
    const id = this.#allocateId()
    const chunkBytes = options.chunkBytes ?? 256 * 1024
    if (!Number.isSafeInteger(chunkBytes) || chunkBytes <= 0) {
      throw new RangeError('chunkBytes must be a positive safe integer')
    }
    if (options.signal?.aborted === true) {
      return new ReadableStream<Uint8Array>({
        start: (controller) => controller.error(abortError()),
      })
    }
    const payload = encodeInjectionData(normalizeGenerationData(data))
    return new ReadableStream<Uint8Array>({
      start: (controller) => {
        const abort = (): void => {
          this.#worker.postMessage({
            version: WORKER_PROTOCOL_VERSION,
            id: this.#allocateId(),
            type: 'cancel',
            targetId: id,
          })
        }
        options.signal?.addEventListener('abort', abort, { once: true })
        this.#pending.set(id, {
          kind: 'generate',
          controller,
          onProgress: options.onProgress,
          abortCleanup: () => options.signal?.removeEventListener('abort', abort),
        })
        this.#worker.postMessage({
          version: WORKER_PROTOCOL_VERSION,
          id,
          type: 'generate',
          templateHandle,
          payload,
          chunkBytes,
        }, [payload])
      },
      cancel: () => {
        this.#worker.postMessage({
          version: WORKER_PROTOCOL_VERSION,
          id: this.#allocateId(),
          type: 'cancel',
          targetId: id,
        })
      },
    })
  }

  generateLiveStream(
    sessionHandle: number,
    revision: number,
    options: GenerateOptions = {},
  ): ReadableStream<Uint8Array> {
    this.#assertOpen()
    assertRevision(revision)
    const id = this.#allocateId()
    const chunkBytes = options.chunkBytes ?? 256 * 1024
    if (!Number.isSafeInteger(chunkBytes) || chunkBytes <= 0) {
      throw new RangeError('chunkBytes must be a positive safe integer')
    }
    if (options.signal?.aborted === true) {
      return new ReadableStream<Uint8Array>({
        start: (controller) => controller.error(abortError()),
      })
    }
    return new ReadableStream<Uint8Array>({
      start: (controller) => {
        const abort = (): void => {
          this.#worker.postMessage({
            version: WORKER_PROTOCOL_VERSION,
            id: this.#allocateId(),
            type: 'cancel',
            targetId: id,
          })
        }
        options.signal?.addEventListener('abort', abort, { once: true })
        this.#pending.set(id, {
          kind: 'generate',
          controller,
          onProgress: options.onProgress,
          abortCleanup: () => options.signal?.removeEventListener('abort', abort),
        })
        this.#worker.postMessage({
          version: WORKER_PROTOCOL_VERSION,
          id,
          type: 'generate-live-session',
          sessionHandle,
          revision,
          chunkBytes,
        })
      },
      cancel: () => {
        this.#worker.postMessage({
          version: WORKER_PROTOCOL_VERSION,
          id: this.#allocateId(),
          type: 'cancel',
          targetId: id,
        })
      },
    })
  }

  async generateLive(
    sessionHandle: number,
    revision: number,
    options: GenerateOptions = {},
  ): Promise<ArrayBuffer> {
    const chunks: Uint8Array[] = []
    let length = 0
    for await (const chunk of this.generateLiveStream(sessionHandle, revision, options)) {
      chunks.push(chunk)
      length += chunk.byteLength
    }
    const output = new Uint8Array(length)
    let offset = 0
    for (const chunk of chunks) {
      output.set(chunk, offset)
      offset += chunk.byteLength
    }
    return output.buffer
  }

  /** Stream a PPTX from the exact immutable overlay used by one deck preview revision. */
  generateDeckStream(
    sessionHandle: number,
    revision: number,
    options: GenerateOptions = {},
  ): ReadableStream<Uint8Array> {
    this.#assertOpen()
    assertRevision(revision)
    if (this.#deckRevisions.get(sessionHandle) !== revision) {
      throw new Error('deck export targets a stale revision')
    }
    const id = this.#allocateId()
    const chunkBytes = options.chunkBytes ?? 256 * 1024
    if (!Number.isSafeInteger(chunkBytes) || chunkBytes <= 0) {
      throw new RangeError('chunkBytes must be a positive safe integer')
    }
    if (options.signal?.aborted === true) {
      return new ReadableStream<Uint8Array>({
        start: (controller) => controller.error(abortError()),
      })
    }
    return new ReadableStream<Uint8Array>({
      start: (controller) => {
        const abort = (): void => {
          this.#worker.postMessage({
            version: WORKER_PROTOCOL_VERSION,
            id: this.#allocateId(),
            type: 'cancel',
            targetId: id,
          })
        }
        options.signal?.addEventListener('abort', abort, { once: true })
        this.#pending.set(id, {
          kind: 'generate',
          controller,
          onProgress: options.onProgress,
          abortCleanup: () => options.signal?.removeEventListener('abort', abort),
        })
        this.#worker.postMessage({
          version: WORKER_PROTOCOL_VERSION,
          id,
          type: 'generate-deck-session',
          sessionHandle,
          revision,
          chunkBytes,
        })
      },
      cancel: () => {
        this.#worker.postMessage({
          version: WORKER_PROTOCOL_VERSION,
          id: this.#allocateId(),
          type: 'cancel',
          targetId: id,
        })
      },
    })
  }

  async generateDeck(
    sessionHandle: number,
    revision: number,
    options: GenerateOptions = {},
  ): Promise<ArrayBuffer> {
    const chunks: Uint8Array[] = []
    let length = 0
    for await (const chunk of this.generateDeckStream(sessionHandle, revision, options)) {
      chunks.push(chunk)
      length += chunk.byteLength
    }
    const output = new Uint8Array(length)
    let offset = 0
    for (const chunk of chunks) {
      output.set(chunk, offset)
      offset += chunk.byteLength
    }
    return output.buffer
  }

  /** Generate a complete caller-owned PPTX buffer by draining `generateStream`. */
  async generate(
    templateHandle: number,
    data: GenerationData | TextBindings = {},
    options: GenerateOptions = {},
  ): Promise<ArrayBuffer> {
    const chunks: Uint8Array[] = []
    let length = 0
    for await (const chunk of this.generateStream(templateHandle, data, options)) {
      chunks.push(chunk)
      length += chunk.byteLength
    }
    const output = new Uint8Array(length)
    let offset = 0
    for (const chunk of chunks) {
      output.set(chunk, offset)
      offset += chunk.byteLength
    }
    return output.buffer
  }

  /** Release one prepared template handle. The handle is invalid after this resolves. */
  async release(templateHandle: number): Promise<void> {
    this.#assertOpen()
    const id = this.#allocateId()
    const result = new Promise<WorkerResponse>((resolve, reject) => {
      this.#pending.set(id, { kind: 'release', resolve, reject })
    })
    this.#worker.postMessage({
      version: WORKER_PROTOCOL_VERSION,
      id,
      type: 'release',
      templateHandle,
    })
    const response = await result
    if (response.type !== 'released') throw new Error('invalid release response')
  }

  /** Hard-stop the Worker and reject every pending operation. Idempotent. */
  terminate(): void {
    if (this.#closed) return
    this.#closed = true
    this.#detach()
    this.#worker.terminate()
    this.#resourceCache.clear()
    this.#releasedPresentations.clear()
    this.#releasedLiveSessions.clear()
    this.#releasedDeckSessions.clear()
    this.#deckRevisions.clear()
    this.#liveResourceFingerprints.clear()
    this.#resourceCacheBytes = 0
    this.#failAll(new Error('wasmppt Worker was terminated'))
  }

  #receive(value: unknown): void {
    const response = normalizeWorkerResponse(value)
    if (response === undefined) return
    const pending = this.#pending.get(response.id)
    if (pending === undefined) return
    if (response.type === 'progress') {
      if (pending.kind === 'generate' &&
        (response.phase === 'generate' || response.phase === 'stream')) {
        pending.onProgress?.(response.phase, response.completed, response.total)
      } else if ((pending.kind === 'open' || pending.kind === 'resolve') &&
        (response.phase === 'open' || response.phase === 'resolve')) {
        pending.onProgress?.(response.phase, response.completed, response.total)
      }
      return
    }
    if (response.type === 'chunk') {
      if (pending.kind === 'generate') pending.controller.enqueue(new Uint8Array(response.bytes))
      return
    }
    this.#pending.delete(response.id)
    if (pending.kind === 'generate') {
      pending.abortCleanup()
      if (response.type === 'generated') pending.controller.close()
      else if (response.type === 'cancelled') pending.controller.error(abortError())
      else if (response.type === 'error') pending.controller.error(remoteError(response))
      else pending.controller.error(new Error('invalid generate response'))
    } else if (response.type === 'error') {
      pending.reject(remoteError(response))
    } else if (response.type === 'cancelled') {
      pending.reject(abortError())
    } else {
      pending.resolve(response)
    }
  }

  #failAll(error: Error): void {
    for (const pending of this.#pending.values()) {
      if (pending.kind === 'generate') {
        pending.abortCleanup()
        pending.controller.error(error)
      } else {
        pending.reject(error)
      }
    }
    this.#pending.clear()
  }

  #detach(): void {
    this.#worker.removeEventListener('message', this.#onMessage)
    this.#worker.removeEventListener('error', this.#onCrash)
    this.#worker.removeEventListener('messageerror', this.#onCrash)
  }

  #allocateId(): number {
    const id = this.#nextId
    this.#nextId = this.#nextId >= Number.MAX_SAFE_INTEGER ? 1 : this.#nextId + 1
    return id
  }

  #unaryRequest(
    id: number,
    kind: Exclude<Pending, { readonly kind: 'generate' }>['kind'],
    signal?: AbortSignal,
    onProgress?: ResolveSlideOptions['onProgress'],
  ): Promise<WorkerResponse> {
    if (signal?.aborted === true) return Promise.reject(abortError())
    return new Promise<WorkerResponse>((resolve, reject) => {
      const abort = (): void => {
        this.#worker.postMessage({
          version: WORKER_PROTOCOL_VERSION,
          id: this.#allocateId(),
          type: 'cancel',
          targetId: id,
        })
      }
      signal?.addEventListener('abort', abort, { once: true })
      this.#pending.set(id, {
        kind,
        resolve: (response) => {
          signal?.removeEventListener('abort', abort)
          resolve(response)
        },
        reject: (error) => {
          signal?.removeEventListener('abort', abort)
          reject(error)
        },
        onProgress,
      })
    })
  }

  #assertOpen(): void {
    if (this.#closed) throw new Error('wasmppt Worker client is closed')
  }
}

function isWorkerResponse(value: unknown): value is WorkerResponse {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as { readonly version?: unknown; readonly id?: unknown; readonly type?: unknown }
  return (
    candidate.version === WORKER_PROTOCOL_VERSION &&
    Number.isSafeInteger(candidate.id) &&
    (candidate.id as number) >= 0 &&
    (candidate.type === 'deck-template-prepared' ||
      candidate.type === 'deck-session-created' ||
      candidate.type === 'deck-session-updated' ||
      candidate.type === 'deck-slide-resolved' ||
      candidate.type === 'deck-session-resource' ||
      candidate.type === 'deck-session-resource-fingerprint' ||
      candidate.type === 'deck-session-cache-telemetry' ||
      candidate.type === 'deck-session-released' ||
      candidate.type === 'deck-template-released' ||
      candidate.type === 'progress' ||
      candidate.type === 'prepared' ||
      candidate.type === 'live-session-created' ||
      candidate.type === 'live-session-updated' ||
      candidate.type === 'chunk' ||
      candidate.type === 'generated' ||
      candidate.type === 'released' ||
      candidate.type === 'cancelled' ||
      candidate.type === 'presentation-opened' ||
      candidate.type === 'slide-resolved' ||
      candidate.type === 'live-slide-resolved' ||
      candidate.type === 'live-session-resource' ||
      candidate.type === 'live-session-resource-fingerprint' ||
      candidate.type === 'live-session-metafile-svg' ||
      candidate.type === 'live-session-cache-telemetry' ||
      candidate.type === 'live-session-released' ||
      candidate.type === 'presentation-resource' ||
      candidate.type === 'presentation-metafile-svg' ||
      candidate.type === 'presentation-released' ||
      candidate.type === 'error')
  )
}

function normalizeWorkerResponse(value: unknown): WorkerResponse | undefined {
  if (isWorkerResponse(value)) return value
  if (typeof value !== 'object' || value === null) return undefined
  const candidate = value as {
    readonly version?: unknown
    readonly id?: unknown
    readonly type?: unknown
    readonly name?: unknown
    readonly message?: unknown
    readonly error?: unknown
  }
  if (candidate.version !== LEGACY_WORKER_PROTOCOL_VERSION ||
    !Number.isSafeInteger(candidate.id) || (candidate.id as number) < 0) return undefined
  if (candidate.type === 'cancelled') {
    return {
      version: WORKER_PROTOCOL_VERSION,
      id: candidate.id as number,
      type: 'cancelled',
      error: cancellationEnvelope(),
    }
  }
  if (candidate.type !== 'error' || typeof candidate.name !== 'string' ||
    typeof candidate.message !== 'string') return undefined
  const error = isWasmpptErrorEnvelope(candidate.error)
    ? candidate.error
    : {
        version: ERROR_ENVELOPE_VERSION,
        domain: 'runtime' as const,
        code: 'legacy-error',
        message: candidate.message,
      }
  return {
    version: WORKER_PROTOCOL_VERSION,
    id: candidate.id as number,
    type: 'error',
    error,
    name: candidate.name,
    message: candidate.message,
  }
}

function remoteError(response: Extract<WorkerResponse, { readonly type: 'error' }>): WasmpptError {
  return new WasmpptError(response.error, response.name)
}

function abortError(): WasmpptError {
  return new WasmpptError(
    cancellationEnvelope('wasmppt generation was cancelled'),
    'AbortError',
  )
}

function abortable<T>(promise: Promise<T>, signal: AbortSignal | undefined): Promise<T> {
  if (signal === undefined) return promise
  if (signal.aborted) return Promise.reject(abortError())
  return new Promise<T>((resolve, reject) => {
    const abort = (): void => reject(abortError())
    signal.addEventListener('abort', abort, { once: true })
    promise.then(
      (value) => { signal.removeEventListener('abort', abort); resolve(value) },
      (error: unknown) => { signal.removeEventListener('abort', abort); reject(error) },
    )
  })
}

function normalizeGenerationData(data: GenerationData | TextBindings): GenerationData {
  const values = Object.values(data)
  if (values.every((value) => typeof value === 'string')) return { text: data as TextBindings }
  return data as GenerationData
}

function assertRevision(value: number): void {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
    throw new RangeError('revision must be an unsigned 32-bit integer')
  }
}
