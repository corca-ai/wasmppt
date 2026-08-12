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

  assert.equal(await page.getByRole('button', { name: /compile/i }).count(), 0)
  assert.equal(await page.getByRole('button', { name: /generate/i }).count(), 0)
  await page.getByText(/^PPTX ready · 2 slides$/).waitFor()
  await page.locator('#preview figure').first().scrollIntoViewIfNeeded()
  await page.locator('#preview canvas').first().waitFor()
  assert((await page.locator('#preview canvas').count()) <= 2)
  assert.equal(await page.locator('#download').getAttribute('aria-disabled'), 'false')
  assert.equal(await page.locator('[data-binding="title"]').inputValue(), 'wasmppt quarterly report')

  const firstDownloadUrl = await page.locator('#download').getAttribute('href')
  await page.locator('[data-binding="title"]').fill('Automatically refreshed title')
  await page.waitForFunction((previous) => {
    const link = document.querySelector('#download')
    return link?.getAttribute('aria-disabled') === 'false' && link.getAttribute('href') !== previous
  }, firstDownloadUrl)
  await page.getByText(/^PPTX ready · 2 slides$/).waitFor()
  await page.locator('#preview figure').first().scrollIntoViewIfNeeded()
  await page.locator('#preview canvas').first().waitFor()
  await assertDownload(page, 2_000)

  const settledRevision = Number(await page.locator('#download').getAttribute('data-revision'))
  await page.locator('[data-binding="title"]').fill('Download waits for this pending edit')
  await assertDownload(page, 2_000)
  assert(Number(await page.locator('#download').getAttribute('data-revision')) > settledRevision)

  const beforeBurst = Number(await page.locator('#download').getAttribute('data-revision'))
  await page.locator('[data-binding="title"]').evaluate((input) => {
    for (let index = 0; index < 20; index += 1) {
      input.value = `coalesced burst ${index}`
      input.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' }))
    }
  })
  await assertDownload(page, 2_000)
  assert.equal(
    Number(await page.locator('#download').getAttribute('data-revision')),
    beforeBurst + 1,
  )

  await page.evaluate(async () => {
    const bytes = await fetch('./fixtures/minimal.potx').then((response) => response.arrayBuffer())
    const transfer = new DataTransfer()
    transfer.items.add(new File([bytes], 'dropped-minimal.potx', { type: 'application/octet-stream' }))
    document.querySelector('#drop-zone').dispatchEvent(new DragEvent('drop', {
      bubbles: true,
      cancelable: true,
      dataTransfer: transfer,
    }))
  })
  await page.getByText(/dropped-minimal\.potx/).waitFor()
  await page.getByText(/^PPTX ready · 1 slide$/).waitFor()
  await page.locator('#preview figure').scrollIntoViewIfNeeded()
  await page.locator('#preview canvas').waitFor()
  assert.equal(await page.locator('#preview canvas').count(), 1)
  assert.equal(await page.locator('#download').getAttribute('aria-disabled'), 'false')
  assert.equal((await page.locator('#diagnostics').textContent()).includes('no repeated table row'), false)
  await assertDownload(page, 1_000)

  assert.deepEqual(errors, [])
  console.log('Pages dogfood auto-generated, rendered, and downloaded bundled and dropped templates')
} finally {
  await browser?.close()
  await new Promise((resolvePromise) => server.close(resolvePromise))
}

async function assertDownload(page, minimumBytes) {
  const downloadPromise = page.waitForEvent('download')
  await page.getByRole('link', { name: 'Download PPTX' }).click()
  const download = await downloadPromise
  const path = await download.path()
  assert(path !== null)
  const bytes = await readFile(path)
  assert.deepEqual([...bytes.subarray(0, 2)], [0x50, 0x4b])
  assert((await stat(path)).size > minimumBytes)
}
