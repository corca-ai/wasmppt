import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { access, mkdir, readFile, writeFile } from 'node:fs/promises'
import { createServer } from 'node:http'
import { arch, cpus, platform, release } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { chromium } from 'playwright'

const packageDirectory = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const workspaceDirectory = resolve(packageDirectory, '../..')
const generatedDirectory = join(workspaceDirectory, 'packages/wasmppt-worker/src/generated')
const pptxBrowserDirectory = join(
  workspaceDirectory,
  'target/benchmark-comparators/pptx-browser',
)
const performanceBudgets = JSON.parse(
  await readFile(join(workspaceDirectory, 'benchmarks/budgets.json'), 'utf8'),
)
const renderCorpus = JSON.parse(
  await readFile(join(workspaceDirectory, 'fixtures/render/corpus.json'), 'utf8'),
)
const parityPayloadHex = (
  await readFile(join(workspaceDirectory, 'fixtures/host-adapters/parity.wppd.hex'), 'utf8')
).trim()
const featureRegions = Object.fromEntries(
  renderCorpus.presentations[0].features.map((feature) => [feature.id, feature.region]),
)
const fidelityFeatureDefinitions = [
  { id: 'shape-autofit', slideIndex: 2, structureIndex: 2 },
  { id: 'normal-autofit', slideIndex: 2, structureIndex: 3 },
  { id: 'unicode-wrap', slideIndex: 2, structureIndex: 4 },
  { id: 'paragraph-metrics', slideIndex: 2, structureIndex: 5 },
  { id: 'columns', slideIndex: 2, structureIndex: 6 },
  { id: 'text-effects', slideIndex: 2, structureIndex: 7 },
]

function structuralTextPassed(structure) {
  return structure !== undefined && structure.lineCount > 0 && structure.runCount > 0 &&
    structure.sourceRangesValid && structure.bounds?.width > 0 && structure.bounds?.height > 0
}

const benchmarkFixtureDirectory = join(workspaceDirectory, 'target/benchmark-fixtures')
await mkdir(benchmarkFixtureDirectory, { recursive: true })
for (const slides of [10, 50, 200]) {
  const fixture = join(benchmarkFixtureDirectory, `mixed-${slides}.potx`)
  try {
    await access(fixture)
  } catch {
    execFileSync('cargo', [
      'run',
      '--quiet',
      '--locked',
      '-p',
      'wasmppt-native',
      '--example',
      'write_benchmark_fixture',
      '--',
      'mixed',
      String(slides),
      fixture,
    ], { cwd: workspaceDirectory, stdio: 'inherit' })
  }
}

const routes = new Map([
  ['/dist/worker-client.js', [join(packageDirectory, 'dist/worker-client.js'), 'text/javascript']],
  ['/dist/injection.js', [join(packageDirectory, 'dist/injection.js'), 'text/javascript']],
  ['/dist/protocol.js', [join(packageDirectory, 'dist/protocol.js'), 'text/javascript']],
  ['/dist/error.js', [join(packageDirectory, 'dist/error.js'), 'text/javascript']],
  ['/dist/canvas.js', [join(packageDirectory, 'dist/canvas.js'), 'text/javascript']],
  ['/dist/dom-svg.js', [join(packageDirectory, 'dist/dom-svg.js'), 'text/javascript']],
  ['/dist/shaper.js', [join(packageDirectory, 'dist/shaper.js'), 'text/javascript']],
  [
    '/dist/worker-runtime.js',
    [join(packageDirectory, 'dist/worker-runtime.js'), 'text/javascript'],
  ],
  [
    '/wasm/wasmppt_wasm.js',
    [join(generatedDirectory, 'wasmppt_wasm.js'), 'text/javascript'],
  ],
  [
    '/wasm/wasmppt_wasm_bg.wasm',
    [join(generatedDirectory, 'wasmppt_wasm_bg.wasm'), 'application/wasm'],
  ],
  [
    '/wasmppt-worker/src/generated/wasmppt_wasm.js',
    [join(generatedDirectory, 'wasmppt_wasm.js'), 'text/javascript'],
  ],
  [
    '/wasmppt-worker/src/generated/wasmppt_wasm_bg.wasm',
    [join(generatedDirectory, 'wasmppt_wasm_bg.wasm'), 'application/wasm'],
  ],
  [
    '/wasmppt-worker/src/generated/metafile/wasmppt_metafile_wasm.js',
    [join(generatedDirectory, 'metafile/wasmppt_metafile_wasm.js'), 'text/javascript'],
  ],
  [
    '/wasmppt-worker/src/generated/metafile/wasmppt_metafile_wasm_bg.wasm',
    [
      join(generatedDirectory, 'metafile/wasmppt_metafile_wasm_bg.wasm'),
      'application/wasm',
    ],
  ],
  [
    '/wasm/metafile/wasmppt_metafile_wasm.js',
    [join(generatedDirectory, 'metafile/wasmppt_metafile_wasm.js'), 'text/javascript'],
  ],
  [
    '/wasm/metafile/wasmppt_metafile_wasm_bg.wasm',
    [join(generatedDirectory, 'metafile/wasmppt_metafile_wasm_bg.wasm'), 'application/wasm'],
  ],
  [
    '/wasm/shaper/wasmppt_shaper_wasm.js',
    [join(generatedDirectory, 'shaper/wasmppt_shaper_wasm.js'), 'text/javascript'],
  ],
  [
    '/wasm/shaper/wasmppt_shaper_wasm_bg.wasm',
    [join(generatedDirectory, 'shaper/wasmppt_shaper_wasm_bg.wasm'), 'application/wasm'],
  ],
  [
    '/font.ttf',
    [join(workspaceDirectory, 'node_modules/katex/dist/fonts/KaTeX_Main-Regular.ttf'), 'font/ttf'],
  ],
  [
    '/fixture.potx',
    [join(workspaceDirectory, 'fixtures/host-adapters/minimal.potx'), 'application/octet-stream'],
  ],
  [
    '/parity.wppd.hex',
    [join(workspaceDirectory, 'fixtures/host-adapters/parity.wppd.hex'), 'text/plain'],
  ],
  [
    '/render-fixture.pptx',
    [join(workspaceDirectory, 'fixtures/render/basic.pptx'), 'application/octet-stream'],
  ],
  [
    '/deck-gate-starter.potx',
    [join(workspaceDirectory, 'fixtures/deck-gates/starter.potx'), 'application/octet-stream'],
  ],
  [
    '/deck-gate-spec.wdsf',
    [join(workspaceDirectory, 'fixtures/deck-gates/deck-spec.wdsf'), 'application/octet-stream'],
  ],
  [
    '/deck-gate-atomic-overflow.wdsf',
    [join(workspaceDirectory, 'fixtures/deck-gates/atomic-overflow.wdsf'), 'application/octet-stream'],
  ],
  [
    '/competitor-fixture.pptx',
    [join(workspaceDirectory, 'target/benchmarks/pptxgenjs-text-10.pptx'), 'application/octet-stream'],
  ],
])
for (const slides of [10, 50, 200]) {
  routes.set(`/benchmark-mixed-${slides}.potx`, [
    join(benchmarkFixtureDirectory, `mixed-${slides}.potx`),
    'application/octet-stream',
  ])
}

const server = createServer(async (request, response) => {
  if (request.url === '/') {
    response.writeHead(200, { 'content-type': 'text/html' })
    response.end('<!doctype html><title>wasmppt browser host integration</title>')
    return
  }
  if (request.url === '/favicon.ico') {
    response.writeHead(204)
    response.end()
    return
  }
  if (request.url?.startsWith('/competitors/pptx-browser/')) {
    const relative = request.url.slice('/competitors/pptx-browser/'.length)
    if (!relative || relative.includes('..') || !relative.endsWith('.js')) {
      response.writeHead(404)
      response.end('not found')
      return
    }
    try {
      const source = await readFile(join(pptxBrowserDirectory, relative))
      response.writeHead(200, { 'content-type': 'text/javascript' })
      response.end(source)
    } catch (error) {
      if (!response.headersSent) {
        response.writeHead(500)
        response.end(error instanceof Error ? error.message : String(error))
      }
    }
    return
  }
  if (request.url?.startsWith('/dist/')) {
    const relative = request.url.slice('/dist/'.length)
    if (!relative || relative.includes('..') || !relative.endsWith('.js')) {
      response.writeHead(404)
      response.end('not found')
      return
    }
    try {
      const source = await readFile(join(packageDirectory, 'dist', relative))
      response.writeHead(200, { 'content-type': 'text/javascript' })
      response.end(source)
    } catch {
      response.writeHead(404)
      response.end('not found')
    }
    return
  }
  const route = routes.get(request.url ?? '')
  if (route === undefined) {
    response.writeHead(404)
    response.end('not found')
    return
  }
  try {
    const source = await readFile(route[0])
    response.writeHead(200, { 'content-type': route[1] })
    response.end(source)
  } catch (error) {
    if (!response.headersSent) {
      response.writeHead(500)
      response.end(error instanceof Error ? error.message : String(error))
    }
  }
})

await new Promise((resolvePromise, reject) => {
  server.once('error', reject)
  server.listen(0, '127.0.0.1', resolvePromise)
})

let browser
try {
  const address = server.address()
  assert(address !== null && typeof address === 'object')
  const launchOptions = process.env.CI ? { headless: true } : { channel: 'chrome', headless: true }
  browser = await chromium.launch(launchOptions)
  const page = await browser.newPage()
  const errors = []
  const competitorWarnings = []
  page.on('console', (message) => {
    if (message.text().includes('Error rendering shape:')) competitorWarnings.push(message.text())
    else console.log(`browser console: ${message.text()}`)
  })
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto(`http://127.0.0.1:${address.port}/`)
  const result = await page.evaluate(async (smartartRegion) => {
    const { connectWasmpptBrowserWorker } = await import('/dist/browser-worker-client.js')
    const { encodeInjectionData } = await import('/dist/injection.js')
    const {
      CanvasDisplayListRenderer,
      FontResolver,
      VirtualizedCanvasViewer,
      decodeRasterImage,
      decodeSvgImage,
      decodeDisplayList,
      renderOffscreenThumbnail,
      wrapText,
    } = await import('/dist/canvas.js')
    const { DomSvgRenderer, VirtualizedDomViewer } = await import('/dist/dom-svg.js')
    const { serializeDeckSessionToHtml, serializeOfflineHtmlDocument } = await import('/dist/offline.js')
    const { WasmFontShaper } = await import('/dist/shaper.js')
    const shaperModule = await import('/wasm/shaper/wasmppt_shaper_wasm.js')
    await shaperModule.default({
      module_or_path: new URL('/wasm/shaper/wasmppt_shaper_wasm_bg.wasm', location.href),
    })
    const exactFontBytes = new Uint8Array(await fetch('/font.ttf').then((response) => response.arrayBuffer()))
    const exactShaper = new WasmFontShaper(shaperModule)
    const exactShapeFirst = await exactShaper.shape({
      fontBytes: exactFontBytes,
      text: 'office',
      direction: 'ltr',
      language: 'en-US',
      script: 'Latn',
      features: ['liga'],
    })
    const exactShapeSecond = await exactShaper.shape({
      fontBytes: exactFontBytes,
      text: 'office',
      direction: 'ltr',
      language: 'en-US',
      script: 'Latn',
      features: ['liga'],
    })
    const exactShapeWithoutLigatures = await exactShaper.shape({
      fontBytes: exactFontBytes,
      text: 'office',
      direction: 'ltr',
      language: 'en-US',
      script: 'Latn',
      features: ['-liga'],
    })
    const exactBreakTokens = await exactShaper.breakText('alpha beta\n日本語')
    const worker = new Worker('/dist/browser-worker.js', { type: 'module' })
    const client = await connectWasmpptBrowserWorker(worker, 10_000)
    const template = await fetch('/fixture.potx').then((response) => response.arrayBuffer())
    const expectedPayloadHex = (await fetch('/parity.wppd.hex').then((response) => response.text()))
      .trim()
    const encodedPayloadHex = [...new Uint8Array(encodeInjectionData({}))]
      .map((byte) => byte.toString(16).padStart(2, '0'))
      .join('')
    if (encodedPayloadHex !== expectedPayloadHex) throw new Error('browser WPPD parity payload drift')
    const coldPrepareStart = performance.now()
    const prepare = client.prepare(template)
    const transferredByteLength = template.byteLength
    const prepared = await prepare
    const coldPrepareMs = performance.now() - coldPrepareStart
    const warmInjectionSamplesMs = []
    let output
    for (let iteration = 0; iteration < 15; iteration += 1) {
      const start = performance.now()
      output = new Uint8Array(await client.generate(prepared.handle))
      warmInjectionSamplesMs.push(performance.now() - start)
    }
    const topologySession = await client.createLiveSession(prepared.handle, {})
    const topologyUpdate = await client.applyLiveDelta(topologySession.handle, 0, {
      slides: { 'ppt/slides/slide1.xml': 2 },
    })
    await client.releaseLiveSession(topologySession.handle)
    await client.release(prepared.handle)
    // Serialized into the browser by Playwright; it cannot reference a Node-side helper.
    // oxlint-disable-next-line unicorn/consistent-function-scoping
    const encodeBase64 = (bytes) => {
      let binary = ''
      const view = new Uint8Array(bytes)
      for (let offset = 0; offset < view.byteLength; offset += 0x8000) {
        binary += String.fromCharCode(...view.subarray(offset, offset + 0x8000))
      }
      return btoa(binary)
    }
    const deckTemplateSource = await fetch('/deck-gate-starter.potx')
      .then((response) => response.arrayBuffer())
    const deckSpecSource = await fetch('/deck-gate-spec.wdsf')
      .then((response) => response.arrayBuffer())
    const deckPlanSamplesMs = []
    const deckResolveAllSamplesMs = []
    const deckExportSamplesMs = []
    let deckTemplate
    let deckSession
    let deckSlides
    let deckPages
    let deckPptx
    let deckVisualQuality
    for (let iteration = 0; iteration < 7; iteration += 1) {
      const deckStarted = performance.now()
      const measuredTemplate = await client.prepareDeckTemplate(deckTemplateSource.slice(0))
      const measuredSession = await client.createDeckSession(
        measuredTemplate.handle,
        deckSpecSource.slice(0),
      )
      deckPlanSamplesMs.push(performance.now() - deckStarted)
      const measuredSlides = []
      const measuredPages = []
      const deckResolveStarted = performance.now()
      for (let slideIndex = 0; slideIndex < measuredSession.slideCount; slideIndex += 1) {
        const resolved = await client.resolveDeckSlide(
          measuredSession.handle,
          measuredSession.revision,
          slideIndex,
        )
        measuredSlides.push(encodeBase64(resolved.displayList))
        measuredPages.push({
          slideIndex,
          pageId: resolved.page.pageId,
          logicalSlideId: resolved.page.logicalSlideId,
          hidden: resolved.page.hidden,
          continuationOrdinal: resolved.page.continuationOrdinal,
          continuationTotal: resolved.page.continuationTotal,
          continuationLabel: resolved.page.continuationLabel ?? null,
        })
      }
      deckResolveAllSamplesMs.push(performance.now() - deckResolveStarted)
      const deckExportStarted = performance.now()
      const measuredPptx = await client.generateDeck(
        measuredSession.handle,
        measuredSession.revision,
      )
      deckExportSamplesMs.push(performance.now() - deckExportStarted)
      if (iteration === 6) {
        deckTemplate = measuredTemplate
        deckSession = measuredSession
        deckSlides = measuredSlides
        deckPages = measuredPages
        deckPptx = measuredPptx
        const renderer = new CanvasDisplayListRenderer(4 * 1024 * 1024)
        let sourceElements = 0
        let minimumChangedPixels = Number.POSITIVE_INFINITY
        const decodedImageSizes = new Map()
        const qualityImageCommands = []
        for (const [slideIndex, encoded] of measuredSlides.entries()) {
          const binary = atob(encoded)
          const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0))
          const scene = decodeDisplayList(bytes)
          const sourceSemantics = scene.semantics.filter((semantic) => semantic.source !== undefined)
          sourceElements += sourceSemantics.length
          if (sourceSemantics.length === 0) {
            throw new Error(`deck slide ${slideIndex} lost semantic source elements`)
          }
          for (const semantic of sourceSemantics) {
            if (semantic.bounds.width <= 0 || semantic.bounds.height <= 0) {
              throw new Error(`deck slide ${slideIndex} has empty semantic bounds`)
            }
            if (semantic.bounds.x < 0 || semantic.bounds.y < 0 ||
                semantic.bounds.x + semantic.bounds.width > scene.width ||
                semantic.bounds.y + semantic.bounds.height > scene.height) {
              throw new Error(`deck slide ${slideIndex} has out-of-page semantic bounds`)
            }
            if (semantic.alternativeText?.startsWith('gate-media|') === true) {
              const [marker, mediaCase, item, expectedWidth, expectedHeight, extra] =
                semantic.alternativeText.split('|')
              if (marker !== 'gate-media' || extra !== undefined) {
                throw new Error(`deck slide ${slideIndex} has malformed gate media metadata`)
              }
              const commands = scene.commands.slice(
                semantic.firstCommand,
                semantic.firstCommand + semantic.commandCount,
              ).filter((command) => command.kind === 'draw-image')
              if (commands.length !== 1) {
                throw new Error(`deck slide ${slideIndex} gate media did not emit one image`)
              }
              const command = commands[0]
              const partName = scene.images[command.resource]?.partName
              if (partName === undefined) {
                throw new Error(`deck slide ${slideIndex} gate media lost its resource part`)
              }
              qualityImageCommands.push({
                semanticId: semantic.source.semanticId,
                case: mediaCase,
                item: Number(item),
                pageIndex: slideIndex,
                expectedWidth: Number(expectedWidth),
                expectedHeight: Number(expectedHeight),
                frameWidth: command.transform.bounds.width,
                frameHeight: command.transform.bounds.height,
                crop: command.crop,
                partName,
              })
            }
          }
          const canvas = new OffscreenCanvas(320, 180)
          const context = canvas.getContext('2d', { alpha: false })
          await renderer.render(scene, context, {
            imageCacheKey: async (image, signal) => {
              if (image.partName === undefined) throw new Error('deck image part is missing')
              return client.deckSessionResourceFingerprint(
                measuredSession.handle, measuredSession.revision, image.partName, { signal },
              )
            },
            imageResolver: async (image, signal) => {
              if (image.partName === undefined) throw new Error('deck image part is missing')
              const partName = image.partName
              const resource = await client.deckSessionResource(
                measuredSession.handle, measuredSession.revision, partName, { signal },
              )
              const resourceBytes = new Uint8Array(resource.bytes)
              const prefix = new TextDecoder().decode(resourceBytes.subarray(0, 100)).trimStart()
              const decoded = prefix.startsWith('<svg')
                ? decodeSvgImage(resourceBytes, signal)
                : decodeRasterImage(resourceBytes, {}, signal)
              let decodedImage
              try {
                decodedImage = await decoded
              } catch (error) {
                throw new Error(`deck image ${partName} could not be decoded`, { cause: error })
              }
              decodedImageSizes.set(partName, {
                width: decodedImage.source.width,
                height: decodedImage.source.height,
              })
              return decodedImage
            },
          })
          const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data
          const background = pixels.slice(0, 4)
          let changedPixels = 0
          for (let offset = 4; offset < pixels.length; offset += 4) {
            if (
              pixels[offset] !== background[0] || pixels[offset + 1] !== background[1] ||
              pixels[offset + 2] !== background[2] || pixels[offset + 3] !== background[3]
            ) changedPixels += 1
          }
          if (changedPixels < 20) throw new Error(`deck slide ${slideIndex} rendered blank`)
          minimumChangedPixels = Math.min(minimumChangedPixels, changedPixels)
        }
        const qualityImages = qualityImageCommands.map((entry) => {
          const source = decodedImageSizes.get(entry.partName)
          if (source === undefined) throw new Error(`gate media ${entry.case} was not decoded`)
          if (source.width !== entry.expectedWidth || source.height !== entry.expectedHeight) {
            throw new Error(
              `gate media ${entry.case} decoded ${source.width}x${source.height} instead of ` +
              `${entry.expectedWidth}x${entry.expectedHeight}`,
            )
          }
          const [left, top, right, bottom] = entry.crop
          const remainingX = 100_000 - left - right
          const remainingY = 100_000 - top - bottom
          const aspectLeft = entry.frameWidth * source.height * remainingY
          const aspectRight = entry.frameHeight * source.width * remainingX
          const aspectErrorPerMillion = Math.floor(
            Math.abs(aspectLeft - aspectRight) * 1_000_000 /
              Math.max(aspectLeft, aspectRight, 1),
          )
          const cropLossPerMille = Math.floor(
            (10_000_000_000 - remainingX * remainingY) * 1_000 / 10_000_000_000,
          )
          if (aspectErrorPerMillion > 25) {
            throw new Error(`gate media ${entry.case} has ${aspectErrorPerMillion} ppm distortion`)
          }
          if (cropLossPerMille > 300) {
            throw new Error(`gate media ${entry.case} loses ${cropLossPerMille} per mille to crop`)
          }
          return {
            semanticId: entry.semanticId,
            case: entry.case,
            item: entry.item,
            pageIndex: entry.pageIndex,
            expectedWidth: entry.expectedWidth,
            expectedHeight: entry.expectedHeight,
            sourceWidth: source.width,
            sourceHeight: source.height,
            frameWidth: entry.frameWidth,
            frameHeight: entry.frameHeight,
            aspectErrorPerMillion,
            cropLossPerMille,
          }
        }).toSorted((left, right) => left.semanticId.localeCompare(right.semanticId))
        deckVisualQuality = {
          schema: 2,
          renderedSlides: measuredSlides.length,
          sourceElements,
          minimumChangedPixels,
          images: qualityImages,
          contracts: {
            canvasBounds: true,
            nonBlankSlides: true,
            aspectFidelity: true,
            boundedCrop: true,
            displayAxisFidelity: true,
          },
        }
      } else {
        await client.releaseDeckSession(measuredSession.handle)
        await client.releaseDeckTemplate(measuredTemplate.handle)
      }
    }
    // Serialized into the browser by Playwright; it cannot reference a Node-side helper.
    // oxlint-disable-next-line unicorn/consistent-function-scoping
    const percentile = (samples, quantile) => {
      const sorted = samples.toSorted((left, right) => left - right)
      return sorted[Math.max(0, Math.ceil(sorted.length * quantile) - 1)]
    }
    let invalidDeckSpecError
    try {
      const truncated = new Uint8Array(
        await fetch('/deck-gate-spec.wdsf').then((response) => response.arrayBuffer()),
      ).slice(0, -1)
      await client.createDeckSession(deckTemplate.handle, truncated.buffer)
    } catch (error) {
      invalidDeckSpecError = error.envelope
    }
    let atomicOverflowError
    try {
      await client.createDeckSession(
        deckTemplate.handle,
        await fetch('/deck-gate-atomic-overflow.wdsf')
          .then((response) => response.arrayBuffer()),
      )
    } catch (error) {
      atomicOverflowError = error.envelope
    }
    const deckEvidence = {
      cacheable: deckTemplate.cacheable,
      templatePlanBase64: encodeBase64(deckTemplate.plan),
      planBase64: encodeBase64(deckSession.plan),
      slidesBase64: deckSlides,
      pptxBase64: encodeBase64(deckPptx),
      topology: {
        slideCount: deckSession.slideCount,
        presentableSlides: deckSession.presentableSlides,
        pages: deckPages,
        diagnostics: deckSession.diagnostics,
      },
      timings: {
        planSamplesMs: deckPlanSamplesMs,
        resolveAllSamplesMs: deckResolveAllSamplesMs,
        exportSamplesMs: deckExportSamplesMs,
        summary: {
          coldPlanMs: deckPlanSamplesMs[0],
          warmPlanP50Ms: percentile(deckPlanSamplesMs.slice(1), 0.5),
          warmPlanP95Ms: percentile(deckPlanSamplesMs.slice(1), 0.95),
          resolveAllP50Ms: percentile(deckResolveAllSamplesMs, 0.5),
          resolveAllP95Ms: percentile(deckResolveAllSamplesMs, 0.95),
          exportP50Ms: percentile(deckExportSamplesMs, 0.5),
          exportP95Ms: percentile(deckExportSamplesMs, 0.95),
        },
      },
      visualQuality: deckVisualQuality,
      invalidDeckSpecError,
      atomicOverflowError,
    }
    await client.releaseDeckSession(deckSession.handle)
    await client.releaseDeckTemplate(deckTemplate.handle)
    const liveEditingMatrix = []
    const pixelCanvas = new OffscreenCanvas(1, 1)
    const pixelContext = pixelCanvas.getContext('2d')
    pixelContext.fillStyle = '#00ff00'
    pixelContext.fillRect(0, 0, 1, 1)
    const onePixelPng = new Uint8Array(await (await pixelCanvas.convertToBlob({
      type: 'image/png',
    })).arrayBuffer())
    for (const slides of [10, 50, 200]) {
      const liveTemplate = await fetch(`/benchmark-mixed-${slides}.potx`)
        .then((response) => response.arrayBuffer())
      const livePrepared = await client.prepare(liveTemplate)
      const text = {}
      const images = {}
      for (let slide = 0; slide < slides; slide += 1) {
        for (let field = 0; field < 8; field += 1) {
          text[`text_${slide}_${field}`] = `Slide ${slide} field ${field}: live browser benchmark`
        }
        images[`image_${slide}`] = {
          bytes: onePixelPng,
          extension: 'png',
          contentType: 'image/png',
        }
      }
      const liveSession = await client.createLiveSession(livePrepared.handle, { text, images })
      const liveRenderer = new CanvasDisplayListRenderer(4 * 1024 * 1024)
      const liveCanvas = new OffscreenCanvas(640, 360)
      const liveContext = liveCanvas.getContext('2d', { alpha: false })
      const samples = []
      let revision = liveSession.revision
      let lastUpdate
      for (let iteration = 0; iteration < 5; iteration += 1) {
        const totalStart = performance.now()
        const applyStart = performance.now()
        lastUpdate = await client.applyLiveDelta(liveSession.handle, revision, {
          text: { text_0_0: `browser live edit ${slides}/${iteration}` },
        })
        const applyMs = performance.now() - applyStart
        revision = lastUpdate.revision
        const resolveStart = performance.now()
        const resolved = await client.resolveLiveSlide(liveSession.handle, revision, 0)
        const scene = decodeDisplayList(resolved.displayList)
        const resolveMs = performance.now() - resolveStart
        const renderStart = performance.now()
        const renderTelemetry = await liveRenderer.render(scene, liveContext, {
          imageCacheKey: async (image, signal) => {
            if (image.partName === undefined) throw new Error('live image part is missing')
            return client.liveSessionResourceFingerprint(
              liveSession.handle, revision, image.partName, { signal },
            )
          },
          imageResolver: async (image, signal) => {
            if (image.partName === undefined) throw new Error('live image part is missing')
            const resource = await client.liveSessionResource(
              liveSession.handle, revision, image.partName, { signal },
            )
            return decodeRasterImage(resource.bytes, {}, signal)
          },
        })
        const renderMs = performance.now() - renderStart
        const pixel = [...liveContext.getImageData(20, 20, 1, 1).data]
        samples.push({
          applyMs,
          resolveMs,
          renderMs,
          inputToPixelsMs: performance.now() - totalStart,
          pixel,
          cacheBytes: renderTelemetry.cacheBytes,
          cacheHitRate: renderTelemetry.cacheHitRate,
        })
      }
      let sustained
      if (slides === 200) {
        const beforeAba = await client.liveSessionCacheTelemetry(liveSession.handle)
        const aba = []
        for (const value of ['cache-A', 'cache-B', 'cache-A']) {
          const started = performance.now()
          lastUpdate = await client.applyLiveDelta(liveSession.handle, revision, {
            text: { text_0_0: value },
          })
          revision = lastUpdate.revision
          await client.resolveLiveSlide(liveSession.handle, revision, 0)
          aba.push(performance.now() - started)
        }
        const afterAba = await client.liveSessionCacheTelemetry(liveSession.handle)
        lastUpdate = await client.applyLiveDelta(liveSession.handle, revision, {
          text: { text_1_0: 'offscreen edit remains deferred' },
        })
        revision = lastUpdate.revision
        const offscreenInvalidatedSlides = lastUpdate.invalidatedSlides
        const traceSamplesMs = []
        let maximumInvalidatedSlides = 0
        for (let iteration = 0; iteration < 100; iteration += 1) {
          const started = performance.now()
          lastUpdate = await client.applyLiveDelta(liveSession.handle, revision, {
            text: { text_0_0: `sustained-${(iteration * 17) % 101}` },
          })
          revision = lastUpdate.revision
          maximumInvalidatedSlides = Math.max(
            maximumInvalidatedSlides,
            lastUpdate.invalidatedSlides.length,
          )
          await client.resolveLiveSlide(liveSession.handle, revision, 0)
          traceSamplesMs.push(performance.now() - started)
        }
        sustained = {
          iterations: traceSamplesMs.length,
          traceSamplesMs,
          maximumInvalidatedSlides,
          offscreenInvalidatedSlides,
          abaSamplesMs: aba,
          abaSceneCacheHits: afterAba.hits - beforeAba.hits,
          cache: await client.liveSessionCacheTelemetry(liveSession.handle),
        }
      }
      const exportStart = performance.now()
      const liveOutput = await client.generateLive(liveSession.handle, revision)
      const backgroundExportMs = performance.now() - exportStart
      const cache = await client.liveSessionCacheTelemetry(liveSession.handle)
      liveRenderer.clear()
      await client.releaseLiveSession(liveSession.handle)
      await client.release(livePrepared.handle)
      liveEditingMatrix.push({
        slides,
        revision,
        copiesPerEdit: {
          templateInput: 0,
          unchangedMedia: 0,
          displayListTransfer: 1,
          exportOutput: 1,
        },
        samples,
        backgroundExportMs,
        outputBytes: liveOutput.byteLength,
        invalidatedSlides: lastUpdate.invalidatedSlides,
        changedBindings: lastUpdate.changedBindings,
        overlay: lastUpdate.overlay,
        cache,
        sustained,
      })
    }
    const renderFixture = await fetch('/render-fixture.pptx').then((response) => response.arrayBuffer())
    const opened = await client.openPresentation(renderFixture)
    const displayBytes = await client.resolveSlide(opened.handle, 0)
    const scene = decodeDisplayList(displayBytes)
    const advancedDisplayBytes = await client.resolveSlide(opened.handle, 1)
    const advancedScene = decodeDisplayList(advancedDisplayBytes)
    const fidelityDisplayBytes = await client.resolveSlide(opened.handle, 2)
    const fidelityScene = decodeDisplayList(fidelityDisplayBytes)
    const offlinePages = [
      {
        slideIndex: 0,
        revision: 7,
        displayList: displayBytes,
        page: {
          pageId: '11'.repeat(16),
          logicalSlideId: '21'.repeat(16),
          hidden: false,
          continuationOrdinal: 1,
          continuationTotal: 2,
        },
      },
      {
        slideIndex: 1,
        revision: 7,
        displayList: displayBytes.slice(0),
        page: {
          pageId: '12'.repeat(16),
          logicalSlideId: '21'.repeat(16),
          hidden: false,
          continuationOrdinal: 2,
          continuationTotal: 2,
          continuationLabel: '2/2',
        },
      },
    ]
    const resolveOfflineResource = async ({ partName }, signal) =>
      partName === 'ppt/media/image1.png'
        ? onePixelPng
        : client.presentationResource(opened.handle, partName, { signal })
    const offline = await serializeOfflineHtmlDocument(
      offlinePages,
      resolveOfflineResource,
      { title: 'Offline <deck>', language: 'ko' },
    )
    const repeatedOffline = await serializeOfflineHtmlDocument(
      offlinePages,
      resolveOfflineResource,
      { title: 'Offline <deck>', language: 'ko' },
    )
    const offlineDocument = new DOMParser().parseFromString(offline.html, 'text/html')
    const readingOrders = [...offlineDocument.querySelectorAll(
      '.wasmppt-offline-page:first-child .wasmppt-dom-text-layer [data-reading-order]',
    )].map((element) => Number(element.dataset.readingOrder))
    const gifBytes = Uint8Array.from(
      atob('R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw=='),
      (value) => value.charCodeAt(0),
    )
    const gifOffline = await serializeOfflineHtmlDocument(
      offlinePages.slice(0, 1),
      async () => gifBytes,
    )
    const gifDocument = new DOMParser().parseFromString(gifOffline.html, 'text/html')
    const svgBytes = new TextEncoder().encode(
      '<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><path fill="blue" d="M0 0h2v2H0z"/></svg>',
    )
    const svgOffline = await serializeOfflineHtmlDocument(
      offlinePages.slice(0, 1),
      async () => svgBytes,
    )
    const svgDocument = new DOMParser().parseFromString(svgOffline.html, 'text/html')
    const advancedOffline = await serializeOfflineHtmlDocument([{
      slideIndex: 2,
      revision: 7,
      displayList: advancedDisplayBytes,
      page: {
        pageId: '13'.repeat(16),
        logicalSlideId: '23'.repeat(16),
        hidden: false,
        continuationOrdinal: 1,
        continuationTotal: 1,
      },
    }], async () => onePixelPng)
    const advancedOfflineDocument = new DOMParser().parseFromString(advancedOffline.html, 'text/html')
    const deckResolveIndices = []
    const deckOffline = await serializeDeckSessionToHtml({
      resolveDeckSlide: async (_handle, revision, slideIndex) => {
        deckResolveIndices.push(slideIndex)
        return {
          handle: 91,
          revision,
          slideIndex,
          fingerprint: `page-${slideIndex}`,
          page: {
            pageId: String(slideIndex + 1).padStart(2, '0').repeat(16),
            logicalSlideId: '31'.repeat(16),
            hidden: false,
            continuationOrdinal: slideIndex === 0 ? 1 : 2,
            continuationTotal: 2,
            ...(slideIndex === 0 ? {} : { continuationLabel: '2/2' }),
          },
          displayList: displayBytes.slice(0),
        }
      },
      deckSessionResource: async () => ({ fingerprint: 'image', bytes: onePixelPng }),
    }, {
      handle: 91,
      revision: 7,
      slideCount: 3,
      presentableSlides: [0, 2],
      plan: new ArrayBuffer(0),
    })
    const deckOfflineDocument = new DOMParser().parseFromString(deckOffline.html, 'text/html')
    let unresolvedResourceCode
    try {
      await serializeOfflineHtmlDocument(offlinePages.slice(0, 1), async () => {
        throw new Error('project access revoked')
      })
    } catch (error) {
      unresolvedResourceCode = error.code
    }
    const mismatchedPageSize = displayBytes.slice(0)
    const mismatchedView = new DataView(mismatchedPageSize)
    mismatchedView.setBigInt64(8, mismatchedView.getBigInt64(8, true) + 9_525n, true)
    let mismatchedPageSizeCode
    try {
      await serializeOfflineHtmlDocument([
        offlinePages[0],
        { ...offlinePages[1], displayList: mismatchedPageSize },
      ], resolveOfflineResource)
    } catch (error) {
      mismatchedPageSizeCode = error.code
    }
    const offlineFacts = {
      deterministic: offline.html === repeatedOffline.html,
      byteLength: offline.bytes.byteLength,
      pageCount: offline.pageCount,
      pageIds: offline.pageIds,
      widthEmu: offline.widthEmu,
      heightEmu: offline.heightEmu,
      resourceCount: offline.resourceCount,
      resourceBytes: offline.resourceBytes,
      csp: offlineDocument.querySelector('meta[http-equiv="Content-Security-Policy"]')?.content,
      language: offlineDocument.documentElement.lang,
      title: offlineDocument.title,
      pageSize: offlineDocument.querySelector('meta[name="wasmppt-page-size"]')?.content,
      continuation: offlineDocument.querySelector('[data-page-id="12121212121212121212121212121212"]')
        ?.dataset.continuationLabel,
      imageUrls: [...offlineDocument.querySelectorAll('svg image')]
        .map((element) => element.getAttribute('href')),
      gifUrl: gifDocument.querySelector('svg image')?.getAttribute('href'),
      svgUrl: svgDocument.querySelector('svg image')?.getAttribute('href'),
      advancedText: advancedOfflineDocument.body.textContent,
      advancedPaths: advancedOfflineDocument.querySelectorAll('svg path').length,
      advancedKinds: [...advancedOfflineDocument.querySelectorAll('[data-semantic-kind]')]
        .map((element) => element.dataset.semanticKind),
      advancedRunLink: advancedOfflineDocument
        .querySelector('a[data-hyperlink="https://example.org/quarter"]')
        ?.getAttribute('href'),
      deckResolveIndices,
      deckPhysicalIndices: [...deckOfflineDocument.querySelectorAll('[data-physical-slide-index]')]
        .map((element) => Number(element.dataset.physicalSlideIndex)),
      deckRole: offlineDocument.querySelector('.wasmppt-offline-deck')?.getAttribute('role'),
      mainRole: offlineDocument.querySelector('main')?.getAttribute('role'),
      pageSetSemantics: [...offlineDocument.querySelectorAll('.wasmppt-offline-page')]
        .map((element) => ({
          role: element.getAttribute('role'),
          position: element.getAttribute('aria-posinset'),
          size: element.getAttribute('aria-setsize'),
        })),
      readingOrderPreserved: readingOrders.every(
        (value, index) => index === 0 || readingOrders[index - 1] <= value,
      ),
      safeLink: offlineDocument.querySelector('a[data-shape-id="2"]')?.getAttribute('href'),
      scripts: offlineDocument.scripts.length,
      unresolvedResourceCode,
      mismatchedPageSizeCode,
    }
    const domHost = document.createElement('div')
    document.body.append(domHost)
    const domRenderer = new DomSvgRenderer()
    const domResult = await domRenderer.render(scene, domHost, {
      revision: 1,
      slideIndex: 0,
      imageResolver: async () =>
        'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><path fill="magenta" d="M0 0h2v2H0z"/></svg>',
    })
    const titleLink = domHost.querySelector('a[data-shape-id="2"]')
    const titleIdentity = titleLink
    const unchangedDomResult = await domRenderer.render(scene, domHost, { revision: 1 })
    await domRenderer.render(scene, domHost, { revision: 2 })
    const staleDomResult = await domRenderer.render(scene, domHost, { revision: 1 })
    const reusedTitle = titleIdentity === domHost.querySelector('a[data-shape-id="2"]')
    const photoGraphic = domHost.querySelector('.wasmppt-dom-text-layer [data-shape-id="3"]')
    const titleGraphicPaths = [...domHost.querySelectorAll('g[data-shape-id="2"] path')]
    const titleRuns = [...(titleLink?.querySelector('span')?.children ?? [])]
    const titleBounds = titleLink?.getBoundingClientRect()
    const domFacts = {
      text: titleLink?.textContent,
      selectable: titleLink?.style.userSelect,
      href: titleLink?.href,
      titleLabel: titleLink?.getAttribute('aria-label'),
      selectionId: titleLink?.dataset.selectionId,
      photoRole: photoGraphic?.getAttribute('role'),
      photoLabel: photoGraphic?.getAttribute('aria-label'),
      svgPaths: domHost.querySelectorAll('svg path').length,
      inlineImages: domHost.querySelectorAll('svg image').length,
      titleGraphicStyles: titleGraphicPaths.map((path) => ({
        fill: path.getAttribute('fill'),
        stroke: path.getAttribute('stroke'),
        strokeWidth: path.getAttribute('stroke-width'),
      })),
      unchangedUpdates: unchangedDomResult.updatedElements,
      staleIgnored: staleDomResult.stale,
      reusedTitle,
      diagnosticCodes: domResult.diagnostics.map((diagnostic) => diagnostic.code),
      titleLineCount: new Set(titleRuns.map((run) => Math.round(Number.parseFloat(run.style.top)))).size,
      titleBounds: titleBounds === undefined ? undefined : {
        width: titleBounds.width,
        height: titleBounds.height,
      },
    }
    const canvas = document.createElement('canvas')
    canvas.id = 'wasmppt-visual-slide-1'
    canvas.width = 640
    canvas.height = 360
    document.body.append(canvas)
    const context = canvas.getContext('2d', { alpha: false })
    const renderer = new CanvasDisplayListRenderer(4096)
    const koreanLines = wrapText('가나다라마바사', 3, (value) => [...value].length)
    const telemetry = await renderer.render(scene, context, {
      fontResolver: new FontResolver({
        theme: { latin: 'Arial', eastAsian: 'Arial', complexScript: 'Arial' },
      }),
      imageResolver: async () => {
        const source = new OffscreenCanvas(16, 16)
        const imageContext = source.getContext('2d')
        imageContext.fillStyle = '#ff00ff'
        imageContext.fillRect(0, 0, 8, 16)
        imageContext.fillStyle = '#00ffff'
        imageContext.fillRect(8, 0, 8, 16)
        const bitmap = source.transferToImageBitmap()
        return { source: bitmap, residentBytes: 16 * 16 * 4, close: () => bitmap.close() }
      },
    })
    const advancedCanvas = document.createElement('canvas')
    advancedCanvas.id = 'wasmppt-visual-slide-2'
    advancedCanvas.width = 640
    advancedCanvas.height = 360
    document.body.append(advancedCanvas)
    const advancedContext = advancedCanvas.getContext('2d', { alpha: false })
    let metafileSvgBytes = 0
    const loadMetafileSvg = async (image, signal) => {
      if (image.partName === undefined) throw new Error('metafile part is missing')
      const bytes = await client.presentationMetafileSvg(opened.handle, image.partName, { signal })
      metafileSvgBytes = bytes.byteLength
      return bytes
    }
    const advancedTelemetry = await renderer.render(advancedScene, advancedContext, {
      imageResolver: async (image, signal) => {
        if (image.partName?.match(/\.(?:emf|wmf)$/i)) {
          const bytes = await loadMetafileSvg(image, signal)
          return decodeSvgImage(bytes, signal)
        }
        if (image.partName === undefined) throw new Error('image part is missing')
        const bytes = await client.presentationResource(opened.handle, image.partName, { signal })
        return decodeRasterImage(bytes, {}, signal)
      },
    })
    const advancedPixels = advancedContext.getImageData(
      0,
      0,
      advancedCanvas.width,
      advancedCanvas.height,
    ).data
    let advancedColoredPixels = 0
    let smartartColoredPixels = 0
    let advancedPixelHash = 0x811c9dc5
    const [smartartX, smartartY, smartartWidth, smartartHeight] = smartartRegion
    for (let offset = 0; offset < advancedPixels.length; offset += 4) {
      advancedPixelHash = Math.imul(advancedPixelHash ^ advancedPixels[offset], 0x01000193) >>> 0
      advancedPixelHash = Math.imul(advancedPixelHash ^ advancedPixels[offset + 1], 0x01000193) >>> 0
      advancedPixelHash = Math.imul(advancedPixelHash ^ advancedPixels[offset + 2], 0x01000193) >>> 0
      advancedPixelHash = Math.imul(advancedPixelHash ^ advancedPixels[offset + 3], 0x01000193) >>> 0
      if (
        advancedPixels[offset] !== 255 ||
        advancedPixels[offset + 1] !== 255 ||
        advancedPixels[offset + 2] !== 255
      ) {
        advancedColoredPixels += 1
        const pixel = offset / 4
        const x = pixel % advancedCanvas.width
        const y = Math.floor(pixel / advancedCanvas.width)
        if (x >= smartartX && x < smartartX + smartartWidth &&
            y >= smartartY && y < smartartY + smartartHeight) smartartColoredPixels += 1
      }
    }
    const advancedDomHost = document.createElement('div')
    document.body.append(advancedDomHost)
    const advancedDom = await new DomSvgRenderer().render(advancedScene, advancedDomHost, {
      revision: 1,
      slideIndex: 1,
      imageResolver: async (image, signal) => {
        const metafile = image.partName?.match(/\.(?:emf|wmf)$/i)
        if (image.partName === undefined) throw new Error('image part is missing')
        const bytes = new Uint8Array(metafile
          ? await loadMetafileSvg(image, signal)
          : await client.presentationResource(opened.handle, image.partName, { signal }))
        let binary = ''
        for (const byte of bytes) binary += String.fromCharCode(byte)
        return `data:${metafile ? 'image/svg+xml' : 'image/png'};base64,${btoa(binary)}`
      },
    })
    const advancedFacts = {
      semanticKinds: advancedScene.semantics.map((semantic) => semantic.kind),
      strings: advancedScene.strings,
      textRuns: advancedScene.commands.flatMap((command) => command.kind === 'draw-rich-text'
        ? command.frame.paragraphs.flatMap((paragraph) => paragraph.runs.map((run) => run.text))
        : command.kind === 'draw-text' ? [advancedScene.strings[command.text]] : []),
      runLinks: advancedScene.commands.flatMap((command) => command.kind === 'draw-rich-text'
        ? command.frame.paragraphs.flatMap((paragraph) => paragraph.runs
            .flatMap((run) => run.hyperlink === undefined ? [] : [run.hyperlink]))
        : []),
      diagnosticCodes: advancedScene.diagnostics.map((diagnostic) => diagnostic.code),
      coloredPixels: advancedColoredPixels,
      smartartColoredPixels,
      pixelHash: advancedPixelHash.toString(16).padStart(8, '0'),
      commandCount: advancedTelemetry.commandCount,
      svgPathCount: advancedDomHost.querySelectorAll('path').length,
      inlineImages: advancedDomHost.querySelectorAll('image').length,
      inlineLink: advancedDomHost
        .querySelector('a[data-hyperlink="https://example.org/quarter"]')
        ?.getAttribute('href'),
      metafileSvgBytes,
      domDiagnosticCodes: advancedDom.diagnostics.map((diagnostic) => diagnostic.code),
    }
    const fidelityCanvas = document.createElement('canvas')
    fidelityCanvas.id = 'wasmppt-visual-slide-3'
    fidelityCanvas.width = 640
    fidelityCanvas.height = 360
    document.body.append(fidelityCanvas)
    const fidelityContext = fidelityCanvas.getContext('2d', { alpha: false })
    const fidelityTelemetry = await renderer.render(fidelityScene, fidelityContext, {
      fontResolver: new FontResolver({
        theme: { latin: 'Arial', eastAsian: 'Arial', complexScript: 'Arial' },
      }),
    })
    const fidelityDomHost = document.createElement('div')
    document.body.append(fidelityDomHost)
    await new DomSvgRenderer().render(fidelityScene, fidelityDomHost, {
      revision: 1,
      slideIndex: 2,
    })
    const fidelityStructure = Object.fromEntries([2, 3, 4, 5, 6, 7].map((shapeId) => {
      const element = fidelityDomHost.querySelector(`.wasmppt-dom-text-layer [data-shape-id="${shapeId}"]`)
      const runs = [...(element?.querySelectorAll(':scope > span > span') ?? [])]
      const bounds = element?.getBoundingClientRect()
      return [String(shapeId), {
        lineCount: new Set(runs.map((run) => Math.round(Number.parseFloat(run.style.top)))).size,
        runCount: runs.length,
        sourceRangesValid: runs.every((run) =>
          run.dataset.sourceStart === undefined ||
          Number(run.dataset.sourceEnd) >= Number(run.dataset.sourceStart)),
        bounds: bounds === undefined ? undefined : { width: bounds.width, height: bounds.height },
      }]
    }))
    const fidelityRegions = {
      'shape-autofit': [20, 15, 180, 55],
      'normal-autofit': [220, 15, 180, 55],
      'unicode-wrap': [415, 15, 200, 55],
      'paragraph-metrics': [20, 88, 275, 120],
      columns: [315, 88, 300, 120],
      'text-effects': [20, 230, 595, 95],
    }
    const fidelityRegionPixels = Object.fromEntries(Object.entries(fidelityRegions).map(([id, region]) => {
      const pixels = fidelityContext.getImageData(...region).data
      let colored = 0
      for (let offset = 0; offset < pixels.length; offset += 4) {
        if (pixels[offset] !== 255 || pixels[offset + 1] !== 255 || pixels[offset + 2] !== 255) colored += 1
      }
      return [id, colored]
    }))
    const fidelityFacts = {
      commandCount: fidelityTelemetry.commandCount,
      diagnosticCodes: fidelityScene.diagnostics.map((diagnostic) => diagnostic.code),
      structure: fidelityStructure,
      regionPixels: fidelityRegionPixels,
    }
    const { default: PptxBrowserRenderer } = await import('/competitors/pptx-browser/index.js')
    const competitorFixture = await fetch('/competitor-fixture.pptx').then((response) => response.arrayBuffer())
    const competitorSamplesMs = []
    let competitorFacts
    for (let iteration = 0; iteration < 10; iteration += 1) {
      const competitor = new PptxBrowserRenderer()
      const competitorCanvas = document.createElement('canvas')
      const start = performance.now()
      await competitor.load(competitorFixture.slice(0))
      await competitor.renderSlide(0, competitorCanvas, 640)
      competitorSamplesMs.push(performance.now() - start)
      const competitorPixels = competitorCanvas.getContext('2d').getImageData(
        0,
        0,
        competitorCanvas.width,
        competitorCanvas.height,
      ).data
      let nonWhitePixels = 0
      for (let offset = 0; offset < competitorPixels.length; offset += 4) {
        if (
          competitorPixels[offset] !== 255 ||
          competitorPixels[offset + 1] !== 255 ||
          competitorPixels[offset + 2] !== 255
        ) nonWhitePixels += 1
      }
      competitorFacts = {
        slideCount: competitor.slideCount,
        width: competitorCanvas.width,
        height: competitorCanvas.height,
        nonWhitePixels,
      }
      competitor.destroy()
    }
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data
    let pixelHash = 0x811c9dc5
    for (const byte of pixels) pixelHash = Math.imul(pixelHash ^ byte, 0x01000193) >>> 0
    const pixelAt = (x, y) => Array.from(
      pixels.slice((y * canvas.width + x) * 4, (y * canvas.width + x) * 4 + 4),
    )
    let darkGroupPixels = 0
    for (let y = 45; y < 115; y += 1) {
      for (let x = 95; x < 610; x += 1) {
        const offset = (y * canvas.width + x) * 4
        if (pixels[offset] < 50 && pixels[offset + 1] < 50 && pixels[offset + 2] < 50) {
          darkGroupPixels += 1
        }
      }
    }
    const viewerRoot = document.createElement('div')
    document.body.append(viewerRoot)
    const viewer = new VirtualizedCanvasViewer(
      { resolveSlide: async () => displayBytes.slice(0) },
      opened.handle,
      viewerRoot,
      new CanvasDisplayListRenderer(0),
      { sceneCacheBytes: displayBytes.byteLength * 2, prefetchNeighbors: 0 },
    )
    const firstVisibleStarted = performance.now()
    await viewer.setVisibleSlides([0, 1])
    const firstVisibleSlideSamplesMs = [performance.now() - firstVisibleStarted]
    const mountedAtPeak = viewer.mountedSlideCount
    await viewer.setVisibleSlides([1])
    const mountedAfterScroll = viewer.mountedSlideCount
    const cachedSceneBytes = viewer.cachedSceneBytes
    viewer.dispose()
    const thumbnail = await renderOffscreenThumbnail(scene, 160)
    const thumbnailFacts = {
      width: thumbnail.width,
      height: thumbnail.height,
      commandCount: thumbnail.telemetry.commandCount,
    }
    thumbnail.bitmap.close()
    for (let iteration = 1; iteration < 5; iteration += 1) {
      const sampleRoot = document.createElement('div')
      document.body.append(sampleRoot)
      const sampleViewer = new VirtualizedCanvasViewer(
        { resolveSlide: async () => displayBytes.slice(0) },
        opened.handle,
        sampleRoot,
        new CanvasDisplayListRenderer(0),
        { prefetchNeighbors: 0 },
      )
      const sampleStarted = performance.now()
      await sampleViewer.setVisibleSlides([0])
      firstVisibleSlideSamplesMs.push(performance.now() - sampleStarted)
      sampleViewer.dispose()
      sampleRoot.remove()
    }
    const mountedAfterDispose = viewerRoot.querySelectorAll('canvas').length
    let staleAbortCount = 0
    const staleRoot = document.createElement('div')
    document.body.append(staleRoot)
    const staleViewer = new VirtualizedCanvasViewer(
      {
        resolveSlide: (_handle, index, options) => new Promise((resolvePromise, reject) => {
          const abort = () => {
            clearTimeout(timer)
            staleAbortCount += 1
            reject(new DOMException('cancelled', 'AbortError'))
          }
          const timer = setTimeout(() => {
            options.signal?.removeEventListener('abort', abort)
            resolvePromise(displayBytes.slice(0))
          }, index === 0 ? 50 : 0)
          options.signal?.addEventListener('abort', abort, { once: true })
        }),
      },
      opened.handle,
      staleRoot,
      new CanvasDisplayListRenderer(0),
      { prefetchNeighbors: 0 },
    )
    const staleRender = staleViewer.setVisibleSlides([0]).catch((error) => error.name)
    await Promise.resolve()
    await staleViewer.setVisibleSlides([1])
    const staleResult = await staleRender
    const staleMountedSlides = [...staleRoot.querySelectorAll('canvas')].map(
      (element) => element.dataset.slideIndex,
    )
    staleViewer.dispose()
    const stressRoot = document.createElement('div')
    document.body.append(stressRoot)
    const stressViewer = new VirtualizedCanvasViewer(
      { resolveSlide: async () => displayBytes.slice(0) },
      opened.handle,
      stressRoot,
      new CanvasDisplayListRenderer(0),
      { sceneCacheBytes: displayBytes.byteLength * 4, prefetchNeighbors: 1 },
    )
    const stressStarted = performance.now()
    let stressPeakMounted = 0
    for (let slideIndex = 0; slideIndex < 1000; slideIndex += 1) {
      await stressViewer.setVisibleSlides([slideIndex])
      stressPeakMounted = Math.max(stressPeakMounted, stressViewer.mountedSlideCount)
    }
    const stressElapsedMs = performance.now() - stressStarted
    const stressCache = stressViewer.sceneCacheTelemetry
    stressViewer.dispose()
    const stressMountedAfterDispose = stressRoot.children.length
    const domViewerRoot = document.createElement('div')
    document.body.append(domViewerRoot)
    const domViewer = new VirtualizedDomViewer(
      { resolveSlide: async () => displayBytes.slice(0) },
      opened.handle,
      domViewerRoot,
      new DomSvgRenderer(),
      { sceneCacheBytes: displayBytes.byteLength * 2, prefetchNeighbors: 0 },
    )
    await domViewer.setVisibleSlides([0, 1])
    const domMountedAtPeak = domViewer.mountedSlideCount
    await domViewer.setVisibleSlides([1])
    const domMountedAfterScroll = domViewer.mountedSlideCount
    domViewer.dispose()
    const domMountedAfterDispose = domViewerRoot.children.length
    const resourceCacheBytes = client.resourceCacheBytes
    await client.releasePresentation(opened.handle)
    renderer.clear()
    let invalidPackageError
    try {
      await client.prepare(new TextEncoder().encode('not a zip').buffer)
    } catch (error) {
      invalidPackageError = error.envelope
    }
    client.terminate()
    let outputBinary = ''
    for (let offset = 0; offset < output.byteLength; offset += 0x8000) {
      outputBinary += String.fromCharCode(...output.subarray(offset, offset + 0x8000))
    }
    return {
      transferredByteLength,
      coldPrepareMs,
      warmInjectionSamplesMs,
      topologyUpdate,
      deckEvidence,
      liveEditingMatrix,
      residentBytes: prepared.residentBytes,
      zipSignature: [...output.subarray(0, 2)],
      outputBytes: output.byteLength,
      outputBase64: btoa(outputBinary),
      parityPayloadHex: encodedPayloadHex,
      slideCount: opened.slideCount,
      commandCount: scene.commands.length,
      pixelHash: pixelHash.toString(16).padStart(8, '0'),
      pixelSamples: {
        background: pixelAt(620, 340),
        imageLeft: pixelAt(280, 80),
        imageRight: pixelAt(340, 80),
        groupFill: pixelAt(120, 130),
      },
      darkGroupPixels,
      mountedAtPeak,
      mountedAfterScroll,
      mountedAfterDispose,
      cachedSceneBytes,
      resourceCacheBytes,
      invalidPackageError,
      firstVisibleSlideSamplesMs,
      displayByteLength: displayBytes.byteLength,
      staleAbortCount,
      staleResult,
      staleMountedSlides,
      stress: {
        slideCount: 1000,
        elapsedMs: stressElapsedMs,
        peakMounted: stressPeakMounted,
        cache: stressCache,
        mountedAfterDispose: stressMountedAfterDispose,
      },
      thumbnailFacts,
      decodedImageBytesAfterClear: renderer.decodedImageBytes,
      koreanLines,
      telemetry,
      advancedFacts,
      fidelityFacts,
      pptxBrowserComparison: { samplesMs: competitorSamplesMs, correctness: competitorFacts },
      offlineFacts,
      domFacts,
      domMountedAtPeak,
      domMountedAfterScroll,
      domMountedAfterDispose,
      exactShaping: {
        cacheIdentity: exactShapeFirst === exactShapeSecond,
        unitsPerEm: exactShapeFirst.unitsPerEm,
        glyphCount: exactShapeFirst.glyphs.length,
        glyphCountWithoutLigatures: exactShapeWithoutLigatures.glyphs.length,
        clusters: exactShapeFirst.glyphs.map((glyph) => glyph.cluster),
        breakTokens: exactBreakTokens,
      },
    }
  }, featureRegions.smartart)
  assert.equal(result.transferredByteLength, 0, 'template ArrayBuffer was cloned, not transferred')
  const sortedWarmSamples = result.warmInjectionSamplesMs.toSorted((left, right) => left - right)
  const warmP50Ms = sortedWarmSamples[Math.ceil(sortedWarmSamples.length * 0.5) - 1]
  const warmP95Ms = sortedWarmSamples[Math.ceil(sortedWarmSamples.length * 0.95) - 1]
  const sortedFirstVisibleSamples = result.firstVisibleSlideSamplesMs.toSorted((left, right) => left - right)
  const firstVisibleP95Ms = sortedFirstVisibleSamples[Math.ceil(sortedFirstVisibleSamples.length * 0.95) - 1]
  assert(result.coldPrepareMs <= performanceBudgets.browserScalarWasm.maximumColdPrepareMs)
  assert(warmP95Ms <= performanceBudgets.browserScalarWasm.maximumWarmInjectionP95Ms)
  assert(firstVisibleP95Ms <= performanceBudgets.browserScalarWasm.maximumFirstVisibleSlideMs)
  assert(result.residentBytes > 0)
  assert.equal(result.topologyUpdate.fullFallback, true)
  assert.equal(result.topologyUpdate.invalidationReason, 'topology')
  assert.equal(result.topologyUpdate.slideCount, 2)
  assert.deepEqual(result.topologyUpdate.invalidatedSlides, [0, 1])
  for (const live of result.liveEditingMatrix) {
    const inputSamples = live.samples.map((sample) => sample.inputToPixelsMs)
      .toSorted((left, right) => left - right)
    const inputP95 = inputSamples[Math.ceil(inputSamples.length * 0.95) - 1]
    assert(inputP95 <= performanceBudgets.browserScalarWasm.maximumLiveInputToPixelsP95Ms)
    assert(live.backgroundExportMs <= performanceBudgets.browserScalarWasm.maximumLiveBackgroundExportMs)
    assert(live.cache.peakBytes <= performanceBudgets.browserScalarWasm.maximumLivePeakCacheBytes)
    assert(live.invalidatedSlides.length <= performanceBudgets.browserScalarWasm.maximumLiveInvalidatedSlides)
    assert.deepEqual(live.changedBindings, ['text_0_0'])
    assert(live.overlay.reusedMaterializedParts > 0)
    assert(live.samples.every((sample) => sample.pixel.length === 4))
    assert(live.samples.at(-1).cacheHitRate.decodedImages > 0)
    assert(
      live.samples.at(-1).cacheHitRate.textMeasurements > 0 ||
      live.samples.at(-1).cacheHitRate.richTextLayouts > 0,
    )
    assert(live.outputBytes > 0)
    if (live.slides === 200) {
      assert.equal(live.sustained.iterations, 100)
      assert.deepEqual(live.sustained.offscreenInvalidatedSlides, [1])
      assert.equal(live.sustained.maximumInvalidatedSlides, 1)
      assert(live.sustained.abaSceneCacheHits >= 1)
      assert(live.sustained.cache.residentBytes <= 16 * 1024 * 1024)
      assert(
        live.sustained.traceSamplesMs.reduce((sum, sample) => sum + sample, 0) /
          live.sustained.iterations <=
        performanceBudgets.browserScalarWasm.maximumLiveSustainedAverageMs,
      )
    }
  }
  assert.deepEqual(result.zipSignature, [0x50, 0x4b])
  assert.equal(result.parityPayloadHex, parityPayloadHex)
  assert(result.outputBytes > 0)
  const parityDirectory = join(workspaceDirectory, 'target/host-parity')
  await mkdir(parityDirectory, { recursive: true })
  await writeFile(join(parityDirectory, 'browser.pptx'), Buffer.from(result.outputBase64, 'base64'))
  const deckGateDirectory = join(workspaceDirectory, 'target/deck-gates')
  await mkdir(deckGateDirectory, { recursive: true })
  await writeFile(
    join(deckGateDirectory, 'browser.wdtp'),
    Buffer.from(result.deckEvidence.templatePlanBase64, 'base64'),
  )
  await writeFile(
    join(deckGateDirectory, 'browser.wdpl'),
    Buffer.from(result.deckEvidence.planBase64, 'base64'),
  )
  await writeFile(
    join(deckGateDirectory, 'browser.pptx'),
    Buffer.from(result.deckEvidence.pptxBase64, 'base64'),
  )
  for (const [index, displayList] of result.deckEvidence.slidesBase64.entries()) {
    await writeFile(
      join(deckGateDirectory, `browser-${String(index).padStart(4, '0')}.wpdl`),
      Buffer.from(displayList, 'base64'),
    )
  }
  await writeFile(
    join(deckGateDirectory, 'browser-topology.json'),
    `${JSON.stringify(result.deckEvidence.topology)}\n`,
  )
  await writeFile(
    join(deckGateDirectory, 'browser-timings.json'),
    `${JSON.stringify(result.deckEvidence.timings)}\n`,
  )
  await writeFile(
    join(deckGateDirectory, 'browser-quality.json'),
    `${JSON.stringify(result.deckEvidence.visualQuality)}\n`,
  )
  assert.equal(result.deckEvidence.cacheable, true)
  assert.equal(result.deckEvidence.topology.slideCount, 73)
  assert.equal(result.deckEvidence.topology.presentableSlides.length, 72)
  assert.equal(result.deckEvidence.visualQuality.renderedSlides, 73)
  assert(result.deckEvidence.visualQuality.sourceElements >= 30)
  assert(result.deckEvidence.topology.pages.some((deckPage) => deckPage.hidden))
  assert(
    result.deckEvidence.topology.diagnostics.some(
      (diagnostic) => diagnostic.code === 300 &&
        diagnostic.name === 'plan-font-risk' &&
        diagnostic.severity === 'warning' &&
        typeof diagnostic.nodeId === 'string',
    ),
  )
  assert.equal(result.deckEvidence.invalidDeckSpecError.domain, 'payload')
  assert.equal(result.deckEvidence.invalidDeckSpecError.code, 'invalid-deck-spec')
  assert.equal(result.deckEvidence.atomicOverflowError.domain, 'layout')
  assert.equal(result.deckEvidence.atomicOverflowError.code, 'plan-atomic-overflow')
  assert(
    result.deckEvidence.timings.summary.coldPlanMs
      <= performanceBudgets.browserScalarWasm.maximumDeckPlanMs,
  )
  assert(
    result.deckEvidence.timings.summary.warmPlanP95Ms
      <= performanceBudgets.browserScalarWasm.maximumDeckPlanMs,
  )
  assert(
    result.deckEvidence.timings.summary.resolveAllP95Ms
      <= performanceBudgets.browserScalarWasm.maximumDeckResolveAllMs,
  )
  assert(
    result.deckEvidence.timings.summary.exportP95Ms
      <= performanceBudgets.browserScalarWasm.maximumDeckExportMs,
  )
  assert.equal(result.slideCount, 3)
  assert.equal(result.commandCount, 12)
  assert.equal(result.decodedImageBytesAfterClear, 0)
  assert.deepEqual(result.koreanLines, ['가나다', '라마바', '사'])
  assert(result.telemetry.displayExecutionMs >= 0)
  assert(result.telemetry.fontMeasurementMs >= 0)
  assert(result.telemetry.mediaDecodeMs >= 0)
  assert(result.advancedFacts.semanticKinds.includes('table'))
  assert(result.advancedFacts.semanticKinds.includes('chart'))
  assert(result.advancedFacts.semanticKinds.includes('image'))
  assert(result.advancedFacts.textRuns.includes('Quarter'))
  assert.deepEqual(result.advancedFacts.runLinks, ['https://example.org/quarter'])
  assert.equal(result.advancedFacts.inlineLink, 'https://example.org/quarter')
  assert(result.advancedFacts.textRuns.includes('42'))
  for (const code of [
    'unsupported-animation',
    'unsupported-transition',
    'unsupported-3d',
  ]) {
    assert(result.advancedFacts.diagnosticCodes.includes(code))
  }
  assert.deepEqual(
    result.advancedFacts.domDiagnosticCodes,
    result.advancedFacts.diagnosticCodes,
  )
  assert(result.advancedFacts.coloredPixels > 10_000)
  assert(result.advancedFacts.smartartColoredPixels > 1_000)
  assert(result.advancedFacts.metafileSvgBytes > 0)
  assert.equal(result.advancedFacts.inlineImages, 2)
  assert(result.advancedFacts.commandCount > 10)
  assert(result.advancedFacts.svgPathCount > 10)
  assert(result.fidelityFacts.commandCount >= 13)
  assert.deepEqual(result.fidelityFacts.diagnosticCodes, [])
  for (const shape of Object.values(result.fidelityFacts.structure)) {
    assert.equal(structuralTextPassed(shape), true)
  }
  const firstStructure = Object.values(result.fidelityFacts.structure)[0]
  assert.equal(
    structuralTextPassed({ ...firstStructure, lineCount: 0 }),
    false,
    'a deliberate one-line structural regression must fail closed',
  )
  for (const coloredPixels of Object.values(result.fidelityFacts.regionPixels)) {
    assert(coloredPixels > 100)
  }
  assert.equal(result.pptxBrowserComparison.correctness.slideCount, 10)
  assert.equal(result.pptxBrowserComparison.correctness.width, 640)
  assert.equal(result.offlineFacts.deterministic, true)
  assert(result.offlineFacts.byteLength > 0)
  assert.equal(result.offlineFacts.pageCount, 2)
  assert.deepEqual(result.offlineFacts.pageIds, ['11'.repeat(16), '12'.repeat(16)])
  assert.equal(result.offlineFacts.pageSize, `${result.offlineFacts.widthEmu}x${result.offlineFacts.heightEmu}`)
  assert.equal(result.offlineFacts.language, 'ko')
  assert.equal(result.offlineFacts.title, 'Offline <deck>')
  assert.equal(result.offlineFacts.continuation, '2/2')
  assert(result.offlineFacts.resourceCount > 0)
  assert(result.offlineFacts.resourceBytes > 0)
  assert(result.offlineFacts.csp.includes("default-src 'none'"))
  assert(result.offlineFacts.imageUrls.every((url) => url?.startsWith('data:')))
  assert(result.offlineFacts.gifUrl.startsWith('data:image/png'))
  assert(result.offlineFacts.svgUrl.startsWith('data:image/svg+xml'))
  assert(result.offlineFacts.advancedText.includes('Quarter'))
  assert(result.offlineFacts.advancedPaths > 10)
  assert(result.offlineFacts.advancedKinds.includes('table'))
  assert(result.offlineFacts.advancedKinds.includes('chart'))
  assert.equal(result.offlineFacts.advancedRunLink, 'https://example.org/quarter')
  assert.deepEqual(result.offlineFacts.deckResolveIndices, [0, 2])
  assert.deepEqual(result.offlineFacts.deckPhysicalIndices, [0, 2])
  assert.equal(result.offlineFacts.deckRole, 'list')
  assert.equal(result.offlineFacts.mainRole, null)
  assert.deepEqual(result.offlineFacts.pageSetSemantics, [
    { role: 'listitem', position: '1', size: '2' },
    { role: 'listitem', position: '2', size: '2' },
  ])
  assert.equal(result.offlineFacts.readingOrderPreserved, true)
  assert.equal(result.offlineFacts.safeLink, 'https://example.com/report')
  assert.equal(result.offlineFacts.scripts, 0)
  assert.equal(result.offlineFacts.unresolvedResourceCode, 'unresolved-resource')
  assert.equal(result.offlineFacts.mismatchedPageSizeCode, 'invalid-document')
  assert.equal(result.domFacts.text, 'Actual title')
  assert.equal(result.domFacts.selectable, 'text')
  assert.equal(result.domFacts.href, 'https://example.com/report')
  assert.equal(result.domFacts.titleLabel, 'Quarterly report title')
  assert.equal(result.domFacts.selectionId, 'shape:2:2')
  assert.equal(result.domFacts.photoRole, 'img')
  assert.equal(result.domFacts.photoLabel, 'Quarterly report photo')
  assert(result.domFacts.svgPaths >= 3)
  assert.equal(result.domFacts.inlineImages, 1)
  assert.deepEqual(result.domFacts.titleGraphicStyles, [
    { fill: 'rgba(91, 132, 173, 1)', stroke: 'none', strokeWidth: null },
    { fill: 'none', stroke: 'rgba(0, 0, 0, 1)', strokeWidth: '12700' },
  ])
  assert.equal(result.domFacts.unchangedUpdates, 0)
  assert.equal(result.domFacts.staleIgnored, true)
  assert.equal(result.domFacts.reusedTitle, true)
  assert.deepEqual(result.domFacts.diagnosticCodes, [
    'unsupported-graphic-frame',
  ])
  assert.equal(result.domFacts.titleLineCount, 1)
  assert(result.domFacts.titleBounds.width > 0)
  assert(result.domFacts.titleBounds.height > 0)
  assert.equal(result.domMountedAtPeak, 2)
  assert.equal(result.domMountedAfterScroll, 1)
  assert.equal(result.domMountedAfterDispose, 0)
  assert.equal(result.exactShaping.cacheIdentity, true)
  assert(result.exactShaping.unitsPerEm > 0)
  assert(result.exactShaping.glyphCount > 0)
  assert(result.exactShaping.glyphCountWithoutLigatures >= result.exactShaping.glyphCount)
  assert(result.exactShaping.clusters.every((cluster) => cluster >= 0 && cluster < 6))
  assert.deepEqual(result.exactShaping.breakTokens, ['alpha', ' ', 'beta', '\n', '日', '本', '語'])
  assert.match(result.pixelHash, /^[0-9a-f]{8}$/)
  assert.deepEqual(result.pixelSamples.background, [255, 255, 255, 255])
  assert.deepEqual(result.pixelSamples.imageLeft, [255, 0, 255, 255])
  assert.deepEqual(result.pixelSamples.imageRight, [0, 255, 255, 255])
  assert.deepEqual(result.pixelSamples.groupFill, [91, 132, 173, 255])
  assert(result.darkGroupPixels > 100)
  assert.equal(result.mountedAtPeak, 2)
  assert.equal(result.mountedAfterScroll, 1)
  assert.equal(result.mountedAfterDispose, 0)
  assert(result.cachedSceneBytes <= result.displayByteLength * 2)
  assert(result.resourceCacheBytes > 0)
  assert.equal(result.invalidPackageError.version, 1)
  assert.equal(result.invalidPackageError.domain, 'package')
  assert.equal(result.invalidPackageError.code, 'truncated')
  assert.equal(result.staleAbortCount, 1)
  assert.equal(result.staleResult, 'AbortError')
  assert.deepEqual(result.staleMountedSlides, ['1'])
  assert.equal(result.stress.slideCount, 1000)
  assert(
    result.stress.elapsedMs / result.stress.slideCount
      <= performanceBudgets.browserScalarWasm.maximumRapidScrollAverageMs,
  )
  assert(
    result.stress.peakMounted
      <= performanceBudgets.browserScalarWasm.maximumStronglyReferencedSlides,
  )
  assert(result.stress.cache.residentBytes <= result.displayByteLength * 4)
  assert.equal(result.stress.mountedAfterDispose, 0)
  assert(result.thumbnailFacts.width <= 160)
  assert(result.thumbnailFacts.height > 0)
  assert.equal(result.thumbnailFacts.commandCount, result.commandCount)
  assert.deepEqual(errors, [])
  const visualDirectory = join(workspaceDirectory, 'target/visual-report')
  await mkdir(visualDirectory, { recursive: true })
  await page.locator('#wasmppt-visual-slide-1').screenshot({
    path: join(visualDirectory, 'slide-1-actual.png'),
  })
  await page.locator('#wasmppt-visual-slide-2').screenshot({
    path: join(visualDirectory, 'slide-2-actual.png'),
  })
  await page.locator('#wasmppt-visual-slide-3').screenshot({
    path: join(visualDirectory, 'slide-3-actual.png'),
  })
  const expectedSamples = {
    background: [255, 255, 255, 255],
    imageLeft: [255, 0, 255, 255],
    imageRight: [0, 255, 255, 255],
    groupFill: [91, 132, 173, 255],
  }
  const sampledPixelDifferences = Object.entries(expectedSamples).filter(
    ([name, expected]) => !expected.every((byte, index) => result.pixelSamples[name][index] === byte),
  ).length
  const visualReport = {
    schema: 2,
    engine: 'chromium-canvas2d',
    fixture: {
      id: renderCorpus.presentations[0].id,
      generator: renderCorpus.generator,
      sha256: createHash('sha256').update(await readFile(join(workspaceDirectory, 'fixtures/render/basic.pptx'))).digest('hex'),
    },
    viewport: { width: 640, height: 360 },
    slides: [
      {
        slideIndex: 0,
        actual: 'slide-1-actual.png',
        pixelHash: result.pixelHash,
        sampledPixelDifferences,
        tolerance: { sampledPixelDifferences: 0 },
        passed: sampledPixelDifferences === 0,
      },
      {
        slideIndex: 1,
        actual: 'slide-2-actual.png',
        pixelHash: result.advancedFacts.pixelHash,
        coloredPixels: result.advancedFacts.coloredPixels,
        tolerance: { minimumColoredPixels: 10_000 },
        passed: result.advancedFacts.coloredPixels > 10_000,
      },
      {
        slideIndex: 2,
        actual: 'slide-3-actual.png',
        featureRegionsPassed: Object.values(result.fidelityFacts.regionPixels)
          .every((pixels) => pixels > 100),
        passed: Object.values(result.fidelityFacts.regionPixels).every((pixels) => pixels > 100),
      },
    ],
    features: [
      { id: 'text', slideIndex: 0, region: featureRegions.text, metric: 'minimum-dark-pixels', actual: result.darkGroupPixels, tolerance: 100, passed: result.darkGroupPixels > 100 && result.domFacts.titleLineCount === 1, structural: { lineCount: result.domFacts.titleLineCount, expectedLineCount: 1, bounds: result.domFacts.titleBounds } },
      { id: 'shapes', slideIndex: 1, region: featureRegions.shapes, metric: 'minimum-colored-pixels', actual: result.advancedFacts.coloredPixels, tolerance: 10_000, passed: result.advancedFacts.coloredPixels > 10_000 },
      { id: 'raster-images', slideIndex: 0, region: featureRegions['raster-images'], metric: 'sampled-pixel-differences', actual: sampledPixelDifferences, tolerance: 0, passed: sampledPixelDifferences === 0 },
      { id: 'charts', slideIndex: 1, region: featureRegions.charts, metric: 'minimum-svg-paths', actual: result.advancedFacts.svgPathCount, tolerance: 10, passed: result.advancedFacts.svgPathCount > 10 },
      { id: 'smartart', slideIndex: 1, region: featureRegions.smartart, metric: 'minimum-fallback-image-pixels', actual: result.advancedFacts.smartartColoredPixels, tolerance: 1_000, passed: result.advancedFacts.smartartColoredPixels > 1_000 },
      { id: 'metafiles', slideIndex: 1, region: featureRegions.metafiles, metric: 'minimum-converted-bytes', actual: result.advancedFacts.metafileSvgBytes, tolerance: 1, passed: result.advancedFacts.metafileSvgBytes > 0 },
      ...fidelityFeatureDefinitions.map((feature) => ({
        id: feature.id,
        slideIndex: feature.slideIndex,
        region: featureRegions[feature.id],
        metric: 'minimum-colored-pixels',
        actual: result.fidelityFacts.regionPixels[feature.id],
        tolerance: 100,
        passed: result.fidelityFacts.regionPixels[feature.id] > 100 &&
          structuralTextPassed(result.fidelityFacts.structure[String(feature.structureIndex)]),
        structural: result.fidelityFacts.structure[String(feature.structureIndex)],
      })),
    ],
  }
  assert(visualReport.slides.every((slide) => slide.passed))
  assert(visualReport.features.every((feature) => feature.passed))
  await writeFile(
    join(visualDirectory, 'report.json'),
    `${JSON.stringify(visualReport, null, 2)}\n`,
  )
  const benchmarkDirectory = join(workspaceDirectory, 'target/benchmarks')
  await mkdir(benchmarkDirectory, { recursive: true })
  await writeFile(
    join(benchmarkDirectory, 'browser.json'),
    `${JSON.stringify({
      schema: 1,
      generatedAt: new Date().toISOString(),
      source: { revision: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim() },
      host: 'chromium-scalar-wasm-module-worker',
      environment: {
        hardware: { cpu: cpus()[0]?.model ?? 'unknown', logicalCpus: cpus().length, architecture: arch() },
        os: { platform: platform(), release: release() },
        runtimes: { node: process.version, chromium: browser.version() },
      },
      configuration: {
        wasm: 'scalar',
        worker: 'module',
        compression: 'deterministic DEFLATE level 6',
        outputChunkBytes: 262144,
      },
      fixture: {
        id: 'host-minimal-potx',
        sha256: createHash('sha256').update(await readFile(join(workspaceDirectory, 'fixtures/host-adapters/minimal.potx'))).digest('hex'),
      },
      iterations: result.warmInjectionSamplesMs.length,
      copies: { input: 0, output: 1 },
      preparedResidentBytes: result.residentBytes,
      coldPrepareMs: result.coldPrepareMs,
      warmInjectionSamplesMs: result.warmInjectionSamplesMs,
      summary: { warmInjectionP50Ms: warmP50Ms, warmInjectionP95Ms: warmP95Ms },
      rendering: {
        firstVisibleSlideSamplesMs: result.firstVisibleSlideSamplesMs,
        summary: { firstVisibleSlideP95Ms: firstVisibleP95Ms },
        stagesMs: result.telemetry,
        cacheBytes: {
          scene: result.cachedSceneBytes,
          decodedImages: result.telemetry.cacheBytes.decodedImages,
          resources: result.resourceCacheBytes,
        },
        stress1000Slides: result.stress,
        offscreenThumbnail: result.thumbnailFacts,
      },
      liveEditing: result.liveEditingMatrix,
      correctness: { zipSignature: result.zipSignature, outputBytes: result.outputBytes },
      comparison: {
        library: { name: 'pptx-browser', version: '4.1.4' },
        excludedLatest: {
          version: '4.1.5',
          reason: 'The published npm tarball omits required modules including src/zip.js and src/render.js.',
        },
        workload: 'cold-load-and-render-first-slide',
        settings: { width: 640, input: 'PptxGenJS text-heavy 10-slide deck', fonts: 'system Arial' },
        semanticDifference: 'Both paint Canvas; comparator eligibility requires that the text-heavy slide contain non-white pixels.',
        samplesMs: result.pptxBrowserComparison.samplesMs,
        summary: {
          p50Ms: result.pptxBrowserComparison.samplesMs.toSorted((a, b) => a - b)[4],
          p95Ms: result.pptxBrowserComparison.samplesMs.toSorted((a, b) => a - b)[9],
        },
        correctness: {
          ...result.pptxBrowserComparison.correctness,
          caughtShapeWarnings: competitorWarnings.length,
          eligible: result.pptxBrowserComparison.correctness.nonWhitePixels > 100 && competitorWarnings.length === 0,
          exclusionReason: result.pptxBrowserComparison.correctness.nonWhitePixels > 100 && competitorWarnings.length === 0
            ? null
            : 'Renderer caught required text-shape failures; timings are ineligible.',
        },
      },
    }, null, 2)}\n`,
  )
  console.log(
    `browser host fixture ok: ${result.outputBytes} output bytes, canvas ${result.pixelHash} ${JSON.stringify(result.pixelSamples)}`,
  )
} finally {
  await browser?.close()
  await new Promise((resolvePromise) => server.close(resolvePromise))
}
