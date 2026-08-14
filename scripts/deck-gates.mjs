import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const options = Object.fromEntries(process.argv.slice(2).map((argument) => {
  const match = /^--([^=]+)=(.+)$/.exec(argument)
  if (match === null) throw new Error(`invalid deck gate argument: ${argument}`)
  return [match[1], match[2]]
}))
const output = resolve(root, options.output ?? 'target/deck-gates')
const workerdLog = resolve(root, options['workerd-log'] ?? 'target/host-parity/workerd.log')

await mkdir(output, { recursive: true })
const workerd = parseWorkerdEvidence(await readFile(workerdLog, 'utf8'))
await writeEvidence(output, 'workerd', workerd)

const nativeTopology = JSON.parse(await readFile(resolve(output, 'native-topology.json'), 'utf8'))
const browserTopology = JSON.parse(await readFile(resolve(output, 'browser-topology.json'), 'utf8'))
const nativeTimings = JSON.parse(await readFile(resolve(output, 'native-timings.json'), 'utf8'))
const browserTimings = JSON.parse(await readFile(resolve(output, 'browser-timings.json'), 'utf8'))
const nativeQuality = JSON.parse(await readFile(resolve(output, 'native-quality.json'), 'utf8'))
const browserQuality = JSON.parse(await readFile(resolve(output, 'browser-quality.json'), 'utf8'))
validateQuality(nativeQuality, browserQuality, nativeTopology)
for (const [host, timings] of [
  ['native', nativeTimings],
  ['browser', browserTimings],
  ['workerd', workerd.timings],
]) validateTimings(host, timings)
assert.deepEqual(browserTopology, nativeTopology, 'browser deck topology differs from native')
assert.deepEqual(workerd.topology, nativeTopology, 'workerd deck topology differs from native')

const names = ['wdtp', 'wdpl', 'pptx']
const slideFiles = (await readdir(output))
  .filter((name) => /^native-\d{4}\.wpdl$/.test(name))
  .sort()
assert.equal(slideFiles.length, nativeTopology.slideCount, 'native WPDL count differs from topology')
for (const name of names) await compareAllHosts(output, name)
for (const nativeName of slideFiles) {
  const suffix = nativeName.slice('native-'.length)
  await compareAllHosts(output, suffix)
}

const nativePlan = await readFile(resolve(output, 'native.wdpl'))
const mutated = Buffer.from(nativePlan)
mutated[mutated.length - 1] ^= 1
assert.throws(
  () => assertHostParity([nativePlan, mutated, nativePlan], 'mutated wdpl'),
  /browser mutated wdpl bytes differ from native/,
  'mutation-sensitive plan check did not fail',
)

const report = {
  schema: 1,
  fixture: {
    starterSha256: sha256(await readFile(resolve(root, 'fixtures/deck-gates/starter.potx'))),
    deckSpecSha256: sha256(await readFile(resolve(root, 'fixtures/deck-gates/deck-spec.wdsf'))),
  },
  contracts: {
    exactTemplatePlanBytes: true,
    exactDeckPlanBytes: true,
    exactDisplayListBytes: true,
    exactPptxBytes: true,
    exactTopology: true,
    mutationSensitive: true,
    automaticLayoutQuality: true,
    canvasVisualContinuity: true,
  },
  quality: { native: nativeQuality, browser: browserQuality },
  topology: nativeTopology,
  hosts: Object.fromEntries(await Promise.all(['native', 'browser', 'workerd'].map(async (host) => [
    host,
    {
      templatePlan: fileFact(await readFile(resolve(output, `${host}.wdtp`))),
      deckPlan: fileFact(await readFile(resolve(output, `${host}.wdpl`))),
      pptx: fileFact(await readFile(resolve(output, `${host}.pptx`))),
      displayLists: await Promise.all(slideFiles.map(async (nativeName) => {
        const suffix = nativeName.slice('native-'.length)
        return fileFact(await readFile(resolve(output, `${host}-${suffix}`)))
      })),
      timings: { native: nativeTimings, browser: browserTimings, workerd: workerd.timings }[host],
    },
  ]))),
}
await writeFile(resolve(output, 'report.json'), `${JSON.stringify(report, null, 2)}\n`)
console.log(JSON.stringify({
  message: 'deck cross-host contracts passed',
  slideCount: nativeTopology.slideCount,
  pptxSha256: report.hosts.native.pptx.sha256,
}))

function parseWorkerdEvidence(log) {
  const line = log.split(/\r?\n/).findLast((candidate) => candidate.startsWith('DECK_GATE_WORKERD:'))
  assert(line, 'workerd deck evidence marker is absent')
  return JSON.parse(line.slice('DECK_GATE_WORKERD:'.length))
}

async function writeEvidence(directory, host, evidence) {
  await writeFile(resolve(directory, `${host}.wdtp`), Buffer.from(evidence.templatePlan, 'base64'))
  await writeFile(resolve(directory, `${host}.wdpl`), Buffer.from(evidence.plan, 'base64'))
  await writeFile(resolve(directory, `${host}.pptx`), Buffer.from(evidence.pptx, 'base64'))
  for (const [index, displayList] of evidence.slides.entries()) {
    await writeFile(
      resolve(directory, `${host}-${String(index).padStart(4, '0')}.wpdl`),
      Buffer.from(displayList, 'base64'),
    )
  }
  await writeFile(
    resolve(directory, `${host}-topology.json`),
    `${JSON.stringify(evidence.topology)}\n`,
  )
  await writeFile(
    resolve(directory, `${host}-timings.json`),
    `${JSON.stringify(evidence.timings)}\n`,
  )
}

function validateTimings(host, timings) {
  for (const name of ['planSamplesMs', 'resolveAllSamplesMs', 'exportSamplesMs']) {
    assert.equal(timings[name].length, 7, `${host} ${name} must retain seven raw samples`)
    assert(timings[name].every((sample) => Number.isFinite(sample) && sample >= 0))
  }
  assert.equal(timings.summary.coldPlanMs, timings.planSamplesMs[0])
  assert.equal(timings.summary.warmPlanP50Ms, percentile(timings.planSamplesMs.slice(1), 0.5))
  assert.equal(timings.summary.warmPlanP95Ms, percentile(timings.planSamplesMs.slice(1), 0.95))
  assert.equal(timings.summary.resolveAllP50Ms, percentile(timings.resolveAllSamplesMs, 0.5))
  assert.equal(timings.summary.resolveAllP95Ms, percentile(timings.resolveAllSamplesMs, 0.95))
  assert.equal(timings.summary.exportP50Ms, percentile(timings.exportSamplesMs, 0.5))
  assert.equal(timings.summary.exportP95Ms, percentile(timings.exportSamplesMs, 0.95))
}

function validateQuality(native, browser, topology) {
  assert.equal(native.schema, 1)
  assert.equal(native.corpus, 'autolayout-v2')
  assert.equal(native.counts.logicalSlides, 11)
  assert.equal(native.counts.physicalPages, topology.slideCount)
  assert(native.counts.flowPages >= 1, 'corpus did not produce a flow-column page')
  assert(native.counts.galleryPages >= 1, 'corpus did not produce a gallery page')
  assert(native.counts.mediaFragments >= 10, 'corpus did not retain its media set')
  assert(native.counts.tableFragments >= 2, 'corpus did not paginate its table')
  assert(Object.values(native.contracts).every((passed) => passed === true))
  assert.equal(browser.schema, 1)
  assert.equal(browser.renderedSlides, topology.slideCount)
  assert(browser.sourceElements >= 30, 'rendered slides lost semantic source elements')
  assert(browser.minimumChangedPixels >= 20, 'a rendered Canvas slide was blank')
  assert(Object.values(browser.contracts).every((passed) => passed === true))
}

function percentile(samples, quantile) {
  const sorted = [...samples].sort((left, right) => left - right)
  return sorted[Math.max(0, Math.ceil(sorted.length * quantile) - 1)]
}

async function compareAllHosts(directory, suffix) {
  const files = await Promise.all(['native', 'browser', 'workerd'].map(
    (host) => readFile(resolve(directory, `${host}.${suffix}`)).catch(() =>
      readFile(resolve(directory, `${host}-${suffix}`))),
  ))
  assertHostParity(files, suffix)
}

function assertHostParity(files, suffix) {
  assert.deepEqual(files[1], files[0], `browser ${suffix} bytes differ from native`)
  assert.deepEqual(files[2], files[0], `workerd ${suffix} bytes differ from native`)
}

function fileFact(bytes) {
  return { bytes: bytes.byteLength, sha256: sha256(bytes) }
}

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex')
}
