import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { createServer } from 'node:http'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { chromium } from 'playwright'

const packageDirectory = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const workspaceDirectory = resolve(packageDirectory, '../..')
const generatedDirectory = join(workspaceDirectory, 'packages/wasmppt-worker/src/generated')

const routes = new Map([
  ['/dist/worker-client.js', [join(packageDirectory, 'dist/worker-client.js'), 'text/javascript']],
  ['/dist/protocol.js', [join(packageDirectory, 'dist/protocol.js'), 'text/javascript']],
  ['/dist/canvas.js', [join(packageDirectory, 'dist/canvas.js'), 'text/javascript']],
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
    '/fixture.potx',
    [join(workspaceDirectory, 'fixtures/host-adapters/minimal.potx'), 'application/octet-stream'],
  ],
  [
    '/render-fixture.pptx',
    [join(workspaceDirectory, 'fixtures/render/basic.pptx'), 'application/octet-stream'],
  ],
])

const workerSource = `
import init, { WasmpptEngine } from '/wasm/wasmppt_wasm.js';
import { installWorkerRuntime } from '/dist/worker-runtime.js';
try {
  await init({ module_or_path: new URL('/wasm/wasmppt_wasm_bg.wasm', self.location.href) });
  installWorkerRuntime(self, new WasmpptEngine());
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
  const route = routes.get(request.url ?? '')
  if (route === undefined) {
    response.writeHead(404)
    response.end('not found')
    return
  }
  try {
    response.writeHead(200, { 'content-type': route[1] })
    response.end(await readFile(route[0]))
  } catch (error) {
    response.writeHead(500)
    response.end(error instanceof Error ? error.message : String(error))
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
  page.on('console', (message) => console.log(`browser console: ${message.text()}`))
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto(`http://127.0.0.1:${address.port}/`)
  const result = await page.evaluate(async () => {
    const { WasmpptWorkerClient } = await import('/dist/worker-client.js')
    const {
      CanvasDisplayListRenderer,
      FontResolver,
      VirtualizedCanvasViewer,
      decodeDisplayList,
      wrapText,
    } = await import('/dist/canvas.js')
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
    const prepare = client.prepare(template)
    const transferredByteLength = template.byteLength
    const prepared = await prepare
    const output = new Uint8Array(await client.generate(prepared.handle))
    await client.release(prepared.handle)
    const renderFixture = await fetch('/render-fixture.pptx').then((response) => response.arrayBuffer())
    const opened = await client.openPresentation(renderFixture)
    const displayBytes = await client.resolveSlide(opened.handle, 0)
    const scene = decodeDisplayList(displayBytes)
    const canvas = document.createElement('canvas')
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
    await viewer.setVisibleSlides([0, 1])
    const mountedAtPeak = viewer.mountedSlideCount
    await viewer.setVisibleSlides([1])
    const mountedAfterScroll = viewer.mountedSlideCount
    const cachedSceneBytes = viewer.cachedSceneBytes
    viewer.dispose()
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
    await client.releasePresentation(opened.handle)
    renderer.clear()
    client.terminate()
    return {
      transferredByteLength,
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
        groupFill: pixelAt(120, 70),
      },
      darkGroupPixels,
      mountedAtPeak,
      mountedAfterScroll,
      mountedAfterDispose,
      cachedSceneBytes,
      displayByteLength: displayBytes.byteLength,
      staleAbortCount,
      staleResult,
      staleMountedSlides,
      decodedImageBytesAfterClear: renderer.decodedImageBytes,
      koreanLines,
      telemetry,
    }
  })
  assert.equal(result.transferredByteLength, 0, 'template ArrayBuffer was cloned, not transferred')
  assert(result.residentBytes > 0)
  assert.deepEqual(result.zipSignature, [0x50, 0x4b])
  assert(result.outputBytes > 0)
  assert.equal(result.slideCount, 2)
  assert.equal(result.commandCount, 9)
  assert.equal(result.decodedImageBytesAfterClear, 0)
  assert.deepEqual(result.koreanLines, ['가나다', '라마바', '사'])
  assert(result.telemetry.displayExecutionMs >= 0)
  assert(result.telemetry.fontMeasurementMs >= 0)
  assert(result.telemetry.mediaDecodeMs >= 0)
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
  assert.equal(result.staleAbortCount, 1)
  assert.equal(result.staleResult, 'AbortError')
  assert.deepEqual(result.staleMountedSlides, ['1'])
  assert.deepEqual(errors, [])
  console.log(
    `browser host fixture ok: ${result.outputBytes} output bytes, canvas ${result.pixelHash} ${JSON.stringify(result.pixelSamples)}`,
  )
} finally {
  await browser?.close()
  await new Promise((resolvePromise) => server.close(resolvePromise))
}
