/** Cloudflare Workers host adapter for the wasmppt Wasm core. */
export const packageName = '@corca-ai/wasmppt-worker' as const

export interface WorkerMemoryBudget {
  readonly maxInputBytes: number
  readonly maxPayloadBytes: number
  readonly maxOutputBytes: number
  readonly maxOutputChunkBytes: number
  readonly maxCachedPlanBytes: number
  readonly r2RangeBytes: number
}

export const DEFAULT_WORKER_MEMORY_BUDGET: WorkerMemoryBudget = Object.freeze({
  maxInputBytes: 16 * 1024 * 1024,
  maxPayloadBytes: 16 * 1024 * 1024,
  maxOutputBytes: 32 * 1024 * 1024,
  maxOutputChunkBytes: 256 * 1024,
  maxCachedPlanBytes: 32 * 1024 * 1024,
  r2RangeBytes: 1024 * 1024,
})

export interface WorkerEngine {
  prepare(template: Uint8Array): number
  prepared_weight(handle: number): bigint
  start_generation_payload(handle: number, payload: Uint8Array): number
  generation_pull(handle: number, maximumBytes: number): Uint8Array
  generation_done(handle: number): boolean
  release_template(handle: number): boolean
  release_generation(handle: number): boolean
}

interface CachedPlan {
  readonly handle: number
  readonly weight: number
}

/** Byte-budgeted LRU of immutable prepared templates. */
export class PreparedPlanCache {
  readonly #entries = new Map<string, CachedPlan>()
  readonly #maxBytes: number
  readonly #release: (handle: number) => void
  #residentBytes = 0

  constructor(maxBytes: number, release: (handle: number) => void) {
    assertPositiveSafeInteger(maxBytes, 'maxCachedPlanBytes')
    this.#maxBytes = maxBytes
    this.#release = release
  }

  get residentBytes(): number {
    return this.#residentBytes
  }

  get(key: string): CachedPlan | undefined {
    const entry = this.#entries.get(key)
    if (entry === undefined) return undefined
    this.#entries.delete(key)
    this.#entries.set(key, entry)
    return entry
  }

  insert(key: string, entry: CachedPlan): boolean {
    if (!Number.isSafeInteger(entry.weight) || entry.weight < 0) {
      throw new RangeError('cache weight must be a non-negative safe integer')
    }
    const previous = this.#entries.get(key)
    if (previous?.handle === entry.handle) {
      if (entry.weight > this.#maxBytes) return true
      this.#entries.delete(key)
      this.#residentBytes -= previous.weight
      this.#entries.set(key, Object.freeze(entry))
      this.#residentBytes += entry.weight
      return true
    }
    if (entry.weight > this.#maxBytes) return false
    if (previous !== undefined) {
      this.#entries.delete(key)
      this.#residentBytes -= previous.weight
      this.#release(previous.handle)
    }
    this.#entries.set(key, Object.freeze(entry))
    this.#residentBytes += entry.weight
    while (this.#residentBytes > this.#maxBytes) {
      const oldest = this.#entries.entries().next().value as [string, CachedPlan] | undefined
      if (oldest === undefined) break
      this.#entries.delete(oldest[0])
      this.#residentBytes -= oldest[1].weight
      this.#release(oldest[1].handle)
    }
    return this.#entries.get(key) === entry
  }

  clear(): void {
    for (const entry of this.#entries.values()) this.#release(entry.handle)
    this.#entries.clear()
    this.#residentBytes = 0
  }
}

/** Async ranged source over an in-process R2 binding. */
export class R2TemplateSource {
  readonly #bucket: R2Bucket
  readonly #key: string
  readonly size: number
  readonly etag: string

  private constructor(bucket: R2Bucket, key: string, size: number, etag: string) {
    this.#bucket = bucket
    this.#key = key
    this.size = size
    this.etag = etag
  }

  static async open(bucket: R2Bucket, key: string): Promise<R2TemplateSource> {
    const metadata = await bucket.head(key)
    if (metadata === null) throw new HttpError(404, `R2 template not found: ${key}`)
    return new R2TemplateSource(bucket, key, metadata.size, metadata.etag)
  }

  async readAt(offset: number, length: number): Promise<Uint8Array> {
    if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(length) || offset < 0 || length < 0) {
      throw new RangeError('R2 byte range must contain non-negative safe integers')
    }
    if (offset + length > this.size) throw new RangeError('R2 byte range exceeds object size')
    if (length === 0) return new Uint8Array()
    const object = await this.#bucket.get(this.#key, { range: { offset, length } })
    if (object === null) throw new HttpError(404, `R2 template disappeared: ${this.#key}`)
    const bytes = await object.bytes()
    if (bytes.byteLength !== length) {
      throw new Error(`R2 ranged read returned ${bytes.byteLength} bytes; expected ${length}`)
    }
    return bytes
  }

  async readAll(maxBytes: number, rangeBytes: number): Promise<Uint8Array> {
    if (this.size > maxBytes) throw new HttpError(413, 'R2 template exceeds maxInputBytes')
    assertPositiveSafeInteger(rangeBytes, 'r2RangeBytes')
    const output = new Uint8Array(this.size)
    for (let offset = 0; offset < this.size; offset += rangeBytes) {
      const length = Math.min(rangeBytes, this.size - offset)
      output.set(await this.readAt(offset, length), offset)
    }
    return output
  }
}

export interface WasmpptWorkerOptions {
  readonly budget?: Partial<WorkerMemoryBudget>
}

/**
 * Create an ES-module Worker around one isolate-local engine.
 *
 * Only immutable prepared templates survive requests. Input bytes, bindings,
 * output handles, offsets, and stream controllers remain request-local.
 */
export function createWasmpptWorker(
  engine: WorkerEngine,
  options: WasmpptWorkerOptions = {},
): ExportedHandler<Env> {
  const budget = validatedBudget(options.budget)
  const cache = new PreparedPlanCache(budget.maxCachedPlanBytes, (handle) => {
    engine.release_template(handle)
  })

  return {
    async fetch(request, env): Promise<Response> {
      const url = new URL(request.url)
      if (url.pathname === '/healthz') {
        return Response.json({ ok: true, cachedPlanBytes: cache.residentBytes })
      }
      if (url.pathname !== '/v1/generate' || request.method !== 'POST') {
        return Response.json({ error: 'not found' }, { status: 404 })
      }
      try {
        const source = await readTemplate(request, env.TEMPLATES, url, budget)
        const cacheKey = source.cacheKey ?? (await sha256Hex(source.bytes))
        let cached = cache.get(cacheKey)
        let releaseTemplate = false
        if (cached === undefined) {
          const handle = engine.prepare(source.bytes)
          try {
            const weight = bigintToSafeNumber(engine.prepared_weight(handle), 'prepared plan weight')
            const entry = Object.freeze({ handle, weight })
            if (!cache.insert(cacheKey, entry)) releaseTemplate = true
            cached = entry
          } catch (error) {
            engine.release_template(handle)
            throw error
          }
        }

        const payload = source.cacheKey !== undefined && isInjectionPayload(request)
          ? await readBoundedBody(request, budget.maxPayloadBytes, 'injection payload')
          : encodeTextPayload(parseTextBindings(request.headers.get('x-wasmppt-bindings')))
        let generationHandle: number
        try {
          generationHandle = engine.start_generation_payload(cached.handle, payload)
        } finally {
          if (releaseTemplate) engine.release_template(cached.handle)
        }

        return new Response(outputStream(engine, generationHandle, budget), {
          headers: {
            'content-type':
              'application/vnd.openxmlformats-officedocument.presentationml.presentation',
            'x-wasmppt-output-mode': 'pull-stream',
            'x-wasmppt-accounted-memory-bytes': String(
              accountedMemoryBytes(budget),
            ),
          },
        })
      } catch (error) {
        const status = error instanceof HttpError ? error.status : 500
        const message = error instanceof Error ? error.message : String(error)
        console.error(JSON.stringify({ message: 'wasmppt generation failed', error: message }))
        return Response.json({ error: message }, { status })
      }
    },
  } satisfies ExportedHandler<Env>
}

async function readTemplate(
  request: Request,
  bucket: R2Bucket,
  url: URL,
  budget: WorkerMemoryBudget,
): Promise<{ readonly bytes: Uint8Array; readonly cacheKey?: string }> {
  const r2Key = url.searchParams.get('r2')
  if (r2Key !== null) {
    const source = await R2TemplateSource.open(bucket, r2Key)
    return {
      bytes: await source.readAll(budget.maxInputBytes, budget.r2RangeBytes),
      cacheKey: `r2:${r2Key}:${source.etag}`,
    }
  }
  return { bytes: await readBoundedBody(request, budget.maxInputBytes, 'request body') }
}

async function readBoundedBody(
  request: Request,
  maxBytes: number,
  label: string,
): Promise<Uint8Array> {
  const contentLength = request.headers.get('content-length')
  if (contentLength !== null && Number(contentLength) > maxBytes) {
    throw new HttpError(413, `${label} exceeds its configured byte limit`)
  }
  if (request.body === null) throw new HttpError(400, `${label} is required`)
  const reader = request.body.getReader()
  const chunks: Uint8Array[] = []
  let length = 0
  try {
    for (;;) {
      const { done, value } = await reader.read()
      if (done) break
      length += value.byteLength
      if (length > maxBytes) throw new HttpError(413, `${label} exceeds its configured byte limit`)
      chunks.push(value)
    }
  } finally {
    reader.releaseLock()
  }
  const output = new Uint8Array(length)
  let offset = 0
  for (const chunk of chunks) {
    output.set(chunk, offset)
    offset += chunk.byteLength
  }
  return output
}

function isInjectionPayload(request: Request): boolean {
  const contentType = request.headers.get('content-type')?.split(';', 1)[0]?.trim().toLowerCase()
  return contentType === 'application/vnd.corca.wasmppt.injection-v2' ||
    contentType === 'application/vnd.corca.wasmppt.injection-v1'
}

function outputStream(
  engine: WorkerEngine,
  handle: number,
  budget: WorkerMemoryBudget,
): ReadableStream<Uint8Array> {
  let outputBytes = 0
  let released = false
  const release = (): void => {
    if (released) return
    released = true
    engine.release_generation(handle)
  }
  return new ReadableStream<Uint8Array>({
    pull(controller) {
      try {
        if (engine.generation_done(handle)) {
          release()
          controller.close()
          return
        }
        const chunk = engine.generation_pull(handle, budget.maxOutputChunkBytes)
        if (chunk.byteLength === 0 && !engine.generation_done(handle)) {
          throw new Error('Wasm generation cursor made no progress')
        }
        outputBytes += chunk.byteLength
        if (outputBytes > budget.maxOutputBytes) {
          throw new HttpError(413, 'generated presentation exceeds maxOutputBytes')
        }
        controller.enqueue(chunk)
      } catch (error) {
        release()
        controller.error(error)
      }
    },
    cancel() {
      release()
    },
  })
}

function parseTextBindings(header: string | null): Readonly<Record<string, string>> {
  if (header === null) return Object.freeze({})
  if (header.length > 64 * 1024) throw new HttpError(431, 'bindings header is too large')
  let value: unknown
  try {
    value = JSON.parse(header)
  } catch {
    throw new HttpError(400, 'bindings header is not valid JSON')
  }
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new HttpError(400, 'bindings must be a JSON object')
  }
  const output: Record<string, string> = Object.create(null)
  for (const [id, binding] of Object.entries(value)) {
    if (typeof binding !== 'string') throw new HttpError(400, `binding ${id} is not a string`)
    output[id] = binding
  }
  return Object.freeze(output)
}

function encodeTextPayload(bindings: Readonly<Record<string, string>>): Uint8Array {
  const encoder = new TextEncoder()
  const entries = Object.entries(bindings).sort(([left], [right]) => left.localeCompare(right))
  const encoded = entries.map(([id, value]) => [encoder.encode(id), encoder.encode(value)] as const)
  const length = 8 + 4 + encoded.reduce((sum, [id, value]) => sum + 8 + id.length + value.length, 0) + 16
  const output = new Uint8Array(length)
  const view = new DataView(output.buffer)
  output.set([0x57, 0x50, 0x50, 0x44], 0)
  view.setUint32(4, 1, true)
  view.setUint32(8, entries.length, true)
  let offset = 12
  for (const [id, value] of encoded) {
    view.setUint32(offset, id.length, true)
    offset += 4
    output.set(id, offset)
    offset += id.length
    view.setUint32(offset, value.length, true)
    offset += 4
    output.set(value, offset)
    offset += value.length
  }
  return output
}

function validatedBudget(overrides: Partial<WorkerMemoryBudget> | undefined): WorkerMemoryBudget {
  const budget = Object.freeze({ ...DEFAULT_WORKER_MEMORY_BUDGET, ...overrides })
  for (const [name, value] of Object.entries(budget)) assertPositiveSafeInteger(value, name)
  const accounted = accountedMemoryBytes(budget)
  if (accounted >= 128 * 1024 * 1024) {
    throw new RangeError(
      'configured input + payload + dirty output + output chunk + plan cache budget must stay below 128 MiB',
    )
  }
  return budget
}

function accountedMemoryBytes(budget: WorkerMemoryBudget): number {
  // A cursor never retains the completed archive, but all dirty entries can coexist while
  // generation starts. maxOutputBytes is therefore the conservative dirty-entry ceiling.
  return budget.maxInputBytes + budget.maxOutputBytes +
    budget.maxPayloadBytes + budget.maxOutputChunkBytes + budget.maxCachedPlanBytes
}

function assertPositiveSafeInteger(value: number, name: string): void {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new RangeError(`${name} must be a positive safe integer`)
  }
}

function bigintToSafeNumber(value: bigint, name: string): number {
  const converted = Number(value)
  if (!Number.isSafeInteger(converted) || converted < 0) throw new RangeError(`${name} is unsafe`)
  return converted
}

async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))
  return Array.from(digest, (byte) => byte.toString(16).padStart(2, '0')).join('')
}

class HttpError extends Error {
  readonly status: number

  constructor(status: number, message: string) {
    super(message)
    this.name = 'HttpError'
    this.status = status
  }
}
