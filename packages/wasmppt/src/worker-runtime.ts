import {
  WORKER_PROTOCOL_VERSION,
  type WorkerEngine,
  type WorkerRequest,
  type WorkerResponse,
} from './protocol.js'

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
  scope.addEventListener('message', (event) => {
    const message = event.data
    if (!isWorkerRequest(message)) return
    if (message.type === 'cancel') {
      cancelled.add(message.targetId)
      return
    }
    void handleRequest(message).catch((error: unknown) => {
      const normalized = normalizeError(error)
      post(scope, { id: message.id, type: 'error', ...normalized })
    })
  })

  async function handleRequest(message: Exclude<WorkerRequest, { readonly type: 'cancel' }>): Promise<void> {
    try {
      if (cancelled.delete(message.id)) {
        post(scope, { id: message.id, type: 'cancelled' })
        return
      }
      switch (message.type) {
        case 'prepare': {
          progress(scope, message.id, 'prepare', 0, 1)
          const template = new Uint8Array(message.template)
          const handle = message.plan === undefined
            ? engine.prepare_with_options(
                template,
                macroPolicyTag(message.options.macroPolicy),
                compatibilityTag(message.options.compatibility),
                compressionTag(message.options.compression),
                message.options.allowVisibleTokens ?? true,
              )
            : engine.prepare_with_plan(template, new Uint8Array(message.plan))
          const plan = exactBuffer(engine.prepared_plan(handle))
          progress(scope, message.id, 'prepare', 1, 1)
          scope.postMessage(
            response({
              id: message.id,
              type: 'prepared',
              templateHandle: handle,
              residentBytes: Number(engine.prepared_weight(handle)),
              plan,
              bindings: decodeBindings(engine.prepared_bindings(handle)),
              diagnostics: decodeDiagnostics(engine.prepared_diagnostics(handle)),
            }),
            [plan],
          )
          return
        }
        case 'generate': {
          progress(scope, message.id, 'generate', 0, 1)
          const generation = engine.start_generation_payload(
            message.templateHandle,
            new Uint8Array(message.payload),
          )
          try {
            progress(scope, message.id, 'generate', 1, 1)
            let offset = 0
            while (!engine.generation_done(generation)) {
              await yieldToWorkerQueue()
              if (cancelled.delete(message.id)) {
                post(scope, { id: message.id, type: 'cancelled' })
                return
              }
              const chunk = exactBuffer(engine.generation_pull(generation, message.chunkBytes))
              if (chunk.byteLength === 0 && !engine.generation_done(generation)) {
                throw new Error('Wasm generation cursor made no progress')
              }
              scope.postMessage(
                response({ id: message.id, type: 'chunk', offset, bytes: chunk }),
                [chunk],
              )
              offset += chunk.byteLength
              progress(scope, message.id, 'stream', offset, 0)
            }
            post(scope, { id: message.id, type: 'generated', byteLength: offset })
          } finally {
            engine.release_generation(generation)
          }
          return
        }
        case 'release':
          engine.release_template(message.templateHandle)
          post(scope, { id: message.id, type: 'released' })
          return
        case 'open-presentation': {
          progress(scope, message.id, 'open', 0, 1)
          const handle = engine.open_presentation(new Uint8Array(message.presentation))
          if (cancelled.delete(message.id)) {
            engine.release_presentation(handle)
            post(scope, { id: message.id, type: 'cancelled' })
            return
          }
          progress(scope, message.id, 'open', 1, 1)
          post(scope, {
            id: message.id,
            type: 'presentation-opened',
            presentationHandle: handle,
            slideCount: engine.presentation_slide_count(handle),
          })
          return
        }
        case 'resolve-slide': {
          progress(scope, message.id, 'resolve', 0, 1)
          const displayList = exactBuffer(
            engine.resolve_presentation_slide(message.presentationHandle, message.slideIndex),
          )
          if (cancelled.delete(message.id)) {
            post(scope, { id: message.id, type: 'cancelled' })
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
            post(scope, { id: message.id, type: 'cancelled' })
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
            post(scope, { id: message.id, type: 'cancelled' })
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
    }
  }
}

function isWorkerRequest(value: unknown): value is WorkerRequest {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as { readonly version?: unknown; readonly id?: unknown; readonly type?: unknown }
  return (
    candidate.version === WORKER_PROTOCOL_VERSION &&
    typeof candidate.id === 'number' &&
    (candidate.type === 'prepare' ||
      candidate.type === 'generate' ||
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

function response(message: ResponseWithoutVersion): WorkerResponse {
  return { version: WORKER_PROTOCOL_VERSION, ...message } as WorkerResponse
}

function post(scope: WorkerRuntimeScope, message: ResponseWithoutVersion): void {
  scope.postMessage(response(message))
}

function progress(
  scope: WorkerRuntimeScope,
  id: number,
  phase: 'prepare' | 'generate' | 'stream' | 'open' | 'resolve',
  completed: number,
  total: number,
): void {
  post(scope, { id, type: 'progress', phase, completed, total })
}

function normalizeError(error: unknown): { readonly name: string; readonly message: string } {
  if (error instanceof Error) return { name: error.name, message: error.message }
  return { name: 'Error', message: String(error) }
}

function yieldToWorkerQueue(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0))
}

function macroPolicyTag(value: 'strip' | 'reject' | 'preserve-as-pptm' | undefined): number {
  if (value === undefined || value === 'strip') return 0
  if (value === 'reject') return 1
  return 2
}

function compatibilityTag(value: 'powerpoint-2016' | 'microsoft-365' | undefined): number {
  return value === 'powerpoint-2016' ? 0 : 1
}

function compressionTag(value: 'balanced-deflate-6' | 'store-media' | undefined): number {
  return value === 'store-media' ? 1 : 0
}

function decodeBindings(rows: unknown[]): import('./protocol.js').TemplateBinding[] {
  return rows.map((value) => {
    if (!Array.isArray(value) || value.length !== 6) throw new TypeError('invalid Wasm binding metadata')
    const [id, kind, partName, source, shapeId, shapeName] = value
    if (typeof id !== 'string' || (kind !== 'text' && kind !== 'image') ||
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
