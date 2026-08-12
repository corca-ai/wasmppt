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
const selected = ci
  ? [['mixed', 10]]
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
  results.push(result)
}

const wasmPath = resolve(root, 'packages/wasmppt-worker/src/generated/wasmppt_wasm_bg.wasm')
const wasmBytes = (await stat(wasmPath)).size
const report = {
  schema: 1,
  generatedAt: new Date().toISOString(),
  source: {
    revision: output('git', ['rev-parse', 'HEAD']),
    dirty: output('git', ['status', '--porcelain']).length > 0,
  },
  corpus: { contractSha256: sha256(await readFile(resolve(root, 'benchmarks/fixtures.json'))), fixtures },
  environment: {
    hardware: { cpu: cpus()[0]?.model ?? 'unknown', logicalCpus: cpus().length, totalMemoryBytes: totalmem(), architecture: arch() },
    os: { platform: platform(), release: release() },
    runtimes: { node: process.version, rustc: output('rustc', ['--version']) },
  },
  configuration: { profile: 'release', wasmProfile: 'wasm-release', compression: fixtureContract.compression, iterations },
  artifacts: { scalarWasmBytes: wasmBytes },
  results,
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
  const sorted = [...samples].sort((left, right) => left - right)
  return sorted[Math.max(0, Math.ceil(sorted.length * quantile) - 1)]
}

function throughput(name, slides, ns) {
  const count = name === 'firstSlide' ? 1 : name === 'visibleSlides' ? Math.min(3, slides) : name === 'allSlides' || name === 'warmInjection' ? slides : 1
  return Number((count / (ns / 1e9)).toFixed(2))
}

function sha256(bytes) { return createHash('sha256').update(bytes).digest('hex') }

function enforceNativeBudget(report, allBudgets) {
  const budget = allBudgets.native
  const result = report.results.find((entry) => `${entry.scenario}-${entry.slides}` === budget.fixture)
  assert(result, `missing budget fixture ${budget.fixture}`)
  for (const [name, maximum] of Object.entries(budget.maximumP95Ns)) {
    assert(result.summary[name].p95Ns <= maximum, `${name} p95 ${result.summary[name].p95Ns}ns exceeds ${maximum}ns`)
  }
  assert(result.peakResidentBytes <= budget.maximumPeakResidentBytes)
  assert(result.zip.rawCopiedEntries >= budget.minimumRawCopiedEntries)
  assert(result.zip.inflatedEntries <= budget.maximumInflatedEntries)
  assert(report.artifacts.scalarWasmBytes <= allBudgets.browserScalarWasm.maximumBinaryBytes)
}
