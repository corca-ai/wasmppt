import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import { evaluateNativeBudget } from '../benchmarks/budget-evaluation.mjs'

const budgets = JSON.parse(await readFile(
  new URL('../benchmarks/budgets.json', import.meta.url),
  'utf8',
))

function benchmarkResult(slides) {
  const timings = Object.fromEntries(
    Object.keys(budgets.native.matrix[String(slides)].maximumP95Ns).map(
      (name) => [name, { p95Ns: slides * 1_000 }],
    ),
  )
  return {
    scenario: 'mixed',
    slides,
    summary: timings,
    peakResidentBytes: slides * 100_000,
    estimatedResidentBytes: slides * 10_000,
    outputBytes: slides * 50_000,
    generation: {
      dirtyUncompressedBytes: slides * 20_000,
      peakDirtyEntryBytes: 32_768,
      maximumOutputChunkBytes: 32_768,
    },
    zip: { rawCopiedEntries: 1, inflatedEntries: 0 },
    live: {
      summary: {
        applyDelta: { p95Ns: slides * 1_000 },
        inputToRenderReady: { p95Ns: slides * 2_000 },
        backgroundExport: { p95Ns: slides * 3_000 },
      },
      maximumInvalidatedSlides: 1,
      cache: { peakResidentBytes: slides * 30_000, minimumReusedMaterializedParts: 1 },
    },
  }
}

function operationResult() {
  return {
    summary: {
      applyDelta: { p95Ns: 1_000 },
      inputToRenderReady: { p95Ns: 2_000 },
    },
    maximumInvalidatedSlides: 1,
    maximumResidentBytes: 1_000_000,
  }
}

function passingReport() {
  return {
    configuration: { processRuns: 3, iterationsPerProcess: 10 },
    artifacts: { scalarWasmBytes: 1, metafileWasmBytes: 1, shaperWasmBytes: 1 },
    results: [10, 50, 200].map(benchmarkResult),
    liveOperations: {
      operations: {
        table: operationResult(),
        chart: operationResult(),
        slideTopology: { ...operationResult(), maximumInvalidatedSlides: 3 },
      },
    },
  }
}

test('native scale budgets publish a margin for every measured contract', () => {
  const evaluation = evaluateNativeBudget(passingReport(), budgets)

  assert.equal(evaluation.passed, true)
  assert.equal(evaluation.failures.length, 0)
  assert(evaluation.checks.length > 80)
  for (const check of evaluation.checks) {
    assert.equal(typeof check.margin, 'number', check.name)
    assert('marginPercent' in check, check.name)
  }
})

test('native scale budgets reject an absolute regression and superlinear growth', () => {
  const report = passingReport()
  const largest = report.results.at(-1)
  largest.summary.allSlides.p95Ns = 5_000_000_000

  const evaluation = evaluateNativeBudget(report, budgets)

  assert.equal(evaluation.passed, false)
  assert(evaluation.failures.some((failure) =>
    failure.startsWith('matrix.200.p95Ns.allSlides:')))
  assert(evaluation.failures.some((failure) =>
    failure.startsWith('growth.50-200.allSlidesP95Ns.normalized:')))
})

test('native scale budgets report a missing fixture without throwing', () => {
  const report = passingReport()
  report.results.pop()

  const evaluation = evaluateNativeBudget(report, budgets)

  assert.equal(evaluation.passed, false)
  assert(evaluation.failures.includes('matrix.200: missing fixture result'))
})
