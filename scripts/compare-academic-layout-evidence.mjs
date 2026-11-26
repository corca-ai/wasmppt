import assert from 'node:assert/strict'
import { readFile, writeFile } from 'node:fs/promises'
import { gunzipSync } from 'node:zlib'
import { basename } from 'node:path'

const quantile = (values, fraction) => values.length ? values.toSorted((a, b) => a - b)[Math.max(0, Math.ceil(values.length * fraction) - 1)] : null
const identity = result => `${result.theme}/${result.presentation}/${result.case}`
const findings = result => result.rejected || result.failedSlides?.length > 0 || result.errors?.length > 0
function summarize(results) {
  const pages = results.flatMap(result => result.pages ?? [])
  const fonts = new Map()
  for (const fragment of pages.flatMap(page => page.fragments)) for (const size of fragment.fontSizes) {
    const values = fonts.get(fragment.role) ?? []
    values.push(size); fonts.set(fragment.role, values)
  }
  const images = pages.flatMap(page => page.fragments.flatMap(fragment => fragment.images.map(image => image.frame.width * image.frame.height / (page.pageWidth * page.pageHeight))))
  return {
    cases: results.length, rejected: results.filter(result => result.rejected).length,
    casesWithFindings: results.filter(findings).length, failedLogicalSlides: results.reduce((sum, result) => sum + (result.failedSlides?.length ?? 0), 0),
    pageCount: pages.length,
    fontSizeRangeHundredthPointByRole: Object.fromEntries([...fonts].map(([role, values]) => [role, { minimum: Math.min(...values), maximum: Math.max(...values) }])),
    medianPreferredBodyAreaRatio: quantile(pages.map(page => page.preferredBodyAreaRatio).filter(value => value !== null), 0.5),
    actualFullWidthTextProbe: pages.filter(page => page.actualBodySafeAreaRatio !== null && page.actualBodySafeAreaRatio !== undefined).map(page => ({ actualBodyTop: page.actualBodyTop, actualBodySafeAreaRatio: page.actualBodySafeAreaRatio, bodyReclaimedEmu: page.bodyReclaimedEmu, bodyReclaimOpportunityEmu: page.bodyReclaimOpportunityEmu, measuredHeaderHeight: page.measuredHeaderHeight, preferredHeaderHeight: page.preferredHeaderHeight, basis: page.actualBodyMetricReason })),
    medianVisibleImageAreaRatio: quantile(images, 0.5), medianWhitespaceRatio: quantile(pages.map(page => page.whitespaceRatio), 0.5),
    replaceP50Ms: quantile(results.map(result => result.elapsedMs), 0.5), replaceP95Ms: quantile(results.map(result => result.elapsedMs), 0.95),
    findings: results.filter(findings).map(result => ({ case: result.case, errors: result.errors ?? [], diagnostics: result.diagnostics.filter(diagnostic => diagnostic.severity === 'error') })),
  }
}
async function readReport(path) {
  const bytes = await readFile(path)
  return JSON.parse((path.endsWith('.gz') ? gunzipSync(bytes) : bytes).toString())
}
const [beforePath, afterPath, outputPath, beforeMemoryPath, afterMemoryPath] = process.argv.slice(2)
assert.ok(beforePath && afterPath && outputPath, 'usage: compare-academic-layout-evidence BEFORE AFTER OUTPUT [BEFORE_MEMORY AFTER_MEMORY]')
assert.equal(Boolean(beforeMemoryPath), Boolean(afterMemoryPath), 'supply both OS memory reports')
const before = await readReport(beforePath), after = await readReport(afterPath)
assert.equal(before.corpusHash, after.corpusHash, 'before/after semantic corpus differs')
assert.deepEqual(before.assets, after.assets, 'before/after asset bytes or dimensions differ')
assert.deepEqual(before.results.map(identity).toSorted(), after.results.map(identity).toSorted(), 'before/after case inventory differs')
const matrix = [...new Set(before.results.map(result => `${result.theme}/${result.presentation}`))].map(key => {
  const selected = report => report.results.filter(result => `${result.theme}/${result.presentation}` === key)
  const oldResults = selected(before), newResults = selected(after)
  return { key, sameTemplateBytes: oldResults.every((oldResult, index) => oldResult.templateHash === newResults[index].templateHash), before: summarize(oldResults), after: summarize(newResults) }
})
async function osPeak(path) {
  const raw = await readFile(path, 'utf8')
  const match = /(\d+)\s+maximum resident set size/.exec(raw)
  assert.ok(match, 'expected /usr/bin/time -l maximum resident set size')
  return { report: basename(path), maximumResidentBytes: Number(match[1]), scope: 'Entire Node process per 480-case matrix, including startup/template preparation, all SDK replacements, all-page resolution, assets, retained report data and JSON output. OS maximum resident set, not engine-scoped heap, logical admission or sampled RSS.' }
}
const processOsPeakMemory = beforeMemoryPath ? { before: await osPeak(beforeMemoryPath), after: await osPeak(afterMemoryPath) } : undefined
await writeFile(outputPath, `${JSON.stringify({ processOsPeakMemory, schema: 1, corpusHash: before.corpusHash, engineBefore: before.engineWasmHash, engineAfter: after.engineWasmHash, measurement: 'Single SDK replace sample per case with reused engine/assets; template preparation/startup and all-page resolution excluded. Memory is sampled after each snapshot, not a process high-water measurement. Whitespace and page count are context, not independent success criteria.', limits: { candidateCount: after.candidateCount, reason: after.candidateCountReason }, before: { ...summarize(before.results), maxAccountedBytes: before.maxAccountedBytes, sampledProcessRssBytes: before.processPeakRssBytes }, after: { ...summarize(after.results), maxAccountedBytes: after.maxAccountedBytes, sampledProcessRssBytes: after.processPeakRssBytes }, matrix }, null, 2)}\n`)
