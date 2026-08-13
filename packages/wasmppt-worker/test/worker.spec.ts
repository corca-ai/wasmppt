import { env, exports } from 'cloudflare:workers'
import { describe, expect, it } from 'vitest'
import {
  PreparedPlanCache,
  createWasmpptWorker,
  encodeLiveEditBundle,
  type WorkerEngine,
} from '../src/index'

declare global {
  namespace Cloudflare {
    interface Env {
      HOST_FIXTURE: number[]
      RENDER_FIXTURE: number[]
      DOGFOOD_FIXTURE: number[]
      WORKER_P95_BUDGET_MS: number
      WORKER_LIVE_P95_BUDGET_MS: number
      WORKER_MEMORY_BUDGET_BYTES: number
      PARITY_PAYLOAD: number[]
    }
  }
}

const fixture = (): Uint8Array => new Uint8Array(env.HOST_FIXTURE)
const testContext = {} as ExecutionContext

describe('prepared plan cache', () => {
  it('does not release a handle while refreshing the same cache entry', () => {
    const released: number[] = []
    const cache = new PreparedPlanCache(32, (handle) => released.push(handle))
    const entry = Object.freeze({ handle: 7, weight: 12 })
    expect(cache.insert('template', entry)).toBe(true)
    expect(cache.insert('template', entry)).toBe(true)
    expect(released).toEqual([])
    expect(cache.residentBytes).toBe(12)
    const lease = cache.acquire('template')
    expect(lease?.handle).toBe(entry.handle)
    lease?.release()
    cache.clear()
    expect(released).toEqual([7])
  })

  it('defers eviction and clear until every active lease is released exactly once', () => {
    const released: number[] = []
    const cache = new PreparedPlanCache(12, (handle) => released.push(handle))
    expect(cache.insert('first', { handle: 1, weight: 8 })).toBe(true)
    const first = cache.acquire('first')!

    expect(cache.insert('second', { handle: 2, weight: 8 })).toBe(true)
    expect(cache.residentBytes).toBe(8)
    expect(cache.retiredBytes).toBe(8)
    expect(cache.ownedBytes).toBe(16)
    expect(released).toEqual([])

    const second = cache.acquire('second')!
    cache.clear()
    cache.clear()
    expect(cache.residentBytes).toBe(0)
    expect(cache.retiredBytes).toBe(16)
    first.release()
    first.release()
    expect(released).toEqual([1])
    second.release()
    second.release()
    expect(released).toEqual([1, 2])
    expect(cache.retiredBytes).toBe(0)
    expect(cache.ownedBytes).toBe(0)
  })

  it('retires a replaced same-key handle without invalidating its lease', () => {
    const released: number[] = []
    const cache = new PreparedPlanCache(16, (handle) => released.push(handle))
    expect(cache.insert('template', { handle: 1, weight: 8 })).toBe(true)
    const oldLease = cache.acquire('template')!
    expect(cache.insert('template', { handle: 2, weight: 8 })).toBe(true)
    const currentLease = cache.acquire('template')!
    expect(currentLease.handle).toBe(2)
    expect(released).toEqual([])
    oldLease.release()
    expect(released).toEqual([1])
    cache.clear()
    currentLease.release()
    expect(released).toEqual([1, 2])
  })

  it('rejects unsafe cache weights', () => {
    const cache = new PreparedPlanCache(32, () => {})
    expect(() => cache.insert('template', { handle: 7, weight: Number.NaN })).toThrow(
      /cache weight/,
    )
  })

  it('bounds request-local live edit bundles before allocation', () => {
    expect(() => encodeLiveEditBundle(new Uint8Array(), Array.from(
      { length: 10_001 },
      () => new Uint8Array(),
    ))).toThrow(/too many deltas/)
  })
})

describe('wasmppt workerd adapter', () => {
  it('pins different-key cache entries across an awaited request body', async () => {
    await env.TEMPLATES.put('lease-a.potx', fixture())
    await env.TEMPLATES.put('lease-b.potx', fixture())
    const engine = new LeaseTestEngine(8n)
    const worker = createWasmpptWorker(engine, { budget: { maxCachedPlanBytes: 8 } })
    const paused = pausedBody()
    const first = dispatch(worker, injectionRequest('lease-a.potx', paused.stream))
    await engine.waitForPrepared(1)

    const second = await dispatch(worker, injectionRequest('lease-b.potx', new Uint8Array()))
    expect(second.status).toBe(200)
    await second.arrayBuffer()
    expect(engine.releasedTemplates).toEqual([])
    const whilePinned = await dispatch(worker, new Request('https://wasmppt.test/healthz'))
    expect(await whilePinned.json()).toMatchObject({
      cachedPlanBytes: 8,
      pinnedEvictedPlanBytes: 8,
      ownedPlanBytes: 16,
    })

    paused.close()
    const firstResponse = await first
    expect(firstResponse.status).toBe(200)
    await firstResponse.arrayBuffer()
    expect(engine.releasedTemplates).toEqual([1])
  })

  it('shares one pinned handle between concurrent same-key requests', async () => {
    await env.TEMPLATES.put('lease-same.potx', fixture())
    const engine = new LeaseTestEngine(8n)
    const worker = createWasmpptWorker(engine, { budget: { maxCachedPlanBytes: 8 } })
    const paused = pausedBody()
    const first = dispatch(worker, injectionRequest('lease-same.potx', paused.stream))
    await engine.waitForPrepared(1)

    const second = await dispatch(worker, injectionRequest('lease-same.potx', new Uint8Array()))
    expect(second.status).toBe(200)
    await second.arrayBuffer()
    expect(engine.preparedTemplates).toEqual([1])
    expect(engine.releasedTemplates).toEqual([])

    paused.close()
    const firstResponse = await first
    expect(firstResponse.status).toBe(200)
    await firstResponse.arrayBuffer()
    expect(engine.releasedTemplates).toEqual([])
  })

  it('releases a lease when request-body cancellation fails the read', async () => {
    await env.TEMPLATES.put('lease-cancel.potx', fixture())
    await env.TEMPLATES.put('lease-after-cancel.potx', fixture())
    const engine = new LeaseTestEngine(8n)
    const worker = createWasmpptWorker(engine, { budget: { maxCachedPlanBytes: 8 } })
    const paused = pausedBody()
    const cancelled = dispatch(worker, injectionRequest('lease-cancel.potx', paused.stream))
    await engine.waitForPrepared(1)
    paused.fail(new DOMException('request cancelled', 'AbortError'))
    const cancelledResponse = await cancelled
    expect(cancelledResponse.status).toBe(499)
    expect(await cancelledResponse.json()).toMatchObject({
      error: { version: 1, domain: 'runtime', code: 'cancelled' },
    })

    const next = await dispatch(
      worker,
      injectionRequest('lease-after-cancel.potx', new Uint8Array()),
    )
    expect(next.status).toBe(200)
    await next.arrayBuffer()
    expect(engine.releasedTemplates).toEqual([1])
  })

  it('keeps an oversized prepared handle request-local', async () => {
    await env.TEMPLATES.put('lease-oversized.potx', fixture())
    const engine = new LeaseTestEngine(9n)
    const worker = createWasmpptWorker(engine, { budget: { maxCachedPlanBytes: 8 } })
    const response = await dispatch(
      worker,
      injectionRequest('lease-oversized.potx', new Uint8Array()),
    )
    expect(response.status).toBe(200)
    expect(engine.releasedTemplates).toEqual([1])
    await response.arrayBuffer()
    const health = await dispatch(worker, new Request('https://wasmppt.test/healthz'))
    expect(await health.json()).toMatchObject({
      cachedPlanBytes: 0,
      pinnedEvictedPlanBytes: 0,
      ownedPlanBytes: 0,
    })
  })

  it('executes the shared native/browser/workerd fixture and streams PPTX', async () => {
    const response = await exports.default.fetch(
      new Request('https://wasmppt.test/v1/generate', {
        method: 'POST',
        body: fixture(),
      }),
    )
    expect(response.status).toBe(200)
    expect(response.headers.get('content-type')).toContain('presentationml.presentation')
    expect(Number(response.headers.get('x-wasmppt-accounted-memory-bytes'))).toBeLessThan(
      128 * 1024 * 1024,
    )
    const output = new Uint8Array(await response.arrayBuffer())
    expect([...output.subarray(0, 2)]).toEqual([0x50, 0x4b])
    expect(response.headers.get('x-wasmppt-output-mode')).toBe('pull-stream')
  })

  it('generates parity evidence from the exact shared WPPD payload', async () => {
    await env.TEMPLATES.put('parity-minimal.potx', fixture())
    const response = await exports.default.fetch(
      new Request('https://wasmppt.test/v1/generate?r2=parity-minimal.potx', {
        method: 'POST',
        headers: { 'content-type': 'application/vnd.corca.wasmppt.injection-v2' },
        body: new Uint8Array(env.PARITY_PAYLOAD),
      }),
    )
    expect(response.status).toBe(200)
    const output = new Uint8Array(await response.arrayBuffer())
    expect([...output.subarray(0, 2)]).toEqual([0x50, 0x4b])
    console.log(`PPTX_PARITY_WORKERD:${btoa(String.fromCharCode(...output))}`)
  })

  it('reads the same fixture through ranged R2 binding calls', async () => {
    await env.TEMPLATES.put('minimal.potx', fixture())
    const response = await exports.default.fetch(
      new Request('https://wasmppt.test/v1/generate?r2=minimal.potx', { method: 'POST' }),
    )
    expect(response.status).toBe(200)
    const output = new Uint8Array(await response.arrayBuffer())
    expect([...output.subarray(0, 2)]).toEqual([0x50, 0x4b])

    const health = await exports.default.fetch('https://wasmppt.test/healthz')
    const state = await health.json<{ readonly cachedPlanBytes: number }>()
    expect(state.cachedPlanBytes).toBeGreaterThan(0)
    expect(state.cachedPlanBytes).toBeLessThanOrEqual(32 * 1024 * 1024)
  })

  it('accepts the shared structured payload for an R2 template', async () => {
    await env.TEMPLATES.put('report.potx', new Uint8Array(env.DOGFOOD_FIXTURE))
    const response = await exports.default.fetch(
      new Request('https://wasmppt.test/v1/generate?r2=report.potx', {
        method: 'POST',
        headers: { 'content-type': 'application/vnd.corca.wasmppt.injection-v1' },
        body: dogfoodPayload(),
      }),
    )
    expect(response.status).toBe(200)
    const output = new Uint8Array(await response.arrayBuffer())
    expect([...output.subarray(0, 2)]).toEqual([0x50, 0x4b])
    expect(output.byteLength).toBeGreaterThan(2_000)
  })

  it('keeps a multi-delta live session inside one request and streams its final revision', async () => {
    await env.TEMPLATES.put('live-report.potx', new Uint8Array(env.DOGFOOD_FIXTURE))
    const response = await exports.default.fetch(
      new Request('https://wasmppt.test/v1/live-generate?r2=live-report.potx', {
        method: 'POST',
        body: encodeLiveEditBundle(dogfoodPayload(), [
          textDeltaPayload('first live edit'),
          textDeltaPayload('final live edit'),
        ]),
      }),
    )
    expect(response.status).toBe(200)
    expect(response.headers.get('x-wasmppt-live-revision')).toBe('2')
    const output = new Uint8Array(await response.arrayBuffer())
    expect([...output.subarray(0, 2)]).toEqual([0x50, 0x4b])
    expect(output.byteLength).toBeGreaterThan(2_000)
  })

  it('rejects malformed request-local live bundles before creating a session', async () => {
    await env.TEMPLATES.put('invalid-live.potx', new Uint8Array(env.DOGFOOD_FIXTURE))
    const response = await exports.default.fetch(
      new Request('https://wasmppt.test/v1/live-generate?r2=invalid-live.potx', {
        method: 'POST',
        body: new Uint8Array(16),
      }),
    )
    expect(response.status).toBe(400)
    expect(await response.json()).toEqual({
      error: {
        version: 1,
        domain: 'runtime',
        code: 'invalid-request',
        message: 'live edit bundle has an invalid magic',
      },
    })
  })

  it('rejects an advertised body larger than the bounded input budget', async () => {
    const response = await exports.default.fetch(
      new Request('https://wasmppt.test/v1/generate', {
        method: 'POST',
        headers: { 'content-length': String(17 * 1024 * 1024) },
        body: fixture(),
      }),
    )
    expect(response.status).toBe(413)
  })

  it('preserves the native package code for the same invalid template bytes', async () => {
    const response = await exports.default.fetch(
      new Request('https://wasmppt.test/v1/generate', {
        method: 'POST',
        body: new TextEncoder().encode('not a zip'),
      }),
    )
    expect(response.status).toBe(400)
    expect(response.headers.get('x-wasmppt-error-version')).toBe('1')
    expect(await response.json()).toMatchObject({
      error: { version: 1, domain: 'package', code: 'truncated' },
    })
  })

  it('maps known Wasm codes to deliberate statuses and hides unknown internal details', async () => {
    const wasmError = new Error('unknown generation handle') as Error & { wasmppt?: unknown }
    wasmError.wasmppt = {
      version: 1,
      domain: 'runtime',
      code: 'unknown-handle',
      message: wasmError.message,
    }
    const conflictWorker = createWasmpptWorker(new ThrowingEngine(wasmError))
    const conflict = await dispatch(
      conflictWorker,
      new Request('https://wasmppt.test/v1/generate', { method: 'POST', body: fixture() }),
    )
    expect(conflict.status).toBe(409)
    expect(await conflict.json()).toMatchObject({
      error: { domain: 'runtime', code: 'unknown-handle' },
    })

    const internalWorker = createWasmpptWorker(new ThrowingEngine(new Error('secret detail')))
    const internal = await dispatch(
      internalWorker,
      new Request('https://wasmppt.test/v1/generate', { method: 'POST', body: fixture() }),
    )
    expect(internal.status).toBe(500)
    expect(await internal.json()).toEqual({
      error: {
        version: 1,
        domain: 'runtime',
        code: 'internal',
        message: 'internal wasmppt failure',
      },
    })
  })

  it('matches the native and browser display-list structure in workerd', async () => {
    const response = await exports.default.fetch(
      new Request('https://wasmppt.test/v1/display-signature', {
        method: 'POST',
        body: new Uint8Array(env.RENDER_FIXTURE),
      }),
    )
    expect(response.status).toBe(200)
    expect(await response.json()).toEqual({ signature: '0698523062a91bcd' })
  })

  it('enforces the warm Cloudflare workerd release budget with raw samples', async () => {
    const samplesMs: number[] = []
    let outputBytes = 0
    let accountedMemoryBytes = 0
    for (let iteration = 0; iteration < 15; iteration += 1) {
      const start = performance.now()
      const response = await exports.default.fetch(
        new Request('https://wasmppt.test/v1/generate', { method: 'POST', body: fixture() }),
      )
      expect(response.status).toBe(200)
      const output = await response.arrayBuffer()
      samplesMs.push(performance.now() - start)
      outputBytes = output.byteLength
      accountedMemoryBytes = Number(response.headers.get('x-wasmppt-accounted-memory-bytes'))
    }
    const sorted = [...samplesMs].sort((left, right) => left - right)
    const p50Ms = sorted[Math.ceil(sorted.length * 0.5) - 1]!
    const p95Ms = sorted[Math.ceil(sorted.length * 0.95) - 1]!
    await env.TEMPLATES.put('live-benchmark.potx', new Uint8Array(env.DOGFOOD_FIXTURE))
    const liveSamplesMs: number[] = []
    let liveOutputBytes = 0
    const bundle = encodeLiveEditBundle(
      dogfoodPayload(),
      [textDeltaPayload('workerd live benchmark')],
    )
    for (let iteration = 0; iteration < 15; iteration += 1) {
      const start = performance.now()
      const response = await exports.default.fetch(
        new Request('https://wasmppt.test/v1/live-generate?r2=live-benchmark.potx', {
          method: 'POST',
          body: bundle,
        }),
      )
      expect(response.status).toBe(200)
      expect(response.headers.get('x-wasmppt-live-revision')).toBe('1')
      liveOutputBytes = (await response.arrayBuffer()).byteLength
      liveSamplesMs.push(performance.now() - start)
    }
    const liveSorted = [...liveSamplesMs].sort((left, right) => left - right)
    const liveP50Ms = liveSorted[Math.ceil(liveSorted.length * 0.5) - 1]!
    const liveP95Ms = liveSorted[Math.ceil(liveSorted.length * 0.95) - 1]!
    expect(p95Ms).toBeLessThanOrEqual(env.WORKER_P95_BUDGET_MS)
    expect(liveP95Ms).toBeLessThanOrEqual(env.WORKER_LIVE_P95_BUDGET_MS)
    expect(accountedMemoryBytes).toBeLessThanOrEqual(env.WORKER_MEMORY_BUDGET_BYTES)
    console.log(`WASM_WORKER_BENCHMARK:${JSON.stringify({
      schema: 2,
      host: 'cloudflare-workerd-scalar-wasm',
      fixture: 'host-minimal-potx',
      iterations: samplesMs.length,
      copies: { input: 1, output: 1 },
      warmRequestSamplesMs: samplesMs,
      summary: { warmRequestP50Ms: p50Ms, warmRequestP95Ms: p95Ms },
      live: {
        fixture: 'dogfood-report-potx',
        revisionsPerRequest: 1,
        requestLocalSession: true,
        copiesPerRequest: { bundleInput: 1, unchangedMedia: 0, output: 1 },
        requestSamplesMs: liveSamplesMs,
        summary: { p50Ms: liveP50Ms, p95Ms: liveP95Ms },
        outputBytes: liveOutputBytes,
      },
      correctness: { outputBytes, accountedMemoryBytes },
    })}`)
  })
})

export {}

function dispatch(
  worker: ReturnType<typeof createWasmpptWorker>,
  request: Request,
): Promise<Response> {
  // Requests constructed inside workerd tests lack runtime-populated `cf` metadata in their type.
  return Promise.resolve(worker.fetch!(request as never, env, testContext))
}

function injectionRequest(key: string, body: BodyInit): Request {
  return new Request(`https://wasmppt.test/v1/generate?r2=${key}`, {
    method: 'POST',
    headers: { 'content-type': 'application/vnd.corca.wasmppt.injection-v2' },
    body,
  })
}

function pausedBody(): {
  readonly stream: ReadableStream<Uint8Array>
  readonly close: () => void
  readonly fail: (error: Error) => void
} {
  let controller: ReadableStreamDefaultController<Uint8Array> | undefined
  const stream = new ReadableStream<Uint8Array>({
    start(value) {
      controller = value
    },
  })
  return {
    stream,
    close: () => controller?.close(),
    fail: (error) => controller?.error(error),
  }
}

class LeaseTestEngine implements WorkerEngine {
  readonly preparedTemplates: number[] = []
  readonly releasedTemplates: number[] = []
  readonly #templateWeight: bigint
  readonly #validTemplates = new Set<number>()
  readonly #finishedGenerations = new Set<number>()
  readonly #preparedWaiters: (() => void)[] = []
  #nextTemplate = 1
  #nextGeneration = 100

  constructor(templateWeight: bigint) {
    this.#templateWeight = templateWeight
  }

  prepare(): number {
    const handle = this.#nextTemplate
    this.#nextTemplate += 1
    this.preparedTemplates.push(handle)
    this.#validTemplates.add(handle)
    for (const notify of this.#preparedWaiters.splice(0)) notify()
    return handle
  }

  async waitForPrepared(count: number): Promise<void> {
    while (this.preparedTemplates.length < count) {
      await new Promise<void>((resolve) => this.#preparedWaiters.push(resolve))
    }
  }

  prepared_weight(handle: number): bigint {
    this.#assertTemplate(handle)
    return this.#templateWeight
  }

  start_generation_payload(handle: number): number {
    this.#assertTemplate(handle)
    const generation = this.#nextGeneration
    this.#nextGeneration += 1
    return generation
  }

  create_live_session_payload(): number {
    throw new Error('live sessions are not used by this test engine')
  }

  apply_live_session_payload(): unknown[] {
    throw new Error('live sessions are not used by this test engine')
  }

  start_live_session_generation(): number {
    throw new Error('live sessions are not used by this test engine')
  }

  generation_pull(handle: number): Uint8Array {
    this.#finishedGenerations.add(handle)
    return Uint8Array.of(0x50, 0x4b)
  }

  generation_done(handle: number): boolean {
    return this.#finishedGenerations.has(handle)
  }

  release_template(handle: number): boolean {
    if (!this.#validTemplates.delete(handle)) return false
    this.releasedTemplates.push(handle)
    return true
  }

  release_generation(handle: number): boolean {
    return this.#finishedGenerations.delete(handle)
  }

  release_live_session(): boolean {
    return false
  }

  #assertTemplate(handle: number): void {
    if (!this.#validTemplates.has(handle)) throw new Error('released template handle reused')
  }
}

class ThrowingEngine implements WorkerEngine {
  readonly #error: Error

  constructor(error: Error) {
    this.#error = error
  }

  prepare(): number { throw this.#error }
  prepared_weight(): bigint { return 0n }
  start_generation_payload(): number { throw this.#error }
  create_live_session_payload(): number { throw this.#error }
  apply_live_session_payload(): unknown[] { throw this.#error }
  start_live_session_generation(): number { throw this.#error }
  generation_pull(): Uint8Array { throw this.#error }
  generation_done(): boolean { return true }
  release_template(): boolean { return false }
  release_generation(): boolean { return false }
  release_live_session(): boolean { return false }
}

function dogfoodPayload(): Uint8Array {
  const chunks: Uint8Array[] = []
  const encoder = new TextEncoder()
  const u32 = (value: number): void => {
    const bytes = new Uint8Array(4)
    new DataView(bytes.buffer).setUint32(0, value, true)
    chunks.push(bytes)
  }
  const bytes = (value: Uint8Array): void => {
    u32(value.byteLength)
    chunks.push(value)
  }
  const string = (value: string): void => bytes(encoder.encode(value))

  chunks.push(new Uint8Array([0x57, 0x50, 0x50, 0x44]))
  u32(1)
  u32(2)
  string('subtitle'); string('generated in workerd')
  string('title'); string('structured Generation API v1')
  u32(1)
  string('hero'); string('png'); string('image/png')
  chunks.push(Uint8Array.of(0))
  bytes(Uint8Array.from(atob(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M/wHwAF/gL+XHI4ZAAAAABJRU5ErkJggg==',
  ), (value) => value.charCodeAt(0)))
  u32(1)
  string('metrics')
  u32(2)
  const rows: readonly (readonly [string, string])[] = [
    ['Latency', '12 ms'],
    ['Throughput', '4,200 slides/s'],
  ]
  for (const [label, value] of rows) {
    u32(2)
    string('label'); string(label)
    string('value'); string(value)
  }
  u32(1)
  string('ppt/slides/slide2.xml'); u32(1)
  u32(0)

  const length = chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0)
  const output = new Uint8Array(length)
  let offset = 0
  for (const chunk of chunks) {
    output.set(chunk, offset)
    offset += chunk.byteLength
  }
  return output
}

function textDeltaPayload(title: string): Uint8Array {
  const encoder = new TextEncoder()
  const titleBytes = encoder.encode(title)
  const idBytes = encoder.encode('title')
  const output = new Uint8Array(4 + 4 + 4 + 4 + idBytes.length + 4 + titleBytes.length + 16)
  const view = new DataView(output.buffer)
  let offset = 0
  output.set([0x57, 0x50, 0x50, 0x44], offset); offset += 4
  view.setUint32(offset, 1, true); offset += 4
  view.setUint32(offset, 1, true); offset += 4
  view.setUint32(offset, idBytes.length, true); offset += 4
  output.set(idBytes, offset); offset += idBytes.length
  view.setUint32(offset, titleBytes.length, true); offset += 4
  output.set(titleBytes, offset); offset += titleBytes.length
  for (let index = 0; index < 4; index += 1) {
    view.setUint32(offset, 0, true); offset += 4
  }
  return output
}
