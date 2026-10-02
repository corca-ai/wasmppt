import assert from 'node:assert/strict'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { performance } from 'node:perf_hooks'
import { textSlide, textWorkload } from './text-workload.mjs'

const root = resolve(import.meta.dirname, '../..')
const [templatePath, outputDirectory, iterationsText] = process.argv.slice(2)
const iterations = Number(iterationsText)
assert(Number.isSafeInteger(iterations) && iterations >= 3)
await mkdir(outputDirectory, { recursive: true })
const template = await readFile(templatePath)
const inputs = Array.from({ length: textWorkload.slides }, (_, slide) => textSlide(slide))
const fields = inputs.flat()
const ids = fields.map((field) => field.binding)
const values = fields.map((field) => field.text)
let start = performance.now()
const wasm = await import('../../packages/wasmppt-worker/src/generated/wasmppt_wasm.js')
await wasm.default({ module_or_path: await readFile(join(root, 'packages/wasmppt-worker/src/generated/wasmppt_wasm_bg.wasm')) })
const wasmInitializationMs = performance.now() - start
start = performance.now()
const { authorTextDeck, canonicalizeNotesMasterOrder } = await import('./pptxgenjs/equivalent.mjs')
const comparatorImportMs = performance.now() - start

function generate(engine, handle) {
  const cursor = engine.generate_text(handle, ids, values)
  try {
    const chunks = []
    while (!engine.generation_done(cursor)) chunks.push(engine.generation_pull(cursor, 65536))
    return Buffer.concat(chunks)
  } finally {
    engine.release_generation(cursor)
  }
}

const samples = []
const warmEngine = new wasm.WasmpptEngine()
start = performance.now()
const warmHandle = warmEngine.prepare(template)
const warmPreparationMs = performance.now() - start
try {
  for (let iteration = 0; iteration < iterations; iteration += 1) {
    // Alternate execution order to expose both libraries to comparable process scheduling.
    const order = iteration % 2 === 0 ? ['cold', 'warm', 'author'] : ['author', 'warm', 'cold']
    for (const phase of order) {
      let bytes
      let preparationMs
      let rawBytes
      let rawDurationMs
      start = performance.now()
      if (phase === 'author') {
        rawBytes = await authorTextDeck(false, inputs)
        rawDurationMs = performance.now() - start
        bytes = await canonicalizeNotesMasterOrder(rawBytes)
      }
      else if (phase === 'warm') bytes = generate(warmEngine, warmHandle)
      else {
        const engine = new wasm.WasmpptEngine()
        let handle
        try {
          handle = engine.prepare(template)
          preparationMs = performance.now() - start
          bytes = generate(engine, handle)
        } finally {
          if (handle !== undefined) engine.release_template(handle)
          engine.free()
        }
      }
      const durationMs = performance.now() - start
      const output = join(outputDirectory, `${phase}-${iteration}.pptx`)
      await writeFile(output, bytes)
      samples.push({ phase, iteration, durationMs, preparationMs, output })
      if (rawBytes) {
        const rawOutput = join(outputDirectory, `raw-author-${iteration}.pptx`)
        await writeFile(rawOutput, rawBytes)
        samples.push({ phase: 'raw-author', iteration, durationMs: rawDurationMs, output: rawOutput })
      }
    }
  }
} finally {
  warmEngine.release_template(warmHandle)
  warmEngine.free()
}
console.log(JSON.stringify({ engineVersion: wasm.engine_version(), wasmInitializationMs, comparatorImportMs, warmPreparationMs, samples }))
