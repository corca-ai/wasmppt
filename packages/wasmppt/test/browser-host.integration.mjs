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
import init, { WasmpptEngine, display_list_signature } from '/wasm/wasmppt_wasm.js';
import { installWorkerRuntime } from '/dist/worker-runtime.js';
try {
  await init({ module_or_path: new URL('/wasm/wasmppt_wasm_bg.wasm', self.location.href) });
  installWorkerRuntime(self, new WasmpptEngine());
  self.addEventListener('message', (event) => {
    if (event.data?.type === 'host-display-signature') {
      self.postMessage({
        type: 'host-display-signature-result',
        signature: display_list_signature(new Uint8Array(event.data.presentation), event.data.slideIndex),
      });
    }
  });
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
    const displaySignature = await new Promise((resolvePromise, reject) => {
      const timer = setTimeout(() => reject(new Error('display signature timed out')), 10_000)
      worker.addEventListener('message', (event) => {
        if (event.data?.type === 'host-display-signature-result') {
          clearTimeout(timer)
          resolvePromise(event.data.signature)
        }
      })
      worker.postMessage(
        { type: 'host-display-signature', presentation: renderFixture, slideIndex: 0 },
        [renderFixture],
      )
    })
    client.terminate()
    return {
      transferredByteLength,
      residentBytes: prepared.residentBytes,
      zipSignature: [...output.subarray(0, 2)],
      outputBytes: output.byteLength,
      displaySignature,
    }
  })
  assert.equal(result.transferredByteLength, 0, 'template ArrayBuffer was cloned, not transferred')
  assert(result.residentBytes > 0)
  assert.deepEqual(result.zipSignature, [0x50, 0x4b])
  assert(result.outputBytes > 0)
  assert.equal(result.displaySignature, 'a53592cdd09d0945')
  assert.deepEqual(errors, [])
  console.log(`browser host fixture ok: ${result.outputBytes} output bytes`)
} finally {
  await browser?.close()
  await new Promise((resolvePromise) => server.close(resolvePromise))
}
