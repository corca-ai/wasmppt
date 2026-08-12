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

/** Install the versioned protocol around one instance-local Wasm engine. */
export function installWorkerRuntime(scope: WorkerRuntimeScope, engine: WorkerEngine): void {
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
          const handle = engine.prepare(new Uint8Array(message.template))
          progress(scope, message.id, 'prepare', 1, 1)
          post(scope, {
            id: message.id,
            type: 'prepared',
            templateHandle: handle,
            residentBytes: Number(engine.prepared_weight(handle)),
          })
          return
        }
        case 'generate': {
          progress(scope, message.id, 'generate', 0, 1)
          const entries = Object.entries(message.text)
          const output = engine.generate_text(
            message.templateHandle,
            entries.map(([id]) => id),
            entries.map(([, value]) => value),
          )
          try {
            const total = engine.output_len(output)
            progress(scope, message.id, 'generate', 1, 1)
            for (let offset = 0; offset < total; offset += message.chunkBytes) {
              await yieldToWorkerQueue()
              if (cancelled.delete(message.id)) {
                post(scope, { id: message.id, type: 'cancelled' })
                return
              }
              const length = Math.min(message.chunkBytes, total - offset)
              const chunk = exactBuffer(engine.output_chunk(output, offset, length))
              scope.postMessage(
                response({ id: message.id, type: 'chunk', offset, bytes: chunk }),
                [chunk],
              )
              progress(scope, message.id, 'stream', offset + length, total)
            }
            post(scope, { id: message.id, type: 'generated', byteLength: total })
          } finally {
            engine.release_output(output)
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
