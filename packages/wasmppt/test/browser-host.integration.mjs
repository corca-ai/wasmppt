import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
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
const featureRegions = Object.fromEntries(
  renderCorpus.presentations[0].features.map((feature) => [feature.id, feature.region]),
)

const routes = new Map([
  ['/dist/worker-client.js', [join(packageDirectory, 'dist/worker-client.js'), 'text/javascript']],
  ['/dist/injection.js', [join(packageDirectory, 'dist/injection.js'), 'text/javascript']],
  ['/dist/protocol.js', [join(packageDirectory, 'dist/protocol.js'), 'text/javascript']],
  ['/dist/canvas.js', [join(packageDirectory, 'dist/canvas.js'), 'text/javascript']],
  ['/dist/dom-svg.js', [join(packageDirectory, 'dist/dom-svg.js'), 'text/javascript']],
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
    '/wasm/metafile/wasmppt_metafile_wasm.js',
    [join(generatedDirectory, 'metafile/wasmppt_metafile_wasm.js'), 'text/javascript'],
  ],
  [
    '/wasm/metafile/wasmppt_metafile_wasm_bg.wasm',
    [join(generatedDirectory, 'metafile/wasmppt_metafile_wasm_bg.wasm'), 'application/wasm'],
  ],
  [
    '/fixture.potx',
    [join(workspaceDirectory, 'fixtures/host-adapters/minimal.potx'), 'application/octet-stream'],
  ],
  [
    '/render-fixture.pptx',
    [join(workspaceDirectory, 'fixtures/render/basic.pptx'), 'application/octet-stream'],
  ],
  [
    '/competitor-fixture.pptx',
    [join(workspaceDirectory, 'target/benchmarks/pptxgenjs-text-10.pptx'), 'application/octet-stream'],
  ],
])

const workerSource = `
import init, { WasmpptEngine } from '/wasm/wasmppt_wasm.js';
import { installWorkerRuntime } from '/dist/worker-runtime.js';
let metafileModule;
async function metafileToSvg(input) {
  metafileModule ??= import('/wasm/metafile/wasmppt_metafile_wasm.js').then(async (module) => {
    await module.default({ module_or_path: new URL('/wasm/metafile/wasmppt_metafile_wasm_bg.wasm', self.location.href) });
    return module;
  });
  return (await metafileModule).convert_metafile_to_svg(input);
}
try {
  await init({ module_or_path: new URL('/wasm/wasmppt_wasm_bg.wasm', self.location.href) });
  installWorkerRuntime(self, new WasmpptEngine(), { metafileToSvg });
  self.postMessage({ type: 'host-ready' });
} catch (error) {
  self.postMessage({ type: 'host-init-error', message: error instanceof Error ? error.stack : String(error) });
  throw error;
}
`

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
  if (request.url === '/worker.js') {
    response.writeHead(200, { 'content-type': 'text/javascript' })
    response.end(workerSource)
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
  const result = await page.evaluate(async () => {
    const { WasmpptWorkerClient } = await import('/dist/worker-client.js')
    const {
      CanvasDisplayListRenderer,
      FontResolver,
      VirtualizedCanvasViewer,
      decodeSvgImage,
      decodeDisplayList,
      wrapText,
    } = await import('/dist/canvas.js')
    const { DomSvgRenderer, VirtualizedDomViewer } = await import('/dist/dom-svg.js')
    const worker = new Worker('/worker.js', { type: 'module' })
    await new Promise((resolvePromise, reject) => {
      const timer = setTimeout(() => reject(new Error('browser Worker initialization timed out')), 10_000)
      worker.addEventListener('error', (event) => {
        clearTimeout(timer)
        reject(new Error(event.message))
      }, { once: true })
      worker.addEventListener('message', (event) => {
        if (event.data?.type === 'host-init-error') {
          clearTimeout(timer)
          reject(new Error(event.data.message))
        } else if (event.data?.type === 'host-ready') {
          clearTimeout(timer)
          resolvePromise()
        }
      })
    })
    const client = new WasmpptWorkerClient(worker)
    const template = await fetch('/fixture.potx').then((response) => response.arrayBuffer())
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
    await client.release(prepared.handle)
    const renderFixture = await fetch('/render-fixture.pptx').then((response) => response.arrayBuffer())
    const opened = await client.openPresentation(renderFixture)
    const displayBytes = await client.resolveSlide(opened.handle, 0)
    const scene = decodeDisplayList(displayBytes)
    const advancedDisplayBytes = await client.resolveSlide(opened.handle, 1)
    const advancedScene = decodeDisplayList(advancedDisplayBytes)
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
        const bytes = await loadMetafileSvg(image, signal)
        return decodeSvgImage(bytes, signal)
      },
    })
    const advancedPixels = advancedContext.getImageData(
      0,
      0,
      advancedCanvas.width,
      advancedCanvas.height,
    ).data
    let advancedColoredPixels = 0
    let advancedPixelHash = 0x811c9dc5
    for (let offset = 0; offset < advancedPixels.length; offset += 4) {
      advancedPixelHash = Math.imul(advancedPixelHash ^ advancedPixels[offset], 0x01000193) >>> 0
      advancedPixelHash = Math.imul(advancedPixelHash ^ advancedPixels[offset + 1], 0x01000193) >>> 0
      advancedPixelHash = Math.imul(advancedPixelHash ^ advancedPixels[offset + 2], 0x01000193) >>> 0
      advancedPixelHash = Math.imul(advancedPixelHash ^ advancedPixels[offset + 3], 0x01000193) >>> 0
      if (
        advancedPixels[offset] !== 255 ||
        advancedPixels[offset + 1] !== 255 ||
        advancedPixels[offset + 2] !== 255
      ) advancedColoredPixels += 1
    }
    const advancedDomHost = document.createElement('div')
    document.body.append(advancedDomHost)
    const advancedDom = await new DomSvgRenderer().render(advancedScene, advancedDomHost, {
      revision: 1,
      slideIndex: 1,
      imageResolver: async (image, signal) => {
        const bytes = new Uint8Array(await loadMetafileSvg(image, signal))
        let binary = ''
        for (const byte of bytes) binary += String.fromCharCode(byte)
        return `data:image/svg+xml;base64,${btoa(binary)}`
      },
    })
    const advancedFacts = {
      semanticKinds: advancedScene.semantics.map((semantic) => semantic.kind),
      strings: advancedScene.strings,
      diagnosticCodes: advancedScene.diagnostics.map((diagnostic) => diagnostic.code),
      coloredPixels: advancedColoredPixels,
      pixelHash: advancedPixelHash.toString(16).padStart(8, '0'),
      commandCount: advancedTelemetry.commandCount,
      svgPathCount: advancedDomHost.querySelectorAll('path').length,
      inlineImages: advancedDomHost.querySelectorAll('image').length,
      metafileSvgBytes,
      domDiagnosticCodes: advancedDom.diagnostics.map((diagnostic) => diagnostic.code),
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
    const pixelAt = (x, y) => [...pixels.slice((y * canvas.width + x) * 4, (y * canvas.width + x) * 4 + 4)]
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
    client.terminate()
    return {
      transferredByteLength,
      coldPrepareMs,
      warmInjectionSamplesMs,
      residentBytes: prepared.residentBytes,
      zipSignature: [...output.subarray(0, 2)],
      outputBytes: output.byteLength,
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
      firstVisibleSlideSamplesMs,
      displayByteLength: displayBytes.byteLength,
      staleAbortCount,
      staleResult,
      staleMountedSlides,
      decodedImageBytesAfterClear: renderer.decodedImageBytes,
      koreanLines,
      telemetry,
      advancedFacts,
      pptxBrowserComparison: { samplesMs: competitorSamplesMs, correctness: competitorFacts },
      domFacts,
      domMountedAtPeak,
      domMountedAfterScroll,
      domMountedAfterDispose,
    }
  })
  assert.equal(result.transferredByteLength, 0, 'template ArrayBuffer was cloned, not transferred')
  const sortedWarmSamples = [...result.warmInjectionSamplesMs].sort((left, right) => left - right)
  const warmP50Ms = sortedWarmSamples[Math.ceil(sortedWarmSamples.length * 0.5) - 1]
  const warmP95Ms = sortedWarmSamples[Math.ceil(sortedWarmSamples.length * 0.95) - 1]
  const sortedFirstVisibleSamples = [...result.firstVisibleSlideSamplesMs].sort((left, right) => left - right)
  const firstVisibleP95Ms = sortedFirstVisibleSamples[Math.ceil(sortedFirstVisibleSamples.length * 0.95) - 1]
  assert(result.coldPrepareMs <= performanceBudgets.browserScalarWasm.maximumColdPrepareMs)
  assert(warmP95Ms <= performanceBudgets.browserScalarWasm.maximumWarmInjectionP95Ms)
  assert(firstVisibleP95Ms <= performanceBudgets.browserScalarWasm.maximumFirstVisibleSlideMs)
  assert(result.residentBytes > 0)
  assert.deepEqual(result.zipSignature, [0x50, 0x4b])
  assert(result.outputBytes > 0)
  assert.equal(result.slideCount, 2)
  assert.equal(result.commandCount, 12)
  assert.equal(result.decodedImageBytesAfterClear, 0)
  assert.deepEqual(result.koreanLines, ['가나다', '라마바', '사'])
  assert(result.telemetry.displayExecutionMs >= 0)
  assert(result.telemetry.fontMeasurementMs >= 0)
  assert(result.telemetry.mediaDecodeMs >= 0)
  assert(result.advancedFacts.semanticKinds.includes('table'))
  assert(result.advancedFacts.semanticKinds.includes('chart'))
  assert(result.advancedFacts.semanticKinds.includes('preserved-graphic'))
  assert(result.advancedFacts.strings.includes('Quarter'))
  assert(result.advancedFacts.strings.includes('42'))
  for (const code of [
    'unsupported-smartart',
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
  assert(result.advancedFacts.metafileSvgBytes > 0)
  assert.equal(result.advancedFacts.inlineImages, 1)
  assert(result.advancedFacts.commandCount > 10)
  assert(result.advancedFacts.svgPathCount > 10)
  assert.equal(result.pptxBrowserComparison.correctness.slideCount, 10)
  assert.equal(result.pptxBrowserComparison.correctness.width, 640)
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
  assert.equal(result.domMountedAtPeak, 2)
  assert.equal(result.domMountedAfterScroll, 1)
  assert.equal(result.domMountedAfterDispose, 0)
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
  assert.equal(result.staleAbortCount, 1)
  assert.equal(result.staleResult, 'AbortError')
  assert.deepEqual(result.staleMountedSlides, ['1'])
  assert.deepEqual(errors, [])
  const visualDirectory = join(workspaceDirectory, 'target/visual-report')
  await mkdir(visualDirectory, { recursive: true })
  await page.locator('#wasmppt-visual-slide-1').screenshot({
    path: join(visualDirectory, 'slide-1-actual.png'),
  })
  await page.locator('#wasmppt-visual-slide-2').screenshot({
    path: join(visualDirectory, 'slide-2-actual.png'),
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
    ],
    features: [
      { id: 'text', slideIndex: 0, region: featureRegions.text, metric: 'minimum-dark-pixels', actual: result.darkGroupPixels, tolerance: 100, passed: result.darkGroupPixels > 100 },
      { id: 'shapes', slideIndex: 1, region: featureRegions.shapes, metric: 'minimum-colored-pixels', actual: result.advancedFacts.coloredPixels, tolerance: 10_000, passed: result.advancedFacts.coloredPixels > 10_000 },
      { id: 'raster-images', slideIndex: 0, region: featureRegions['raster-images'], metric: 'sampled-pixel-differences', actual: sampledPixelDifferences, tolerance: 0, passed: sampledPixelDifferences === 0 },
      { id: 'charts', slideIndex: 1, region: featureRegions.charts, metric: 'minimum-svg-paths', actual: result.advancedFacts.svgPathCount, tolerance: 10, passed: result.advancedFacts.svgPathCount > 10 },
      { id: 'metafiles', slideIndex: 1, region: featureRegions.metafiles, metric: 'minimum-converted-bytes', actual: result.advancedFacts.metafileSvgBytes, tolerance: 1, passed: result.advancedFacts.metafileSvgBytes > 0 },
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
      },
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
          p50Ms: [...result.pptxBrowserComparison.samplesMs].sort((a, b) => a - b)[4],
          p95Ms: [...result.pptxBrowserComparison.samplesMs].sort((a, b) => a - b)[9],
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
