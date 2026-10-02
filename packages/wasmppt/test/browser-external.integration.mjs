import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { createServer } from 'node:http'
import { basename, join, resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { chromium } from 'playwright'
import { externalSlideFailures, externalVisualCases } from '../../../scripts/external-visual.mjs'

const root = resolve(import.meta.dirname, '../../..')
const corpus = JSON.parse(await readFile(join(root, 'fixtures/corpus.json'), 'utf8'))
const renderCorpus = JSON.parse(await readFile(join(root, 'fixtures/render/corpus.json'), 'utf8'))
const cases = externalVisualCases(corpus, renderCorpus)
const corpusDirectory = resolve(process.env.WASMPPT_EXTERNAL_CORPUS_DIR ?? join(root, 'target/corpus'))
const outputDirectory = join(root, 'target/visual-report')
await mkdir(outputDirectory, { recursive: true })
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex')
const report = {
  schema: 1,
  evidence: 'external-pixel-smoke',
  generatedBaselineReport: 'report.json',
  fidelity: 'Smoke assertions and repeatability only; no Office reference comparison.',
  sourceRevision: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
  sourceDirty: execFileSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' }).trim().length > 0,
  engine: 'chromium-scalar-wasm-module-worker-canvas2d',
  fontPolicy: 'browser-system-fonts',
  nodeVersion: process.version,
  cases: cases.map((entry) => Object.assign({}, entry, { actualSha256: null, results: [], failures: [] })),
  failures: [],
  passed: false,
}
const fixtures = new Map()
const server = createServer(async (request, response) => {
  const path = new URL(request.url, 'http://127.0.0.1').pathname
  if (path === '/') {
    response.writeHead(200, { 'content-type': 'text/html' })
    response.end('<!doctype html><title>wasmppt external visual smoke</title>')
    return
  }
  if (path === '/favicon.ico') {
    response.writeHead(204).end()
    return
  }
  const fixture = fixtures.get(path)
  if (fixture !== undefined) {
    response.writeHead(200, { 'content-type': 'application/octet-stream' }).end(fixture)
    return
  }
  if (!/^\/packages\/(?:wasmppt\/dist|wasmppt-worker\/src\/generated)\/[\w/-]+\.(?:js|wasm)$/.test(path)) {
    response.writeHead(404).end('not found')
    return
  }
  try {
    const bytes = await readFile(join(root, path))
    response.writeHead(200, { 'content-type': path.endsWith('.wasm') ? 'application/wasm' : 'text/javascript' }).end(bytes)
  } catch {
    response.writeHead(404).end('not found')
  }
})

let browser
try {
  // Recheck bytes at execution time, even when the fetch helper has already verified them.
  for (const entry of report.cases) {
    const path = join(corpusDirectory, basename(new URL(entry.fixture.url).pathname))
    try {
      const bytes = await readFile(path)
      entry.actualSha256 = sha256(bytes)
      assert.equal(entry.actualSha256, entry.fixture.sha256, `${entry.fixtureId} fixture hash mismatch`)
      fixtures.set(`/${entry.fixtureId}.pptx`, bytes)
    } catch (error) {
      entry.failures.push(error.message)
    }
  }
  assert(report.cases.every((entry) => entry.failures.length === 0), 'external corpus missing or changed; run scripts/fetch-corpus.mjs')
  report.scalarWasmSha256 = sha256(await readFile(join(root, 'packages/wasmppt-worker/src/generated/wasmppt_wasm_bg.wasm')))
  await new Promise((resolvePromise, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', resolvePromise)
  })
  browser = await chromium.launch(process.env.CI ? { headless: true } : { channel: 'chrome', headless: true })
  report.chromiumVersion = browser.version()
  const origin = `http://127.0.0.1:${server.address().port}`
  for (const entry of report.cases) {
    for (const declaration of entry.slides) {
      const page = await browser.newPage({ viewport: entry.viewport, deviceScaleFactor: 1 })
      const failures = []
      page.on('pageerror', (error) => failures.push(error.message))
      page.on('console', (message) => {
        if (message.type() === 'error') failures.push(message.text())
      })
      const result = { slideIndex: declaration.slideIndex, failures, passed: false }
      entry.results.push(result)
      const timeout = new AbortController()
      try {
        await page.goto(origin)
        const actual = await Promise.race([
          page.evaluate(async ({ fixtureId, slide, viewport }) => {
            const { connectWasmpptBrowserWorker } = await import('/packages/wasmppt/dist/browser-worker-client.js')
            const { CanvasDisplayListRenderer, decodeDisplayList, decodeRasterImage, decodeSvgImage } = await import('/packages/wasmppt/dist/canvas.js')
            const client = await connectWasmpptBrowserWorker(new Worker('/packages/wasmppt/dist/browser-worker.js', { type: 'module' }))
            const renderer = new CanvasDisplayListRenderer()
            const signal = AbortSignal.timeout(25_000)
            let presentation
            try {
              const response = await fetch(`/${fixtureId}.pptx`, { signal })
              if (!response.ok) throw new Error(`fixture HTTP ${response.status}`)
              presentation = await client.openPresentation(await response.arrayBuffer(), { signal })
              const bytes = await client.resolveSlide(presentation.handle, slide.slideIndex, { signal })
              const scene = decodeDisplayList(bytes)
              const canvas = document.createElement('canvas')
              canvas.id = 'actual'
              canvas.width = viewport.width
              canvas.height = viewport.height
              document.body.append(canvas)
              const resourceFailures = []
              const options = {
                signal,
                imageResolver: async (image, resourceSignal) => {
                  try {
                    if (!image.partName) throw new Error('image has no package part')
                    if (/\.(?:emf|wmf)$/i.test(image.partName)) {
                      return await decodeSvgImage(await client.presentationMetafileSvg(presentation.handle, image.partName, { signal: resourceSignal }), resourceSignal)
                    }
                    const resourceBytes = await client.presentationResource(presentation.handle, image.partName, { signal: resourceSignal })
                    return /\.svg$/i.test(image.partName) ? await decodeSvgImage(resourceBytes, resourceSignal) : await decodeRasterImage(resourceBytes, {}, resourceSignal)
                  } catch (error) {
                    resourceFailures.push(`${image.partName}: ${error.message}`)
                    throw error
                  }
                },
              }
              const context = canvas.getContext('2d', { alpha: false })
              const telemetry = await renderer.render(scene, context, options)
              const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data
              let nonBackgroundPixels = 0
              for (let offset = 0; offset < pixels.length; offset += 4) {
                if (slide.background.some((byte, index) => pixels[offset + index] !== byte)) nonBackgroundPixels += 1
              }
              const repeated = document.createElement('canvas')
              repeated.width = canvas.width
              repeated.height = canvas.height
              const repeatedContext = repeated.getContext('2d', { alpha: false })
              await renderer.render(scene, repeatedContext, options)
              const [displayListSha256, pixelSha256, repeatedPixelSha256] = await Promise.all([
                bytes, pixels, repeatedContext.getImageData(0, 0, canvas.width, canvas.height).data,
              ].map(async (input) => [...new Uint8Array(await crypto.subtle.digest('SHA-256', input))]
                .map((byte) => byte.toString(16).padStart(2, '0')).join('')))
              const bounds = scene.commands.flatMap((command) => 'bounds' in command
                ? [command.bounds]
                : typeof command.transform === 'object' ? [command.transform.bounds] : [])
              return {
                slideCount: presentation.slideCount,
                width: scene.width,
                height: scene.height,
                commandCount: scene.commands.length,
                commandKinds: Object.fromEntries([...new Set(scene.commands.map((command) => command.kind))]
                  .map((kind) => [kind, scene.commands.filter((command) => command.kind === kind).length])),
                bounds,
                finiteBounds: bounds.length > 0 && bounds.every((rect) => Object.values(rect).every(Number.isFinite) && rect.width >= 0 && rect.height >= 0),
                diagnostics: scene.diagnostics,
                diagnosticCodes: scene.diagnostics.map((diagnostic) => diagnostic.code),
                displayListVersion: scene.version,
                displayListSha256,
                pixelSha256,
                repeatedPixelSha256,
                nonBackgroundPixels,
                resourceFailures,
                telemetry,
              }
            } finally {
              renderer.clear()
              try {
                if (presentation) await client.releasePresentation(presentation.handle)
              } finally {
                client.terminate()
              }
            }
          }, { fixtureId: entry.fixtureId, slide: declaration, viewport: entry.viewport }),
          delay(30_000, undefined, { signal: timeout.signal }).then(() => { throw new Error('external slide render exceeded 30 seconds') }),
        ])
        Object.assign(result, actual)
        failures.push(...externalSlideFailures(entry, declaration, actual))
        result.actual = `${entry.fixtureId}-slide-${declaration.slideIndex + 1}-actual.png`
        const screenshot = await page.locator('#actual').screenshot({ path: join(outputDirectory, result.actual) })
        result.pngSha256 = sha256(screenshot)
        result.passed = failures.length === 0
        console.log(`${entry.fixtureId} slide ${declaration.slideIndex}: ${actual.commandCount} commands, ${actual.nonBackgroundPixels} non-background pixels, passed=${result.passed}`)
      } catch (error) {
        failures.push(error.message)
      } finally {
        timeout.abort()
        await page.close()
      }
    }
  }
  for (const entry of report.cases) {
    entry.passed = entry.failures.length === 0 && entry.results.length === entry.slides.length && entry.results.every((slide) => slide.passed)
  }
  report.passed = report.cases.every((entry) => entry.passed)
  assert(report.passed, 'external visual smoke failed; see target/visual-report/external-report.json')
} catch (error) {
  report.failures.push(error.message)
  throw error
} finally {
  await writeFile(join(outputDirectory, 'external-report.json'), `${JSON.stringify(report, null, 2)}\n`)
  await browser?.close()
  if (server.listening) await new Promise((resolvePromise) => server.close(resolvePromise))
}
