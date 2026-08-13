import assert from 'node:assert/strict'
import test from 'node:test'
import { checkRepositoryContracts, contractErrors } from './check-contract-sync.mjs'

test('repository contracts stay synchronized across code, docs, fixtures, and CI', async () => {
  await checkRepositoryContracts()
})

test('contract checker reports every independently stale consumer', () => {
  const errors = contractErrors({
    rustDisplay: 'pub const DISPLAY_LIST_VERSION: u16 = 2;',
    canvas: 'if (version !== 1) {',
    capabilities: { displayListVersion: 1 },
    docs: { 'docs/rendering.md': 'WPDL v1' },
    displayTest: 'structural_signature(), 0xaaaa_bbbb',
    ci: "grep 'signature cccccccc'",
    workerTest: "signature: 'dddddddd'",
    browserIntegration: "const report = [{ id: 'text', slideIndex: 0 }]",
    nativeBenchmark: '',
    nativeBudgetEvaluator: '',
    renderCorpus: {
      presentations: [{ path: 'basic.pptx', features: [{ id: 'image' }] }],
    },
    corpus: { fixtures: [] },
    benchmarkFixtures: { slideCounts: [10, 50] },
    budgets: {
      browserScalarWasm: { maximumFirstVisibleSlideMs: 500 },
      native: { matrix: { 10: { maximumP95Ns: {} } } },
    },
  })

  assert.deepEqual(errors, [
    'capability matrix declares WPDL v1; Rust emits v2',
    'TypeScript decoder accepts WPDL versions 1; expected 1, 2',
    'docs/rendering.md does not identify WPDL v2 as the current format',
    'CI expects display signature cccccccc; Rust expects aaaabbbb',
    'Worker expects display signature dddddddd; Rust expects aaaabbbb',
    'visual report features (text) do not match render corpus (image)',
    'render fixture fixtures/render/basic.pptx is absent from fixtures/corpus.json',
    'browser performance budget maximumFirstVisibleSlideMs is not enforced by benchmark code',
    'native performance matrix budgets (10) do not match fixtures (10, 50)',
    'native benchmark does not publish per-metric budget margins',
  ])
})
