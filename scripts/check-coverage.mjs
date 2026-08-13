import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'

const [summaryPath = 'target/coverage/summary.json', baselinePath = 'quality/coverage-baseline.json'] =
  process.argv.slice(2)
const summary = JSON.parse(await readFile(summaryPath, 'utf8'))
const baseline = JSON.parse(await readFile(baselinePath, 'utf8'))
assert.equal(baseline.schema, 1, 'unsupported coverage baseline schema')
const totals = summary.data?.[0]?.totals
assert(totals !== undefined, 'coverage summary has no totals')

const measurements = {
  linePercent: totals.lines?.percent,
  functionPercent: totals.functions?.percent,
  regionPercent: totals.regions?.percent,
}
const tolerance = baseline.tolerancePercentagePoints
for (const [metric, minimum] of Object.entries(baseline.minimum)) {
  const actual = measurements[metric]
  assert(Number.isFinite(actual), `coverage summary has no finite ${metric}`)
  if (actual + tolerance < minimum) {
    throw new Error(
      `${metric} regressed: ${actual.toFixed(2)}% < ${minimum.toFixed(2)}% baseline ` +
        `(tolerance ${tolerance.toFixed(2)} percentage points)`,
    )
  }
}

console.log(
  `coverage ratchet ok: lines ${measurements.linePercent.toFixed(2)}%, ` +
    `functions ${measurements.functionPercent.toFixed(2)}%, ` +
    `regions ${measurements.regionPercent.toFixed(2)}%`,
)
