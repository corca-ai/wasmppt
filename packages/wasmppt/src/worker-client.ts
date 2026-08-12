import {
  WORKER_PROTOCOL_VERSION,
  type TextBindings,
  type WorkerRequest,
  type WorkerResponse,
} from './protocol.js'

export interface WorkerLike {
  postMessage(message: WorkerRequest, transfer?: readonly Transferable[]): void
  addEventListener(type: 'message', listener: (event: MessageEvent<unknown>) => void): void
  addEventListener(type: 'error' | 'messageerror', listener: (event: Event) => void): void
  removeEventListener(type: 'message', listener: (event: MessageEvent<unknown>) => void): void
  removeEventListener(type: 'error' | 'messageerror', listener: (event: Event) => void): void
  terminate(): void
}

export interface PreparedBrowserTemplate {
  readonly handle: number
  readonly residentBytes: number
}

export interface GenerateOptions {
  readonly signal?: AbortSignal
  readonly chunkBytes?: number
  readonly onProgress?: (phase: 'generate' | 'stream', completed: number, total: number) => void
}

type Pending =
  | {
      readonly kind: 'prepare' | 'release'
      readonly resolve: (value: WorkerResponse) => void
      readonly reject: (error: Error) => void
    }
  | {
      readonly kind: 'generate'
      readonly controller: ReadableStreamDefaultController<Uint8Array>
      readonly onProgress: GenerateOptions['onProgress']
      readonly abortCleanup: () => void
    }

/** Main-thread client that settles every request on completion, abort, or Worker crash. */
export class WasmpptWorkerClient {
  readonly #worker: WorkerLike
  readonly #pending = new Map<number, Pending>()
  #nextId = 1
  #closed = false

  readonly #onMessage = (event: MessageEvent<unknown>): void => this.#receive(event.data)
  readonly #onCrash = (): void => this.#failAll(new Error('wasmppt Worker terminated unexpectedly'))

  constructor(worker: WorkerLike) {
    this.#worker = worker
    worker.addEventListener('message', this.#onMessage)
    worker.addEventListener('error', this.#onCrash)
    worker.addEventListener('messageerror', this.#onCrash)
  }

  async prepare(template: ArrayBuffer): Promise<PreparedBrowserTemplate> {
    this.#assertOpen()
    const id = this.#allocateId()
    const result = new Promise<WorkerResponse>((resolve, reject) => {
      this.#pending.set(id, { kind: 'prepare', resolve, reject })
    })
    this.#worker.postMessage(
      { version: WORKER_PROTOCOL_VERSION, id, type: 'prepare', template },
      [template],
    )
    const response = await result
    if (response.type !== 'prepared') throw new Error('invalid prepare response')
    return { handle: response.templateHandle, residentBytes: response.residentBytes }
  }

  generateStream(
    templateHandle: number,
    text: TextBindings = {},
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
          text,
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

  async generate(
    templateHandle: number,
    text: TextBindings = {},
    options: GenerateOptions = {},
  ): Promise<ArrayBuffer> {
    const chunks: Uint8Array[] = []
    let length = 0
    for await (const chunk of this.generateStream(templateHandle, text, options)) {
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

  terminate(): void {
    if (this.#closed) return
    this.#closed = true
    this.#detach()
    this.#worker.terminate()
    this.#failAll(new Error('wasmppt Worker was terminated'))
  }

  #receive(value: unknown): void {
    if (!isWorkerResponse(value)) return
    const pending = this.#pending.get(value.id)
    if (pending === undefined) return
    if (value.type === 'progress') {
      if (pending.kind === 'generate' && value.phase !== 'prepare') {
        pending.onProgress?.(value.phase, value.completed, value.total)
      }
      return
    }
    if (value.type === 'chunk') {
      if (pending.kind === 'generate') pending.controller.enqueue(new Uint8Array(value.bytes))
      return
    }
    this.#pending.delete(value.id)
    if (pending.kind === 'generate') {
      pending.abortCleanup()
      if (value.type === 'generated') pending.controller.close()
      else if (value.type === 'cancelled') pending.controller.error(abortError())
      else if (value.type === 'error') pending.controller.error(remoteError(value))
      else pending.controller.error(new Error('invalid generate response'))
    } else if (value.type === 'error') {
      pending.reject(remoteError(value))
    } else if (value.type === 'cancelled') {
      pending.reject(abortError())
    } else {
      pending.resolve(value)
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

  #assertOpen(): void {
    if (this.#closed) throw new Error('wasmppt Worker client is closed')
  }
}

function isWorkerResponse(value: unknown): value is WorkerResponse {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as { readonly version?: unknown; readonly id?: unknown; readonly type?: unknown }
  return (
    candidate.version === WORKER_PROTOCOL_VERSION &&
    typeof candidate.id === 'number' &&
    typeof candidate.type === 'string'
  )
}

function remoteError(response: Extract<WorkerResponse, { readonly type: 'error' }>): Error {
  const error = new Error(response.message)
  error.name = response.name
  return error
}

function abortError(): DOMException {
  return new DOMException('wasmppt generation was cancelled', 'AbortError')
}
