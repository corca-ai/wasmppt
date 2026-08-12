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
  assert.deepEqual(Object.keys(budgets.nativeLive), ['10', '50', '200'])
  assert.deepEqual(Object.keys(budgets.nativeLiveOperations), ['table', 'chart', 'slideTopology'])
  assert(budgets.browserScalarWasm.maximumBinaryBytes > 0)
  assert(budgets.browserScalarWasm.maximumFirstVisibleSlideMs > 0)
  assert(budgets.browserScalarWasm.maximumLiveInputToPixelsP95Ms > 0)
  assert(budgets.browserScalarWasm.maximumLiveBackgroundExportMs > 0)
  assert(budgets.browserScalarWasm.maximumLivePeakCacheBytes > 0)
  assert.equal(budgets.browserScalarWasm.maximumLiveInvalidatedSlides, 1)
  assert(budgets.browserScalarWasm.maximumLiveSustainedAverageMs > 0)
  assert(budgets.browserScalarWasm.maximumRapidScrollAverageMs > 0)
  assert(budgets.browserScalarWasm.maximumStronglyReferencedSlides > 0)
  assert(budgets.cloudflareWorkerd.maximumWarmRequestP95Ms > 0)
  assert(budgets.cloudflareWorkerd.maximumLiveRequestP95Ms > 0)
  for (const name of ['coldTemplateCompile', 'warmInjection', 'firstSlide', 'visibleSlides', 'allSlides']) {
    assert(budgets.native.maximumP95Ns[name] > 0)
  }
  for (const budget of Object.values(budgets.nativeLive)) {
    assert(budget.maximumApplyDeltaP95Ns > 0)
    assert(budget.maximumInputToRenderReadyP95Ns > 0)
    assert(budget.maximumBackgroundExportP95Ns > 0)
    assert.equal(budget.maximumInvalidatedSlides, 1)
    assert(budget.minimumReusedMaterializedParts > 0)
  }
  for (const budget of Object.values(budgets.nativeLiveOperations)) {
    assert(budget.maximumApplyDeltaP95Ns > 0)
    assert(budget.maximumInputToRenderReadyP95Ns > 0)
    assert(budget.maximumInvalidatedSlides > 0)
    assert(budget.maximumResidentBytes > 0)
  }
})
