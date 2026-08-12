import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { arch, cpus, platform, release } from 'node:os'
import { dirname } from 'node:path'

const [input, output] = process.argv.slice(2)
assert(input && output, 'usage: extract-workerd.mjs INPUT.log OUTPUT.json')
const log = await readFile(input, 'utf8')
const marker = 'WASM_WORKER_BENCHMARK:'
const line = log.split('\n').find((candidate) => candidate.includes(marker))
assert(line, 'workerd benchmark marker is absent')
const measured = JSON.parse(line.slice(line.indexOf(marker) + marker.length))
const root = new URL('../', import.meta.url)
const fixture = await readFile(new URL('fixtures/host-adapters/minimal.potx', root))
const packageLock = JSON.parse(await readFile(new URL('package-lock.json', root), 'utf8'))
const workerdVersion = packageLock.packages['node_modules/workerd']?.version ?? 'unknown'
const report = {
  ...measured,
  generatedAt: new Date().toISOString(),
  source: { revision: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim() },
  environment: {
    hardware: { cpu: cpus()[0]?.model ?? 'unknown', logicalCpus: cpus().length, architecture: arch() },
    os: { platform: platform(), release: release() },
    runtimes: { node: process.version, workerd: workerdVersion },
  },
  configuration: {
    wasm: 'scalar',
    compression: 'deterministic DEFLATE level 6',
    outputChunkBytes: 262144,
    planCacheBytes: 33554432,
  },
  fixture: { id: measured.fixture, sha256: createHash('sha256').update(fixture).digest('hex') },
}
await mkdir(dirname(output), { recursive: true })
await writeFile(output, `${JSON.stringify(report, null, 2)}\n`)
