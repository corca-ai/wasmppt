import assert from 'node:assert/strict'
import { cp, mkdir, stat } from 'node:fs/promises'

const root = new URL('../', import.meta.url)
const source = new URL(
  'benchmarks/comparisons/pptx-browser/node_modules/pptx-browser/src/',
  root,
)
const target = new URL('target/benchmark-comparators/pptx-browser/', root)
for (const required of ['index.js', 'zip.js', 'render.js']) {
  assert((await stat(new URL(required, source))).isFile(), `pptx-browser package omits src/${required}`)
}
await mkdir(target, { recursive: true })
await cp(source, target, { recursive: true, force: true })
