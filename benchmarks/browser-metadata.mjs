import { performance } from 'node:perf_hooks'
import { inspectRasterImageMetadata } from '../packages/wasmppt/dist/image.js'
import { DeckSourceText } from '../packages/wasmppt/dist/deck.js'

// Header inspection should not decode the entropy-coded JPEG payload as UTF-8.
const jpeg = new Uint8Array(8 * 1024 * 1024)
jpeg.set([255, 216, 255, 192, 0, 11, 8, 0, 1, 0, 2, 1, 1, 17, 0, 255, 218, 0, 2])
const text = 'Hello 한글 😀\n'.repeat(10_000)

function measure(name, operation, inputUnits) {
  for (let index = 0; index < 5; index++) operation()
  const samples = []
  for (let index = 0; index < 30; index++) {
    const start = performance.now()
    operation()
    samples.push(performance.now() - start)
  }
  const sorted = [...samples].sort((a, b) => a - b)
  return { name, inputUnits, samples, medianMs: sorted[15], p95Ms: sorted[28] }
}

console.log(JSON.stringify({ node: process.version, platform: process.platform, arch: process.arch,
  results: [
    measure('jpeg-metadata', () => inspectRasterImageMetadata(jpeg), jpeg.byteLength),
    measure('unicode-source-offsets', () => new DeckSourceText('doc', text), text.length),
  ],
}, null, 2))
