import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises'
import { extname, resolve, sep } from 'node:path'

import { chromium } from 'playwright'

const root = resolve(import.meta.dirname, '../../..')
const pages = resolve(root, 'target/pages')
const downloads = resolve(root, 'target/pages-downloads')
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
await mkdir(downloads, { recursive: true })
await new Promise((resolvePromise, reject) => {
  server.once('error', reject)
  server.listen(0, '127.0.0.1', resolvePromise)
})
let browser
try {
  const address = server.address()
  assert(address !== null && typeof address === 'object')
  browser = await chromium.launch(process.env.CI ? { headless: true } : { channel: 'chrome', headless: true })
  const page = await browser.newPage({ acceptDownloads: true, viewport: { width: 1440, height: 1000 } })
  const errors = []
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('console', (message) => console.log(`browser console: ${message.text()}`))
  await page.goto(`http://127.0.0.1:${address.port}/`)

  assert.equal(await page.locator('input[type="file"]').count(), 0)
  assert.equal(await page.getByRole('button', { name: /compile|generate/i }).count(), 0)
  assert.equal(await page.locator('[data-deck]').count(), 2)
  await page.getByText(/^Both decks are live/).waitFor()
  assert.equal(await page.locator('[data-binding]').count(), 4)

  const deckCards = page.locator('[data-deck]')
  for (let index = 0; index < 2; index += 1) {
    const deck = deckCards.nth(index)
    await deck.scrollIntoViewIfNeeded()
    await deck.locator('canvas').first().waitFor()
    assert.equal(await deck.locator('[data-download]').getAttribute('aria-disabled'), 'false')
    assert.equal(Number(await deck.getAttribute('data-render-revision')), 0)
  }
  const initialPixels = await canvasSignatures(page, 0)
  assert.notEqual(initialPixels[0], initialPixels[1])

  const title = page.locator('[data-binding="title"]')
  assert.equal(await title.inputValue(), 'One story, two visual worlds')
  const priorRevisions = await downloadRevisions(page)
  await title.fill('A single edit blooms twice')
  await page.waitForFunction((previous) => {
    const decks = [...document.querySelectorAll('[data-deck]')]
    return decks.every((deck, index) =>
      Number(deck.dataset.renderRevision) > previous[index] &&
      Number(deck.querySelector('[data-download]')?.dataset.revision) > previous[index],
    )
  }, priorRevisions)
  const editedPixels = await canvasSignatures(page, 0)
  assert.notEqual(editedPixels[0], initialPixels[0])
  assert.notEqual(editedPixels[1], initialPixels[1])

  const metricPixels = await canvasSignatures(page, 1)
  const metricRevisions = await downloadRevisions(page)
  await page.locator('[data-binding="metrics.value"]').fill('42× faster')
  await page.waitForFunction((previous) => [...document.querySelectorAll('[data-download]')]
    .every((link, index) => Number(link.dataset.revision) > previous[index]), metricRevisions)
  const editedMetricPixels = await canvasSignatures(page, 1)
  assert.notEqual(editedMetricPixels[0], metricPixels[0])
  assert.notEqual(editedMetricPixels[1], metricPixels[1])

  const beforeBurst = await downloadRevisions(page)
  await title.evaluate((input) => {
    for (let index = 0; index < 20; index += 1) {
      input.value = `parallel burst ${index}`
      input.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' }))
    }
  })
  await page.waitForFunction((previous) => [...document.querySelectorAll('[data-download]')]
    .every((link, index) => Number(link.dataset.revision) > previous[index]), beforeBurst)
  assert.deepEqual(await downloadRevisions(page), beforeBurst.map((revision) => revision + 1))

  await assertDownload(
    page,
    '[data-deck="atlas"] [data-download]',
    2_000,
    'wasmppt-atlas-report.pptx',
  )
  await assertDownload(
    page,
    '[data-deck="garden"] [data-download]',
    2_000,
    'wasmppt-signal-garden.pptx',
  )
  assert.equal((await page.locator('#diagnostics').textContent()).includes('no repeated table row'), false)
  assert.deepEqual(errors, [])
  console.log('Pages garden rendered two templates from one coalesced editor and downloaded both PPTX files')
} finally {
  await browser?.close()
  await new Promise((resolvePromise) => server.close(resolvePromise))
}

async function canvasSignatures(page, slideIndex) {
  return page.locator('[data-deck]').evaluateAll((decks, index) => decks.map((deck) => {
    const canvas = deck.querySelectorAll('canvas')[index]
    return canvas?.toDataURL() ?? ''
  }), slideIndex)
}

async function downloadRevisions(page) {
  return page.locator('[data-download]').evaluateAll((links) =>
    links.map((link) => Number(link.dataset.revision)),
  )
}

async function assertDownload(page, selector, minimumBytes, outputName) {
  const downloadPromise = page.waitForEvent('download')
  await page.locator(selector).click()
  const download = await downloadPromise
  const path = await download.path()
  assert(path !== null)
  const bytes = await readFile(path)
  assert.deepEqual([...bytes.subarray(0, 2)], [0x50, 0x4b])
  assert((await stat(path)).size > minimumBytes)
  await writeFile(resolve(downloads, outputName), bytes)
}
