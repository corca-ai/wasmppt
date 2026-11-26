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
function paintedRun(scene, text) {
  return scene.commands.filter(command => command.kind === 'draw-rich-text')
    .flatMap(command => command.frame.paragraphs.flatMap(paragraph => paragraph.runs)).find(run => run.text === text)
}

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

test('document layout is captured before queuing and invalid layout preserves accepted snapshots', async t => {
  const { deck, template } = await fixture(t)
  const input = { ...spec(), layout: { presentation: 'conference', imageFit: 'contain' } }
  const pending = deck.replace({ template, spec: input })
  input.layout.presentation = 'invalid-after-capture'
  const result = await pending
  assert.equal(result.ok, true, JSON.stringify(result.diagnostics))
  const first = result.value
  const original = await pptx(first)
  assert.equal(paintedRun(await first.resolvePage(first.pages[0].id), 'First revision').style.fontSize, 4000)
  for (const layout of [{ presentation: 'poster' }, { imageFit: 'cover' }]) {
    const rejected = await deck.replace({ template, spec: { ...spec(), layout } })
    assert.equal(rejected.ok, false)
    assert.equal(rejected.diagnostics[0].category, 'content')
  }
  const next = await replace(deck, template, { ...spec(), layout: { presentation: 'handout' } })
  assert.equal(next.revision, 1, 'rejected purposes never publish revisions')
  assert.equal(paintedRun(await next.resolvePage(next.pages[0].id), 'First revision').style.fontSize, 3200)
  assert.equal(paintedRun(await first.resolvePage(first.pages[0].id), 'First revision').style.fontSize, 4000)
  assert.deepEqual(await pptx(first), original, 'retained export keeps its accepted document layout')
  const restored = await replace(deck, template, { ...spec(), layout: { presentation: 'conference' } })
  assert.deepEqual(await pptx(restored), original, 'A-B-A purpose changes are history independent')
  await first.dispose(); await next.dispose(); await restored.dispose()
})

test('purpose controls heading, body and explicit caption sizes through immutable SDK snapshots', async t => {
  const { deck, template } = await fixture(t)
  const content = {
    key: 'academic', slides: [{ key: 'question', kind: 'content', nodes: [
      { key: 'heading', role: 'title', content: { kind: 'text', text: { runs: [{ text: '연구 질문 / Research question' }] } } },
      { key: 'body', role: 'prose', split: 'text', content: { kind: 'text', text: { runs: [{ text: 'Ordinary emphasized prose', marks: { italic: true } }] } } },
      { key: 'caption', role: 'caption', content: { kind: 'text', text: { runs: [{ text: 'Figure 1. Explicit caption' }] } } },
    ] }],
  }
  for (const [presentation, sizes] of [
    ['conference', [3000, 2400, 1800]], ['seminar', [2800, 2000, 1600]], ['handout', [2600, 1800, 1400]],
  ]) {
    const snapshot = await replace(deck, template, { ...content, layout: { presentation, imageFit: 'contain' } })
    assert.deepEqual(snapshot.failedSlides, [])
    assert.equal(snapshot.pages.length, 1)
    const scene = await snapshot.resolvePage(snapshot.pages[0].id)
    const runs = ['연구 질문 / Research question', 'Ordinary emphasized prose', 'Figure 1. Explicit caption'].map(text => paintedRun(scene, text))
    assert.deepEqual(runs.map(run => run.style.fontSize), sizes)
    assert.equal(runs[1].style.italic, true, 'emphasis does not turn body text into a caption')
    assert.equal(snapshot.diagnostics.some(item => item.category === 'font'), true, 'fallback font diagnostics remain observable')
    assert.ok((await pptx(snapshot)).length > 0)
    await snapshot.dispose()
  }
})

test('custom POTX without generation metadata keeps authored typography across purpose changes', async t => {
  const { engine, deck } = await fixture(t)
  const prepared = await engine.prepareDeckTemplate(await readFile(new URL('../../../fixtures/deck-gates/custom.potx', import.meta.url)))
  assert.equal(prepared.ok, true, JSON.stringify(prepared.diagnostics))
  const custom = prepared.value
  t.after(() => custom.dispose())
  let expected
  for (const presentation of ['seminar', 'conference', 'handout']) {
    const snapshot = await replace(deck, custom, { ...spec(), layout: { presentation } })
    const scene = await snapshot.resolvePage(snapshot.pages[0].id)
    assert.equal(paintedRun(scene, 'First revision').style.fontSize, 3200)
    const bytes = await pptx(snapshot)
    expected ??= bytes
    assert.deepEqual(bytes, expected, 'purpose leaves unopted authored styles and geometry unchanged')
    await snapshot.dispose()
  }
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


test('recoverable layout failures keep neighboring slides, source diagnostics and immutable recovery', async t => {
  const { deck, template } = await fixture(t)
  const input = spec()
  const broken = structuredClone(input.slides[0])
  broken.key = 'broken'
  broken.nodes[0].key = 'broken-heading'
  broken.nodes[0].content.text.runs[0].text = 'Cannot fit\n'.repeat(200)
  const after = structuredClone(input.slides[0])
  after.key = 'after'
  after.nodes[0].key = 'after-heading'
  after.nodes[0].content.text.runs[0].text = 'After failure'
  input.slides.push(broken, after)
  const partial = await replace(deck, template, input)
  assert.deepEqual(partial.failedSlides, [{ logicalSlideKey: 'broken', hidden: false }])
  assert.deepEqual(partial.pages.map(page => page.logicalSlideKey), ['cover', 'after'])
  assert.ok(partial.diagnostics.some(item => item.code === 'plan-atomic-overflow' && item.nodeKey === 'broken-heading' && item.source?.source === 'slides.md'))
  assert.throws(() => partial.exportPptx(), /failed logical slides/)
  const { exportHtml } = await import('../dist/browser.js')
  await assert.rejects(exportHtml(partial), /failed logical slides/)
  assert.ok((await partial.resolvePage(partial.pages[1].id)).commands.some(command => command.kind === 'draw-rich-text' && command.frame.paragraphs.some(paragraph => paragraph.runs.some(run => run.text === 'After failure'))))
  input.slides[1].nodes[0].content.text.runs[0].text = 'Repaired'
  const recovered = await replace(deck, template, input)
  assert.deepEqual(recovered.failedSlides, [])
  assert.equal(recovered.pages.length, 3)
  assert.ok((await pptx(recovered)).length > 0)
  assert.equal(partial.failedSlides.length, 1, 'repair does not mutate a retained revision')
  await partial.dispose(); await recovered.dispose()
})

test('an entirely unplaceable deck still has a source-owned failed inventory', async t => {
  const { deck, template } = await fixture(t)
  const failed = await replace(deck, template, spec('Impossible\n'.repeat(200)))
  assert.equal(failed.pages.length, 0)
  assert.deepEqual(failed.failedSlides, [{ logicalSlideKey: 'cover', hidden: false }])
  assert.throws(() => failed.exportPptx(), /failed logical slides/)
  await failed.dispose()
})


test('source offsets match UTF-8 encoding across scalar widths and lone surrogates', () => {
  const text = '\0\u007f\u0080\u07ff\u0800\uffff😀\ud800X\udfff'
  const source = new DeckSourceText('doc', text)
  const encoder = new TextEncoder()
  let utf16 = 0
  let utf8 = 0
  for (const scalar of text) {
    utf16 += scalar.length
    utf8 += encoder.encode(scalar).length
    assert.equal(source.toUtf8(utf16), utf8)
    assert.equal(source.toUtf16(utf8), utf16)
  }
})
