import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises'
import { arch, cpus, platform, release, totalmem } from 'node:os'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')
const ci = process.argv.includes('--ci')
const iterations = ci ? 10 : Number(process.env.WASMPPT_BENCH_ITERATIONS ?? 30)
assert(Number.isSafeInteger(iterations) && iterations >= 3)
const fixtureContract = JSON.parse(await readFile(resolve(root, 'benchmarks/fixtures.json'), 'utf8'))
const budgets = JSON.parse(await readFile(resolve(root, 'benchmarks/budgets.json'), 'utf8'))
const outputDirectory = resolve(root, 'target/benchmarks')
const fixtureDirectory = resolve(root, 'target/benchmark-fixtures')
await mkdir(outputDirectory, { recursive: true })
await mkdir(fixtureDirectory, { recursive: true })

exec('cargo', ['build', '--locked', '--release', '-p', 'wasmppt-native', '--examples'])
const generator = resolve(root, 'target/release/examples/write_benchmark_fixture')
const runner = resolve(root, 'target/release/examples/benchmark')
const operationRunner = resolve(root, 'target/release/examples/benchmark_live_operations')
const selected = ci
  ? fixtureContract.slideCounts.map((slides) => ['mixed', slides])
  : fixtureContract.scenarios.flatMap((scenario) => fixtureContract.slideCounts.map((slides) => [scenario, slides]))
const fixtures = []
const results = []
for (const [scenario, slides] of selected) {
  const id = `${scenario}-${slides}`
  const path = resolve(fixtureDirectory, `${id}.potx`)
  exec(generator, [scenario, String(slides), path])
  const bytes = await readFile(path)
  fixtures.push({ id, bytes: bytes.byteLength, sha256: sha256(bytes) })
  const measured = measuredProcess(runner, [path, scenario, String(slides), String(iterations)])
  const result = JSON.parse(measured.stdout.trim().split('\n').at(-1))
  result.peakResidentBytes = measured.peakResidentBytes
  result.summary = Object.fromEntries(Object.entries(result.samplesNs).map(([name, samples]) => [
    name,
    { p50Ns: percentile(samples, 0.50), p95Ns: percentile(samples, 0.95), slidesPerSecond: throughput(name, slides, percentile(samples, 0.50)) },
  ]))
  result.live.summary = Object.fromEntries(Object.entries(result.live.samplesNs).map(([name, samples]) => [
    name,
    { p50Ns: percentile(samples, 0.50), p95Ns: percentile(samples, 0.95) },
  ]))
  results.push(result)
}
const operationMeasured = measuredProcess(operationRunner, [String(iterations)])
const liveOperations = JSON.parse(operationMeasured.stdout.trim().split('\n').at(-1))
for (const operation of Object.values(liveOperations.operations)) {
  operation.summary = {
    applyDelta: {
      p50Ns: percentile(operation.applyDeltaNs, 0.50),
      p95Ns: percentile(operation.applyDeltaNs, 0.95),
    },
    inputToRenderReady: {
      p50Ns: percentile(operation.inputToRenderReadyNs, 0.50),
      p95Ns: percentile(operation.inputToRenderReadyNs, 0.95),
    },
  }
}
liveOperations.peakResidentBytes = operationMeasured.peakResidentBytes

const wasmPath = resolve(root, 'packages/wasmppt-worker/src/generated/wasmppt_wasm_bg.wasm')
const wasmBytes = (await stat(wasmPath)).size
const metafileWasmBytes = (await stat(resolve(root, 'packages/wasmppt-worker/src/generated/metafile/wasmppt_metafile_wasm_bg.wasm'))).size
const shaperWasmBytes = (await stat(resolve(root, 'packages/wasmppt-worker/src/generated/shaper/wasmppt_shaper_wasm_bg.wasm'))).size
const trackedChanges = execFileSync(
  'git',
  ['status', '--porcelain', '--untracked-files=no'],
  { cwd: root, encoding: 'utf8' },
)
  .split('\n')
  .filter(Boolean)
const generatedArtifactChanges = trackedChanges.filter((line) =>
  line.includes('packages/wasmppt-worker/src/generated/'),
)
const report = {
  schema: 2,
  generatedAt: new Date().toISOString(),
  source: {
    revision: output('git', ['rev-parse', 'HEAD']),
    dirty: trackedChanges.length !== generatedArtifactChanges.length,
    regeneratedTrackedArtifacts: generatedArtifactChanges.map((line) => line.slice(3)),
  },
  corpus: { contractSha256: sha256(await readFile(resolve(root, 'benchmarks/fixtures.json'))), fixtures },
  environment: {
    hardware: { cpu: cpus()[0]?.model ?? 'unknown', logicalCpus: cpus().length, totalMemoryBytes: totalmem(), architecture: arch() },
    os: { platform: platform(), release: release() },
    runtimes: { node: process.version, rustc: output('rustc', ['--version']) },
  },
  configuration: { profile: 'release', wasmProfile: 'wasm-release', compression: fixtureContract.compression, iterations },
  artifacts: { scalarWasmBytes: wasmBytes, metafileWasmBytes, shaperWasmBytes },
  results,
  liveOperations,
}
if (ci) enforceNativeBudget(report, budgets)
await writeFile(resolve(outputDirectory, 'native.json'), `${JSON.stringify(report, null, 2)}\n`)
console.log(`benchmark report: ${resolve(outputDirectory, 'native.json')}`)

function exec(command, args) {
  execFileSync(command, args, { cwd: root, stdio: 'inherit' })
}

function output(command, args) {
  return execFileSync(command, args, { cwd: root, encoding: 'utf8' }).trim()
}

function measuredProcess(command, args) {
  const timeArgs = platform() === 'darwin' ? ['-l', command, ...args] : ['-v', command, ...args]
  const child = spawnSync('/usr/bin/time', timeArgs, { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 })
  if (child.status !== 0) throw new Error(child.stderr || `benchmark exited ${child.status}`)
  const linux = /Maximum resident set size \(kbytes\):\s*(\d+)/.exec(child.stderr)
  const mac = /^\s*(\d+)\s+maximum resident set size/m.exec(child.stderr)
  const peakResidentBytes = linux ? Number(linux[1]) * 1024 : mac ? Number(mac[1]) : null
  if (peakResidentBytes === null) throw new Error(`cannot parse peak memory from /usr/bin/time:\n${child.stderr}`)
  return { stdout: child.stdout, peakResidentBytes }
}

function percentile(samples, quantile) {
  const sorted = samples.toSorted((left, right) => left - right)
  return sorted[Math.max(0, Math.ceil(sorted.length * quantile) - 1)]
}

function throughput(name, slides, ns) {
  const count = name === 'firstSlide' ? 1 : name === 'visibleSlides' ? Math.min(3, slides) : name === 'allSlides' || name === 'warmInjection' ? slides : 1
  return Number((count / (ns / 1e9)).toFixed(2))
}

function sha256(bytes) { return createHash('sha256').update(bytes).digest('hex') }

function enforceNativeBudget(benchmarkReport, allBudgets) {
  const budget = allBudgets.native
  const result = benchmarkReport.results.find((entry) => `${entry.scenario}-${entry.slides}` === budget.fixture)
  assert(result, `missing budget fixture ${budget.fixture}`)
  for (const [name, maximum] of Object.entries(budget.maximumP95Ns)) {
    assert(result.summary[name].p95Ns <= maximum, `${name} p95 ${result.summary[name].p95Ns}ns exceeds ${maximum}ns`)
  }
  assert(result.peakResidentBytes <= budget.maximumPeakResidentBytes)
  assert(result.generation.peakDirtyEntryBytes <= budget.maximumPeakDirtyEntryBytes)
  assert(result.generation.maximumOutputChunkBytes <= budget.maximumOutputChunkBytes)
  assert(result.zip.rawCopiedEntries >= budget.minimumRawCopiedEntries)
  assert(result.zip.inflatedEntries <= budget.maximumInflatedEntries)
  assert(benchmarkReport.artifacts.scalarWasmBytes <= allBudgets.browserScalarWasm.maximumBinaryBytes)
  assert(benchmarkReport.artifacts.metafileWasmBytes <= allBudgets.browserScalarWasm.maximumMetafileBinaryBytes)
  assert(benchmarkReport.artifacts.shaperWasmBytes <= allBudgets.browserScalarWasm.maximumShaperBinaryBytes)
  for (const [slides, limits] of Object.entries(allBudgets.nativeLive)) {
    const live = benchmarkReport.results.find((entry) => entry.scenario === 'mixed' && entry.slides === Number(slides))
    assert(live, `missing live-editing budget fixture mixed-${slides}`)
    assert(live.live.summary.applyDelta.p95Ns <= limits.maximumApplyDeltaP95Ns)
    assert(live.live.summary.inputToRenderReady.p95Ns <= limits.maximumInputToRenderReadyP95Ns)
    assert(live.live.summary.backgroundExport.p95Ns <= limits.maximumBackgroundExportP95Ns)
    assert(live.live.maximumInvalidatedSlides <= limits.maximumInvalidatedSlides)
    assert(live.live.cache.peakResidentBytes <= limits.maximumPeakResidentBytes)
    assert(live.live.cache.minimumReusedMaterializedParts >= limits.minimumReusedMaterializedParts)
  }
  for (const [name, limits] of Object.entries(allBudgets.nativeLiveOperations)) {
    const operation = benchmarkReport.liveOperations.operations[name]
    assert(operation, `missing live operation benchmark ${name}`)
    assert(operation.summary.applyDelta.p95Ns <= limits.maximumApplyDeltaP95Ns)
    assert(operation.summary.inputToRenderReady.p95Ns <= limits.maximumInputToRenderReadyP95Ns)
    assert(operation.maximumInvalidatedSlides <= limits.maximumInvalidatedSlides)
    assert(operation.maximumResidentBytes <= limits.maximumResidentBytes)
  }
}
