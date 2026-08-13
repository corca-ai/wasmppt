import { describe, expect, it } from 'vitest'
import { PreparedPlanCache, encodeLiveEditBundle } from '../src/index'

describe('prepared plan cache without workerd', () => {
  it('keeps a refreshed handle alive and releases it once', () => {
    const released: number[] = []
    const cache = new PreparedPlanCache(32, (handle) => released.push(handle))
    const entry = Object.freeze({ handle: 7, weight: 12 })

    expect(cache.insert('template', entry)).toBe(true)
    expect(cache.insert('template', entry)).toBe(true)
    const lease = cache.acquire('template')
    lease?.release()
    lease?.release()
    cache.clear()

    expect(released).toEqual([7])
  })

  it('defers eviction until an active lease is released', () => {
    const released: number[] = []
    const cache = new PreparedPlanCache(12, (handle) => released.push(handle))
    expect(cache.insert('first', { handle: 1, weight: 8 })).toBe(true)
    const first = cache.acquire('first')!

    expect(cache.insert('second', { handle: 2, weight: 8 })).toBe(true)
    expect(cache.retiredBytes).toBe(8)
    expect(released).toEqual([])
    first.release()

    expect(released).toEqual([1])
    expect(cache.retiredBytes).toBe(0)
  })

  it('rejects unbounded live edit bundles before allocation', () => {
    expect(() =>
      encodeLiveEditBundle(
        new Uint8Array(),
        Array.from({ length: 10_001 }, () => new Uint8Array()),
      ),
    ).toThrow(/too many deltas/)
  })
})
