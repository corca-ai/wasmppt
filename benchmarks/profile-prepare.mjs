import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { Session } from 'node:inspector'
import { arch, cpus, platform } from 'node:os'
import { dirname, resolve } from 'node:path'
import { performance } from 'node:perf_hooks'
import { spawnSync } from 'node:child_process'
import { pathToFileURL } from 'node:url'
import { timingSummary } from './comparisons/equivalent-report.mjs'

const root = resolve(import.meta.dirname, '..')
const [templateArgument, reportArgument, engineArgument] = process.argv.slice(2)
assert(templateArgument && reportArgument, 'usage: node benchmarks/profile-prepare.mjs TEMPLATE REPORT [ENGINE_DIRECTORY]')
const engineDirectory = resolve(engineArgument ?? 'packages/wasmppt-worker/src/generated')
const reportPath = resolve(reportArgument)
const template = await readFile(resolve(templateArgument))
const engineBytes = await readFile(resolve(engineDirectory, 'wasmppt_wasm_bg.wasm'))
const wasm = await import(pathToFileURL(resolve(engineDirectory, 'wasmppt_wasm.js')).href)
await wasm.default({ module_or_path: engineBytes })
const iterations = 200
const warmupIterations = 20
function prepare() {
  const engine = new wasm.WasmpptEngine()
  let handle
  try {
    const start = performance.now()
    handle = engine.prepare(template)
    return performance.now() - start
  } finally {
    if (handle !== undefined) engine.release_template(handle)
    engine.free()
  }
}
for (let index = 0; index < warmupIterations; index += 1) prepare()
const samplesMs = Array.from({ length: iterations }, prepare)

// Profiling is a separate pass so inspector sampling cannot inflate the timing samples.
const inspector = new Session()
inspector.connect()
const post = (method, params = {}) => new Promise((accept, reject) => {
  inspector.post(method, params, (error, result) => error ? reject(error) : accept(result))
})
let profile
try {
  await post('Profiler.enable')
  await post('Profiler.setSamplingInterval', { interval: 100 })
  await post('Profiler.start')
  for (let index = 0; index < iterations; index += 1) prepare()
  profile = (await post('Profiler.stop')).profile
} finally {
  inspector.disconnect()
}
const nodes = new Map(profile.nodes.map((node) => [node.id, node]))
const parents = new Map(profile.nodes.flatMap((node) => (node.children ?? []).map((id) => [id, node.id])))
const hits = new Map()
for (const sample of profile.samples ?? []) {
  let id = sample
  const seen = new Set()
  while (id !== undefined) {
    // Compiler hash suffixes identify builds, not logical functions. Keep those in
    // the raw profile and aggregate the report by its readable function name.
    const name = nodes.get(id).callFrame.functionName.replace(/::h[0-9a-f]{16}(?=\[|$)/g, '')
    if (!seen.has(name)) {
      const entry = hits.get(name) ?? { name, selfSamples: 0, inclusiveSamples: 0 }
      if (id === sample) entry.selfSamples += 1
      entry.inclusiveSamples += 1
      hits.set(name, entry)
      seen.add(name)
    }
    id = parents.get(id)
  }
}
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex')
const revision = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' })
assert.equal(revision.status, 0)
const dirty = spawnSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' })
assert.equal(dirty.status, 0)
const profilePath = `${reportPath}.cpuprofile`
const report = {
  schema: 1,
  source: { revision: revision.stdout.trim(), dirty: Boolean(dirty.stdout.trim()) },
  environment: { node: process.version, platform: platform(), arch: arch(), cpu: cpus()[0].model },
  scope: 'scalar-Wasm prepare only; fresh engine per sample; initialization and disposal outside timing; separate profiler pass includes disposal',
  iterations,
  warmupIterations,
  templateSha256: sha256(template),
  engineSha256: sha256(engineBytes),
  engineDirectory,
  samplesMs,
  summary: timingSummary(samplesMs),
  profile: {
    path: profilePath,
    intervalMicroseconds: 100,
    sampleCount: profile.samples?.length ?? 0,
    functions: [...hits.values()].sort((a, b) => b.selfSamples - a.selfSamples),
  },
}
await mkdir(dirname(reportPath), { recursive: true })
await writeFile(profilePath, JSON.stringify(profile))
await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`)
console.log(JSON.stringify({ report: reportPath, summary: report.summary }))
