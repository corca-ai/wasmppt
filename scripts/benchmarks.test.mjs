import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const root = new URL('../', import.meta.url)
const fixtures = JSON.parse(await readFile(new URL('benchmarks/fixtures.json', root), 'utf8'))
const budgets = JSON.parse(await readFile(new URL('benchmarks/budgets.json', root), 'utf8'))

test('benchmark matrix and all three release hosts remain machine-checkable', () => {
  assert.deepEqual(fixtures.scenarios, ['text', 'image', 'mixed'])
  assert.deepEqual(fixtures.slideCounts, [10, 50, 200])
  assert.equal(fixtures.scenarios.length * fixtures.slideCounts.length, 9)
  assert(budgets.native.maximumPeakResidentBytes > 0)
  assert(budgets.native.maximumPeakDirtyEntryBytes > 0)
  assert(budgets.native.maximumOutputChunkBytes > 0)
  assert(budgets.browserScalarWasm.maximumBinaryBytes > 0)
  assert(budgets.browserScalarWasm.maximumFirstVisibleSlideMs > 0)
  assert(budgets.browserScalarWasm.maximumRapidScrollAverageMs > 0)
  assert(budgets.browserScalarWasm.maximumStronglyReferencedSlides > 0)
  assert(budgets.cloudflareWorkerd.maximumWarmRequestP95Ms > 0)
  for (const name of ['coldTemplateCompile', 'warmInjection', 'firstSlide', 'visibleSlides', 'allSlides']) {
    assert(budgets.native.maximumP95Ns[name] > 0)
  }
})
