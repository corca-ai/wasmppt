import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import { createNodeEngine } from '../dist/node.js'
import { DeckSourceText } from '../dist/deck.js'

const templateBytes = await readFile(new URL('../../../fixtures/deck-gates/starter.potx', import.meta.url))
const spec = (text = 'First revision', start = 0) => ({
  key: 'document', slides: [{ key: 'cover', kind: 'title',
    source: { source: 'slides.md', start, end: start + 100 },
    nodes: [{ key: 'heading', role: 'title', source: { source: 'slides.md', start, end: start + 20 },
      content: { kind: 'text', text: { runs: [{ text }] } } }],
  }],
})

async function fixture(t, options) {
  const engine = await createNodeEngine(options)
  t.after(() => engine.dispose())
  const prepared = await engine.prepareDeckTemplate(templateBytes)
  assert.equal(prepared.ok, true, JSON.stringify(prepared.diagnostics))
  const template = prepared.value
  const deck = engine.createDeck()
  t.after(async () => { await deck.dispose(); await template.dispose() })
  return { engine, deck, template }
}
async function replace(deck, template, input = spec(), fonts) {
  const result = await deck.replace({ template, spec: input, fonts })
  assert.equal(result.ok, true, JSON.stringify(result.diagnostics))
  return result.value
}
async function pptx(snapshot) { return new Uint8Array(await new Response(snapshot.exportPptx()).arrayBuffer()) }

test('Node consumers inspect structured templates without internal package paths', async t => {
  const { template } = await fixture(t)
  const description = template.describe()
  assert.deepEqual(description.layouts.map(layout => layout.capability).sort(), ['content-envelope', 'statement', 'title'])
  assert.ok(description.pageSize.width > 0)
  assert.ok(description.layouts.every(layout => layout.regions.length > 0))
  assert.throws(() => { description.pageSize.width = 0 }, TypeError)
  assert.equal(templateBytes.byteLength > 0, true, 'preparation did not detach caller bytes')
})

test('an older snapshot and its output survive replacement and independent owner disposal', async t => {
  const { deck, template } = await fixture(t)
  const first = await replace(deck, template)
  const expected = await pptx(first)
  const output = first.exportPptx()
  const second = await replace(deck, template, spec('Second revision'))
  assert.equal(second.revision, 1)
  assert.deepEqual(first.pages.map(page => page.id), second.pages.map(page => page.id))
  assert.notDeepEqual(await pptx(second), expected)
  await first.dispose()
  assert.deepEqual(new Uint8Array(await new Response(output).arrayBuffer()), expected)
  assert.throws(() => first.retain(), /disposed/)
  assert.equal((await second.resolvePage(second.pages[0].id)).commands.some(command => command.kind === 'draw-rich-text' && command.frame.paragraphs.some(paragraph => paragraph.runs.some(run => run.text === 'Second revision'))), true)
  await second.dispose()
})

test('rejected and cancelled edits leave the prior accepted revision intact', async t => {
  const { deck, template } = await fixture(t)
  const first = await replace(deck, template)
  const bad = spec()
  bad.slides[0].nodes.push(bad.slides[0].nodes[0])
  const failure = await deck.replace({ template, spec: bad })
  assert.equal(failure.ok, false)
  assert.equal(failure.diagnostics[0].category, 'content')
  const cancellation = new AbortController()
  const pending = deck.replace({ template, spec: spec('Discarded') }, { signal: cancellation.signal })
  cancellation.abort()
  await assert.rejects(pending, { name: 'AbortError' })
  const second = await replace(deck, template, spec('Accepted'))
  assert.equal(second.revision, 1)
  assert.equal((await first.resolvePage(first.pages[0].id)).commands.some(command => command.kind === 'draw-rich-text' && command.frame.paragraphs.some(paragraph => paragraph.runs.some(run => run.text === 'First revision'))), true)
  await first.dispose(); await second.dispose()
})

test('source relocation retains semantic/page keys and refreshes cached hit-test spans', async t => {
  const { deck, template } = await fixture(t)
  const first = await replace(deck, template)
  const oldScene = await first.resolvePage(first.pages[0].id)
  const next = await replace(deck, template, spec('First revision', 17))
  const scene = await next.resolvePage(next.pages[0].id)
  const before = oldScene.semantics.find(item => item.source)?.source
  const after = scene.semantics.find(item => item.source)?.source
  assert.equal(before.semanticId, after.semanticId)
  assert.equal(next.sourceIdentity(after.semanticId).key, 'heading')
  assert.equal(before.start, 0)
  assert.equal(after.start, 17)
  assert.equal(first.pages[0].id, next.pages[0].id)
  await first.dispose(); await next.dispose()
})

test('asset registrations deduplicate bytes, have independent leases, and snapshots retain them', async t => {
  const { engine, deck, template } = await fixture(t)
  const bytes = Uint8Array.from(Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a/e0AAAAASUVORK5CYII=', 'base64'))
  const first = await engine.registerAsset({ bytes, mediaType: 'image/png' })
  const weight = await engine.accountedBytes()
  const second = await engine.registerAsset({ bytes, mediaType: 'image/png' })
  assert.equal(first.id, second.id)
  assert.equal(await engine.accountedBytes(), weight)
  await first.dispose()
  const input = { key: 'images', slides: [{ key: 'figure', kind: 'content', nodes: [
    { key: 'image', role: 'figure', content: { kind: 'image', assetId: second.id, altText: 'A pixel' } },
  ] }] }
  const queued = deck.replace({ template, spec: input })
  await second.dispose()
  const result = await queued
  assert.equal(result.ok, true, JSON.stringify(result.diagnostics))
  assert.equal((await result.value.resolvePage(result.value.pages[0].id)).images.length, 1)
  assert.ok((await pptx(result.value)).length > 0)
  await result.value.dispose()
})

test('registered exact font bytes reach semantic planning and remain valid after owner disposal', async t => {
  const { engine, deck, template } = await fixture(t)
  const bytes = await readFile(new URL('../../../node_modules/katex/dist/fonts/KaTeX_Main-Regular.ttf', import.meta.url))
  const fonts = await engine.registerFonts(template.describe().requiredFonts.map(family => ({ family, bytes })))
  const snapshot = await replace(deck, template, spec(), fonts)
  assert.equal(snapshot.diagnostics.some(item => item.category === 'font'), false)
  await fonts.dispose()
  assert.ok((await pptx(snapshot)).length > 0)
  await snapshot.dispose()
})

test('retained snapshots enforce a byte budget and repeated replacement releases old state', async t => {
  const { engine, deck, template } = await fixture(t, { maxRetainedBytes: 40 * 1024 * 1024 })
  const first = await replace(deck, template)
  const second = await replace(deck, template, spec('Second'))
  await assert.rejects(deck.replace({ template, spec: spec('Third') }), error => error.code === 'limit-exceeded')
  await first.dispose(); await second.dispose()
  const baseline = await engine.accountedBytes()
  for (let index = 0; index < 12; index += 1) {
    const snapshot = await replace(deck, template, spec(`Revision ${index}`))
    await snapshot.dispose()
    assert.ok(await engine.accountedBytes() < baseline + 4096)
  }
  await deck.dispose(); await template.dispose()
  assert.equal(await engine.accountedBytes(), 0)
})

test('source offsets round-trip Unicode scalar boundaries and reject split surrogate/UTF-8 offsets', () => {
  const source = new DeckSourceText('slides.md', 'A한🙂Z', 'edit:3')
  assert.deepEqual(source.range(1, 4), { source: 'slides.md', start: 1, end: 8, version: 'edit:3' })
  assert.equal(source.toUtf16(8), 4)
  assert.throws(() => source.toUtf8(3), /scalar/)
  assert.throws(() => source.toUtf16(3), /scalar/)
})

test('stream cancellation and shutdown release pinned output without blocking later edits', async t => {
  const { engine, deck, template } = await fixture(t)
  const first = await replace(deck, template)
  const cancelled = first.exportPptx()
  await cancelled.cancel()
  const controller = new AbortController()
  const aborted = first.exportPptx({ signal: controller.signal })
  controller.abort()
  await assert.rejects(new Response(aborted).arrayBuffer(), { name: 'AbortError' })
  const second = await replace(deck, template, spec('Still editable'))
  await first.dispose()
  const output = second.exportPptx()
  engine.dispose()
  await assert.rejects(new Response(output).arrayBuffer())
  await second.dispose()
})

test('template registration respects the retained budget and rejects invalid numeric options', async t => {
  for (const maxRetainedBytes of [0, -1, 1.5, NaN, 2 ** 32 + 1]) {
    await assert.rejects(createNodeEngine({ maxRetainedBytes }), RangeError)
  }
  const engine = await createNodeEngine({ maxRetainedBytes: templateBytes.byteLength - 1 })
  t.after(() => engine.dispose())
  await assert.rejects(engine.prepareDeckTemplate(templateBytes), error => error.code === 'limit-exceeded')
  assert.equal(await engine.accountedBytes(), 0)
})
