import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { readFile, stat } from 'node:fs/promises'
import { extname, resolve, sep } from 'node:path'

import { chromium } from 'playwright'

const root = resolve(import.meta.dirname, '../../..')
const pages = resolve(root, 'target/pages')
const types = new Map([
  ['.html', 'text/html; charset=utf-8'],
  ['.css', 'text/css; charset=utf-8'],
  ['.js', 'text/javascript; charset=utf-8'],
  ['.wasm', 'application/wasm'],
  ['.potx', 'application/octet-stream'],
])
const server = createServer(async (request, response) => {
  try {
    const pathname = new URL(request.url ?? '/', 'http://localhost').pathname
    const relative = pathname === '/' ? 'index.html' : pathname.slice(1)
    const file = resolve(pages, relative)
    if (!file.startsWith(`${pages}${sep}`)) throw new Error('unsafe path')
    const bytes = await readFile(file)
    response.writeHead(200, { 'content-type': types.get(extname(file)) ?? 'application/octet-stream' })
    response.end(bytes)
  } catch {
    response.writeHead(404)
    response.end('not found')
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
  browser = await chromium.launch(process.env.CI ? { headless: true } : { channel: 'chrome', headless: true })
  const page = await browser.newPage({ acceptDownloads: true })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => console.log(`browser console: ${message.text()}`))
  await page.goto(`http://127.0.0.1:${address.port}/`)
  await page.getByText('WebAssembly ready', { exact: false }).waitFor()
  await page.getByRole('button', { name: 'Compile template' }).click()
  await page.getByText('Template compiled', { exact: false }).waitFor()
  assert.equal(await page.locator('[data-binding="title"]').inputValue(), 'wasmppt quarterly report')
  const downloadPromise = page.waitForEvent('download')
  await page.getByRole('button', { name: 'Generate PPTX' }).click()
  const download = await downloadPromise
  const path = await download.path()
  assert(path !== null)
  const bytes = await readFile(path)
  assert.deepEqual([...bytes.subarray(0, 2)], [0x50, 0x4b])
  assert((await stat(path)).size > 1000)
  await page.getByText('PPTX generated locally', { exact: false }).waitFor()
  assert.deepEqual(errors, [])
  console.log(`Pages dogfood generated ${bytes.byteLength} bytes`)
} finally {
  await browser?.close()
  await new Promise((resolvePromise) => server.close(resolvePromise))
}
