import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { writeFile } from 'node:fs/promises'
import { arch, cpus, platform, release } from 'node:os'
import { performance } from 'node:perf_hooks'
import pptxgen from 'pptxgenjs'

const [slidesText, iterationsText, outputPath] = process.argv.slice(2)
const slides = Number(slidesText)
const iterations = Number(iterationsText)
assert(Number.isSafeInteger(slides) && slides > 0)
assert(Number.isSafeInteger(iterations) && iterations >= 3)
assert(outputPath)
const samplesMs = []
let bytes
for (let iteration = 0; iteration < iterations; iteration += 1) {
  const start = performance.now()
  const deck = new pptxgen()
  deck.layout = 'LAYOUT_WIDE'
  deck.author = 'wasmppt benchmark adapter'
  deck.subject = 'text-heavy generation comparison'
  deck.company = 'corca-ai'
  for (let slideIndex = 0; slideIndex < slides; slideIndex += 1) {
    const slide = deck.addSlide()
    for (let field = 0; field < 8; field += 1) {
      slide.addText(
        `Slide ${slideIndex} field ${field}: 한국어 العربية 👨🏽‍💻 ${'benchmark payload '.repeat(24)}`,
        { x: 0.5, y: 0.3 + field * 0.65, w: 11.5, h: 0.5, fontFace: 'Arial', fontSize: 10 },
      )
    }
  }
  bytes = new Uint8Array(await deck.write({ outputType: 'arraybuffer', compression: true }))
  samplesMs.push(performance.now() - start)
}
assert.deepEqual([...bytes.subarray(0, 2)], [0x50, 0x4b])
await writeFile(outputPath, bytes)
const sorted = samplesMs.toSorted((left, right) => left - right)
console.log(JSON.stringify({
  schema: 1,
  generatedAt: new Date().toISOString(),
  source: { revision: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim() },
  environment: {
    hardware: { cpu: cpus()[0]?.model ?? 'unknown', logicalCpus: cpus().length, architecture: arch() },
    os: { platform: platform(), release: release() },
    runtimes: { node: process.version },
  },
  library: { name: 'PptxGenJS', version: '4.0.1' },
  workload: 'author-new-text-heavy-deck',
  settings: { layout: 'LAYOUT_WIDE', compression: true, textBoxesPerSlide: 8 },
  semanticDifference: 'Authors a new PPTX; it does not compile or inject a POTX/POTM template.',
  slides,
  iterations,
  samplesMs,
  summary: {
    p50Ms: sorted[Math.ceil(sorted.length * 0.5) - 1],
    p95Ms: sorted[Math.ceil(sorted.length * 0.95) - 1],
  },
  correctness: {
    zipSignature: [0x50, 0x4b],
    outputBytes: bytes.byteLength,
    outputSha256: createHash('sha256').update(bytes).digest('hex'),
  },
}))
