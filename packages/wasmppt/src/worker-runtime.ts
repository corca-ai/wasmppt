import {
  WORKER_PROTOCOL_VERSION,
  type WorkerEngine,
  type WorkerRequest,
  type WorkerResponse,
} from './protocol.js'
import { cancellationEnvelope, normalizeWasmpptError } from './error.js'

type ResponseWithoutVersion = WorkerResponse extends infer Response
  ? Response extends WorkerResponse
    ? Omit<Response, 'version'>
    : never
  : never

export interface WorkerRuntimeScope {
  postMessage(message: WorkerResponse, transfer?: readonly Transferable[]): void
  addEventListener(type: 'message', listener: (event: MessageEvent<unknown>) => void): void
}

export interface WorkerRuntimeOptions {
  /** Optional lazy converter kept outside the primary presentation-engine Wasm module. */
  readonly metafileToSvg?: (input: Uint8Array) => Promise<Uint8Array>
}

/** Install the versioned protocol around one instance-local Wasm engine. */
export function installWorkerRuntime(
  scope: WorkerRuntimeScope,
  engine: WorkerEngine,
  options: WorkerRuntimeOptions = {},
): void {
  const cancelled = new Set<number>()
  const active = new Set<number>()
  scope.addEventListener('message', (event) => {
    const message = event.data
    if (!isWorkerRequest(message)) return
    if (message.type === 'cancel') {
      if (active.has(message.targetId)) cancelled.add(message.targetId)
      return
    }
    void handleRequest(message).catch((error: unknown) => {
      const normalized = normalizeError(error)
      post(scope, { id: message.id, type: 'error', ...normalized })
    })
  })

  async function handleRequest(message: Exclude<WorkerRequest, { readonly type: 'cancel' }>): Promise<void> {
    active.add(message.id)
    try {
      if (cancelled.delete(message.id)) {
        postCancelled(scope, message.id)
        return
      }
      switch (message.type) {
        case 'prepare-deck-template': {
          progress(scope, message.id, 'prepare', 0, 1)
          const template = new Uint8Array(message.template)
          const handle = message.plan === undefined
            ? engine.prepare_deck_template(template)
            : engine.prepare_deck_template_with_plan(template, new Uint8Array(message.plan))
          try {
            const plan = exactBuffer(engine.deck_template_plan(handle))
            progress(scope, message.id, 'prepare', 1, 1)
            scope.postMessage(response({
              id: message.id,
              type: 'deck-template-prepared',
              templateHandle: handle,
              cacheable: engine.deck_template_cacheable(handle),
              plan,
            }), [plan])
          } catch (error) {
            engine.release_deck_template(handle)
            throw error
          }
          return
        }
        case 'create-deck-session': {
          progress(scope, message.id, 'session', 0, 1)
          const spec = new Uint8Array(message.spec)
          const handle = message.plan === undefined
            ? engine.create_deck_session(message.templateHandle, spec)
            : engine.create_deck_session_with_plan(
                message.templateHandle,
                spec,
                new Uint8Array(message.plan),
              )
          try {
            const revision = engine.deck_session_revision(handle)
            const plan = exactBuffer(engine.deck_session_plan(handle, revision))
            const presentableSlides = decodeIndexArray(
              engine.deck_session_presentable_slides(handle),
              'presentable slide',
            )
            const slideCount = engine.deck_session_slide_count(handle)
            const pages = decodeDeckPageInventory(
              engine,
              handle,
              revision,
              slideCount,
              presentableSlides,
            )
            progress(scope, message.id, 'session', 1, 1)
            scope.postMessage(response({
              id: message.id,
              type: 'deck-session-created',
              sessionHandle: handle,
              revision,
              slideCount,
              presentableSlides,
              pages,
              plan,
            }), [plan])
          } catch (error) {
            engine.release_deck_session(handle)
            throw error
          }
          return
        }
        case 'update-deck-session': {
          progress(scope, message.id, 'delta', 0, 1)
          const update = decodeDeckUpdate(engine.apply_deck_session_spec(
            message.sessionHandle,
            message.expectedRevision,
            message.nextRevision,
            new Uint8Array(message.spec),
          ))
          const pages = decodeDeckPageInventory(
            engine,
            message.sessionHandle,
            update.revision,
            update.slideCount,
            update.presentableSlides,
          )
          progress(scope, message.id, 'delta', 1, 1)
          post(scope, {
            id: message.id,
            type: 'deck-session-updated',
            sessionHandle: message.sessionHandle,
            ...update,
            pages,
          })
          return
        }
        case 'generate-deck-session': {
          progress(scope, message.id, 'generate', 0, 1)
          const generation = engine.start_deck_session_generation(
            message.sessionHandle,
            message.revision,
          )
          await streamGeneration(message.id, generation, message.chunkBytes)
          return
        }
        case 'resolve-deck-slide': {
          progress(scope, message.id, 'resolve', 0, 1)
          const page = decodeDeckPageMetadata(engine.deck_session_slide_metadata(
            message.sessionHandle,
            message.revision,
            message.slideIndex,
          ))
          const fingerprint = engine.deck_session_slide_fingerprint(
            message.sessionHandle,
            message.revision,
            message.slideIndex,
          )
          const displayList = exactBuffer(engine.resolve_deck_session_slide(
            message.sessionHandle,
            message.revision,
            message.slideIndex,
          ))
          if (cancelled.delete(message.id)) {
            postCancelled(scope, message.id)
            return
          }
          progress(scope, message.id, 'resolve', 1, 1)
          scope.postMessage(response({
            id: message.id,
            type: 'deck-slide-resolved',
            sessionHandle: message.sessionHandle,
            revision: message.revision,
            slideIndex: message.slideIndex,
            fingerprint,
            page,
            displayList,
          }), [displayList])
          return
        }
        case 'deck-session-resource': {
          const fingerprint = engine.deck_session_resource_fingerprint(
            message.sessionHandle,
            message.revision,
            message.partName,
          )
          const bytes = exactBuffer(engine.deck_session_resource(
            message.sessionHandle,
            message.revision,
            message.partName,
          ))
          scope.postMessage(response({
            id: message.id,
            type: 'deck-session-resource',
            sessionHandle: message.sessionHandle,
            revision: message.revision,
            partName: message.partName,
            fingerprint,
            bytes,
          }), [bytes])
          return
        }
        case 'deck-session-resource-fingerprint': {
          const fingerprint = engine.deck_session_resource_fingerprint(
            message.sessionHandle,
            message.revision,
            message.partName,
          )
          post(scope, {
            id: message.id,
            type: 'deck-session-resource-fingerprint',
            sessionHandle: message.sessionHandle,
            revision: message.revision,
            partName: message.partName,
            fingerprint,
          })
          return
        }
        case 'deck-session-cache-telemetry': {
          const telemetry = decodeCacheTelemetry(engine.deck_session_cache_telemetry(message.sessionHandle))
          post(scope, { id: message.id, type: 'deck-session-cache-telemetry', ...telemetry })
          return
        }
        case 'release-deck-session':
          engine.release_deck_session(message.sessionHandle)
          post(scope, { id: message.id, type: 'deck-session-released' })
          return
        case 'release-deck-template':
          engine.release_deck_template(message.templateHandle)
          post(scope, { id: message.id, type: 'deck-template-released' })
          return
        case 'prepare': {
          progress(scope, message.id, 'prepare', 0, 1)
          const template = new Uint8Array(message.template)
          const handle = message.plan === undefined
            ? engine.prepare_with_options(
                template,
                macroPolicyTag(message.options.macroPolicy),
                message.options.allowVisibleTokens ?? true,
              )
            : engine.prepare_with_plan(template, new Uint8Array(message.plan))
          try {
            const plan = exactBuffer(engine.prepared_plan(handle))
            progress(scope, message.id, 'prepare', 1, 1)
            scope.postMessage(
              response({
                id: message.id,
                type: 'prepared',
                templateHandle: handle,
                residentBytes: safeResidentBytes(engine.prepared_weight(handle)),
                plan,
                bindings: decodeBindings(engine.prepared_bindings(handle)),
                diagnostics: decodeDiagnostics(engine.prepared_diagnostics(handle)),
              }),
              [plan],
            )
          } catch (error) {
            engine.release_template(handle)
            throw error
          }
          return
        }
        case 'generate': {
          progress(scope, message.id, 'generate', 0, 1)
          const generation = engine.start_generation_payload(
            message.templateHandle,
            new Uint8Array(message.payload),
          )
          await streamGeneration(message.id, generation, message.chunkBytes)
          return
        }
        case 'create-live-session': {
          progress(scope, message.id, 'session', 0, 1)
          const handle = engine.create_live_session_payload(
            message.templateHandle,
            new Uint8Array(message.payload),
          )
          try {
            const revision = engine.live_session_revision(handle)
            const slideCount = engine.live_session_slide_count(handle)
            progress(scope, message.id, 'session', 1, 1)
            post(scope, {
              id: message.id,
              type: 'live-session-created',
              sessionHandle: handle,
              revision,
              slideCount,
            })
          } catch (error) {
            engine.release_live_session(handle)
            throw error
          }
          return
        }
        case 'apply-live-delta': {
          progress(scope, message.id, 'delta', 0, 1)
          const raw = engine.apply_live_session_payload(
            message.sessionHandle,
            message.expectedRevision,
            message.nextRevision,
            new Uint8Array(message.payload),
          )
          const update = decodeLiveUpdate(raw)
          progress(scope, message.id, 'delta', 1, 1)
          post(scope, {
            id: message.id,
            type: 'live-session-updated',
            sessionHandle: message.sessionHandle,
            ...update,
          })
          return
        }
        case 'generate-live-session': {
          progress(scope, message.id, 'generate', 0, 1)
          const generation = engine.start_live_session_generation(
            message.sessionHandle,
            message.revision,
          )
          await streamGeneration(message.id, generation, message.chunkBytes)
          return
        }
        case 'resolve-live-slide': {
          progress(scope, message.id, 'resolve', 0, 1)
          const fingerprint = engine.live_session_slide_fingerprint(
            message.sessionHandle,
            message.revision,
            message.slideIndex,
          )
          const displayList = exactBuffer(engine.resolve_live_session_slide(
            message.sessionHandle,
            message.revision,
            message.slideIndex,
          ))
          if (cancelled.delete(message.id)) {
            postCancelled(scope, message.id)
            return
          }
          progress(scope, message.id, 'resolve', 1, 1)
          scope.postMessage(response({
            id: message.id,
            type: 'live-slide-resolved',
            sessionHandle: message.sessionHandle,
            revision: message.revision,
            slideIndex: message.slideIndex,
            fingerprint,
            displayList,
          }), [displayList])
          return
        }
        case 'live-session-resource': {
          const fingerprint = engine.live_session_resource_fingerprint(
            message.sessionHandle,
            message.revision,
            message.partName,
          )
          const bytes = exactBuffer(engine.live_session_resource(
            message.sessionHandle,
            message.revision,
            message.partName,
          ))
          scope.postMessage(response({
            id: message.id,
            type: 'live-session-resource',
            sessionHandle: message.sessionHandle,
            revision: message.revision,
            partName: message.partName,
            fingerprint,
            bytes,
          }), [bytes])
          return
        }
        case 'live-session-resource-fingerprint': {
          const fingerprint = engine.live_session_resource_fingerprint(
            message.sessionHandle,
            message.revision,
            message.partName,
          )
          post(scope, {
            id: message.id,
            type: 'live-session-resource-fingerprint',
            sessionHandle: message.sessionHandle,
            revision: message.revision,
            partName: message.partName,
            fingerprint,
          })
          return
        }
        case 'live-session-metafile-svg': {
          if (!/\.(?:emf|wmf)$/i.test(message.partName)) {
            throw new TypeError('live session resource is not an EMF or WMF part')
          }
          if (options.metafileToSvg === undefined) {
            throw new Error('this Worker does not provide the optional metafile converter')
          }
          const fingerprint = engine.live_session_resource_fingerprint(
            message.sessionHandle,
            message.revision,
            message.partName,
          )
          const source = engine.live_session_resource(
            message.sessionHandle,
            message.revision,
            message.partName,
          )
          const bytes = exactBuffer(await options.metafileToSvg(source))
          scope.postMessage(response({
            id: message.id,
            type: 'live-session-metafile-svg',
            sessionHandle: message.sessionHandle,
            revision: message.revision,
            partName: message.partName,
            fingerprint: `${fingerprint}:metafile-svg-v1`,
            bytes,
          }), [bytes])
          return
        }
        case 'live-session-cache-telemetry': {
          const telemetry = decodeCacheTelemetry(
            engine.live_session_cache_telemetry(message.sessionHandle),
          )
          post(scope, { id: message.id, type: 'live-session-cache-telemetry', ...telemetry })
          return
        }
        case 'release-live-session':
          engine.release_live_session(message.sessionHandle)
          post(scope, { id: message.id, type: 'live-session-released' })
          return
        case 'release':
          engine.release_template(message.templateHandle)
          post(scope, { id: message.id, type: 'released' })
          return
        case 'open-presentation': {
          progress(scope, message.id, 'open', 0, 1)
          const handle = engine.open_presentation(new Uint8Array(message.presentation))
          if (cancelled.delete(message.id)) {
            engine.release_presentation(handle)
            postCancelled(scope, message.id)
            return
          }
          try {
            const slideCount = engine.presentation_slide_count(handle)
            progress(scope, message.id, 'open', 1, 1)
            post(scope, {
              id: message.id,
              type: 'presentation-opened',
              presentationHandle: handle,
              slideCount,
            })
            return
          } catch (error) {
            engine.release_presentation(handle)
            throw error
          }
        }
        case 'resolve-slide': {
          progress(scope, message.id, 'resolve', 0, 1)
          const displayList = exactBuffer(
            engine.resolve_presentation_slide(message.presentationHandle, message.slideIndex),
          )
          if (cancelled.delete(message.id)) {
            postCancelled(scope, message.id)
            return
          }
          progress(scope, message.id, 'resolve', 1, 1)
          scope.postMessage(
            response({
              id: message.id,
              type: 'slide-resolved',
              slideIndex: message.slideIndex,
              displayList,
            }),
            [displayList],
          )
          return
        }
        case 'presentation-resource': {
          const bytes = exactBuffer(
            engine.presentation_resource(message.presentationHandle, message.partName),
          )
          if (cancelled.delete(message.id)) {
            postCancelled(scope, message.id)
            return
          }
          scope.postMessage(
            response({
              id: message.id,
              type: 'presentation-resource',
              partName: message.partName,
              bytes,
            }),
            [bytes],
          )
          return
        }
        case 'presentation-metafile-svg': {
          if (!/\.(?:emf|wmf)$/i.test(message.partName)) {
            throw new TypeError('presentation resource is not an EMF or WMF part')
          }
          if (options.metafileToSvg === undefined) {
            throw new Error('this Worker does not provide the optional metafile converter')
          }
          const source = engine.presentation_resource(
            message.presentationHandle,
            message.partName,
          )
          const bytes = exactBuffer(await options.metafileToSvg(source))
          if (cancelled.delete(message.id)) {
            postCancelled(scope, message.id)
            return
          }
          scope.postMessage(
            response({
              id: message.id,
              type: 'presentation-metafile-svg',
              partName: message.partName,
              bytes,
            }),
            [bytes],
          )
          return
        }
        case 'release-presentation':
          engine.release_presentation(message.presentationHandle)
          post(scope, { id: message.id, type: 'presentation-released' })
          return
      }
    } catch (error) {
      const normalized = normalizeError(error)
      post(scope, { id: message.id, type: 'error', ...normalized })
    } finally {
      active.delete(message.id)
      cancelled.delete(message.id)
    }
  }

  async function streamGeneration(id: number, generation: number, chunkBytes: number): Promise<void> {
    try {
      progress(scope, id, 'generate', 1, 1)
      let offset = 0
      while (!engine.generation_done(generation)) {
        await yieldToWorkerQueue()
        if (cancelled.delete(id)) {
          postCancelled(scope, id)
          return
        }
        const chunk = exactBuffer(engine.generation_pull(generation, chunkBytes))
        if (chunk.byteLength === 0 && !engine.generation_done(generation)) {
          throw new Error('Wasm generation cursor made no progress')
        }
        scope.postMessage(response({ id, type: 'chunk', offset, bytes: chunk }), [chunk])
        offset += chunk.byteLength
        progress(scope, id, 'stream', offset, 0)
      }
      post(scope, { id, type: 'generated', byteLength: offset })
    } finally {
      engine.release_generation(generation)
    }
  }
}

function isWorkerRequest(value: unknown): value is WorkerRequest {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as { readonly version?: unknown; readonly id?: unknown; readonly type?: unknown }
  return (
    candidate.version === WORKER_PROTOCOL_VERSION &&
    Number.isSafeInteger(candidate.id) &&
    (candidate.id as number) >= 0 &&
    (candidate.type === 'prepare-deck-template' ||
      candidate.type === 'create-deck-session' ||
      candidate.type === 'update-deck-session' ||
      candidate.type === 'generate-deck-session' ||
      candidate.type === 'resolve-deck-slide' ||
      candidate.type === 'deck-session-resource' ||
      candidate.type === 'deck-session-resource-fingerprint' ||
      candidate.type === 'deck-session-cache-telemetry' ||
      candidate.type === 'release-deck-session' ||
      candidate.type === 'release-deck-template' ||
      candidate.type === 'prepare' ||
      candidate.type === 'generate' ||
      candidate.type === 'create-live-session' ||
      candidate.type === 'apply-live-delta' ||
      candidate.type === 'generate-live-session' ||
      candidate.type === 'resolve-live-slide' ||
      candidate.type === 'live-session-resource' ||
      candidate.type === 'live-session-resource-fingerprint' ||
      candidate.type === 'live-session-metafile-svg' ||
      candidate.type === 'live-session-cache-telemetry' ||
      candidate.type === 'release-live-session' ||
      candidate.type === 'release' ||
      candidate.type === 'open-presentation' ||
      candidate.type === 'resolve-slide' ||
      candidate.type === 'presentation-resource' ||
      candidate.type === 'presentation-metafile-svg' ||
      candidate.type === 'release-presentation' ||
      candidate.type === 'cancel')
  )
}

function exactBuffer(bytes: Uint8Array): ArrayBuffer {
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer
}

function safeResidentBytes(value: bigint): number {
  const converted = Number(value)
  if (!Number.isSafeInteger(converted) || converted < 0) {
    throw new RangeError('prepared template resident byte weight is unsafe')
  }
  return converted
}

function response(message: ResponseWithoutVersion): WorkerResponse {
  return { version: WORKER_PROTOCOL_VERSION, ...message } as WorkerResponse
}

function post(scope: WorkerRuntimeScope, message: ResponseWithoutVersion): void {
  scope.postMessage(response(message))
}

function progress(
  scope: WorkerRuntimeScope,
  id: number,
  phase: 'prepare' | 'session' | 'delta' | 'generate' | 'stream' | 'open' | 'resolve',
  completed: number,
  total: number,
): void {
  post(scope, { id, type: 'progress', phase, completed, total })
}

function decodeLiveUpdate(rows: unknown[]): Omit<
  Extract<WorkerResponse, { readonly type: 'live-session-updated' }>,
  'version' | 'id' | 'type' | 'sessionHandle'
> {
  if (rows.length !== 14) throw new TypeError('invalid live update metadata')
  const [revision, graphChanged, fullFallback, invalidationReason, slideCount, invalidatedSlides,
    changedBindings,
    changedParts, reusedMaterializedParts,
    logicalParts, materializedParts, materializedBytes, reusedSourceBytes, removedParts] = rows
  const overlayValues = [reusedMaterializedParts, logicalParts, materializedParts,
    materializedBytes, reusedSourceBytes, removedParts]
  if (!isNonNegativeInteger(revision) || typeof graphChanged !== 'boolean' ||
    typeof fullFallback !== 'boolean' || !isNonNegativeInteger(slideCount) ||
    !['topology', 'dependency', 'none'].includes(invalidationReason as string) ||
    !Array.isArray(invalidatedSlides) || !invalidatedSlides.every(isNonNegativeInteger) ||
    !Array.isArray(changedBindings) || !changedBindings.every((value) => typeof value === 'string') ||
    !Array.isArray(changedParts) || !changedParts.every((value) => typeof value === 'string') ||
    !overlayValues.every(isNonNegativeInteger)) {
    throw new TypeError('invalid live update metadata')
  }
  return {
    revision,
    graphChanged,
    fullFallback,
    invalidationReason: invalidationReason as 'topology' | 'dependency' | 'none',
    slideCount,
    invalidatedSlides,
    changedBindings,
    changedParts,
    overlay: {
      reusedMaterializedParts: overlayValues[0]!,
      logicalParts: overlayValues[1]!,
      materializedParts: overlayValues[2]!,
      materializedBytes: overlayValues[3]!,
      reusedSourceBytes: overlayValues[4]!,
      removedParts: overlayValues[5]!,
    },
  }
}

function decodeDeckPageMetadata(rows: unknown[]): import('./protocol.js').DeckPageMetadata {
  if (rows.length !== 6) throw new TypeError('invalid deck page metadata')
  const [pageId, logicalSlideId, hidden, continuationOrdinal, continuationTotal, continuationLabel] = rows
  if (!isStableId(pageId) || !isStableId(logicalSlideId) || typeof hidden !== 'boolean' ||
    !isPositiveInteger(continuationOrdinal) || !isPositiveInteger(continuationTotal) ||
    continuationOrdinal > continuationTotal ||
    !(continuationLabel === null || typeof continuationLabel === 'string')) {
    throw new TypeError('invalid deck page metadata')
  }
  return {
    pageId,
    logicalSlideId,
    hidden,
    continuationOrdinal,
    continuationTotal,
    ...(continuationLabel === null ? {} : { continuationLabel }),
  }
}

function decodeDeckPageInventory(
  engine: WorkerEngine,
  sessionHandle: number,
  revision: number,
  slideCount: number,
  presentableSlides: readonly number[],
): import('./protocol.js').DeckPageMetadata[] {
  const pages = Array.from({ length: slideCount }, (_, slideIndex) =>
    decodeDeckPageMetadata(engine.deck_session_slide_metadata(
      sessionHandle,
      revision,
      slideIndex,
    )))
  const expectedPresentableSlides = pages.flatMap((page, slideIndex) =>
    page.hidden ? [] : [slideIndex])
  if (presentableSlides.length !== expectedPresentableSlides.length ||
    presentableSlides.some((slideIndex, index) => slideIndex !== expectedPresentableSlides[index])) {
    throw new TypeError('deck page inventory does not match presentable slides')
  }
  return pages
}

function decodeDeckUpdate(
  rows: unknown[],
): Omit<import('./protocol.js').DeckSessionUpdate, 'pages'> {
  if (rows.length !== 14) throw new TypeError('invalid deck update metadata')
  const [revision, slideCount, presentableSlides, invalidatedSlides,
    invalidatedLogicalSlideIds, removedPageIds, changedParts, reusedPages, fullFallback,
    logicalParts, materializedParts, materializedBytes, reusedSourceBytes, removedParts] = rows
  const ids = [invalidatedLogicalSlideIds, removedPageIds, changedParts]
  const overlayValues = [logicalParts, materializedParts, materializedBytes, reusedSourceBytes, removedParts]
  if (!isNonNegativeInteger(revision) || !isNonNegativeInteger(slideCount) ||
    !isNonNegativeInteger(reusedPages) || typeof fullFallback !== 'boolean' ||
    !Array.isArray(presentableSlides) || !presentableSlides.every(isNonNegativeInteger) ||
    !Array.isArray(invalidatedSlides) || !invalidatedSlides.every(isNonNegativeInteger) ||
    !ids.every((values) => Array.isArray(values) && values.every((value) => typeof value === 'string')) ||
    !overlayValues.every(isNonNegativeInteger)) {
    throw new TypeError('invalid deck update metadata')
  }
  return {
    revision,
    slideCount,
    presentableSlides,
    invalidatedSlides,
    invalidatedLogicalSlideIds: invalidatedLogicalSlideIds as string[],
    removedPageIds: removedPageIds as string[],
    changedParts: changedParts as string[],
    reusedPages,
    fullFallback,
    overlay: {
      logicalParts: logicalParts as number,
      materializedParts: materializedParts as number,
      materializedBytes: materializedBytes as number,
      reusedSourceBytes: reusedSourceBytes as number,
      removedParts: removedParts as number,
    },
  }
}

function decodeIndexArray(rows: unknown[], label: string): number[] {
  if (!rows.every(isNonNegativeInteger)) throw new TypeError(`invalid ${label} metadata`)
  return rows as number[]
}

function decodeCacheTelemetry(rows: unknown[]): {
  readonly residentBytes: number
  readonly peakBytes: number
  readonly entries: number
  readonly hits: number
  readonly misses: number
  readonly evictions: number
} {
  if (rows.length !== 6 || !rows.every(isNonNegativeInteger)) {
    throw new TypeError('invalid live session cache telemetry')
  }
  const [residentBytes, peakBytes, entries, hits, misses, evictions] = rows as number[]
  return { residentBytes: residentBytes!, peakBytes: peakBytes!, entries: entries!, hits: hits!, misses: misses!, evictions: evictions! }
}

function isNonNegativeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0
}

function isPositiveInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) > 0
}

function isStableId(value: unknown): value is string {
  return typeof value === 'string' && /^[0-9a-f]{32}$/u.test(value)
}

function normalizeError(error: unknown): {
  readonly error: import('./error.js').WasmpptErrorEnvelope
  readonly name: string
  readonly message: string
} {
  const normalized = normalizeWasmpptError(error)
  return {
    error: normalized.envelope,
    name: normalized.name,
    message: normalized.envelope.message,
  }
}

function postCancelled(scope: WorkerRuntimeScope, id: number): void {
  post(scope, { id, type: 'cancelled', error: cancellationEnvelope() })
}

function yieldToWorkerQueue(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0))
}

function macroPolicyTag(value: 'strip' | 'reject' | undefined): number {
  if (value === undefined || value === 'strip') return 0
  return 1
}

function decodeBindings(rows: unknown[]): import('./protocol.js').TemplateBinding[] {
  return rows.map((value) => {
    if (!Array.isArray(value) || value.length !== 6) throw new TypeError('invalid Wasm binding metadata')
    const [id, kind, partName, source, shapeId, shapeName] = value
    if (typeof id !== 'string' || (kind !== 'text' && kind !== 'image' && kind !== 'chart') ||
      typeof partName !== 'string' ||
      (source !== 'visible-token' && source !== 'shape-metadata' && source !== 'manifest')) {
      throw new TypeError('invalid Wasm binding metadata')
    }
    return {
      id,
      kind,
      partName,
      source,
      ...(typeof shapeId === 'number' ? { shapeId } : {}),
      ...(typeof shapeName === 'string' ? { shapeName } : {}),
    }
  })
}

function decodeDiagnostics(rows: unknown[]): import('./protocol.js').TemplateDiagnostic[] {
  return rows.map((value) => {
    if (!Array.isArray(value) || value.length !== 4) throw new TypeError('invalid Wasm diagnostic metadata')
    const [code, bindingId, partName, message] = value
    if (typeof code !== 'string' || typeof message !== 'string') {
      throw new TypeError('invalid Wasm diagnostic metadata')
    }
    return {
      code: code as import('./protocol.js').TemplateDiagnostic['code'],
      ...(typeof bindingId === 'string' ? { bindingId } : {}),
      ...(typeof partName === 'string' ? { partName } : {}),
      message,
    }
  })
}
