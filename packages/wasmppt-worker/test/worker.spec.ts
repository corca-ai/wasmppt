import { env, exports } from 'cloudflare:workers'
import { describe, expect, it } from 'vitest'

declare global {
  namespace Cloudflare {
    interface Env {
      HOST_FIXTURE: number[]
      RENDER_FIXTURE: number[]
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
    expect(Number(response.headers.get('x-wasmppt-output-bytes'))).toBe(output.byteLength)
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
    expect(await response.json()).toEqual({ signature: '16041fe2c07f3636' })
  })
})

export {}
