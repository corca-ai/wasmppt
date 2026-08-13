import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

function summary(lines, functions, regions) {
  return { data: [{ totals: {
    lines: { percent: lines },
    functions: { percent: functions },
    regions: { percent: regions },
  } }] }
}

test('coverage ratchet accepts the baseline and rejects a regression', async (context) => {
  const directory = await mkdtemp(join(tmpdir(), 'wasmppt-coverage-'))
  context.after(() => rm(directory, { recursive: true, force: true }))
  const baselinePath = join(directory, 'baseline.json')
  const summaryPath = join(directory, 'summary.json')
  await writeFile(baselinePath, JSON.stringify({
    schema: 1,
    minimum: { linePercent: 70, functionPercent: 60, regionPercent: 65 },
    tolerancePercentagePoints: 0.01,
  }))
  await writeFile(summaryPath, JSON.stringify(summary(70, 60, 65)))
  const script = new URL('check-coverage.mjs', import.meta.url)
  assert.equal(spawnSync(process.execPath, [script.pathname, summaryPath, baselinePath]).status, 0)

  await writeFile(summaryPath, JSON.stringify(summary(69.98, 60, 65)))
  const regressed = spawnSync(process.execPath, [script.pathname, summaryPath, baselinePath], {
    encoding: 'utf8',
  })
  assert.equal(regressed.status, 1)
  assert.match(regressed.stderr, /linePercent regressed/)
})
