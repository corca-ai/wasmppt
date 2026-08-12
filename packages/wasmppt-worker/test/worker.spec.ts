import { env, exports } from 'cloudflare:workers'
import { describe, expect, it } from 'vitest'
import { PreparedPlanCache, encodeLiveEditBundle } from '../src/index'

declare global {
  namespace Cloudflare {
    interface Env {
      HOST_FIXTURE: number[]
      RENDER_FIXTURE: number[]
      DOGFOOD_FIXTURE: number[]
      WORKER_P95_BUDGET_MS: number
      WORKER_LIVE_P95_BUDGET_MS: number
      WORKER_MEMORY_BUDGET_BYTES: number
    }
  }
}

const fixture = (): Uint8Array => new Uint8Array(env.HOST_FIXTURE)

describe('prepared plan cache', () => {
  it('does not release a handle while refreshing the same cache entry', () => {
    const released: number[] = []
    const cache = new PreparedPlanCache(32, (handle) => released.push(handle))
    const entry = Object.freeze({ handle: 7, weight: 12 })
    expect(cache.insert('template', entry)).toBe(true)
    expect(cache.insert('template', entry)).toBe(true)
    expect(released).toEqual([])
    expect(cache.residentBytes).toBe(12)
    expect(cache.get('template')).toBe(entry)
    cache.clear()
    expect(released).toEqual([7])
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
    expect(await response.json()).toEqual({ error: 'live edit bundle has an invalid magic' })
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

  it('matches the native and browser display-list structure in workerd', async () => {
    const response = await exports.default.fetch(
      new Request('https://wasmppt.test/v1/display-signature', {
        method: 'POST',
        body: new Uint8Array(env.RENDER_FIXTURE),
      }),
    )
    expect(response.status).toBe(200)
    expect(await response.json()).toEqual({ signature: '36fb217963862a0a' })
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
