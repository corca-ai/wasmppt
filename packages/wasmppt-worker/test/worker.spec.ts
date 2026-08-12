import { env, exports } from 'cloudflare:workers'
import { describe, expect, it } from 'vitest'

declare global {
  namespace Cloudflare {
    interface Env {
      HOST_FIXTURE: number[]
      RENDER_FIXTURE: number[]
      DOGFOOD_FIXTURE: number[]
      WORKER_P95_BUDGET_MS: number
      WORKER_MEMORY_BUDGET_BYTES: number
    }
  }
}

const fixture = (): Uint8Array => new Uint8Array(env.HOST_FIXTURE)

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
    expect(await response.json()).toEqual({ signature: '43e5ba4501300db3' })
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
    expect(p95Ms).toBeLessThanOrEqual(env.WORKER_P95_BUDGET_MS)
    expect(accountedMemoryBytes).toBeLessThanOrEqual(env.WORKER_MEMORY_BUDGET_BYTES)
    console.log(`WASM_WORKER_BENCHMARK:${JSON.stringify({
      schema: 1,
      host: 'cloudflare-workerd-scalar-wasm',
      fixture: 'host-minimal-potx',
      iterations: samplesMs.length,
      copies: { input: 1, output: 1 },
      warmRequestSamplesMs: samplesMs,
      summary: { warmRequestP50Ms: p50Ms, warmRequestP95Ms: p95Ms },
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
